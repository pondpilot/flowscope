//! LINT_RF_003: References consistency.
//!
//! In single-source queries, avoid mixing qualified and unqualified references.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

use super::semantic_helpers::{
    count_reference_qualification_in_expr_excluding_aliases, select_projection_alias_set,
    select_source_count, visit_select_expressions, visit_selects_in_statement,
};

pub struct ReferencesConsistent;

impl LintRule for ReferencesConsistent {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_003
    }

    fn name(&self) -> &'static str {
        "References consistent"
    }

    fn description(&self) -> &'static str {
        "Avoid mixing qualified and unqualified references."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut mixed_count = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            if select_source_count(select) != 1 {
                return;
            }

            let aliases = select_projection_alias_set(select);
            let mut qualified = 0usize;
            let mut unqualified = 0usize;

            visit_select_expressions(select, &mut |expr| {
                let (q, u) =
                    count_reference_qualification_in_expr_excluding_aliases(expr, &aliases);
                qualified += q;
                unqualified += u;
            });

            if qualified > 0 && unqualified > 0 {
                mixed_count += 1;
            }
        });

        (0..mixed_count)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_RF_003,
                    "Avoid mixing qualified and unqualified references.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesConsistent;
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

    // --- Edge cases adopted from sqlfluff RF03 ---

    #[test]
    fn flags_mixed_qualification_single_table() {
        let issues = run("SELECT my_tbl.bar, baz FROM my_tbl");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_003);
    }

    #[test]
    fn allows_consistently_unqualified_references() {
        let issues = run("SELECT bar FROM my_tbl");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_consistently_qualified_references() {
        let issues = run("SELECT my_tbl.bar FROM my_tbl");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_mixed_qualification_in_subquery() {
        let issues = run("SELECT * FROM (SELECT my_tbl.bar, baz FROM my_tbl)");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_consistent_references_in_subquery() {
        let issues = run("SELECT * FROM (SELECT my_tbl.bar FROM my_tbl)");
        assert!(issues.is_empty());
    }
}
