//! LINT_ST_003: Unused CTE.
//!
//! A CTE (WITH clause) is defined but never referenced in the query body
//! or subsequent CTEs. This is likely dead code.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;
use std::collections::HashSet;

pub struct UnusedCte;

impl BuiltinLintRule for UnusedCte {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_003
    }

    fn name(&self) -> &'static str {
        "Unused CTE"
    }

    fn description(&self) -> &'static str {
        "Query defines a CTE (common-table expression) but does not use it."
    }

    fn check_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let query = match stmt {
            Statement::Query(q) => q,
            Statement::Insert(ins) => {
                if let Some(ref source) = ins.source {
                    source
                } else {
                    return Vec::new();
                }
            }
            Statement::CreateView(CreateView { query, .. }) => query,
            Statement::CreateTable(create) => {
                if let Some(ref q) = create.query {
                    q
                } else {
                    return Vec::new();
                }
            }
            Statement::Delete(delete) => {
                let mut issues = Vec::new();
                check_delete_for_nested_ctes(delete, ctx, &mut issues);
                return issues;
            }
            _ => return Vec::new(),
        };

        let mut issues = Vec::new();
        check_query_unused_ctes(query, ctx, &mut issues);
        issues
    }
}

/// Checks a query for unused CTEs, including nested WITH clauses inside CTE
/// bodies.
fn check_query_unused_ctes(query: &Query, ctx: &RuleContext, issues: &mut Vec<Issue>) {
    let with = match &query.with {
        Some(w) => w,
        None => {
            // Even without a top-level WITH, the body may contain nested CTEs.
            check_set_expr_for_nested_ctes(&query.body, ctx, issues);
            return;
        }
    };

    // Collect table references from the query body only (the body's own
    // references, not inner CTE definitions which are a separate scope).
    let mut referenced = HashSet::new();
    collect_table_refs(&query.body, &mut referenced);
    if let Some(order_by) = &query.order_by {
        collect_order_by_refs(order_by, &mut referenced);
    }

    // Each CTE can reference earlier CTEs; collect those refs too.
    for (i, cte) in with.cte_tables.iter().enumerate() {
        let mut cte_refs = HashSet::new();
        collect_query_refs(&cte.query, &mut cte_refs);
        for later_cte in &with.cte_tables[i + 1..] {
            collect_query_refs(&later_cte.query, &mut cte_refs);
        }
        referenced.extend(cte_refs);
    }

    for (i, cte) in with.cte_tables.iter().enumerate() {
        let name_upper = cte.alias.name.value.to_uppercase();
        if !referenced.contains(&name_upper) {
            let referenced_by_later = with.cte_tables[i + 1..].iter().any(|later| {
                let mut refs = HashSet::new();
                collect_query_refs(&later.query, &mut refs);
                refs.contains(&name_upper)
            });
            if referenced_by_later {
                continue;
            }

            let span = find_cte_name_span(&cte.alias.name, ctx);
            let mut issue = Issue::warning(
                issue_codes::LINT_ST_003,
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

        // Recursively check nested CTEs inside this CTE's body.
        check_query_unused_ctes(&cte.query, ctx, issues);
    }

    // Also check nested CTEs in the main body (e.g. subqueries with WITH).
    check_set_expr_for_nested_ctes(&query.body, ctx, issues);
}

/// Walks a set expression looking for nested queries that might contain WITH
/// clauses to check.
fn check_set_expr_for_nested_ctes(expr: &SetExpr, ctx: &RuleContext, issues: &mut Vec<Issue>) {
    match expr {
        SetExpr::Select(select) => {
            for item in &select.from {
                check_relation_for_nested_ctes(&item.relation, ctx, issues);
                for join in &item.joins {
                    check_relation_for_nested_ctes(&join.relation, ctx, issues);
                }
            }
            // Check subqueries in projections and predicates.
            for item in &select.projection {
                if let SelectItem::UnnamedExpr(e) | SelectItem::ExprWithAlias { expr: e, .. } = item
                {
                    check_expr_for_nested_ctes(e, ctx, issues);
                }
            }
            if let Some(sel) = &select.selection {
                check_expr_for_nested_ctes(sel, ctx, issues);
            }
        }
        SetExpr::Query(q) => check_query_unused_ctes(q, ctx, issues),
        SetExpr::SetOperation { left, right, .. } => {
            check_set_expr_for_nested_ctes(left, ctx, issues);
            check_set_expr_for_nested_ctes(right, ctx, issues);
        }
        _ => {}
    }
}

/// Checks a DELETE statement for CTEs inside USING and FROM subqueries.
fn check_delete_for_nested_ctes(delete: &Delete, ctx: &RuleContext, issues: &mut Vec<Issue>) {
    if let Some(using) = &delete.using {
        for twj in using {
            check_relation_for_nested_ctes(&twj.relation, ctx, issues);
            for join in &twj.joins {
                check_relation_for_nested_ctes(&join.relation, ctx, issues);
            }
        }
    }
    let from_tables = match &delete.from {
        FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => tables,
    };
    for twj in from_tables {
        check_relation_for_nested_ctes(&twj.relation, ctx, issues);
        for join in &twj.joins {
            check_relation_for_nested_ctes(&join.relation, ctx, issues);
        }
    }
}

fn check_relation_for_nested_ctes(
    relation: &TableFactor,
    ctx: &RuleContext,
    issues: &mut Vec<Issue>,
) {
    if let TableFactor::Derived { subquery, .. } = relation {
        check_query_unused_ctes(subquery, ctx, issues);
    }
}

fn check_expr_for_nested_ctes(expr: &Expr, ctx: &RuleContext, issues: &mut Vec<Issue>) {
    match expr {
        Expr::Subquery(q) | Expr::Exists { subquery: q, .. } => {
            check_query_unused_ctes(q, ctx, issues);
        }
        Expr::InSubquery { subquery, expr, .. } => {
            check_query_unused_ctes(subquery, ctx, issues);
            check_expr_for_nested_ctes(expr, ctx, issues);
        }
        Expr::BinaryOp { left, right, .. } => {
            check_expr_for_nested_ctes(left, ctx, issues);
            check_expr_for_nested_ctes(right, ctx, issues);
        }
        Expr::Nested(inner) => check_expr_for_nested_ctes(inner, ctx, issues),
        _ => {}
    }
}

/// Collects all table references from a query, including nested CTE bodies.
fn collect_query_refs(query: &Query, refs: &mut HashSet<String>) {
    if let Some(w) = &query.with {
        for cte in &w.cte_tables {
            collect_query_refs(&cte.query, refs);
        }
    }
    collect_table_refs(&query.body, refs);
    if let Some(order_by) = &query.order_by {
        collect_order_by_refs(order_by, refs);
    }
}

fn collect_statement_refs(stmt: &Statement, refs: &mut HashSet<String>) {
    match stmt {
        Statement::Query(query) => collect_query_refs(query, refs),
        Statement::Insert(insert) => {
            if let Some(source) = &insert.source {
                collect_query_refs(source, refs);
            }
        }
        Statement::CreateView(CreateView { query, .. }) => collect_query_refs(query, refs),
        Statement::CreateTable(create) => {
            if let Some(query) = &create.query {
                collect_query_refs(query, refs);
            }
        }
        Statement::Update(Update {
            table,
            from,
            selection,
            ..
        }) => {
            collect_relation_refs(&table.relation, refs);
            for join in &table.joins {
                collect_relation_refs(&join.relation, refs);
                collect_join_constraint_refs(&join.join_operator, refs);
            }
            if let Some(from_kind) = from {
                let tables = match from_kind {
                    UpdateTableFromKind::BeforeSet(t) | UpdateTableFromKind::AfterSet(t) => t,
                };
                for twj in tables {
                    collect_relation_refs(&twj.relation, refs);
                    for join in &twj.joins {
                        collect_relation_refs(&join.relation, refs);
                        collect_join_constraint_refs(&join.join_operator, refs);
                    }
                }
            }
            if let Some(sel) = selection {
                collect_expr_table_refs(sel, refs);
            }
        }
        Statement::Delete(delete) => {
            if let Some(using) = &delete.using {
                for twj in using {
                    collect_relation_refs(&twj.relation, refs);
                    for join in &twj.joins {
                        collect_relation_refs(&join.relation, refs);
                        collect_join_constraint_refs(&join.join_operator, refs);
                    }
                }
            }
            if let Some(sel) = &delete.selection {
                collect_expr_table_refs(sel, refs);
            }
        }
        _ => {}
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
        SetExpr::Insert(stmt)
        | SetExpr::Update(stmt)
        | SetExpr::Delete(stmt)
        | SetExpr::Merge(stmt) => {
            collect_statement_refs(stmt, refs);
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

fn find_cte_name_span(name: &Ident, ctx: &RuleContext) -> Option<crate::types::Span> {
    ident_span_in_statement(name, ctx)
}

fn ident_span_in_statement(name: &Ident, ctx: &RuleContext) -> Option<crate::types::Span> {
    use crate::analyzer::helpers::line_col_to_offset;

    let start = line_col_to_offset(
        ctx.sql,
        name.span.start.line as usize,
        name.span.start.column as usize,
    )?;
    let end = line_col_to_offset(
        ctx.sql,
        name.span.end.line as usize,
        name.span.end.column as usize,
    )?;

    if start >= end {
        return None;
    }

    if start < ctx.statement_range.start || end > ctx.statement_range.end {
        return None;
    }

    Some(crate::types::Span::new(start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = UnusedCte;
        let ctx = RuleContext::new(sql, 0..sql.len(), 0);
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check_with_context(stmt, &ctx));
        }
        issues
    }

    #[test]
    fn test_unused_cte_detected() {
        let issues = check_sql("WITH unused AS (SELECT 1) SELECT 2");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_ST_003");
        assert!(issues[0].message.contains("unused"));
    }

    #[test]
    fn test_unused_cte_span_matches_cte_name() {
        let sql = "WITH unused AS (SELECT 1) SELECT 2";
        let issues = check_sql(sql);
        let span = issues[0].span.expect("span");
        assert_eq!(&sql[span.start..span.end], "unused");
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
    fn test_with_insert_ctes_used_ok() {
        let issues = check_sql(
            "WITH a AS (SELECT 1), b AS (SELECT * FROM a) \
             INSERT INTO target SELECT * FROM b",
        );
        assert!(
            issues.is_empty(),
            "expected no unused CTEs, got: {issues:#?}"
        );
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

    #[test]
    fn test_update_cte_used_in_from() {
        // SQLFluff: test_pass_update_cte
        let sql = "\
            WITH cte AS (SELECT id, name, description FROM table1) \
            UPDATE table2 SET name = cte.name, description = cte.description \
            FROM cte WHERE table2.id = cte.id";
        assert!(check_sql(sql).is_empty());
    }

    #[test]
    fn test_nested_cte_unused() {
        // SQLFluff: test_fail_nested_cte
        let sql = "WITH a AS (WITH b AS (SELECT 1 FROM foo) SELECT 1) SELECT * FROM a";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("b"));
    }

    #[test]
    fn test_nested_with_cte_used() {
        // SQLFluff: test_pass_nested_with_cte
        let sql = "\
            WITH example_cte AS (SELECT 1), \
            container_cte AS (\
                WITH nested_cte AS (SELECT * FROM example_cte) \
                SELECT * FROM nested_cte\
            ) SELECT * FROM container_cte";
        assert!(check_sql(sql).is_empty());
    }

    #[test]
    fn test_snowflake_delete_cte() {
        // SQLFluff: test_snowflake_delete_cte
        // CTE inside a derived table (USING subquery) is unused.
        let sql = "\
            DELETE FROM MYTABLE1 \
            USING (\
                WITH MYCTE AS (SELECT COLUMN2 FROM MYTABLE3) \
                SELECT COLUMN3 FROM MYTABLE3\
            ) X \
            WHERE COLUMN1 = X.COLUMN3";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.to_uppercase().contains("MYCTE"));
    }
}
