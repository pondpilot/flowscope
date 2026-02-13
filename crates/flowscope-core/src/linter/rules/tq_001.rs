//! LINT_TQ_001: TSQL `sp_` prefix.
//!
//! SQLFluff TQ01 parity (current scope): avoid stored procedure names starting
//! with `sp_`.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct TsqlSpPrefix;

impl LintRule for TsqlSpPrefix {
    fn code(&self) -> &'static str {
        issue_codes::LINT_TQ_001
    }

    fn name(&self) -> &'static str {
        "TSQL sp_ prefix"
    }

    fn description(&self) -> &'static str {
        "Avoid sp_ procedure prefix in TSQL."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_violation = has_re(
            ctx.statement_sql(),
            r"(?i)\bcreate\s+(?:proc|procedure)\s+sp_[A-Za-z0-9_]*",
        );
        if has_violation {
            vec![Issue::warning(
                issue_codes::LINT_TQ_001,
                "Avoid stored procedure names with sp_ prefix.",
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
        let rule = TsqlSpPrefix;
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
    fn flags_sp_prefixed_procedure_name_pattern() {
        let issues = run("SELECT 'CREATE PROCEDURE sp_legacy AS SELECT 1' AS sql_snippet");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_TQ_001);
    }

    #[test]
    fn does_not_flag_non_sp_prefixed_procedure_name_pattern() {
        let issues = run("SELECT 'CREATE PROCEDURE proc_legacy AS SELECT 1' AS sql_snippet");
        assert!(issues.is_empty());
    }
}
