//! LINT_CP_001: Keyword capitalisation.
//!
//! SQLFluff CP01 parity (current scope): detect mixed-case keyword usage.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use regex::Regex;
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

use super::capitalisation_policy_helpers::{
    ignored_words_from_config, ignored_words_regex_from_config, token_is_ignored,
    tokens_violate_policy, CapitalisationPolicy,
};

pub struct CapitalisationKeywords {
    policy: CapitalisationPolicy,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
}

impl CapitalisationKeywords {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            policy: CapitalisationPolicy::from_rule_config(
                config,
                issue_codes::LINT_CP_001,
                "capitalisation_policy",
            ),
            ignore_words: ignored_words_from_config(config, issue_codes::LINT_CP_001),
            ignore_words_regex: ignored_words_regex_from_config(config, issue_codes::LINT_CP_001),
        }
    }
}

impl Default for CapitalisationKeywords {
    fn default() -> Self {
        Self {
            policy: CapitalisationPolicy::Consistent,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
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
            &keyword_tokens_for_context(ctx, &self.ignore_words, self.ignore_words_regex.as_ref()),
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

fn keyword_tokens_for_context(
    ctx: &LintContext,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
) -> Vec<String> {
    let from_document_tokens = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut out = Vec::new();
        for token in tokens {
            let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                continue;
            };
            if start < ctx.statement_range.start || end > ctx.statement_range.end {
                continue;
            }

            if let Token::Word(word) = &token.token {
                // Document token spans are tied to rendered SQL. If the source
                // slice does not match the token text, fall back to
                // statement-local tokenization.
                if !source_word_matches(ctx.sql, start, end, word.value.as_str()) {
                    return None;
                }
                if is_tracked_keyword(word.value.as_str())
                    && !is_excluded_keyword(word.value.as_str())
                    && !token_is_ignored(word.value.as_str(), ignore_words, ignore_words_regex)
                {
                    out.push(word.value.clone());
                }
            }
        }
        Some(out)
    });

    if let Some(tokens) = from_document_tokens {
        return tokens;
    }

    keyword_tokens(
        ctx.statement_sql(),
        ignore_words,
        ignore_words_regex,
        ctx.dialect(),
    )
}

fn keyword_tokens(
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

    tokens
        .into_iter()
        .filter_map(|token| match token {
            Token::Word(word)
                if is_tracked_keyword(word.value.as_str())
                    && !is_excluded_keyword(word.value.as_str())
                    && !token_is_ignored(word.value.as_str(), ignore_words, ignore_words_regex) =>
            {
                Some(word.value)
            }
            _ => None,
        })
        .collect()
}

fn token_with_span_offsets(sql: &str, token: &TokenWithSpan) -> Option<(usize, usize)> {
    let start = line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )?;
    let end = line_col_to_offset(
        sql,
        token.span.end.line as usize,
        token.span.end.column as usize,
    )?;
    Some((start, end))
}

fn line_col_to_offset(sql: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }

    let mut current_line = 1usize;
    let mut current_col = 1usize;

    for (offset, ch) in sql.char_indices() {
        if current_line == line && current_col == column {
            return Some(offset);
        }

        if ch == '\n' {
            current_line += 1;
            current_col = 1;
        } else {
            current_col += 1;
        }
    }

    if current_line == line && current_col == column {
        return Some(sql.len());
    }

    None
}

fn source_word_matches(sql: &str, start: usize, end: usize, value: &str) -> bool {
    let Some(raw) = sql.get(start..end) else {
        return false;
    };
    let normalized = raw.trim_matches(|ch| matches!(ch, '"' | '`' | '[' | ']'));
    normalized.eq_ignore_ascii_case(value)
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
            | "ALTER"
            | "TABLE"
            | "TYPE"
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
            | "IS"
            | "IN"
            | "EXISTS"
            | "DISTINCT"
            | "LIMIT"
            | "OFFSET"
            | "INTERVAL"
            | "YEAR"
            | "MONTH"
            | "DAY"
            | "HOUR"
            | "MINUTE"
            | "SECOND"
            | "WEEK"
            | "MONDAY"
            | "TUESDAY"
            | "WEDNESDAY"
            | "THURSDAY"
            | "FRIDAY"
            | "SATURDAY"
            | "SUNDAY"
            | "CUBE"
            | "CAST"
            | "COALESCE"
            | "SAFE_CAST"
            | "TRY_CAST"
    )
}

fn is_excluded_keyword(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "NULL"
            | "TRUE"
            | "FALSE"
            | "INT"
            | "INTEGER"
            | "BIGINT"
            | "SMALLINT"
            | "TINYINT"
            | "VARCHAR"
            | "CHAR"
            | "TEXT"
            | "BOOLEAN"
            | "BOOL"
            | "STRING"
            | "INT64"
            | "FLOAT64"
            | "BYTES"
            | "NUMERIC"
            | "DECIMAL"
            | "FLOAT"
            | "DOUBLE"
            | "DATE"
            | "TIME"
            | "TIMESTAMP"
            | "STRUCT"
            | "ARRAY"
            | "MAP"
            | "ENUM"
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

    #[test]
    fn ignore_words_regex_excludes_keywords_from_check() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.keywords".to_string(),
                serde_json::json!({"ignore_words_regex": "^from$"}),
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
