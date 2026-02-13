//! LINT_CV_001: Not-equal style.
//!
//! SQLFluff CV01 parity (current scope): flag statements that mix `<>` and
//! `!=` not-equal operators.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

pub struct ConventionNotEqual;

impl LintRule for ConventionNotEqual {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_001
    }

    fn name(&self) -> &'static str {
        "Not-equal style"
    }

    fn description(&self) -> &'static str {
        "Use a consistent not-equal operator style."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if statement_mixes_not_equal_styles(ctx.statement_sql()) {
            vec![Issue::info(
                issue_codes::LINT_CV_001,
                "Use consistent not-equal style (prefer !=).",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn statement_mixes_not_equal_styles(sql: &str) -> bool {
    let mut chars = sql.char_indices().peekable();
    let mut saw_angle_style = false;
    let mut saw_bang_style = false;

    enum ScanState {
        Normal,
        SingleQuote,
        DoubleQuote,
        LineComment,
        BlockComment,
    }

    let mut state = ScanState::Normal;

    while let Some((idx, ch)) = chars.next() {
        match state {
            ScanState::Normal => match ch {
                '\'' => state = ScanState::SingleQuote,
                '"' => state = ScanState::DoubleQuote,
                '-' => {
                    if sql[idx + ch.len_utf8()..].starts_with('-') {
                        chars.next();
                        state = ScanState::LineComment;
                    }
                }
                '/' => {
                    if sql[idx + ch.len_utf8()..].starts_with('*') {
                        chars.next();
                        state = ScanState::BlockComment;
                    }
                }
                '<' => {
                    if sql[idx + ch.len_utf8()..].starts_with('>') {
                        saw_angle_style = true;
                    }
                }
                '!' => {
                    if sql[idx + ch.len_utf8()..].starts_with('=') {
                        saw_bang_style = true;
                    }
                }
                _ => {}
            },
            ScanState::SingleQuote => {
                if ch == '\'' {
                    if sql[idx + ch.len_utf8()..].starts_with('\'') {
                        chars.next();
                    } else {
                        state = ScanState::Normal;
                    }
                }
            }
            ScanState::DoubleQuote => {
                if ch == '"' {
                    if sql[idx + ch.len_utf8()..].starts_with('"') {
                        chars.next();
                    } else {
                        state = ScanState::Normal;
                    }
                }
            }
            ScanState::LineComment => {
                if ch == '\n' {
                    state = ScanState::Normal;
                }
            }
            ScanState::BlockComment => {
                if ch == '*' && sql[idx + ch.len_utf8()..].starts_with('/') {
                    chars.next();
                    state = ScanState::Normal;
                }
            }
        }

        if saw_angle_style && saw_bang_style {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionNotEqual;
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
    fn flags_mixed_not_equal_styles() {
        let issues = run("SELECT * FROM t WHERE a <> b AND c != d");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_001);
    }

    #[test]
    fn does_not_flag_single_not_equal_style() {
        assert!(run("SELECT * FROM t WHERE a <> b").is_empty());
        assert!(run("SELECT * FROM t WHERE a != b").is_empty());
    }

    #[test]
    fn does_not_flag_not_equal_tokens_inside_string_literal() {
        assert!(run("SELECT 'a <> b and c != d' AS txt FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_not_equal_tokens_inside_comments() {
        assert!(run("SELECT * FROM t -- a <> b and c != d").is_empty());
    }
}
