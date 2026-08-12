use super::*;

pub(super) fn fix_statement(stmt: &mut Statement, rule_filter: &RuleFilter) {
    match stmt {
        Statement::Query(query) => fix_query(query, rule_filter),
        Statement::Insert(insert) => {
            if let Some(source) = insert.source.as_mut() {
                fix_query(source, rule_filter);
            }
        }
        Statement::CreateView(CreateView { query, .. }) => fix_query(query, rule_filter),
        Statement::CreateTable(create) => {
            if let Some(query) = create.query.as_mut() {
                fix_query(query, rule_filter);
            }
        }
        _ => {}
    }
}

pub(super) fn fix_query(query: &mut Query, rule_filter: &RuleFilter) {
    if let Some(with) = query.with.as_mut() {
        for cte in &mut with.cte_tables {
            fix_query(&mut cte.query, rule_filter);
        }
    }

    fix_set_expr(query.body.as_mut(), rule_filter);
    rewrite_simple_derived_subqueries_to_ctes(query, rule_filter);

    if let Some(order_by) = query.order_by.as_mut() {
        fix_order_by(order_by, rule_filter);
    }

    if let Some(limit_clause) = query.limit_clause.as_mut() {
        fix_limit_clause(limit_clause, rule_filter);
    }

    if let Some(fetch) = query.fetch.as_mut() {
        if let Some(quantity) = fetch.quantity.as_mut() {
            fix_expr(quantity, rule_filter);
        }
    }
}

pub(super) fn fix_set_expr(body: &mut SetExpr, rule_filter: &RuleFilter) {
    match body {
        SetExpr::Select(select) => fix_select(select, rule_filter),
        SetExpr::Query(query) => fix_query(query, rule_filter),
        SetExpr::SetOperation { left, right, .. } => {
            fix_set_expr(left, rule_filter);
            fix_set_expr(right, rule_filter);
        }
        SetExpr::Values(values) => {
            for row in &mut values.rows {
                for expr in row {
                    fix_expr(expr, rule_filter);
                }
            }
        }
        SetExpr::Insert(stmt)
        | SetExpr::Update(stmt)
        | SetExpr::Delete(stmt)
        | SetExpr::Merge(stmt) => fix_statement(stmt, rule_filter),
        _ => {}
    }
}

pub(super) fn fix_select(select: &mut Select, rule_filter: &RuleFilter) {
    for item in &mut select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) => {
                fix_expr(expr, rule_filter);
            }
            SelectItem::ExprWithAlias { expr, .. } => {
                fix_expr(expr, rule_filter);
            }
            SelectItem::QualifiedWildcard(SelectItemQualifiedWildcardKind::Expr(expr), _) => {
                fix_expr(expr, rule_filter);
            }
            _ => {}
        }
    }

    for table_with_joins in &mut select.from {
        if rule_filter.allows(issue_codes::LINT_CV_008) {
            rewrite_right_join_to_left(table_with_joins);
        }

        fix_table_factor(&mut table_with_joins.relation, rule_filter);

        let mut left_ref = table_factor_reference_name(&table_with_joins.relation);

        for join in &mut table_with_joins.joins {
            let right_ref = table_factor_reference_name(&join.relation);
            if rule_filter.allows(issue_codes::LINT_ST_007) {
                rewrite_using_join_constraint(
                    &mut join.join_operator,
                    left_ref.as_deref(),
                    right_ref.as_deref(),
                );
            }

            fix_table_factor(&mut join.relation, rule_filter);
            fix_join_operator(&mut join.join_operator, rule_filter);

            if right_ref.is_some() {
                left_ref = right_ref;
            }
        }
    }

    if let Some(prewhere) = select.prewhere.as_mut() {
        fix_expr(prewhere, rule_filter);
    }

    if let Some(selection) = select.selection.as_mut() {
        fix_expr(selection, rule_filter);
    }

    if let Some(having) = select.having.as_mut() {
        fix_expr(having, rule_filter);
    }

    if let Some(qualify) = select.qualify.as_mut() {
        fix_expr(qualify, rule_filter);
    }

    if let GroupByExpr::Expressions(exprs, _) = &mut select.group_by {
        for expr in exprs {
            fix_expr(expr, rule_filter);
        }
    }

    for expr in &mut select.cluster_by {
        fix_expr(expr, rule_filter);
    }

    for expr in &mut select.distribute_by {
        fix_expr(expr, rule_filter);
    }

    for expr in &mut select.sort_by {
        fix_expr(&mut expr.expr, rule_filter);
    }

    for lateral_view in &mut select.lateral_views {
        fix_expr(&mut lateral_view.lateral_view, rule_filter);
    }

    for connect_by_kind in &mut select.connect_by {
        match connect_by_kind {
            ConnectByKind::ConnectBy { relationships, .. } => {
                for relationship in relationships {
                    fix_expr(relationship, rule_filter);
                }
            }
            ConnectByKind::StartWith { condition, .. } => {
                fix_expr(condition, rule_filter);
            }
        }
    }
}

pub(super) fn rewrite_simple_derived_subqueries_to_ctes(
    query: &mut Query,
    rule_filter: &RuleFilter,
) {
    if !rule_filter.allows(issue_codes::LINT_ST_005) {
        return;
    }

    let SetExpr::Select(select) = query.body.as_mut() else {
        return;
    };

    let outer_source_names = select_source_names_upper(select);
    let mut used_cte_names: HashSet<String> = query
        .with
        .as_ref()
        .map(|with| {
            with.cte_tables
                .iter()
                .map(|cte| cte.alias.name.value.to_ascii_uppercase())
                .collect()
        })
        .unwrap_or_default();
    used_cte_names.extend(outer_source_names.iter().cloned());

    let mut new_ctes = Vec::new();

    for table_with_joins in &mut select.from {
        if rule_filter.st005_forbid_subquery_in.forbid_from() {
            if let Some(cte) = rewrite_derived_table_factor_to_cte(
                &mut table_with_joins.relation,
                &outer_source_names,
                &mut used_cte_names,
            ) {
                new_ctes.push(cte);
            }
        }

        if rule_filter.st005_forbid_subquery_in.forbid_join() {
            for join in &mut table_with_joins.joins {
                if let Some(cte) = rewrite_derived_table_factor_to_cte(
                    &mut join.relation,
                    &outer_source_names,
                    &mut used_cte_names,
                ) {
                    new_ctes.push(cte);
                }
            }
        }
    }

    if new_ctes.is_empty() {
        return;
    }

    let with = query.with.get_or_insert_with(|| With {
        with_token: AttachedToken::empty(),
        recursive: false,
        cte_tables: Vec::new(),
    });
    with.cte_tables.extend(new_ctes);
}

pub(super) fn rewrite_derived_table_factor_to_cte(
    relation: &mut TableFactor,
    outer_source_names: &HashSet<String>,
    used_cte_names: &mut HashSet<String>,
) -> Option<Cte> {
    let (lateral, subquery, alias) = match relation {
        TableFactor::Derived {
            lateral,
            subquery,
            alias,
            ..
        } => (lateral, subquery, alias),
        _ => return None,
    };

    if *lateral {
        return None;
    }

    // Keep this rewrite conservative: only SELECT subqueries that do not
    // appear to reference outer sources.
    if !matches!(subquery.body.as_ref(), SetExpr::Select(_))
        || query_text_references_outer_sources(subquery, outer_source_names)
    {
        return None;
    }

    let cte_alias = alias.clone().unwrap_or_else(|| TableAlias {
        explicit: false,
        name: Ident::new(next_generated_cte_name(used_cte_names)),
        columns: Vec::new(),
    });
    let cte_name_ident = cte_alias.name.clone();
    let cte_name_upper = cte_name_ident.value.to_ascii_uppercase();
    used_cte_names.insert(cte_name_upper);

    let cte = Cte {
        alias: cte_alias,
        query: subquery.clone(),
        from: None,
        materialized: None,
        closing_paren_token: AttachedToken::empty(),
    };

    *relation = TableFactor::Table {
        name: vec![cte_name_ident].into(),
        alias: None,
        args: None,
        with_hints: Vec::new(),
        version: None,
        with_ordinality: false,
        partitions: Vec::new(),
        json_path: None,
        sample: None,
        index_hints: Vec::new(),
    };

    Some(cte)
}

pub(super) fn next_generated_cte_name(used_cte_names: &HashSet<String>) -> String {
    let mut index = 1usize;
    loop {
        let candidate = format!("cte_subquery_{index}");
        if !used_cte_names.contains(&candidate.to_ascii_uppercase()) {
            return candidate;
        }
        index += 1;
    }
}

pub(super) fn query_text_references_outer_sources(
    query: &Query,
    outer_source_names: &HashSet<String>,
) -> bool {
    if outer_source_names.is_empty() {
        return false;
    }

    let rendered_upper = query.to_string().to_ascii_uppercase();
    outer_source_names
        .iter()
        .any(|name| rendered_upper.contains(&format!("{name}.")))
}

pub(super) fn select_source_names_upper(select: &Select) -> HashSet<String> {
    let mut names = HashSet::new();
    for table in &select.from {
        collect_source_names_from_table_factor(&table.relation, &mut names);
        for join in &table.joins {
            collect_source_names_from_table_factor(&join.relation, &mut names);
        }
    }
    names
}

pub(super) fn collect_source_names_from_table_factor(
    relation: &TableFactor,
    names: &mut HashSet<String>,
) {
    match relation {
        TableFactor::Table { name, alias, .. } => {
            if let Some(last) = name.0.last().and_then(|part| part.as_ident()) {
                names.insert(last.value.to_ascii_uppercase());
            }
            if let Some(alias) = alias {
                names.insert(alias.name.value.to_ascii_uppercase());
            }
        }
        TableFactor::Derived { alias, .. }
        | TableFactor::TableFunction { alias, .. }
        | TableFactor::Function { alias, .. }
        | TableFactor::UNNEST { alias, .. }
        | TableFactor::JsonTable { alias, .. }
        | TableFactor::OpenJsonTable { alias, .. }
        | TableFactor::NestedJoin { alias, .. }
        | TableFactor::Pivot { alias, .. }
        | TableFactor::Unpivot { alias, .. } => {
            if let Some(alias) = alias {
                names.insert(alias.name.value.to_ascii_uppercase());
            }
        }
        _ => {}
    }
}

pub(super) fn rewrite_right_join_to_left(table_with_joins: &mut TableWithJoins) {
    while let Some(index) = table_with_joins
        .joins
        .iter()
        .position(|join| rewritable_right_join(&join.join_operator))
    {
        rewrite_right_join_at_index(table_with_joins, index);
    }
}

pub(super) fn rewrite_right_join_at_index(table_with_joins: &mut TableWithJoins, index: usize) {
    let mut suffix = table_with_joins.joins.split_off(index);
    let mut join = suffix.remove(0);

    let old_operator = std::mem::replace(
        &mut join.join_operator,
        JoinOperator::CrossJoin(JoinConstraint::None),
    );
    let Some(new_operator) = rewritten_left_join_operator(old_operator) else {
        table_with_joins.joins.push(join);
        table_with_joins.joins.append(&mut suffix);
        return;
    };

    let previous_relation = std::mem::replace(&mut table_with_joins.relation, join.relation);
    let prefix_joins = std::mem::take(&mut table_with_joins.joins);

    join.relation = if prefix_joins.is_empty() {
        previous_relation
    } else {
        TableFactor::NestedJoin {
            table_with_joins: Box::new(TableWithJoins {
                relation: previous_relation,
                joins: prefix_joins,
            }),
            alias: None,
        }
    };
    join.join_operator = new_operator;

    table_with_joins.joins.push(join);
    table_with_joins.joins.append(&mut suffix);
}

pub(super) fn rewritable_right_join(operator: &JoinOperator) -> bool {
    matches!(
        operator,
        JoinOperator::Right(_)
            | JoinOperator::RightOuter(_)
            | JoinOperator::RightSemi(_)
            | JoinOperator::RightAnti(_)
    )
}

pub(super) fn rewritten_left_join_operator(operator: JoinOperator) -> Option<JoinOperator> {
    match operator {
        JoinOperator::Right(constraint) => Some(JoinOperator::Left(constraint)),
        JoinOperator::RightOuter(constraint) => Some(JoinOperator::LeftOuter(constraint)),
        JoinOperator::RightSemi(constraint) => Some(JoinOperator::LeftSemi(constraint)),
        JoinOperator::RightAnti(constraint) => Some(JoinOperator::LeftAnti(constraint)),
        _ => None,
    }
}

pub(super) fn table_factor_alias_ident(relation: &TableFactor) -> Option<&Ident> {
    let alias = match relation {
        TableFactor::Table { alias, .. }
        | TableFactor::Derived { alias, .. }
        | TableFactor::TableFunction { alias, .. }
        | TableFactor::Function { alias, .. }
        | TableFactor::UNNEST { alias, .. }
        | TableFactor::JsonTable { alias, .. }
        | TableFactor::OpenJsonTable { alias, .. }
        | TableFactor::NestedJoin { alias, .. }
        | TableFactor::Pivot { alias, .. }
        | TableFactor::Unpivot { alias, .. } => alias.as_ref(),
        _ => None,
    }?;

    Some(&alias.name)
}

pub(super) fn table_factor_reference_name(relation: &TableFactor) -> Option<String> {
    match relation {
        TableFactor::Table { name, alias, .. } => {
            if let Some(alias) = alias {
                Some(alias.name.value.clone())
            } else {
                name.0
                    .last()
                    .and_then(|part| part.as_ident())
                    .map(|ident| ident.value.clone())
            }
        }
        _ => None,
    }
}

pub(super) fn rewrite_using_join_constraint(
    join_operator: &mut JoinOperator,
    left_ref: Option<&str>,
    right_ref: Option<&str>,
) {
    let (Some(left_ref), Some(right_ref)) = (left_ref, right_ref) else {
        return;
    };

    let Some(constraint) = join_constraint_mut(join_operator) else {
        return;
    };

    let JoinConstraint::Using(columns) = constraint else {
        return;
    };

    if columns.is_empty() {
        return;
    }

    let mut combined: Option<Expr> = None;
    for object_name in columns.iter() {
        let Some(column_ident) = object_name
            .0
            .last()
            .and_then(|part| part.as_ident())
            .cloned()
        else {
            continue;
        };

        let equality = Expr::BinaryOp {
            left: Box::new(Expr::CompoundIdentifier(vec![
                Ident::new(left_ref),
                column_ident.clone(),
            ])),
            op: BinaryOperator::Eq,
            right: Box::new(Expr::CompoundIdentifier(vec![
                Ident::new(right_ref),
                column_ident,
            ])),
        };

        combined = Some(match combined {
            Some(prev) => Expr::BinaryOp {
                left: Box::new(prev),
                op: BinaryOperator::And,
                right: Box::new(equality),
            },
            None => equality,
        });
    }

    if let Some(on_expr) = combined {
        *constraint = JoinConstraint::On(on_expr);
    }
}

pub(super) fn fix_table_factor(relation: &mut TableFactor, rule_filter: &RuleFilter) {
    match relation {
        TableFactor::Table {
            args, with_hints, ..
        } => {
            if let Some(args) = args {
                for arg in &mut args.args {
                    fix_function_arg(arg, rule_filter);
                }
            }
            for hint in with_hints {
                fix_expr(hint, rule_filter);
            }
        }
        TableFactor::Derived { subquery, .. } => fix_query(subquery, rule_filter),
        TableFactor::TableFunction { expr, .. } => fix_expr(expr, rule_filter),
        TableFactor::Function { args, .. } => {
            for arg in args {
                fix_function_arg(arg, rule_filter);
            }
        }
        TableFactor::UNNEST { array_exprs, .. } => {
            for expr in array_exprs {
                fix_expr(expr, rule_filter);
            }
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            if rule_filter.allows(issue_codes::LINT_CV_008) {
                rewrite_right_join_to_left(table_with_joins);
            }

            fix_table_factor(&mut table_with_joins.relation, rule_filter);

            let mut left_ref = table_factor_reference_name(&table_with_joins.relation);

            for join in &mut table_with_joins.joins {
                let right_ref = table_factor_reference_name(&join.relation);
                if rule_filter.allows(issue_codes::LINT_ST_007) {
                    rewrite_using_join_constraint(
                        &mut join.join_operator,
                        left_ref.as_deref(),
                        right_ref.as_deref(),
                    );
                }

                fix_table_factor(&mut join.relation, rule_filter);
                fix_join_operator(&mut join.join_operator, rule_filter);

                if right_ref.is_some() {
                    left_ref = right_ref;
                }
            }
        }
        TableFactor::Pivot {
            table,
            aggregate_functions,
            value_column,
            default_on_null,
            ..
        } => {
            fix_table_factor(table, rule_filter);
            for func in aggregate_functions {
                fix_expr(&mut func.expr, rule_filter);
            }
            for expr in value_column {
                fix_expr(expr, rule_filter);
            }
            if let Some(expr) = default_on_null {
                fix_expr(expr, rule_filter);
            }
        }
        TableFactor::Unpivot {
            table,
            value,
            columns,
            ..
        } => {
            fix_table_factor(table, rule_filter);
            fix_expr(value, rule_filter);
            for column in columns {
                fix_expr(&mut column.expr, rule_filter);
            }
        }
        TableFactor::JsonTable { json_expr, .. } => fix_expr(json_expr, rule_filter),
        TableFactor::OpenJsonTable { json_expr, .. } => fix_expr(json_expr, rule_filter),
        _ => {}
    }
}

pub(super) fn fix_join_operator(op: &mut JoinOperator, rule_filter: &RuleFilter) {
    match op {
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
        | JoinOperator::StraightJoin(constraint) => fix_join_constraint(constraint, rule_filter),
        JoinOperator::AsOf {
            match_condition,
            constraint,
        } => {
            fix_expr(match_condition, rule_filter);
            fix_join_constraint(constraint, rule_filter);
        }
        JoinOperator::CrossApply | JoinOperator::OuterApply => {}
    }
}

pub(super) fn join_constraint_mut(join_operator: &mut JoinOperator) -> Option<&mut JoinConstraint> {
    match join_operator {
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
        | JoinOperator::StraightJoin(constraint) => Some(constraint),
        JoinOperator::AsOf { constraint, .. } => Some(constraint),
        JoinOperator::CrossApply | JoinOperator::OuterApply => None,
    }
}

pub(super) fn fix_join_constraint(constraint: &mut JoinConstraint, rule_filter: &RuleFilter) {
    if let JoinConstraint::On(expr) = constraint {
        fix_expr(expr, rule_filter);
    }
}

pub(super) fn fix_order_by(order_by: &mut OrderBy, rule_filter: &RuleFilter) {
    if let OrderByKind::Expressions(exprs) = &mut order_by.kind {
        for order_expr in exprs.iter_mut() {
            fix_expr(&mut order_expr.expr, rule_filter);
        }
    }

    if let Some(interpolate) = order_by.interpolate.as_mut() {
        if let Some(exprs) = interpolate.exprs.as_mut() {
            for expr in exprs {
                if let Some(inner) = expr.expr.as_mut() {
                    fix_expr(inner, rule_filter);
                }
            }
        }
    }
}

pub(super) fn fix_limit_clause(limit_clause: &mut LimitClause, rule_filter: &RuleFilter) {
    match limit_clause {
        LimitClause::LimitOffset {
            limit,
            offset,
            limit_by,
        } => {
            if let Some(limit) = limit {
                fix_expr(limit, rule_filter);
            }
            if let Some(offset) = offset {
                fix_expr(&mut offset.value, rule_filter);
            }
            for expr in limit_by {
                fix_expr(expr, rule_filter);
            }
        }
        LimitClause::OffsetCommaLimit { offset, limit } => {
            fix_expr(offset, rule_filter);
            fix_expr(limit, rule_filter);
        }
    }
}

pub(super) fn fix_expr(expr: &mut Expr, rule_filter: &RuleFilter) {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            fix_expr(left, rule_filter);
            fix_expr(right, rule_filter);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::IsTrue(inner)
        | Expr::IsNotTrue(inner)
        | Expr::IsFalse(inner)
        | Expr::IsNotFalse(inner)
        | Expr::IsUnknown(inner)
        | Expr::IsNotUnknown(inner) => fix_expr(inner, rule_filter),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand.as_mut() {
                fix_expr(operand, rule_filter);
            }
            for case_when in conditions {
                fix_expr(&mut case_when.condition, rule_filter);
                fix_expr(&mut case_when.result, rule_filter);
            }
            if let Some(else_result) = else_result.as_mut() {
                fix_expr(else_result, rule_filter);
            }
        }
        Expr::Function(func) => fix_function(func, rule_filter),
        Expr::Cast { expr: inner, .. } => fix_expr(inner, rule_filter),
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => {
            fix_expr(inner, rule_filter);
            fix_query(subquery, rule_filter);
        }
        Expr::Subquery(subquery) | Expr::Exists { subquery, .. } => {
            fix_query(subquery, rule_filter)
        }
        Expr::Between {
            expr: target,
            low,
            high,
            ..
        } => {
            fix_expr(target, rule_filter);
            fix_expr(low, rule_filter);
            fix_expr(high, rule_filter);
        }
        Expr::InList {
            expr: target, list, ..
        } => {
            fix_expr(target, rule_filter);
            for item in list {
                fix_expr(item, rule_filter);
            }
        }
        Expr::Tuple(items) => {
            for item in items {
                fix_expr(item, rule_filter);
            }
        }
        _ => {}
    }

    // CV11 cast-style rewriting is now handled entirely by the core autofix
    // in cv_011.rs, which correctly supports first-seen consistent mode,
    // CONVERT conversions, and chained :: expressions.

    if rule_filter.allows(issue_codes::LINT_ST_004) {
        if let Some(rewritten) = nested_case_rewrite(expr) {
            *expr = rewritten;
        }
    }
}

pub(super) fn fix_function(func: &mut Function, rule_filter: &RuleFilter) {
    if let FunctionArguments::List(arg_list) = &mut func.args {
        for arg in &mut arg_list.args {
            fix_function_arg(arg, rule_filter);
        }
        for clause in &mut arg_list.clauses {
            match clause {
                FunctionArgumentClause::OrderBy(order_by_exprs) => {
                    for order_by_expr in order_by_exprs {
                        fix_expr(&mut order_by_expr.expr, rule_filter);
                    }
                }
                FunctionArgumentClause::Limit(expr) => fix_expr(expr, rule_filter),
                _ => {}
            }
        }
    }

    if let Some(filter) = func.filter.as_mut() {
        fix_expr(filter, rule_filter);
    }

    for order_expr in &mut func.within_group {
        fix_expr(&mut order_expr.expr, rule_filter);
    }
}

pub(super) fn fix_function_arg(arg: &mut FunctionArg, rule_filter: &RuleFilter) {
    match arg {
        FunctionArg::Named { arg, .. }
        | FunctionArg::ExprNamed { arg, .. }
        | FunctionArg::Unnamed(arg) => {
            if let FunctionArgExpr::Expr(expr) = arg {
                fix_expr(expr, rule_filter);
            }
        }
    }
}

pub(super) fn nested_case_rewrite(expr: &Expr) -> Option<Expr> {
    let Expr::Case {
        case_token,
        operand: outer_operand,
        conditions: outer_conditions,
        else_result: Some(outer_else),
        end_token,
    } = expr
    else {
        return None;
    };

    if outer_conditions.is_empty() {
        return None;
    }

    let Expr::Case {
        operand: inner_operand,
        conditions: inner_conditions,
        else_result: inner_else,
        ..
    } = nested_case_expr(outer_else.as_ref())?
    else {
        return None;
    };

    if inner_conditions.is_empty() {
        return None;
    }

    if !case_operands_match(outer_operand.as_deref(), inner_operand.as_deref()) {
        return None;
    }

    let mut merged_conditions = outer_conditions.clone();
    merged_conditions.extend(inner_conditions.iter().cloned());

    Some(Expr::Case {
        case_token: case_token.clone(),
        operand: outer_operand.clone(),
        conditions: merged_conditions,
        else_result: inner_else.clone(),
        end_token: end_token.clone(),
    })
}

pub(super) fn nested_case_expr(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Case { .. } => Some(expr),
        Expr::Nested(inner) => nested_case_expr(inner),
        _ => None,
    }
}

pub(super) fn case_operands_match(outer: Option<&Expr>, inner: Option<&Expr>) -> bool {
    match (outer, inner) {
        (None, None) => true,
        (Some(left), Some(right)) => format!("{left}") == format!("{right}"),
        _ => false,
    }
}
