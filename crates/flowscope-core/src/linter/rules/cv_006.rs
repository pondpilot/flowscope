//! LINT_CV_006: Statement terminator.
//!
//! Enforce consistent semicolon termination within a SQL document.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

pub struct ConventionTerminator;

impl LintRule for ConventionTerminator {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_006
    }

    fn name(&self) -> &'static str {
        "Statement terminator"
    }

    fn description(&self) -> &'static str {
        "Statements must end with a semi-colon."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if ctx.sql.contains(';') && !statement_has_terminal_semicolon(ctx) {
            vec![Issue::info(
                issue_codes::LINT_CV_006,
                "Statement terminator style is inconsistent.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn statement_has_terminal_semicolon(ctx: &LintContext) -> bool {
    if ctx.sql[..ctx.statement_range.end.min(ctx.sql.len())]
        .trim_end()
        .ends_with(';')
    {
        return true;
    }

    let bytes = ctx.sql.as_bytes();
    let mut idx = ctx.statement_range.end;
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    idx < bytes.len() && bytes[idx] == b';'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).expect("parse");
        let rule = ConventionTerminator;
        stmts
            .iter()
            .enumerate()
            .flat_map(|(index, stmt)| {
                rule.check(
                    stmt,
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
    fn flags_when_file_has_mixed_terminator_style() {
        let issues = run("select 1; select 2");
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_006);
    }

    #[test]
    fn allows_consistent_terminated_statements() {
        let issues = run("select 1; select 2;");
        assert!(issues.is_empty());
    }
}
