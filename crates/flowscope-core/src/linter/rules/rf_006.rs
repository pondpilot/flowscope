//! LINT_RF_006: References quoting.
//!
//! SQLFluff RF06 parity (current scope): quoted identifiers that are valid
//! bare identifiers are treated as unnecessarily quoted.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

use super::references_quoted_helpers::quoted_identifiers_in_statement;

#[derive(Default)]
pub struct ReferencesQuoting {
    prefer_quoted_identifiers: bool,
    case_sensitive: bool,
}

impl ReferencesQuoting {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            prefer_quoted_identifiers: config
                .rule_option_bool(issue_codes::LINT_RF_006, "prefer_quoted_identifiers")
                .unwrap_or(false),
            case_sensitive: config
                .rule_option_bool(issue_codes::LINT_RF_006, "case_sensitive")
                .unwrap_or(false),
        }
    }
}

impl LintRule for ReferencesQuoting {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_006
    }

    fn name(&self) -> &'static str {
        "References quoting"
    }

    fn description(&self) -> &'static str {
        "Avoid unnecessary identifier quoting."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if self.prefer_quoted_identifiers {
            return Vec::new();
        }

        let has_unnecessary_quoting = quoted_identifiers_in_statement(statement)
            .into_iter()
            .any(|ident| is_unnecessarily_quoted_identifier(&ident, self.case_sensitive));

        if has_unnecessary_quoting {
            vec![Issue::info(
                issue_codes::LINT_RF_006,
                "Identifier quoting appears unnecessary.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn is_unnecessarily_quoted_identifier(ident: &str, case_sensitive: bool) -> bool {
    let mut chars = ident.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    if !chars
        .clone()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return false;
    }

    if case_sensitive && ident.chars().any(|ch| ch.is_ascii_uppercase()) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesQuoting::default();
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
    fn flags_unnecessary_quoted_identifier() {
        let issues = run("SELECT \"good_name\" FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_006);
    }

    #[test]
    fn does_not_flag_quoted_identifier_with_special_char() {
        let issues = run("SELECT \"bad-name\" FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_double_quotes_inside_string_literal() {
        let issues = run("SELECT '\"good_name\"' AS note FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn prefer_quoted_identifiers_true_disables_unnecessary_quote_issues() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.quoting".to_string(),
                serde_json::json!({"prefer_quoted_identifiers": true}),
            )]),
        };
        let rule = ReferencesQuoting::from_config(&config);
        let sql = "SELECT \"good_name\" FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn case_sensitive_true_allows_quoted_mixed_case_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_RF_006".to_string(),
                serde_json::json!({"case_sensitive": true}),
            )]),
        };
        let rule = ReferencesQuoting::from_config(&config);
        let sql = "SELECT \"MixedCase\" FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert!(issues.is_empty());
    }
}
