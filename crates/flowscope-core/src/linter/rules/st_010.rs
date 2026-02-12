//! LINT_ST_010: Constant boolean predicate.
//!
//! Detect redundant constant expressions in predicates.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{BinaryOperator, Expr, Statement};

use super::semantic_helpers::{visit_select_expressions, visit_selects_in_statement};

pub struct StructureConstantExpression;

impl LintRule for StructureConstantExpression {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_010
    }

    fn name(&self) -> &'static str {
        "Structure constant expression"
    }

    fn description(&self) -> &'static str {
        "Avoid constant boolean expressions in predicates."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut found = false;

        visit_selects_in_statement(statement, &mut |select| {
            if found {
                return;
            }

            visit_select_expressions(select, &mut |expr| {
                if contains_constant_predicate(expr) {
                    found = true;
                }
            });
        });

        if found {
            vec![Issue::warning(
                issue_codes::LINT_ST_010,
                "Constant boolean expression detected in predicate.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn contains_constant_predicate(expr: &Expr) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            let direct_match = is_comparison_operator(op.clone())
                && match (literal_key(left), literal_key(right)) {
                    (Some(left_literal), Some(right_literal)) => {
                        !is_allowed_literal_comparison(op.clone(), &left_literal, &right_literal)
                    }
                    _ => expr_equivalent(left, right),
                };

            direct_match || contains_constant_predicate(left) || contains_constant_predicate(right)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => contains_constant_predicate(inner),
        Expr::InList { expr, list, .. } => {
            contains_constant_predicate(expr) || list.iter().any(contains_constant_predicate)
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            contains_constant_predicate(expr)
                || contains_constant_predicate(low)
                || contains_constant_predicate(high)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand
                .as_ref()
                .is_some_and(|expr| contains_constant_predicate(expr))
                || conditions.iter().any(|when| {
                    contains_constant_predicate(&when.condition)
                        || contains_constant_predicate(&when.result)
                })
                || else_result
                    .as_ref()
                    .is_some_and(|expr| contains_constant_predicate(expr))
        }
        _ => false,
    }
}

fn is_comparison_operator(op: BinaryOperator) -> bool {
    matches!(
        op,
        BinaryOperator::Eq
            | BinaryOperator::NotEq
            | BinaryOperator::Lt
            | BinaryOperator::Gt
            | BinaryOperator::LtEq
            | BinaryOperator::GtEq
    )
}

fn is_allowed_literal_comparison(op: BinaryOperator, left: &str, right: &str) -> bool {
    op == BinaryOperator::Eq && ((left == "1" && right == "1") || (left == "1" && right == "0"))
}

fn literal_key(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Value(value) => Some(value.to_string().to_ascii_uppercase()),
        Expr::Nested(inner)
        | Expr::UnaryOp { expr: inner, .. }
        | Expr::Cast { expr: inner, .. } => literal_key(inner),
        _ => None,
    }
}

fn expr_equivalent(left: &Expr, right: &Expr) -> bool {
    match (left, right) {
        (Expr::Identifier(left_ident), Expr::Identifier(right_ident)) => {
            left_ident.value.eq_ignore_ascii_case(&right_ident.value)
        }
        (Expr::CompoundIdentifier(left_parts), Expr::CompoundIdentifier(right_parts)) => {
            left_parts.len() == right_parts.len()
                && left_parts
                    .iter()
                    .zip(right_parts.iter())
                    .all(|(left, right)| left.value.eq_ignore_ascii_case(&right.value))
        }
        (Expr::Nested(left_inner), _) => expr_equivalent(left_inner, right),
        (_, Expr::Nested(right_inner)) => expr_equivalent(left, right_inner),
        (
            Expr::UnaryOp {
                expr: left_inner, ..
            },
            _,
        ) => expr_equivalent(left_inner, right),
        (
            _,
            Expr::UnaryOp {
                expr: right_inner, ..
            },
        ) => expr_equivalent(left, right_inner),
        (
            Expr::Cast {
                expr: left_inner, ..
            },
            _,
        ) => expr_equivalent(left_inner, right),
        (
            _,
            Expr::Cast {
                expr: right_inner, ..
            },
        ) => expr_equivalent(left, right_inner),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = StructureConstantExpression;
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

    // --- Edge cases adopted from sqlfluff ST10 ---

    #[test]
    fn allows_normal_where_predicate() {
        let issues = run("select * from foo where col = 3");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_self_comparison_in_where_clause() {
        let issues = run("select * from foo where col = col");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_010);
    }

    #[test]
    fn flags_self_comparison_in_join_predicate() {
        let issues = run("select foo.a, bar.b from foo left join bar on foo.a = foo.a");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_expected_codegen_literals() {
        let true_case = run("select col from foo where 1=1 and col = 'val'");
        assert!(true_case.is_empty());

        let false_case = run("select col from foo where 1=0 or col = 'val'");
        assert!(false_case.is_empty());
    }

    #[test]
    fn flags_disallowed_literal_comparisons() {
        let issues = run("select col from foo where 'a'!='b' and col = 'val'");
        assert_eq!(issues.len(), 1);

        let issues = run("select col from foo where 1 = 2 or col = 'val'");
        assert_eq!(issues.len(), 1);

        let issues = run("select col from foo where 1 <> 1 or col = 'val'");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn finds_nested_constant_predicates() {
        let issues = run("select col from foo where cond=1 and (score=score or avg_score >= 3)");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_true_false_literal_predicates() {
        let true_issues = run("select * from foo where true and x > 3");
        assert!(true_issues.is_empty());

        let false_issues = run("select * from foo where false OR x < 1 OR y != z");
        assert!(false_issues.is_empty());
    }
}
