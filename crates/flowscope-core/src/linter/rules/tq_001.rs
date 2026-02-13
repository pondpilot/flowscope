//! LINT_TQ_001: TSQL `sp_` prefix.
//!
//! SQLFluff TQ01 parity (current scope): avoid stored procedure names starting
//! with `sp_`.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{ObjectName, Statement};

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

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_violation = match statement {
            Statement::CreateProcedure { name, .. } => procedure_name_has_sp_prefix(name),
            _ => false,
        };

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

fn procedure_name_has_sp_prefix(name: &ObjectName) -> bool {
    name.0
        .last()
        .and_then(|part| part.as_ident())
        .is_some_and(|ident| ident.value.to_ascii_lowercase().starts_with("sp_"))
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
    fn flags_sp_prefixed_procedure_name() {
        let issues = run("CREATE PROCEDURE sp_legacy AS SELECT 1;");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_TQ_001);
    }

    #[test]
    fn does_not_flag_non_sp_prefixed_procedure_name() {
        let issues = run("CREATE PROCEDURE proc_legacy AS SELECT 1;");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_sp_prefix_text_inside_string_literal() {
        let issues = run("SELECT 'CREATE PROCEDURE sp_legacy AS SELECT 1' AS sql_snippet");
        assert!(issues.is_empty());
    }
}
