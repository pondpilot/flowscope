//! LINT_RF_004: References keywords.
//!
//! SQLFluff RF04 parity (current scope): avoid keyword-looking aliases in
//! explicit `FROM/JOIN ... AS <alias>` patterns.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

use super::semantic_helpers::{table_factor_alias_name, visit_selects_in_statement};

pub struct ReferencesKeywords;

impl LintRule for ReferencesKeywords {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_004
    }

    fn name(&self) -> &'static str {
        "References keywords"
    }

    fn description(&self) -> &'static str {
        "Avoid keywords as identifiers."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut has_keyword_alias = false;
        visit_selects_in_statement(statement, &mut |select| {
            if has_keyword_alias {
                return;
            }

            for table in &select.from {
                if table_factor_alias_name(&table.relation).is_some_and(is_keyword) {
                    has_keyword_alias = true;
                    return;
                }

                for join in &table.joins {
                    if table_factor_alias_name(&join.relation).is_some_and(is_keyword) {
                        has_keyword_alias = true;
                        return;
                    }
                }
            }
        });

        if has_keyword_alias {
            vec![Issue::info(
                issue_codes::LINT_RF_004,
                "Keyword used as identifier alias.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn is_keyword(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "SELECT"
            | "FROM"
            | "WHERE"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "FULL"
            | "OUTER"
            | "INNER"
            | "CROSS"
            | "ON"
            | "USING"
            | "AS"
            | "GROUP"
            | "ORDER"
            | "HAVING"
            | "LIMIT"
            | "OFFSET"
            | "UNION"
            | "ALL"
            | "DISTINCT"
            | "BY"
            | "WHEN"
            | "THEN"
            | "ELSE"
            | "END"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesKeywords;
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
    fn flags_keyword_table_alias() {
        let issues = run("SELECT \"select\".id FROM users AS \"select\"");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn does_not_flag_non_keyword_alias() {
        let issues = run("SELECT u.id FROM users AS u");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_sql_like_string_literal() {
        let issues = run("SELECT 'FROM users AS date' AS snippet");
        assert!(issues.is_empty());
    }
}
