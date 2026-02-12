//! Expression visitor utilities for lint rules.
//!
//! Provides reusable visitor functions that walk the AST and invoke a callback
//! on each expression. This avoids duplicating the traversal logic in every rule.

use sqlparser::ast::*;

/// Visits all expressions in a statement, calling the visitor for each one.
pub fn visit_expressions<F: FnMut(&Expr)>(stmt: &Statement, visitor: &mut F) {
    match stmt {
        Statement::Query(q) => visit_query_expressions(q, visitor),
        Statement::Insert(ins) => {
            if let Some(ref source) = ins.source {
                visit_query_expressions(source, visitor);
            }
        }
        Statement::CreateView { query, .. } => visit_query_expressions(query, visitor),
        Statement::CreateTable(create) => {
            if let Some(ref q) = create.query {
                visit_query_expressions(q, visitor);
            }
        }
        _ => {}
    }
}

pub fn visit_query_expressions<F: FnMut(&Expr)>(query: &Query, visitor: &mut F) {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            visit_query_expressions(&cte.query, visitor);
        }
    }
    visit_set_expr_expressions(&query.body, visitor);

    // ORDER BY expressions
    if let Some(ref order_by) = query.order_by {
        if let OrderByKind::Expressions(exprs) = &order_by.kind {
            for order_expr in exprs {
                visit_expr(&order_expr.expr, visitor);
            }
        }
    }
}

pub fn visit_set_expr_expressions<F: FnMut(&Expr)>(body: &SetExpr, visitor: &mut F) {
    match body {
        SetExpr::Select(select) => {
            for item in &select.projection {
                if let SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } = item
                {
                    visit_expr(expr, visitor);
                }
            }
            if let Some(ref selection) = select.selection {
                visit_expr(selection, visitor);
            }
            if let Some(ref having) = select.having {
                visit_expr(having, visitor);
            }
            if let Some(ref qualify) = select.qualify {
                visit_expr(qualify, visitor);
            }

            // GROUP BY expressions
            if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
                for expr in exprs {
                    visit_expr(expr, visitor);
                }
            }

            // JOIN ON expressions
            for table_with_joins in &select.from {
                for join in &table_with_joins.joins {
                    visit_join_constraint(&join.join_operator, visitor);
                }
                // Derived table subqueries
                if let TableFactor::Derived { subquery, .. } = &table_with_joins.relation {
                    visit_query_expressions(subquery, visitor);
                }
                for join in &table_with_joins.joins {
                    if let TableFactor::Derived { subquery, .. } = &join.relation {
                        visit_query_expressions(subquery, visitor);
                    }
                }
            }
        }
        SetExpr::Query(q) => visit_query_expressions(q, visitor),
        SetExpr::SetOperation { left, right, .. } => {
            visit_set_expr_expressions(left, visitor);
            visit_set_expr_expressions(right, visitor);
        }
        _ => {}
    }
}

/// Recursively visits an expression and all its children.
pub fn visit_expr<F: FnMut(&Expr)>(expr: &Expr, visitor: &mut F) {
    visitor(expr);
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            visit_expr(left, visitor);
            visit_expr(right, visitor);
        }
        Expr::UnaryOp { expr: inner, .. } => visit_expr(inner, visitor),
        Expr::Nested(inner) => visit_expr(inner, visitor),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                visit_expr(op, visitor);
            }
            for case_when in conditions {
                visit_expr(&case_when.condition, visitor);
                visit_expr(&case_when.result, visitor);
            }
            if let Some(el) = else_result {
                visit_expr(el, visitor);
            }
        }
        Expr::Function(func) => {
            if let FunctionArguments::List(arg_list) = &func.args {
                for arg in &arg_list.args {
                    if let FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) = arg {
                        visit_expr(e, visitor);
                    }
                }
            }
        }
        Expr::Cast { expr: inner, .. } => visit_expr(inner, visitor),
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => {
            visit_expr(inner, visitor);
            visit_query_expressions(subquery, visitor);
        }
        Expr::Subquery(subquery) => {
            visit_query_expressions(subquery, visitor);
        }
        Expr::Exists { subquery, .. } => {
            visit_query_expressions(subquery, visitor);
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            visit_expr(expr, visitor);
            visit_expr(low, visitor);
            visit_expr(high, visitor);
        }
        Expr::IsNull(inner) | Expr::IsNotNull(inner) => visit_expr(inner, visitor),
        Expr::InList { expr, list, .. } => {
            visit_expr(expr, visitor);
            for item in list {
                visit_expr(item, visitor);
            }
        }
        _ => {}
    }
}

/// Visits the expression inside a JOIN constraint (ON clause).
fn visit_join_constraint<F: FnMut(&Expr)>(op: &JoinOperator, visitor: &mut F) {
    let constraint = match op {
        JoinOperator::Join(c)
        | JoinOperator::Inner(c)
        | JoinOperator::Left(c)
        | JoinOperator::LeftOuter(c)
        | JoinOperator::Right(c)
        | JoinOperator::RightOuter(c)
        | JoinOperator::FullOuter(c)
        | JoinOperator::CrossJoin(c)
        | JoinOperator::Semi(c)
        | JoinOperator::LeftSemi(c)
        | JoinOperator::RightSemi(c)
        | JoinOperator::Anti(c)
        | JoinOperator::LeftAnti(c)
        | JoinOperator::RightAnti(c)
        | JoinOperator::StraightJoin(c) => c,
        JoinOperator::AsOf { constraint, .. } => constraint,
        JoinOperator::CrossApply | JoinOperator::OuterApply => return,
    };
    if let JoinConstraint::On(expr) = constraint {
        visit_expr(expr, visitor);
    }
}
