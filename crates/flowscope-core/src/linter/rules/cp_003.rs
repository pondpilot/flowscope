//! LINT_CP_003: Function capitalisation.
//!
//! SQLFluff CP03 parity (current scope): detect inconsistent function name
//! capitalisation.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct CapitalisationFunctions;

impl LintRule for CapitalisationFunctions {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_003
    }

    fn name(&self) -> &'static str {
        "Function capitalisation"
    }

    fn description(&self) -> &'static str {
        "Functions should use a consistent case style."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = mask_comments_and_single_quoted_strings(ctx.statement_sql());
        let functions = function_tokens_with_spans(&sql);
        if functions.is_empty() {
            return Vec::new();
        }

        let preferred_style = functions
            .iter()
            .map(|(name, _, _)| case_style(name))
            .find(|style| *style == "lower" || *style == "upper")
            .unwrap_or("lower");

        functions
            .into_iter()
            .filter(|(name, _, _)| {
                let style = case_style(name);
                (style == "lower" || style == "upper" || style == "mixed")
                    && style != preferred_style
            })
            .map(|(_, start, end)| {
                Issue::info(
                    issue_codes::LINT_CP_003,
                    "Function names use inconsistent capitalisation.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
            })
            .collect()
    }
}

fn is_keyword(token: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "ALL",
        "ALTER",
        "AND",
        "ANY",
        "ANTI",
        "ARRAY",
        "AS",
        "ASC",
        "BEGIN",
        "BETWEEN",
        "BY",
        "CAST",
        "CASE",
        "CONFLICT",
        "CONSTRAINT",
        "CREATE",
        "CROSS",
        "CURRENT",
        "CURRENT_DATE",
        "CURRENT_TIME",
        "CURRENT_TIMESTAMP",
        "DATE",
        "DAY",
        "DECIMAL",
        "DELETE",
        "DESC",
        "DISTINCT",
        "DO",
        "DOUBLE",
        "DROP",
        "DOW",
        "DOY",
        "EPOCH",
        "ELSE",
        "END",
        "EXCEPT",
        "EXCLUDED",
        "EXISTS",
        "FALSE",
        "FETCH",
        "FILTER",
        "FIRST",
        "FLOAT",
        "FOLLOWING",
        "FOR",
        "FOREIGN",
        "FROM",
        "HOUR",
        "FULL",
        "GO",
        "GROUP",
        "HAVING",
        "IF",
        "ILIKE",
        "IN",
        "INNER",
        "INSERT",
        "INTEGER",
        "INTERSECT",
        "INTERVAL",
        "ISODOW",
        "ISOYEAR",
        "INTO",
        "IS",
        "JOIN",
        "KEY",
        "LAST",
        "LATERAL",
        "LEFT",
        "LIKE",
        "LIMIT",
        "LOCALTIME",
        "LOCALTIMESTAMP",
        "MATERIALIZED",
        "NATURAL",
        "NO",
        "MONTH",
        "NOT",
        "NULL",
        "NULLS",
        "NUMERIC",
        "OFFSET",
        "ON",
        "ONLY",
        "OR",
        "ORDER",
        "OUTER",
        "OVER",
        "PARTITION",
        "PRECEDING",
        "PRIMARY",
        "PROCEDURE",
        "RANGE",
        "RECURSIVE",
        "REFERENCES",
        "RETURNING",
        "RIGHT",
        "ROWS",
        "SECOND",
        "SELECT",
        "SET",
        "TABLE",
        "THEN",
        "TO",
        "TRUE",
        "UNBOUNDED",
        "UNION",
        "UNIQUE",
        "UNKNOWN",
        "UPDATE",
        "USING",
        "VALUES",
        "VIEW",
        "WHEN",
        "WHERE",
        "WINDOW",
        "WITH",
        "YEAR",
    ];
    KEYWORDS.contains(&token.to_ascii_uppercase().as_str())
}

fn case_style(token: &str) -> &'static str {
    if token.is_empty() {
        return "unknown";
    }
    if token == token.to_ascii_uppercase() {
        "upper"
    } else if token == token.to_ascii_lowercase() {
        "lower"
    } else if token
        .chars()
        .all(|ch| !ch.is_ascii_alphabetic() || ch.is_ascii_uppercase())
    {
        "upper"
    } else if token
        .chars()
        .all(|ch| !ch.is_ascii_alphabetic() || ch.is_ascii_lowercase())
    {
        "lower"
    } else {
        "mixed"
    }
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

fn function_tokens_with_spans(sql: &str) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();

    for (name, start, end) in
        capture_group_with_spans(sql, r"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\s*\(", 1)
    {
        if is_keyword(&name) && !name.eq_ignore_ascii_case("date") {
            continue;
        }

        let prev_word = sql[..start]
            .split_whitespace()
            .last()
            .unwrap_or("")
            .to_ascii_uppercase();
        if matches!(
            prev_word.as_str(),
            "INTO" | "FROM" | "JOIN" | "UPDATE" | "TABLE"
        ) {
            continue;
        }

        if start > 0 && sql.as_bytes()[start - 1] == b'.' {
            continue;
        }

        out.push((name, start, end));
    }

    out
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
                } else {
                    bytes[i] = b' ';
                    i += 1;
                }
            }
            State::SingleQuoted => {
                if bytes[i] == b'\'' {
                    bytes[i] = b' ';
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        bytes[i + 1] = b' ';
                        i += 2;
                    } else {
                        i += 1;
                        state = State::Normal;
                    }
                } else {
                    bytes[i] = b' ';
                    i += 1;
                }
            }
        }
    }

    String::from_utf8(bytes).expect("input SQL remains valid utf8 after masking")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CapitalisationFunctions;
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
    fn flags_mixed_function_case() {
        let issues = run("SELECT COUNT(*), count(x) FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_003);
    }

    #[test]
    fn does_not_flag_consistent_function_case() {
        assert!(run("SELECT lower(x), upper(y) FROM t").is_empty());
    }
}
