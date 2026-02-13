//! LINT_LT_015: Layout newlines.
//!
//! SQLFluff LT15 parity (current scope): detect excessive blank lines.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

pub struct LayoutNewlines;

impl LintRule for LayoutNewlines {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_015
    }

    fn name(&self) -> &'static str {
        "Layout newlines"
    }

    fn description(&self) -> &'static str {
        "Avoid excessive blank lines."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if has_excessive_blank_lines(ctx.statement_sql()) {
            vec![Issue::info(
                issue_codes::LINT_LT_015,
                "SQL contains excessive blank lines.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn has_excessive_blank_lines(sql: &str) -> bool {
    let mut blank_run = 0usize;

    for line in sql.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run >= 2 {
                return true;
            }
        } else {
            blank_run = 0;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutNewlines;
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
    fn flags_excessive_blank_lines() {
        let issues = run("SELECT 1\n\n\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }

    #[test]
    fn does_not_flag_single_blank_line() {
        assert!(run("SELECT 1\n\nFROM t").is_empty());
    }

    #[test]
    fn flags_blank_lines_with_whitespace() {
        let issues = run("SELECT 1\n\n   \nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }
}
