//! LINT_AL_008: Unique column alias.
//!
//! Column aliases should be unique in each SELECT projection.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Query, Select, SelectItem, SetExpr, Statement, TableFactor, TableWithJoins};
use std::collections::HashSet;

pub struct AliasingUniqueColumn;

impl LintRule for AliasingUniqueColumn {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_008
    }

    fn name(&self) -> &'static str {
        "Unique column alias"
    }

    fn description(&self) -> &'static str {
        "Column aliases should be unique in projection lists."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if first_duplicate_column_alias_in_statement(statement).is_none() {
            return Vec::new();
        }

        vec![Issue::warning(
            issue_codes::LINT_AL_008,
            "Column aliases should be unique within SELECT projection.",
        )
        .with_statement(ctx.statement_index)]
    }
}

fn first_duplicate_column_alias_in_statement(statement: &Statement) -> Option<String> {
    match statement {
        Statement::Query(query) => first_duplicate_column_alias_in_query(query),
        Statement::Insert(insert) => insert
            .source
            .as_deref()
            .and_then(first_duplicate_column_alias_in_query),
        Statement::CreateView { query, .. } => first_duplicate_column_alias_in_query(query),
        Statement::CreateTable(create) => create
            .query
            .as_deref()
            .and_then(first_duplicate_column_alias_in_query),
        _ => None,
    }
}

fn first_duplicate_column_alias_in_query(query: &Query) -> Option<String> {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            if let Some(duplicate) = first_duplicate_column_alias_in_query(&cte.query) {
                return Some(duplicate);
            }
        }
    }

    first_duplicate_column_alias_in_set_expr(&query.body)
}

fn first_duplicate_column_alias_in_set_expr(set_expr: &SetExpr) -> Option<String> {
    match set_expr {
        SetExpr::Select(select) => first_duplicate_column_alias_in_select(select),
        SetExpr::Query(query) => first_duplicate_column_alias_in_query(query),
        SetExpr::SetOperation { left, right, .. } => first_duplicate_column_alias_in_set_expr(left)
            .or_else(|| first_duplicate_column_alias_in_set_expr(right)),
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => first_duplicate_column_alias_in_statement(statement),
        _ => None,
    }
}

fn first_duplicate_column_alias_in_select(select: &Select) -> Option<String> {
    let mut aliases = Vec::new();
    for item in &select.projection {
        if let SelectItem::ExprWithAlias { alias, .. } = item {
            aliases.push(alias.value.clone());
        }
    }

    if let Some(duplicate) = first_duplicate_case_insensitive(&aliases) {
        return Some(duplicate);
    }

    for table_with_joins in &select.from {
        if let Some(duplicate) =
            first_duplicate_column_alias_in_table_with_joins_children(table_with_joins)
        {
            return Some(duplicate);
        }
    }

    None
}

fn first_duplicate_column_alias_in_table_with_joins_children(
    table_with_joins: &TableWithJoins,
) -> Option<String> {
    first_duplicate_column_alias_in_table_factor_children(&table_with_joins.relation).or_else(
        || {
            for join in &table_with_joins.joins {
                if let Some(duplicate) =
                    first_duplicate_column_alias_in_table_factor_children(&join.relation)
                {
                    return Some(duplicate);
                }
            }
            None
        },
    )
}

fn first_duplicate_column_alias_in_table_factor_children(
    table_factor: &TableFactor,
) -> Option<String> {
    match table_factor {
        TableFactor::Derived { subquery, .. } => first_duplicate_column_alias_in_query(subquery),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => first_duplicate_column_alias_in_nested_scope(table_with_joins),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            first_duplicate_column_alias_in_table_factor_children(table)
        }
        _ => None,
    }
}

fn first_duplicate_column_alias_in_nested_scope(
    table_with_joins: &TableWithJoins,
) -> Option<String> {
    first_duplicate_column_alias_in_table_with_joins_children(table_with_joins)
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
        let rule = AliasingUniqueColumn;
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
    fn flags_duplicate_projection_alias() {
        let issues = run("select a as x, b as x from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_008);
    }

    #[test]
    fn allows_unique_projection_aliases() {
        let issues = run("select a as x, b as y from t");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_same_alias_in_different_cte_scopes() {
        let sql = "with a as (select col as x from t1), b as (select col as x from t2) select * from a join b on a.x = b.x";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_duplicate_alias_in_nested_subquery() {
        let sql = "select * from (select a as x, b as x from t) s";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
    }
}
