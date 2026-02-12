//! LINT_CV_004: Prefer LEFT JOIN over RIGHT JOIN.
//!
//! RIGHT JOIN is functionally valid but harder to read and reason about in many
//! codebases. Prefer LEFT JOIN for consistent join direction.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

pub struct LeftJoinOverRightJoin;

impl LintRule for LeftJoinOverRightJoin {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_004
    }

    fn name(&self) -> &'static str {
        "LEFT JOIN convention"
    }

    fn description(&self) -> &'static str {
        "Prefer LEFT JOIN over RIGHT JOIN for consistent query style."
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
                    if is_right_join(&join.join_operator) {
                        issues.push(
                            Issue::info(
                                issue_codes::LINT_CV_004,
                                "Prefer LEFT JOIN over RIGHT JOIN for readability.",
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
                if is_right_join(&join.join_operator) {
                    issues.push(
                        Issue::info(
                            issue_codes::LINT_CV_004,
                            "Prefer LEFT JOIN over RIGHT JOIN for readability.",
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

fn is_right_join(op: &JoinOperator) -> bool {
    matches!(
        op,
        JoinOperator::Right(_)
            | JoinOperator::RightOuter(_)
            | JoinOperator::RightSemi(_)
            | JoinOperator::RightAnti(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = LeftJoinOverRightJoin;
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
    fn test_right_join_detected() {
        let issues = check_sql("SELECT * FROM a RIGHT JOIN b ON a.id = b.id");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_CV_004");
    }

    #[test]
    fn test_left_join_ok() {
        let issues = check_sql("SELECT * FROM a LEFT JOIN b ON a.id = b.id");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_inner_join_ok() {
        let issues = check_sql("SELECT * FROM a JOIN b ON a.id = b.id");
        assert!(issues.is_empty());
    }
}
