//! LINT_AL_006: Alias length.
//!
//! SQLFluff AL06 parity (current scope): table aliases longer than 30
//! characters are discouraged.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Select, Statement, TableFactor, TableWithJoins};

use super::semantic_helpers::{table_factor_alias_name, visit_selects_in_statement};

const DEFAULT_MIN_ALIAS_LENGTH: usize = 0;
const DEFAULT_MAX_ALIAS_LENGTH: Option<usize> = Some(30);

pub struct AliasingLength {
    min_alias_length: usize,
    max_alias_length: Option<usize>,
}

impl AliasingLength {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            min_alias_length: config
                .rule_option_usize(issue_codes::LINT_AL_006, "min_alias_length")
                .unwrap_or(DEFAULT_MIN_ALIAS_LENGTH),
            max_alias_length: config
                .rule_option_usize(issue_codes::LINT_AL_006, "max_alias_length")
                .or(DEFAULT_MAX_ALIAS_LENGTH),
        }
    }
}

impl Default for AliasingLength {
    fn default() -> Self {
        Self {
            min_alias_length: DEFAULT_MIN_ALIAS_LENGTH,
            max_alias_length: DEFAULT_MAX_ALIAS_LENGTH,
        }
    }
}

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
            violations += alias_length_violation_count_in_select(
                select,
                self.min_alias_length,
                self.max_alias_length,
            );
        });

        (0..violations)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_AL_006,
                    "Alias length violates configured bounds.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn alias_length_violation_count_in_select(
    select: &Select,
    min_alias_length: usize,
    max_alias_length: Option<usize>,
) -> usize {
    let mut count = 0usize;

    for table in &select.from {
        count += alias_length_violation_count_in_table_with_joins(
            table,
            min_alias_length,
            max_alias_length,
        );
    }

    count
}

fn alias_length_violation_count_in_table_with_joins(
    table_with_joins: &TableWithJoins,
    min_alias_length: usize,
    max_alias_length: Option<usize>,
) -> usize {
    let mut count = alias_length_violation_count_in_table_factor(
        &table_with_joins.relation,
        min_alias_length,
        max_alias_length,
    );
    for join in &table_with_joins.joins {
        count += alias_length_violation_count_in_table_factor(
            &join.relation,
            min_alias_length,
            max_alias_length,
        );
    }
    count
}

fn alias_length_violation_count_in_table_factor(
    table_factor: &TableFactor,
    min_alias_length: usize,
    max_alias_length: Option<usize>,
) -> usize {
    let mut count = 0usize;

    if table_factor_alias_name(table_factor)
        .is_some_and(|alias| alias_length_violates(alias, min_alias_length, max_alias_length))
    {
        count += 1;
    }

    match table_factor {
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            count += alias_length_violation_count_in_table_with_joins(
                table_with_joins,
                min_alias_length,
                max_alias_length,
            )
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            count += alias_length_violation_count_in_table_factor(
                table,
                min_alias_length,
                max_alias_length,
            )
        }
        _ => {}
    }

    count
}

fn alias_length_violates(
    alias: &str,
    min_alias_length: usize,
    max_alias_length: Option<usize>,
) -> bool {
    let length = alias.len();
    if length < min_alias_length {
        return true;
    }

    if let Some(max_alias_length) = max_alias_length {
        return length > max_alias_length;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingLength::default();
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

    #[test]
    fn applies_max_alias_length_from_config() {
        let statements = parse_sql("SELECT * FROM users eleven_chars").expect("parse");
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_AL_006".to_string(),
                serde_json::json!({"max_alias_length": 10}),
            )]),
        };
        let rule = AliasingLength::from_config(&config);

        let issues = statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check(
                    statement,
                    &LintContext {
                        sql: "SELECT * FROM users eleven_chars",
                        statement_range: 0.."SELECT * FROM users eleven_chars".len(),
                        statement_index: index,
                    },
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn applies_min_alias_length_from_config() {
        let statements = parse_sql("SELECT * FROM users a").expect("parse");
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.length".to_string(),
                serde_json::json!({"min_alias_length": 2}),
            )]),
        };
        let rule = AliasingLength::from_config(&config);

        let issues = statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check(
                    statement,
                    &LintContext {
                        sql: "SELECT * FROM users a",
                        statement_range: 0.."SELECT * FROM users a".len(),
                        statement_index: index,
                    },
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(issues.len(), 1);
    }
}
