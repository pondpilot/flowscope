//! LINT_CP_001: Keyword capitalisation.
//!
//! SQLFluff CP01 parity (current scope): detect mixed-case keyword usage.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
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

impl BuiltinLintRule for CapitalisationKeywords {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_001
    }

    fn name(&self) -> &'static str {
        "Keyword capitalisation"
    }

    fn description(&self) -> &'static str {
        "Inconsistent capitalisation of keywords."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let keywords =
            keyword_tokens_for_context(ctx, &self.ignore_words, self.ignore_words_regex.as_ref());
        let keyword_values = keywords
            .iter()
            .map(|candidate| candidate.value.clone())
            .collect::<Vec<_>>();
        if !tokens_violate_policy(&keyword_values, self.policy) {
            Vec::new()
        } else {
            let mut issue = Issue::info(
                issue_codes::LINT_CP_001,
                "SQL keywords use inconsistent capitalisation.",
            )
            .with_statement(ctx.statement_index);

            let autofix_edits = keyword_autofix_edits(ctx, &keywords, self.policy);
            if !autofix_edits.is_empty() {
                issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, autofix_edits);
            }

            vec![issue]
        }
    }
}

#[derive(Clone)]
struct KeywordCandidate {
    value: String,
    start: usize,
    end: usize,
}

fn keyword_tokens_for_context(
    ctx: &RuleContext,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
) -> Vec<KeywordCandidate> {
    let from_document_tokens = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut out = Vec::new();
        let mut prev_is_period = false;
        for token in tokens {
            let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                continue;
            };
            if start < ctx.statement_range.start || end > ctx.statement_range.end {
                continue;
            }

            match &token.token {
                Token::Period => {
                    prev_is_period = true;
                    continue;
                }
                Token::Whitespace(_) => continue,
                _ => {}
            }

            if let Token::Word(word) = &token.token {
                let after_period = prev_is_period;
                prev_is_period = false;
                if after_period {
                    continue;
                }
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
                    let Some(local_start) = start.checked_sub(ctx.statement_range.start) else {
                        continue;
                    };
                    let Some(local_end) = end.checked_sub(ctx.statement_range.start) else {
                        continue;
                    };
                    out.push(KeywordCandidate {
                        value: word.value.clone(),
                        start: local_start,
                        end: local_end,
                    });
                }
            } else {
                prev_is_period = false;
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
) -> Vec<KeywordCandidate> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return Vec::new();
    };

    // Track the previous non-whitespace token to skip keywords used as
    // column references in compound identifiers (e.g. `wu.type`).
    let mut prev_is_period = false;
    let mut out = Vec::new();
    for token in &tokens {
        match &token.token {
            Token::Period => {
                prev_is_period = true;
                continue;
            }
            Token::Whitespace(_) => continue,
            Token::Word(word) => {
                let after_period = prev_is_period;
                prev_is_period = false;
                if after_period {
                    continue;
                }
                if is_tracked_keyword(word.value.as_str())
                    && !is_excluded_keyword(word.value.as_str())
                    && !token_is_ignored(word.value.as_str(), ignore_words, ignore_words_regex)
                {
                    if let Some((start, end)) = token_with_span_offsets(sql, token) {
                        out.push(KeywordCandidate {
                            value: word.value.clone(),
                            start,
                            end,
                        });
                    }
                }
            }
            _ => {
                prev_is_period = false;
            }
        }
    }
    out
}

fn keyword_autofix_edits(
    ctx: &RuleContext,
    keywords: &[KeywordCandidate],
    policy: CapitalisationPolicy,
) -> Vec<IssuePatchEdit> {
    // For consistent mode, resolve to the first-seen concrete style.
    let resolved_policy = if policy == CapitalisationPolicy::Consistent {
        resolve_consistent_policy(keywords)
    } else {
        policy
    };

    let mut edits = Vec::new();

    for candidate in keywords {
        let Some(replacement) = keyword_case_replacement(candidate.value.as_str(), resolved_policy)
        else {
            continue;
        };
        if replacement == candidate.value {
            continue;
        }

        edits.push(IssuePatchEdit::new(
            ctx.span_from_statement_offset(candidate.start, candidate.end),
            replacement,
        ));
    }

    edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
    edits.dedup_by(|left, right| {
        left.span.start == right.span.start
            && left.span.end == right.span.end
            && left.replacement == right.replacement
    });
    edits
}

/// Determine the concrete capitalisation style using SQLFluff's cumulative
/// refutation algorithm. Refuted cases accumulate across keywords:
///
/// 1. For each keyword, determine which styles it rules out.
/// 2. If any styles remain possible, save the first one and continue.
/// 3. If ALL styles are refuted, use the last remembered style (default: upper).
fn resolve_consistent_policy(keywords: &[KeywordCandidate]) -> CapitalisationPolicy {
    const UPPER: u8 = 0b001;
    const LOWER: u8 = 0b010;
    const CAPITALISE: u8 = 0b100;

    let mut refuted: u8 = 0;
    let mut latest_possible = CapitalisationPolicy::Upper; // default

    for kw in keywords {
        let v = kw.value.as_str();

        // Determine which styles this keyword refutes.
        let first_is_lower = v
            .chars()
            .find(|c| c.is_ascii_alphabetic())
            .is_some_and(|c| c.is_ascii_lowercase());

        if first_is_lower {
            refuted |= UPPER | CAPITALISE;
            if v != v.to_ascii_lowercase() {
                refuted |= LOWER;
            }
        } else {
            refuted |= LOWER;
            if v != v.to_ascii_uppercase() {
                refuted |= UPPER;
            }
            if v != capitalise_ascii_token(v) {
                refuted |= CAPITALISE;
            }
        }

        let possible = (UPPER | LOWER | CAPITALISE) & !refuted;
        if possible == 0 {
            return latest_possible;
        }

        // Pick the first non-refuted style in SQLFluff's ordering.
        if possible & UPPER != 0 {
            latest_possible = CapitalisationPolicy::Upper;
        } else if possible & LOWER != 0 {
            latest_possible = CapitalisationPolicy::Lower;
        } else {
            latest_possible = CapitalisationPolicy::Capitalise;
        }
    }

    latest_possible
}

fn keyword_case_replacement(value: &str, policy: CapitalisationPolicy) -> Option<String> {
    match policy {
        CapitalisationPolicy::Consistent => {
            // Consistent mode is resolved to a concrete style before calling
            // this function (see keyword_autofix_edits). Should not reach here.
            Some(value.to_ascii_lowercase())
        }
        CapitalisationPolicy::Lower => Some(value.to_ascii_lowercase()),
        CapitalisationPolicy::Upper => Some(value.to_ascii_uppercase()),
        CapitalisationPolicy::Capitalise => Some(capitalise_ascii_token(value)),
        // These policies are currently report-only in CP01 autofix scope.
        CapitalisationPolicy::Pascal
        | CapitalisationPolicy::Camel
        | CapitalisationPolicy::Snake => None,
    }
}

fn capitalise_ascii_token(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut seen_alpha = false;

    for ch in value.chars() {
        if !ch.is_ascii_alphabetic() {
            out.push(ch);
            continue;
        }

        if !seen_alpha {
            out.push(ch.to_ascii_uppercase());
            seen_alpha = true;
        } else {
            out.push(ch.to_ascii_lowercase());
        }
    }

    out
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
            | "ASC"
            | "DESC"
            | "CROSS"
            | "NATURAL"
            | "OVER"
            | "PARTITION"
            | "BETWEEN"
            | "LIKE"
            | "SET"
            | "QUALIFY"
            | "LATERAL"
            | "ROLLUP"
            | "GROUPING"
            | "SETS"
            | "ALL"
            | "ANY"
            | "SOME"
            | "EXCEPT"
            | "INTERSECT"
            | "VALUES"
            | "DROP"
            | "IF"
            | "VIEW"
            | "USING"
            | "FETCH"
            | "NEXT"
            | "ROWS"
            | "ONLY"
            | "FIRST"
            | "LAST"
            | "RECURSIVE"
            | "WINDOW"
            | "RANGE"
            | "UNBOUNDED"
            | "PRECEDING"
            | "FOLLOWING"
            | "CURRENT"
            | "ROW"
            | "NULLS"
            | "TOP"
            | "PERCENT"
            | "REPLACE"
            | "GRANT"
            | "REVOKE"
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
            | "INTERVAL"
            | "STRUCT"
            | "ARRAY"
            | "MAP"
            | "ENUM"
            // Function-like keywords tracked by CP03, not CP01 in SQLFluff.
            | "COALESCE"
            | "CAST"
            | "SAFE_CAST"
            | "TRY_CAST"
            | "ANY"
            | "SOME"
            | "REPLACE"
            // TYPE is very commonly used as a column name; SQLFluff does not
            // track it under CP01.
            | "TYPE"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CapitalisationKeywords::default();
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn apply_issue_autofix(sql: &str, issue: &Issue) -> Option<String> {
        let autofix = issue.autofix.as_ref()?;
        let mut out = sql.to_string();
        let mut edits = autofix.edits.clone();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        for edit in edits.into_iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    #[test]
    fn flags_mixed_keyword_case() {
        let issues = run("SELECT a from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_001);
    }

    #[test]
    fn emits_safe_autofix_for_mixed_keyword_case() {
        let sql = "SELECT a from t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT a FROM t");
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
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn upper_policy_emits_uppercase_autofix() {
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
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT a FROM t");
    }

    #[test]
    fn camel_policy_violation_remains_report_only() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.keywords".to_string(),
                serde_json::json!({"capitalisation_policy": "camel"}),
            )]),
        };
        let rule = CapitalisationKeywords::from_config(&config);
        let sql = "SELECT a FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0].autofix.is_none(),
            "camel/pascal/snake are report-only in current CP001 autofix scope"
        );
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
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
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
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }
}
