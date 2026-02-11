//! LINT_ST_003: Deeply nested CASE expressions.
//!
//! CASE expressions nested more than 3 levels deep are hard to read
//! and maintain. Consider refactoring into a CTE, lookup table, or
//! using COALESCE/NULLIF where appropriate.

use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

/// Maximum nesting depth before triggering a warning.
const MAX_CASE_DEPTH: usize = 3;

pub struct DeeplyNestedCase;

impl LintRule for DeeplyNestedCase {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_003
    }

    fn name(&self) -> &'static str {
        "Deeply nested CASE"
    }

    fn description(&self) -> &'static str {
        "CASE expressions nested more than 3 levels deep are hard to maintain."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut reported = false;
        visit::visit_expressions(stmt, &mut |expr| {
            if !reported {
                let depth = case_nesting_depth(expr);
                if depth > MAX_CASE_DEPTH {
                    issues.push(
                        Issue::warning(
                            issue_codes::LINT_ST_003,
                            format!(
                                "CASE expression nested {} levels deep (max recommended: {}).",
                                depth, MAX_CASE_DEPTH
                            ),
                        )
                        .with_statement(ctx.statement_index),
                    );
                    reported = true;
                }
            }
        });
        issues
    }
}

/// Calculates the nesting depth of CASE expressions.
fn case_nesting_depth(expr: &Expr) -> usize {
    match expr {
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let mut max_child = 0;
            if let Some(op) = operand {
                max_child = max_child.max(case_nesting_depth(op));
            }
            for case_when in conditions {
                max_child = max_child.max(case_nesting_depth(&case_when.condition));
                max_child = max_child.max(case_nesting_depth(&case_when.result));
            }
            if let Some(el) = else_result {
                max_child = max_child.max(case_nesting_depth(el));
            }
            1 + max_child
        }
        Expr::Nested(inner) => case_nesting_depth(inner),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = DeeplyNestedCase;
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
    fn test_deeply_nested_case_detected() {
        let sql = "SELECT CASE WHEN a THEN CASE WHEN b THEN CASE WHEN c THEN CASE WHEN d THEN 1 END END END END FROM t";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_ST_003");
    }

    #[test]
    fn test_shallow_case_ok() {
        let sql = "SELECT CASE WHEN a THEN 1 WHEN b THEN 2 ELSE 3 END FROM t";
        let issues = check_sql(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_three_levels_ok() {
        let sql = "SELECT CASE WHEN a THEN CASE WHEN b THEN CASE WHEN c THEN 1 END END END FROM t";
        let issues = check_sql(sql);
        assert!(issues.is_empty());
    }

    // --- Edge cases ---

    #[test]
    fn test_nested_in_else_branch() {
        let sql = "SELECT CASE WHEN a THEN 1 ELSE CASE WHEN b THEN 2 ELSE CASE WHEN c THEN 3 ELSE CASE WHEN d THEN 4 END END END END FROM t";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("4"));
    }

    #[test]
    fn test_two_levels_ok() {
        let sql = "SELECT CASE WHEN a THEN CASE WHEN b THEN 1 END END FROM t";
        let issues = check_sql(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_one_report_per_statement() {
        // Even with multiple deeply nested CASEs, we only report once per statement
        let sql = "SELECT \
            CASE WHEN a THEN CASE WHEN b THEN CASE WHEN c THEN CASE WHEN d THEN 1 END END END END, \
            CASE WHEN e THEN CASE WHEN f THEN CASE WHEN g THEN CASE WHEN h THEN 2 END END END END \
            FROM t";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_deeply_nested_in_cte() {
        let sql = "WITH cte AS (SELECT CASE WHEN a THEN CASE WHEN b THEN CASE WHEN c THEN CASE WHEN d THEN 1 END END END END AS val FROM t) SELECT * FROM cte";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
    }
}
