//! LINT_LT_007: Layout CTE bracket.
//!
//! SQLFluff LT07 parity (current scope): in multiline CTE bodies, the closing
//! bracket should appear on its own line.

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
        if has_misplaced_cte_closing_bracket(ctx.statement_sql()) {
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

fn has_misplaced_cte_closing_bracket(sql: &str) -> bool {
    if !sql
        .as_bytes()
        .windows(4)
        .any(|window| window.eq_ignore_ascii_case(b"with"))
    {
        return false;
    }

    let bytes = sql.as_bytes();
    let mut index = 0usize;

    while let Some((as_start, as_end)) = find_word(bytes, index, "as") {
        let open_idx = consume_whitespace(bytes, as_end);
        if open_idx >= bytes.len() || bytes[open_idx] != b'(' {
            index = as_start + 1;
            continue;
        }

        let Some(close_idx) = matching_close_paren_ignoring_strings_and_comments(sql, open_idx)
        else {
            index = open_idx + 1;
            continue;
        };

        let body = &sql[open_idx + 1..close_idx];
        if body.contains('\n') && !line_prefix_before(sql, close_idx).trim().is_empty() {
            return true;
        }

        index = close_idx + 1;
    }

    false
}

fn line_prefix_before(sql: &str, idx: usize) -> &str {
    let line_start = sql[..idx].rfind('\n').map_or(0, |pos| pos + 1);
    &sql[line_start..idx]
}

fn find_word(bytes: &[u8], from: usize, target: &str) -> Option<(usize, usize)> {
    let mut i = from;
    while i < bytes.len() {
        let Some((start, end)) = parse_word(bytes, i) else {
            i += 1;
            continue;
        };

        if eq_ignore_ascii_case(bytes, start, end, target) {
            return Some((start, end));
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
    fn flags_closing_paren_after_sql_code_in_multiline_cte() {
        let issues = run("with cte_1 as (\n    select foo\n    from tbl_1) select * from cte_1");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_007);
    }

    #[test]
    fn does_not_flag_single_line_cte_body() {
        assert!(run("WITH cte AS (SELECT 1) SELECT * FROM cte").is_empty());
    }

    #[test]
    fn does_not_flag_multiline_cte_with_own_line_close() {
        let sql = "with cte as (\n    select 1\n) select * from cte";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn flags_templated_close_paren_on_same_line_as_cte_body_code() {
        let sql =
            "with\n{% if true %}\n  cte as (\n      select 1)\n{% endif %}\nselect * from cte";
        assert!(has_misplaced_cte_closing_bracket(sql));
    }
}
