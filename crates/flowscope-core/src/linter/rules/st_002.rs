//! LINT_ST_002: Unnecessary ELSE NULL in CASE expressions.
//!
//! `CASE ... ELSE NULL END` is redundant because CASE already returns NULL
//! when no branch matches. The ELSE NULL can be removed.

use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

pub struct UnnecessaryElseNull;

impl LintRule for UnnecessaryElseNull {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_002
    }

    fn name(&self) -> &'static str {
        "Unnecessary ELSE NULL"
    }

    fn description(&self) -> &'static str {
        "ELSE NULL is redundant in CASE expressions; CASE returns NULL by default when no branch matches."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        visit::visit_expressions(stmt, &mut |expr| {
            if let Expr::Case {
                else_result: Some(else_expr),
                ..
            } = expr
            {
                if is_null_expr(else_expr) {
                    issues.push(
                        Issue::info(
                            issue_codes::LINT_ST_002,
                            "ELSE NULL is redundant in CASE expressions; it can be removed.",
                        )
                        .with_statement(ctx.statement_index),
                    );
                }
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
        let rule = UnnecessaryElseNull;
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
    fn test_else_null_detected() {
        let issues = check_sql("SELECT CASE WHEN x > 1 THEN 'a' ELSE NULL END FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_ST_002");
    }

    #[test]
    fn test_no_else_ok() {
        let issues = check_sql("SELECT CASE WHEN x > 1 THEN 'a' END FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_else_value_ok() {
        let issues = check_sql("SELECT CASE WHEN x > 1 THEN 'a' ELSE 'b' END FROM t");
        assert!(issues.is_empty());
    }

    // --- Edge cases adopted from sqlfluff ST01 (structure.else_null) ---

    #[test]
    fn test_simple_case_else_null() {
        // CASE x WHEN ... ELSE NULL END
        let issues = check_sql(
            "SELECT CASE name WHEN 'cat' THEN 'meow' WHEN 'dog' THEN 'woof' ELSE NULL END FROM t",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_else_with_complex_expression_ok() {
        let issues =
            check_sql("SELECT CASE name WHEN 'cat' THEN 'meow' ELSE UPPER(name) END FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_multiple_when_branches_else_null() {
        let issues = check_sql(
            "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' WHEN x = 3 THEN 'c' ELSE NULL END FROM t",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_nested_case_else_null() {
        // Both the inner and outer CASE have ELSE NULL
        let issues = check_sql(
            "SELECT CASE WHEN x > 0 THEN CASE WHEN y > 0 THEN 'pos' ELSE NULL END ELSE NULL END FROM t",
        );
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_else_null_in_where_clause() {
        let issues =
            check_sql("SELECT * FROM t WHERE (CASE WHEN x > 0 THEN 1 ELSE NULL END) IS NOT NULL");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_else_null_in_cte() {
        let issues = check_sql(
            "WITH cte AS (SELECT CASE WHEN x > 0 THEN 'yes' ELSE NULL END AS flag FROM t) SELECT * FROM cte",
        );
        assert_eq!(issues.len(), 1);
    }
}
