//! LINT_AM_007: Ambiguous set-operation columns.
//!
//! SQLFluff AM07 parity: set-operation branches should resolve to the same
//! number of output columns when wildcard expansion is deterministically known.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Query, Select, SetExpr, Statement, TableFactor};
use std::collections::{HashMap, HashSet};

use super::column_count_helpers::{
    build_query_cte_map, resolve_set_expr_output_columns, CteColumnCounts,
};

pub struct AmbiguousSetColumns;

#[derive(Default)]
struct SetCountStats {
    counts: HashSet<usize>,
    fully_resolved: bool,
}

impl LintRule for AmbiguousSetColumns {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_007
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
                    issue_codes::LINT_AM_007,
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
        assert_eq!(issues[0].code, issue_codes::LINT_AM_007);
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

    #[test]
    fn resolves_nested_with_wildcard_for_set_comparison() {
        let issues = run(
            "SELECT * FROM (WITH cte2 AS (SELECT a, b FROM table2) SELECT * FROM cte2 as cte_al) UNION SELECT e, f FROM table3",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_nested_with_wildcard_mismatch_for_set_comparison() {
        let issues = run(
            "SELECT * FROM (WITH cte2 AS (SELECT a FROM table2) SELECT * FROM cte2 as cte_al) UNION SELECT e, f FROM table3",
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_007);
    }

    #[test]
    fn resolves_nested_cte_chain_for_set_comparison() {
        let issues = run(
            "with a as (with b as (select 1 from c) select * from b) select * from a union all select k from t2",
        );
        assert!(issues.is_empty());
    }
}
