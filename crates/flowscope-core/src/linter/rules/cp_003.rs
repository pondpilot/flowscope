//! LINT_CP_003: Function capitalisation.
//!
//! SQLFluff CP03 parity (current scope): detect inconsistent function name
//! capitalisation.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit::visit_expressions;
use crate::parser::parse_sql_with_dialect;
use crate::types::{issue_codes, Dialect, Issue};
use regex::Regex;
use sqlparser::ast::{Expr, Statement};

use super::capitalisation_policy_helpers::{
    ignored_words_from_config, ignored_words_regex_from_config, token_is_ignored,
    tokens_violate_policy, CapitalisationPolicy,
};

pub struct CapitalisationFunctions {
    policy: CapitalisationPolicy,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
}

impl CapitalisationFunctions {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            policy: CapitalisationPolicy::from_rule_config(
                config,
                issue_codes::LINT_CP_003,
                "extended_capitalisation_policy",
            ),
            ignore_words: ignored_words_from_config(config, issue_codes::LINT_CP_003),
            ignore_words_regex: ignored_words_regex_from_config(config, issue_codes::LINT_CP_003),
        }
    }
}

impl Default for CapitalisationFunctions {
    fn default() -> Self {
        Self {
            policy: CapitalisationPolicy::Consistent,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
        }
    }
}

impl LintRule for CapitalisationFunctions {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_003
    }

    fn name(&self) -> &'static str {
        "Function capitalisation"
    }

    fn description(&self) -> &'static str {
        "Functions should use a consistent case style."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let functions = function_tokens(
            statement,
            ctx.statement_sql(),
            &self.ignore_words,
            self.ignore_words_regex.as_ref(),
            ctx.dialect(),
        );
        if functions.is_empty() {
            return Vec::new();
        }

        if tokens_violate_policy(&functions, self.policy) {
            vec![Issue::info(
                issue_codes::LINT_CP_003,
                "Function names use inconsistent capitalisation.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn function_tokens(
    statement: &Statement,
    sql: &str,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
    dialect: Dialect,
) -> Vec<String> {
    let mut out = ast_function_tokens(statement, ignore_words, ignore_words_regex);
    if out.is_empty() {
        if let Ok(statements) = parse_sql_with_dialect(sql, dialect) {
            for parsed_statement in &statements {
                out.extend(ast_function_tokens(
                    parsed_statement,
                    ignore_words,
                    ignore_words_regex,
                ));
            }
        }
    }
    out
}

fn ast_function_tokens(
    statement: &Statement,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
) -> Vec<String> {
    let mut out = Vec::new();
    visit_expressions(statement, &mut |expr| {
        if let Expr::Function(function) = expr {
            if let Some(part) = function.name.0.last() {
                let name = part
                    .as_ident()
                    .map(|ident| ident.value.clone())
                    .unwrap_or_else(|| part.to_string());
                if token_is_ignored(&name, ignore_words, ignore_words_regex) {
                    return;
                }
                out.push(name);
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CapitalisationFunctions::default();
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
    fn flags_mixed_function_case() {
        let issues = run("SELECT COUNT(*), count(x) FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_003);
    }

    #[test]
    fn does_not_flag_consistent_function_case() {
        assert!(run("SELECT lower(x), upper(y) FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_function_like_text_in_strings_or_comments() {
        let sql = "SELECT 'COUNT(x) count(y)' AS txt -- COUNT(x)\nFROM t";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn lower_policy_flags_uppercase_function_name() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CP_003".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "lower"}),
            )]),
        };
        let rule = CapitalisationFunctions::from_config(&config);
        let sql = "SELECT COUNT(x) FROM t";
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
    fn ignore_words_regex_excludes_functions_from_check() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CP_003".to_string(),
                serde_json::json!({"ignore_words_regex": "^count$"}),
            )]),
        };
        let rule = CapitalisationFunctions::from_config(&config);
        let sql = "SELECT COUNT(*), count(x) FROM t";
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
    fn bare_function_keywords_are_tracked() {
        let issues = run("SELECT CURRENT_TIMESTAMP, current_timestamp FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_003);
    }
}
