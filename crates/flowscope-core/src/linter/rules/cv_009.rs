//! LINT_CV_009: Blocked words.
//!
//! SQLFluff CV09 parity (current scope): detect placeholder words such as
//! TODO/FIXME/foo/bar.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct ConventionBlockedWords;

impl LintRule for ConventionBlockedWords {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_009
    }

    fn name(&self) -> &'static str {
        "Blocked words"
    }

    fn description(&self) -> &'static str {
        "Avoid blocked placeholder words."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if has_re(ctx.statement_sql(), r"(?i)\b(todo|fixme|foo|bar)\b") {
            vec![Issue::warning(
                issue_codes::LINT_CV_009,
                "Blocked placeholder words detected (e.g., TODO/FIXME/foo/bar).",
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
        let rule = ConventionBlockedWords;
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
    fn flags_blocked_word() {
        let issues = run("SELECT foo FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_009);
    }

    #[test]
    fn does_not_flag_clean_identifier() {
        assert!(run("SELECT customer_id FROM t").is_empty());
    }
}
