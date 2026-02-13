//! LINT_LT_004: Layout commas.
//!
//! SQLFluff LT04 parity (current scope): detect compact or leading-space comma
//! patterns.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct LayoutCommas;

impl LintRule for LayoutCommas {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_004
    }

    fn name(&self) -> &'static str {
        "Layout commas"
    }

    fn description(&self) -> &'static str {
        "Comma spacing should be consistent."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = ctx.statement_sql();
        if has_re(sql, r"\s+,") || has_re(sql, r",[^\s\n]") {
            vec![Issue::info(
                issue_codes::LINT_LT_004,
                "Comma spacing appears inconsistent.",
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
        let rule = LayoutCommas;
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
    fn flags_tight_comma_spacing() {
        let issues = run("SELECT a,b FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_004);
    }

    #[test]
    fn does_not_flag_spaced_commas() {
        assert!(run("SELECT a, b FROM t").is_empty());
    }
}
