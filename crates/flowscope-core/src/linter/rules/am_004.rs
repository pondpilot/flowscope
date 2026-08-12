//! LINT_AM_004: Ambiguous column count (SQLFluff AM04 parity).
//!
//! Flags queries whose output width is not deterministically known, usually due
//! to unresolved wildcard projections (`*` / `alias.*`).

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{CreateView, Expr, SelectItem, SetExpr, Statement, Value};
use std::collections::HashMap;

use super::column_count_helpers::{resolve_query_output_columns_strict, CteColumnCounts};

pub struct AmbiguousColumnCount;

impl BuiltinLintRule for AmbiguousColumnCount {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_004
    }

    fn name(&self) -> &'static str {
        "Ambiguous column count"
    }

    fn description(&self) -> &'static str {
        "Query produces an unknown number of result columns."
    }

    fn check_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let ast_unknown = statement_has_unknown_result_columns(stmt, &HashMap::new());
        let fallback_unknown = !ast_unknown
            && statement_has_unknown_result_columns_fallback(stmt, ctx.statement_sql());

        if ast_unknown || fallback_unknown {
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
        Statement::Query(query) => {
            query_outputs_result_set(query)
                && resolve_query_output_columns_strict(query, outer_ctes).is_none()
        }
        Statement::Insert(insert) => insert.source.as_ref().is_some_and(|source| {
            resolve_query_output_columns_strict(source, outer_ctes).is_none()
        }),
        Statement::CreateView(CreateView { query, .. }) => {
            query_outputs_result_set(query)
                && resolve_query_output_columns_strict(query, outer_ctes).is_none()
        }
        Statement::CreateTable(create) => create.query.as_ref().is_some_and(|query| {
            query_outputs_result_set(query)
                && resolve_query_output_columns_strict(query, outer_ctes).is_none()
        }),
        _ => false,
    }
}

fn query_outputs_result_set(query: &sqlparser::ast::Query) -> bool {
    matches!(
        query.body.as_ref(),
        SetExpr::Select(_) | SetExpr::Query(_) | SetExpr::Values(_) | SetExpr::SetOperation { .. }
    )
}

fn statement_has_unknown_result_columns_fallback(stmt: &Statement, sql: &str) -> bool {
    if !is_synthetic_select_one_statement(stmt, sql) {
        return false;
    }

    // Targeted parser-fallback heuristic for AM04 issue #930-style CTE chains:
    // WITH a AS (SELECT * FROM ...), b AS (SELECT * FROM a) SELECT * FROM b
    // should be treated as unknown-width output.
    let compact = sql
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    if !compact.starts_with("with ") {
        return false;
    }

    let wildcard_count = compact.match_indices("select * from").count();
    wildcard_count >= 3 && compact.contains("),") && compact.contains(" as (")
}

fn is_synthetic_select_one_statement(stmt: &Statement, statement_sql: &str) -> bool {
    if statement_sql.trim().eq_ignore_ascii_case("select 1") {
        return false;
    }

    let Statement::Query(query) = stmt else {
        return false;
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        return false;
    };
    let group_by_empty = match &select.group_by {
        sqlparser::ast::GroupByExpr::Expressions(exprs, _) => exprs.is_empty(),
        _ => false,
    };
    if !select.from.is_empty()
        || select.selection.is_some()
        || !group_by_empty
        || select.having.is_some()
    {
        return false;
    }
    if select.projection.len() != 1 {
        return false;
    }

    matches!(
        &select.projection[0],
        SelectItem::UnnamedExpr(Expr::Value(v))
            if matches!(&v.value, Value::Number(num, false) if num == "1")
    )
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
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
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

    #[test]
    fn allows_unresolved_qualified_wildcard_for_non_am04_concerns() {
        let sql = "with cte as (\n    select\n        a, b\n    from\n        t\n)\nselect\n    cte.*,\n    t_alias.a\nfrom cte1\njoin (select * from t) as t_alias\nusing (a)\n";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_with_update_statement_without_select_output() {
        let sql = "WITH mycte AS ( SELECT foo, bar FROM mytable1 )\nUPDATE sometable SET sometable.baz = mycte.bar FROM mycte;";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn statementless_fallback_flags_unknown_cte_wildcard_chain() {
        let sql = "with\nhubspot__contacts as (\n  select * from ANALYTICS.PUBLIC_intermediate.hubspot__contacts\n),\nfinal as (\n  select *\n  from\n    hubspot__contacts\n    where not coalesce(_fivetran_deleted, false)\n)\nselect * from final\n";
        let synthetic = parse_sql("SELECT 1").expect("parse");
        let rule = AmbiguousColumnCount;
        let issues =
            rule.check_with_context(&synthetic[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_004);
    }
}
