//! LINT_CV_011: Casting style.
//!
//! SQLFluff CV11 parity (current scope): detect mixed use of `::` and `CAST()`
//! within the same statement.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct ConventionCastingStyle;

impl LintRule for ConventionCastingStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_011
    }

    fn name(&self) -> &'static str {
        "Casting style"
    }

    fn description(&self) -> &'static str {
        "Use consistent casting style."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = ctx.statement_sql();
        if has_re(sql, r"::") && has_re(sql, r"(?i)\bcast\s*\(") {
            vec![Issue::info(
                issue_codes::LINT_CV_011,
                "Use consistent casting style (avoid mixing :: and CAST).",
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
        let rule = ConventionCastingStyle;
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
    fn flags_mixed_casting_styles() {
        let issues = run("SELECT CAST(amount AS INT)::TEXT FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_011);
    }

    #[test]
    fn does_not_flag_single_casting_style() {
        assert!(run("SELECT amount::INT FROM t").is_empty());
        assert!(run("SELECT CAST(amount AS INT) FROM t").is_empty());
    }
}
