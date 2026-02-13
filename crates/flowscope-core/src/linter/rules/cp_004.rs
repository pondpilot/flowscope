//! LINT_CP_004: Literal capitalisation.
//!
//! SQLFluff CP04 parity (current scope): detect mixed-case usage for
//! NULL/TRUE/FALSE literal keywords.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::tokenizer::{Token, Tokenizer};

use super::capitalisation_policy_helpers::{tokens_violate_policy, CapitalisationPolicy};

pub struct CapitalisationLiterals {
    policy: CapitalisationPolicy,
}

impl CapitalisationLiterals {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            policy: CapitalisationPolicy::from_rule_config(
                config,
                issue_codes::LINT_CP_004,
                "extended_capitalisation_policy",
            ),
        }
    }
}

impl Default for CapitalisationLiterals {
    fn default() -> Self {
        Self {
            policy: CapitalisationPolicy::Consistent,
        }
    }
}

impl LintRule for CapitalisationLiterals {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_004
    }

    fn name(&self) -> &'static str {
        "Literal capitalisation"
    }

    fn description(&self) -> &'static str {
        "NULL/TRUE/FALSE should use a consistent case style."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if tokens_violate_policy(&literal_tokens(ctx.statement_sql()), self.policy) {
            vec![Issue::info(
                issue_codes::LINT_CP_004,
                "Literal keywords (NULL/TRUE/FALSE) use inconsistent capitalisation.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn literal_tokens(sql: &str) -> Vec<String> {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return Vec::new();
    };

    tokens
        .into_iter()
        .filter_map(|token| match token {
            Token::Word(word)
                if matches!(
                    word.value.to_ascii_uppercase().as_str(),
                    "NULL" | "TRUE" | "FALSE"
                ) =>
            {
                Some(word.value)
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CapitalisationLiterals::default();
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
    fn flags_mixed_literal_case() {
        let issues = run("SELECT NULL, true FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_004);
    }

    #[test]
    fn does_not_flag_consistent_literal_case() {
        assert!(run("SELECT NULL, TRUE FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_literal_words_in_strings_or_comments() {
        let sql = "SELECT 'null true false' AS txt -- NULL true\nFROM t";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn upper_policy_flags_lowercase_literal() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.literals".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "upper"}),
            )]),
        };
        let rule = CapitalisationLiterals::from_config(&config);
        let sql = "SELECT true FROM t";
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
