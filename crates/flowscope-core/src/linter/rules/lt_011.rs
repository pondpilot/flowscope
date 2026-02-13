//! LINT_LT_011: Layout set operators.
//!
//! SQLFluff LT11 parity (current scope): enforce own-line placement for set
//! operators in multiline statements.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct LayoutSetOperators;

impl LintRule for LayoutSetOperators {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_011
    }

    fn name(&self) -> &'static str {
        "Layout set operators"
    }

    fn description(&self) -> &'static str {
        "Set operators should be consistently line-broken."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = ctx.statement_sql();
        let has_set_operator = has_re(sql, r"(?i)\b(union|intersect|except)\b");
        let is_multiline = sql.contains('\n');
        let has_violation = has_set_operator
            && is_multiline
            && sql.lines().any(|line| {
                let trimmed = line.trim().to_ascii_lowercase();
                match trimmed.as_str() {
                    "union" | "union all" | "intersect" | "except" => false,
                    _ => has_re(&trimmed, r"\b(union|intersect|except)\b"),
                }
            });

        if has_violation {
            vec![Issue::info(
                issue_codes::LINT_LT_011,
                "Set operators should be on their own line in multiline queries.",
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
        let rule = LayoutSetOperators;
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
    fn flags_inline_set_operator_in_multiline_statement() {
        let issues = run("SELECT 1 UNION SELECT 2\nUNION SELECT 3");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_011);
    }

    #[test]
    fn does_not_flag_own_line_set_operators() {
        let issues = run("SELECT 1\nUNION\nSELECT 2\nUNION\nSELECT 3");
        assert!(issues.is_empty());
    }
}
