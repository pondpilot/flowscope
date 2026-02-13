//! LINT_ST_002: Structure simple case.
//!
//! SQLFluff ST02 parity: prefer simple `CASE <expr> WHEN ...` form when all
//! searched-case predicates compare the same operand for equality.

use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{BinaryOperator, Expr, Statement};

pub struct StructureSimpleCase;

impl LintRule for StructureSimpleCase {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_002
    }

    fn name(&self) -> &'static str {
        "Structure simple case"
    }

    fn description(&self) -> &'static str {
        "Use simple CASE when an expression repeatedly compares the same operand."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violations = 0usize;

        visit::visit_expressions(stmt, &mut |expr| {
            if is_simple_case_candidate(expr) {
                violations += 1;
            }
        });

        (0..violations)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_ST_002,
                    "CASE expression may be simplified to simple CASE form.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn is_simple_case_candidate(expr: &Expr) -> bool {
    let Expr::Case {
        operand: None,
        conditions,
        ..
    } = expr
    else {
        return false;
    };

    if conditions.len() < 2 {
        return false;
    }

    let mut common_operand: Option<Expr> = None;

    for case_when in conditions {
        let Some((operand_expr, _value_expr)) =
            split_case_when_equality(&case_when.condition, common_operand.as_ref())
        else {
            return false;
        };
        if common_operand.is_none() {
            common_operand = Some(operand_expr);
        }
    }

    common_operand.is_some()
}

fn split_case_when_equality(
    condition: &Expr,
    expected_operand: Option<&Expr>,
) -> Option<(Expr, Expr)> {
    let Expr::BinaryOp { left, op, right } = condition else {
        return None;
    };

    if *op != BinaryOperator::Eq {
        return None;
    }

    if let Some(expected) = expected_operand {
        if exprs_equivalent(left, expected) {
            return Some((left.as_ref().clone(), right.as_ref().clone()));
        }
        if exprs_equivalent(right, expected) {
            return Some((right.as_ref().clone(), left.as_ref().clone()));
        }
        return None;
    }

    if simple_case_operand_candidate(left) {
        return Some((left.as_ref().clone(), right.as_ref().clone()));
    }
    if simple_case_operand_candidate(right) {
        return Some((right.as_ref().clone(), left.as_ref().clone()));
    }

    None
}

fn simple_case_operand_candidate(expr: &Expr) -> bool {
    matches!(expr, Expr::Identifier(_) | Expr::CompoundIdentifier(_))
}

fn exprs_equivalent(left: &Expr, right: &Expr) -> bool {
    format!("{left}") == format!("{right}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = StructureSimpleCase;
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

    // --- Edge cases adopted from sqlfluff ST02 ---

    #[test]
    fn flags_simple_case_candidate() {
        let sql = "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' END FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_002);
    }

    #[test]
    fn flags_reversed_operand_comparisons() {
        let sql = "SELECT CASE WHEN 1 = x THEN 'a' WHEN 2 = x THEN 'b' END FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn does_not_flag_when_operands_differ() {
        let sql = "SELECT CASE WHEN x = 1 THEN 'a' WHEN y = 2 THEN 'b' END FROM t";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_simple_case_form() {
        let sql = "SELECT CASE x WHEN 1 THEN 'a' WHEN 2 THEN 'b' END FROM t";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_single_when_case() {
        let sql = "SELECT CASE WHEN x = 1 THEN 'a' END FROM t";
        let issues = run(sql);
        assert!(issues.is_empty());
    }
}
