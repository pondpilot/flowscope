//! LINT_AM_003: Ambiguous ORDER BY direction.
//!
//! SQLFluff AM03 parity: if any ORDER BY item specifies ASC/DESC, all should.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{
    Expr, FunctionArg, FunctionArgExpr, FunctionArguments, OrderByKind, Query, Select, SetExpr,
    Statement, TableFactor, WindowType,
};

use super::semantic_helpers::join_on_expr;

pub struct AmbiguousOrderBy;

impl LintRule for AmbiguousOrderBy {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_003
    }

    fn name(&self) -> &'static str {
        "Ambiguous ORDER BY"
    }

    fn description(&self) -> &'static str {
        "ORDER BY direction should be either explicit for all items or omitted for all items."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violation_count = 0usize;
        check_statement(statement, &mut violation_count);

        (0..violation_count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_AM_003,
                    "Ambiguous ORDER BY clause. Specify ASC/DESC for all columns or none.",
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

    if order_by_mixes_explicit_and_implicit_direction(query) {
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

fn order_by_mixes_explicit_and_implicit_direction(query: &Query) -> bool {
    let Some(order_by) = &query.order_by else {
        return false;
    };

    let OrderByKind::Expressions(order_exprs) = &order_by.kind else {
        return false;
    };

    let mut has_explicit = false;
    let mut has_implicit = false;

    for order_expr in order_exprs {
        if order_expr.options.asc.is_some() {
            has_explicit = true;
        } else {
            has_implicit = true;
        }
    }

    has_explicit && has_implicit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AmbiguousOrderBy;
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

    // --- Edge cases adopted from sqlfluff AM03 ---

    #[test]
    fn allows_unspecified_single_order_item() {
        let issues = run("SELECT * FROM t ORDER BY a");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_unspecified_all_order_items() {
        let issues = run("SELECT * FROM t ORDER BY a, b");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_all_explicit_order_items() {
        let issues = run("SELECT * FROM t ORDER BY a ASC, b DESC");
        assert!(issues.is_empty());

        let issues = run("SELECT * FROM t ORDER BY a DESC, b ASC");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_mixed_implicit_and_explicit_order_items() {
        let issues = run("SELECT * FROM t ORDER BY a, b DESC");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_003);

        let issues = run("SELECT * FROM t ORDER BY a DESC, b");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_nulls_clause_without_explicit_direction_when_mixed() {
        let issues = run("SELECT * FROM t ORDER BY a DESC, b NULLS LAST");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_consistent_order_by_with_comments() {
        let issues = run("SELECT * FROM t ORDER BY a /* Comment */ DESC, b ASC");
        assert!(issues.is_empty());
    }
}
