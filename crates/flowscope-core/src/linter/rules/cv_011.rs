//! LINT_CV_011: Casting style.
//!
//! SQLFluff CV11 parity (current scope): detect mixed use of `::` and `CAST()`
//! within the same statement.

use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit::visit_expressions;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{CastKind, Expr, Statement};

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

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if statement_mixes_casting_styles(statement) {
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

fn statement_mixes_casting_styles(statement: &Statement) -> bool {
    let mut has_function_style_cast = false;
    let mut has_double_colon_cast = false;

    visit_expressions(statement, &mut |expr| {
        if has_function_style_cast && has_double_colon_cast {
            return;
        }

        let Expr::Cast { kind, .. } = expr else {
            return;
        };

        match kind {
            CastKind::DoubleColon => has_double_colon_cast = true,
            CastKind::Cast | CastKind::TryCast | CastKind::SafeCast => {
                has_function_style_cast = true
            }
        }
    });

    has_function_style_cast && has_double_colon_cast
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

    #[test]
    fn does_not_flag_cast_like_tokens_inside_string_literal() {
        assert!(run("SELECT 'value::TEXT and CAST(value AS INT)' AS note").is_empty());
    }

    #[test]
    fn flags_mixed_try_cast_and_double_colon_styles() {
        let issues = run("SELECT TRY_CAST(amount AS INT)::TEXT FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_011);
    }
}
