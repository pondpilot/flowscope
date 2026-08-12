//! LINT_CP_003: Function capitalisation.
//!
//! SQLFluff CP03 parity (current scope): detect inconsistent function name
//! capitalisation.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use regex::Regex;
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

use super::capitalisation_policy_helpers::{
    apply_camel_transform, apply_pascal_transform, apply_snake_transform,
    ignored_words_from_config, ignored_words_regex_from_config, token_is_ignored,
    tokens_violate_policy, CapitalisationPolicy,
};

pub struct CapitalisationFunctions {
    policy: CapitalisationPolicy,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
    ignore_templated_areas: bool,
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
            ignore_templated_areas: config
                .core_option_bool("ignore_templated_areas")
                .unwrap_or(false),
        }
    }
}

impl Default for CapitalisationFunctions {
    fn default() -> Self {
        Self {
            policy: CapitalisationPolicy::Consistent,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
            ignore_templated_areas: false,
        }
    }
}

impl BuiltinLintRule for CapitalisationFunctions {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_003
    }

    fn name(&self) -> &'static str {
        "Function capitalisation"
    }

    fn description(&self) -> &'static str {
        "Inconsistent capitalisation of function names."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let functions = function_candidates_for_context(
            ctx,
            &self.ignore_words,
            self.ignore_words_regex.as_ref(),
        );
        if functions.is_empty() {
            return Vec::new();
        }

        let mut function_tokens = functions
            .iter()
            .map(|candidate| candidate.value.clone())
            .collect::<Vec<_>>();
        if ctx.is_templated() && !self.ignore_templated_areas {
            if let Some(rendered_tokens) = rendered_function_values_for_context(
                ctx,
                &self.ignore_words,
                self.ignore_words_regex.as_ref(),
            ) {
                if !rendered_tokens.is_empty() {
                    function_tokens = rendered_tokens;
                }
            }
        }
        if !tokens_violate_policy(&function_tokens, self.policy) {
            return Vec::new();
        }

        let resolved_policy = if self.policy == CapitalisationPolicy::Consistent {
            resolve_consistent_policy_from_values(&function_tokens)
        } else {
            self.policy
        };

        let autofix_edits = function_autofix_edits(ctx, &functions, resolved_policy);

        // Emit one issue per violating function name at its specific position
        // (SQLFluff reports per-identifier, not per-statement).
        if autofix_edits.is_empty() {
            return vec![Issue::info(
                issue_codes::LINT_CP_003,
                "Function names use inconsistent capitalisation.",
            )
            .with_statement(ctx.statement_index)];
        }

        autofix_edits
            .into_iter()
            .map(|edit| {
                let span = Span::new(edit.span.start, edit.span.end);
                Issue::info(
                    issue_codes::LINT_CP_003,
                    "Function names use inconsistent capitalisation.",
                )
                .with_statement(ctx.statement_index)
                .with_span(span)
                .with_autofix_edits(IssueAutofixApplicability::Safe, vec![edit])
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
struct FunctionCandidate {
    value: String,
    start: usize,
    end: usize,
}

fn function_candidates_for_context(
    ctx: &RuleContext,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
) -> Vec<FunctionCandidate> {
    let sql = ctx.statement_sql();
    let Some(tokens) = tokenized(sql, ctx.dialect()) else {
        return Vec::new();
    };

    let mut candidates = function_candidates(sql, &tokens, ignore_words, ignore_words_regex);
    candidates.sort_by_key(|candidate| (candidate.start, candidate.end));
    candidates
}

fn function_candidates(
    sql: &str,
    tokens: &[TokenWithSpan],
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
) -> Vec<FunctionCandidate> {
    let mut out = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        let Token::Word(word) = &token.token else {
            continue;
        };

        if token_is_ignored(word.value.as_str(), ignore_words, ignore_words_regex) {
            continue;
        }

        // Skip qualified function names (e.g. project1.foo) — these are
        // user-defined and case-sensitive. Check if a period precedes this word.
        if index > 0 && matches!(tokens[index - 1].token, Token::Period) {
            continue;
        }

        // Skip data type names — they have their own rule (CP05). Without AST
        // context we cannot distinguish `VARCHAR(10)` the type from a function
        // call, so exclude known type keywords by name.
        if is_data_type_keyword(word.value.as_str()) {
            continue;
        }

        // Skip SQL keywords that can precede `(` but are not function calls
        // (e.g. `IN (...)`, `AS (...)`, `EXISTS (...)`).
        if is_non_function_sql_keyword(word.value.as_str()) {
            continue;
        }

        let next_index = next_non_trivia_index(tokens, index + 1);
        let is_regular_function_call = next_index
            .map(|idx| matches!(tokens[idx].token, Token::LParen))
            .unwrap_or(false);
        let is_bare_function = is_bare_function_keyword(word.value.as_str());
        if !is_regular_function_call && !is_bare_function {
            continue;
        }

        let Some((start, end)) = token_offsets(sql, token) else {
            continue;
        };

        out.push(FunctionCandidate {
            value: word.value.clone(),
            start,
            end,
        });
    }

    out
}

fn function_autofix_edits(
    ctx: &RuleContext,
    functions: &[FunctionCandidate],
    resolved_policy: CapitalisationPolicy,
) -> Vec<IssuePatchEdit> {
    let mut ordered_functions = functions.to_vec();
    ordered_functions.sort_by_key(|candidate| (candidate.start, candidate.end));

    let mut edits = Vec::new();

    for candidate in &ordered_functions {
        let Some(replacement) =
            function_case_replacement(candidate.value.as_str(), resolved_policy)
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

fn function_case_replacement(value: &str, policy: CapitalisationPolicy) -> Option<String> {
    match policy {
        CapitalisationPolicy::Consistent => {
            // Consistent mode is resolved before calling this function.
            Some(value.to_ascii_lowercase())
        }
        CapitalisationPolicy::Lower => Some(value.to_ascii_lowercase()),
        CapitalisationPolicy::Upper => Some(value.to_ascii_uppercase()),
        CapitalisationPolicy::Capitalise => Some(capitalise_ascii_token(value)),
        CapitalisationPolicy::Pascal => Some(apply_pascal_transform(value)),
        CapitalisationPolicy::Camel => Some(apply_camel_transform(value)),
        CapitalisationPolicy::Snake => Some(apply_snake_transform(value)),
    }
}

fn resolve_consistent_policy_from_values(values: &[String]) -> CapitalisationPolicy {
    const UPPER: u8 = 0b001;
    const LOWER: u8 = 0b010;
    const CAPITALISE: u8 = 0b100;

    let mut refuted: u8 = 0;
    let mut latest_possible = CapitalisationPolicy::Upper; // default

    for v in values {
        let v = v.as_str();

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

fn rendered_function_values_for_context(
    ctx: &RuleContext,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
) -> Option<Vec<String>> {
    ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }
        Some(function_token_values(
            tokens,
            ignore_words,
            ignore_words_regex,
        ))
    })
}

fn function_token_values(
    tokens: &[TokenWithSpan],
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
) -> Vec<String> {
    let mut out = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        let Token::Word(word) = &token.token else {
            continue;
        };

        if token_is_ignored(word.value.as_str(), ignore_words, ignore_words_regex) {
            continue;
        }

        if index > 0 && matches!(tokens[index - 1].token, Token::Period) {
            continue;
        }

        if is_data_type_keyword(word.value.as_str()) {
            continue;
        }

        let next_index = next_non_trivia_index(tokens, index + 1);
        let is_regular_function_call = next_index
            .map(|idx| matches!(tokens[idx].token, Token::LParen))
            .unwrap_or(false);
        let is_bare_function = is_bare_function_keyword(word.value.as_str());
        if !is_regular_function_call && !is_bare_function {
            continue;
        }

        out.push((
            token.span.start.line,
            token.span.start.column,
            word.value.clone(),
        ));
    }

    out.sort_by_key(|(line, column, _)| (*line, *column));
    out.into_iter().map(|(_, _, value)| value).collect()
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

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn next_non_trivia_index(tokens: &[TokenWithSpan], mut index: usize) -> Option<usize> {
    while index < tokens.len() {
        if !is_trivia_token(&tokens[index].token) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(
            Whitespace::Space
                | Whitespace::Newline
                | Whitespace::Tab
                | Whitespace::SingleLineComment { .. }
                | Whitespace::MultiLineComment(_)
        )
    )
}

fn token_offsets(sql: &str, token: &TokenWithSpan) -> Option<(usize, usize)> {
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

fn is_bare_function_keyword(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "CURRENT_TIMESTAMP" | "CURRENT_DATE" | "CURRENT_TIME" | "LOCALTIME" | "LOCALTIMESTAMP"
    )
}

/// SQL clause/operator keywords that can precede `(` but are not function calls.
/// Without AST context, these would be false-positive function candidates.
fn is_non_function_sql_keyword(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        // Operators and predicates
        "IN" | "NOT" | "EXISTS" | "BETWEEN" | "LIKE" | "ILIKE"
            // Logical operators
            | "AND" | "OR" | "IS"
            // Clause keywords that precede parenthesized content
            | "AS" | "ON" | "USING" | "OVER" | "FILTER" | "WITHIN"
            // DML/DDL clause heads
            | "VALUES" | "SET" | "INTO" | "FROM" | "WHERE" | "HAVING"
            | "SELECT" | "TABLE" | "JOIN"
            | "CONFLICT"
            // Set operations and CTE
            | "UNION" | "INTERSECT" | "EXCEPT" | "WITH" | "RECURSIVE"
            // CASE expression parts
            | "WHEN" | "THEN" | "ELSE" | "END" | "CASE"
            // Ordering/grouping
            | "GROUP" | "ORDER" | "PARTITION" | "LIMIT" | "OFFSET" | "FETCH"
            // Literals and modifiers
            | "NULL" | "TRUE" | "FALSE" | "DISTINCT" | "LATERAL"
    )
}

/// Data type keywords that can look like function calls (e.g. `VARCHAR(10)`)
/// but belong to CP05's scope, not CP03's.
fn is_data_type_keyword(value: &str) -> bool {
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
            | "STRING"
            | "INT64"
            | "FLOAT64"
            | "BYTES"
            | "TIME"
            | "TIMESTAMP"
            | "INTERVAL"
            | "NUMERIC"
            | "DECIMAL"
            | "FLOAT"
            | "DOUBLE"
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
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CapitalisationFunctions::default();
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

    fn apply_all_autofixes(sql: &str, issues: &[Issue]) -> String {
        let mut edits: Vec<_> = issues
            .iter()
            .filter_map(|i| i.autofix.as_ref())
            .flat_map(|a| a.edits.clone())
            .collect();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        let mut out = sql.to_string();
        for edit in edits.into_iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        out
    }

    #[test]
    fn flags_mixed_function_case() {
        let issues = run("SELECT COUNT(*), count(x) FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_003);
    }

    #[test]
    fn emits_safe_autofix_for_mixed_function_case() {
        let sql = "SELECT COUNT(*), count(x) FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT COUNT(*), COUNT(x) FROM t");
    }

    #[test]
    fn does_not_flag_consistent_function_case() {
        assert!(run("SELECT lower(x), upper(y) FROM t").is_empty());
    }

    #[test]
    fn on_conflict_clause_keyword_is_not_treated_as_function_name() {
        let sql = "INSERT INTO t (id) VALUES (1) ON CONFLICT (id) DO UPDATE SET updated_at = now(), touched_at = NOW()";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_003);
    }

    #[test]
    fn date_function_calls_are_tracked() {
        let issues = run("SELECT date(ts), DATE(ts) FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_003);
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
                "LINT_CP_003".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "upper"}),
            )]),
        };
        let rule = CapitalisationFunctions::from_config(&config);
        let sql = "SELECT count(x) FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT COUNT(x) FROM t");
    }

    #[test]
    fn camel_policy_emits_autofix() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CP_003".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "camel"}),
            )]),
        };
        let rule = CapitalisationFunctions::from_config(&config);
        let sql = "SELECT COUNT(x), SUM(y) FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        // Both COUNT and SUM violate camel → 2 violations.
        assert_eq!(issues.len(), 2);
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT cOUNT(x), sUM(y) FROM t");
    }

    #[test]
    fn pascal_policy_emits_autofix() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CP_003".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "pascal"}),
            )]),
        };
        let rule = CapitalisationFunctions::from_config(&config);
        let sql = "SELECT current_timestamp, min(a) FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        // Both current_timestamp and min violate pascal → 2 violations.
        assert_eq!(issues.len(), 2);
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT Current_Timestamp, Min(a) FROM t");
    }

    #[test]
    fn snake_policy_emits_autofix() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CP_003".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "snake"}),
            )]),
        };
        let rule = CapitalisationFunctions::from_config(&config);
        let sql = "SELECT Current_Timestamp, Min(a) FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        // Both Current_Timestamp and Min violate snake → 2 violations.
        assert_eq!(issues.len(), 2);
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT current_timestamp, min(a) FROM t");
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
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn bare_function_keywords_are_tracked() {
        let issues = run("SELECT CURRENT_TIMESTAMP, current_timestamp FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_003);
    }

    #[test]
    fn quantified_any_keyword_tracks_capitalisation_for_parity() {
        let sql = "SELECT count(*), col = ANY(arr) FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT count(*), col = any(arr) FROM t");
    }

    #[test]
    fn consistent_policy_autofix_uses_source_order_even_when_candidates_are_unsorted() {
        let sql = "SELECT greatest(x), GREATEST(y) FROM t";
        let ctx = RuleContext::new(sql, 0..sql.len(), 0);

        let upper_start = sql.find("GREATEST").expect("uppercase function position");
        let lower_start = sql.find("greatest").expect("lowercase function position");
        let unsorted = vec![
            FunctionCandidate {
                value: "GREATEST".to_string(),
                start: upper_start,
                end: upper_start + "GREATEST".len(),
            },
            FunctionCandidate {
                value: "greatest".to_string(),
                start: lower_start,
                end: lower_start + "greatest".len(),
            },
        ];

        let edits = function_autofix_edits(&ctx, &unsorted, CapitalisationPolicy::Consistent);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].span.start, upper_start);
        assert_eq!(edits[0].span.end, upper_start + "GREATEST".len());
        assert_eq!(edits[0].replacement, "greatest");
    }

    #[test]
    fn templated_policy_tokens_drive_source_mapped_autofix_when_not_ignored() {
        let source_sql = "SELECT\n    {{ \"greatest(a, b)\" }},\n    GREATEST(i, j)\n";
        let rendered_sql = "SELECT\n    greatest(a, b),\n    GREATEST(i, j)\n";
        let rendered_tokens = tokenized(rendered_sql, Dialect::Ansi).expect("rendered tokens");
        let statements = parse_sql("SELECT 1").expect("synthetic parse");
        let rule = CapitalisationFunctions::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "core".to_string(),
                serde_json::json!({"ignore_templated_areas": false}),
            )]),
        });

        let issues = rule.check_with_context(
            &statements[0],
            &RuleContext::new(source_sql, 0..source_sql.len(), 0)
                .with_tokens(&rendered_tokens)
                .with_templated(true),
        );

        assert_eq!(issues.len(), 1);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected autofix metadata");
        assert!(
            autofix
                .edits
                .iter()
                .any(
                    |edit| &source_sql[edit.span.start..edit.span.end] == "GREATEST"
                        && edit.replacement == "greatest"
                ),
            "expected source-mapped GREATEST fix, got edits={:?}",
            autofix.edits
        );
    }
}
