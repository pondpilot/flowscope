//! LINT_TQ_003: TSQL empty batch.
//!
//! SQLFluff TQ03 parity (current scope): detect empty batches between repeated
//! `GO` separators.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct TsqlEmptyBatch;

impl LintRule for TsqlEmptyBatch {
    fn code(&self) -> &'static str {
        issue_codes::LINT_TQ_003
    }

    fn name(&self) -> &'static str {
        "TSQL empty batch"
    }

    fn description(&self) -> &'static str {
        "Avoid empty TSQL batches between GO separators."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_violation = has_re(
            ctx.statement_sql(),
            r"(?im)^\s*GO\s*$\s*(?:\r?\n\s*GO\s*$)+",
        );
        if has_violation {
            vec![Issue::warning(
                issue_codes::LINT_TQ_003,
                "Empty TSQL batch detected between GO separators.",
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
        let rule = TsqlEmptyBatch;
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
    fn flags_repeated_go_separator_pattern() {
        let issues = run("SELECT '\nGO\nGO\n' AS sql_snippet");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_TQ_003);
    }

    #[test]
    fn does_not_flag_single_go_separator_pattern() {
        let issues = run("SELECT '\nGO\n' AS sql_snippet");
        assert!(issues.is_empty());
    }
}
