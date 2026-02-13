//! LINT_AL_009: Self alias column.
//!
//! SQLFluff AL09 parity: avoid aliasing a column to its own name.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Expr, SelectItem, Statement};

use super::semantic_helpers::visit_selects_in_statement;

pub struct AliasingSelfAliasColumn;

impl LintRule for AliasingSelfAliasColumn {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_009
    }

    fn name(&self) -> &'static str {
        "Self alias column"
    }

    fn description(&self) -> &'static str {
        "Column aliases should not alias to itself."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violations = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            for item in &select.projection {
                let SelectItem::ExprWithAlias { expr, alias } = item else {
                    continue;
                };

                if aliases_expression_to_itself(expr, alias.value.as_str()) {
                    violations += 1;
                }
            }
        });

        (0..violations)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_AL_009,
                    "Column aliases should not alias to itself.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn aliases_expression_to_itself(expr: &Expr, alias: &str) -> bool {
    let Some(source_name) = expression_name(expr) else {
        return false;
    };

    source_name.eq_ignore_ascii_case(alias)
}

fn expression_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Identifier(identifier) => Some(identifier.value.as_str()),
        Expr::CompoundIdentifier(parts) => parts.last().map(|part| part.value.as_str()),
        Expr::Nested(inner) => expression_name(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingSelfAliasColumn;
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
    fn flags_plain_self_alias() {
        let issues = run("SELECT a AS a FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_009);
    }

    #[test]
    fn flags_qualified_self_alias() {
        let issues = run("SELECT t.a AS a FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_case_insensitive_self_alias() {
        let issues = run("SELECT a AS A FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_alias_name() {
        let issues = run("SELECT a AS b FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_non_identifier_expression() {
        let issues = run("SELECT a + 1 AS a FROM t");
        assert!(issues.is_empty());
    }
}
