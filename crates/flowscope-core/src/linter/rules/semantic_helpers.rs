//! Shared AST traversal utilities for semantic lint rules.

use std::collections::HashSet;

use sqlparser::ast::*;

pub fn visit_selects_in_statement<F: FnMut(&Select)>(statement: &Statement, visitor: &mut F) {
    match statement {
        Statement::Query(query) => visit_selects_in_query(query, visitor),
        Statement::Insert(insert) => {
            if let Some(source) = &insert.source {
                visit_selects_in_query(source, visitor);
            }
        }
        Statement::CreateView { query, .. } => visit_selects_in_query(query, visitor),
        Statement::CreateTable(create) => {
            if let Some(query) = &create.query {
                visit_selects_in_query(query, visitor);
            }
        }
        _ => {}
    }
}

pub fn visit_selects_in_query<F: FnMut(&Select)>(query: &Query, visitor: &mut F) {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            visit_selects_in_query(&cte.query, visitor);
        }
    }

    visit_selects_in_set_expr(&query.body, visitor);
}

pub fn visit_selects_in_set_expr<F: FnMut(&Select)>(set_expr: &SetExpr, visitor: &mut F) {
    match set_expr {
        SetExpr::Select(select) => {
            visitor(select);

            for table in &select.from {
                visit_selects_in_table_with_joins(table, visitor);
            }

            visit_select_expressions(select, &mut |expr| visit_selects_in_expr(expr, visitor));
        }
        SetExpr::Query(query) => visit_selects_in_query(query, visitor),
        SetExpr::SetOperation { left, right, .. } => {
            visit_selects_in_set_expr(left, visitor);
            visit_selects_in_set_expr(right, visitor);
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => visit_selects_in_statement(statement, visitor),
        _ => {}
    }
}

fn visit_selects_in_table_with_joins<F: FnMut(&Select)>(table: &TableWithJoins, visitor: &mut F) {
    visit_selects_in_table_factor(&table.relation, visitor);
    for join in &table.joins {
        visit_selects_in_table_factor(&join.relation, visitor);
        if let Some(on_expr) = join_on_expr(&join.join_operator) {
            visit_selects_in_expr(on_expr, visitor);
        }
    }
}

fn visit_selects_in_table_factor<F: FnMut(&Select)>(table_factor: &TableFactor, visitor: &mut F) {
    match table_factor {
        TableFactor::Derived { subquery, .. } => visit_selects_in_query(subquery, visitor),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => visit_selects_in_table_with_joins(table_with_joins, visitor),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            visit_selects_in_table_factor(table, visitor)
        }
        _ => {}
    }
}

fn visit_selects_in_expr<F: FnMut(&Select)>(expr: &Expr, visitor: &mut F) {
    match expr {
        Expr::Subquery(query)
        | Expr::Exists {
            subquery: query, ..
        } => visit_selects_in_query(query, visitor),
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => {
            visit_selects_in_expr(inner, visitor);
            visit_selects_in_query(subquery, visitor);
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. } => {
            visit_selects_in_expr(left, visitor);
            visit_selects_in_expr(right, visitor);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => visit_selects_in_expr(inner, visitor),
        Expr::InList { expr, list, .. } => {
            visit_selects_in_expr(expr, visitor);
            for item in list {
                visit_selects_in_expr(item, visitor);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            visit_selects_in_expr(expr, visitor);
            visit_selects_in_expr(low, visitor);
            visit_selects_in_expr(high, visitor);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand {
                visit_selects_in_expr(operand, visitor);
            }
            for when in conditions {
                visit_selects_in_expr(&when.condition, visitor);
                visit_selects_in_expr(&when.result, visitor);
            }
            if let Some(otherwise) = else_result {
                visit_selects_in_expr(otherwise, visitor);
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
                        } => visit_selects_in_expr(expr, visitor),
                        _ => {}
                    }
                }
            }

            if let Some(filter) = &function.filter {
                visit_selects_in_expr(filter, visitor);
            }

            for order_expr in &function.within_group {
                visit_selects_in_expr(&order_expr.expr, visitor);
            }

            if let Some(WindowType::WindowSpec(spec)) = &function.over {
                for expr in &spec.partition_by {
                    visit_selects_in_expr(expr, visitor);
                }
                for order_expr in &spec.order_by {
                    visit_selects_in_expr(&order_expr.expr, visitor);
                }
            }
        }
        _ => {}
    }
}

pub fn visit_select_expressions<F: FnMut(&Expr)>(select: &Select, visitor: &mut F) {
    for item in &select.projection {
        if let SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } = item {
            visitor(expr);
        }
    }

    if let Some(prewhere) = &select.prewhere {
        visitor(prewhere);
    }

    if let Some(selection) = &select.selection {
        visitor(selection);
    }

    if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            visitor(expr);
        }
    }

    if let Some(having) = &select.having {
        visitor(having);
    }

    if let Some(qualify) = &select.qualify {
        visitor(qualify);
    }

    for sort in &select.sort_by {
        visitor(&sort.expr);
    }

    for table in &select.from {
        for join in &table.joins {
            if let Some(on_expr) = join_on_expr(&join.join_operator) {
                visitor(on_expr);
            }
        }
    }
}

pub fn join_on_expr(join_operator: &JoinOperator) -> Option<&Expr> {
    let constraint = match join_operator {
        JoinOperator::Join(constraint)
        | JoinOperator::Inner(constraint)
        | JoinOperator::Left(constraint)
        | JoinOperator::LeftOuter(constraint)
        | JoinOperator::Right(constraint)
        | JoinOperator::RightOuter(constraint)
        | JoinOperator::FullOuter(constraint)
        | JoinOperator::CrossJoin(constraint)
        | JoinOperator::Semi(constraint)
        | JoinOperator::LeftSemi(constraint)
        | JoinOperator::RightSemi(constraint)
        | JoinOperator::Anti(constraint)
        | JoinOperator::LeftAnti(constraint)
        | JoinOperator::RightAnti(constraint)
        | JoinOperator::StraightJoin(constraint) => constraint,
        JoinOperator::AsOf { constraint, .. } => constraint,
        JoinOperator::CrossApply | JoinOperator::OuterApply => return None,
    };

    if let JoinConstraint::On(expr) = constraint {
        Some(expr)
    } else {
        None
    }
}

pub fn select_source_count(select: &Select) -> usize {
    let mut count = 0usize;
    for table in &select.from {
        count += 1;
        count += table.joins.len();
    }
    count
}

pub fn table_factor_alias_name(table_factor: &TableFactor) -> Option<&str> {
    let alias = match table_factor {
        TableFactor::Table { alias, .. }
        | TableFactor::Derived { alias, .. }
        | TableFactor::TableFunction { alias, .. }
        | TableFactor::Function { alias, .. }
        | TableFactor::UNNEST { alias, .. }
        | TableFactor::JsonTable { alias, .. }
        | TableFactor::OpenJsonTable { alias, .. }
        | TableFactor::NestedJoin { alias, .. }
        | TableFactor::Pivot { alias, .. }
        | TableFactor::Unpivot { alias, .. }
        | TableFactor::MatchRecognize { alias, .. }
        | TableFactor::XmlTable { alias, .. }
        | TableFactor::SemanticView { alias, .. } => alias.as_ref(),
    }?;

    Some(alias.name.value.as_str())
}

pub fn table_factor_reference_name(table_factor: &TableFactor) -> Option<String> {
    if let Some(alias) = table_factor_alias_name(table_factor) {
        return Some(alias.to_ascii_uppercase());
    }

    match table_factor {
        TableFactor::Table { name, .. } => {
            let full = name.to_string();
            full.rsplit('.')
                .next()
                .map(|last| last.trim_matches('"').to_ascii_uppercase())
        }
        _ => None,
    }
}

pub fn select_projection_alias_set(select: &Select) -> HashSet<String> {
    let mut aliases = HashSet::new();
    for item in &select.projection {
        if let SelectItem::ExprWithAlias { alias, .. } = item {
            aliases.insert(alias.value.to_ascii_uppercase());
        }
    }
    aliases
}

pub fn count_reference_qualification_in_expr_excluding_aliases(
    expr: &Expr,
    aliases: &HashSet<String>,
) -> (usize, usize) {
    match expr {
        Expr::Identifier(identifier) => {
            if aliases.contains(&identifier.value.to_ascii_uppercase()) {
                (0, 0)
            } else {
                (0, 1)
            }
        }
        Expr::CompoundIdentifier(parts) => {
            if parts.len() > 1 {
                (1, 0)
            } else {
                let name = parts
                    .first()
                    .map(|part| part.value.to_ascii_uppercase())
                    .unwrap_or_default();
                if aliases.contains(&name) {
                    (0, 0)
                } else {
                    (0, 1)
                }
            }
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. } => {
            let (left_q, left_u) =
                count_reference_qualification_in_expr_excluding_aliases(left, aliases);
            let (right_q, right_u) =
                count_reference_qualification_in_expr_excluding_aliases(right, aliases);
            (left_q + right_q, left_u + right_u)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            count_reference_qualification_in_expr_excluding_aliases(inner, aliases)
        }
        Expr::InList { expr, list, .. } => {
            let (mut qualified, mut unqualified) =
                count_reference_qualification_in_expr_excluding_aliases(expr, aliases);
            for item in list {
                let (q, u) = count_reference_qualification_in_expr_excluding_aliases(item, aliases);
                qualified += q;
                unqualified += u;
            }
            (qualified, unqualified)
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            let (eq, eu) = count_reference_qualification_in_expr_excluding_aliases(expr, aliases);
            let (lq, lu) = count_reference_qualification_in_expr_excluding_aliases(low, aliases);
            let (hq, hu) = count_reference_qualification_in_expr_excluding_aliases(high, aliases);
            (eq + lq + hq, eu + lu + hu)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let mut qualified = 0usize;
            let mut unqualified = 0usize;

            if let Some(operand) = operand {
                let (q, u) =
                    count_reference_qualification_in_expr_excluding_aliases(operand, aliases);
                qualified += q;
                unqualified += u;
            }

            for when in conditions {
                let (cq, cu) = count_reference_qualification_in_expr_excluding_aliases(
                    &when.condition,
                    aliases,
                );
                let (rq, ru) =
                    count_reference_qualification_in_expr_excluding_aliases(&when.result, aliases);
                qualified += cq + rq;
                unqualified += cu + ru;
            }

            if let Some(otherwise) = else_result {
                let (q, u) =
                    count_reference_qualification_in_expr_excluding_aliases(otherwise, aliases);
                qualified += q;
                unqualified += u;
            }

            (qualified, unqualified)
        }
        Expr::Function(function) => {
            let mut qualified = 0usize;
            let mut unqualified = 0usize;

            if let FunctionArguments::List(arguments) = &function.args {
                for arg in &arguments.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(expr),
                            ..
                        } => {
                            let (q, u) = count_reference_qualification_in_expr_excluding_aliases(
                                expr, aliases,
                            );
                            qualified += q;
                            unqualified += u;
                        }
                        _ => {}
                    }
                }
            }

            if let Some(filter) = &function.filter {
                let (q, u) =
                    count_reference_qualification_in_expr_excluding_aliases(filter, aliases);
                qualified += q;
                unqualified += u;
            }

            for order_expr in &function.within_group {
                let (q, u) = count_reference_qualification_in_expr_excluding_aliases(
                    &order_expr.expr,
                    aliases,
                );
                qualified += q;
                unqualified += u;
            }

            if let Some(WindowType::WindowSpec(spec)) = &function.over {
                for expr in &spec.partition_by {
                    let (q, u) =
                        count_reference_qualification_in_expr_excluding_aliases(expr, aliases);
                    qualified += q;
                    unqualified += u;
                }
                for order_expr in &spec.order_by {
                    let (q, u) = count_reference_qualification_in_expr_excluding_aliases(
                        &order_expr.expr,
                        aliases,
                    );
                    qualified += q;
                    unqualified += u;
                }
            }

            (qualified, unqualified)
        }
        Expr::InSubquery { expr, .. } => {
            count_reference_qualification_in_expr_excluding_aliases(expr, aliases)
        }
        Expr::Exists { .. } | Expr::Subquery(_) => (0, 0),
        _ => (0, 0),
    }
}

pub fn collect_qualifier_prefixes_in_expr(expr: &Expr, prefixes: &mut HashSet<String>) {
    match expr {
        Expr::CompoundIdentifier(parts) if parts.len() > 1 => {
            if let Some(first) = parts.first() {
                prefixes.insert(first.value.to_ascii_uppercase());
            }
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. } => {
            collect_qualifier_prefixes_in_expr(left, prefixes);
            collect_qualifier_prefixes_in_expr(right, prefixes);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => collect_qualifier_prefixes_in_expr(inner, prefixes),
        Expr::InList { expr, list, .. } => {
            collect_qualifier_prefixes_in_expr(expr, prefixes);
            for item in list {
                collect_qualifier_prefixes_in_expr(item, prefixes);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            collect_qualifier_prefixes_in_expr(expr, prefixes);
            collect_qualifier_prefixes_in_expr(low, prefixes);
            collect_qualifier_prefixes_in_expr(high, prefixes);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand {
                collect_qualifier_prefixes_in_expr(operand, prefixes);
            }
            for when in conditions {
                collect_qualifier_prefixes_in_expr(&when.condition, prefixes);
                collect_qualifier_prefixes_in_expr(&when.result, prefixes);
            }
            if let Some(otherwise) = else_result {
                collect_qualifier_prefixes_in_expr(otherwise, prefixes);
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
                        } => collect_qualifier_prefixes_in_expr(expr, prefixes),
                        _ => {}
                    }
                }
            }

            if let Some(filter) = &function.filter {
                collect_qualifier_prefixes_in_expr(filter, prefixes);
            }

            for order_expr in &function.within_group {
                collect_qualifier_prefixes_in_expr(&order_expr.expr, prefixes);
            }

            if let Some(WindowType::WindowSpec(spec)) = &function.over {
                for expr in &spec.partition_by {
                    collect_qualifier_prefixes_in_expr(expr, prefixes);
                }
                for order_expr in &spec.order_by {
                    collect_qualifier_prefixes_in_expr(&order_expr.expr, prefixes);
                }
            }
        }
        Expr::InSubquery { expr, subquery, .. } => {
            collect_qualifier_prefixes_in_expr(expr, prefixes);
            collect_prefixes_in_query(subquery, prefixes);
        }
        Expr::Subquery(query)
        | Expr::Exists {
            subquery: query, ..
        } => collect_prefixes_in_query(query, prefixes),
        _ => {}
    }
}

fn collect_prefixes_in_query(query: &Query, prefixes: &mut HashSet<String>) {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            collect_prefixes_in_query(&cte.query, prefixes);
        }
    }

    collect_prefixes_in_set_expr(&query.body, prefixes);

    if let Some(order_by) = &query.order_by {
        if let OrderByKind::Expressions(order_exprs) = &order_by.kind {
            for expr in order_exprs {
                collect_qualifier_prefixes_in_expr(&expr.expr, prefixes);
            }
        }
    }
}

fn collect_prefixes_in_set_expr(set_expr: &SetExpr, prefixes: &mut HashSet<String>) {
    match set_expr {
        SetExpr::Select(select) => collect_prefixes_in_select(select, prefixes),
        SetExpr::Query(query) => collect_prefixes_in_query(query, prefixes),
        SetExpr::SetOperation { left, right, .. } => {
            collect_prefixes_in_set_expr(left, prefixes);
            collect_prefixes_in_set_expr(right, prefixes);
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => visit_selects_in_statement(statement, &mut |select| {
            collect_prefixes_in_select(select, prefixes);
        }),
        _ => {}
    }
}

fn collect_prefixes_in_select(select: &Select, prefixes: &mut HashSet<String>) {
    visit_select_expressions(select, &mut |expr| {
        collect_qualifier_prefixes_in_expr(expr, prefixes)
    });

    for table in &select.from {
        collect_prefixes_in_table_factor(&table.relation, prefixes);
        for join in &table.joins {
            collect_prefixes_in_table_factor(&join.relation, prefixes);
        }
    }
}

fn collect_prefixes_in_table_factor(table_factor: &TableFactor, prefixes: &mut HashSet<String>) {
    match table_factor {
        TableFactor::Derived { subquery, .. } => collect_prefixes_in_query(subquery, prefixes),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_prefixes_in_table_factor(&table_with_joins.relation, prefixes);
            for join in &table_with_joins.joins {
                collect_prefixes_in_table_factor(&join.relation, prefixes);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_prefixes_in_table_factor(table, prefixes)
        }
        _ => {}
    }
}

#[allow(dead_code)]
pub fn expr_contains_equijoin(expr: &Expr) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            (*op == BinaryOperator::Eq
                && expr_is_column_reference(left)
                && expr_is_column_reference(right))
                || expr_contains_equijoin(left)
                || expr_contains_equijoin(right)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => expr_contains_equijoin(inner),
        Expr::InList { expr, list, .. } => {
            expr_contains_equijoin(expr) || list.iter().any(expr_contains_equijoin)
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            expr_contains_equijoin(expr)
                || expr_contains_equijoin(low)
                || expr_contains_equijoin(high)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand
                .as_ref()
                .is_some_and(|expr| expr_contains_equijoin(expr))
                || conditions.iter().any(|when| {
                    expr_contains_equijoin(&when.condition) || expr_contains_equijoin(&when.result)
                })
                || else_result
                    .as_ref()
                    .is_some_and(|expr| expr_contains_equijoin(expr))
        }
        _ => false,
    }
}

#[allow(dead_code)]
pub fn expr_is_trivial_join_condition(expr: &Expr) -> bool {
    match expr {
        Expr::Value(value) if matches!(value.value, Value::Boolean(true)) => true,
        Expr::Nested(inner)
        | Expr::UnaryOp { expr: inner, .. }
        | Expr::Cast { expr: inner, .. } => expr_is_trivial_join_condition(inner),
        Expr::BinaryOp { left, op, right } if *op == BinaryOperator::Eq => literal_value(left)
            .zip(literal_value(right))
            .is_some_and(|(left, right)| left == right),
        _ => false,
    }
}

fn expr_is_column_reference(expr: &Expr) -> bool {
    match expr {
        Expr::Identifier(_) => true,
        Expr::CompoundIdentifier(parts) => !parts.is_empty(),
        _ => false,
    }
}

fn literal_value(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Value(value) => Some(value.to_string().to_ascii_uppercase()),
        Expr::Nested(inner)
        | Expr::UnaryOp { expr: inner, .. }
        | Expr::Cast { expr: inner, .. } => literal_value(inner),
        _ => None,
    }
}
