//! LINT_LT_006: Layout functions.
//!
//! SQLFluff LT06 parity (current scope): flag function-like tokens separated
//! from opening parenthesis.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct LayoutFunctions;

impl LintRule for LayoutFunctions {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_006
    }

    fn name(&self) -> &'static str {
        "Layout functions"
    }

    fn description(&self) -> &'static str {
        "Function call spacing should be consistent."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = mask_comments_and_single_quoted_strings(ctx.statement_sql());
        let re = Regex::new(r"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\s+\(").expect("valid regex");

        for caps in re.captures_iter(&sql) {
            let token = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            if is_keyword(token) {
                continue;
            }
            if let Some(name) = caps.get(1) {
                let prev_word = sql[..name.start()]
                    .split_whitespace()
                    .last()
                    .unwrap_or("")
                    .to_ascii_uppercase();

                // Skip table/target contexts like INSERT INTO table_name (...).
                if matches!(
                    prev_word.as_str(),
                    "INTO" | "FROM" | "JOIN" | "UPDATE" | "TABLE"
                ) {
                    continue;
                }

                // Skip schema-qualified object references (e.g. metrics.table_name (...)).
                if name.start() > 0 && sql.as_bytes()[name.start() - 1] == b'.' {
                    continue;
                }

                return vec![Issue::info(
                    issue_codes::LINT_LT_006,
                    "Function call spacing appears inconsistent.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(name.start(), name.end()))];
            }
        }

        Vec::new()
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
        "OVERWRITE",
        "PARTITION",
        "PRECEDING",
        "PRIMARY",
        "QUALIFY",
        "RANGE",
        "RECURSIVE",
        "REFERENCES",
        "RETURNING",
        "RIGHT",
        "ROW",
        "ROWS",
        "SECOND",
        "SELECT",
        "SEMI",
        "SET",
        "SMALLINT",
        "SOME",
        "STRAIGHT",
        "TABLE",
        "TEXT",
        "THEN",
        "TIMESTAMP",
        "WEEK",
        "YEAR",
        "TINYINT",
        "TRUE",
        "UNBOUNDED",
        "UNION",
        "UNNEST",
        "UPDATE",
        "USING",
        "UUID",
        "VALUES",
        "VARCHAR",
        "VIEW",
        "WHEN",
        "WHERE",
        "WINDOW",
        "WITH",
        "WITHIN",
        "WITHOUT",
    ];
    KEYWORDS.contains(&token.to_ascii_uppercase().as_str())
}

fn mask_comments_and_single_quoted_strings(sql: &str) -> String {
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
                    bytes[i] = b' ';
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        bytes[i + 1] = b' ';
                        i += 2;
                    } else {
                        i += 1;
                        state = State::Normal;
                    }
                } else {
                    if bytes[i] != b'\n' {
                        bytes[i] = b' ';
                    }
                    i += 1;
                }
            }
        }
    }

    String::from_utf8(bytes).expect("masked SQL remains UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutFunctions;
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
    fn flags_space_between_function_name_and_paren() {
        let issues = run("SELECT COUNT (1) FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_006);
    }

    #[test]
    fn does_not_flag_normal_function_call() {
        assert!(run("SELECT COUNT(1) FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_table_name_followed_by_paren() {
        assert!(run("INSERT INTO metrics_table (id) VALUES (1)").is_empty());
    }
}
