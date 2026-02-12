//! LINT_ST_004: Avoid JOIN ... USING (...).
//!
//! USING can hide which side a column originates from and may create ambiguity
//! in complex joins. Prefer explicit ON conditions.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

pub struct AvoidUsingJoin;

impl LintRule for AvoidUsingJoin {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_004
    }

    fn name(&self) -> &'static str {
        "Avoid USING in JOIN"
    }

    fn description(&self) -> &'static str {
        "Prefer explicit ON conditions instead of JOIN ... USING (...)."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        check_statement(stmt, ctx, &mut issues);
        issues
    }
}

fn check_statement(stmt: &Statement, ctx: &LintContext, issues: &mut Vec<Issue>) {
    match stmt {
        Statement::Query(q) => check_query(q, ctx, issues),
        Statement::Insert(ins) => {
            if let Some(ref source) = ins.source {
                check_query(source, ctx, issues);
            }
        }
        Statement::CreateView { query, .. } => check_query(query, ctx, issues),
        Statement::CreateTable(create) => {
            if let Some(ref q) = create.query {
                check_query(q, ctx, issues);
            }
        }
        _ => {}
    }
}

fn check_query(query: &Query, ctx: &LintContext, issues: &mut Vec<Issue>) {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            check_query(&cte.query, ctx, issues);
        }
    }
    check_set_expr(&query.body, ctx, issues);
}

fn check_set_expr(body: &SetExpr, ctx: &LintContext, issues: &mut Vec<Issue>) {
    match body {
        SetExpr::Select(select) => {
            for from_item in &select.from {
                check_table_factor(&from_item.relation, ctx, issues);
                for join in &from_item.joins {
                    if has_using_constraint(&join.join_operator) {
                        issues.push(
                            Issue::warning(
                                issue_codes::LINT_ST_004,
                                "Avoid JOIN ... USING (...); prefer explicit ON conditions.",
                            )
                            .with_statement(ctx.statement_index),
                        );
                    }
                    check_table_factor(&join.relation, ctx, issues);
                }
            }
        }
        SetExpr::Query(q) => check_query(q, ctx, issues),
        SetExpr::SetOperation { left, right, .. } => {
            check_set_expr(left, ctx, issues);
            check_set_expr(right, ctx, issues);
        }
        _ => {}
    }
}

fn check_table_factor(relation: &TableFactor, ctx: &LintContext, issues: &mut Vec<Issue>) {
    match relation {
        TableFactor::Derived { subquery, .. } => check_query(subquery, ctx, issues),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            check_table_factor(&table_with_joins.relation, ctx, issues);
            for join in &table_with_joins.joins {
                if has_using_constraint(&join.join_operator) {
                    issues.push(
                        Issue::warning(
                            issue_codes::LINT_ST_004,
                            "Avoid JOIN ... USING (...); prefer explicit ON conditions.",
                        )
                        .with_statement(ctx.statement_index),
                    );
                }
                check_table_factor(&join.relation, ctx, issues);
            }
        }
        _ => {}
    }
}

fn has_using_constraint(op: &JoinOperator) -> bool {
    join_constraint(op).is_some_and(|constraint| matches!(constraint, JoinConstraint::Using(_)))
}

fn join_constraint(op: &JoinOperator) -> Option<&JoinConstraint> {
    match op {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = AvoidUsingJoin;
        let ctx = LintContext {
            sql,
            statement_range: 0..sql.len(),
            statement_index: 0,
        };
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check(stmt, &ctx));
        }
        issues
    }

    #[test]
    fn test_using_join_detected() {
        let issues = check_sql("SELECT * FROM a JOIN b USING (id)");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_ST_004");
    }

    #[test]
    fn test_on_join_ok() {
        let issues = check_sql("SELECT * FROM a JOIN b ON a.id = b.id");
        assert!(issues.is_empty());
    }
}
