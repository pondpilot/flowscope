//! LINT_LT_003: Layout operators.
//!
//! SQLFluff LT03 parity (current scope): flag trailing operators at end of line.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct LayoutOperators;

impl LintRule for LayoutOperators {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_003
    }

    fn name(&self) -> &'static str {
        "Layout operators"
    }

    fn description(&self) -> &'static str {
        "Operator line placement should be consistent."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if has_re(ctx.statement_sql(), r"(?m)(\+|-|\*|/|=|<>|!=|<|>)\s*$") {
            vec![Issue::info(
                issue_codes::LINT_LT_003,
                "Operator line placement appears inconsistent.",
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
        let rule = LayoutOperators;
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
    fn flags_trailing_operator() {
        let issues = run("SELECT a +\n b FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_003);
    }

    #[test]
    fn does_not_flag_leading_operator() {
        assert!(run("SELECT a\n + b FROM t").is_empty());
    }
}
