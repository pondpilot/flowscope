//! LINT_ST_001: Unused CTE.
//!
//! A CTE (WITH clause) is defined but never referenced in the query body
//! or subsequent CTEs. This is likely dead code.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;
use std::collections::HashSet;

pub struct UnusedCte;

impl LintRule for UnusedCte {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_001
    }

    fn name(&self) -> &'static str {
        "Unused CTE"
    }

    fn description(&self) -> &'static str {
        "CTE defined in WITH clause but never referenced."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let query = match stmt {
            Statement::Query(q) => q,
            Statement::Insert(ins) => {
                if let Some(ref source) = ins.source {
                    source
                } else {
                    return Vec::new();
                }
            }
            Statement::CreateView { query, .. } => query,
            Statement::CreateTable(create) => {
                if let Some(ref q) = create.query {
                    q
                } else {
                    return Vec::new();
                }
            }
            _ => return Vec::new(),
        };

        let with = match &query.with {
            Some(w) => w,
            None => return Vec::new(),
        };

        // Collect all table references from the query body and other CTEs
        let mut referenced = HashSet::new();
        collect_query_refs(query, &mut referenced);

        // Each CTE can reference earlier CTEs
        for (i, cte) in with.cte_tables.iter().enumerate() {
            let mut cte_refs = HashSet::new();
            collect_query_refs(&cte.query, &mut cte_refs);
            // CTEs defined after this one can reference it
            for later_cte in &with.cte_tables[i + 1..] {
                collect_query_refs(&later_cte.query, &mut cte_refs);
            }
            referenced.extend(cte_refs);
        }

        let mut issues = Vec::new();
        for (i, cte) in with.cte_tables.iter().enumerate() {
            let name_upper = cte.alias.name.value.to_uppercase();
            if !referenced.contains(&name_upper) {
                // Check if any later CTE references this one
                let referenced_by_later = with.cte_tables[i + 1..].iter().any(|later| {
                    let mut refs = HashSet::new();
                    collect_query_refs(&later.query, &mut refs);
                    refs.contains(&name_upper)
                });
                if referenced_by_later {
                    continue;
                }

                let stmt_sql = ctx.statement_sql();
                let span = find_cte_name_span(stmt_sql, &cte.alias.name.value, ctx);
                let mut issue = Issue::warning(
                    issue_codes::LINT_ST_001,
                    format!(
                        "CTE '{}' is defined but never referenced.",
                        cte.alias.name.value
                    ),
                )
                .with_statement(ctx.statement_index);
                if let Some(s) = span {
                    issue = issue.with_span(s);
                }
                issues.push(issue);
            }
        }
        issues
    }
}

fn collect_query_refs(query: &Query, refs: &mut HashSet<String>) {
    collect_table_refs(&query.body, refs);
    if let Some(order_by) = &query.order_by {
        collect_order_by_refs(order_by, refs);
    }
}

/// Recursively collects uppercase table/CTE names referenced in a set expression.
fn collect_table_refs(expr: &SetExpr, refs: &mut HashSet<String>) {
    match expr {
        SetExpr::Select(select) => {
            for item in &select.from {
                collect_relation_refs(&item.relation, refs);
                for join in &item.joins {
                    collect_relation_refs(&join.relation, refs);
                    collect_join_constraint_refs(&join.join_operator, refs);
                }
            }
            // Check subqueries in SELECT and predicate expressions.
            for item in &select.projection {
                if let SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } = item
                {
                    collect_expr_table_refs(expr, refs);
                }
            }
            if let Some(prewhere) = &select.prewhere {
                collect_expr_table_refs(prewhere, refs);
            }
            if let Some(ref selection) = select.selection {
                collect_expr_table_refs(selection, refs);
            }
            if let Some(ref having) = select.having {
                collect_expr_table_refs(having, refs);
            }
            if let Some(ref qualify) = select.qualify {
                collect_expr_table_refs(qualify, refs);
            }
            if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
                for expr in exprs {
                    collect_expr_table_refs(expr, refs);
                }
            }
            for sort_expr in &select.sort_by {
                collect_expr_table_refs(&sort_expr.expr, refs);
            }
        }
        SetExpr::Query(q) => {
            collect_query_refs(q, refs);
            // Also check subquery CTEs
            if let Some(w) = &q.with {
                for cte in &w.cte_tables {
                    collect_query_refs(&cte.query, refs);
                }
            }
        }
        SetExpr::SetOperation { left, right, .. } => {
            collect_table_refs(left, refs);
            collect_table_refs(right, refs);
        }
        _ => {}
    }
}

/// Collects table/CTE references from subqueries inside expressions.
fn collect_expr_table_refs(expr: &Expr, refs: &mut HashSet<String>) {
    match expr {
        Expr::InSubquery { subquery, expr, .. } => {
            collect_query_refs(subquery, refs);
            if let Some(w) = &subquery.with {
                for cte in &w.cte_tables {
                    collect_query_refs(&cte.query, refs);
                }
            }
            collect_expr_table_refs(expr, refs);
        }
        Expr::Subquery(subquery) | Expr::Exists { subquery, .. } => {
            collect_query_refs(subquery, refs);
            if let Some(w) = &subquery.with {
                for cte in &w.cte_tables {
                    collect_query_refs(&cte.query, refs);
                }
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_expr_table_refs(left, refs);
            collect_expr_table_refs(right, refs);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner) => {
            collect_expr_table_refs(inner, refs);
        }
        Expr::InList { expr, list, .. } => {
            collect_expr_table_refs(expr, refs);
            for item in list {
                collect_expr_table_refs(item, refs);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            collect_expr_table_refs(expr, refs);
            collect_expr_table_refs(low, refs);
            collect_expr_table_refs(high, refs);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                collect_expr_table_refs(op, refs);
            }
            for case_when in conditions {
                collect_expr_table_refs(&case_when.condition, refs);
                collect_expr_table_refs(&case_when.result, refs);
            }
            if let Some(el) = else_result {
                collect_expr_table_refs(el, refs);
            }
        }
        Expr::Cast { expr: inner, .. } => {
            collect_expr_table_refs(inner, refs);
        }
        Expr::Function(func) => {
            if let FunctionArguments::List(arg_list) = &func.args {
                for arg in &arg_list.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(e),
                            ..
                        } => collect_expr_table_refs(e, refs),
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
}

fn collect_relation_refs(relation: &TableFactor, refs: &mut HashSet<String>) {
    match relation {
        TableFactor::Table { name, .. } => {
            // Use the last part of the name (table name) for CTE matching
            if let Some(part) = name.0.last() {
                let value = part
                    .as_ident()
                    .map(|ident| ident.value.clone())
                    .unwrap_or_else(|| part.to_string());
                refs.insert(value.to_uppercase());
            }
        }
        TableFactor::Derived { subquery, .. } => {
            collect_query_refs(subquery, refs);
            if let Some(w) = &subquery.with {
                for cte in &w.cte_tables {
                    collect_query_refs(&cte.query, refs);
                }
            }
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_relation_refs(&table_with_joins.relation, refs);
            for join in &table_with_joins.joins {
                collect_relation_refs(&join.relation, refs);
                collect_join_constraint_refs(&join.join_operator, refs);
            }
        }
        _ => {}
    }
}

fn collect_order_by_refs(order_by: &OrderBy, refs: &mut HashSet<String>) {
    if let OrderByKind::Expressions(order_exprs) = &order_by.kind {
        for order_expr in order_exprs {
            collect_expr_table_refs(&order_expr.expr, refs);
        }
    }
}

fn collect_join_constraint_refs(join_operator: &JoinOperator, refs: &mut HashSet<String>) {
    let constraint = match join_operator {
        JoinOperator::Join(c)
        | JoinOperator::Inner(c)
        | JoinOperator::LeftOuter(c)
        | JoinOperator::RightOuter(c)
        | JoinOperator::FullOuter(c)
        | JoinOperator::LeftSemi(c)
        | JoinOperator::RightSemi(c)
        | JoinOperator::LeftAnti(c)
        | JoinOperator::RightAnti(c) => c,
        _ => return,
    };
    if let JoinConstraint::On(expr) = constraint {
        collect_expr_table_refs(expr, refs);
    }
}

fn find_cte_name_span(stmt_sql: &str, name: &str, ctx: &LintContext) -> Option<crate::types::Span> {
    use crate::analyzer::helpers::find_cte_definition_span;
    find_cte_definition_span(stmt_sql, name, 0)
        .map(|s| ctx.span_from_statement_offset(s.start, s.end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = UnusedCte;
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
    fn test_unused_cte_detected() {
        let issues = check_sql("WITH unused AS (SELECT 1) SELECT 2");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_ST_001");
        assert!(issues[0].message.contains("unused"));
    }

    #[test]
    fn test_used_cte_ok() {
        let issues = check_sql("WITH my_cte AS (SELECT 1) SELECT * FROM my_cte");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_cte_referenced_by_later_cte() {
        let issues = check_sql("WITH a AS (SELECT 1), b AS (SELECT * FROM a) SELECT * FROM b");
        assert!(issues.is_empty());
    }

    // --- Edge cases adopted from sqlfluff ST03 (structure.unused_cte) ---

    #[test]
    fn test_no_cte_ok() {
        let issues = check_sql("SELECT * FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_multiple_ctes_all_used() {
        let issues = check_sql(
            "WITH cte1 AS (SELECT a FROM t), cte2 AS (SELECT b FROM t) \
             SELECT cte1.a, cte2.b FROM cte1 JOIN cte2 ON cte1.a = cte2.b",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_multiple_ctes_one_unused() {
        let issues = check_sql(
            "WITH cte1 AS (SELECT a FROM t), cte2 AS (SELECT b FROM t), cte3 AS (SELECT c FROM t) \
             SELECT * FROM cte1 JOIN cte3 ON cte1.a = cte3.c",
        );
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("cte2"));
    }

    #[test]
    fn test_cte_used_in_subquery() {
        let issues = check_sql(
            "WITH cte AS (SELECT id FROM t) \
             SELECT * FROM t2 WHERE id IN (SELECT id FROM cte)",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_cte_used_in_exists_subquery() {
        let issues = check_sql(
            "WITH cte AS (SELECT id FROM t) \
             SELECT 1 WHERE EXISTS (SELECT 1 FROM cte)",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_cte_in_insert() {
        let issues = check_sql("INSERT INTO target WITH unused AS (SELECT 1) SELECT 2");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_cte_in_create_view() {
        let issues = check_sql("CREATE VIEW v AS WITH unused AS (SELECT 1) SELECT 2");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_chained_ctes_three_levels() {
        let issues = check_sql(
            "WITH a AS (SELECT 1), b AS (SELECT * FROM a), c AS (SELECT * FROM b) \
             SELECT * FROM c",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_cte_case_insensitive() {
        let issues = check_sql("WITH My_Cte AS (SELECT 1) SELECT * FROM my_cte");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_cte_used_in_join() {
        let issues = check_sql(
            "WITH cte AS (SELECT id FROM t) \
             SELECT * FROM t2 JOIN cte ON t2.id = cte.id",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_all_ctes_unused() {
        let issues = check_sql("WITH a AS (SELECT 1), b AS (SELECT 2) SELECT 3");
        assert_eq!(issues.len(), 2);
    }
}
