//! LINT_AL_001: Table alias style.
//!
//! Require explicit `AS` when aliasing tables.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct AliasingTableStyle;

impl LintRule for AliasingTableStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_001
    }

    fn name(&self) -> &'static str {
        "Table alias style"
    }

    fn description(&self) -> &'static str {
        "Implicit/explicit aliasing of table."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = mask_comments_and_single_quoted_strings(ctx.statement_sql());
        let spans = implicit_table_alias_spans(&sql);

        spans
            .into_iter()
            .map(|(start, end)| {
                Issue::warning(
                    issue_codes::LINT_AL_001,
                    "Use explicit AS when aliasing tables.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
            })
            .collect()
    }
}

fn has_re(haystack: &str, pattern: &str) -> bool {
    Regex::new(pattern).expect("valid regex").is_match(haystack)
}

fn capture_group_with_spans(
    haystack: &str,
    pattern: &str,
    group: usize,
) -> Vec<(String, usize, usize)> {
    let re = Regex::new(pattern).expect("valid regex");
    re.captures_iter(haystack)
        .filter_map(|caps| {
            caps.get(group)
                .map(|m| (m.as_str().to_string(), m.start(), m.end()))
        })
        .collect()
}

fn matching_open_paren(sql: &str, close_paren_idx: usize) -> Option<usize> {
    let bytes = sql.as_bytes();
    let mut depth = 0usize;
    for idx in (0..=close_paren_idx).rev() {
        match bytes[idx] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }
    None
}

fn previous_significant_token(sql: &str, before: usize) -> Option<(String, usize)> {
    let bytes = sql.as_bytes();
    let mut idx = before;
    while idx > 0 && bytes[idx - 1].is_ascii_whitespace() {
        idx -= 1;
    }
    if idx == 0 {
        return None;
    }
    if bytes[idx - 1] == b',' {
        return Some((",".to_string(), idx - 1));
    }
    let token_end = idx;
    while idx > 0 {
        let b = bytes[idx - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            idx -= 1;
        } else {
            break;
        }
    }
    if idx == token_end {
        return None;
    }
    Some((sql[idx..token_end].to_ascii_uppercase(), idx))
}

fn is_derived_table_alias(sql: &str, alias_start: usize) -> bool {
    let bytes = sql.as_bytes();
    let mut idx = alias_start;
    while idx > 0 && bytes[idx - 1].is_ascii_whitespace() {
        idx -= 1;
    }
    if idx == 0 || bytes[idx - 1] != b')' {
        return false;
    }

    let close_paren_idx = idx - 1;
    let Some(open_paren_idx) = matching_open_paren(sql, close_paren_idx) else {
        return false;
    };
    let Some((mut token, token_start)) = previous_significant_token(sql, open_paren_idx) else {
        return false;
    };

    if token == "LATERAL" {
        let Some((prev_token, _)) = previous_significant_token(sql, token_start) else {
            return false;
        };
        token = prev_token;
    }

    if token != "FROM" && token != "JOIN" && token != "," {
        return false;
    }

    let inner = &sql[open_paren_idx + 1..close_paren_idx];
    has_re(inner, r"(?i)\bselect\b")
}

fn is_keyword(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "SELECT"
            | "FROM"
            | "WHERE"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "FULL"
            | "OUTER"
            | "INNER"
            | "CROSS"
            | "ON"
            | "USING"
            | "AS"
            | "GROUP"
            | "ORDER"
            | "HAVING"
            | "LIMIT"
            | "OFFSET"
            | "UNION"
            | "ALL"
            | "DISTINCT"
            | "BY"
            | "WHEN"
            | "THEN"
            | "ELSE"
            | "END"
    )
}

fn implicit_table_alias_spans(sql: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();

    for (alias, start, end) in capture_group_with_spans(
        sql,
        r"(?i)\b(?:from|join)\s+(?:only\s+)?(?:[A-Za-z_][A-Za-z0-9_\$]*)(?:\.[A-Za-z_][A-Za-z0-9_\$]*)*\s+([A-Za-z_][A-Za-z0-9_]*)",
        1,
    ) {
        if !is_keyword(&alias) {
            spans.push((start, end));
        }
    }

    for (alias, start, end) in capture_group_with_spans(
        sql,
        r"(?is)\b(?:from|join)\s+(?:lateral\s+)?[A-Za-z_][A-Za-z0-9_]*\s*\([^)]*\)\s+([A-Za-z_][A-Za-z0-9_]*)",
        1,
    ) {
        if !is_keyword(&alias) {
            spans.push((start, end));
        }
    }

    for (alias, start, end) in
        capture_group_with_spans(sql, r"(?i)\)\s+([A-Za-z_][A-Za-z0-9_]*)", 1)
    {
        if is_keyword(&alias) {
            continue;
        }
        if is_derived_table_alias(sql, start) {
            spans.push((start, end));
        }
    }

    spans.sort_unstable();
    spans.dedup();
    spans
}

fn mask_comments_and_single_quoted_strings(sql: &str) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Normal,
        LineComment,
        BlockComment,
        SingleQuoted,
    }

    let mut bytes = sql.as_bytes().to_vec();
    let mut i = 0usize;
    let mut state = State::Normal;

    while i < bytes.len() {
        match state {
            State::Normal => {
                if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                    bytes[i] = b' ';
                    bytes[i + 1] = b' ';
                    i += 2;
                    state = State::LineComment;
                } else if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    bytes[i] = b' ';
                    bytes[i + 1] = b' ';
                    i += 2;
                    state = State::BlockComment;
                } else if bytes[i] == b'\'' {
                    bytes[i] = b' ';
                    i += 1;
                    state = State::SingleQuoted;
                } else {
                    i += 1;
                }
            }
            State::LineComment => {
                if bytes[i] == b'\n' {
                    i += 1;
                    state = State::Normal;
                } else {
                    bytes[i] = b' ';
                    i += 1;
                }
            }
            State::BlockComment => {
                if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    bytes[i] = b' ';
                    bytes[i + 1] = b' ';
                    i += 2;
                    state = State::Normal;
                } else if bytes[i] == b'\n' {
                    i += 1;
                } else {
                    bytes[i] = b' ';
                    i += 1;
                }
            }
            State::SingleQuoted => {
                if bytes[i] == b'\'' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        bytes[i] = b' ';
                        bytes[i + 1] = b' ';
                        i += 2;
                    } else {
                        bytes[i] = b' ';
                        i += 1;
                        state = State::Normal;
                    }
                } else if bytes[i] == b'\n' {
                    i += 1;
                } else {
                    bytes[i] = b' ';
                    i += 1;
                }
            }
        }
    }

    String::from_utf8(bytes).expect("masked SQL remains utf8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).expect("parse");
        let rule = AliasingTableStyle;
        stmts
            .iter()
            .enumerate()
            .flat_map(|(index, stmt)| {
                rule.check(
                    stmt,
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
    fn flags_implicit_table_aliases() {
        let issues = run("select * from users u join orders o on u.id = o.user_id");
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|i| i.code == issue_codes::LINT_AL_001));
    }

    #[test]
    fn allows_explicit_as_table_aliases() {
        let issues = run("select * from users as u join orders as o on u.id = o.user_id");
        assert!(issues.is_empty());
    }
}
