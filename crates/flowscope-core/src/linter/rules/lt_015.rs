//! LINT_LT_015: Layout newlines.
//!
//! SQLFluff LT15 parity (current scope): detect excessive blank lines.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
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
        if has_re(ctx.statement_sql(), r"\n\s*\n\s*\n+") {
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

fn has_re(haystack: &str, pattern: &str) -> bool {
    Regex::new(pattern).expect("valid regex").is_match(haystack)
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
}
