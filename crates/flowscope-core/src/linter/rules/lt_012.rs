//! LINT_LT_012: Layout end of file.
//!
//! SQLFluff LT12 parity (current scope): SQL text should end with exactly one
//! trailing newline.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

pub struct LayoutEndOfFile;

impl LintRule for LayoutEndOfFile {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_012
    }

    fn name(&self) -> &'static str {
        "Layout end of file"
    }

    fn description(&self) -> &'static str {
        "File should end with a single trailing newline."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let content_end = ctx
            .sql
            .trim_end_matches(|ch: char| ch.is_ascii_whitespace())
            .len();
        let is_last_statement = ctx.statement_range.end >= content_end;
        let has_violation =
            is_last_statement && ctx.sql.contains('\n') && trailing_newline_count(ctx.sql) != 1;

        if has_violation {
            vec![Issue::info(
                issue_codes::LINT_LT_012,
                "SQL document should end with a single trailing newline.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn trailing_newline_count(sql: &str) -> usize {
    sql.chars()
        .rev()
        .take_while(|ch| *ch == '\n' || *ch == '\r')
        .filter(|ch| *ch == '\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutEndOfFile;
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
    fn flags_missing_trailing_newline() {
        let issues = run("SELECT 1\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_012);
    }

    #[test]
    fn does_not_flag_when_trailing_newline_present() {
        assert!(run("SELECT 1\nFROM t\n").is_empty());
    }

    #[test]
    fn does_not_flag_single_line_without_newline() {
        assert!(run("SELECT 1").is_empty());
    }

    #[test]
    fn flags_multiple_trailing_newlines() {
        let issues = run("SELECT 1\nFROM t\n\n");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_012);
    }
}
