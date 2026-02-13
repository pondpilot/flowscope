//! LINT_LT_012: Layout end of file.
//!
//! SQLFluff LT12 parity (current scope): SQL text containing newlines should
//! end with a trailing newline.

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
        "File should end with newline."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_violation = ctx.statement_range.end == ctx.sql.len()
            && ctx.sql.contains('\n')
            && !ctx.sql.ends_with('\n');

        if has_violation {
            vec![Issue::info(
                issue_codes::LINT_LT_012,
                "SQL document should end with a trailing newline.",
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
}
