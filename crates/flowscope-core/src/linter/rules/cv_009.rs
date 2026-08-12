//! LINT_CV_009: Blocked words.
//!
//! SQLFluff CV09 parity (current scope): detect placeholder words such as
//! TODO/FIXME/foo/bar.

use crate::extractors::extract_tables;
use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::linter::visit::visit_expressions;
use crate::types::{issue_codes, Issue};
use regex::{Regex, RegexBuilder};
use sqlparser::ast::{Expr, SelectItem, Statement};
use std::collections::HashSet;

use super::semantic_helpers::{table_factor_alias_name, visit_selects_in_statement};

pub struct ConventionBlockedWords {
    blocked_words: HashSet<String>,
    blocked_regexes: Vec<Regex>,
    match_source: bool,
    ignore_templated_areas: bool,
}

impl ConventionBlockedWords {
    pub fn from_config(config: &LintConfig) -> Self {
        let blocked_words = configured_blocked_words(config)
            .unwrap_or_else(default_blocked_words)
            .into_iter()
            .map(|word| normalized_token(&word))
            .collect();

        let blocked_regexes = configured_blocked_regexes(config);
        let match_source = config
            .rule_option_bool(issue_codes::LINT_CV_009, "match_source")
            .unwrap_or(false);
        let ignore_templated_areas = config
            .core_option_bool("ignore_templated_areas")
            .unwrap_or(true);

        Self {
            blocked_words,
            blocked_regexes,
            match_source,
            ignore_templated_areas,
        }
    }
}

impl Default for ConventionBlockedWords {
    fn default() -> Self {
        Self {
            blocked_words: default_blocked_words()
                .into_iter()
                .map(|word| normalized_token(&word))
                .collect(),
            blocked_regexes: Vec::new(),
            match_source: false,
            ignore_templated_areas: true,
        }
    }
}

impl BuiltinLintRule for ConventionBlockedWords {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_009
    }

    fn name(&self) -> &'static str {
        "Blocked words"
    }

    fn description(&self) -> &'static str {
        "Block a list of configurable words from being used."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let source_violation = if self.match_source && ctx.statement_index == 0 {
            let source = if self.ignore_templated_areas {
                mask_templated_areas(ctx.sql)
            } else {
                ctx.sql.to_string()
            };
            self.blocked_regexes
                .iter()
                .any(|regex| regex.is_match(&source))
        } else {
            false
        };

        if source_violation || statement_contains_blocked_word(statement, self) {
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

fn configured_blocked_words(config: &LintConfig) -> Option<Vec<String>> {
    if let Some(words) = config.rule_option_string_list(issue_codes::LINT_CV_009, "blocked_words") {
        return Some(words);
    }

    config
        .rule_option_str(issue_codes::LINT_CV_009, "blocked_words")
        .map(|words| {
            words
                .split(',')
                .map(str::trim)
                .filter(|word| !word.is_empty())
                .map(str::to_string)
                .collect()
        })
}

fn configured_blocked_regexes(config: &LintConfig) -> Vec<Regex> {
    let mut patterns = Vec::new();

    if let Some(list) = config.rule_option_string_list(issue_codes::LINT_CV_009, "blocked_regex") {
        patterns.extend(list);
    } else if let Some(pattern) = config.rule_option_str(issue_codes::LINT_CV_009, "blocked_regex")
    {
        patterns.push(pattern.to_string());
    }

    patterns
        .into_iter()
        .filter_map(|pattern| {
            let trimmed = pattern.trim();
            if trimmed.is_empty() {
                None
            } else {
                RegexBuilder::new(trimmed)
                    .case_insensitive(true)
                    .build()
                    .ok()
            }
        })
        .collect()
}

fn default_blocked_words() -> Vec<String> {
    vec![
        "TODO".to_string(),
        "FIXME".to_string(),
        "foo".to_string(),
        "bar".to_string(),
    ]
}

fn statement_contains_blocked_word(statement: &Statement, config: &ConventionBlockedWords) -> bool {
    if extract_tables(std::slice::from_ref(statement))
        .into_iter()
        .any(|name| name_contains_blocked_word(&name, config))
    {
        return true;
    }

    let mut found = false;
    visit_expressions(statement, &mut |expr| {
        if found {
            return;
        }
        if expr_contains_blocked_word(expr, config) {
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
                if token_is_blocked(&alias.value, config) {
                    found = true;
                    return;
                }
            }
        }

        for table in &select.from {
            if table_factor_alias_name(&table.relation)
                .is_some_and(|alias| token_is_blocked(alias, config))
            {
                found = true;
                return;
            }
            for join in &table.joins {
                if table_factor_alias_name(&join.relation)
                    .is_some_and(|alias| token_is_blocked(alias, config))
                {
                    found = true;
                    return;
                }
            }
        }
    });

    found
}

fn expr_contains_blocked_word(expr: &Expr, config: &ConventionBlockedWords) -> bool {
    match expr {
        Expr::Identifier(ident) => token_is_blocked(&ident.value, config),
        Expr::CompoundIdentifier(parts) => parts
            .iter()
            .any(|part| token_is_blocked(&part.value, config)),
        Expr::Function(function) => name_contains_blocked_word(&function.name.to_string(), config),
        _ => false,
    }
}

fn name_contains_blocked_word(name: &str, config: &ConventionBlockedWords) -> bool {
    name.split('.').any(|token| token_is_blocked(token, config))
}

fn token_is_blocked(token: &str, config: &ConventionBlockedWords) -> bool {
    let normalized = normalized_token(token);
    config.blocked_words.contains(&normalized)
        || config
            .blocked_regexes
            .iter()
            .any(|regex| regex.is_match(&normalized))
}

fn normalized_token(token: &str) -> String {
    token
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '`' | '\'' | '[' | ']'))
        .to_ascii_uppercase()
}

fn mask_templated_areas(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut index = 0usize;

    while let Some((open_index, close_marker)) = find_next_template_open(sql, index) {
        out.push_str(&sql[index..open_index]);
        let marker_start = open_index + 2;
        if let Some(close_offset) = sql[marker_start..].find(close_marker) {
            let close_index = marker_start + close_offset + close_marker.len();
            out.push_str(&mask_non_newlines(&sql[open_index..close_index]));
            index = close_index;
        } else {
            out.push_str(&mask_non_newlines(&sql[open_index..]));
            return out;
        }
    }

    out.push_str(&sql[index..]);
    out
}

fn find_next_template_open(sql: &str, from: usize) -> Option<(usize, &'static str)> {
    let rest = sql.get(from..)?;
    let candidates = [("{{", "}}"), ("{%", "%}"), ("{#", "#}")];

    candidates
        .into_iter()
        .filter_map(|(open, close)| rest.find(open).map(|offset| (from + offset, close)))
        .min_by_key(|(index, _)| *index)
}

fn mask_non_newlines(segment: &str) -> String {
    segment
        .chars()
        .map(|ch| if ch == '\n' { '\n' } else { ' ' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionBlockedWords::default();
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
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

    #[test]
    fn configured_blocked_words_override_default_list() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.blocked_words".to_string(),
                serde_json::json!({"blocked_words": ["wip"]}),
            )]),
        };
        let rule = ConventionBlockedWords::from_config(&config);
        let sql = "SELECT foo, wip FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn configured_blocked_regex_matches_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_009".to_string(),
                serde_json::json!({"blocked_words": [], "blocked_regex": "^TMP_"}),
            )]),
        };
        let rule = ConventionBlockedWords::from_config(&config);
        let sql = "SELECT tmp_value FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn blocked_regex_array_matches_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_009".to_string(),
                serde_json::json!({"blocked_words": [], "blocked_regex": ["^TMP_", "^WIP_"]}),
            )]),
        };
        let rule = ConventionBlockedWords::from_config(&config);
        let sql = "SELECT wip_item FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn match_source_true_allows_raw_sql_regex_matching() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.blocked_words".to_string(),
                serde_json::json!({"blocked_words": [], "blocked_regex": "TODO", "match_source": true}),
            )]),
        };
        let rule = ConventionBlockedWords::from_config(&config);
        let sql = "SELECT 'TODO' AS note FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn match_source_true_checks_full_source_in_statementless_mode() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([
                (
                    "core".to_string(),
                    serde_json::json!({"ignore_templated_areas": false}),
                ),
                (
                    "convention.blocked_words".to_string(),
                    serde_json::json!({
                        "blocked_words": [],
                        "blocked_regex": "ref\\('deprecated_",
                        "match_source": true
                    }),
                ),
            ]),
        };
        let rule = ConventionBlockedWords::from_config(&config);
        let sql = "SELECT * FROM {{ ref('deprecated_table') }}";
        let synthetic = parse_sql("SELECT 1").expect("parse");
        let issues =
            rule.check_with_context(&synthetic[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_009);
    }

    #[test]
    fn match_source_true_respects_ignore_templated_areas_core_option() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([
                (
                    "core".to_string(),
                    serde_json::json!({"ignore_templated_areas": true}),
                ),
                (
                    "convention.blocked_words".to_string(),
                    serde_json::json!({
                        "blocked_words": [],
                        "blocked_regex": "ref\\('deprecated_",
                        "match_source": true
                    }),
                ),
            ]),
        };
        let rule = ConventionBlockedWords::from_config(&config);
        let sql = "SELECT * FROM {{ ref('deprecated_table') }}";
        let synthetic = parse_sql("SELECT 1").expect("parse");
        let issues =
            rule.check_with_context(&synthetic[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }
}
