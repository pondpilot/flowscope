//! LINT_CP_001: Keyword capitalisation.
//!
//! SQLFluff CP01 parity (current scope): detect mixed-case keyword usage.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer};

use super::capitalisation_policy_helpers::{tokens_violate_policy, CapitalisationPolicy};

pub struct CapitalisationKeywords {
    policy: CapitalisationPolicy,
    ignore_words: HashSet<String>,
}

impl CapitalisationKeywords {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            policy: CapitalisationPolicy::from_rule_config(
                config,
                issue_codes::LINT_CP_001,
                "capitalisation_policy",
            ),
            ignore_words: ignored_words_from_config(config),
        }
    }
}

impl Default for CapitalisationKeywords {
    fn default() -> Self {
        Self {
            policy: CapitalisationPolicy::Consistent,
            ignore_words: HashSet::new(),
        }
    }
}

impl LintRule for CapitalisationKeywords {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_001
    }

    fn name(&self) -> &'static str {
        "Keyword capitalisation"
    }

    fn description(&self) -> &'static str {
        "SQL keywords should use a consistent case style."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if tokens_violate_policy(
            &keyword_tokens(ctx.statement_sql(), &self.ignore_words),
            self.policy,
        ) {
            vec![Issue::info(
                issue_codes::LINT_CP_001,
                "SQL keywords use inconsistent capitalisation.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn ignored_words_from_config(config: &LintConfig) -> HashSet<String> {
    if let Some(words) = config.rule_option_string_list(issue_codes::LINT_CP_001, "ignore_words") {
        return words
            .into_iter()
            .map(|word| word.trim().to_ascii_uppercase())
            .filter(|word| !word.is_empty())
            .collect();
    }

    config
        .rule_option_str(issue_codes::LINT_CP_001, "ignore_words")
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|word| !word.is_empty())
                .map(str::to_ascii_uppercase)
                .collect()
        })
        .unwrap_or_default()
}

fn keyword_tokens(sql: &str, ignore_words: &HashSet<String>) -> Vec<String> {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return Vec::new();
    };

    tokens
        .into_iter()
        .filter_map(|token| match token {
            Token::Word(word)
                if word.keyword != Keyword::NoKeyword
                    && is_tracked_keyword(word.value.as_str())
                    && !ignore_words.contains(&word.value.to_ascii_uppercase()) =>
            {
                Some(word.value)
            }
            _ => None,
        })
        .collect()
}

fn is_tracked_keyword(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "SELECT"
            | "FROM"
            | "WHERE"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "FULL"
            | "INNER"
            | "OUTER"
            | "ON"
            | "GROUP"
            | "BY"
            | "ORDER"
            | "HAVING"
            | "UNION"
            | "INSERT"
            | "INTO"
            | "UPDATE"
            | "DELETE"
            | "CREATE"
            | "TABLE"
            | "WITH"
            | "AS"
            | "CASE"
            | "WHEN"
            | "THEN"
            | "ELSE"
            | "END"
            | "AND"
            | "OR"
            | "NOT"
            | "NULL"
            | "IS"
            | "IN"
            | "EXISTS"
            | "DISTINCT"
            | "LIMIT"
            | "OFFSET"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CapitalisationKeywords::default();
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
    fn flags_mixed_keyword_case() {
        let issues = run("SELECT a from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_001);
    }

    #[test]
    fn does_not_flag_consistent_keyword_case() {
        assert!(run("SELECT a FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_keyword_words_in_strings_or_comments() {
        let sql = "SELECT 'select from where' AS txt -- select from where\nFROM t";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn upper_policy_flags_lowercase_keywords() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.keywords".to_string(),
                serde_json::json!({"capitalisation_policy": "upper"}),
            )]),
        };
        let rule = CapitalisationKeywords::from_config(&config);
        let sql = "select a from t";
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
    fn ignore_words_excludes_keywords_from_check() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CP_001".to_string(),
                serde_json::json!({"ignore_words": ["FROM"]}),
            )]),
        };
        let rule = CapitalisationKeywords::from_config(&config);
        let sql = "SELECT a from t";
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
