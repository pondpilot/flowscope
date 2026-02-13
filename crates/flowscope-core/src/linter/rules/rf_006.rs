//! LINT_RF_006: References quoting.
//!
//! SQLFluff RF06 parity (current scope): quoted identifiers that are valid
//! bare identifiers are treated as unnecessarily quoted.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::{Regex, RegexBuilder};
use sqlparser::ast::Statement;

use super::identifier_candidates_helpers::{
    collect_identifier_candidates, IdentifierCandidate, IdentifierPolicy,
};

pub struct ReferencesQuoting {
    prefer_quoted_identifiers: bool,
    prefer_quoted_keywords: bool,
    case_sensitive: bool,
    quoted_policy: IdentifierPolicy,
    unquoted_policy: IdentifierPolicy,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
}

impl Default for ReferencesQuoting {
    fn default() -> Self {
        Self {
            prefer_quoted_identifiers: false,
            prefer_quoted_keywords: false,
            case_sensitive: false,
            quoted_policy: IdentifierPolicy::All,
            unquoted_policy: IdentifierPolicy::All,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
        }
    }
}

impl ReferencesQuoting {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            prefer_quoted_identifiers: config
                .rule_option_bool(issue_codes::LINT_RF_006, "prefer_quoted_identifiers")
                .unwrap_or(false),
            prefer_quoted_keywords: config
                .rule_option_bool(issue_codes::LINT_RF_006, "prefer_quoted_keywords")
                .unwrap_or(false),
            case_sensitive: config
                .rule_option_bool(issue_codes::LINT_RF_006, "case_sensitive")
                .unwrap_or(false),
            quoted_policy: IdentifierPolicy::from_config(
                config,
                issue_codes::LINT_RF_006,
                "quoted_identifiers_policy",
                "all",
            ),
            unquoted_policy: IdentifierPolicy::from_config(
                config,
                issue_codes::LINT_RF_006,
                "unquoted_identifiers_policy",
                "all",
            ),
            ignore_words: configured_ignore_words(config)
                .into_iter()
                .map(|word| normalize_token(&word))
                .collect(),
            ignore_words_regex: config
                .rule_option_str(issue_codes::LINT_RF_006, "ignore_words_regex")
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
        let has_violation = collect_identifier_candidates(statement)
            .into_iter()
            .any(|candidate| candidate_triggers_rule(&candidate, self));

        if has_violation {
            let message = if self.prefer_quoted_identifiers {
                "Identifiers should be quoted."
            } else {
                "Identifier quoting appears unnecessary."
            };
            vec![Issue::info(issue_codes::LINT_RF_006, message).with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn candidate_triggers_rule(candidate: &IdentifierCandidate, rule: &ReferencesQuoting) -> bool {
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

    if rule.prefer_quoted_identifiers {
        return !candidate.quoted;
    }

    if !candidate.quoted {
        return false;
    }

    if rule.prefer_quoted_keywords && is_keyword(&candidate.value) {
        return false;
    }

    is_unnecessarily_quoted_identifier(&candidate.value, rule.case_sensitive)
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

fn configured_ignore_words(config: &LintConfig) -> Vec<String> {
    if let Some(words) = config.rule_option_string_list(issue_codes::LINT_RF_006, "ignore_words") {
        return words;
    }

    config
        .rule_option_str(issue_codes::LINT_RF_006, "ignore_words")
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

fn is_ignored_token(token: &str, rule: &ReferencesQuoting) -> bool {
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

fn is_keyword(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_uppercase().as_str(),
        "ALL"
            | "AS"
            | "BY"
            | "CASE"
            | "CROSS"
            | "DISTINCT"
            | "ELSE"
            | "END"
            | "FROM"
            | "FULL"
            | "GROUP"
            | "HAVING"
            | "INNER"
            | "JOIN"
            | "LEFT"
            | "LIMIT"
            | "OFFSET"
            | "ON"
            | "ORDER"
            | "OUTER"
            | "RECURSIVE"
            | "RIGHT"
            | "SELECT"
            | "SUM"
            | "THEN"
            | "UNION"
            | "USING"
            | "WHEN"
            | "WHERE"
            | "WITH"
    )
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
        let sql = "SELECT \"good_name\" FROM \"t\"";
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

    #[test]
    fn prefer_quoted_identifiers_true_flags_unquoted_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.quoting".to_string(),
                serde_json::json!({"prefer_quoted_identifiers": true}),
            )]),
        };
        let rule = ReferencesQuoting::from_config(&config);
        let sql = "SELECT good_name FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn prefer_quoted_keywords_true_allows_quoted_keyword_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_RF_006".to_string(),
                serde_json::json!({"prefer_quoted_keywords": true}),
            )]),
        };
        let rule = ReferencesQuoting::from_config(&config);
        let sql = "SELECT \"select\".id FROM users AS \"select\"";
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
    fn quoted_policy_none_skips_quoted_identifier_checks() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.quoting".to_string(),
                serde_json::json!({"quoted_identifiers_policy": "none"}),
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
    fn ignore_words_suppresses_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_RF_006".to_string(),
                serde_json::json!({"ignore_words": ["good_name"]}),
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
    fn ignore_words_regex_suppresses_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.quoting".to_string(),
                serde_json::json!({"ignore_words_regex": "^GOOD_"}),
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
}
