//! LINT_RF_005: References special chars.
//!
//! SQLFluff RF05 parity (current scope): quoted identifiers containing
//! unsupported special characters are flagged.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct ReferencesSpecialChars;

impl LintRule for ReferencesSpecialChars {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_005
    }

    fn name(&self) -> &'static str {
        "References special chars"
    }

    fn description(&self) -> &'static str {
        "Avoid unsupported special characters in identifiers."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_special_chars = capture_group(ctx.statement_sql(), r#""([^"]+)""#, 1)
            .into_iter()
            .any(|ident| !has_re(&ident, r"^[A-Za-z0-9_]+$"));

        if has_special_chars {
            vec![Issue::warning(
                issue_codes::LINT_RF_005,
                "Identifier contains unsupported special characters.",
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

fn capture_group(sql: &str, pattern: &str, group: usize) -> Vec<String> {
    Regex::new(pattern)
        .expect("valid regex")
        .captures_iter(sql)
        .filter_map(|captures| captures.get(group).map(|m| m.as_str().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesSpecialChars;
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
    fn flags_quoted_identifier_with_hyphen() {
        let issues = run("SELECT \"bad-name\" FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_005);
    }

    #[test]
    fn does_not_flag_quoted_identifier_with_underscore() {
        let issues = run("SELECT \"good_name\" FROM t");
        assert!(issues.is_empty());
    }
}
