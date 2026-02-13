//! LINT_CV_010: Quoted literals style.
//!
//! SQLFluff CV10 parity (current scope): detect double-quoted literal-like
//! segments.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

use super::references_quoted_helpers::double_quoted_identifiers_in_statement;

pub struct ConventionQuotedLiterals;

impl LintRule for ConventionQuotedLiterals {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_010
    }

    fn name(&self) -> &'static str {
        "Quoted literals style"
    }

    fn description(&self) -> &'static str {
        "Quoted literal style is inconsistent with SQL convention."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if !double_quoted_identifiers_in_statement(statement).is_empty() {
            vec![Issue::info(
                issue_codes::LINT_CV_010,
                "Quoted literal style appears inconsistent.",
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
        let rule = ConventionQuotedLiterals;
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
    fn flags_double_quoted_literal_like_token() {
        let issues = run("SELECT \"abc\" FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_010);
    }

    #[test]
    fn does_not_flag_single_quoted_literal() {
        assert!(run("SELECT 'abc' FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_double_quotes_inside_single_quoted_literal() {
        assert!(run("SELECT '\"abc\"' FROM t").is_empty());
    }
}
