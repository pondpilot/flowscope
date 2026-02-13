//! LINT_RF_004: References keywords.
//!
//! SQLFluff RF04 parity (current scope): avoid keyword-looking identifiers in
//! expression references, aliases, CTE names/columns, and table-name parts.

use crate::extractors::extract_tables;
use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit::visit_expressions;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Expr, Query, SelectItem, SetExpr, Statement};

use super::semantic_helpers::{table_factor_alias_name, visit_selects_in_statement};

pub struct ReferencesKeywords;

impl LintRule for ReferencesKeywords {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_004
    }

    fn name(&self) -> &'static str {
        "References keywords"
    }

    fn description(&self) -> &'static str {
        "Avoid keywords as identifiers."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if statement_contains_keyword_identifier(statement) {
            vec![Issue::info(
                issue_codes::LINT_RF_004,
                "Keyword used as identifier alias.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn statement_contains_keyword_identifier(statement: &Statement) -> bool {
    if extract_tables(std::slice::from_ref(statement))
        .into_iter()
        .any(|name| name.split('.').any(is_keyword))
    {
        return true;
    }

    if statement_contains_keyword_cte_identifier(statement) {
        return true;
    }

    let mut found = false;
    visit_expressions(statement, &mut |expr| {
        if found {
            return;
        }
        if expr_contains_keyword_identifier(expr) {
            found = true;
        }
    });
    if found {
        return true;
    }

    visit_selects_in_statement(statement, &mut |select| {
        if found {
            return;
        }

        for item in &select.projection {
            if let SelectItem::ExprWithAlias { alias, .. } = item {
                if is_keyword(&alias.value) {
                    found = true;
                    return;
                }
            }
        }

        for table in &select.from {
            if table_factor_alias_name(&table.relation).is_some_and(is_keyword) {
                found = true;
                return;
            }

            for join in &table.joins {
                if table_factor_alias_name(&join.relation).is_some_and(is_keyword) {
                    found = true;
                    return;
                }
            }
        }
    });

    found
}

fn statement_contains_keyword_cte_identifier(statement: &Statement) -> bool {
    match statement {
        Statement::Query(query) => query_contains_keyword_cte_identifier(query),
        Statement::Insert(insert) => insert
            .source
            .as_ref()
            .map(Box::as_ref)
            .is_some_and(query_contains_keyword_cte_identifier),
        Statement::CreateView { query, .. } => query_contains_keyword_cte_identifier(query),
        Statement::CreateTable(create) => create
            .query
            .as_ref()
            .map(Box::as_ref)
            .is_some_and(query_contains_keyword_cte_identifier),
        _ => false,
    }
}

fn query_contains_keyword_cte_identifier(query: &Query) -> bool {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            if is_keyword(&cte.alias.name.value)
                || cte
                    .alias
                    .columns
                    .iter()
                    .any(|column| is_keyword(&column.name.value))
                || query_contains_keyword_cte_identifier(&cte.query)
            {
                return true;
            }
        }
    }

    set_expr_contains_keyword_cte_identifier(&query.body)
}

fn set_expr_contains_keyword_cte_identifier(set_expr: &SetExpr) -> bool {
    match set_expr {
        SetExpr::Query(query) => query_contains_keyword_cte_identifier(query),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_contains_keyword_cte_identifier(left)
                || set_expr_contains_keyword_cte_identifier(right)
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => statement_contains_keyword_cte_identifier(statement),
        _ => false,
    }
}

fn expr_contains_keyword_identifier(expr: &Expr) -> bool {
    match expr {
        Expr::Identifier(ident) => is_keyword(&ident.value),
        Expr::CompoundIdentifier(parts) => parts.iter().any(|part| is_keyword(&part.value)),
        _ => false,
    }
}

fn is_keyword(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_uppercase().as_str(),
        "ALL"
            | "AS"
            | "BY"
            | "CASE"
            | "CROSS"
            | "DISTINCT"
            | "ELSE"
            | "END"
            | "FROM"
            | "FULL"
            | "GROUP"
            | "HAVING"
            | "INNER"
            | "JOIN"
            | "LEFT"
            | "LIMIT"
            | "OFFSET"
            | "ON"
            | "ORDER"
            | "OUTER"
            | "RECURSIVE"
            | "RIGHT"
            | "SELECT"
            | "THEN"
            | "UNION"
            | "USING"
            | "WHEN"
            | "WHERE"
            | "WITH"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesKeywords;
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
    fn flags_keyword_table_alias() {
        let issues = run("SELECT \"select\".id FROM users AS \"select\"");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn flags_keyword_projection_alias() {
        let issues = run("SELECT amount AS \"from\" FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn flags_keyword_identifier_reference() {
        let issues = run("SELECT \"group\" FROM users");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn flags_keyword_cte_alias_and_columns() {
        let issues = run("WITH \"select\"(\"from\") AS (SELECT 1) SELECT \"from\" FROM \"select\"");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn does_not_flag_non_keyword_alias() {
        let issues = run("SELECT u.id FROM users AS u");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_sql_like_string_literal() {
        let issues = run("SELECT 'FROM users AS date' AS snippet");
        assert!(issues.is_empty());
    }
}
