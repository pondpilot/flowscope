//! LINT_RF_001: References from.
//!
//! Qualified column prefixes should resolve to known FROM/JOIN sources.

use std::collections::HashSet;

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Statement, TableFactor};

use super::semantic_helpers::{
    collect_qualifier_prefixes_in_expr, table_factor_alias_name, visit_select_expressions,
    visit_selects_in_statement,
};

pub struct ReferencesFrom;

impl LintRule for ReferencesFrom {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_001
    }

    fn name(&self) -> &'static str {
        "References from"
    }

    fn description(&self) -> &'static str {
        "Qualified references should resolve to known FROM/JOIN sources."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut unresolved_count = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            let mut known_sources = HashSet::new();
            for table in &select.from {
                register_table_factor_sources(&table.relation, &mut known_sources);
                for join in &table.joins {
                    register_table_factor_sources(&join.relation, &mut known_sources);
                }
            }

            for pseudo in ["EXCLUDED", "INSERTED", "DELETED", "NEW", "OLD"] {
                known_sources.insert(pseudo.to_string());
            }

            if known_sources.is_empty() {
                return;
            }

            let mut qualifier_prefixes = HashSet::new();
            visit_select_expressions(select, &mut |expr| {
                collect_qualifier_prefixes_in_expr(expr, &mut qualifier_prefixes);
            });

            for prefix in qualifier_prefixes {
                if !known_sources.contains(&prefix) {
                    unresolved_count += 1;
                }
            }
        });

        (0..unresolved_count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_RF_001,
                    "Reference prefix appears unresolved from FROM/JOIN sources.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn register_table_factor_sources(table_factor: &TableFactor, known_sources: &mut HashSet<String>) {
    if let Some(alias) = table_factor_alias_name(table_factor) {
        known_sources.insert(alias.to_ascii_uppercase());
    }

    match table_factor {
        TableFactor::Table { name, .. } => {
            let full = name.to_string();
            known_sources.insert(full.to_ascii_uppercase());

            for part in full.split('.') {
                let clean = part.trim_matches('"').trim();
                if !clean.is_empty() {
                    known_sources.insert(clean.to_ascii_uppercase());
                }
            }

            if let Some(last) = full.rsplit('.').next() {
                known_sources.insert(last.trim_matches('"').to_ascii_uppercase());
            }
        }
        TableFactor::Derived { subquery, .. } => {
            if let Some(with) = &subquery.with {
                for cte in &with.cte_tables {
                    known_sources.insert(cte.alias.name.value.to_ascii_uppercase());
                }
            }
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            register_table_factor_sources(&table_with_joins.relation, known_sources);
            for join in &table_with_joins.joins {
                register_table_factor_sources(&join.relation, known_sources);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            register_table_factor_sources(table, known_sources)
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
        let rule = ReferencesFrom;
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

    // --- Edge cases adopted from sqlfluff RF01 ---

    #[test]
    fn flags_unknown_qualifier() {
        let issues = run("SELECT * FROM my_tbl WHERE foo.bar > 0");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_001);
    }

    #[test]
    fn allows_known_table_qualifier() {
        let issues = run("SELECT users.id FROM users");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_nested_subquery_references_that_resolve_locally() {
        let issues = run("SELECT * FROM db.sc.tbl2 WHERE a NOT IN (SELECT a FROM db.sc.tbl1)");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_unresolved_two_part_reference() {
        let issues = run("select * from schema1.agent1 where schema2.agent1.agent_code <> 'abc'");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_simple_delete_statement() {
        let issues = run("delete from table1 where 1 = 1");
        assert!(issues.is_empty());
    }
}
