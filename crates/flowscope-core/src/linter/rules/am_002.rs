//! LINT_AM_002: ORDER BY without LIMIT in subqueries.
//!
//! ORDER BY in a subquery or CTE without LIMIT/TOP is meaningless because
//! the outer query does not preserve the ordering. The database may ignore it
//! or it may add unnecessary sorting overhead.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

pub struct OrderByWithoutLimit;

impl LintRule for OrderByWithoutLimit {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_002
    }

    fn name(&self) -> &'static str {
        "ORDER BY without LIMIT"
    }

    fn description(&self) -> &'static str {
        "ORDER BY in a subquery or CTE without LIMIT has no guaranteed effect."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        match stmt {
            Statement::Query(q) => {
                // Only check subqueries and CTEs, not the top-level query
                if let Some(ref with) = q.with {
                    for cte in &with.cte_tables {
                        check_subquery(&cte.query, ctx, &mut issues);
                    }
                }
                check_set_expr_subqueries(&q.body, ctx, &mut issues);
            }
            Statement::Insert(ins) => {
                if let Some(ref source) = ins.source {
                    if let Some(ref with) = source.with {
                        for cte in &with.cte_tables {
                            check_subquery(&cte.query, ctx, &mut issues);
                        }
                    }
                    check_set_expr_subqueries(&source.body, ctx, &mut issues);
                }
            }
            Statement::CreateView { query, .. } => {
                // The VIEW query itself is essentially a subquery
                check_subquery(query, ctx, &mut issues);
            }
            _ => {}
        }
        issues
    }
}

fn has_order_by(query: &Query) -> bool {
    query.order_by.as_ref().is_some_and(|ob| match &ob.kind {
        OrderByKind::Expressions(exprs) => !exprs.is_empty(),
        OrderByKind::All(_) => true,
    })
}

fn has_limit(query: &Query) -> bool {
    query.limit_clause.is_some()
        || query.fetch.is_some()
        || query
            .body
            .as_select()
            .is_some_and(|select| select.top.is_some())
}

fn check_subquery(query: &Query, ctx: &LintContext, issues: &mut Vec<Issue>) {
    if has_order_by(query) && !has_limit(query) {
        issues.push(
            Issue::info(
                issue_codes::LINT_AM_002,
                "ORDER BY in a subquery or CTE without LIMIT has no guaranteed effect.",
            )
            .with_statement(ctx.statement_index),
        );
    }

    // Recurse into CTEs and subqueries within this query
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            check_subquery(&cte.query, ctx, issues);
        }
    }
    check_set_expr_subqueries(&query.body, ctx, issues);
}

fn check_set_expr_subqueries(body: &SetExpr, ctx: &LintContext, issues: &mut Vec<Issue>) {
    match body {
        SetExpr::Select(select) => {
            // Check derived tables (subqueries in FROM)
            for item in &select.from {
                check_table_factor_subqueries(&item.relation, ctx, issues);
                for join in &item.joins {
                    check_table_factor_subqueries(&join.relation, ctx, issues);
                    if let Some(on_expr) = join_constraint_expr(&join.join_operator) {
                        check_expr_subqueries(on_expr, ctx, issues);
                    }
                }
            }
            check_select_expr_subqueries(select, ctx, issues);
        }
        SetExpr::Query(q) => {
            check_subquery(q, ctx, issues);
        }
        SetExpr::SetOperation { left, right, .. } => {
            check_set_expr_subqueries(left, ctx, issues);
            check_set_expr_subqueries(right, ctx, issues);
        }
        _ => {}
    }
}

fn check_select_expr_subqueries(select: &Select, ctx: &LintContext, issues: &mut Vec<Issue>) {
    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                check_expr_subqueries(expr, ctx, issues);
            }
            _ => {}
        }
    }
    if let Some(selection) = &select.selection {
        check_expr_subqueries(selection, ctx, issues);
    }
    if let Some(having) = &select.having {
        check_expr_subqueries(having, ctx, issues);
    }
    if let Some(qualify) = &select.qualify {
        check_expr_subqueries(qualify, ctx, issues);
    }
    if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            check_expr_subqueries(expr, ctx, issues);
        }
    }
    for sort_expr in &select.sort_by {
        check_expr_subqueries(&sort_expr.expr, ctx, issues);
    }
}

fn check_expr_subqueries(expr: &Expr, ctx: &LintContext, issues: &mut Vec<Issue>) {
    match expr {
        Expr::Subquery(subquery) | Expr::Exists { subquery, .. } => {
            check_subquery(subquery, ctx, issues);
        }
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => {
            check_expr_subqueries(inner, ctx, issues);
            check_subquery(subquery, ctx, issues);
        }
        Expr::BinaryOp { left, right, .. } => {
            check_expr_subqueries(left, ctx, issues);
            check_expr_subqueries(right, ctx, issues);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            check_expr_subqueries(inner, ctx, issues);
        }
        Expr::InList { expr, list, .. } => {
            check_expr_subqueries(expr, ctx, issues);
            for item in list {
                check_expr_subqueries(item, ctx, issues);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            check_expr_subqueries(expr, ctx, issues);
            check_expr_subqueries(low, ctx, issues);
            check_expr_subqueries(high, ctx, issues);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                check_expr_subqueries(op, ctx, issues);
            }
            for case_when in conditions {
                check_expr_subqueries(&case_when.condition, ctx, issues);
                check_expr_subqueries(&case_when.result, ctx, issues);
            }
            if let Some(el) = else_result {
                check_expr_subqueries(el, ctx, issues);
            }
        }
        Expr::Function(func) => {
            if let FunctionArguments::List(arg_list) = &func.args {
                for arg in &arg_list.args {
                    if let FunctionArg::Unnamed(FunctionArgExpr::Expr(inner)) = arg {
                        check_expr_subqueries(inner, ctx, issues);
                    }
                }
            }
        }
        _ => {}
    }
}

fn join_constraint_expr(op: &JoinOperator) -> Option<&Expr> {
    let constraint = match op {
        JoinOperator::Join(c)
        | JoinOperator::Inner(c)
        | JoinOperator::LeftOuter(c)
        | JoinOperator::RightOuter(c)
        | JoinOperator::FullOuter(c)
        | JoinOperator::LeftSemi(c)
        | JoinOperator::RightSemi(c)
        | JoinOperator::LeftAnti(c)
        | JoinOperator::RightAnti(c) => c,
        _ => return None,
    };
    match constraint {
        JoinConstraint::On(expr) => Some(expr),
        _ => None,
    }
}

fn check_table_factor_subqueries(
    relation: &TableFactor,
    ctx: &LintContext,
    issues: &mut Vec<Issue>,
) {
    match relation {
        TableFactor::Derived { subquery, .. } => {
            check_subquery(subquery, ctx, issues);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            check_table_factor_subqueries(&table_with_joins.relation, ctx, issues);
            for join in &table_with_joins.joins {
                check_table_factor_subqueries(&join.relation, ctx, issues);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = OrderByWithoutLimit;
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
    fn test_order_by_in_cte_without_limit() {
        let issues = check_sql("WITH cte AS (SELECT * FROM t ORDER BY id) SELECT * FROM cte");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_AM_002");
    }

    #[test]
    fn test_order_by_in_cte_with_limit_ok() {
        let issues =
            check_sql("WITH cte AS (SELECT * FROM t ORDER BY id LIMIT 10) SELECT * FROM cte");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_order_by_at_top_level_ok() {
        let issues = check_sql("SELECT * FROM t ORDER BY id");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_order_by_in_subquery_without_limit() {
        let issues = check_sql("SELECT * FROM (SELECT * FROM t ORDER BY id) AS sub");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_order_by_in_scalar_subquery_without_limit() {
        let issues = check_sql("SELECT (SELECT x FROM t ORDER BY x) AS y");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_AM_002");
    }

    // --- Edge cases adopted from sqlfluff ---

    #[test]
    fn test_order_by_in_cte_with_fetch_ok() {
        let issues = check_sql(
            "WITH cte AS (SELECT * FROM t ORDER BY id FETCH FIRST 10 ROWS ONLY) SELECT * FROM cte",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_order_by_in_create_view() {
        let issues = check_sql("CREATE VIEW v AS SELECT * FROM t ORDER BY id");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_nested_subquery_order_by() {
        // ORDER BY in inner subquery without LIMIT
        let issues = check_sql(
            "SELECT * FROM (SELECT * FROM (SELECT * FROM t ORDER BY id) AS inner_sub) AS outer_sub",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_order_by_in_insert_source() {
        let issues = check_sql(
            "INSERT INTO target WITH cte AS (SELECT * FROM t ORDER BY id) SELECT * FROM cte",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_multiple_ctes_one_with_order_by() {
        let issues = check_sql(
            "WITH a AS (SELECT * FROM t ORDER BY id), b AS (SELECT * FROM t) SELECT * FROM a JOIN b ON a.id = b.id",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_order_by_in_subquery_with_limit_ok() {
        let issues = check_sql("SELECT * FROM (SELECT * FROM t ORDER BY id LIMIT 5) AS sub");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_no_order_by_anywhere_ok() {
        let issues = check_sql("WITH cte AS (SELECT * FROM t) SELECT * FROM cte");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_order_by_in_cte_with_top_ok() {
        let issues =
            check_sql("WITH cte AS (SELECT TOP 10 * FROM t ORDER BY id) SELECT * FROM cte");
        assert!(issues.is_empty());
    }
}
