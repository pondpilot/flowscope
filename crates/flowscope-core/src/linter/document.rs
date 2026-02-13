//! Shared linter document model.
//!
//! A `LintDocument` is constructed once per SQL source and reused across all
//! rule engines. It carries source text, dialect metadata, parsed statements,
//! and tokenizer output with stable spans.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

use crate::analyzer::helpers::line_col_to_offset;
use crate::linter::config::canonicalize_rule_code;
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

#[derive(Debug, Clone)]
enum NoqaDirective {
    All,
    Rules(HashSet<String>),
}

/// `-- noqa` suppression directives indexed by 1-based line number.
#[derive(Debug, Clone, Default)]
pub struct NoqaMap {
    directives: HashMap<usize, NoqaDirective>,
}

impl NoqaMap {
    /// Returns true if `code` is suppressed on `line`.
    pub fn is_suppressed(&self, line: usize, code: &str) -> bool {
        let Some(directive) = self.directives.get(&line) else {
            return false;
        };

        match directive {
            NoqaDirective::All => true,
            NoqaDirective::Rules(rules) => {
                let canonical = canonicalize_rule_code(code)
                    .unwrap_or_else(|| code.trim().to_ascii_uppercase());
                rules.contains(&canonical)
            }
        }
    }

    fn suppress_all(&mut self, line: usize) {
        self.directives.insert(line, NoqaDirective::All);
    }

    fn suppress_rules(&mut self, line: usize, codes: HashSet<String>) {
        match self.directives.get_mut(&line) {
            Some(NoqaDirective::All) => {}
            Some(NoqaDirective::Rules(existing)) => existing.extend(codes),
            None => {
                self.directives.insert(line, NoqaDirective::Rules(codes));
            }
        }
    }
}

/// Normalized lint input model for a single SQL source.
pub struct LintDocument<'a> {
    pub sql: &'a str,
    pub dialect: Dialect,
    pub statements: Vec<LintStatement<'a>>,
    pub tokens: Vec<LintToken>,
    pub noqa: NoqaMap,
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
        let noqa = extract_noqa(sql, &tokens);

        Self {
            sql,
            dialect,
            statements,
            tokens,
            noqa,
            parser_fallback_used: false,
            tokenizer_fallback_used,
        }
    }
}

fn extract_noqa(sql: &str, tokens: &[LintToken]) -> NoqaMap {
    let mut directives = NoqaMap::default();

    for token in tokens {
        if token.kind != LintTokenKind::Comment {
            continue;
        }

        let Some(parsed) = parse_noqa_comment(&token.text) else {
            continue;
        };

        let line = offset_to_line(sql, token.span.start);
        match parsed {
            ParsedNoqa::All => directives.suppress_all(line),
            ParsedNoqa::Rules(rules) => directives.suppress_rules(line, rules),
        }
    }

    directives
}

enum ParsedNoqa {
    All,
    Rules(HashSet<String>),
}

fn parse_noqa_comment(comment_text: &str) -> Option<ParsedNoqa> {
    let lowered = comment_text.to_ascii_lowercase();
    let marker_pos = lowered.find("noqa")?;
    let suffix = comment_text[marker_pos + 4..].trim();

    if suffix.is_empty() {
        return Some(ParsedNoqa::All);
    }

    let Some(rule_list) = suffix.strip_prefix(':') else {
        return Some(ParsedNoqa::All);
    };
    let rule_list = rule_list.trim();
    if rule_list.is_empty() {
        return Some(ParsedNoqa::All);
    }

    let mut rules = HashSet::new();
    for item in rule_list.split(',') {
        let token = item
            .trim()
            .trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | ';'));
        if token.is_empty() {
            continue;
        }
        if let Some(code) = canonicalize_rule_code(token) {
            rules.insert(code);
        }
    }

    if rules.is_empty() {
        return None;
    }

    Some(ParsedNoqa::Rules(rules))
}

fn offset_to_line(sql: &str, offset: usize) -> usize {
    1 + sql
        .as_bytes()
        .iter()
        .take(offset.min(sql.len()))
        .filter(|byte| **byte == b'\n')
        .count()
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

    #[test]
    fn parses_noqa_directives() {
        let sql = "SELECT a FROM foo -- noqa: AL01, ambiguous.join\nSELECT 1 -- noqa";
        let document = LintDocument::new(sql, Dialect::Generic, Vec::new());

        assert!(document.noqa.is_suppressed(1, "AL01"));
        assert!(document.noqa.is_suppressed(1, "LINT_AM_005"));
        assert!(!document.noqa.is_suppressed(1, "LINT_RF_001"));
        assert!(document.noqa.is_suppressed(2, "LINT_RF_001"));
    }
}
