//! LINT_ST_005: Structure subquery.
//!
//! SQLFluff ST05 parity: avoid subqueries in FROM/JOIN clauses; prefer CTEs.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Statement, TableFactor};

use super::semantic_helpers::visit_selects_in_statement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ForbidSubqueryIn {
    Both,
    Join,
    From,
}

impl ForbidSubqueryIn {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_ST_005, "forbid_subquery_in")
            .unwrap_or("join")
            .to_ascii_lowercase()
            .as_str()
        {
            "join" => Self::Join,
            "from" => Self::From,
            _ => Self::Both,
        }
    }

    fn forbid_from(self) -> bool {
        matches!(self, Self::Both | Self::From)
    }

    fn forbid_join(self) -> bool {
        matches!(self, Self::Both | Self::Join)
    }
}

pub struct StructureSubquery {
    forbid_subquery_in: ForbidSubqueryIn,
}

impl StructureSubquery {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            forbid_subquery_in: ForbidSubqueryIn::from_config(config),
        }
    }
}

impl Default for StructureSubquery {
    fn default() -> Self {
        Self {
            forbid_subquery_in: ForbidSubqueryIn::Join,
        }
    }
}

impl LintRule for StructureSubquery {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_005
    }

    fn name(&self) -> &'static str {
        "Structure subquery"
    }

    fn description(&self) -> &'static str {
        "Join/From clauses should not contain subqueries. Use CTEs instead."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violations = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            for table in &select.from {
                if self.forbid_subquery_in.forbid_from()
                    && table_factor_contains_derived(&table.relation)
                {
                    violations += 1;
                }
                if self.forbid_subquery_in.forbid_join() {
                    for join in &table.joins {
                        if table_factor_contains_derived(&join.relation) {
                            violations += 1;
                        }
                    }
                }
            }
        });

        (0..violations)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_ST_005,
                    "Join/From clauses should not contain subqueries. Use CTEs instead.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn table_factor_contains_derived(table_factor: &TableFactor) -> bool {
    match table_factor {
        TableFactor::Derived { .. } => true,
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            table_factor_contains_derived(&table_with_joins.relation)
                || table_with_joins
                    .joins
                    .iter()
                    .any(|join| table_factor_contains_derived(&join.relation))
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => table_factor_contains_derived(table),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::{config::LintConfig, rule::LintContext, Linter};
    use crate::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse sql");
        let linter = Linter::new(LintConfig::default());
        let stmt = &statements[0];
        let ctx = LintContext {
            sql,
            statement_range: 0..sql.len(),
            statement_index: 0,
        };
        linter.check_statement(stmt, &ctx)
    }

    #[test]
    fn default_does_not_flag_subquery_in_from() {
        let issues = run("SELECT * FROM (SELECT * FROM t) sub");
        assert!(!issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn default_flags_subquery_in_join() {
        let issues = run("SELECT * FROM t JOIN (SELECT * FROM u) sub ON t.id = sub.id");
        assert!(issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn does_not_flag_cte_usage() {
        let issues = run("WITH sub AS (SELECT * FROM t) SELECT * FROM sub");
        assert!(!issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn does_not_flag_scalar_subquery_in_where() {
        let issues = run("SELECT * FROM t WHERE id IN (SELECT id FROM u)");
        assert!(!issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn forbid_subquery_in_join_does_not_flag_from_subquery() {
        let sql = "SELECT * FROM (SELECT * FROM t) sub";
        let statements = parse_sql(sql).expect("parse sql");
        let rule = StructureSubquery::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "structure.subquery".to_string(),
                serde_json::json!({"forbid_subquery_in": "join"}),
            )]),
        });
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

    #[test]
    fn forbid_subquery_in_from_does_not_flag_join_subquery() {
        let sql = "SELECT * FROM t JOIN (SELECT * FROM u) sub ON t.id = sub.id";
        let statements = parse_sql(sql).expect("parse sql");
        let rule = StructureSubquery::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_ST_005".to_string(),
                serde_json::json!({"forbid_subquery_in": "from"}),
            )]),
        });
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

    #[test]
    fn forbid_both_flags_subquery_inside_cte_body() {
        let sql = "WITH b AS (SELECT x, z FROM (SELECT x, z FROM p_cte)) SELECT b.z FROM b";
        let statements = parse_sql(sql).expect("parse sql");
        let rule = StructureSubquery::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "structure.subquery".to_string(),
                serde_json::json!({"forbid_subquery_in": "both"}),
            )]),
        });
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
    fn forbid_both_flags_subqueries_in_set_operation_second_branch() {
        let sql = "SELECT 1 AS value_name UNION SELECT value FROM (SELECT 2 AS value_name) CROSS JOIN (SELECT 1 AS v2)";
        let statements = parse_sql(sql).expect("parse sql");
        let rule = StructureSubquery::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "structure.subquery".to_string(),
                serde_json::json!({"forbid_subquery_in": "both"}),
            )]),
        });
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 2);
    }
}
