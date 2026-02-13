//! Shared column-count resolution helpers for AM rules.

use sqlparser::ast::{
    JoinConstraint, JoinOperator, Query, Select, SelectItem, SetExpr, TableFactor, TableWithJoins,
};
use std::collections::{HashMap, HashSet};

use super::semantic_helpers::{table_factor_alias_name, table_factor_reference_name};

pub(crate) type CteColumnCounts = HashMap<String, Option<usize>>;

#[derive(Default)]
pub(crate) struct SourceColumns {
    pub(crate) names: Vec<String>,
    pub(crate) column_count: Option<usize>,
    pub(crate) column_names: Option<Vec<String>>,
}

struct JoinOutputShape {
    column_count: usize,
    column_names: Option<Vec<String>>,
}

pub(crate) fn build_query_cte_map(query: &Query, outer_ctes: &CteColumnCounts) -> CteColumnCounts {
    let mut ctes = outer_ctes.clone();

    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            let cte_name = cte.alias.name.value.to_ascii_uppercase();
            ctes.insert(cte_name.clone(), None);
            let count = declared_cte_column_count(cte.alias.columns.len())
                .or_else(|| resolve_query_output_columns(&cte.query, &ctes));
            ctes.insert(cte_name, count);
        }
    }

    ctes
}

pub(crate) fn build_query_cte_map_strict(
    query: &Query,
    outer_ctes: &CteColumnCounts,
) -> CteColumnCounts {
    let mut ctes = outer_ctes.clone();

    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            let cte_name = cte.alias.name.value.to_ascii_uppercase();
            ctes.insert(cte_name.clone(), None);
            let count = declared_cte_column_count(cte.alias.columns.len())
                .or_else(|| resolve_query_output_columns_strict(&cte.query, &ctes));
            ctes.insert(cte_name, count);
        }
    }

    ctes
}

fn declared_cte_column_count(count: usize) -> Option<usize> {
    (count > 0).then_some(count)
}

pub(crate) fn resolve_query_output_columns(
    query: &Query,
    outer_ctes: &CteColumnCounts,
) -> Option<usize> {
    let ctes = build_query_cte_map(query, outer_ctes);
    resolve_set_expr_output_columns(&query.body, &ctes)
}

pub(crate) fn resolve_query_output_columns_strict(
    query: &Query,
    outer_ctes: &CteColumnCounts,
) -> Option<usize> {
    let ctes = build_query_cte_map_strict(query, outer_ctes);
    resolve_set_expr_output_columns_strict(&query.body, &ctes)
}

pub(crate) fn resolve_set_expr_output_columns(
    set_expr: &SetExpr,
    ctes: &CteColumnCounts,
) -> Option<usize> {
    match set_expr {
        SetExpr::Select(select) => resolve_select_output_columns(select, ctes),
        SetExpr::Query(query) => resolve_query_output_columns(query, ctes),
        SetExpr::Values(values) => values.rows.first().map(std::vec::Vec::len),
        // Follow SQLFluff AM07 behavior for wildcard resolution through set
        // expressions by treating the first selectable as representative.
        SetExpr::SetOperation { left, .. } => resolve_set_expr_output_columns(left, ctes),
        _ => None,
    }
}

pub(crate) fn resolve_set_expr_output_columns_strict(
    set_expr: &SetExpr,
    ctes: &CteColumnCounts,
) -> Option<usize> {
    match set_expr {
        SetExpr::Select(select) => resolve_select_output_columns(select, ctes),
        SetExpr::Query(query) => resolve_query_output_columns_strict(query, ctes),
        SetExpr::Values(values) => values.rows.first().map(std::vec::Vec::len),
        SetExpr::SetOperation { left, right, .. } => {
            let left_count = resolve_set_expr_output_columns_strict(left, ctes)?;
            resolve_set_expr_output_columns_strict(right, ctes)?;
            Some(left_count)
        }
        _ => None,
    }
}

pub(crate) fn resolve_select_output_columns(
    select: &Select,
    ctes: &CteColumnCounts,
) -> Option<usize> {
    let sources = collect_select_sources(select, ctes);

    let mut count = 0usize;
    for item in &select.projection {
        match item {
            SelectItem::Wildcard(_) => {
                count += sum_all_source_columns(&sources)?;
            }
            SelectItem::QualifiedWildcard(name, _) => {
                count += resolve_qualified_wildcard_columns(&name.to_string(), &sources)?;
            }
            _ => count += 1,
        }
    }

    Some(count)
}

pub(crate) fn collect_select_sources(
    select: &Select,
    ctes: &CteColumnCounts,
) -> Vec<SourceColumns> {
    let mut sources = Vec::new();

    for table in &select.from {
        sources.push(source_columns_for_table_factor(&table.relation, ctes));
        for join in &table.joins {
            sources.push(source_columns_for_table_factor(&join.relation, ctes));
        }
    }

    sources
}

pub(crate) fn source_columns_for_table_factor(
    table_factor: &TableFactor,
    ctes: &CteColumnCounts,
) -> SourceColumns {
    let mut names = Vec::new();

    if let Some(reference_name) = table_factor_reference_name(table_factor) {
        names.push(reference_name);
    }

    if let Some(alias_name) = table_factor_alias_name(table_factor) {
        let alias_upper = alias_name.to_ascii_uppercase();
        if !names.contains(&alias_upper) {
            names.push(alias_upper);
        }
    }

    let (column_count, column_names) =
        if let Some(alias_columns) = table_factor_alias_column_names(table_factor) {
            (Some(alias_columns.len()), Some(alias_columns))
        } else {
            match table_factor {
                TableFactor::Table { name, .. } => {
                    let key = normalize_identifier(name.to_string());
                    (ctes.get(&key).copied().flatten(), None)
                }
                TableFactor::Derived { subquery, .. } => (
                    resolve_query_output_columns(subquery, ctes),
                    resolve_query_output_column_names(subquery),
                ),
                TableFactor::NestedJoin {
                    table_with_joins, ..
                } => resolve_nested_join_output_shape(table_with_joins, ctes)
                    .map(|shape| (Some(shape.column_count), shape.column_names))
                    .unwrap_or((None, None)),
                TableFactor::Pivot { table, .. }
                | TableFactor::Unpivot { table, .. }
                | TableFactor::MatchRecognize { table, .. } => {
                    let source = source_columns_for_table_factor(table, ctes);
                    (source.column_count, source.column_names)
                }
                _ => (None, None),
            }
        };

    let resolved_column_count =
        column_count.or_else(|| column_names.as_ref().map(std::vec::Vec::len));

    SourceColumns {
        names,
        column_count: resolved_column_count,
        column_names,
    }
}

fn table_factor_alias_column_names(table_factor: &TableFactor) -> Option<Vec<String>> {
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

    if alias.columns.is_empty() {
        return None;
    }

    Some(
        alias
            .columns
            .iter()
            .map(|column| normalize_identifier(column.name.value.clone()))
            .collect(),
    )
}

fn resolve_nested_join_output_shape(
    table_with_joins: &TableWithJoins,
    ctes: &CteColumnCounts,
) -> Option<JoinOutputShape> {
    let mut total = source_columns_for_table_factor(&table_with_joins.relation, ctes)
        .resolved_join_output_shape()?;
    for join in &table_with_joins.joins {
        let right =
            source_columns_for_table_factor(&join.relation, ctes).resolved_join_output_shape()?;
        total = combine_join_shape(total, right, &join.join_operator)?;
    }
    Some(total)
}

fn combine_join_shape(
    left: JoinOutputShape,
    right: JoinOutputShape,
    operator: &JoinOperator,
) -> Option<JoinOutputShape> {
    match join_constraint(operator) {
        Some(JoinConstraint::Using(columns)) => {
            let count = left
                .column_count
                .checked_add(right.column_count)?
                .checked_sub(columns.len())?;
            let column_names =
                combine_using_column_names(left.column_names, right.column_names, columns);
            Some(JoinOutputShape {
                column_count: count,
                column_names,
            })
        }
        Some(JoinConstraint::Natural) => {
            let (column_names, overlap_count) =
                combine_natural_column_names(left.column_names, right.column_names)?;
            let count = left
                .column_count
                .checked_add(right.column_count)?
                .checked_sub(overlap_count)?;
            Some(JoinOutputShape {
                column_count: count,
                column_names: Some(column_names),
            })
        }
        Some(JoinConstraint::None | JoinConstraint::On(_)) => {
            let count = left.column_count.checked_add(right.column_count)?;
            let column_names = combine_cross_column_names(left.column_names, right.column_names);
            Some(JoinOutputShape {
                column_count: count,
                column_names,
            })
        }
        // APPLY joins are shape-dependent and not represented with explicit
        // join constraints here.
        None => None,
    }
}

fn join_constraint(operator: &JoinOperator) -> Option<&JoinConstraint> {
    match operator {
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

pub(crate) fn sum_all_source_columns(sources: &[SourceColumns]) -> Option<usize> {
    if sources.is_empty() {
        return None;
    }

    let mut total = 0usize;
    for source in sources {
        total += source.column_count?;
    }

    Some(total)
}

pub(crate) fn resolve_qualified_wildcard_columns(
    qualifier: &str,
    sources: &[SourceColumns],
) -> Option<usize> {
    let cleaned = qualifier.strip_suffix(".*").unwrap_or(qualifier);
    let qualifier_upper = normalize_identifier(cleaned.to_string());

    find_source_columns_by_name(&qualifier_upper, sources).or_else(|| {
        qualifier_upper
            .rsplit('.')
            .next()
            .and_then(|tail| find_source_columns_by_name(tail, sources))
    })
}

fn find_source_columns_by_name(name: &str, sources: &[SourceColumns]) -> Option<usize> {
    for source in sources {
        if source.names.iter().any(|candidate| candidate == name) {
            return source.column_count;
        }
    }
    None
}

fn resolve_query_output_column_names(query: &Query) -> Option<Vec<String>> {
    resolve_set_expr_output_column_names(&query.body)
}

fn resolve_set_expr_output_column_names(set_expr: &SetExpr) -> Option<Vec<String>> {
    match set_expr {
        SetExpr::Select(select) => resolve_select_output_column_names(select),
        SetExpr::Query(query) => resolve_query_output_column_names(query),
        // Follow SQLFluff AM07 behavior for set expressions by treating the
        // first selectable as representative for output shape.
        SetExpr::SetOperation { left, .. } => resolve_set_expr_output_column_names(left),
        _ => None,
    }
}

fn resolve_select_output_column_names(select: &Select) -> Option<Vec<String>> {
    let mut names = Vec::new();

    for item in &select.projection {
        let name = projection_output_name(item)?;
        names.push(name);
    }

    Some(names)
}

fn projection_output_name(item: &SelectItem) -> Option<String> {
    match item {
        SelectItem::ExprWithAlias { alias, .. } => Some(normalize_identifier(alias.value.clone())),
        SelectItem::UnnamedExpr(expr) => Some(expr_output_name(expr)),
        SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => None,
    }
}

fn expr_output_name(expr: &sqlparser::ast::Expr) -> String {
    match expr {
        sqlparser::ast::Expr::Identifier(identifier) => {
            normalize_identifier(identifier.value.clone())
        }
        sqlparser::ast::Expr::CompoundIdentifier(parts) => parts
            .last()
            .map(|part| normalize_identifier(part.value.clone()))
            .unwrap_or_else(|| normalize_identifier(expr.to_string())),
        sqlparser::ast::Expr::Nested(inner)
        | sqlparser::ast::Expr::UnaryOp { expr: inner, .. }
        | sqlparser::ast::Expr::Cast { expr: inner, .. } => expr_output_name(inner),
        _ => normalize_identifier(expr.to_string()),
    }
}

fn combine_cross_column_names(
    left: Option<Vec<String>>,
    right: Option<Vec<String>>,
) -> Option<Vec<String>> {
    let mut left = left?;
    let right = right?;
    left.extend(right);
    Some(left)
}

fn combine_using_column_names<T: std::fmt::Display>(
    left: Option<Vec<String>>,
    right: Option<Vec<String>>,
    using_columns: &[T],
) -> Option<Vec<String>> {
    let mut left = left?;
    let right = right?;
    let using_names: HashSet<String> = using_columns
        .iter()
        .map(|column| normalize_identifier(column.to_string()))
        .collect();

    for column in right {
        if using_names.contains(&column) {
            continue;
        }
        left.push(column);
    }

    Some(left)
}

fn combine_natural_column_names(
    left: Option<Vec<String>>,
    right: Option<Vec<String>>,
) -> Option<(Vec<String>, usize)> {
    let mut left = left?;
    let right = right?;
    let mut left_names: HashSet<String> = left.iter().cloned().collect();
    let right_names: HashSet<String> = right.iter().cloned().collect();
    let overlap_count = left_names.intersection(&right_names).count();

    for column in right {
        if left_names.contains(&column) {
            continue;
        }
        left_names.insert(column.clone());
        left.push(column);
    }

    Some((left, overlap_count))
}

impl SourceColumns {
    fn resolved_join_output_shape(self) -> Option<JoinOutputShape> {
        Some(JoinOutputShape {
            column_count: self.column_count?,
            column_names: self.column_names,
        })
    }
}

pub(crate) fn normalize_identifier(raw: String) -> String {
    raw.rsplit('.')
        .next()
        .unwrap_or(&raw)
        .trim_matches('"')
        .trim_matches('`')
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_uppercase()
}
