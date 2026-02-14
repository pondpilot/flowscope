//! LINT_AM_008: Ambiguous JOIN condition.
//!
//! SQLFluff AM08 parity: detect implicit cross joins where JOIN-like operators
//! omit ON/USING/NATURAL conditions.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{JoinConstraint, JoinOperator, Select, Statement, TableFactor};

use super::semantic_helpers::visit_selects_in_statement;

pub struct AmbiguousJoinCondition;

impl LintRule for AmbiguousJoinCondition {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_008
    }

    fn name(&self) -> &'static str {
        "Ambiguous join condition"
    }

    fn description(&self) -> &'static str {
        "Implicit cross joins should be written as CROSS JOIN."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violation_count = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            violation_count += count_implicit_cross_join_violations(select);
        });

        (0..violation_count)
            .map(|_| {
                Issue::warning(issue_codes::LINT_AM_008, "Implicit cross join detected.")
                    .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn count_implicit_cross_join_violations(select: &Select) -> usize {
    // SQLFluff AM08 defers JOIN+WHERE patterns to CV12.
    if select.selection.is_some() {
        return 0;
    }

    let mut violations = 0usize;

    for table in &select.from {
        for join in &table.joins {
            if !operator_requires_join_condition(&join.join_operator) {
                continue;
            }

            if join_constraint_is_explicit(&join.join_operator) {
                continue;
            }

            if is_unnest_join_target(&join.relation) {
                continue;
            }

            violations += 1;
        }
    }

    violations
}

fn operator_requires_join_condition(join_operator: &JoinOperator) -> bool {
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

fn join_constraint_is_explicit(join_operator: &JoinOperator) -> bool {
    let Some(constraint) = join_constraint(join_operator) else {
        return false;
    };

    matches!(
        constraint,
        JoinConstraint::On(_) | JoinConstraint::Using(_) | JoinConstraint::Natural
    )
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

fn is_unnest_join_target(table_factor: &TableFactor) -> bool {
    matches!(table_factor, TableFactor::UNNEST { .. })
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
    fn flags_missing_on_clause_for_inner_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo INNER JOIN bar");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_008);
    }

    #[test]
    fn flags_missing_on_clause_for_left_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo left join bar");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_each_missing_join_condition_in_join_chain() {
        let issues =
            run("SELECT foo.a, bar.b FROM foo left join bar left join baz on foo.x = bar.y");
        assert_eq!(issues.len(), 1);

        let issues =
            run("SELECT foo.a, bar.b FROM foo left join bar on foo.x = bar.y left join baz");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn does_not_flag_join_without_on_when_where_clause_exists() {
        let issues = run("SELECT foo.a, bar.b FROM foo left join bar where foo.x = bar.y");
        assert!(issues.is_empty());

        let issues = run("SELECT foo.a, bar.b FROM foo JOIN bar WHERE foo.a = bar.a OR foo.x = 3");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_explicit_join_conditions() {
        let issues = run("SELECT foo.a, bar.b FROM foo INNER JOIN bar ON 1=1");
        assert!(issues.is_empty());

        let issues = run("SELECT foo.id, bar.id FROM foo LEFT JOIN bar USING (id)");
        assert!(issues.is_empty());

        let issues = run("SELECT foo.x FROM foo NATURAL JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_explicit_cross_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo CROSS JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn ignores_unnest_joins() {
        let issues = run("SELECT t.id FROM t INNER JOIN UNNEST(t.items) AS item");
        assert!(issues.is_empty());
    }
}
