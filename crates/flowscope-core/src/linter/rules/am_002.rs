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
    query.limit_clause.is_some() || query.fetch.is_some()
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
                }
            }
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
}
