//! LINT_AM_005: Ambiguous JOIN style.
//!
//! Require explicit JOIN type keywords (`INNER`, `LEFT`, etc.) instead of bare
//! `JOIN` for clearer intent.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{JoinOperator, Statement};

use super::semantic_helpers::visit_selects_in_statement;

pub struct AmbiguousJoinStyle;

impl LintRule for AmbiguousJoinStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_005
    }

    fn name(&self) -> &'static str {
        "Ambiguous join style"
    }

    fn description(&self) -> &'static str {
        "Join clauses should be fully qualified."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut plain_join_count = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            for table in &select.from {
                for join in &table.joins {
                    if matches!(join.join_operator, JoinOperator::Join(_)) {
                        plain_join_count += 1;
                    }
                }
            }
        });

        (0..plain_join_count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_AM_005,
                    "Join clauses should be fully qualified.",
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
        let rule = AmbiguousJoinStyle;
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

    // --- Edge cases adopted from sqlfluff AM05 ---

    #[test]
    fn flags_plain_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo JOIN bar");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_005);
    }

    #[test]
    fn flags_lowercase_plain_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo join bar");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_inner_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo INNER JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_left_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo LEFT JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_right_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo RIGHT JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_full_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo FULL JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_left_outer_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo LEFT OUTER JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_right_outer_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo RIGHT OUTER JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_full_outer_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo FULL OUTER JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_cross_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo CROSS JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_each_plain_join_in_chain() {
        let issues = run("SELECT * FROM a JOIN b ON a.id = b.id JOIN c ON b.id = c.id");
        assert_eq!(issues.len(), 2);
        assert!(issues
            .iter()
            .all(|issue| issue.code == issue_codes::LINT_AM_005));
    }
}
