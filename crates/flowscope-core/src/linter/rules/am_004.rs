//! LINT_AM_004: Ambiguous column count (SQLFluff AM04 parity).
//!
//! Flags queries whose output width is not deterministically known, usually due
//! to unresolved wildcard projections (`*` / `alias.*`).

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use std::collections::HashMap;

use super::column_count_helpers::{resolve_query_output_columns_strict, CteColumnCounts};

pub struct AmbiguousColumnCount;

impl LintRule for AmbiguousColumnCount {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_004
    }

    fn name(&self) -> &'static str {
        "Ambiguous column count"
    }

    fn description(&self) -> &'static str {
        "Query should produce a known number of result columns."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if statement_has_unknown_result_columns(stmt, &HashMap::new()) {
            vec![Issue::warning(
                issue_codes::LINT_AM_004,
                "Query produces an unknown number of result columns.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn statement_has_unknown_result_columns(stmt: &Statement, outer_ctes: &CteColumnCounts) -> bool {
    match stmt {
        Statement::Query(query) => resolve_query_output_columns_strict(query, outer_ctes).is_none(),
        Statement::Insert(insert) => insert.source.as_ref().is_some_and(|source| {
            resolve_query_output_columns_strict(source, outer_ctes).is_none()
        }),
        Statement::CreateView { query, .. } => {
            resolve_query_output_columns_strict(query, outer_ctes).is_none()
        }
        Statement::CreateTable(create) => create
            .query
            .as_ref()
            .is_some_and(|query| resolve_query_output_columns_strict(query, outer_ctes).is_none()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AmbiguousColumnCount;
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

    // --- Edge cases adopted from sqlfluff AM04 ---

    #[test]
    fn flags_unknown_result_columns_for_select_star_from_table() {
        let issues = run("select * from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_004);
    }

    #[test]
    fn allows_known_result_columns_for_explicit_projection() {
        let issues = run("select a, b from t");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_select_star_from_known_cte_columns() {
        let issues = run("with cte as (select a, b from t) select * from cte");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_select_star_from_declared_cte_columns_even_if_query_uses_wildcard() {
        let issues = run("with cte(a, b) as (select * from t) select * from cte");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_select_star_from_unknown_cte_columns() {
        let issues = run("with cte as (select * from t) select * from cte");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_explicit_projection_even_if_cte_uses_wildcard() {
        let issues = run("with cte as (select * from t) select a, b from cte");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_qualified_wildcard_from_external_source() {
        let issues = run("select t.* from t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_qualified_wildcard_from_known_derived_alias() {
        let issues = run("select t_alias.* from (select a from t) as t_alias");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_qualified_wildcard_from_declared_derived_alias_columns() {
        let issues = run("select t_alias.* from (select * from t) as t_alias(a, b)");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_qualified_wildcard_from_known_nested_join_alias() {
        let issues = run(
            "select j.* from ((select a from t1) as a1 join (select b from t2) as b1 on a1.a = b1.b) as j",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_qualified_wildcard_from_unknown_derived_alias() {
        let issues = run("select t_alias.* from (select * from t) as t_alias");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_nested_join_wildcard_when_using_width_is_resolved() {
        let issues =
            run("select j.* from ((select a from t1) as a1 join (select a from t2) as b1 using (a)) as j");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_nested_join_wildcard_when_natural_join_width_is_resolved() {
        let issues = run(
            "select j.* from ((select a from t1) as a1 natural join (select a from t2) as b1) as j",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_nested_join_wildcard_when_natural_join_width_is_unknown() {
        let issues = run(
            "select j.* from ((select * from t1) as a1 natural join (select a from t2) as b1) as j",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_any_unknown_wildcard_in_projection() {
        let issues = run("select *, t.*, t.a, b from t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_set_operation_with_unknown_wildcard_branch() {
        let issues = run("select a from t1 union all select * from t2");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_set_operation_with_known_columns() {
        let issues = run("select a from t1 union all select b from t2");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_nested_cte_unknown_column_chain() {
        let issues = run("with a as (with b as (select * from c) select * from b) select * from a");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_non_select_statement_without_query_body() {
        let issues = run("create table my_table (id integer)");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_select_star_without_from_source() {
        let issues = run("select *");
        assert_eq!(issues.len(), 1);
    }
}
