//! LINT_CV_002: Prefer COUNT(*) over COUNT(1).
//!
//! `COUNT(1)` and `COUNT(*)` are semantically identical in all major databases,
//! but `COUNT(*)` is the standard convention and more clearly expresses intent.

use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

pub struct CountStyle;

impl LintRule for CountStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_002
    }

    fn name(&self) -> &'static str {
        "COUNT style"
    }

    fn description(&self) -> &'static str {
        "Prefer COUNT(*) over COUNT(1) for clarity."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        visit::visit_expressions(stmt, &mut |expr| {
            if let Expr::Function(func) = expr {
                let fname = func.name.to_string().to_uppercase();
                if fname == "COUNT" && is_count_one(&func.args) {
                    issues.push(
                        Issue::info(
                            issue_codes::LINT_CV_002,
                            "Use COUNT(*) instead of COUNT(1) for clarity.",
                        )
                        .with_statement(ctx.statement_index),
                    );
                }
            }
        });
        issues
    }
}

fn is_count_one(args: &FunctionArguments) -> bool {
    let arg_list = match args {
        FunctionArguments::List(list) => list,
        _ => return false,
    };

    if arg_list.args.len() != 1 {
        return false;
    }
    matches!(
        &arg_list.args[0],
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(ValueWithSpan {
            value: Value::Number(n, _),
            ..
        }))) if n == "1"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = CountStyle;
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
    fn test_count_one_detected() {
        let issues = check_sql("SELECT COUNT(1) FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_CV_002");
    }

    #[test]
    fn test_count_star_ok() {
        let issues = check_sql("SELECT COUNT(*) FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_count_column_ok() {
        let issues = check_sql("SELECT COUNT(id) FROM t");
        assert!(issues.is_empty());
    }

    // --- Edge cases ---

    #[test]
    fn test_count_zero_not_flagged() {
        let issues = check_sql("SELECT COUNT(0) FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_count_one_in_having() {
        let issues = check_sql("SELECT col FROM t GROUP BY col HAVING COUNT(1) > 5");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_count_one_in_subquery() {
        let issues =
            check_sql("SELECT * FROM t WHERE id IN (SELECT COUNT(1) FROM t2 GROUP BY col)");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_multiple_count_one() {
        let issues = check_sql("SELECT COUNT(1), COUNT(1) FROM t");
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_count_distinct_ok() {
        let issues = check_sql("SELECT COUNT(DISTINCT id) FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_count_one_in_cte() {
        let issues = check_sql("WITH cte AS (SELECT COUNT(1) AS cnt FROM t) SELECT * FROM cte");
        assert_eq!(issues.len(), 1);
    }
}
