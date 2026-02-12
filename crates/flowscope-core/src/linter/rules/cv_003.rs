//! LINT_CV_003: Prefer IS [NOT] NULL over =/<> NULL.
//!
//! Comparisons like `col = NULL` or `col <> NULL` are not valid null checks in SQL.
//! Use `IS NULL` / `IS NOT NULL` instead.

use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

pub struct NullComparison;

impl LintRule for NullComparison {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_003
    }

    fn name(&self) -> &'static str {
        "Null comparison style"
    }

    fn description(&self) -> &'static str {
        "Use IS NULL / IS NOT NULL instead of = NULL or <> NULL."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        visit::visit_expressions(stmt, &mut |expr| {
            let Expr::BinaryOp { left, op, right } = expr else {
                return;
            };

            if !is_null_expr(left) && !is_null_expr(right) {
                return;
            }

            let message = match op {
                BinaryOperator::Eq => Some("Use IS NULL instead of = NULL."),
                BinaryOperator::NotEq => Some("Use IS NOT NULL instead of <> NULL or != NULL."),
                _ => None,
            };

            if let Some(message) = message {
                issues.push(
                    Issue::info(issue_codes::LINT_CV_003, message)
                        .with_statement(ctx.statement_index),
                );
            }
        });
        issues
    }
}

fn is_null_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Value(ValueWithSpan {
            value: Value::Null,
            ..
        })
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = NullComparison;
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
    fn test_eq_null_detected() {
        let issues = check_sql("SELECT * FROM t WHERE a = NULL");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_CV_003");
    }

    #[test]
    fn test_not_eq_null_detected() {
        let issues = check_sql("SELECT * FROM t WHERE a <> NULL");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_CV_003");
    }

    #[test]
    fn test_null_left_side_detected() {
        let issues = check_sql("SELECT * FROM t WHERE NULL = a");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_is_null_ok() {
        let issues = check_sql("SELECT * FROM t WHERE a IS NULL");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_is_not_null_ok() {
        let issues = check_sql("SELECT * FROM t WHERE a IS NOT NULL");
        assert!(issues.is_empty());
    }
}
