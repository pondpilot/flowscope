//! LINT_ST_003: Flattenable nested CASE in ELSE.
//!
//! SQLFluff ST04 parity: flag `CASE ... ELSE CASE ... END END` patterns where
//! the nested ELSE-case can be flattened into the outer CASE.

use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Expr, Statement};

pub struct FlattenableNestedCase;

impl LintRule for FlattenableNestedCase {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_003
    }

    fn name(&self) -> &'static str {
        "Flattenable nested CASE"
    }

    fn description(&self) -> &'static str {
        "Nested CASE in ELSE can be flattened into a single CASE expression."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();

        visit::visit_expressions(stmt, &mut |expr| {
            if is_flattenable_nested_else_case(expr) {
                issues.push(
                    Issue::warning(
                        issue_codes::LINT_ST_003,
                        "Nested CASE in ELSE clause can be flattened.",
                    )
                    .with_statement(ctx.statement_index),
                );
            }
        });

        issues
    }
}

fn is_flattenable_nested_else_case(expr: &Expr) -> bool {
    let Expr::Case {
        operand: outer_operand,
        conditions: outer_conditions,
        else_result: Some(outer_else),
        ..
    } = expr
    else {
        return false;
    };

    // SQLFluff ST04 only applies when there is at least one WHEN in the outer CASE.
    if outer_conditions.is_empty() {
        return false;
    }

    let Some((inner_operand, _inner_conditions, _inner_else)) = case_parts(outer_else) else {
        return false;
    };

    case_operands_match(outer_operand.as_deref(), inner_operand)
}

fn case_parts(
    case_expr: &Expr,
) -> Option<(Option<&Expr>, &[sqlparser::ast::CaseWhen], Option<&Expr>)> {
    match case_expr {
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => Some((
            operand.as_deref(),
            conditions.as_slice(),
            else_result.as_deref(),
        )),
        Expr::Nested(inner) => case_parts(inner),
        _ => None,
    }
}

fn case_operands_match(outer: Option<&Expr>, inner: Option<&Expr>) -> bool {
    match (outer, inner) {
        (None, None) => true,
        (Some(left), Some(right)) => exprs_equal(left, right),
        _ => false,
    }
}

fn exprs_equal(left: &Expr, right: &Expr) -> bool {
    format!("{left}") == format!("{right}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = FlattenableNestedCase;
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

    // --- Edge cases adopted from sqlfluff ST04 ---

    #[test]
    fn passes_nested_case_under_when_clause() {
        let sql = "SELECT CASE WHEN species = 'Rat' THEN CASE WHEN colour = 'Black' THEN 'Growl' WHEN colour = 'Grey' THEN 'Squeak' END END AS sound FROM mytable";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn passes_nested_case_inside_larger_else_expression() {
        let sql = "SELECT CASE WHEN flag = 1 THEN TRUE ELSE score > 10 + CASE WHEN kind = 'b' THEN 8 WHEN kind = 'c' THEN 9 END END AS test FROM t";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_simple_flattenable_else_case() {
        let sql = "SELECT CASE WHEN species = 'Rat' THEN 'Squeak' ELSE CASE WHEN species = 'Dog' THEN 'Woof' END END AS sound FROM mytable";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_003);
    }

    #[test]
    fn flags_nested_else_case_with_multiple_when_clauses() {
        let sql = "SELECT CASE WHEN species = 'Rat' THEN 'Squeak' ELSE CASE WHEN species = 'Dog' THEN 'Woof' WHEN species = 'Mouse' THEN 'Squeak' END END AS sound FROM mytable";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn passes_when_outer_and_inner_case_operands_differ() {
        let sql = "SELECT CASE WHEN day_of_month IN (11, 12, 13) THEN 'TH' ELSE CASE MOD(day_of_month, 10) WHEN 1 THEN 'ST' WHEN 2 THEN 'ND' WHEN 3 THEN 'RD' ELSE 'TH' END END AS ordinal_suffix FROM calendar";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_when_outer_and_inner_simple_case_operands_match() {
        let sql = "SELECT CASE x WHEN 0 THEN 'zero' WHEN 5 THEN 'five' ELSE CASE x WHEN 10 THEN 'ten' WHEN 20 THEN 'twenty' ELSE 'other' END END FROM tab_a";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
    }
}
