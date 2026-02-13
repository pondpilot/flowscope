//! LINT_RF_005: References special chars.
//!
//! SQLFluff RF05 parity (current scope): flag identifiers containing disallowed
//! special characters with SQLFluff-style identifier policy/config controls.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::{Regex, RegexBuilder};
use sqlparser::ast::Statement;

use super::identifier_candidates_helpers::{
    collect_identifier_candidates, IdentifierCandidate, IdentifierPolicy,
};

pub struct ReferencesSpecialChars {
    quoted_policy: IdentifierPolicy,
    unquoted_policy: IdentifierPolicy,
    additional_allowed_characters: HashSet<char>,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
}

impl ReferencesSpecialChars {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            quoted_policy: IdentifierPolicy::from_config(
                config,
                issue_codes::LINT_RF_005,
                "quoted_identifiers_policy",
                "all",
            ),
            unquoted_policy: IdentifierPolicy::from_config(
                config,
                issue_codes::LINT_RF_005,
                "unquoted_identifiers_policy",
                "all",
            ),
            additional_allowed_characters: configured_additional_allowed_characters(config),
            ignore_words: configured_ignore_words(config)
                .into_iter()
                .map(|word| normalize_token(&word))
                .collect(),
            ignore_words_regex: config
                .rule_option_str(issue_codes::LINT_RF_005, "ignore_words_regex")
                .filter(|pattern| !pattern.trim().is_empty())
                .and_then(|pattern| {
                    RegexBuilder::new(pattern)
                        .case_insensitive(true)
                        .build()
                        .ok()
                }),
        }
    }
}

impl Default for ReferencesSpecialChars {
    fn default() -> Self {
        Self {
            quoted_policy: IdentifierPolicy::All,
            unquoted_policy: IdentifierPolicy::All,
            additional_allowed_characters: HashSet::new(),
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
        }
    }
}

impl LintRule for ReferencesSpecialChars {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_005
    }

    fn name(&self) -> &'static str {
        "References special chars"
    }

    fn description(&self) -> &'static str {
        "Avoid unsupported special characters in identifiers."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_special_chars = collect_identifier_candidates(statement)
            .into_iter()
            .any(|candidate| candidate_triggers_rule(&candidate, self));

        if has_special_chars {
            vec![Issue::warning(
                issue_codes::LINT_RF_005,
                "Identifier contains unsupported special characters.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn candidate_triggers_rule(candidate: &IdentifierCandidate, rule: &ReferencesSpecialChars) -> bool {
    if is_ignored_token(&candidate.value, rule) {
        return false;
    }

    let policy = if candidate.quoted {
        rule.quoted_policy
    } else {
        rule.unquoted_policy
    };
    if !policy.allows(candidate.kind) {
        return false;
    }

    contains_disallowed_identifier_chars(&candidate.value, &rule.additional_allowed_characters)
}

fn contains_disallowed_identifier_chars(ident: &str, additional_allowed: &HashSet<char>) -> bool {
    ident
        .chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || additional_allowed.contains(&ch)))
}

fn configured_additional_allowed_characters(config: &LintConfig) -> HashSet<char> {
    if let Some(values) =
        config.rule_option_string_list(issue_codes::LINT_RF_005, "additional_allowed_characters")
    {
        let mut chars = HashSet::new();
        for value in values {
            chars.extend(value.chars());
        }
        return chars;
    }

    config
        .rule_option_str(issue_codes::LINT_RF_005, "additional_allowed_characters")
        .map(|value| {
            value
                .split(',')
                .flat_map(|item| item.trim().chars())
                .collect()
        })
        .unwrap_or_default()
}

fn configured_ignore_words(config: &LintConfig) -> Vec<String> {
    if let Some(words) = config.rule_option_string_list(issue_codes::LINT_RF_005, "ignore_words") {
        return words;
    }

    config
        .rule_option_str(issue_codes::LINT_RF_005, "ignore_words")
        .map(|words| {
            words
                .split(',')
                .map(str::trim)
                .filter(|word| !word.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn is_ignored_token(token: &str, rule: &ReferencesSpecialChars) -> bool {
    let normalized = normalize_token(token);
    rule.ignore_words.contains(&normalized)
        || rule
            .ignore_words_regex
            .as_ref()
            .is_some_and(|regex| regex.is_match(&normalized))
}

fn normalize_token(token: &str) -> String {
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
        run_with_config(sql, LintConfig::default())
    }

    fn run_with_config(sql: &str, config: LintConfig) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesSpecialChars::from_config(&config);
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
    fn flags_quoted_identifier_with_hyphen() {
        let issues = run("SELECT \"bad-name\" FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_005);
    }

    #[test]
    fn does_not_flag_quoted_identifier_with_underscore() {
        let issues = run("SELECT \"good_name\" FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_double_quotes_inside_string_literal() {
        let issues = run("SELECT '\"bad-name\"' AS note FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn additional_allowed_characters_permit_hyphen() {
        let issues = run_with_config(
            "SELECT \"bad-name\" FROM t",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "references.special_chars".to_string(),
                    serde_json::json!({"additional_allowed_characters": "-"}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn quoted_policy_none_skips_quoted_identifier_checks() {
        let issues = run_with_config(
            "SELECT \"bad-name\" FROM t",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "LINT_RF_005".to_string(),
                    serde_json::json!({"quoted_identifiers_policy": "none"}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_words_suppresses_configured_identifier() {
        let issues = run_with_config(
            "SELECT \"bad-name\" FROM t",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "references.special_chars".to_string(),
                    serde_json::json!({"ignore_words": ["bad-name"]}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_words_regex_suppresses_configured_identifier() {
        let issues = run_with_config(
            "SELECT \"bad-name\" FROM t",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "LINT_RF_005".to_string(),
                    serde_json::json!({"ignore_words_regex": "^BAD-"}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }
}
