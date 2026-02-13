//! LINT_CV_001: Not-equal style.
//!
//! SQLFluff CV01 parity (current scope): flag statements that mix `<>` and
//! `!=` not-equal operators.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
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
        let sql = ctx.statement_sql();
        if has_re(sql, r"<>") && has_re(sql, r"!=") {
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

fn has_re(haystack: &str, pattern: &str) -> bool {
    Regex::new(pattern).expect("valid regex").is_match(haystack)
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
}
