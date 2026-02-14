//! LINT_CV_009: Blocked words.
//!
//! SQLFluff CV09 parity (current scope): detect placeholder words such as
//! TODO/FIXME/foo/bar.

use crate::extractors::extract_tables;
use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit::visit_expressions;
use crate::types::{issue_codes, Issue};
use regex::{Regex, RegexBuilder};
use sqlparser::ast::{Expr, SelectItem, Statement};
use std::collections::HashSet;

use super::semantic_helpers::{table_factor_alias_name, visit_selects_in_statement};

pub struct ConventionBlockedWords {
    blocked_words: HashSet<String>,
    blocked_regex: Option<Regex>,
}

impl ConventionBlockedWords {
    pub fn from_config(config: &LintConfig) -> Self {
        let blocked_words = configured_blocked_words(config)
            .unwrap_or_else(default_blocked_words)
            .into_iter()
            .map(|word| normalized_token(&word))
            .collect();

        let blocked_regex = config
            .rule_option_str(issue_codes::LINT_CV_009, "blocked_regex")
            .filter(|pattern| !pattern.trim().is_empty())
            .and_then(|pattern| {
                RegexBuilder::new(pattern)
                    .case_insensitive(true)
                    .build()
                    .ok()
            });

        Self {
            blocked_words,
            blocked_regex,
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
            blocked_regex: None,
        }
    }
}

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
        if statement_contains_blocked_word(statement, self) {
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
            .blocked_regex
            .as_ref()
            .is_some_and(|regex| regex.is_match(&normalized))
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
        let rule = ConventionBlockedWords::default();
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
}
