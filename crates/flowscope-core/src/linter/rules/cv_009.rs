//! LINT_CV_009: Blocked words.
//!
//! SQLFluff CV09 parity (current scope): detect placeholder words such as
//! TODO/FIXME/foo/bar.

use crate::extractors::extract_tables;
use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit::visit_expressions;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Expr, SelectItem, Statement};

use super::semantic_helpers::{table_factor_alias_name, visit_selects_in_statement};

pub struct ConventionBlockedWords;

impl LintRule for ConventionBlockedWords {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_009
    }

    fn name(&self) -> &'static str {
        "Blocked words"
    }

    fn description(&self) -> &'static str {
        "Avoid blocked placeholder words."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if statement_contains_blocked_word(statement) {
            vec![Issue::warning(
                issue_codes::LINT_CV_009,
                "Blocked placeholder words detected (e.g., TODO/FIXME/foo/bar).",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn statement_contains_blocked_word(statement: &Statement) -> bool {
    if extract_tables(std::slice::from_ref(statement))
        .into_iter()
        .any(|name| name_contains_blocked_word(&name))
    {
        return true;
    }

    let mut found = false;
    visit_expressions(statement, &mut |expr| {
        if found {
            return;
        }
        if expr_contains_blocked_word(expr) {
            found = true;
        }
    });
    if found {
        return true;
    }

    visit_selects_in_statement(statement, &mut |select| {
        if found {
            return;
        }

        for item in &select.projection {
            if let SelectItem::ExprWithAlias { alias, .. } = item {
                if token_is_blocked(&alias.value) {
                    found = true;
                    return;
                }
            }
        }

        for table in &select.from {
            if table_factor_alias_name(&table.relation).is_some_and(token_is_blocked) {
                found = true;
                return;
            }
            for join in &table.joins {
                if table_factor_alias_name(&join.relation).is_some_and(token_is_blocked) {
                    found = true;
                    return;
                }
            }
        }
    });

    found
}

fn expr_contains_blocked_word(expr: &Expr) -> bool {
    match expr {
        Expr::Identifier(ident) => token_is_blocked(&ident.value),
        Expr::CompoundIdentifier(parts) => parts.iter().any(|part| token_is_blocked(&part.value)),
        Expr::Function(function) => name_contains_blocked_word(&function.name.to_string()),
        _ => false,
    }
}

fn name_contains_blocked_word(name: &str) -> bool {
    name.split('.').any(token_is_blocked)
}

fn token_is_blocked(token: &str) -> bool {
    matches!(
        normalized_token(token).as_str(),
        "TODO" | "FIXME" | "FOO" | "BAR"
    )
}

fn normalized_token(token: &str) -> String {
    token
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '`' | '\'' | '[' | ']'))
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionBlockedWords;
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
    fn flags_blocked_word() {
        let issues = run("SELECT foo FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_009);
    }

    #[test]
    fn does_not_flag_clean_identifier() {
        assert!(run("SELECT customer_id FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_blocked_word_in_string_literal() {
        assert!(run("SELECT 'foo' AS note FROM t").is_empty());
    }

    #[test]
    fn flags_blocked_table_name() {
        let issues = run("SELECT id FROM foo");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_009);
    }

    #[test]
    fn flags_blocked_projection_alias() {
        let issues = run("SELECT amount AS bar FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_009);
    }

    #[test]
    fn flags_blocked_table_alias() {
        let issues = run("SELECT foo.id FROM users foo JOIN orders o ON foo.id = o.user_id");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_009);
    }
}
