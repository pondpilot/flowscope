//! LINT_AM_009: LIMIT/OFFSET without ORDER BY.
//!
//! SQLFluff AM09 parity: use of LIMIT/OFFSET without ORDER BY may lead to
//! non-deterministic results.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{
    Expr, FunctionArg, FunctionArgExpr, FunctionArguments, LimitClause, OrderByKind, Query, Select,
    SetExpr, Statement, TableFactor, WindowType,
};

use super::semantic_helpers::join_on_expr;

pub struct LimitOffsetWithoutOrderBy;

impl LintRule for LimitOffsetWithoutOrderBy {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_009
    }

    fn name(&self) -> &'static str {
        "LIMIT/OFFSET without ORDER BY"
    }

    fn description(&self) -> &'static str {
        "Using LIMIT/OFFSET without ORDER BY may lead to non-deterministic results."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violation_count = 0usize;
        check_statement(statement, &mut violation_count);

        (0..violation_count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_AM_009,
                    "LIMIT/OFFSET used without ORDER BY may lead to non-deterministic results.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn check_statement(statement: &Statement, violations: &mut usize) {
    match statement {
        Statement::Query(query) => check_query(query, violations),
        Statement::Insert(insert) => {
            if let Some(source) = &insert.source {
                check_query(source, violations);
            }
        }
        Statement::CreateView { query, .. } => check_query(query, violations),
        Statement::CreateTable(create) => {
            if let Some(query) = &create.query {
                check_query(query, violations);
            }
        }
        _ => {}
    }
}

fn check_query(query: &Query, violations: &mut usize) {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            check_query(&cte.query, violations);
        }
    }

    check_set_expr(&query.body, violations);

    if query_has_limit_or_offset(query) && !query_has_order_by(query) {
        *violations += 1;
    }
}

fn check_set_expr(set_expr: &SetExpr, violations: &mut usize) {
    match set_expr {
        SetExpr::Select(select) => check_select(select, violations),
        SetExpr::Query(query) => check_query(query, violations),
        SetExpr::SetOperation { left, right, .. } => {
            check_set_expr(left, violations);
            check_set_expr(right, violations);
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => check_statement(statement, violations),
        _ => {}
    }
}

fn check_select(select: &Select, violations: &mut usize) {
    for table in &select.from {
        check_table_factor(&table.relation, violations);
        for join in &table.joins {
            check_table_factor(&join.relation, violations);
            if let Some(on_expr) = join_on_expr(&join.join_operator) {
                check_expr_for_subqueries(on_expr, violations);
            }
        }
    }

    for item in &select.projection {
        if let sqlparser::ast::SelectItem::UnnamedExpr(expr)
        | sqlparser::ast::SelectItem::ExprWithAlias { expr, .. } = item
        {
            check_expr_for_subqueries(expr, violations);
        }
    }

    if let Some(prewhere) = &select.prewhere {
        check_expr_for_subqueries(prewhere, violations);
    }

    if let Some(selection) = &select.selection {
        check_expr_for_subqueries(selection, violations);
    }

    if let sqlparser::ast::GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            check_expr_for_subqueries(expr, violations);
        }
    }

    if let Some(having) = &select.having {
        check_expr_for_subqueries(having, violations);
    }

    if let Some(qualify) = &select.qualify {
        check_expr_for_subqueries(qualify, violations);
    }

    for order_expr in &select.sort_by {
        check_expr_for_subqueries(&order_expr.expr, violations);
    }
}

fn check_table_factor(table_factor: &TableFactor, violations: &mut usize) {
    match table_factor {
        TableFactor::Derived { subquery, .. } => check_query(subquery, violations),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            check_table_factor(&table_with_joins.relation, violations);
            for join in &table_with_joins.joins {
                check_table_factor(&join.relation, violations);
                if let Some(on_expr) = join_on_expr(&join.join_operator) {
                    check_expr_for_subqueries(on_expr, violations);
                }
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => check_table_factor(table, violations),
        _ => {}
    }
}

fn check_expr_for_subqueries(expr: &Expr, violations: &mut usize) {
    match expr {
        Expr::Subquery(query)
        | Expr::Exists {
            subquery: query, ..
        } => check_query(query, violations),
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => {
            check_expr_for_subqueries(inner, violations);
            check_query(subquery, violations);
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. } => {
            check_expr_for_subqueries(left, violations);
            check_expr_for_subqueries(right, violations);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => check_expr_for_subqueries(inner, violations),
        Expr::InList { expr, list, .. } => {
            check_expr_for_subqueries(expr, violations);
            for item in list {
                check_expr_for_subqueries(item, violations);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            check_expr_for_subqueries(expr, violations);
            check_expr_for_subqueries(low, violations);
            check_expr_for_subqueries(high, violations);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand {
                check_expr_for_subqueries(operand, violations);
            }
            for when in conditions {
                check_expr_for_subqueries(&when.condition, violations);
                check_expr_for_subqueries(&when.result, violations);
            }
            if let Some(otherwise) = else_result {
                check_expr_for_subqueries(otherwise, violations);
            }
        }
        Expr::Function(function) => {
            if let FunctionArguments::List(arguments) = &function.args {
                for arg in &arguments.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(expr),
                            ..
                        } => check_expr_for_subqueries(expr, violations),
                        _ => {}
                    }
                }
            }

            if let Some(filter) = &function.filter {
                check_expr_for_subqueries(filter, violations);
            }

            for order_expr in &function.within_group {
                check_expr_for_subqueries(&order_expr.expr, violations);
            }

            if let Some(WindowType::WindowSpec(spec)) = &function.over {
                for expr in &spec.partition_by {
                    check_expr_for_subqueries(expr, violations);
                }
                for order_expr in &spec.order_by {
                    check_expr_for_subqueries(&order_expr.expr, violations);
                }
            }
        }
        _ => {}
    }
}

fn query_has_order_by(query: &Query) -> bool {
    let Some(order_by) = &query.order_by else {
        return false;
    };

    match &order_by.kind {
        OrderByKind::Expressions(order_exprs) => !order_exprs.is_empty(),
        OrderByKind::All(_) => true,
    }
}

fn query_has_limit_or_offset(query: &Query) -> bool {
    match &query.limit_clause {
        Some(LimitClause::LimitOffset { limit, offset, .. }) => limit.is_some() || offset.is_some(),
        Some(LimitClause::OffsetCommaLimit { .. }) => true,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LimitOffsetWithoutOrderBy;
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

    // --- Edge cases adopted from sqlfluff AM09 ---

    #[test]
    fn fails_limit_without_order_by() {
        let issues = run("SELECT * FROM foo LIMIT 10");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_009);
    }

    #[test]
    fn fails_limit_and_offset_without_order_by() {
        let issues = run("SELECT * FROM foo LIMIT 10 OFFSET 5");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn passes_limit_with_order_by() {
        let issues = run("SELECT * FROM foo ORDER BY id LIMIT 10");
        assert!(issues.is_empty());
    }

    #[test]
    fn passes_limit_and_offset_with_order_by() {
        let issues = run("SELECT * FROM foo ORDER BY id LIMIT 10 OFFSET 5");
        assert!(issues.is_empty());
    }

    #[test]
    fn passes_without_limit_or_offset() {
        let issues = run("SELECT * FROM foo");
        assert!(issues.is_empty());
    }

    #[test]
    fn fails_limit_in_subquery_without_order_by() {
        let issues = run("SELECT * FROM (SELECT * FROM foo LIMIT 10) subquery");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn passes_limit_in_subquery_with_order_by() {
        let issues = run("SELECT * FROM (SELECT * FROM foo ORDER BY id LIMIT 10) subquery");
        assert!(issues.is_empty());
    }

    #[test]
    fn fails_limit_in_cte_without_order_by() {
        let issues = run("WITH cte AS (SELECT * FROM foo LIMIT 10) SELECT * FROM cte");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn passes_fetch_without_order_by() {
        let issues = run("SELECT * FROM foo FETCH FIRST 10 ROWS ONLY");
        assert!(issues.is_empty());
    }

    #[test]
    fn passes_top_without_order_by() {
        let issues = run("SELECT TOP 10 * FROM foo");
        assert!(issues.is_empty());
    }
}
