//! LINT_CP_004: Literal capitalisation.
//!
//! SQLFluff CP04 parity (current scope): detect mixed-case usage for
//! NULL/TRUE/FALSE literal keywords.

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

pub struct CapitalisationLiterals {
    policy: CapitalisationPolicy,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
}

impl CapitalisationLiterals {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            policy: CapitalisationPolicy::from_rule_config(
                config,
                issue_codes::LINT_CP_004,
                "extended_capitalisation_policy",
            ),
            ignore_words: ignored_words_from_config(config, issue_codes::LINT_CP_004),
            ignore_words_regex: ignored_words_regex_from_config(config, issue_codes::LINT_CP_004),
        }
    }
}

impl Default for CapitalisationLiterals {
    fn default() -> Self {
        Self {
            policy: CapitalisationPolicy::Consistent,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
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
        if tokens_violate_policy(
            &literal_tokens_for_context(ctx, &self.ignore_words, self.ignore_words_regex.as_ref()),
            self.policy,
        ) {
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

fn literal_tokens_for_context(
    ctx: &LintContext,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
) -> Vec<String> {
    if has_template_markers(ctx.statement_sql()) {
        return literal_tokens(
            ctx.statement_sql(),
            ignore_words,
            ignore_words_regex,
            ctx.dialect(),
        );
    }

    let from_document_tokens = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        Some(
            tokens
                .iter()
                .filter_map(|token| {
                    let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                        return None;
                    };
                    if start < ctx.statement_range.start || end > ctx.statement_range.end {
                        return None;
                    }

                    match &token.token {
                        Token::Word(word)
                            if source_word_matches(ctx.sql, start, end, word.value.as_str())
                                && matches!(
                                    word.value.to_ascii_uppercase().as_str(),
                                    "NULL" | "TRUE" | "FALSE"
                                )
                                && !token_is_ignored(
                                    word.value.as_str(),
                                    ignore_words,
                                    ignore_words_regex,
                                ) =>
                        {
                            Some(word.value.clone())
                        }
                        _ => None,
                    }
                })
                .collect::<Vec<_>>(),
        )
    });

    if let Some(tokens) = from_document_tokens {
        return tokens;
    }

    literal_tokens(
        ctx.statement_sql(),
        ignore_words,
        ignore_words_regex,
        ctx.dialect(),
    )
}

fn literal_tokens(
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
                if matches!(
                    word.value.to_ascii_uppercase().as_str(),
                    "NULL" | "TRUE" | "FALSE"
                ) && !token_is_ignored(
                    word.value.as_str(),
                    ignore_words,
                    ignore_words_regex,
                ) =>
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

fn has_template_markers(sql: &str) -> bool {
    sql.contains("{{") || sql.contains("{%") || sql.contains("{#")
}

fn source_word_matches(sql: &str, start: usize, end: usize, value: &str) -> bool {
    let Some(raw) = sql.get(start..end) else {
        return false;
    };
    let normalized = raw.trim_matches(|ch| matches!(ch, '"' | '`' | '[' | ']'));
    normalized.eq_ignore_ascii_case(value)
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

    #[test]
    fn ignore_words_regex_excludes_literals_from_check() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.literals".to_string(),
                serde_json::json!({"ignore_words_regex": "^true$"}),
            )]),
        };
        let rule = CapitalisationLiterals::from_config(&config);
        let sql = "SELECT NULL, true FROM t";
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
