//! LINT_RF_003: References consistency.
//!
//! In single-source queries, avoid mixing qualified and unqualified references.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Select, SelectItem, Statement};

use super::semantic_helpers::{
    count_reference_qualification_in_expr_excluding_aliases, select_projection_alias_set,
    select_source_count, visit_select_expressions, visit_selects_in_statement,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SingleTableReferencesMode {
    Consistent,
    Qualified,
    Unqualified,
}

impl SingleTableReferencesMode {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_RF_003, "single_table_references")
            .unwrap_or("consistent")
            .to_ascii_lowercase()
            .as_str()
        {
            "qualified" => Self::Qualified,
            "unqualified" => Self::Unqualified,
            _ => Self::Consistent,
        }
    }

    fn violation(self, qualified: usize, unqualified: usize) -> bool {
        match self {
            Self::Consistent => qualified > 0 && unqualified > 0,
            Self::Qualified => unqualified > 0,
            Self::Unqualified => qualified > 0,
        }
    }
}

pub struct ReferencesConsistent {
    single_table_references: SingleTableReferencesMode,
    force_enable: bool,
}

impl ReferencesConsistent {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            single_table_references: SingleTableReferencesMode::from_config(config),
            force_enable: config
                .rule_option_bool(issue_codes::LINT_RF_003, "force_enable")
                .unwrap_or(true),
        }
    }
}

impl Default for ReferencesConsistent {
    fn default() -> Self {
        Self {
            single_table_references: SingleTableReferencesMode::Consistent,
            force_enable: true,
        }
    }
}

impl LintRule for ReferencesConsistent {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_003
    }

    fn name(&self) -> &'static str {
        "References consistent"
    }

    fn description(&self) -> &'static str {
        "Avoid mixing qualified and unqualified references."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if !self.force_enable {
            return Vec::new();
        }

        let mut mixed_count = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            if select_source_count(select) != 1 {
                return;
            }

            let aliases = select_projection_alias_set(select);
            let mut qualified = 0usize;
            let mut unqualified = 0usize;

            visit_select_expressions(select, &mut |expr| {
                let (q, u) =
                    count_reference_qualification_in_expr_excluding_aliases(expr, &aliases);
                qualified += q;
                unqualified += u;
            });
            let (projection_qualified, projection_unqualified) =
                projection_wildcard_qualification_counts(select);
            qualified += projection_qualified;
            unqualified += projection_unqualified;

            if self
                .single_table_references
                .violation(qualified, unqualified)
            {
                mixed_count += 1;
            }
        });

        (0..mixed_count)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_RF_003,
                    "Avoid mixing qualified and unqualified references.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn projection_wildcard_qualification_counts(select: &Select) -> (usize, usize) {
    let mut qualified = 0usize;

    for item in &select.projection {
        match item {
            // SQLFluff RF03 parity: treat qualified wildcards as qualified references.
            SelectItem::QualifiedWildcard(_, _) => qualified += 1,
            // Keep unqualified wildcard neutral to avoid forcing `SELECT *` style choices.
            SelectItem::Wildcard(_) => {}
            _ => {}
        }
    }

    (qualified, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesConsistent::default();
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

    // --- Edge cases adopted from sqlfluff RF03 ---

    #[test]
    fn flags_mixed_qualification_single_table() {
        let issues = run("SELECT my_tbl.bar, baz FROM my_tbl");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_003);
    }

    #[test]
    fn allows_consistently_unqualified_references() {
        let issues = run("SELECT bar FROM my_tbl");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_consistently_qualified_references() {
        let issues = run("SELECT my_tbl.bar FROM my_tbl");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_mixed_qualification_in_subquery() {
        let issues = run("SELECT * FROM (SELECT my_tbl.bar, baz FROM my_tbl)");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_consistent_references_in_subquery() {
        let issues = run("SELECT * FROM (SELECT my_tbl.bar FROM my_tbl)");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_mixed_qualification_with_qualified_wildcard() {
        let issues = run("SELECT my_tbl.*, bar FROM my_tbl");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_consistent_qualified_wildcard_and_columns() {
        let issues = run("SELECT my_tbl.*, my_tbl.bar FROM my_tbl");
        assert!(issues.is_empty());
    }

    #[test]
    fn qualified_mode_flags_unqualified_references() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.consistent".to_string(),
                serde_json::json!({"single_table_references": "qualified"}),
            )]),
        };
        let rule = ReferencesConsistent::from_config(&config);
        let sql = "SELECT bar FROM my_tbl";
        let statements = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn force_enable_false_disables_rule() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_RF_003".to_string(),
                serde_json::json!({"force_enable": false}),
            )]),
        };
        let rule = ReferencesConsistent::from_config(&config);
        let sql = "SELECT my_tbl.bar, baz FROM my_tbl";
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
