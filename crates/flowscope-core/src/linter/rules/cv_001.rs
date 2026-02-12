//! LINT_CV_001: COALESCE over CASE WHEN IS NULL.
//!
//! Detects `CASE WHEN x IS NULL THEN y ELSE x END` patterns that can be
//! simplified to `COALESCE(x, y)`.

use crate::linter::helpers;
use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

pub struct CoalesceOverCase;

impl LintRule for CoalesceOverCase {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_001
    }

    fn name(&self) -> &'static str {
        "COALESCE pattern"
    }

    fn description(&self) -> &'static str {
        "CASE WHEN x IS NULL THEN y ELSE x END can be simplified to COALESCE(x, y)."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        visit::visit_expressions(stmt, &mut |expr| {
            if helpers::is_coalesce_pattern(expr) {
                issues.push(
                    Issue::info(
                        issue_codes::LINT_CV_001,
                        "Use COALESCE(x, y) instead of CASE WHEN x IS NULL THEN y ELSE x END.",
                    )
                    .with_statement(ctx.statement_index),
                );
            }
        });
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = CoalesceOverCase;
        let ctx = LintContext {
            sql,
            statement_range: 0..sql.len(),
            statement_index: 0,
        };
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check(stmt, &ctx));
        }
        issues
    }

    #[test]
    fn test_coalesce_pattern_detected() {
        let issues = check_sql("SELECT CASE WHEN x IS NULL THEN 'default' ELSE x END FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_CV_001");
    }

    #[test]
    fn test_different_else_ok() {
        let issues = check_sql("SELECT CASE WHEN x IS NULL THEN 'default' ELSE y END FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_not_is_null_ok() {
        let issues = check_sql("SELECT CASE WHEN x > 0 THEN 'positive' ELSE x END FROM t");
        assert!(issues.is_empty());
    }

    // --- Edge cases adopted from sqlfluff CV02 (convention.coalesce) ---

    #[test]
    fn test_multiple_when_branches_not_triggered() {
        // Only single-branch CASE should match COALESCE pattern
        let issues = check_sql(
            "SELECT CASE WHEN x IS NULL THEN 'a' WHEN y IS NULL THEN 'b' ELSE x END FROM t",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_coalesce_already_used_ok() {
        let issues = check_sql("SELECT COALESCE(x, 'default') FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_case_is_not_null_not_triggered() {
        let issues = check_sql("SELECT CASE WHEN x IS NOT NULL THEN x ELSE 'default' END FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_coalesce_pattern_in_where_clause() {
        let issues = check_sql("SELECT * FROM t WHERE (CASE WHEN x IS NULL THEN 0 ELSE x END) > 5");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_coalesce_pattern_in_cte() {
        let issues = check_sql(
            "WITH cte AS (SELECT CASE WHEN x IS NULL THEN 0 ELSE x END AS val FROM t) SELECT * FROM cte",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_simple_case_not_triggered() {
        // Simple CASE (CASE x WHEN ...) is not the COALESCE pattern
        let issues = check_sql("SELECT CASE x WHEN 1 THEN 'a' ELSE 'b' END FROM t");
        assert!(issues.is_empty());
    }
}
