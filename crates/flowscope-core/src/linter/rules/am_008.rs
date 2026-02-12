//! LINT_AM_008: Ambiguous set-operation columns.
//!
//! SQLFluff AM07 parity: set-operation branches should resolve to the same
//! number of output columns when wildcard expansion is deterministically known.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Query, Select, SelectItem, SetExpr, Statement, TableFactor};
use std::collections::{HashMap, HashSet};

use super::semantic_helpers::{table_factor_alias_name, table_factor_reference_name};

pub struct AmbiguousSetColumns;

type CteColumnCounts = HashMap<String, Option<usize>>;

#[derive(Default)]
struct SetCountStats {
    counts: HashSet<usize>,
    fully_resolved: bool,
}

#[derive(Default)]
struct SourceColumns {
    names: Vec<String>,
    column_count: Option<usize>,
}

impl LintRule for AmbiguousSetColumns {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_008
    }

    fn name(&self) -> &'static str {
        "Ambiguous set columns"
    }

    fn description(&self) -> &'static str {
        "Set operation branches should return the same number of columns."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violation_count = 0usize;
        lint_statement_set_ops(statement, &HashMap::new(), &mut violation_count);

        (0..violation_count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_AM_008,
                    "Set operation branches resolve to different column counts.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn lint_statement_set_ops(
    statement: &Statement,
    outer_ctes: &CteColumnCounts,
    violations: &mut usize,
) {
    match statement {
        Statement::Query(query) => lint_query_set_ops(query, outer_ctes, violations),
        Statement::Insert(insert) => {
            if let Some(source) = &insert.source {
                lint_query_set_ops(source, outer_ctes, violations);
            }
        }
        Statement::CreateView { query, .. } => lint_query_set_ops(query, outer_ctes, violations),
        Statement::CreateTable(create) => {
            if let Some(query) = &create.query {
                lint_query_set_ops(query, outer_ctes, violations);
            }
        }
        _ => {}
    }
}

fn lint_query_set_ops(query: &Query, outer_ctes: &CteColumnCounts, violations: &mut usize) {
    let ctes = build_query_cte_map(query, outer_ctes);
    lint_set_expr_set_ops(&query.body, &ctes, violations);
}

fn lint_set_expr_set_ops(set_expr: &SetExpr, ctes: &CteColumnCounts, violations: &mut usize) {
    match set_expr {
        SetExpr::SetOperation { left, right, .. } => {
            let stats = collect_set_branch_counts(set_expr, ctes);
            if stats.fully_resolved && stats.counts.len() > 1 {
                *violations += 1;
            }

            lint_set_expr_set_ops(left, ctes, violations);
            lint_set_expr_set_ops(right, ctes, violations);
        }
        SetExpr::Query(query) => lint_query_set_ops(query, ctes, violations),
        SetExpr::Select(select) => lint_select_subqueries_set_ops(select, ctes, violations),
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => lint_statement_set_ops(statement, ctes, violations),
        _ => {}
    }
}

fn lint_select_subqueries_set_ops(select: &Select, ctes: &CteColumnCounts, violations: &mut usize) {
    for table in &select.from {
        lint_table_factor_set_ops(&table.relation, ctes, violations);
        for join in &table.joins {
            lint_table_factor_set_ops(&join.relation, ctes, violations);
        }
    }
}

fn lint_table_factor_set_ops(
    table_factor: &TableFactor,
    ctes: &CteColumnCounts,
    violations: &mut usize,
) {
    match table_factor {
        TableFactor::Derived { subquery, .. } => lint_query_set_ops(subquery, ctes, violations),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            lint_table_factor_set_ops(&table_with_joins.relation, ctes, violations);
            for join in &table_with_joins.joins {
                lint_table_factor_set_ops(&join.relation, ctes, violations);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            lint_table_factor_set_ops(table, ctes, violations)
        }
        _ => {}
    }
}

fn collect_set_branch_counts(set_expr: &SetExpr, ctes: &CteColumnCounts) -> SetCountStats {
    match set_expr {
        SetExpr::SetOperation { left, right, .. } => {
            let left_stats = collect_set_branch_counts(left, ctes);
            let right_stats = collect_set_branch_counts(right, ctes);

            let mut counts = left_stats.counts;
            counts.extend(right_stats.counts);

            SetCountStats {
                counts,
                fully_resolved: left_stats.fully_resolved && right_stats.fully_resolved,
            }
        }
        _ => {
            if let Some(count) = resolve_set_expr_output_columns(set_expr, ctes) {
                let mut counts = HashSet::new();
                counts.insert(count);
                SetCountStats {
                    counts,
                    fully_resolved: true,
                }
            } else {
                SetCountStats {
                    counts: HashSet::new(),
                    fully_resolved: false,
                }
            }
        }
    }
}

fn build_query_cte_map(query: &Query, outer_ctes: &CteColumnCounts) -> CteColumnCounts {
    let mut ctes = outer_ctes.clone();

    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            let cte_name = cte.alias.name.value.to_ascii_uppercase();
            ctes.insert(cte_name.clone(), None);
            let count = resolve_query_output_columns(&cte.query, &ctes);
            ctes.insert(cte_name, count);
        }
    }

    ctes
}

fn resolve_query_output_columns(query: &Query, outer_ctes: &CteColumnCounts) -> Option<usize> {
    let ctes = build_query_cte_map(query, outer_ctes);
    resolve_set_expr_output_columns(&query.body, &ctes)
}

fn resolve_set_expr_output_columns(set_expr: &SetExpr, ctes: &CteColumnCounts) -> Option<usize> {
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

fn resolve_select_output_columns(select: &Select, ctes: &CteColumnCounts) -> Option<usize> {
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

fn collect_select_sources(select: &Select, ctes: &CteColumnCounts) -> Vec<SourceColumns> {
    let mut sources = Vec::new();

    for table in &select.from {
        sources.push(source_columns_for_table_factor(&table.relation, ctes));
        for join in &table.joins {
            sources.push(source_columns_for_table_factor(&join.relation, ctes));
        }
    }

    sources
}

fn source_columns_for_table_factor(
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

    let column_count = match table_factor {
        TableFactor::Table { name, .. } => {
            let key = normalize_identifier(name.to_string());
            ctes.get(&key).copied().flatten()
        }
        TableFactor::Derived { subquery, .. } => resolve_query_output_columns(subquery, ctes),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            source_columns_for_table_factor(table, ctes).column_count
        }
        _ => None,
    };

    SourceColumns {
        names,
        column_count,
    }
}

fn sum_all_source_columns(sources: &[SourceColumns]) -> Option<usize> {
    if sources.is_empty() {
        return None;
    }

    let mut total = 0usize;
    for source in sources {
        total += source.column_count?;
    }

    Some(total)
}

fn resolve_qualified_wildcard_columns(qualifier: &str, sources: &[SourceColumns]) -> Option<usize> {
    let qualifier_upper = normalize_identifier(qualifier.to_string());

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

fn normalize_identifier(raw: String) -> String {
    raw.rsplit('.')
        .next()
        .unwrap_or(&raw)
        .trim_matches('"')
        .trim_matches('`')
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AmbiguousSetColumns;
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

    // --- Edge cases adopted from sqlfluff AM07 ---

    #[test]
    fn flags_known_set_column_count_mismatch() {
        let issues = run("select a from t union all select c, d from k");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_008);
    }

    #[test]
    fn allows_known_set_column_count_match() {
        let issues = run("select a, b from t union all select c, d from k");
        assert!(issues.is_empty());
    }

    #[test]
    fn resolves_cte_wildcard_columns_for_set_comparison() {
        let issues =
            run("with cte as (select a, b from t) select * from cte union select c, d from t2");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_resolved_cte_wildcard_mismatch() {
        let issues =
            run("with cte as (select a, b, c from t) select * from cte union select d, e from t2");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn unresolved_external_wildcard_does_not_trigger() {
        let issues = run("select a from t1 union all select * from t2");
        assert!(issues.is_empty());
    }

    #[test]
    fn resolves_derived_alias_wildcard() {
        let issues = run(
            "select t_alias.* from t2 join (select a from t) as t_alias using (a) union select b from t3",
        );
        assert!(issues.is_empty());
    }
}
