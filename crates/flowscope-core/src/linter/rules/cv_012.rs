//! LINT_CV_012: JOIN condition convention.
//!
//! Plain `JOIN` clauses without ON/USING should use explicit join predicates,
//! not implicit relationships hidden in WHERE.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{BinaryOperator, Expr, JoinConstraint, JoinOperator, Select, Statement};

use super::semantic_helpers::{table_factor_reference_name, visit_selects_in_statement};

pub struct ConventionJoinCondition;

impl LintRule for ConventionJoinCondition {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_012
    }

    fn name(&self) -> &'static str {
        "Join condition convention"
    }

    fn description(&self) -> &'static str {
        "JOIN clauses should use explicit, meaningful join predicates."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut found_violation = false;

        visit_selects_in_statement(statement, &mut |select| {
            if found_violation {
                return;
            }

            if select_has_implicit_where_join(select) {
                found_violation = true;
            }
        });

        if found_violation {
            vec![Issue::warning(
                issue_codes::LINT_CV_012,
                "JOIN clause appears to lack a meaningful join condition.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn select_has_implicit_where_join(select: &Select) -> bool {
    for table in &select.from {
        let mut seen_sources = Vec::new();
        if let Some(base) = table_factor_reference_name(&table.relation) {
            seen_sources.push(base);
        }

        for join in &table.joins {
            let current_source = table_factor_reference_name(&join.relation);
            let Some(constraint) = join_constraint(&join.join_operator) else {
                if let Some(source) = current_source {
                    seen_sources.push(source);
                }
                continue;
            };

            let has_explicit_join_clause = matches!(
                constraint,
                JoinConstraint::On(_) | JoinConstraint::Using(_) | JoinConstraint::Natural
            );
            if has_explicit_join_clause {
                if let Some(source) = current_source {
                    seen_sources.push(source);
                }
                continue;
            }

            if select.selection.as_ref().is_some_and(|where_expr| {
                where_contains_join_predicate(where_expr, current_source.as_ref(), &seen_sources)
            }) {
                return true;
            }

            if let Some(source) = current_source {
                seen_sources.push(source);
            }
        }
    }

    false
}

fn join_constraint(join_operator: &JoinOperator) -> Option<&JoinConstraint> {
    match join_operator {
        JoinOperator::Join(constraint)
        | JoinOperator::Inner(constraint)
        | JoinOperator::Left(constraint)
        | JoinOperator::LeftOuter(constraint)
        | JoinOperator::Right(constraint)
        | JoinOperator::RightOuter(constraint)
        | JoinOperator::FullOuter(constraint)
        | JoinOperator::CrossJoin(constraint)
        | JoinOperator::Semi(constraint)
        | JoinOperator::LeftSemi(constraint)
        | JoinOperator::RightSemi(constraint)
        | JoinOperator::Anti(constraint)
        | JoinOperator::LeftAnti(constraint)
        | JoinOperator::RightAnti(constraint)
        | JoinOperator::StraightJoin(constraint) => Some(constraint),
        JoinOperator::AsOf { constraint, .. } => Some(constraint),
        JoinOperator::CrossApply | JoinOperator::OuterApply => None,
    }
}

fn where_contains_join_predicate(
    expr: &Expr,
    current_source: Option<&String>,
    seen_sources: &[String],
) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            let direct_match = matches!(
                op,
                BinaryOperator::Eq
                    | BinaryOperator::NotEq
                    | BinaryOperator::Lt
                    | BinaryOperator::Gt
                    | BinaryOperator::LtEq
                    | BinaryOperator::GtEq
            ) && is_column_reference(left)
                && is_column_reference(right)
                && references_joined_sources(left, right, current_source, seen_sources);

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

fn is_column_reference(expr: &Expr) -> bool {
    matches!(expr, Expr::Identifier(_) | Expr::CompoundIdentifier(_))
}

fn references_joined_sources(
    left: &Expr,
    right: &Expr,
    current_source: Option<&String>,
    seen_sources: &[String],
) -> bool {
    let left_prefix = qualifier_prefix(left);
    let right_prefix = qualifier_prefix(right);

    match (left_prefix, right_prefix, current_source) {
        (Some(left), Some(right), Some(current)) => {
            (left == *current && seen_sources.contains(&right))
                || (right == *current && seen_sources.contains(&left))
        }
        // Unqualified `a = b` in a plain join WHERE is still ambiguous and should fail.
        (None, None, _) => true,
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
        let rule = ConventionJoinCondition;
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

    // --- Edge cases adopted from sqlfluff CV12 ---

    #[test]
    fn allows_plain_join_without_where_clause() {
        let issues = run("SELECT foo.a, bar.b FROM foo JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_plain_join_with_implicit_where_predicate() {
        let issues = run("SELECT foo.a, bar.b FROM foo JOIN bar WHERE foo.x = bar.y");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_012);
    }

    #[test]
    fn flags_plain_join_with_unqualified_where_predicate() {
        let issues = run("SELECT foo.a, bar.b FROM foo JOIN bar WHERE a = b");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_join_with_explicit_on_clause() {
        let issues = run("SELECT foo.a, bar.b FROM foo LEFT JOIN bar ON foo.x = bar.x");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_cross_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo CROSS JOIN bar WHERE bar.x > 3");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_inner_join_without_on_with_where_predicate() {
        let issues = run("SELECT foo.a, bar.b FROM foo INNER JOIN bar WHERE foo.x = bar.y");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_012);
    }
}
