//! LINT_CP_003: Function capitalisation.
//!
//! SQLFluff CP03 parity (current scope): detect inconsistent function name
//! capitalisation.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use regex::Regex;
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

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

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let functions = function_tokens(
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
    sql: &str,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
    dialect: Dialect,
) -> Vec<String> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return Vec::new();
    };

    let mut out = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        let Token::Word(word) = token else {
            continue;
        };

        if word.quote_style.is_some() {
            continue;
        }

        if is_non_function_word(word.value.as_str()) {
            continue;
        }

        if token_is_ignored(word.value.as_str(), ignore_words, ignore_words_regex) {
            continue;
        }

        let Some(next_index) = next_non_trivia_index(&tokens, index + 1) else {
            if is_bare_function_name(word.value.as_str()) {
                out.push(word.value.clone());
            }
            continue;
        };
        if !matches!(tokens[next_index], Token::LParen) {
            if is_bare_function_name(word.value.as_str()) {
                out.push(word.value.clone());
            }
            continue;
        }

        if let Some(prev_index) = prev_non_trivia_index(&tokens, index) {
            match &tokens[prev_index] {
                Token::Period => continue,
                Token::Word(prev_word)
                    if matches!(
                        prev_word.keyword,
                        Keyword::INTO
                            | Keyword::FROM
                            | Keyword::JOIN
                            | Keyword::UPDATE
                            | Keyword::TABLE
                    ) =>
                {
                    continue;
                }
                _ => {}
            }
        }

        out.push(word.value.clone());
    }

    out
}

fn is_bare_function_name(word: &str) -> bool {
    matches!(
        word.to_ascii_uppercase().as_str(),
        "CURRENT_DATE"
            | "CURRENT_TIME"
            | "CURRENT_TIMESTAMP"
            | "LOCALTIME"
            | "LOCALTIMESTAMP"
            | "CURRENT_USER"
            | "SESSION_USER"
            | "SYSTEM_USER"
            | "USER"
    )
}

fn next_non_trivia_index(tokens: &[Token], mut index: usize) -> Option<usize> {
    while index < tokens.len() {
        if !is_trivia_token(&tokens[index]) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn is_non_function_word(word: &str) -> bool {
    matches!(
        word.to_ascii_uppercase().as_str(),
        "ALL"
            | "AND"
            | "ANY"
            | "AS"
            | "BETWEEN"
            | "BY"
            | "CASE"
            | "ELSE"
            | "END"
            | "EXISTS"
            | "FROM"
            | "GROUP"
            | "HAVING"
            | "IN"
            | "INTERSECT"
            | "IS"
            | "JOIN"
            | "LIKE"
            | "ILIKE"
            | "LIMIT"
            | "NOT"
            | "OFFSET"
            | "ON"
            | "OR"
            | "ORDER"
            | "OVER"
            | "PARTITION"
            | "SELECT"
            | "THEN"
            | "UNION"
            | "WHEN"
            | "WHERE"
            | "WINDOW"
    )
}

fn prev_non_trivia_index(tokens: &[Token], mut index: usize) -> Option<usize> {
    while index > 0 {
        index -= 1;
        if !is_trivia_token(&tokens[index]) {
            return Some(index);
        }
    }
    None
}

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
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
}
