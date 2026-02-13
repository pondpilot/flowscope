//! LINT_CP_005: Type capitalisation.
//!
//! SQLFluff CP05 parity (current scope): detect mixed-case type names.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::tokenizer::{Token, Tokenizer};

use super::capitalisation_policy_helpers::{
    ignored_words_from_config, ignored_words_regex_from_config, token_is_ignored,
    tokens_violate_policy, CapitalisationPolicy,
};

pub struct CapitalisationTypes {
    policy: CapitalisationPolicy,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
}

impl CapitalisationTypes {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            policy: CapitalisationPolicy::from_rule_config(
                config,
                issue_codes::LINT_CP_005,
                "extended_capitalisation_policy",
            ),
            ignore_words: ignored_words_from_config(config, issue_codes::LINT_CP_005),
            ignore_words_regex: ignored_words_regex_from_config(config, issue_codes::LINT_CP_005),
        }
    }
}

impl Default for CapitalisationTypes {
    fn default() -> Self {
        Self {
            policy: CapitalisationPolicy::Consistent,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
        }
    }
}

impl LintRule for CapitalisationTypes {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_005
    }

    fn name(&self) -> &'static str {
        "Type capitalisation"
    }

    fn description(&self) -> &'static str {
        "Type names should use a consistent case style."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if tokens_violate_policy(
            &type_tokens(
                ctx.statement_sql(),
                &self.ignore_words,
                self.ignore_words_regex.as_ref(),
            ),
            self.policy,
        ) {
            vec![Issue::info(
                issue_codes::LINT_CP_005,
                "Type names use inconsistent capitalisation.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn type_tokens(
    sql: &str,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
) -> Vec<String> {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return Vec::new();
    };

    tokens
        .into_iter()
        .filter_map(|token| match token {
            Token::Word(word)
                if word.quote_style.is_none()
                    && is_tracked_type_name(word.value.as_str())
                    && !token_is_ignored(word.value.as_str(), ignore_words, ignore_words_regex) =>
            {
                Some(word.value)
            }
            _ => None,
        })
        .collect()
}

fn is_tracked_type_name(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "INT"
            | "INTEGER"
            | "BIGINT"
            | "SMALLINT"
            | "TINYINT"
            | "VARCHAR"
            | "CHAR"
            | "TEXT"
            | "BOOLEAN"
            | "BOOL"
            | "DATE"
            | "TIMESTAMP"
            | "NUMERIC"
            | "DECIMAL"
            | "FLOAT"
            | "DOUBLE"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CapitalisationTypes::default();
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
    fn flags_mixed_type_case() {
        let issues = run("CREATE TABLE t (a INT, b varchar(10))");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_005);
    }

    #[test]
    fn does_not_flag_consistent_type_case() {
        assert!(run("CREATE TABLE t (a int, b varchar(10))").is_empty());
    }

    #[test]
    fn does_not_flag_type_words_in_strings_or_comments() {
        let sql = "SELECT 'INT varchar BOOLEAN' AS txt -- INT varchar\nFROM t";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn upper_policy_flags_lowercase_type_name() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CP_005".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "upper"}),
            )]),
        };
        let rule = CapitalisationTypes::from_config(&config);
        let sql = "CREATE TABLE t (a int)";
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
    fn ignore_words_regex_excludes_types_from_check() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CP_005".to_string(),
                serde_json::json!({"ignore_words_regex": "^varchar$"}),
            )]),
        };
        let rule = CapitalisationTypes::from_config(&config);
        let sql = "CREATE TABLE t (a INT, b varchar(10))";
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
