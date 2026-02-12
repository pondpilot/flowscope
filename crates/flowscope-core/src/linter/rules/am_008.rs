//! LINT_AM_008: Ambiguous set-operation columns.
//!
//! Avoid wildcard projections in set operations (`UNION`/`INTERSECT`/`EXCEPT`).

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{SelectItem, SetExpr, Statement};

pub struct AmbiguousSetColumns;

impl LintRule for AmbiguousSetColumns {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_008
    }

    fn name(&self) -> &'static str {
        "Ambiguous set columns"
    }

    fn description(&self) -> &'static str {
        "Avoid wildcard projections in set operations."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if statement_has_wildcard_set_operation(statement) {
            vec![Issue::warning(
                issue_codes::LINT_AM_008,
                "Avoid wildcard projections in UNION/INTERSECT/EXCEPT branches.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn statement_has_wildcard_set_operation(statement: &Statement) -> bool {
    match statement {
        Statement::Query(query) => set_expr_has_wildcard_set_operation(&query.body),
        Statement::Insert(insert) => insert
            .source
            .as_deref()
            .is_some_and(|query| set_expr_has_wildcard_set_operation(&query.body)),
        Statement::CreateView { query, .. } => set_expr_has_wildcard_set_operation(&query.body),
        Statement::CreateTable(create) => create
            .query
            .as_deref()
            .is_some_and(|query| set_expr_has_wildcard_set_operation(&query.body)),
        _ => false,
    }
}

fn set_expr_has_wildcard_set_operation(set_expr: &SetExpr) -> bool {
    match set_expr {
        SetExpr::SetOperation { left, right, .. } => {
            branch_has_wildcard(left)
                || branch_has_wildcard(right)
                || set_expr_has_wildcard_set_operation(left)
                || set_expr_has_wildcard_set_operation(right)
        }
        SetExpr::Query(query) => set_expr_has_wildcard_set_operation(&query.body),
        _ => false,
    }
}

fn branch_has_wildcard(set_expr: &SetExpr) -> bool {
    match set_expr {
        SetExpr::Select(select) => select.projection.iter().any(|item| {
            matches!(
                item,
                SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _)
            )
        }),
        SetExpr::Query(query) => branch_has_wildcard(&query.body),
        SetExpr::SetOperation { left, right, .. } => {
            branch_has_wildcard(left) || branch_has_wildcard(right)
        }
        _ => false,
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

    #[test]
    fn flags_wildcard_in_set_operation() {
        let issues = run("SELECT * FROM a UNION SELECT * FROM b");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_008);
    }

    #[test]
    fn allows_explicit_column_set_operation() {
        let issues = run("SELECT a FROM a UNION SELECT b FROM b");
        assert!(issues.is_empty());
    }

    #[test]
    fn ignores_wildcard_outside_set_operations() {
        let issues = run("SELECT * FROM a");
        assert!(issues.is_empty());
    }
}
