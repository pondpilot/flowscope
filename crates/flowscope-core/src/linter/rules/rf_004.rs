//! LINT_RF_004: References keywords.
//!
//! SQLFluff RF04 parity (current scope): avoid keyword-looking identifiers with
//! SQLFluff-style quoted/unquoted identifier-policy controls.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use regex::{Regex, RegexBuilder};
use sqlparser::ast::Statement;
use sqlparser::keywords::ALL_KEYWORDS;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

use super::identifier_candidates_helpers::{
    collect_identifier_candidates, IdentifierCandidate, IdentifierKind, IdentifierPolicy,
};

pub struct ReferencesKeywords {
    quoted_policy: IdentifierPolicy,
    unquoted_policy: IdentifierPolicy,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
}

impl ReferencesKeywords {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            quoted_policy: IdentifierPolicy::from_config(
                config,
                issue_codes::LINT_RF_004,
                "quoted_identifiers_policy",
                "none",
            ),
            unquoted_policy: IdentifierPolicy::from_config(
                config,
                issue_codes::LINT_RF_004,
                "unquoted_identifiers_policy",
                "aliases",
            ),
            ignore_words: configured_ignore_words(config)
                .into_iter()
                .map(|word| normalize_token(&word))
                .collect(),
            ignore_words_regex: config
                .rule_option_str(issue_codes::LINT_RF_004, "ignore_words_regex")
                .filter(|pattern| !pattern.trim().is_empty())
                .and_then(|pattern| {
                    RegexBuilder::new(pattern)
                        .case_insensitive(true)
                        .build()
                        .ok()
                }),
        }
    }
}

impl Default for ReferencesKeywords {
    fn default() -> Self {
        Self {
            quoted_policy: IdentifierPolicy::None,
            unquoted_policy: IdentifierPolicy::Aliases,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
        }
    }
}

impl BuiltinLintRule for ReferencesKeywords {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_004
    }

    fn name(&self) -> &'static str {
        "References keywords"
    }

    fn description(&self) -> &'static str {
        "Keywords should not be used as identifiers."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        if !statement_contains_keyword_identifier(statement, self) {
            return Vec::new();
        }

        let mut issue = Issue::info(issue_codes::LINT_RF_004, "Keyword used as identifier.")
            .with_statement(ctx.statement_index);

        let autofix_edits =
            keyword_table_alias_autofix_edits(ctx.statement_sql(), ctx.dialect(), self)
                .into_iter()
                .map(|edit| {
                    IssuePatchEdit::new(
                        ctx.span_from_statement_offset(edit.start, edit.end),
                        edit.replacement,
                    )
                })
                .collect::<Vec<_>>();
        if !autofix_edits.is_empty() {
            issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, autofix_edits);
        }

        vec![issue]
    }
}

struct Rf004AutofixEdit {
    start: usize,
    end: usize,
    replacement: String,
}

#[derive(Clone)]
struct SimpleTableAliasDecl {
    keyword_start: usize,
    keyword_end: usize,
    table_start: usize,
    table_end: usize,
    alias_end: usize,
    alias: String,
    explicit_as: bool,
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn keyword_table_alias_autofix_edits(
    sql: &str,
    dialect: Dialect,
    rule: &ReferencesKeywords,
) -> Vec<Rf004AutofixEdit> {
    if !rule.unquoted_policy.allows(IdentifierKind::TableAlias) {
        return Vec::new();
    }

    let Some(decls) = collect_simple_table_alias_declarations(sql, dialect) else {
        return Vec::new();
    };

    let mut edits = Vec::new();
    for decl in decls {
        if !decl.explicit_as
            || !is_alias_keyword_token(&decl.alias)
            || is_ignored_token(&decl.alias, rule)
        {
            continue;
        }
        let clause = &sql[decl.keyword_start..decl.keyword_end];
        let table = &sql[decl.table_start..decl.table_end];
        edits.push(Rf004AutofixEdit {
            start: decl.keyword_start,
            end: decl.alias_end,
            replacement: format!(
                "{clause} {table} AS alias_{}",
                decl.alias.to_ascii_lowercase()
            ),
        });
    }

    edits
}

fn collect_simple_table_alias_declarations(
    sql: &str,
    dialect: Dialect,
) -> Option<Vec<SimpleTableAliasDecl>> {
    let tokens = tokenize_with_offsets(sql, dialect)?;
    let mut out = Vec::new();
    let mut index = 0usize;

    while index < tokens.len() {
        if !token_matches_keyword(&tokens[index].token, "FROM")
            && !token_matches_keyword(&tokens[index].token, "JOIN")
        {
            index += 1;
            continue;
        }

        let keyword_start = tokens[index].start;
        let keyword_end = tokens[index].end;

        let Some(mut cursor) = next_non_trivia_token(&tokens, index + 1) else {
            index += 1;
            continue;
        };
        if token_simple_identifier(&tokens[cursor].token).is_none() {
            index += 1;
            continue;
        }

        let table_start = tokens[cursor].start;
        let mut table_end = tokens[cursor].end;
        cursor += 1;

        while let Some(dot_index) = next_non_trivia_token(&tokens, cursor) {
            if !matches!(tokens[dot_index].token, Token::Period) {
                break;
            }
            let Some(next_index) = next_non_trivia_token(&tokens, dot_index + 1) else {
                break;
            };
            if token_simple_identifier(&tokens[next_index].token).is_none() {
                break;
            }
            table_end = tokens[next_index].end;
            cursor = next_index + 1;
        }

        let Some(mut alias_index) = next_non_trivia_token(&tokens, cursor) else {
            index += 1;
            continue;
        };
        let mut explicit_as = false;
        if token_matches_keyword(&tokens[alias_index].token, "AS") {
            explicit_as = true;
            let Some(next_index) = next_non_trivia_token(&tokens, alias_index + 1) else {
                index += 1;
                continue;
            };
            alias_index = next_index;
        }

        let Some(alias) = token_simple_identifier(&tokens[alias_index].token) else {
            index += 1;
            continue;
        };

        out.push(SimpleTableAliasDecl {
            keyword_start,
            keyword_end,
            table_start,
            table_end,
            alias_end: tokens[alias_index].end,
            alias: alias.to_string(),
            explicit_as,
        });
        index = alias_index + 1;
    }

    Some(out)
}

fn tokenize_with_offsets(sql: &str, dialect: Dialect) -> Option<Vec<LocatedToken>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        let (start, end) = token_with_span_offsets(sql, &token)?;
        out.push(LocatedToken {
            token: token.token,
            start,
            end,
        });
    }
    Some(out)
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

fn next_non_trivia_token(tokens: &[LocatedToken], mut start: usize) -> Option<usize> {
    while start < tokens.len() {
        if !is_trivia_token(&tokens[start].token) {
            return Some(start);
        }
        start += 1;
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

fn token_matches_keyword(token: &Token, keyword: &str) -> bool {
    matches!(token, Token::Word(word) if word.value.eq_ignore_ascii_case(keyword))
}

fn token_simple_identifier(token: &Token) -> Option<&str> {
    match token {
        Token::Word(word) if is_simple_identifier(&word.value) => Some(&word.value),
        _ => None,
    }
}

fn is_simple_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !is_ascii_ident_start(bytes[0]) {
        return false;
    }
    bytes[1..].iter().copied().all(is_ascii_ident_continue)
}

fn is_ascii_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ascii_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_alias_keyword_token(alias: &str) -> bool {
    is_keyword(alias)
}

fn statement_contains_keyword_identifier(statement: &Statement, rule: &ReferencesKeywords) -> bool {
    collect_identifier_candidates(statement)
        .into_iter()
        .any(|candidate| candidate_triggers_rule(&candidate, rule))
}

fn candidate_triggers_rule(candidate: &IdentifierCandidate, rule: &ReferencesKeywords) -> bool {
    // SQLFluff skips 1-character identifiers (e.g. datepart keywords like "d").
    if candidate.value.len() <= 1 {
        return false;
    }
    if is_ignored_token(&candidate.value, rule) || !is_keyword(&candidate.value) {
        return false;
    }

    if candidate.quoted {
        rule.quoted_policy.allows(candidate.kind)
    } else {
        rule.unquoted_policy.allows(candidate.kind)
    }
}

fn configured_ignore_words(config: &LintConfig) -> Vec<String> {
    if let Some(words) = config.rule_option_string_list(issue_codes::LINT_RF_004, "ignore_words") {
        return words;
    }

    config
        .rule_option_str(issue_codes::LINT_RF_004, "ignore_words")
        .map(|words| {
            words
                .split(',')
                .map(str::trim)
                .filter(|word| !word.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn is_ignored_token(token: &str, rule: &ReferencesKeywords) -> bool {
    let normalized = normalize_token(token);
    rule.ignore_words.contains(&normalized)
        || rule
            .ignore_words_regex
            .as_ref()
            .is_some_and(|regex| regex.is_match(&normalized))
}

fn normalize_token(token: &str) -> String {
    token
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '`' | '\'' | '[' | ']'))
        .to_ascii_uppercase()
}

fn is_keyword(token: &str) -> bool {
    let upper = token.trim().to_ascii_uppercase();
    (ALL_KEYWORDS.binary_search(&upper.as_str()).is_ok() || is_sqlfluff_extra_keyword(&upper))
        && !is_non_keyword_identifier(&upper)
}

fn is_sqlfluff_extra_keyword(upper: &str) -> bool {
    // SQLFluff treats COST as a keyword for PostgreSQL, while sqlparser does not.
    matches!(upper, "COST")
}

/// Returns true for words that sqlparser includes in `ALL_KEYWORDS` but that
/// SQLFluff does not treat as keywords for RF04.  These fall into two groups:
///
///  1. Window/aggregate function names (compound names with underscores like
///     `ROW_NUMBER`, `DATE_PART`) — excluding `CURRENT_*` / `LOCAL_*` /
///     `SESSION_*` / `SYSTEM_*` which are SQL-standard reserved pseudo-functions.
///  2. Dialect-specific parser tokens that are not general SQL keywords
///     (`METADATA` for BigQuery, `CHANNEL` for PostgreSQL LISTEN/NOTIFY, etc.).
fn is_non_keyword_identifier(upper: &str) -> bool {
    if upper.contains('_')
        && !upper.starts_with("CURRENT_")
        && !upper.starts_with("LOCAL_")
        && !upper.starts_with("SESSION_")
        && !upper.starts_with("SYSTEM_")
    {
        return true;
    }
    matches!(upper, "CHANNEL" | "GENERATED" | "METADATA" | "STATUS")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        run_with_config(sql, LintConfig::default())
    }

    fn run_with_config(sql: &str, config: LintConfig) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesKeywords::from_config(&config);
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
    fn flags_unquoted_keyword_table_alias() {
        let issues = run("SELECT sum.id FROM users AS sum");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn emits_safe_autofix_for_explicit_keyword_table_alias() {
        let sql = "select a from users as select";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "select a from users AS alias_select");
    }

    #[test]
    fn flags_unquoted_keyword_projection_alias() {
        let issues = run("SELECT amount AS sum FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn flags_cost_alias_as_keyword_identifier() {
        let issues = run("SELECT 1 AS cost");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn flags_unquoted_keyword_cte_alias() {
        let issues = run("WITH sum AS (SELECT 1 AS value) SELECT value FROM sum");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn does_not_flag_quoted_keyword_alias_by_default() {
        assert!(run("SELECT \"select\".id FROM users AS \"select\"").is_empty());
    }

    #[test]
    fn does_not_flag_non_keyword_alias() {
        let issues = run("SELECT u.id FROM users AS u");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_sql_like_string_literal() {
        let issues = run("SELECT 'FROM users AS date' AS snippet");
        assert!(issues.is_empty());
    }

    #[test]
    fn quoted_identifiers_policy_all_flags_quoted_keyword_alias() {
        let issues = run_with_config(
            "SELECT \"select\".id FROM users AS \"select\"",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "references.keywords".to_string(),
                    serde_json::json!({"quoted_identifiers_policy": "all"}),
                )]),
            },
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn unquoted_column_alias_policy_does_not_flag_table_alias() {
        let issues = run_with_config(
            "SELECT sum.id FROM users AS sum",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "LINT_RF_004".to_string(),
                    serde_json::json!({"unquoted_identifiers_policy": "column_aliases"}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_words_suppresses_keyword_identifier() {
        let issues = run_with_config(
            "SELECT amount AS sum FROM t",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "references.keywords".to_string(),
                    serde_json::json!({"ignore_words": ["sum"]}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_words_regex_suppresses_keyword_identifier() {
        let issues = run_with_config(
            "SELECT amount AS sum FROM t",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "LINT_RF_004".to_string(),
                    serde_json::json!({"ignore_words_regex": "^s.*"}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_keyword_as_column_name_in_create_table() {
        // SQLFluff: test_fail_keyword_as_identifier_column
        let issues = run("CREATE TABLE artist(create TEXT)");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn flags_keyword_as_column_alias() {
        // SQLFluff: test_fail_keyword_as_identifier_column_alias
        let issues = run("SELECT 1 as parameter");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn flags_keyword_as_table_alias() {
        // SQLFluff: test_fail_keyword_as_identifier_table_alias
        let issues = run("SELECT x FROM tbl AS parameter");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn does_not_flag_non_alias_with_aliases_policy() {
        // SQLFluff: test_pass_valid_identifier_not_alias
        // Default unquoted_identifiers_policy is "aliases", so bare column
        // references that are not aliases should not trigger.
        assert!(run("SELECT parameter").is_empty());
    }

    #[test]
    fn flags_non_alias_with_all_policy() {
        // SQLFluff: test_fail_keyword_as_identifier_not_alias_all
        let issues = run_with_config(
            "SELECT parameter",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "references.keywords".to_string(),
                    serde_json::json!({"unquoted_identifiers_policy": "all"}),
                )]),
            },
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_column_alias_with_column_aliases_policy() {
        // SQLFluff: test_fail_keyword_as_identifier_column_alias_config
        let issues = run_with_config(
            "SELECT x AS date FROM tbl AS parameter",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "references.keywords".to_string(),
                    serde_json::json!({"unquoted_identifiers_policy": "column_aliases"}),
                )]),
            },
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_quoted_keyword_column_in_create_table() {
        // SQLFluff: test_fail_keyword_as_quoted_identifier_column
        let issues = run_with_config(
            "CREATE TABLE \"artist\"(\"create\" TEXT)",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "references.keywords".to_string(),
                    serde_json::json!({"quoted_identifiers_policy": "aliases"}),
                )]),
            },
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_keyword_as_column_name_postgres() {
        // SQLFluff: test_fail_keyword_as_column_name_postgres
        let issues = run("CREATE TABLE test_table (type varchar(30) NOT NULL)");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn does_not_flag_function_name_as_keyword() {
        // ROW_NUMBER is a window function name, not a SQL keyword.
        assert!(run("SELECT ROW_NUMBER() OVER () AS row_number FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_non_keyword_identifiers() {
        // Words in sqlparser's ALL_KEYWORDS that are not treated as keywords
        // by SQLFluff: METADATA, CHANNEL, STATUS, GENERATED.
        assert!(run("WITH generated AS (SELECT 1 AS x) SELECT x FROM generated").is_empty());
        assert!(run("SELECT x AS status FROM t").is_empty());
        assert!(run("SELECT x AS metadata FROM t").is_empty());
        assert!(run("SELECT x AS channel FROM t").is_empty());
    }

    #[test]
    fn still_flags_current_date_as_keyword() {
        // CURRENT_DATE is a SQL-standard reserved pseudo-function.
        let issues = run("SELECT x AS current_date FROM t");
        assert_eq!(issues.len(), 1);
    }
}
