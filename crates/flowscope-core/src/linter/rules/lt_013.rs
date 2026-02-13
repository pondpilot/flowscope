//! LINT_LT_013: Layout start of file.
//!
//! SQLFluff LT13 parity (current scope): avoid leading blank lines.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct LayoutStartOfFile;

impl LintRule for LayoutStartOfFile {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_013
    }

    fn name(&self) -> &'static str {
        "Layout start of file"
    }

    fn description(&self) -> &'static str {
        "Avoid leading blank lines at file start."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_violation = ctx.statement_index == 0 && has_re(ctx.sql, r"^\s*\n");
        if has_violation {
            vec![Issue::info(
                issue_codes::LINT_LT_013,
                "Avoid leading blank lines at the start of SQL file.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn has_re(haystack: &str, pattern: &str) -> bool {
    Regex::new(pattern).expect("valid regex").is_match(haystack)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutStartOfFile;
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
    fn flags_leading_blank_lines() {
        let issues = run("\n\nSELECT 1");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_013);
    }

    #[test]
    fn does_not_flag_clean_start() {
        assert!(run("SELECT 1").is_empty());
    }
}
