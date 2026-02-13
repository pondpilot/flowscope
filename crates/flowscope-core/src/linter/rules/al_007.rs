//! LINT_AL_007: Forbid unnecessary alias.
//!
//! SQLFluff AL07 parity: base-table aliases are unnecessary unless they are
//! needed to disambiguate repeated references to the same table (self-joins).

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Select, Statement, TableFactor, TableWithJoins};
use std::collections::HashMap;

use super::semantic_helpers::visit_selects_in_statement;

pub struct AliasingForbidSingleTable {
    force_enable: bool,
}

impl AliasingForbidSingleTable {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            force_enable: config
                .rule_option_bool(issue_codes::LINT_AL_007, "force_enable")
                .unwrap_or(true),
        }
    }
}

impl Default for AliasingForbidSingleTable {
    fn default() -> Self {
        Self { force_enable: true }
    }
}

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
        if !self.force_enable {
            return Vec::new();
        }

        let mut violations = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            violations += unnecessary_table_alias_count(select);
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

#[derive(Clone)]
struct TableAliasCandidate {
    canonical_name: String,
    has_alias: bool,
}

fn unnecessary_table_alias_count(select: &Select) -> usize {
    let mut candidates = Vec::new();
    for table in &select.from {
        collect_table_alias_candidates_from_table_with_joins(table, &mut candidates);
    }

    if candidates.is_empty() {
        return 0;
    }

    let mut table_occurrence_counts: HashMap<String, usize> = HashMap::new();
    for candidate in &candidates {
        *table_occurrence_counts
            .entry(candidate.canonical_name.clone())
            .or_insert(0) += 1;
    }

    let is_multi_source_scope = candidates.len() > 1;

    candidates
        .iter()
        .filter(|candidate| {
            candidate.has_alias
                && (!is_multi_source_scope
                    || table_occurrence_counts
                        .get(&candidate.canonical_name)
                        .copied()
                        .unwrap_or(0)
                        == 1)
        })
        .count()
}

fn collect_table_alias_candidates_from_table_with_joins(
    table: &TableWithJoins,
    candidates: &mut Vec<TableAliasCandidate>,
) {
    collect_table_alias_candidates_from_table_factor(&table.relation, candidates);
    for join in &table.joins {
        collect_table_alias_candidates_from_table_factor(&join.relation, candidates);
    }
}

fn collect_table_alias_candidates_from_table_factor(
    table_factor: &TableFactor,
    candidates: &mut Vec<TableAliasCandidate>,
) {
    match table_factor {
        TableFactor::Table { name, alias, .. } => candidates.push(TableAliasCandidate {
            canonical_name: name.to_string().to_ascii_uppercase(),
            has_alias: alias.is_some(),
        }),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_table_alias_candidates_from_table_with_joins(table_with_joins, candidates);
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_table_alias_candidates_from_table_factor(table, candidates);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingForbidSingleTable::default();
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
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn allows_self_join_aliases() {
        let issues = run("SELECT * FROM users u1 JOIN users u2 ON u1.id = u2.id");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_non_self_join_alias_in_self_join_scope() {
        let issues = run(
            "SELECT * FROM users u1 JOIN users u2 ON u1.id = u2.id JOIN orders o ON o.user_id = u1.id",
        );
        assert_eq!(issues.len(), 1);
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

    #[test]
    fn force_enable_false_disables_rule() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.forbid".to_string(),
                serde_json::json!({"force_enable": false}),
            )]),
        };
        let rule = AliasingForbidSingleTable::from_config(&config);
        let sql = "SELECT * FROM users u";
        let statements = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert!(issues.is_empty());
    }
}
