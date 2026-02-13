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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NoqaDisableRange {
    start_line: usize,
    end_line: Option<usize>,
}

/// `-- noqa` suppression directives indexed by 1-based line number.
#[derive(Debug, Clone, Default)]
pub struct NoqaMap {
    directives: HashMap<usize, NoqaDirective>,
    disable_all_ranges: Vec<NoqaDisableRange>,
}

impl NoqaMap {
    /// Returns true if `code` is suppressed on `line`.
    pub fn is_suppressed(&self, line: usize, code: &str) -> bool {
        if self.disable_all_ranges.iter().any(|range| {
            line >= range.start_line
                && range
                    .end_line
                    .map(|end_line| line <= end_line)
                    .unwrap_or(true)
        }) {
            return true;
        }

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

    fn suppress_all_range(&mut self, start_line: usize, end_line: Option<usize>) {
        self.disable_all_ranges.push(NoqaDisableRange {
            start_line,
            end_line,
        });
    }
}

/// Normalized lint input model for a single SQL source.
pub struct LintDocument<'a> {
    pub sql: &'a str,
    pub source_sql: Option<&'a str>,
    pub source_statement_ranges: Vec<Option<Range<usize>>>,
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
        Self::new_with_parser_fallback_and_source(sql, None, dialect, statements, false, None)
    }

    /// Build a lint document with parser fallback provenance metadata.
    #[must_use]
    pub fn new_with_parser_fallback(
        sql: &'a str,
        dialect: Dialect,
        statements: Vec<LintStatement<'a>>,
        parser_fallback_used: bool,
    ) -> Self {
        Self::new_with_parser_fallback_and_source(
            sql,
            None,
            dialect,
            statements,
            parser_fallback_used,
            None,
        )
    }

    /// Build a lint document with parser fallback metadata and optional
    /// untemplated source mapping.
    #[must_use]
    pub fn new_with_parser_fallback_and_source(
        sql: &'a str,
        source_sql: Option<&'a str>,
        dialect: Dialect,
        statements: Vec<LintStatement<'a>>,
        parser_fallback_used: bool,
        source_statement_ranges: Option<Vec<Option<Range<usize>>>>,
    ) -> Self {
        let (tokens, tokenizer_fallback_used) = match tokenize_sql(sql, dialect, &statements) {
            Ok(tokens) => (tokens, false),
            Err(_) => (Vec::new(), true),
        };
        let noqa = extract_noqa(sql, &tokens);

        Self {
            sql,
            source_sql,
            source_statement_ranges: source_statement_ranges
                .unwrap_or_else(|| vec![None; statements.len()]),
            dialect,
            statements,
            tokens,
            noqa,
            parser_fallback_used,
            tokenizer_fallback_used,
        }
    }
}

fn extract_noqa(sql: &str, tokens: &[LintToken]) -> NoqaMap {
    let mut directives = NoqaMap::default();
    let mut disable_all_start: Option<usize> = None;

    for token in tokens {
        if token.kind != LintTokenKind::Comment {
            continue;
        }

        let Some(parsed) = parse_noqa_comment(&token.text) else {
            continue;
        };

        let start_line = offset_to_line(sql, token.span.start);
        let end_offset = token.span.end.saturating_sub(1);
        let end_line = offset_to_line(sql, end_offset);
        match parsed {
            ParsedNoqa::All => {
                for line in start_line..=end_line {
                    directives.suppress_all(line);
                }
            }
            ParsedNoqa::Rules(rules) => {
                for line in start_line..=end_line {
                    directives.suppress_rules(line, rules.clone());
                }
            }
            ParsedNoqa::DisableAll => {
                if disable_all_start.is_none() {
                    disable_all_start = Some(start_line);
                }
            }
            ParsedNoqa::EnableAll => {
                if let Some(start_line) = disable_all_start.take() {
                    directives.suppress_all_range(start_line, Some(end_line));
                }
            }
        }
    }

    if let Some(start_line) = disable_all_start {
        directives.suppress_all_range(start_line, None);
    }

    directives
}

enum ParsedNoqa {
    All,
    Rules(HashSet<String>),
    DisableAll,
    EnableAll,
}

fn parse_noqa_comment(comment_text: &str) -> Option<ParsedNoqa> {
    let body = comment_body(comment_text);
    let lowered = body.to_ascii_lowercase();
    let mut search_start = 0usize;
    let mut marker_pos = None;

    while let Some(rel) = lowered[search_start..].find("noqa") {
        let absolute = search_start + rel;
        let prefix = &body[..absolute];
        if prefix.trim().is_empty() || prefix.trim_end().ends_with("--") {
            marker_pos = Some(absolute);
            break;
        }
        search_start = absolute + 4;
    }

    let marker_pos = marker_pos?;
    let suffix = body[marker_pos + 4..].trim();

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

    if rule_list.eq_ignore_ascii_case("disable=all") {
        return Some(ParsedNoqa::DisableAll);
    }
    if rule_list.eq_ignore_ascii_case("enable=all") {
        return Some(ParsedNoqa::EnableAll);
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

fn comment_body(comment_text: &str) -> &str {
    let trimmed = comment_text.trim();
    if let Some(inner) = trimmed
        .strip_prefix("/*")
        .and_then(|text| text.strip_suffix("*/"))
    {
        return inner.trim();
    }
    if let Some(inner) = trimmed.strip_prefix("--") {
        return inner.trim();
    }
    if let Some(inner) = trimmed.strip_prefix('#') {
        return inner.trim();
    }
    trimmed
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
    fn records_parser_fallback_provenance() {
        let sql = "SELECT 1";
        let statements = parse_sql_with_dialect(sql, Dialect::Generic).expect("parse");
        let lint_statements = statements
            .iter()
            .enumerate()
            .map(|(index, statement)| LintStatement {
                statement,
                statement_index: index,
                statement_range: 0..sql.len(),
            })
            .collect::<Vec<_>>();

        let document =
            LintDocument::new_with_parser_fallback(sql, Dialect::Generic, lint_statements, true);

        assert!(document.parser_fallback_used);
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

    #[test]
    fn parses_disable_enable_all_noqa_directives() {
        let sql = "/* -- noqa: disable=all */\nSELECT 1\n/* noqa: enable=all */\nSELECT 2";
        let document = LintDocument::new(sql, Dialect::Generic, Vec::new());

        assert!(document.noqa.is_suppressed(2, "LINT_LT_005"));
        assert!(!document.noqa.is_suppressed(4, "LINT_LT_005"));
    }

    #[test]
    fn ignores_invalid_disable_all_without_double_dash_prefix() {
        let sql = "/* This won't work: noqa: disable=all */\nSELECT 1";
        let document = LintDocument::new(sql, Dialect::Generic, Vec::new());
        assert!(!document.noqa.is_suppressed(2, "LINT_LT_005"));
    }

    #[test]
    fn ignores_invalid_disable_all_with_trailing_text() {
        let sql = "/* -- noqa: disable=all Invalid declaration */\nSELECT 1";
        let document = LintDocument::new(sql, Dialect::Generic, Vec::new());
        assert!(!document.noqa.is_suppressed(2, "LINT_LT_005"));
    }
}
