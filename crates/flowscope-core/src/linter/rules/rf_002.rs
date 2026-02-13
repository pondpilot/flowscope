//! LINT_RF_002: References qualification.
//!
//! In multi-table queries, require qualified column references.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

use super::semantic_helpers::{
    count_reference_qualification_in_expr_excluding_aliases, select_projection_alias_set,
    select_source_count, visit_select_expressions, visit_selects_in_statement,
};

pub struct ReferencesQualification {
    force_enable: bool,
}

impl ReferencesQualification {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            force_enable: config
                .rule_option_bool(issue_codes::LINT_RF_002, "force_enable")
                .unwrap_or(true),
        }
    }
}

impl Default for ReferencesQualification {
    fn default() -> Self {
        Self { force_enable: true }
    }
}

impl LintRule for ReferencesQualification {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_002
    }

    fn name(&self) -> &'static str {
        "References qualification"
    }

    fn description(&self) -> &'static str {
        "Use qualification consistently in multi-table queries."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if !self.force_enable {
            return Vec::new();
        }

        let mut unqualified_count = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            if select_source_count(select) <= 1 {
                return;
            }

            let aliases = select_projection_alias_set(select);
            visit_select_expressions(select, &mut |expr| {
                let (_, unqualified) =
                    count_reference_qualification_in_expr_excluding_aliases(expr, &aliases);
                unqualified_count += unqualified;
            });
        });

        (0..unqualified_count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_RF_002,
                    "Use qualified references in multi-table queries.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesQualification::default();
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

    // --- Edge cases adopted from sqlfluff RF02 ---

    #[test]
    fn allows_fully_qualified_multi_table_query() {
        let issues = run("SELECT foo.a, vee.b FROM foo LEFT JOIN vee ON vee.a = foo.a");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_unqualified_multi_table_query() {
        let issues = run("SELECT a, b FROM foo LEFT JOIN vee ON vee.a = foo.a");
        assert!(!issues.is_empty());
        assert!(issues
            .iter()
            .all(|issue| issue.code == issue_codes::LINT_RF_002));
    }

    #[test]
    fn allows_qualified_multi_table_query_inside_subquery() {
        let issues =
            run("SELECT a FROM (SELECT foo.a, vee.b FROM foo LEFT JOIN vee ON vee.a = foo.a)");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_unqualified_multi_table_query_inside_subquery() {
        let issues = run("SELECT a FROM (SELECT a, b FROM foo LEFT JOIN vee ON vee.a = foo.a)");
        assert!(!issues.is_empty());
    }

    #[test]
    fn force_enable_false_disables_rule() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_RF_002".to_string(),
                serde_json::json!({"force_enable": false}),
            )]),
        };
        let rule = ReferencesQualification::from_config(&config);
        let sql = "SELECT a, b FROM foo LEFT JOIN vee ON vee.a = foo.a";
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
