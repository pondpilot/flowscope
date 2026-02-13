//! LINT_RF_004: References keywords.
//!
//! SQLFluff RF04 parity (current scope): avoid keyword-looking aliases in
//! explicit `FROM/JOIN ... AS <alias>` patterns.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

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

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_keyword_alias = capture_group(
            ctx.statement_sql(),
            r"(?i)\b(?:from|join)\s+[A-Za-z_][A-Za-z0-9_\.]*\s+as\s+([A-Za-z_][A-Za-z0-9_]*)",
            1,
        )
        .into_iter()
        .any(|alias| is_keyword(&alias));

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

fn capture_group(sql: &str, pattern: &str, group: usize) -> Vec<String> {
    Regex::new(pattern)
        .expect("valid regex")
        .captures_iter(sql)
        .filter_map(|captures| captures.get(group).map(|m| m.as_str().to_string()))
        .collect()
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
    fn flags_keyword_alias_pattern() {
        // Preserves current regex-based parity behavior.
        let issues = run("SELECT 'FROM tbl AS SELECT' AS sql_snippet");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn does_not_flag_non_keyword_alias_pattern() {
        let issues = run("SELECT 'FROM tbl AS alias_name' AS sql_snippet");
        assert!(issues.is_empty());
    }
}
