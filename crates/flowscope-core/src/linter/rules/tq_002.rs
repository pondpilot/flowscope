//! LINT_TQ_002: TSQL procedure BEGIN/END block.
//!
//! SQLFluff TQ02 parity (current scope): `CREATE PROCEDURE` should include a
//! `BEGIN`/`END` block.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{ConditionalStatements, Statement};

pub struct TsqlProcedureBeginEnd;

impl LintRule for TsqlProcedureBeginEnd {
    fn code(&self) -> &'static str {
        issue_codes::LINT_TQ_002
    }

    fn name(&self) -> &'static str {
        "TSQL procedure BEGIN/END"
    }

    fn description(&self) -> &'static str {
        "TSQL procedures should include BEGIN/END block."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_violation = match statement {
            Statement::CreateProcedure { body, .. } => !procedure_body_uses_begin_end(body),
            _ => false,
        };

        if has_violation {
            vec![Issue::warning(
                issue_codes::LINT_TQ_002,
                "CREATE PROCEDURE should include BEGIN/END block.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn procedure_body_uses_begin_end(body: &ConditionalStatements) -> bool {
    matches!(body, ConditionalStatements::BeginEnd(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = TsqlProcedureBeginEnd;
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
    fn flags_procedure_without_begin_end() {
        let issues = run("CREATE PROCEDURE p AS SELECT 1;");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_TQ_002);
    }

    #[test]
    fn does_not_flag_procedure_with_begin_end() {
        let issues = run("CREATE PROCEDURE p AS BEGIN SELECT 1; END;");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_procedure_text_inside_string_literal() {
        let issues = run("SELECT 'CREATE PROCEDURE p AS SELECT 1' AS sql_snippet");
        assert!(issues.is_empty());
    }
}
