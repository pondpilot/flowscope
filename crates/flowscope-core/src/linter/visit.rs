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
            for assignment in &ins.assignments {
                visit_expr(&assignment.value, visitor);
            }
            if let Some(partitioned) = &ins.partitioned {
                for expr in partitioned {
                    visit_expr(expr, visitor);
                }
            }
            if let Some(returning) = &ins.returning {
                for item in returning {
                    visit_select_item_expressions(item, visitor);
                }
            }
        }
        Statement::Update {
            table,
            assignments,
            from,
            selection,
            returning,
            limit,
            ..
        } => {
            visit_table_with_joins_expressions(table, visitor);
            for assignment in assignments {
                visit_expr(&assignment.value, visitor);
            }
            if let Some(from) = from {
                match from {
                    UpdateTableFromKind::BeforeSet(tables)
                    | UpdateTableFromKind::AfterSet(tables) => {
                        for table in tables {
                            visit_table_with_joins_expressions(table, visitor);
                        }
                    }
                }
            }
            if let Some(selection) = selection {
                visit_expr(selection, visitor);
            }
            if let Some(returning) = returning {
                for item in returning {
                    visit_select_item_expressions(item, visitor);
                }
            }
            if let Some(limit) = limit {
                visit_expr(limit, visitor);
            }
        }
        Statement::Delete(delete) => {
            match &delete.from {
                FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => {
                    for table in tables {
                        visit_table_with_joins_expressions(table, visitor);
                    }
                }
            }
            if let Some(using) = &delete.using {
                for table in using {
                    visit_table_with_joins_expressions(table, visitor);
                }
            }
            if let Some(selection) = &delete.selection {
                visit_expr(selection, visitor);
            }
            if let Some(returning) = &delete.returning {
                for item in returning {
                    visit_select_item_expressions(item, visitor);
                }
            }
            for order_by_expr in &delete.order_by {
                visit_expr(&order_by_expr.expr, visitor);
            }
            if let Some(limit) = &delete.limit {
                visit_expr(limit, visitor);
            }
        }
        Statement::Merge {
            table,
            source,
            on,
            clauses,
            output,
            ..
        } => {
            visit_table_factor_expressions(table, visitor);
            visit_table_factor_expressions(source, visitor);
            visit_expr(on, visitor);
            for clause in clauses {
                if let Some(predicate) = &clause.predicate {
                    visit_expr(predicate, visitor);
                }
                match &clause.action {
                    MergeAction::Insert(insert) => {
                        if let MergeInsertKind::Values(values) = &insert.kind {
                            for row in &values.rows {
                                for expr in row {
                                    visit_expr(expr, visitor);
                                }
                            }
                        }
                    }
                    MergeAction::Update { assignments } => {
                        for assignment in assignments {
                            visit_expr(&assignment.value, visitor);
                        }
                    }
                    MergeAction::Delete => {}
                }
            }
            if let Some(output) = output {
                match output {
                    OutputClause::Output { select_items, .. }
                    | OutputClause::Returning { select_items } => {
                        for item in select_items {
                            visit_select_item_expressions(item, visitor);
                        }
                    }
                }
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
        SetExpr::Values(values) => {
            for row in &values.rows {
                for expr in row {
                    visit_expr(expr, visitor);
                }
            }
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => visit_expressions(statement, visitor),
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
        Expr::Function(func) => match &func.args {
            FunctionArguments::Subquery(query) => visit_query_expressions(query, visitor),
            FunctionArguments::List(arg_list) => {
                for arg in &arg_list.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(expr),
                            ..
                        } => visit_expr(expr, visitor),
                        FunctionArg::ExprNamed { name, arg, .. } => {
                            visit_expr(name, visitor);
                            if let FunctionArgExpr::Expr(expr) = arg {
                                visit_expr(expr, visitor);
                            }
                        }
                        _ => {}
                    }
                }
                for clause in &arg_list.clauses {
                    match clause {
                        FunctionArgumentClause::OrderBy(order_by_exprs) => {
                            for order_by_expr in order_by_exprs {
                                visit_expr(&order_by_expr.expr, visitor);
                            }
                        }
                        FunctionArgumentClause::Limit(expr) => visit_expr(expr, visitor),
                        FunctionArgumentClause::Having(HavingBound(_, expr)) => {
                            visit_expr(expr, visitor)
                        }
                        _ => {}
                    }
                }
            }
            FunctionArguments::None => {}
        },
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

fn visit_select_item_expressions<F: FnMut(&Expr)>(item: &SelectItem, visitor: &mut F) {
    if let SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } = item {
        visit_expr(expr, visitor);
    }
}

fn visit_table_with_joins_expressions<F: FnMut(&Expr)>(table: &TableWithJoins, visitor: &mut F) {
    visit_table_factor_expressions(&table.relation, visitor);
    for join in &table.joins {
        visit_join_constraint(&join.join_operator, visitor);
        visit_table_factor_expressions(&join.relation, visitor);
    }
}

fn visit_table_factor_expressions<F: FnMut(&Expr)>(table_factor: &TableFactor, visitor: &mut F) {
    match table_factor {
        TableFactor::Derived { subquery, .. } => visit_query_expressions(subquery, visitor),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => visit_table_with_joins_expressions(table_with_joins, visitor),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            visit_table_factor_expressions(table, visitor)
        }
        _ => {}
    }
}
