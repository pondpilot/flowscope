//! LINT_CV_007: Statement brackets.
//!
//! SQLFluff CV07 parity (current scope): avoid wrapping an entire statement in
//! unnecessary outer brackets.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

pub struct ConventionStatementBrackets;

impl LintRule for ConventionStatementBrackets {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_007
    }

    fn name(&self) -> &'static str {
        "Statement brackets"
    }

    fn description(&self) -> &'static str {
        "Avoid unnecessary wrapping brackets around full statements."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = ctx.statement_sql().trim();
        if sql.starts_with('(') && sql.ends_with(')') {
            vec![Issue::info(
                issue_codes::LINT_CV_007,
                "Avoid wrapping the full statement in unnecessary brackets.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionStatementBrackets;
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
    fn flags_wrapped_statement() {
        let issues = run("(SELECT 1)");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_007);
    }

    #[test]
    fn does_not_flag_normal_statement() {
        assert!(run("SELECT 1").is_empty());
    }
}
