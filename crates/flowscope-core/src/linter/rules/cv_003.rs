//! LINT_CV_003: Select trailing comma.
//!
//! Avoid trailing comma before FROM.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct ConventionSelectTrailingComma;

impl LintRule for ConventionSelectTrailingComma {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_003
    }

    fn name(&self) -> &'static str {
        "Select trailing comma"
    }

    fn description(&self) -> &'static str {
        "Trailing commas within select clause."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let re = Regex::new(r"(?is)\bselect\b[^;]*,\s*\bfrom\b").expect("valid regex");
        if re.is_match(ctx.statement_sql()) {
            vec![Issue::warning(
                issue_codes::LINT_CV_003,
                "Avoid trailing comma before FROM in SELECT clause.",
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
        let stmts = parse_sql(sql).expect("parse");
        let rule = ConventionSelectTrailingComma;
        stmts
            .iter()
            .enumerate()
            .flat_map(|(index, stmt)| {
                rule.check(
                    stmt,
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
    fn flags_trailing_comma_before_from() {
        let issues = run("select a, from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_003);
    }

    #[test]
    fn allows_no_trailing_comma() {
        let issues = run("select a from t");
        assert!(issues.is_empty());
    }
}
