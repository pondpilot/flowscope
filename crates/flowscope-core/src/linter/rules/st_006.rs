//! LINT_ST_006: Structure column order.
//!
//! SQLFluff ST06 parity: prefer simple column references before complex
//! expressions in SELECT projection lists.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Expr, Select, SelectItem, Statement};

use super::semantic_helpers::visit_selects_in_statement;

pub struct StructureColumnOrder;

impl LintRule for StructureColumnOrder {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_006
    }

    fn name(&self) -> &'static str {
        "Structure column order"
    }

    fn description(&self) -> &'static str {
        "Select wildcards then simple targets before calculations."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violations = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            if has_simple_after_leading_complex(select) {
                violations += 1;
            }
        });

        (0..violations)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_ST_006,
                    "Prefer simple columns before complex expressions in SELECT.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn has_simple_after_leading_complex(select: &Select) -> bool {
    let Some(first_simple_idx) = select.projection.iter().position(is_simple_projection_item)
    else {
        return false;
    };

    first_simple_idx > 0
}

fn is_simple_projection_item(item: &SelectItem) -> bool {
    match item {
        SelectItem::UnnamedExpr(Expr::Identifier(_))
        | SelectItem::UnnamedExpr(Expr::CompoundIdentifier(_)) => true,
        SelectItem::ExprWithAlias { expr, .. } => {
            matches!(expr, Expr::Identifier(_) | Expr::CompoundIdentifier(_))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = StructureColumnOrder;
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

    // --- Edge cases adopted from sqlfluff ST06 ---

    #[test]
    fn flags_when_complex_projection_precedes_first_simple_target() {
        let issues = run("SELECT a + 1, a FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_006);
    }

    #[test]
    fn does_not_flag_when_simple_target_starts_projection() {
        let issues = run("SELECT a, a + 1 FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_when_simple_target_appears_again_after_complex() {
        let issues = run("SELECT a, a + 1, b FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_when_alias_wraps_simple_identifier() {
        let issues = run("SELECT a AS first_a, b FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_in_nested_select_scopes() {
        let issues = run("SELECT * FROM (SELECT a + 1, a FROM t) AS sub");
        assert_eq!(issues.len(), 1);
    }
}
