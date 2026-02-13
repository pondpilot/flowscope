//! LINT_LT_007: Layout CTE bracket.
//!
//! SQLFluff LT07 parity (current scope): detect `WITH ... AS SELECT` patterns
//! that appear to miss CTE-body brackets.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

pub struct LayoutCteBracket;

impl LintRule for LayoutCteBracket {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_007
    }

    fn name(&self) -> &'static str {
        "Layout CTE bracket"
    }

    fn description(&self) -> &'static str {
        "CTE bodies should be wrapped in brackets."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if has_unbracketed_cte_pattern(ctx.statement_sql()) {
            vec![Issue::warning(
                issue_codes::LINT_LT_007,
                "CTE AS clause appears to be missing surrounding brackets.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn has_unbracketed_cte_pattern(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut index = 0usize;

    while let Some(with_start) = find_word(bytes, index, "with") {
        let mut cursor = with_start + 4;

        let ws_after_with = consume_whitespace(bytes, cursor);
        if ws_after_with == cursor {
            index = with_start + 1;
            continue;
        }
        cursor = ws_after_with;

        let Some((_, ident_end)) = parse_identifier(bytes, cursor) else {
            index = with_start + 1;
            continue;
        };
        cursor = ident_end;

        let ws_after_ident = consume_whitespace(bytes, cursor);
        if ws_after_ident == cursor {
            index = with_start + 1;
            continue;
        }
        cursor = ws_after_ident;

        let Some((as_start, as_end)) = parse_word(bytes, cursor) else {
            index = with_start + 1;
            continue;
        };
        if !eq_ignore_ascii_case(bytes, as_start, as_end, "as") {
            index = with_start + 1;
            continue;
        }
        cursor = as_end;

        let ws_after_as = consume_whitespace(bytes, cursor);
        if ws_after_as == cursor {
            index = with_start + 1;
            continue;
        }
        cursor = ws_after_as;

        let Some((select_start, select_end)) = parse_word(bytes, cursor) else {
            index = with_start + 1;
            continue;
        };
        if eq_ignore_ascii_case(bytes, select_start, select_end, "select") {
            return true;
        }

        index = with_start + 1;
    }

    false
}

fn find_word(bytes: &[u8], from: usize, target: &str) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
        let Some((start, end)) = parse_word(bytes, i) else {
            i += 1;
            continue;
        };

        if eq_ignore_ascii_case(bytes, start, end, target) {
            return Some(start);
        }

        i = end;
    }

    None
}

fn parse_word(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    if start >= bytes.len() || !is_word_char(bytes[start]) {
        return None;
    }

    let mut end = start;
    while end < bytes.len() && is_word_char(bytes[end]) {
        end += 1;
    }

    if start > 0 && is_word_char(bytes[start - 1]) {
        return None;
    }
    if end < bytes.len() && is_word_char(bytes[end]) {
        return None;
    }

    Some((start, end))
}

fn parse_identifier(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    if start >= bytes.len() || !is_identifier_start(bytes[start]) {
        return None;
    }

    let mut end = start + 1;
    while end < bytes.len() && is_identifier_char(bytes[end]) {
        end += 1;
    }

    Some((start, end))
}

fn consume_whitespace(bytes: &[u8], mut start: usize) -> usize {
    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    start
}

fn eq_ignore_ascii_case(bytes: &[u8], start: usize, end: usize, target: &str) -> bool {
    let len = end.saturating_sub(start);
    len == target.len() && bytes[start..end].eq_ignore_ascii_case(target.as_bytes())
}

fn is_word_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_char(byte: u8) -> bool {
    is_word_char(byte)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutCteBracket;
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
    fn flags_missing_cte_brackets_pattern() {
        let issues = run("SELECT 'WITH cte AS SELECT 1' AS sql_snippet");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_007);
    }

    #[test]
    fn does_not_flag_bracketed_cte() {
        assert!(run("WITH cte AS (SELECT 1) SELECT * FROM cte").is_empty());
    }
}
