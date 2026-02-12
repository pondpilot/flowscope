//! LINT_AM_009: Ambiguous JOIN condition.
//!
//! Flags join clauses that omit an explicit ON/USING condition when we cannot
//! find an equivalent join predicate in the SELECT WHERE clause.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{
    BinaryOperator, Expr, JoinConstraint, JoinOperator, Select, Statement, TableFactor,
};

use super::semantic_helpers::{table_factor_reference_name, visit_selects_in_statement};

pub struct AmbiguousJoinCondition;

impl LintRule for AmbiguousJoinCondition {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_009
    }

    fn name(&self) -> &'static str {
        "Ambiguous join condition"
    }

    fn description(&self) -> &'static str {
        "Join conditions should be explicit and meaningful."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violation_count = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            violation_count += count_missing_join_conditions(select);
        });

        (0..violation_count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_AM_009,
                    "Join condition appears ambiguous (e.g., ON TRUE / ON 1=1).",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn count_missing_join_conditions(select: &Select) -> usize {
    let mut violations = 0usize;

    for table in &select.from {
        let mut seen_sources = Vec::new();
        if let Some(base) = table_factor_reference_name(&table.relation) {
            seen_sources.push(base);
        }

        for join in &table.joins {
            let join_source = table_factor_reference_name(&join.relation);

            if requires_join_condition(&join.join_operator)
                && !is_value_table_join_target(&join.relation)
                && !join_has_condition(&join.join_operator)
            {
                let covered_by_where = join_source.as_ref().is_some_and(|current| {
                    select.selection.as_ref().is_some_and(|expr| {
                        where_contains_join_predicate(expr, current, &seen_sources)
                    })
                });

                if !covered_by_where {
                    violations += 1;
                }
            }

            if let Some(source) = join_source {
                seen_sources.push(source);
            }
        }
    }

    violations
}

fn requires_join_condition(join_operator: &JoinOperator) -> bool {
    matches!(
        join_operator,
        JoinOperator::Join(_)
            | JoinOperator::Inner(_)
            | JoinOperator::Left(_)
            | JoinOperator::LeftOuter(_)
            | JoinOperator::Right(_)
            | JoinOperator::RightOuter(_)
            | JoinOperator::FullOuter(_)
            | JoinOperator::StraightJoin(_)
    )
}

fn join_has_condition(join_operator: &JoinOperator) -> bool {
    let constraint = match join_operator {
        JoinOperator::Join(constraint)
        | JoinOperator::Inner(constraint)
        | JoinOperator::Left(constraint)
        | JoinOperator::LeftOuter(constraint)
        | JoinOperator::Right(constraint)
        | JoinOperator::RightOuter(constraint)
        | JoinOperator::FullOuter(constraint)
        | JoinOperator::StraightJoin(constraint)
        | JoinOperator::CrossJoin(constraint)
        | JoinOperator::Semi(constraint)
        | JoinOperator::LeftSemi(constraint)
        | JoinOperator::RightSemi(constraint)
        | JoinOperator::Anti(constraint)
        | JoinOperator::LeftAnti(constraint)
        | JoinOperator::RightAnti(constraint)
        | JoinOperator::AsOf { constraint, .. } => constraint,
        JoinOperator::CrossApply | JoinOperator::OuterApply => return true,
    };

    matches!(
        constraint,
        JoinConstraint::On(_) | JoinConstraint::Using(_) | JoinConstraint::Natural
    )
}

fn is_value_table_join_target(table_factor: &TableFactor) -> bool {
    matches!(
        table_factor,
        TableFactor::UNNEST { .. }
            | TableFactor::Function { .. }
            | TableFactor::TableFunction { .. }
    )
}

fn where_contains_join_predicate(
    expr: &Expr,
    current_source: &str,
    seen_sources: &[String],
) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            let direct_match =
                matches!(
                    op,
                    BinaryOperator::Eq
                        | BinaryOperator::NotEq
                        | BinaryOperator::Lt
                        | BinaryOperator::Gt
                        | BinaryOperator::LtEq
                        | BinaryOperator::GtEq
                ) && join_predicate_matches_sources(left, right, current_source, seen_sources);

            direct_match
                || where_contains_join_predicate(left, current_source, seen_sources)
                || where_contains_join_predicate(right, current_source, seen_sources)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            where_contains_join_predicate(inner, current_source, seen_sources)
        }
        Expr::InList { expr, list, .. } => {
            where_contains_join_predicate(expr, current_source, seen_sources)
                || list
                    .iter()
                    .any(|item| where_contains_join_predicate(item, current_source, seen_sources))
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            where_contains_join_predicate(expr, current_source, seen_sources)
                || where_contains_join_predicate(low, current_source, seen_sources)
                || where_contains_join_predicate(high, current_source, seen_sources)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand.as_ref().is_some_and(|expr| {
                where_contains_join_predicate(expr, current_source, seen_sources)
            }) || conditions.iter().any(|when| {
                where_contains_join_predicate(&when.condition, current_source, seen_sources)
                    || where_contains_join_predicate(&when.result, current_source, seen_sources)
            }) || else_result.as_ref().is_some_and(|expr| {
                where_contains_join_predicate(expr, current_source, seen_sources)
            })
        }
        _ => false,
    }
}

fn join_predicate_matches_sources(
    left: &Expr,
    right: &Expr,
    current_source: &str,
    seen_sources: &[String],
) -> bool {
    let left_prefix = qualifier_prefix(left);
    let right_prefix = qualifier_prefix(right);

    match (left_prefix, right_prefix) {
        (Some(left), Some(right)) => {
            (left == current_source && seen_sources.contains(&right))
                || (right == current_source && seen_sources.contains(&left))
        }
        _ => false,
    }
}

fn qualifier_prefix(expr: &Expr) -> Option<String> {
    match expr {
        Expr::CompoundIdentifier(parts) if parts.len() > 1 => {
            parts.first().map(|part| part.value.to_ascii_uppercase())
        }
        Expr::Nested(inner)
        | Expr::UnaryOp { expr: inner, .. }
        | Expr::Cast { expr: inner, .. } => qualifier_prefix(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AmbiguousJoinCondition;
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

    // --- Edge cases adopted from sqlfluff AM08 ---

    #[test]
    fn flags_inner_join_missing_on_clause() {
        let issues = run("SELECT foo.a, bar.b FROM foo INNER JOIN bar");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_009);
    }

    #[test]
    fn flags_missing_condition_in_multi_join_chain() {
        let issues = run("SELECT foo.a, bar.b FROM foo left join bar on 1=2 left join baz");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_where_based_join_predicate_when_on_missing() {
        let issues = run("SELECT foo.a, bar.b FROM foo left join bar where foo.x = bar.y");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_join_with_explicit_on_clause() {
        let issues = run("SELECT foo.a, bar.b FROM foo INNER JOIN bar ON 1=1");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_complex_where_predicate_for_plain_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo JOIN bar WHERE foo.a = bar.a OR foo.x = 3");
        assert!(issues.is_empty());
    }
}
