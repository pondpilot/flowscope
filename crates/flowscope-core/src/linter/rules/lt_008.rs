//! LINT_LT_008: Layout CTE newline.
//!
//! SQLFluff LT08 parity (current scope): require a blank line between CTE body
//! closing parenthesis and following query/CTE text.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

pub struct LayoutCteNewline;

impl LintRule for LayoutCteNewline {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_008
    }

    fn name(&self) -> &'static str {
        "Layout CTE newline"
    }

    fn description(&self) -> &'static str {
        "Blank line should separate CTE blocks from following code."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        lt08_violation_spans(ctx.statement_sql())
            .into_iter()
            .map(|(start, end)| {
                Issue::info(
                    issue_codes::LINT_LT_008,
                    "Blank line expected but not found after CTE closing bracket.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
            })
            .collect()
    }
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_keyword_at(sql: &str, idx: usize, keyword: &str) -> bool {
    let bytes = sql.as_bytes();
    let kw = keyword.as_bytes();
    if idx + kw.len() > bytes.len() {
        return false;
    }
    if idx > 0 && is_word_byte(bytes[idx - 1]) {
        return false;
    }
    if idx + kw.len() < bytes.len() && is_word_byte(bytes[idx + kw.len()]) {
        return false;
    }
    bytes[idx..idx + kw.len()].eq_ignore_ascii_case(kw)
}

fn skip_whitespace_and_comments(sql: &str, mut idx: usize) -> usize {
    let bytes = sql.as_bytes();
    while idx < bytes.len() {
        if bytes[idx].is_ascii_whitespace() {
            idx += 1;
            continue;
        }
        if idx + 1 < bytes.len() && bytes[idx] == b'-' && bytes[idx + 1] == b'-' {
            idx += 2;
            while idx < bytes.len() && bytes[idx] != b'\n' {
                idx += 1;
            }
            continue;
        }
        if idx + 1 < bytes.len() && bytes[idx] == b'/' && bytes[idx + 1] == b'*' {
            idx += 2;
            while idx + 1 < bytes.len() && !(bytes[idx] == b'*' && bytes[idx + 1] == b'/') {
                idx += 1;
            }
            if idx + 1 < bytes.len() {
                idx += 2;
            }
            continue;
        }
        break;
    }
    idx
}

fn matching_close_paren_ignoring_strings_and_comments(sql: &str, open_idx: usize) -> Option<usize> {
    let bytes = sql.as_bytes();
    if open_idx >= bytes.len() || bytes[open_idx] != b'(' {
        return None;
    }

    let mut idx = open_idx + 1;
    let mut depth = 1usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while idx < bytes.len() {
        if in_line_comment {
            if bytes[idx] == b'\n' {
                in_line_comment = false;
            }
            idx += 1;
            continue;
        }

        if in_block_comment {
            if idx + 1 < bytes.len() && bytes[idx] == b'*' && bytes[idx + 1] == b'/' {
                in_block_comment = false;
                idx += 2;
            } else {
                idx += 1;
            }
            continue;
        }

        if in_single {
            if bytes[idx] == b'\'' {
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'\'' {
                    idx += 2;
                } else {
                    in_single = false;
                    idx += 1;
                }
            } else {
                idx += 1;
            }
            continue;
        }

        if in_double {
            if bytes[idx] == b'"' {
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'"' {
                    idx += 2;
                } else {
                    in_double = false;
                    idx += 1;
                }
            } else {
                idx += 1;
            }
            continue;
        }

        if idx + 1 < bytes.len() && bytes[idx] == b'-' && bytes[idx + 1] == b'-' {
            in_line_comment = true;
            idx += 2;
            continue;
        }
        if idx + 1 < bytes.len() && bytes[idx] == b'/' && bytes[idx + 1] == b'*' {
            in_block_comment = true;
            idx += 2;
            continue;
        }
        if bytes[idx] == b'\'' {
            in_single = true;
            idx += 1;
            continue;
        }
        if bytes[idx] == b'"' {
            in_double = true;
            idx += 1;
            continue;
        }

        match bytes[idx] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
        idx += 1;
    }

    None
}

fn lt08_anchor_end(sql: &str, start: usize) -> usize {
    let bytes = sql.as_bytes();
    if start >= bytes.len() {
        return start;
    }

    if is_word_byte(bytes[start]) {
        let mut end = start + 1;
        while end < bytes.len() && is_word_byte(bytes[end]) {
            end += 1;
        }
        end
    } else {
        (start + 1).min(bytes.len())
    }
}

fn lt08_suffix_summary(sql: &str, mut idx: usize) -> (usize, Option<usize>, bool) {
    let bytes = sql.as_bytes();
    let mut blank_lines = 0usize;
    let mut line_blank = false;
    let mut saw_comma = false;

    while idx < bytes.len() {
        if idx + 1 < bytes.len() && bytes[idx] == b'-' && bytes[idx + 1] == b'-' {
            line_blank = false;
            idx += 2;
            while idx < bytes.len() && bytes[idx] != b'\n' {
                idx += 1;
            }
            continue;
        }

        if idx + 1 < bytes.len() && bytes[idx] == b'/' && bytes[idx + 1] == b'*' {
            line_blank = false;
            idx += 2;
            while idx + 1 < bytes.len() {
                if bytes[idx] == b'\n' {
                    if line_blank {
                        blank_lines += 1;
                    }
                    line_blank = true;
                    idx += 1;
                    continue;
                }

                if bytes[idx] == b'*' && bytes[idx + 1] == b'/' {
                    idx += 2;
                    break;
                }

                line_blank = false;
                idx += 1;
            }
            continue;
        }

        match bytes[idx] {
            b',' => {
                saw_comma = true;
                idx += 1;
            }
            b'\n' => {
                if line_blank {
                    blank_lines += 1;
                }
                line_blank = true;
                idx += 1;
            }
            b if b.is_ascii_whitespace() => idx += 1,
            _ => return (blank_lines, Some(idx), saw_comma),
        }
    }

    (blank_lines, None, saw_comma)
}

fn lt08_violation_spans(sql: &str) -> Vec<(usize, usize)> {
    let bytes = sql.as_bytes();
    let mut spans = Vec::new();

    let mut idx = skip_whitespace_and_comments(sql, 0);
    if !is_keyword_at(sql, idx, "WITH") {
        return spans;
    }
    idx += "WITH".len();
    idx = skip_whitespace_and_comments(sql, idx);
    if is_keyword_at(sql, idx, "RECURSIVE") {
        idx += "RECURSIVE".len();
    }

    while idx < bytes.len() {
        idx = skip_whitespace_and_comments(sql, idx);
        if idx >= bytes.len() {
            break;
        }

        if !is_keyword_at(sql, idx, "AS") {
            idx += 1;
            continue;
        }

        let mut body_start = skip_whitespace_and_comments(sql, idx + "AS".len());
        if is_keyword_at(sql, body_start, "NOT") {
            body_start = skip_whitespace_and_comments(sql, body_start + "NOT".len());
        }
        if is_keyword_at(sql, body_start, "MATERIALIZED") {
            body_start = skip_whitespace_and_comments(sql, body_start + "MATERIALIZED".len());
        }
        if body_start >= bytes.len() || bytes[body_start] != b'(' {
            idx += 1;
            continue;
        }

        let Some(close_idx) = matching_close_paren_ignoring_strings_and_comments(sql, body_start)
        else {
            break;
        };

        let (blank_lines, next_code_idx, saw_comma) = lt08_suffix_summary(sql, close_idx + 1);
        if blank_lines == 0 {
            if let Some(start) = next_code_idx {
                spans.push((start, lt08_anchor_end(sql, start)));
            }
        }

        if !saw_comma {
            break;
        }
        let Some(next_idx) = next_code_idx else {
            break;
        };
        idx = next_idx;
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutCteNewline;
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
    fn flags_missing_blank_line_after_cte() {
        assert!(!run("WITH cte AS (SELECT 1) SELECT * FROM cte").is_empty());
        assert!(!run("WITH cte AS (SELECT 1)\nSELECT * FROM cte").is_empty());
    }

    #[test]
    fn does_not_flag_with_blank_line_after_cte() {
        assert!(run("WITH cte AS (SELECT 1)\n\nSELECT * FROM cte").is_empty());
    }

    #[test]
    fn flags_each_missing_separator_between_multiple_ctes() {
        let issues = run("WITH a AS (SELECT 1),
-- comment between CTEs
b AS (SELECT 2)
SELECT * FROM b");
        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_008)
                .count(),
            2,
        );
    }
}
