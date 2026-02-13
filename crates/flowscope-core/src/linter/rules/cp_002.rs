//! LINT_CP_002: Identifier capitalisation.
//!
//! SQLFluff CP02 parity (current scope): detect inconsistent identifier case.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

use super::capitalisation_policy_helpers::{
    ignored_words_from_config, ignored_words_regex_from_config, token_is_ignored,
    tokens_violate_policy, CapitalisationPolicy,
};
use super::identifier_candidates_helpers::{collect_identifier_candidates, IdentifierPolicy};

pub struct CapitalisationIdentifiers {
    policy: CapitalisationPolicy,
    unquoted_policy: IdentifierPolicy,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
}

impl CapitalisationIdentifiers {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            policy: CapitalisationPolicy::from_rule_config(
                config,
                issue_codes::LINT_CP_002,
                "extended_capitalisation_policy",
            ),
            unquoted_policy: IdentifierPolicy::from_config(
                config,
                issue_codes::LINT_CP_002,
                "unquoted_identifiers_policy",
                "all",
            ),
            ignore_words: ignored_words_from_config(config, issue_codes::LINT_CP_002),
            ignore_words_regex: ignored_words_regex_from_config(config, issue_codes::LINT_CP_002),
        }
    }
}

impl Default for CapitalisationIdentifiers {
    fn default() -> Self {
        Self {
            policy: CapitalisationPolicy::Consistent,
            unquoted_policy: IdentifierPolicy::All,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
        }
    }
}

impl LintRule for CapitalisationIdentifiers {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_002
    }

    fn name(&self) -> &'static str {
        "Identifier capitalisation"
    }

    fn description(&self) -> &'static str {
        "Identifiers should use a consistent case style."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let identifiers = identifier_tokens(
            statement,
            self.unquoted_policy,
            &self.ignore_words,
            self.ignore_words_regex.as_ref(),
        );
        if !tokens_violate_policy(&identifiers, self.policy) {
            return Vec::new();
        }

        vec![Issue::info(
            issue_codes::LINT_CP_002,
            "Identifiers use inconsistent capitalisation.",
        )
        .with_statement(ctx.statement_index)]
    }
}

fn identifier_tokens(
    statement: &Statement,
    unquoted_policy: IdentifierPolicy,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
) -> Vec<String> {
    collect_identifier_candidates(statement)
        .into_iter()
        .filter_map(|candidate| {
            if candidate.quoted || !unquoted_policy.allows(candidate.kind) {
                return None;
            }

            if token_is_ignored(candidate.value.as_str(), ignore_words, ignore_words_regex) {
                return None;
            }

            Some(candidate.value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql_with_dialect;
    use crate::types::Dialect;

    fn run(sql: &str) -> Vec<Issue> {
        run_with_config(sql, LintConfig::default())
    }

    fn run_with_config(sql: &str, config: LintConfig) -> Vec<Issue> {
        run_with_config_in_dialect(sql, Dialect::Generic, config)
    }

    fn run_with_config_in_dialect(sql: &str, dialect: Dialect, config: LintConfig) -> Vec<Issue> {
        let statements = parse_sql_with_dialect(sql, dialect).expect("parse");
        let rule = CapitalisationIdentifiers::from_config(&config);
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
    fn flags_mixed_identifier_case() {
        let issues = run("SELECT Col, col FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_002);
    }

    #[test]
    fn does_not_flag_consistent_identifiers() {
        assert!(run("SELECT col_one, col_two FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_identifier_like_words_in_strings_or_comments() {
        let sql = "SELECT 'Col col' AS txt -- Col col\nFROM t";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn upper_policy_flags_lowercase_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.identifiers".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "upper"}),
            )]),
        };
        let issues = run_with_config("SELECT col FROM t", config);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn ignore_words_regex_excludes_identifiers_from_check() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.identifiers".to_string(),
                serde_json::json!({"ignore_words_regex": "^col$"}),
            )]),
        };
        let issues = run_with_config("SELECT Col, col FROM t", config);
        assert!(issues.is_empty());
    }

    #[test]
    fn aliases_policy_ignores_non_alias_identifiers() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.identifiers".to_string(),
                serde_json::json!({"unquoted_identifiers_policy": "aliases"}),
            )]),
        };
        let issues = run_with_config("SELECT Col AS alias FROM t", config);
        assert!(issues.is_empty());
    }

    #[test]
    fn column_alias_policy_flags_mixed_column_alias_case() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CP_002".to_string(),
                serde_json::json!({"unquoted_identifiers_policy": "column_aliases"}),
            )]),
        };
        let issues = run_with_config("SELECT amount AS Col, amount AS col FROM t", config);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_002);
    }

    #[test]
    fn consistent_policy_allows_single_letter_upper_with_capitalised_identifier() {
        let issues = run("SELECT A, Boo");
        assert!(issues.is_empty());
    }

    #[test]
    fn pascal_policy_allows_all_caps_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.identifiers".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "pascal"}),
            )]),
        };
        let issues = run_with_config("SELECT PASCALCASE", config);
        assert!(issues.is_empty());
    }

    #[test]
    fn databricks_tblproperties_mixed_case_property_is_flagged() {
        let issues = run_with_config_in_dialect(
            "SHOW TBLPROPERTIES customer (created.BY.user)",
            Dialect::Databricks,
            LintConfig::default(),
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_002);
    }

    #[test]
    fn databricks_tblproperties_lowercase_property_is_allowed() {
        let issues = run_with_config_in_dialect(
            "SHOW TBLPROPERTIES customer (created.by.user)",
            Dialect::Databricks,
            LintConfig::default(),
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn databricks_tblproperties_capitalised_property_is_flagged() {
        let issues = run_with_config_in_dialect(
            "SHOW TBLPROPERTIES customer (Created.By.User)",
            Dialect::Databricks,
            LintConfig::default(),
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_002);
    }

    #[test]
    fn flags_mixed_identifier_case_in_delete_predicate() {
        let issues = run("DELETE FROM t WHERE Col = col");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_002);
    }

    #[test]
    fn flags_mixed_identifier_case_in_update_assignment() {
        let issues = run("UPDATE t SET Col = col");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_002);
    }
}
