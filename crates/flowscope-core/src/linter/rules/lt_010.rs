//! LINT_LT_010: Layout select modifiers.
//!
//! SQLFluff LT10 parity (current scope): detect multiline SELECT modifiers in
//! inconsistent positions.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct LayoutSelectModifiers;

impl LintRule for LayoutSelectModifiers {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_010
    }

    fn name(&self) -> &'static str {
        "Layout select modifiers"
    }

    fn description(&self) -> &'static str {
        "SELECT modifiers should be placed consistently."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if has_re(
            ctx.statement_sql(),
            r"(?is)\bselect\s*\n+\s*(distinct|all)\b",
        ) {
            vec![Issue::info(
                issue_codes::LINT_LT_010,
                "SELECT modifiers (DISTINCT/ALL) should be consistently formatted.",
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
        let rule = LayoutSelectModifiers;
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
    fn flags_distinct_on_next_line() {
        let issues = run("SELECT\nDISTINCT a\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_010);
    }

    #[test]
    fn does_not_flag_single_line_modifier() {
        assert!(run("SELECT DISTINCT a FROM t").is_empty());
    }
}
