//! Shared helpers for quoted-identifier reference rules.

use std::collections::BTreeSet;

use crate::linter::visit::visit_expressions;
use sqlparser::ast::{Expr, SelectItem, Statement, TableAlias, TableFactor};

use super::semantic_helpers::visit_selects_in_statement;

pub(crate) fn quoted_identifiers_in_statement(statement: &Statement) -> Vec<String> {
    collect_quoted_identifiers(statement, QuoteFilter::Any)
}

pub(crate) fn double_quoted_identifiers_in_statement(statement: &Statement) -> Vec<String> {
    collect_quoted_identifiers(statement, QuoteFilter::DoubleOnly)
}

#[derive(Clone, Copy)]
enum QuoteFilter {
    Any,
    DoubleOnly,
}

fn collect_quoted_identifiers(statement: &Statement, filter: QuoteFilter) -> Vec<String> {
    let mut quoted = BTreeSet::new();

    visit_expressions(statement, &mut |expr| {
        collect_quoted_identifiers_in_expr(expr, &mut quoted, filter);
    });

    visit_selects_in_statement(statement, &mut |select| {
        for item in &select.projection {
            if let SelectItem::ExprWithAlias { alias, .. } = item {
                collect_quoted_ident(alias, &mut quoted, filter);
            }
        }

        for table in &select.from {
            collect_quoted_identifiers_in_table_factor(&table.relation, &mut quoted, filter);
            for join in &table.joins {
                collect_quoted_identifiers_in_table_factor(&join.relation, &mut quoted, filter);
            }
        }
    });

    quoted.into_iter().collect()
}

fn collect_quoted_identifiers_in_expr(
    expr: &Expr,
    quoted: &mut BTreeSet<String>,
    filter: QuoteFilter,
) {
    match expr {
        Expr::Identifier(ident) => collect_quoted_ident(ident, quoted, filter),
        Expr::CompoundIdentifier(parts) => {
            for part in parts {
                collect_quoted_ident(part, quoted, filter);
            }
        }
        _ => {}
    }
}

fn collect_quoted_identifiers_in_table_factor(
    table_factor: &TableFactor,
    quoted: &mut BTreeSet<String>,
    filter: QuoteFilter,
) {
    if let Some(alias) = table_factor_alias(table_factor) {
        collect_quoted_ident(&alias.name, quoted, filter);
    }

    match table_factor {
        TableFactor::Table { name, .. } => {
            for part in name.to_string().split('.') {
                let trimmed = part.trim();
                if let Some(unquoted) = strip_quoted_part(trimmed, filter) {
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
            collect_quoted_identifiers_in_table_factor(&table_with_joins.relation, quoted, filter);
            for join in &table_with_joins.joins {
                collect_quoted_identifiers_in_table_factor(&join.relation, quoted, filter);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_quoted_identifiers_in_table_factor(table, quoted, filter);
        }
    }
}

fn collect_quoted_ident(
    ident: &sqlparser::ast::Ident,
    quoted: &mut BTreeSet<String>,
    filter: QuoteFilter,
) {
    if matches_quote_style(ident.quote_style, filter) {
        quoted.insert(ident.value.clone());
    }
}

fn strip_quoted_part(part: &str, filter: QuoteFilter) -> Option<&str> {
    if part.len() >= 2 && part.starts_with('"') && part.ends_with('"') {
        if matches_quote_style(Some('"'), filter) {
            return Some(&part[1..part.len() - 1]);
        }
        return None;
    }

    if part.len() >= 2 && part.starts_with('`') && part.ends_with('`') {
        if matches_quote_style(Some('`'), filter) {
            return Some(&part[1..part.len() - 1]);
        }
        return None;
    }

    if part.len() >= 3 && part.starts_with('[') && part.ends_with(']') {
        if matches_quote_style(Some('['), filter) {
            return Some(&part[1..part.len() - 1]);
        }
        return None;
    }

    None
}

fn matches_quote_style(style: Option<char>, filter: QuoteFilter) -> bool {
    match filter {
        QuoteFilter::Any => style.is_some(),
        QuoteFilter::DoubleOnly => matches!(style, Some('"')),
    }
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
