//! LINT_AL_004: Unique table alias.
//!
//! Table aliases should be unique within a query scope.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Query, Select, SetExpr, Statement, TableFactor, TableWithJoins};
use std::collections::HashSet;

use super::semantic_helpers::table_factor_alias_name;

pub struct AliasingUniqueTable;

impl LintRule for AliasingUniqueTable {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_004
    }

    fn name(&self) -> &'static str {
        "Unique table alias"
    }

    fn description(&self) -> &'static str {
        "Table aliases should be unique within a statement."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if first_duplicate_table_alias_in_statement(statement).is_none() {
            return Vec::new();
        }

        vec![Issue::warning(
            issue_codes::LINT_AL_004,
            "Table aliases should be unique within a statement.",
        )
        .with_statement(ctx.statement_index)]
    }
}

fn first_duplicate_table_alias_in_statement(statement: &Statement) -> Option<String> {
    match statement {
        Statement::Query(query) => first_duplicate_table_alias_in_query(query),
        Statement::Insert(insert) => insert
            .source
            .as_deref()
            .and_then(first_duplicate_table_alias_in_query),
        Statement::CreateView { query, .. } => first_duplicate_table_alias_in_query(query),
        Statement::CreateTable(create) => create
            .query
            .as_deref()
            .and_then(first_duplicate_table_alias_in_query),
        _ => None,
    }
}

fn first_duplicate_table_alias_in_query(query: &Query) -> Option<String> {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            if let Some(duplicate) = first_duplicate_table_alias_in_query(&cte.query) {
                return Some(duplicate);
            }
        }
    }

    first_duplicate_table_alias_in_set_expr(&query.body)
}

fn first_duplicate_table_alias_in_set_expr(set_expr: &SetExpr) -> Option<String> {
    match set_expr {
        SetExpr::Select(select) => first_duplicate_table_alias_in_select(select),
        SetExpr::Query(query) => first_duplicate_table_alias_in_query(query),
        SetExpr::SetOperation { left, right, .. } => first_duplicate_table_alias_in_set_expr(left)
            .or_else(|| first_duplicate_table_alias_in_set_expr(right)),
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => first_duplicate_table_alias_in_statement(statement),
        _ => None,
    }
}

fn first_duplicate_table_alias_in_select(select: &Select) -> Option<String> {
    let mut aliases = Vec::new();
    for table_with_joins in &select.from {
        collect_scope_table_aliases(table_with_joins, &mut aliases);
    }

    if let Some(duplicate) = first_duplicate_case_insensitive(&aliases) {
        return Some(duplicate);
    }

    for table_with_joins in &select.from {
        if let Some(duplicate) =
            first_duplicate_table_alias_in_table_with_joins_children(table_with_joins)
        {
            return Some(duplicate);
        }
    }

    None
}

fn collect_scope_table_aliases(table_with_joins: &TableWithJoins, aliases: &mut Vec<String>) {
    collect_scope_table_aliases_from_factor(&table_with_joins.relation, aliases);
    for join in &table_with_joins.joins {
        collect_scope_table_aliases_from_factor(&join.relation, aliases);
    }
}

fn collect_scope_table_aliases_from_factor(table_factor: &TableFactor, aliases: &mut Vec<String>) {
    if let Some(alias) = inferred_alias_name(table_factor) {
        aliases.push(alias);
    }

    match table_factor {
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => collect_scope_table_aliases(table_with_joins, aliases),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_scope_table_aliases_from_factor(table, aliases)
        }
        _ => {}
    }
}

fn inferred_alias_name(table_factor: &TableFactor) -> Option<String> {
    if let Some(alias) = table_factor_alias_name(table_factor) {
        return Some(alias.to_string());
    }

    match table_factor {
        TableFactor::Table { name, .. } => name.0.last().map(|part| {
            part.as_ident()
                .map(|ident| ident.value.clone())
                .unwrap_or_else(|| part.to_string())
        }),
        _ => None,
    }
}

fn first_duplicate_table_alias_in_table_with_joins_children(
    table_with_joins: &TableWithJoins,
) -> Option<String> {
    first_duplicate_table_alias_in_table_factor_children(&table_with_joins.relation).or_else(|| {
        for join in &table_with_joins.joins {
            if let Some(duplicate) =
                first_duplicate_table_alias_in_table_factor_children(&join.relation)
            {
                return Some(duplicate);
            }
        }
        None
    })
}

fn first_duplicate_table_alias_in_table_factor_children(
    table_factor: &TableFactor,
) -> Option<String> {
    match table_factor {
        TableFactor::Derived { subquery, .. } => first_duplicate_table_alias_in_query(subquery),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => first_duplicate_table_alias_in_nested_scope(table_with_joins),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            first_duplicate_table_alias_in_table_factor_children(table)
        }
        _ => None,
    }
}

fn first_duplicate_table_alias_in_nested_scope(
    table_with_joins: &TableWithJoins,
) -> Option<String> {
    let mut aliases = Vec::new();
    collect_scope_table_aliases(table_with_joins, &mut aliases);
    if let Some(duplicate) = first_duplicate_case_insensitive(&aliases) {
        return Some(duplicate);
    }

    first_duplicate_table_alias_in_table_with_joins_children(table_with_joins)
}

fn first_duplicate_case_insensitive(values: &[String]) -> Option<String> {
    let mut seen = HashSet::new();
    for value in values {
        let key = value.to_ascii_uppercase();
        if !seen.insert(key) {
            return Some(value.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingUniqueTable;
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

    #[test]
    fn flags_duplicate_alias_in_same_scope() {
        let issues = run("select * from users u join orders u on u.id = u.user_id");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_004);
    }

    #[test]
    fn allows_unique_aliases() {
        let issues = run("select * from users u join orders o on u.id = o.user_id");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_same_alias_in_separate_cte_scopes() {
        let sql = "with a as (select * from users u), b as (select * from orders u) select * from a join b on a.id = b.id";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_duplicate_alias_in_nested_subquery() {
        let sql = "select * from (select * from users u join orders u on u.id = u.user_id) t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_duplicate_implicit_table_name_aliases() {
        let sql =
            "select * from analytics.foo join reporting.foo on analytics.foo.id = reporting.foo.id";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_004);
    }
}
