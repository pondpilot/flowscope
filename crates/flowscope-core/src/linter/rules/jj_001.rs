//! LINT_JJ_001: Jinja padding.
//!
//! SQLFluff JJ01 parity (current scope): detect inconsistent whitespace around
//! Jinja delimiters.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

pub struct JinjaPadding;

impl LintRule for JinjaPadding {
    fn code(&self) -> &'static str {
        issue_codes::LINT_JJ_001
    }

    fn name(&self) -> &'static str {
        "Jinja padding"
    }

    fn description(&self) -> &'static str {
        "Jinja tags should use consistent padding."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let Some((start, end)) = jinja_padding_violation_span(ctx) else {
            return Vec::new();
        };

        vec![Issue::info(
            issue_codes::LINT_JJ_001,
            "Jinja tag spacing appears inconsistent.",
        )
        .with_statement(ctx.statement_index)
        .with_span(ctx.span_from_statement_offset(start, end))]
    }
}

fn jinja_padding_violation_span(ctx: &LintContext) -> Option<(usize, usize)> {
    let sql = ctx.statement_sql();
    let tokens = token_spans_for_context(ctx).or_else(|| token_spans(sql, ctx.dialect()))?;

    for token in &tokens {
        if let Some(span) = token_text_violation(sql, token) {
            return Some(span);
        }
    }

    for pair in tokens.windows(2) {
        let left = &pair[0];
        let right = &pair[1];
        if is_open_delimiter_tokens(&left.token, &right.token) {
            let delimiter_start = left.start;
            let delimiter_end = right.end;
            if has_missing_padding_after(sql, delimiter_end) {
                return Some((delimiter_start, delimiter_end));
            }
        }

        if is_close_delimiter_tokens(&left.token, &right.token) {
            let delimiter_start = left.start;
            let delimiter_end = right.end;
            if has_missing_padding_before(sql, delimiter_start) {
                return Some((delimiter_start, delimiter_end));
            }
        }
    }

    None
}

struct TokenSpan {
    token: Token,
    start: usize,
    end: usize,
}

fn token_spans(sql: &str, dialect: Dialect) -> Option<Vec<TokenSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens: Vec<TokenWithSpan> = tokenizer.tokenize_with_location().ok()?;

    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        let start = line_col_to_offset(
            sql,
            token.span.start.line as usize,
            token.span.start.column as usize,
        )?;
        let end = line_col_to_offset(
            sql,
            token.span.end.line as usize,
            token.span.end.column as usize,
        )?;
        if start < end {
            out.push(TokenSpan {
                token: token.token,
                start,
                end,
            });
        }
    }

    Some(out)
}

fn token_spans_for_context(ctx: &LintContext) -> Option<Vec<TokenSpan>> {
    let offset = ctx.statement_range.start;
    ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut out = Vec::new();
        for token in tokens {
            let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                continue;
            };
            if start < ctx.statement_range.start || end > ctx.statement_range.end {
                continue;
            }
            if start < end {
                out.push(TokenSpan {
                    token: token.token.clone(),
                    start: start - offset,
                    end: end - offset,
                });
            }
        }

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    })
}

fn token_text_violation(sql: &str, token: &TokenSpan) -> Option<(usize, usize)> {
    let text = &sql[token.start..token.end];

    for pattern in &OPEN_DELIMITERS {
        for (idx, _) in text.match_indices(pattern) {
            let delimiter_start = token.start + idx;
            let delimiter_end = delimiter_start + pattern.len();
            if has_missing_padding_after(sql, delimiter_end) {
                return Some((delimiter_start, delimiter_end));
            }
        }
    }

    for pattern in &CLOSE_DELIMITERS {
        for (idx, _) in text.match_indices(pattern) {
            let delimiter_start = token.start + idx;
            if has_missing_padding_before(sql, delimiter_start) {
                return Some((delimiter_start, delimiter_start + pattern.len()));
            }
        }
    }

    None
}

const OPEN_DELIMITERS: [&str; 3] = ["{{", "{%", "{#"];
const CLOSE_DELIMITERS: [&str; 3] = ["}}", "%}", "#}"];

fn is_open_delimiter_tokens(left: &Token, right: &Token) -> bool {
    matches!(
        (left, right),
        (Token::LBrace, Token::LBrace)
            | (Token::LBrace, Token::Mod)
            | (Token::LBrace, Token::Sharp)
    )
}

fn is_close_delimiter_tokens(left: &Token, right: &Token) -> bool {
    matches!(
        (left, right),
        (Token::RBrace, Token::RBrace)
            | (Token::Mod, Token::RBrace)
            | (Token::Sharp, Token::RBrace)
    )
}

fn has_missing_padding_after(sql: &str, delimiter_end: usize) -> bool {
    match sql
        .get(delimiter_end..)
        .and_then(|remainder| remainder.chars().next())
    {
        Some(next) => !is_padding_or_trim_marker(next),
        None => true,
    }
}

fn has_missing_padding_before(sql: &str, delimiter_start: usize) -> bool {
    if delimiter_start == 0 {
        return true;
    }

    match sql[..delimiter_start].chars().rev().next() {
        Some(prev) => !is_padding_or_trim_marker(prev),
        None => true,
    }
}

fn is_padding_or_trim_marker(ch: char) -> bool {
    ch.is_whitespace() || ch == '-'
}

fn line_col_to_offset(sql: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }

    let mut current_line = 1usize;
    let mut current_col = 1usize;

    for (offset, ch) in sql.char_indices() {
        if current_line == line && current_col == column {
            return Some(offset);
        }

        if ch == '\n' {
            current_line += 1;
            current_col = 1;
        } else {
            current_col += 1;
        }
    }

    if current_line == line && current_col == column {
        return Some(sql.len());
    }

    None
}

fn token_with_span_offsets(sql: &str, token: &TokenWithSpan) -> Option<(usize, usize)> {
    let start = line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )?;
    let end = line_col_to_offset(
        sql,
        token.span.end.line as usize,
        token.span.end.column as usize,
    )?;
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = JinjaPadding;
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check(
                    statement,
                    &LintContext {
                        sql,
                        statement_range: 0..sql.len(),
                        statement_index: index,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn flags_missing_padding_in_jinja_expression() {
        let issues = run("SELECT '{{foo}}' AS templated");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
        assert_eq!(
            issues[0].span.expect("expected span").start,
            "SELECT '{{foo}}' AS templated"
                .find("{{")
                .expect("expected opening delimiter"),
        );
    }

    #[test]
    fn does_not_flag_padded_jinja_expression() {
        assert!(run("SELECT '{{ foo }}' AS templated").is_empty());
    }

    #[test]
    fn flags_missing_padding_in_jinja_statement_tag() {
        let issues = run("SELECT '{%for x in y %}' AS templated");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
    }

    #[test]
    fn flags_missing_padding_before_statement_close_tag() {
        let issues = run("SELECT '{% for x in y%}' AS templated");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
    }

    #[test]
    fn flags_missing_padding_in_jinja_comment_tag() {
        let issues = run("SELECT '{#comment#}' AS templated");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
    }

    #[test]
    fn allows_jinja_trim_markers() {
        assert!(run("SELECT '{{- foo -}}' AS templated").is_empty());
        assert!(run("SELECT '{%- if x -%}' AS templated").is_empty());
    }
}
