//! Shared column-count resolution helpers for AM rules.

use sqlparser::ast::{
    JoinConstraint, JoinOperator, Query, Select, SelectItem, SetExpr, TableFactor, TableWithJoins,
};
use std::collections::HashMap;

use super::semantic_helpers::{table_factor_alias_name, table_factor_reference_name};

pub(crate) type CteColumnCounts = HashMap<String, Option<usize>>;

#[derive(Default)]
pub(crate) struct SourceColumns {
    pub(crate) names: Vec<String>,
    pub(crate) column_count: Option<usize>,
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

    let column_count =
        table_factor_alias_column_count(table_factor).or_else(|| match table_factor {
            TableFactor::Table { name, .. } => {
                let key = normalize_identifier(name.to_string());
                ctes.get(&key).copied().flatten()
            }
            TableFactor::Derived { subquery, .. } => resolve_query_output_columns(subquery, ctes),
            TableFactor::NestedJoin {
                table_with_joins, ..
            } => resolve_nested_join_output_columns(table_with_joins, ctes),
            TableFactor::Pivot { table, .. }
            | TableFactor::Unpivot { table, .. }
            | TableFactor::MatchRecognize { table, .. } => {
                source_columns_for_table_factor(table, ctes).column_count
            }
            _ => None,
        });

    SourceColumns {
        names,
        column_count,
    }
}

fn table_factor_alias_column_count(table_factor: &TableFactor) -> Option<usize> {
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

    declared_cte_column_count(alias.columns.len())
}

fn resolve_nested_join_output_columns(
    table_with_joins: &TableWithJoins,
    ctes: &CteColumnCounts,
) -> Option<usize> {
    // USING/NATURAL joins collapse duplicate key columns, so additive counting
    // is unsafe in those cases.
    if table_with_joins
        .joins
        .iter()
        .any(|join| has_non_additive_join_constraint(&join.join_operator))
    {
        return None;
    }

    let mut total =
        source_columns_for_table_factor(&table_with_joins.relation, ctes).column_count?;
    for join in &table_with_joins.joins {
        total += source_columns_for_table_factor(&join.relation, ctes).column_count?;
    }
    Some(total)
}

fn has_non_additive_join_constraint(operator: &JoinOperator) -> bool {
    match join_constraint(operator) {
        Some(JoinConstraint::Using(_) | JoinConstraint::Natural) => true,
        Some(JoinConstraint::None | JoinConstraint::On(_)) => false,
        // APPLY joins are shape-dependent and not represented with explicit
        // join constraints here.
        None => true,
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
