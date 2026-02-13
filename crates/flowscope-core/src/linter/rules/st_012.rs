//! LINT_ST_012: Structure consecutive semicolons.
//!
//! SQLFluff ST12 parity (current scope): detect consecutive semicolons in the
//! document text.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct StructureConsecutiveSemicolons;

impl LintRule for StructureConsecutiveSemicolons {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_012
    }

    fn name(&self) -> &'static str {
        "Structure consecutive semicolons"
    }

    fn description(&self) -> &'static str {
        "Avoid consecutive semicolons."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_violation = ctx.statement_index == 0 && has_re(ctx.sql, r";\s*;");
        if has_violation {
            vec![
                Issue::warning(issue_codes::LINT_ST_012, "Consecutive semicolons detected.")
                    .with_statement(ctx.statement_index),
            ]
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
        let rule = StructureConsecutiveSemicolons;
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
    fn flags_consecutive_semicolons() {
        let issues = run("SELECT 1;;");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_012);
    }

    #[test]
    fn does_not_flag_single_semicolon() {
        let issues = run("SELECT 1;");
        assert!(issues.is_empty());
    }
}
