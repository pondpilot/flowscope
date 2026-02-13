//! Shared helpers for quoted-identifier reference rules.

use std::collections::BTreeSet;

use crate::linter::visit::visit_expressions;
use sqlparser::ast::{Expr, SelectItem, Statement, TableAlias, TableFactor};

use super::semantic_helpers::visit_selects_in_statement;

pub(crate) fn quoted_identifiers_in_statement(statement: &Statement) -> Vec<String> {
    let mut quoted = BTreeSet::new();

    visit_expressions(statement, &mut |expr| {
        collect_quoted_identifiers_in_expr(expr, &mut quoted);
    });

    visit_selects_in_statement(statement, &mut |select| {
        for item in &select.projection {
            if let SelectItem::ExprWithAlias { alias, .. } = item {
                collect_quoted_ident(alias, &mut quoted);
            }
        }

        for table in &select.from {
            collect_quoted_identifiers_in_table_factor(&table.relation, &mut quoted);
            for join in &table.joins {
                collect_quoted_identifiers_in_table_factor(&join.relation, &mut quoted);
            }
        }
    });

    quoted.into_iter().collect()
}

fn collect_quoted_identifiers_in_expr(expr: &Expr, quoted: &mut BTreeSet<String>) {
    match expr {
        Expr::Identifier(ident) => collect_quoted_ident(ident, quoted),
        Expr::CompoundIdentifier(parts) => {
            for part in parts {
                collect_quoted_ident(part, quoted);
            }
        }
        _ => {}
    }
}

fn collect_quoted_identifiers_in_table_factor(
    table_factor: &TableFactor,
    quoted: &mut BTreeSet<String>,
) {
    if let Some(alias) = table_factor_alias(table_factor) {
        collect_quoted_ident(&alias.name, quoted);
    }

    match table_factor {
        TableFactor::Table { name, .. } => {
            for part in name.to_string().split('.') {
                let trimmed = part.trim();
                if let Some(unquoted) = strip_quoted_part(trimmed) {
                    quoted.insert(unquoted.to_string());
                }
            }
        }
        TableFactor::Derived { .. }
        | TableFactor::TableFunction { .. }
        | TableFactor::Function { .. }
        | TableFactor::UNNEST { .. }
        | TableFactor::JsonTable { .. }
        | TableFactor::OpenJsonTable { .. }
        | TableFactor::XmlTable { .. }
        | TableFactor::SemanticView { .. } => {}
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_quoted_identifiers_in_table_factor(&table_with_joins.relation, quoted);
            for join in &table_with_joins.joins {
                collect_quoted_identifiers_in_table_factor(&join.relation, quoted);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_quoted_identifiers_in_table_factor(table, quoted);
        }
    }
}

fn collect_quoted_ident(ident: &sqlparser::ast::Ident, quoted: &mut BTreeSet<String>) {
    if ident.quote_style.is_some() {
        quoted.insert(ident.value.clone());
    }
}

fn strip_quoted_part(part: &str) -> Option<&str> {
    if part.len() >= 2
        && ((part.starts_with('"') && part.ends_with('"'))
            || (part.starts_with('`') && part.ends_with('`')))
    {
        return Some(&part[1..part.len() - 1]);
    }

    if part.len() >= 3 && part.starts_with('[') && part.ends_with(']') {
        return Some(&part[1..part.len() - 1]);
    }

    None
}

fn table_factor_alias(table_factor: &TableFactor) -> Option<&TableAlias> {
    match table_factor {
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
    }
}
