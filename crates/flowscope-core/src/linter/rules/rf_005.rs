//! LINT_RF_005: References special chars.
//!
//! SQLFluff RF05 parity (current scope): flag identifiers containing disallowed
//! special characters with SQLFluff-style identifier policy/config controls.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

use super::identifier_candidates_helpers::{
    collect_identifier_candidates, IdentifierCandidate, IdentifierPolicy,
};

pub struct ReferencesSpecialChars {
    quoted_policy: IdentifierPolicy,
    unquoted_policy: IdentifierPolicy,
    additional_allowed_characters: HashSet<char>,
    allow_space_in_identifier: bool,
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
            allow_space_in_identifier: config
                .rule_option_bool(issue_codes::LINT_RF_005, "allow_space_in_identifier")
                .unwrap_or(false),
            ignore_words: configured_ignore_words(config)
                .into_iter()
                .map(|word| normalize_token(&word))
                .collect(),
            ignore_words_regex: config
                .rule_option_str(issue_codes::LINT_RF_005, "ignore_words_regex")
                .filter(|pattern| !pattern.trim().is_empty())
                .and_then(|pattern| Regex::new(pattern).ok()),
        }
    }
}

impl Default for ReferencesSpecialChars {
    fn default() -> Self {
        Self {
            quoted_policy: IdentifierPolicy::All,
            unquoted_policy: IdentifierPolicy::All,
            additional_allowed_characters: HashSet::new(),
            allow_space_in_identifier: false,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
        }
    }
}

impl BuiltinLintRule for ReferencesSpecialChars {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_005
    }

    fn name(&self) -> &'static str {
        "References special chars"
    }

    fn description(&self) -> &'static str {
        "Do not use special characters in identifiers."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let dialect = ctx.dialect();
        let has_special_chars = collect_identifier_candidates(statement)
            .into_iter()
            .any(|candidate| candidate_triggers_rule(&candidate, self, dialect))
            || show_tblproperties_property_key_triggers_rule(ctx.statement_sql(), self, dialect);

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

fn candidate_triggers_rule(
    candidate: &IdentifierCandidate,
    rule: &ReferencesSpecialChars,
    dialect: Dialect,
) -> bool {
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

    // Snowflake pivot references use identifiers that look like "'VALUE'" -
    // these are valid Snowflake syntax and should not be flagged.
    if candidate.quoted && candidate.value.starts_with('\'') && candidate.value.ends_with('\'') {
        return false;
    }

    // BigQuery allows hyphens, dots, and trailing wildcards in backtick identifiers
    // by default. SparkSQL/Databricks allows arbitrary file paths in backtick identifiers.
    if candidate.quote_char == Some('`') {
        match dialect {
            Dialect::Bigquery => {
                // BigQuery allows `-` and `.` throughout backtick identifiers,
                // but `*` only as a trailing wildcard suffix (e.g., `table_*`).
                let value = &candidate.value;
                let has_mid_star = value
                    .char_indices()
                    .any(|(i, ch)| ch == '*' && i + 1 < value.len());
                if has_mid_star {
                    return true;
                }
                return contains_disallowed_identifier_chars_with_extras(
                    value,
                    &rule.additional_allowed_characters,
                    rule.allow_space_in_identifier,
                    &['-', '.', '*'],
                );
            }
            Dialect::Databricks => {
                // SparkSQL/Databricks backtick identifiers can contain file
                // paths with any characters - do not flag them.
                return false;
            }
            _ => {}
        }
    }

    // BigQuery allows hyphens in unquoted identifiers as well.
    if matches!(dialect, Dialect::Bigquery) && !candidate.quoted {
        return contains_disallowed_identifier_chars_with_extras(
            &candidate.value,
            &rule.additional_allowed_characters,
            rule.allow_space_in_identifier,
            &['-', '.'],
        );
    }

    // Snowflake allows $ in identifiers (e.g. METADATA$FILENAME).
    if matches!(dialect, Dialect::Snowflake) && !candidate.quoted {
        return contains_disallowed_identifier_chars_with_extras(
            &candidate.value,
            &rule.additional_allowed_characters,
            rule.allow_space_in_identifier,
            &['$'],
        );
    }

    contains_disallowed_identifier_chars(
        &candidate.value,
        &rule.additional_allowed_characters,
        rule.allow_space_in_identifier,
    )
}

fn show_tblproperties_property_key_triggers_rule(
    sql: &str,
    rule: &ReferencesSpecialChars,
    dialect: Dialect,
) -> bool {
    if !matches!(dialect, Dialect::Databricks) {
        return false;
    }

    // SparkSQL grammar uses a string literal for the optional property key in
    // SHOW TBLPROPERTIES. SQLFluff still applies RF05 semantics to that key.
    let lowered = sql.to_ascii_lowercase();
    if !lowered.contains("show tblproperties") {
        return false;
    }

    let Some(open_paren) = sql.find('(') else {
        return false;
    };
    let Some(close_rel) = sql[open_paren + 1..].find(')') else {
        return false;
    };
    let inside = sql[open_paren + 1..open_paren + 1 + close_rel].trim();
    if inside.len() < 2 || !inside.starts_with('\'') || !inside.ends_with('\'') {
        return false;
    }

    let property_key = inside.trim_matches('\'');
    if is_ignored_token(property_key, rule) {
        return false;
    }

    contains_disallowed_identifier_chars_with_extras(
        property_key,
        &rule.additional_allowed_characters,
        rule.allow_space_in_identifier,
        &['.'],
    )
}

fn contains_disallowed_identifier_chars(
    ident: &str,
    additional_allowed: &HashSet<char>,
    allow_space: bool,
) -> bool {
    ident.chars().any(|ch| {
        !(ch.is_ascii_alphanumeric()
            || ch == '_'
            || (allow_space && ch == ' ')
            || additional_allowed.contains(&ch))
    })
}

fn contains_disallowed_identifier_chars_with_extras(
    ident: &str,
    additional_allowed: &HashSet<char>,
    allow_space: bool,
    extras: &[char],
) -> bool {
    ident.chars().any(|ch| {
        !(ch.is_ascii_alphanumeric()
            || ch == '_'
            || (allow_space && ch == ' ')
            || extras.contains(&ch)
            || additional_allowed.contains(&ch))
    })
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
    // ignore_words matches case-insensitively (via normalization).
    if rule.ignore_words.contains(&normalized) {
        return true;
    }
    // ignore_words_regex matches case-sensitively against the raw value,
    // consistent with SQLFluff behavior.
    if let Some(regex) = &rule.ignore_words_regex {
        let raw = token
            .trim()
            .trim_matches(|ch| matches!(ch, '"' | '`' | '\'' | '[' | ']'));
        if regex.is_match(raw) {
            return true;
        }
    }
    false
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
    use crate::parser::parse_sql_with_dialect;
    use crate::types::Dialect;

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
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn run_in_dialect(sql: &str, dialect: Dialect) -> Vec<Issue> {
        let statements = parse_sql_with_dialect(sql, dialect).expect("parse");
        let rule = ReferencesSpecialChars::default();
        let mut issues = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            issues.extend(rule.check_with_context(
                statement,
                &RuleContext::new(sql, 0..sql.len(), index).with_dialect(dialect),
            ));
        }
        issues
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
                    serde_json::json!({"ignore_words_regex": "^bad-"}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_words_regex_is_case_sensitive() {
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
        assert_eq!(issues.len(), 1, "regex should be case-sensitive");
    }

    #[test]
    fn flags_create_table_column_with_space() {
        let issues = run("CREATE TABLE DBO.ColumnNames (\n    \"Internal Space\" INT\n)");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_005);
    }

    #[test]
    fn allow_space_in_identifier_permits_space() {
        let issues = run_with_config(
            "CREATE TABLE DBO.ColumnNames (\n    \"Internal Space\" INT\n)",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "references.special_chars".to_string(),
                    serde_json::json!({"allow_space_in_identifier": true}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn sparksql_show_tblproperties_allows_dot_in_property_key() {
        let issues = run_in_dialect(
            "SHOW TBLPROPERTIES customer ('created.date');",
            Dialect::Databricks,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn sparksql_show_tblproperties_flags_wildcard_in_property_key() {
        let issues = run_in_dialect(
            "SHOW TBLPROPERTIES customer ('created.*');",
            Dialect::Databricks,
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_005);
    }
}
