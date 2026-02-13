//! LINT_ST_008: Structure distinct.
//!
//! SQLFluff ST08 parity: `SELECT DISTINCT(<expr>)` should be rewritten to
//! `SELECT DISTINCT <expr>`.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Distinct, Expr, SelectItem, Statement};

use super::semantic_helpers::visit_selects_in_statement;

pub struct StructureDistinct;

impl LintRule for StructureDistinct {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_008
    }

    fn name(&self) -> &'static str {
        "Structure distinct"
    }

    fn description(&self) -> &'static str {
        "`DISTINCT` used with parentheses."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violations = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            if distinct_parenthesized_projection(select) {
                violations += 1;
            }
        });

        (0..violations)
            .map(|_| {
                Issue::info(issue_codes::LINT_ST_008, "DISTINCT used with parentheses.")
                    .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn distinct_parenthesized_projection(select: &sqlparser::ast::Select) -> bool {
    if !matches!(select.distinct, Some(Distinct::Distinct)) {
        return false;
    }

    if select.projection.len() != 1 {
        return false;
    }

    matches!(
        select.projection[0],
        SelectItem::UnnamedExpr(Expr::Nested(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = StructureDistinct;
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

    #[test]
    fn flags_distinct_parenthesized_projection() {
        let issues = run("SELECT DISTINCT(a) FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_008);
    }

    #[test]
    fn does_not_flag_normal_distinct_projection() {
        let issues = run("SELECT DISTINCT a FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_when_projection_has_multiple_items() {
        let issues = run("SELECT DISTINCT(a), b FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_in_nested_select_scope() {
        let issues = run("SELECT * FROM (SELECT DISTINCT(a) FROM t) AS sub");
        assert_eq!(issues.len(), 1);
    }
}
