//! LINT_RF_005: References special chars.
//!
//! SQLFluff RF05 parity (current scope): quoted identifiers containing
//! unsupported special characters are flagged.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

use super::references_quoted_helpers::quoted_identifiers_in_statement;

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

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_special_chars = quoted_identifiers_in_statement(statement)
            .into_iter()
            .any(|ident| !has_only_simple_identifier_chars(&ident));

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

fn has_only_simple_identifier_chars(ident: &str) -> bool {
    ident
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
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

    #[test]
    fn does_not_flag_double_quotes_inside_string_literal() {
        let issues = run("SELECT '\"bad-name\"' AS note FROM t");
        assert!(issues.is_empty());
    }
}
