//! LINT_JJ_001: Jinja padding.
//!
//! SQLFluff JJ01 parity (current scope): detect inconsistent whitespace around
//! Jinja delimiters.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct JinjaPadding;

impl LintRule for JinjaPadding {
    fn code(&self) -> &'static str {
        issue_codes::LINT_JJ_001
    }

    fn name(&self) -> &'static str {
        "Jinja padding"
    }

    fn description(&self) -> &'static str {
        "Jinja tags should use consistent padding."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = ctx.statement_sql();
        let has_violation =
            has_re(sql, r"\{\{[^ \n]") || has_re(sql, r"[^ \n]\}\}") || has_re(sql, r"\{%[^ \n]");

        if has_violation {
            vec![Issue::info(
                issue_codes::LINT_JJ_001,
                "Jinja tag spacing appears inconsistent.",
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
        let rule = JinjaPadding;
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
    fn flags_missing_padding_in_jinja_expression() {
        let issues = run("SELECT '{{foo}}' AS templated");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
    }

    #[test]
    fn does_not_flag_padded_jinja_expression() {
        assert!(run("SELECT '{{ foo }}' AS templated").is_empty());
    }
}
