//! LINT_CP_004: Literal capitalisation.
//!
//! SQLFluff CP04 parity (current scope): detect mixed-case usage for
//! NULL/TRUE/FALSE literal keywords.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct CapitalisationLiterals;

impl LintRule for CapitalisationLiterals {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_004
    }

    fn name(&self) -> &'static str {
        "Literal capitalisation"
    }

    fn description(&self) -> &'static str {
        "NULL/TRUE/FALSE should use a consistent case style."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = mask_comments_and_single_quoted_strings(ctx.statement_sql());
        if mixed_case_for_tokens(&literal_tokens(&sql)) {
            vec![Issue::info(
                issue_codes::LINT_CP_004,
                "Literal keywords (NULL/TRUE/FALSE) use inconsistent capitalisation.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn capture_group(sql: &str, pattern: &str, group: usize) -> Vec<String> {
    Regex::new(pattern)
        .expect("valid regex")
        .captures_iter(sql)
        .filter_map(|captures| captures.get(group).map(|m| m.as_str().to_string()))
        .collect()
}

fn literal_tokens(sql: &str) -> Vec<String> {
    capture_group(sql, r"(?i)\b(null|true|false)\b", 1)
}

fn mixed_case_for_tokens(tokens: &[String]) -> bool {
    if tokens.len() < 2 {
        return false;
    }

    let mut saw_upper = false;
    let mut saw_lower = false;
    let mut saw_mixed = false;

    for token in tokens {
        let upper = token.to_ascii_uppercase();
        let lower = token.to_ascii_lowercase();
        if token == &upper {
            saw_upper = true;
        } else if token == &lower {
            saw_lower = true;
        } else {
            saw_mixed = true;
        }
    }

    saw_mixed || (saw_upper && saw_lower)
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
        let rule = CapitalisationLiterals;
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
    fn flags_mixed_literal_case() {
        let issues = run("SELECT NULL, true FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_004);
    }

    #[test]
    fn does_not_flag_consistent_literal_case() {
        assert!(run("SELECT NULL, TRUE FROM t").is_empty());
    }
}
