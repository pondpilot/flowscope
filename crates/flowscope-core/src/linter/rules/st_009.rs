//! LINT_ST_009: Reversed JOIN condition ordering.
//!
//! Detect predicates where the newly joined relation appears on the left side
//! and prior relation on the right side (e.g. `o.user_id = u.id`).

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{BinaryOperator, Expr, Statement};

use super::semantic_helpers::{
    join_on_expr, table_factor_reference_name, visit_selects_in_statement,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreferredFirstTableInJoinClause {
    Earlier,
    Later,
}

impl PreferredFirstTableInJoinClause {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(
                issue_codes::LINT_ST_009,
                "preferred_first_table_in_join_clause",
            )
            .unwrap_or("earlier")
            .to_ascii_lowercase()
            .as_str()
        {
            "later" => Self::Later,
            _ => Self::Earlier,
        }
    }

    fn left_source<'a>(self, current: &'a str, previous: &'a str) -> &'a str {
        match self {
            Self::Earlier => current,
            Self::Later => previous,
        }
    }

    fn right_source<'a>(self, current: &'a str, previous: &'a str) -> &'a str {
        match self {
            Self::Earlier => previous,
            Self::Later => current,
        }
    }
}

pub struct StructureJoinConditionOrder {
    preferred_first_table: PreferredFirstTableInJoinClause,
}

impl StructureJoinConditionOrder {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            preferred_first_table: PreferredFirstTableInJoinClause::from_config(config),
        }
    }
}

impl Default for StructureJoinConditionOrder {
    fn default() -> Self {
        Self {
            preferred_first_table: PreferredFirstTableInJoinClause::Earlier,
        }
    }
}

impl LintRule for StructureJoinConditionOrder {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_009
    }

    fn name(&self) -> &'static str {
        "Structure join condition order"
    }

    fn description(&self) -> &'static str {
        "Join condition ordering appears reversed."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violation_count = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            let mut seen_sources: Vec<String> = Vec::new();

            for table in &select.from {
                if let Some(base) = table_factor_reference_name(&table.relation) {
                    seen_sources.push(base);
                }

                for join in &table.joins {
                    let join_name = table_factor_reference_name(&join.relation);
                    let previous_source = seen_sources.last().cloned();

                    if let (Some(current), Some(previous), Some(on_expr)) = (
                        join_name.as_ref(),
                        previous_source.as_ref(),
                        join_on_expr(&join.join_operator),
                    ) {
                        let left = self.preferred_first_table.left_source(current, previous);
                        let right = self.preferred_first_table.right_source(current, previous);
                        if has_join_pair(on_expr, left, right) {
                            violation_count += 1;
                        }
                    }

                    if let Some(name) = join_name {
                        seen_sources.push(name);
                    }
                }
            }
        });

        (0..violation_count)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_ST_009,
                    "Join condition ordering appears inconsistent with configured preference.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn has_join_pair(expr: &Expr, left_source_name: &str, right_source_name: &str) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            let direct = if *op == BinaryOperator::Eq {
                if let (Some(left_prefix), Some(right_prefix)) =
                    (expr_qualified_prefix(left), expr_qualified_prefix(right))
                {
                    left_prefix == left_source_name && right_prefix == right_source_name
                } else {
                    false
                }
            } else {
                false
            };

            direct
                || has_join_pair(left, left_source_name, right_source_name)
                || has_join_pair(right, left_source_name, right_source_name)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            has_join_pair(inner, left_source_name, right_source_name)
        }
        Expr::InList { expr, list, .. } => {
            has_join_pair(expr, left_source_name, right_source_name)
                || list
                    .iter()
                    .any(|item| has_join_pair(item, left_source_name, right_source_name))
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            has_join_pair(expr, left_source_name, right_source_name)
                || has_join_pair(low, left_source_name, right_source_name)
                || has_join_pair(high, left_source_name, right_source_name)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand
                .as_ref()
                .is_some_and(|operand| has_join_pair(operand, left_source_name, right_source_name))
                || conditions.iter().any(|when| {
                    has_join_pair(&when.condition, left_source_name, right_source_name)
                        || has_join_pair(&when.result, left_source_name, right_source_name)
                })
                || else_result.as_ref().is_some_and(|otherwise| {
                    has_join_pair(otherwise, left_source_name, right_source_name)
                })
        }
        _ => false,
    }
}

fn expr_qualified_prefix(expr: &Expr) -> Option<String> {
    match expr {
        Expr::CompoundIdentifier(parts) if parts.len() > 1 => {
            parts.first().map(|ident| ident.value.to_ascii_uppercase())
        }
        Expr::Nested(inner)
        | Expr::UnaryOp { expr: inner, .. }
        | Expr::Cast { expr: inner, .. } => expr_qualified_prefix(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = StructureJoinConditionOrder::default();
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

    // --- Edge cases adopted from sqlfluff ST09 ---

    #[test]
    fn allows_queries_without_joins() {
        let issues = run("select * from foo");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_expected_source_order_in_join_condition() {
        let issues = run("select foo.a, bar.b from foo left join bar on foo.a = bar.a");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_reversed_source_order_in_join_condition() {
        let issues = run("select foo.a, bar.b from foo left join bar on bar.a = foo.a");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_009);
    }

    #[test]
    fn allows_unqualified_reference_side() {
        let issues = run("select foo.a, bar.b from foo left join bar on bar.b = a");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_multiple_reversed_subconditions() {
        let issues = run(
            "select foo.a, foo.b, bar.c from foo left join bar on bar.a = foo.a and bar.b = foo.b",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn later_preference_flags_earlier_on_left_side() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "structure.join_condition_order".to_string(),
                serde_json::json!({"preferred_first_table_in_join_clause": "later"}),
            )]),
        };
        let rule = StructureJoinConditionOrder::from_config(&config);
        let sql = "select foo.a, bar.b from foo left join bar on foo.a = bar.a";
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
        assert_eq!(issues[0].code, issue_codes::LINT_ST_009);
    }
}
