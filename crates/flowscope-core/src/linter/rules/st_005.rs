//! LINT_ST_005: Structure subquery.
//!
//! SQLFluff ST05 parity: avoid subqueries in FROM/JOIN clauses; prefer CTEs.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Query, Select, SetExpr, Statement, TableFactor};
use std::collections::HashSet;

use super::semantic_helpers::{
    collect_qualifier_prefixes_in_expr, visit_select_expressions, visit_selects_in_statement,
};

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
            let outer_source_names = source_names_in_select(select);
            for table in &select.from {
                if self.forbid_subquery_in.forbid_from()
                    && table_factor_contains_derived(&table.relation, &outer_source_names)
                {
                    violations += 1;
                }
                if self.forbid_subquery_in.forbid_join() {
                    for join in &table.joins {
                        if table_factor_contains_derived(&join.relation, &outer_source_names) {
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

fn table_factor_contains_derived(
    table_factor: &TableFactor,
    outer_source_names: &HashSet<String>,
) -> bool {
    match table_factor {
        TableFactor::Derived { subquery, .. } => {
            !query_references_outer_sources(subquery, outer_source_names)
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            table_factor_contains_derived(&table_with_joins.relation, outer_source_names)
                || table_with_joins
                    .joins
                    .iter()
                    .any(|join| table_factor_contains_derived(&join.relation, outer_source_names))
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            table_factor_contains_derived(table, outer_source_names)
        }
        _ => false,
    }
}

fn query_references_outer_sources(query: &Query, outer_source_names: &HashSet<String>) -> bool {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            if query_references_outer_sources(&cte.query, outer_source_names) {
                return true;
            }
        }
    }

    set_expr_references_outer_sources(&query.body, outer_source_names)
}

fn set_expr_references_outer_sources(
    set_expr: &SetExpr,
    outer_source_names: &HashSet<String>,
) -> bool {
    match set_expr {
        SetExpr::Select(select) => select_references_outer_sources(select, outer_source_names),
        SetExpr::Query(query) => query_references_outer_sources(query, outer_source_names),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_references_outer_sources(left, outer_source_names)
                || set_expr_references_outer_sources(right, outer_source_names)
        }
        _ => false,
    }
}

fn select_references_outer_sources(select: &Select, outer_source_names: &HashSet<String>) -> bool {
    let mut qualifier_prefixes = HashSet::new();
    visit_select_expressions(select, &mut |expr| {
        collect_qualifier_prefixes_in_expr(expr, &mut qualifier_prefixes);
    });

    let local_source_names = source_names_in_select(select);
    if qualifier_prefixes
        .iter()
        .any(|name| outer_source_names.contains(name) && !local_source_names.contains(name))
    {
        return true;
    }

    for table in &select.from {
        if table_factor_references_outer_sources(&table.relation, outer_source_names) {
            return true;
        }
        for join in &table.joins {
            if table_factor_references_outer_sources(&join.relation, outer_source_names) {
                return true;
            }
        }
    }
    false
}

fn table_factor_references_outer_sources(
    table_factor: &TableFactor,
    outer_source_names: &HashSet<String>,
) -> bool {
    match table_factor {
        TableFactor::Derived { subquery, .. } => {
            query_references_outer_sources(subquery, outer_source_names)
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            table_factor_references_outer_sources(&table_with_joins.relation, outer_source_names)
                || table_with_joins.joins.iter().any(|join| {
                    table_factor_references_outer_sources(&join.relation, outer_source_names)
                })
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            table_factor_references_outer_sources(table, outer_source_names)
        }
        _ => false,
    }
}

fn source_names_in_select(select: &Select) -> HashSet<String> {
    let mut names = HashSet::new();
    for table in &select.from {
        collect_source_names_from_table_factor(&table.relation, &mut names);
        for join in &table.joins {
            collect_source_names_from_table_factor(&join.relation, &mut names);
        }
    }
    names
}

fn collect_source_names_from_table_factor(table_factor: &TableFactor, names: &mut HashSet<String>) {
    match table_factor {
        TableFactor::Table { name, alias, .. } => {
            if let Some(last) = name.0.last().and_then(|part| part.as_ident()) {
                names.insert(last.value.to_ascii_uppercase());
            }
            if let Some(alias) = alias {
                names.insert(alias.name.value.to_ascii_uppercase());
            }
        }
        TableFactor::Derived {
            alias,
            subquery,
            ..
        } => {
            if let Some(alias) = alias {
                names.insert(alias.name.value.to_ascii_uppercase());
            }
            if let Some(with) = &subquery.with {
                for cte in &with.cte_tables {
                    names.insert(cte.alias.name.value.to_ascii_uppercase());
                }
            }
        }
        TableFactor::TableFunction { alias, .. }
        | TableFactor::Function { alias, .. }
        | TableFactor::UNNEST { alias, .. }
        | TableFactor::JsonTable { alias, .. }
        | TableFactor::OpenJsonTable { alias, .. } => {
            if let Some(alias) = alias {
                names.insert(alias.name.value.to_ascii_uppercase());
            }
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_source_names_from_table_factor(&table_with_joins.relation, names);
            for join in &table_with_joins.joins {
                collect_source_names_from_table_factor(&join.relation, names);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_source_names_from_table_factor(table, names);
        }
        _ => {}
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
    fn default_allows_correlated_subquery_join_without_alias() {
        let issues = run(
            "SELECT pd.* \
             FROM person_dates \
             JOIN (SELECT * FROM events WHERE events.name = person_dates.name)",
        );
        assert!(!issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn default_allows_correlated_subquery_join_with_alias_reference() {
        let issues = run(
            "SELECT pd.* \
             FROM person_dates AS pd \
             JOIN (SELECT * FROM events AS ce WHERE ce.name = pd.name)",
        );
        assert!(!issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn default_allows_correlated_subquery_join_with_outer_table_name_reference() {
        let issues = run(
            "SELECT pd.* \
             FROM person_dates AS pd \
             JOIN (SELECT * FROM events AS ce WHERE ce.name = person_dates.name)",
        );
        assert!(!issues
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
