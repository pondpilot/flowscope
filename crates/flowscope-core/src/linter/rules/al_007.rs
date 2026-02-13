//! LINT_AL_007: Forbid unnecessary alias.
//!
//! SQLFluff AL07 parity (current scope): single-source SELECT queries should
//! not alias base tables unnecessarily.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Select, Statement, TableFactor};

use super::semantic_helpers::{select_source_count, visit_selects_in_statement};

pub struct AliasingForbidSingleTable;

impl LintRule for AliasingForbidSingleTable {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_007
    }

    fn name(&self) -> &'static str {
        "Forbid unnecessary alias"
    }

    fn description(&self) -> &'static str {
        "Single-table queries should avoid unnecessary aliases."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violations = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            if has_unnecessary_single_table_alias(select) {
                violations += 1;
            }
        });

        (0..violations)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_AL_007,
                    "Avoid unnecessary aliases in single-table queries.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn has_unnecessary_single_table_alias(select: &Select) -> bool {
    if select_source_count(select) != 1 || select.from.len() != 1 {
        return false;
    }

    matches!(
        &select.from[0].relation,
        TableFactor::Table { alias: Some(_), .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingForbidSingleTable;
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
    fn flags_single_table_alias() {
        let issues = run("SELECT * FROM users u");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_007);
    }

    #[test]
    fn does_not_flag_single_table_without_alias() {
        let issues = run("SELECT * FROM users");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_multi_source_query() {
        let issues = run("SELECT * FROM users u JOIN orders o ON u.id = o.user_id");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_derived_table_alias() {
        let issues = run("SELECT * FROM (SELECT 1 AS id) sub");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_nested_single_table_alias() {
        let issues = run("SELECT * FROM (SELECT * FROM users u) sub");
        assert_eq!(issues.len(), 1);
    }
}
