//! LINT_CV_001: prefer COALESCE over IFNULL/NVL.
//!
//! SQLFluff CV02 parity: detect IFNULL/NVL function usage and recommend
//! COALESCE for portability and consistency.

use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Expr, Statement};

pub struct CoalesceConvention;

impl LintRule for CoalesceConvention {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_001
    }

    fn name(&self) -> &'static str {
        "COALESCE convention"
    }

    fn description(&self) -> &'static str {
        "Use COALESCE instead of IFNULL or NVL."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();

        visit::visit_expressions(stmt, &mut |expr| {
            let Expr::Function(function) = expr else {
                return;
            };

            let function_name = function.name.to_string();
            let function_name_upper = function_name.to_ascii_uppercase();

            if function_name_upper != "IFNULL" && function_name_upper != "NVL" {
                return;
            }

            issues.push(
                Issue::info(
                    issue_codes::LINT_CV_001,
                    format!("Use 'COALESCE' instead of '{}'.", function_name_upper),
                )
                .with_statement(ctx.statement_index),
            );
        });

        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CoalesceConvention;
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

    // --- Edge cases adopted from sqlfluff CV02 ---

    #[test]
    fn passes_coalesce() {
        let issues = run("SELECT coalesce(foo, 0) AS bar FROM baz");
        assert!(issues.is_empty());
    }

    #[test]
    fn fails_ifnull() {
        let issues = run("SELECT ifnull(foo, 0) AS bar FROM baz");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_001);
    }

    #[test]
    fn fails_nvl() {
        let issues = run("SELECT nvl(foo, 0) AS bar FROM baz");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_001);
    }

    #[test]
    fn does_not_flag_case_when_null_pattern_anymore() {
        let issues = run("SELECT CASE WHEN x IS NULL THEN 'default' ELSE x END FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_nested_ifnull_calls() {
        let issues = run("SELECT SUM(IFNULL(amount, 0)) AS total FROM orders");
        assert_eq!(issues.len(), 1);
    }
}
