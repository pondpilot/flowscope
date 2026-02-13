//! LINT_AL_006: Alias length.
//!
//! SQLFluff AL06 parity (current scope): table aliases longer than 30
//! characters are discouraged.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Select, Statement, TableFactor, TableWithJoins};

use super::semantic_helpers::{table_factor_alias_name, visit_selects_in_statement};

const MAX_ALIAS_LENGTH: usize = 30;

pub struct AliasingLength;

impl LintRule for AliasingLength {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_006
    }

    fn name(&self) -> &'static str {
        "Alias length"
    }

    fn description(&self) -> &'static str {
        "Alias names should be readable and not excessively long."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violations = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            violations += overlong_alias_count_in_select(select);
        });

        (0..violations)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_AL_006,
                    "Alias length should not exceed 30 characters.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn overlong_alias_count_in_select(select: &Select) -> usize {
    let mut count = 0usize;

    for table in &select.from {
        count += overlong_alias_count_in_table_with_joins(table);
    }

    count
}

fn overlong_alias_count_in_table_with_joins(table_with_joins: &TableWithJoins) -> usize {
    let mut count = overlong_alias_count_in_table_factor(&table_with_joins.relation);
    for join in &table_with_joins.joins {
        count += overlong_alias_count_in_table_factor(&join.relation);
    }
    count
}

fn overlong_alias_count_in_table_factor(table_factor: &TableFactor) -> usize {
    let mut count = 0usize;

    if table_factor_alias_name(table_factor).is_some_and(|alias| alias.len() > MAX_ALIAS_LENGTH) {
        count += 1;
    }

    match table_factor {
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => count += overlong_alias_count_in_table_with_joins(table_with_joins),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            count += overlong_alias_count_in_table_factor(table)
        }
        _ => {}
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingLength;
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
    fn flags_overlong_table_alias() {
        let issues = run("SELECT * FROM users this_alias_name_is_longer_than_thirty_chars");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_006);
    }

    #[test]
    fn does_not_flag_short_alias() {
        let issues = run("SELECT * FROM users u");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_alias_at_length_limit() {
        let issues = run("SELECT * FROM users aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_nested_select_alias() {
        let issues = run(
            "SELECT * FROM (SELECT * FROM users this_alias_name_is_longer_than_thirty_chars) sub",
        );
        assert_eq!(issues.len(), 1);
    }
}
