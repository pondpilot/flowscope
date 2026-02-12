//! LINT_RF_002: References qualification.
//!
//! In multi-table queries, require qualified column references.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

use super::semantic_helpers::{
    count_reference_qualification_in_expr_excluding_aliases, select_projection_alias_set,
    select_source_count, visit_select_expressions, visit_selects_in_statement,
};

pub struct ReferencesQualification;

impl LintRule for ReferencesQualification {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_002
    }

    fn name(&self) -> &'static str {
        "References qualification"
    }

    fn description(&self) -> &'static str {
        "Use qualification consistently in multi-table queries."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut unqualified_count = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            if select_source_count(select) <= 1 {
                return;
            }

            let aliases = select_projection_alias_set(select);
            visit_select_expressions(select, &mut |expr| {
                let (_, unqualified) =
                    count_reference_qualification_in_expr_excluding_aliases(expr, &aliases);
                unqualified_count += unqualified;
            });
        });

        (0..unqualified_count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_RF_002,
                    "Use qualified references in multi-table queries.",
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
        let rule = ReferencesQualification;
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

    // --- Edge cases adopted from sqlfluff RF02 ---

    #[test]
    fn allows_fully_qualified_multi_table_query() {
        let issues = run("SELECT foo.a, vee.b FROM foo LEFT JOIN vee ON vee.a = foo.a");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_unqualified_multi_table_query() {
        let issues = run("SELECT a, b FROM foo LEFT JOIN vee ON vee.a = foo.a");
        assert!(!issues.is_empty());
        assert!(issues
            .iter()
            .all(|issue| issue.code == issue_codes::LINT_RF_002));
    }

    #[test]
    fn allows_qualified_multi_table_query_inside_subquery() {
        let issues =
            run("SELECT a FROM (SELECT foo.a, vee.b FROM foo LEFT JOIN vee ON vee.a = foo.a)");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_unqualified_multi_table_query_inside_subquery() {
        let issues = run("SELECT a FROM (SELECT a, b FROM foo LEFT JOIN vee ON vee.a = foo.a)");
        assert!(!issues.is_empty());
    }
}
