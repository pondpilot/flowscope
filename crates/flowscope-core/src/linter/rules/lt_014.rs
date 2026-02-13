//! LINT_LT_014: Layout keyword newline.
//!
//! SQLFluff LT14 parity (current scope): detect inconsistent major-clause
//! keyword placement relative to the SELECT line.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct LayoutKeywordNewline;

impl LintRule for LayoutKeywordNewline {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_014
    }

    fn name(&self) -> &'static str {
        "Layout keyword newline"
    }

    fn description(&self) -> &'static str {
        "Major clauses should be consistently line-broken."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = mask_comments_and_single_quoted_strings(ctx.statement_sql());
        let select_line_re = Regex::new(r"(?im)^\s*select\b([^\n]*)").expect("valid regex");
        let major_clause_re =
            Regex::new(r"(?i)\b(from|where|group\s+by|order\s+by)\b").expect("valid regex");

        let Some(select_caps) = select_line_re.captures(&sql) else {
            return Vec::new();
        };
        let Some(select_tail) = select_caps.get(1) else {
            return Vec::new();
        };

        let mut clause_iter = major_clause_re.find_iter(select_tail.as_str());
        let Some(first_clause) = clause_iter.next() else {
            return Vec::new();
        };

        let has_second_clause_on_select_line = clause_iter.next().is_some();
        let has_major_clause_on_later_line = major_clause_re.is_match(&sql[select_tail.end()..]);
        if !has_second_clause_on_select_line && !has_major_clause_on_later_line {
            return Vec::new();
        }

        let keyword_start = select_tail.start() + first_clause.start();
        let keyword_end = select_tail.start() + first_clause.end();

        vec![Issue::info(
            issue_codes::LINT_LT_014,
            "Major clauses should be consistently line-broken.",
        )
        .with_statement(ctx.statement_index)
        .with_span(ctx.span_from_statement_offset(keyword_start, keyword_end))]
    }
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
        let rule = LayoutKeywordNewline;
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
    fn flags_inconsistent_major_clause_placement() {
        assert!(!run("SELECT a FROM t WHERE a = 1").is_empty());
        assert!(!run("SELECT a FROM t\nWHERE a = 1").is_empty());
    }

    #[test]
    fn does_not_flag_consistent_layout() {
        assert!(run("SELECT a FROM t").is_empty());
        assert!(run("SELECT a\nFROM t\nWHERE a = 1").is_empty());
    }
}
