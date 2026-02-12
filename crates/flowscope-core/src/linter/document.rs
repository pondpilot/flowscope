//! Shared linter document model.
//!
//! A `LintDocument` is constructed once per SQL source and reused across all
//! rule engines. It carries source text, dialect metadata, parsed statements,
//! and tokenizer output with stable spans.

use std::ops::Range;

use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

use crate::analyzer::helpers::line_col_to_offset;
use crate::types::{Dialect, Span};

/// A parsed statement entry within a lint document.
pub struct LintStatement<'a> {
    /// Parsed statement AST.
    pub statement: &'a sqlparser::ast::Statement,
    /// Zero-based statement index in the overall analysis batch.
    pub statement_index: usize,
    /// Byte range of the statement within the source SQL.
    pub statement_range: Range<usize>,
}

/// Token class used by lexical/document lint engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintTokenKind {
    Keyword,
    Identifier,
    Literal,
    Operator,
    Symbol,
    Comment,
    Whitespace,
    Other,
}

/// A token emitted by the SQL tokenizer with stable source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintToken {
    pub kind: LintTokenKind,
    pub span: Span,
    pub text: String,
    pub statement_index: Option<usize>,
}

/// Normalized lint input model for a single SQL source.
pub struct LintDocument<'a> {
    pub sql: &'a str,
    pub dialect: Dialect,
    pub statements: Vec<LintStatement<'a>>,
    pub tokens: Vec<LintToken>,
    pub parser_fallback_used: bool,
    pub tokenizer_fallback_used: bool,
}

impl<'a> LintDocument<'a> {
    /// Build a lint document from source SQL and parsed statements.
    #[must_use]
    pub fn new(sql: &'a str, dialect: Dialect, statements: Vec<LintStatement<'a>>) -> Self {
        let (tokens, tokenizer_fallback_used) = match tokenize_sql(sql, dialect, &statements) {
            Ok(tokens) => (tokens, false),
            Err(_) => (Vec::new(), true),
        };

        Self {
            sql,
            dialect,
            statements,
            tokens,
            parser_fallback_used: false,
            tokenizer_fallback_used,
        }
    }
}

fn tokenize_sql(
    sql: &str,
    dialect: Dialect,
    statements: &[LintStatement<'_>],
) -> Result<Vec<LintToken>, String> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens: Vec<TokenWithSpan> = tokenizer
        .tokenize_with_location()
        .map_err(|error| error.to_string())?;

    let mut out = Vec::with_capacity(tokens.len());

    for token in tokens {
        let Some(span) = token_span_to_offsets(sql, &token.span) else {
            continue;
        };

        let statement_index = statements
            .iter()
            .find(|statement| {
                span.start >= statement.statement_range.start
                    && span.start < statement.statement_range.end
            })
            .map(|statement| statement.statement_index);

        out.push(LintToken {
            kind: classify_token(&token.token),
            span,
            text: token.token.to_string(),
            statement_index,
        });
    }

    Ok(out)
}

fn token_span_to_offsets(sql: &str, span: &sqlparser::tokenizer::Span) -> Option<Span> {
    let start = line_col_to_offset(sql, span.start.line as usize, span.start.column as usize)?;
    let end = line_col_to_offset(sql, span.end.line as usize, span.end.column as usize)?;
    Some(Span::new(start, end))
}

fn classify_token(token: &Token) -> LintTokenKind {
    match token {
        Token::Word(word) if word.keyword != Keyword::NoKeyword => LintTokenKind::Keyword,
        Token::Word(_) => LintTokenKind::Identifier,
        Token::Number(_, _)
        | Token::SingleQuotedString(_)
        | Token::DoubleQuotedString(_)
        | Token::NationalStringLiteral(_)
        | Token::EscapedStringLiteral(_)
        | Token::HexStringLiteral(_) => LintTokenKind::Literal,
        Token::Eq
        | Token::Neq
        | Token::Lt
        | Token::Gt
        | Token::LtEq
        | Token::GtEq
        | Token::Plus
        | Token::Minus
        | Token::Mul
        | Token::Div
        | Token::Mod
        | Token::StringConcat => LintTokenKind::Operator,
        Token::Comma
        | Token::Period
        | Token::LParen
        | Token::RParen
        | Token::SemiColon
        | Token::LBracket
        | Token::RBracket
        | Token::LBrace
        | Token::RBrace
        | Token::Colon
        | Token::DoubleColon
        | Token::Assignment => LintTokenKind::Symbol,
        Token::Whitespace(Whitespace::SingleLineComment { .. })
        | Token::Whitespace(Whitespace::MultiLineComment(_)) => LintTokenKind::Comment,
        Token::Whitespace(_) => LintTokenKind::Whitespace,
        _ => LintTokenKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql_with_dialect;

    #[test]
    fn builds_tokens_with_statement_mapping() {
        let sql = "SELECT 1; SELECT 2";
        let statements = parse_sql_with_dialect(sql, Dialect::Generic).expect("parse");

        let lint_statements = statements
            .iter()
            .enumerate()
            .map(|(index, statement)| LintStatement {
                statement,
                statement_index: index,
                statement_range: if index == 0 { 0..8 } else { 9..17 },
            })
            .collect::<Vec<_>>();

        let document = LintDocument::new(sql, Dialect::Generic, lint_statements);

        assert!(!document.tokens.is_empty());
        assert!(document
            .tokens
            .iter()
            .any(|token| token.statement_index == Some(0)));
        assert!(document
            .tokens
            .iter()
            .any(|token| token.statement_index == Some(1)));
    }
}
