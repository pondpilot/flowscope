//! LINT_RF_006: References quoting.
//!
//! SQLFluff RF06 parity (current scope): quoted identifiers that are valid
//! bare identifiers are treated as unnecessarily quoted.

use std::collections::HashSet;

use crate::generated::NormalizationStrategy;
use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use regex::Regex;
use sqlparser::ast::Statement;

use super::identifier_candidates_helpers::{
    collect_identifier_candidates, IdentifierCandidate, IdentifierKind, IdentifierPolicy,
};

pub struct ReferencesQuoting {
    prefer_quoted_identifiers: bool,
    prefer_quoted_keywords: bool,
    case_sensitive_override: Option<bool>,
    quoted_policy: IdentifierPolicy,
    unquoted_policy: IdentifierPolicy,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
}

impl Default for ReferencesQuoting {
    fn default() -> Self {
        Self {
            prefer_quoted_identifiers: false,
            prefer_quoted_keywords: false,
            case_sensitive_override: None,
            quoted_policy: IdentifierPolicy::All,
            unquoted_policy: IdentifierPolicy::All,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
        }
    }
}

impl ReferencesQuoting {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            prefer_quoted_identifiers: config
                .rule_option_bool(issue_codes::LINT_RF_006, "prefer_quoted_identifiers")
                .unwrap_or(false),
            prefer_quoted_keywords: config
                .rule_option_bool(issue_codes::LINT_RF_006, "prefer_quoted_keywords")
                .unwrap_or(false),
            case_sensitive_override: config
                .rule_option_bool(issue_codes::LINT_RF_006, "case_sensitive"),
            quoted_policy: IdentifierPolicy::from_config(
                config,
                issue_codes::LINT_RF_006,
                "quoted_identifiers_policy",
                "all",
            ),
            unquoted_policy: IdentifierPolicy::from_config(
                config,
                issue_codes::LINT_RF_006,
                "unquoted_identifiers_policy",
                "all",
            ),
            ignore_words: configured_ignore_words(config)
                .into_iter()
                .map(|word| normalize_token(&word))
                .collect(),
            ignore_words_regex: config
                .rule_option_str(issue_codes::LINT_RF_006, "ignore_words_regex")
                .filter(|pattern| !pattern.trim().is_empty())
                .and_then(|pattern| Regex::new(pattern).ok()),
        }
    }

    /// Resolve whether to check case-sensitively for the given dialect.
    fn is_case_aware(&self, dialect: Dialect) -> bool {
        match self.case_sensitive_override {
            Some(false) => false,
            Some(true) => true,
            None => {
                // Default: follow the dialect's normalization strategy.
                // Dialects with case folding (lowercase/uppercase) are case-aware by default.
                // Case-insensitive dialects are not case-aware by default.
                matches!(
                    dialect.normalization_strategy(),
                    NormalizationStrategy::Lowercase | NormalizationStrategy::Uppercase
                )
            }
        }
    }
}

impl BuiltinLintRule for ReferencesQuoting {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_006
    }

    fn name(&self) -> &'static str {
        "References quoting"
    }

    fn description(&self) -> &'static str {
        "Unnecessary quoted identifier."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let dialect = ctx.dialect();

        let ast_has_violation = collect_identifier_candidates(statement)
            .into_iter()
            .any(|candidate| candidate_triggers_rule(&candidate, self, dialect));

        let mut autofix_edits_raw =
            unnecessary_quoted_identifier_edits(ctx.statement_sql(), self, dialect);
        if is_spark_insert_overwrite_directory_options(ctx.statement_sql(), dialect) {
            autofix_edits_raw.extend(spark_options_unquote_key_edits(ctx.statement_sql(), self));
            autofix_edits_raw.sort_by_key(|edit| (edit.start, edit.end));
            autofix_edits_raw.dedup_by(|left, right| {
                left.start == right.start
                    && left.end == right.end
                    && left.replacement == right.replacement
            });
        }

        let fallback_has_violation =
            is_spark_insert_overwrite_directory_options(ctx.statement_sql(), dialect)
                && !autofix_edits_raw.is_empty();

        let has_violation = ast_has_violation || fallback_has_violation;

        if !has_violation {
            return Vec::new();
        }

        let message = if self.prefer_quoted_identifiers {
            "Identifiers should be quoted."
        } else {
            "Identifier quoting appears unnecessary."
        };
        let mut issue =
            Issue::info(issue_codes::LINT_RF_006, message).with_statement(ctx.statement_index);

        let autofix_edits = autofix_edits_raw
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

fn is_spark_insert_overwrite_directory_options(sql: &str, dialect: Dialect) -> bool {
    if !matches!(dialect, Dialect::Databricks) {
        return false;
    }
    let upper = sql.to_ascii_uppercase();
    upper.contains("INSERT OVERWRITE DIRECTORY")
        && upper.contains("USING")
        && upper.contains("OPTIONS")
}

fn spark_options_unquote_key_edits(sql: &str, rule: &ReferencesQuoting) -> Vec<Rf006AutofixEdit> {
    if rule.prefer_quoted_identifiers || !rule.quoted_policy.allows(IdentifierKind::Other) {
        return Vec::new();
    }

    let bytes = sql.as_bytes();
    let mut edits = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] != b'"' {
            index += 1;
            continue;
        }

        let start = index;
        index += 1;
        let ident_start = index;
        while index < bytes.len() && bytes[index] != b'"' {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let ident_end = index;
        let end = index + 1;

        let Some(ident) = sql.get(ident_start..ident_end) else {
            index = end;
            continue;
        };
        if ident.is_empty() {
            index = end;
            continue;
        }
        if is_keyword(ident) || is_ignored_token(ident, rule) || !is_valid_bare_identifier(ident) {
            index = end;
            continue;
        }

        let mut prev = start;
        while prev > 0 && bytes[prev - 1].is_ascii_whitespace() {
            prev -= 1;
        }
        if prev == 0 || !matches!(bytes[prev - 1], b'(' | b',') {
            index = end;
            continue;
        }

        let mut probe = end;
        while probe < bytes.len() && bytes[probe].is_ascii_whitespace() {
            probe += 1;
        }
        if probe >= bytes.len() || bytes[probe] != b'=' {
            index = end;
            continue;
        }

        edits.push(Rf006AutofixEdit {
            start,
            end,
            replacement: ident.to_string(),
        });
        index = end;
    }

    edits
}

struct Rf006AutofixEdit {
    start: usize,
    end: usize,
    replacement: String,
}

fn unnecessary_quoted_identifier_edits(
    sql: &str,
    rule: &ReferencesQuoting,
    dialect: Dialect,
) -> Vec<Rf006AutofixEdit> {
    if rule.prefer_quoted_identifiers || !rule.quoted_policy.allows(IdentifierKind::Other) {
        return Vec::new();
    }

    let case_aware = rule.is_case_aware(dialect);
    let strategy = dialect.normalization_strategy();

    let bytes = sql.as_bytes();
    let mut edits = Vec::new();
    let mut index = 0usize;
    let mut in_single = false;

    // Determine quote characters for this dialect.
    let quote_chars: &[u8] = match dialect {
        Dialect::Mssql => b"\"[",
        Dialect::Bigquery | Dialect::Databricks | Dialect::Hive | Dialect::Mysql => b"`",
        _ => b"\"",
    };

    while index < bytes.len() {
        // Track single-quoted string literals to avoid false matches.
        if bytes[index] == b'\'' {
            if in_single && index + 1 < bytes.len() && bytes[index + 1] == b'\'' {
                index += 2;
                continue;
            }
            in_single = !in_single;
            index += 1;
            continue;
        }

        if in_single {
            index += 1;
            continue;
        }

        let is_quote = quote_chars.contains(&bytes[index]);
        if !is_quote {
            index += 1;
            continue;
        }

        let quote_byte = bytes[index];
        let close_byte = if quote_byte == b'[' { b']' } else { quote_byte };

        let start = index;
        index += 1;
        let ident_start = index;
        let mut escaped_quote = false;

        while index < bytes.len() {
            if bytes[index] == close_byte {
                if close_byte != b']' && index + 1 < bytes.len() && bytes[index + 1] == close_byte {
                    escaped_quote = true;
                    index += 2;
                    continue;
                }
                break;
            }
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let ident_end = index;
        let end = index + 1;
        let Some(ident) = sql.get(ident_start..ident_end) else {
            index += 1;
            continue;
        };

        if !escaped_quote
            && quoted_identifier_allows_safe_unquote(ident, rule, dialect, case_aware, strategy)
            && can_unquote_identifier(ident, case_aware, strategy)
        {
            edits.push(Rf006AutofixEdit {
                start,
                end,
                replacement: ident.to_string(),
            });
        }

        index = end;
    }

    edits
}

fn quoted_identifier_allows_safe_unquote(
    ident: &str,
    rule: &ReferencesQuoting,
    _dialect: Dialect,
    case_aware: bool,
    strategy: NormalizationStrategy,
) -> bool {
    if is_ignored_token(ident, rule) {
        return false;
    }

    if !rule.quoted_policy.allows(IdentifierKind::Other) {
        return false;
    }

    if rule.prefer_quoted_keywords && is_keyword(ident) {
        return false;
    }

    is_unnecessarily_quoted(ident, case_aware, strategy)
}

fn can_unquote_identifier(
    identifier: &str,
    case_aware: bool,
    strategy: NormalizationStrategy,
) -> bool {
    if !is_valid_bare_identifier(identifier) {
        return false;
    }

    if is_keyword(identifier) {
        return false;
    }

    // When case-aware, only unquote if the identifier matches the casefold.
    if case_aware {
        return matches_casefold(identifier, strategy);
    }

    true
}

fn candidate_triggers_rule(
    candidate: &IdentifierCandidate,
    rule: &ReferencesQuoting,
    dialect: Dialect,
) -> bool {
    if is_ignored_token(&candidate.value, rule) {
        return false;
    }

    let policy = if candidate.quoted {
        rule.quoted_policy
    } else {
        rule.unquoted_policy
    };
    if !policy.allows(candidate.kind) {
        return false;
    }

    if rule.prefer_quoted_identifiers {
        return !candidate.quoted;
    }

    if !candidate.quoted {
        return false;
    }

    if rule.prefer_quoted_keywords && is_keyword(&candidate.value) {
        return false;
    }

    let case_aware = rule.is_case_aware(dialect);
    let strategy = dialect.normalization_strategy();
    is_unnecessarily_quoted(&candidate.value, case_aware, strategy)
}

fn is_unnecessarily_quoted(ident: &str, case_aware: bool, strategy: NormalizationStrategy) -> bool {
    if !is_valid_bare_identifier(ident) {
        return false;
    }

    if is_keyword(ident) {
        return false;
    }

    if case_aware {
        return matches_casefold(ident, strategy);
    }

    true
}

fn is_valid_bare_identifier(ident: &str) -> bool {
    let mut chars = ident.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn matches_casefold(ident: &str, strategy: NormalizationStrategy) -> bool {
    match strategy {
        NormalizationStrategy::Uppercase => ident.chars().all(|ch| !ch.is_ascii_lowercase()),
        NormalizationStrategy::Lowercase => ident.chars().all(|ch| !ch.is_ascii_uppercase()),
        NormalizationStrategy::CaseInsensitive => true,
        NormalizationStrategy::CaseSensitive => true,
    }
}

fn configured_ignore_words(config: &LintConfig) -> Vec<String> {
    if let Some(words) = config.rule_option_string_list(issue_codes::LINT_RF_006, "ignore_words") {
        return words;
    }

    config
        .rule_option_str(issue_codes::LINT_RF_006, "ignore_words")
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

fn is_ignored_token(token: &str, rule: &ReferencesQuoting) -> bool {
    let normalized = normalize_token(token);
    // ignore_words matches case-insensitively (via normalization).
    if rule.ignore_words.contains(&normalized) {
        return true;
    }
    // ignore_words_regex matches case-sensitively against the raw value,
    // consistent with SQLFluff behavior.
    if let Some(regex) = &rule.ignore_words_regex {
        let raw = token
            .trim()
            .trim_matches(|ch| matches!(ch, '"' | '`' | '\'' | '[' | ']'));
        if regex.is_match(raw) {
            return true;
        }
    }
    false
}

fn normalize_token(token: &str) -> String {
    token
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '`' | '\'' | '[' | ']'))
        .to_ascii_uppercase()
}

fn is_keyword(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_uppercase().as_str(),
        "ALL"
            | "AND"
            | "AS"
            | "ASC"
            | "BETWEEN"
            | "BY"
            | "CASE"
            | "CROSS"
            | "DEFAULT"
            | "DELETE"
            | "DESC"
            | "DISTINCT"
            | "ELSE"
            | "END"
            | "EXISTS"
            | "FALSE"
            | "FOR"
            | "FROM"
            | "FULL"
            | "GROUP"
            | "HAVING"
            | "IF"
            | "IN"
            | "INNER"
            | "INSERT"
            | "INTO"
            | "IS"
            | "JOIN"
            | "LEFT"
            | "LIKE"
            | "LIMIT"
            | "METADATA"
            | "NOT"
            | "NULL"
            | "OFFSET"
            | "ON"
            | "OR"
            | "ORDER"
            | "OUTER"
            | "RECURSIVE"
            | "RIGHT"
            | "SELECT"
            | "SET"
            | "SUM"
            | "TABLE"
            | "THEN"
            | "TRUE"
            | "UNION"
            | "UPDATE"
            | "USER"
            | "USING"
            | "VALUES"
            | "WHEN"
            | "WHERE"
            | "WITH"
            | "DATETIME"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::Dialect;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesQuoting::default();
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
    fn flags_unnecessary_quoted_identifier() {
        let sql = "SELECT \"good_name\" FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_006);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT good_name FROM t");
    }

    #[test]
    fn mixed_case_quoted_identifier_remains_report_only() {
        let issues = run("SELECT \"MixedCase\" FROM t");
        // In generic dialect (CaseInsensitive), MixedCase is unnecessarily quoted.
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn does_not_flag_quoted_identifier_with_special_char() {
        let issues = run("SELECT \"bad-name\" FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_double_quotes_inside_string_literal() {
        let issues = run("SELECT '\"good_name\"' AS note FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn prefer_quoted_identifiers_true_disables_unnecessary_quote_issues() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.quoting".to_string(),
                serde_json::json!({"prefer_quoted_identifiers": true}),
            )]),
        };
        let rule = ReferencesQuoting::from_config(&config);
        let sql = "SELECT \"good_name\" FROM \"t\"";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn prefer_quoted_identifiers_true_flags_unquoted_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.quoting".to_string(),
                serde_json::json!({"prefer_quoted_identifiers": true}),
            )]),
        };
        let rule = ReferencesQuoting::from_config(&config);
        let sql = "SELECT good_name FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn prefer_quoted_keywords_true_allows_quoted_keyword_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_RF_006".to_string(),
                serde_json::json!({"prefer_quoted_keywords": true}),
            )]),
        };
        let rule = ReferencesQuoting::from_config(&config);
        let sql = "SELECT \"select\".id FROM users AS \"select\"";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn quoted_policy_none_skips_quoted_identifier_checks() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.quoting".to_string(),
                serde_json::json!({"quoted_identifiers_policy": "none"}),
            )]),
        };
        let rule = ReferencesQuoting::from_config(&config);
        let sql = "SELECT \"good_name\" FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_words_suppresses_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_RF_006".to_string(),
                serde_json::json!({"ignore_words": ["good_name"]}),
            )]),
        };
        let rule = ReferencesQuoting::from_config(&config);
        let sql = "SELECT \"good_name\" FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_words_regex_suppresses_identifier() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.quoting".to_string(),
                serde_json::json!({"ignore_words_regex": "^good_"}),
            )]),
        };
        let rule = ReferencesQuoting::from_config(&config);
        let sql = "SELECT \"good_name\" FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_keyword_identifier() {
        let issues = run("SELECT \"SELECT\" FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_datetime_keyword_identifier() {
        let issues = run("SELECT \"datetime\" FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn sparksql_insert_overwrite_directory_options_uses_fallback_detection_and_fix() {
        let sql = "INSERT OVERWRITE DIRECTORY '/tmp/destination'\nUSING PARQUET\nOPTIONS (\"col1\" = \"1\", \"col2\" = \"2\", \"col3\" = 'test', \"user\" = \"a person\")\nSELECT a FROM test_table;";
        let statements = parse_sql("SELECT 1").expect("synthetic parse");
        let rule = ReferencesQuoting::default();

        let issues = rule.check_with_context(
            &statements[0],
            &RuleContext::new(sql, 0..sql.len(), 0).with_dialect(Dialect::Databricks),
        );
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "INSERT OVERWRITE DIRECTORY '/tmp/destination'\nUSING PARQUET\nOPTIONS (col1 = \"1\", col2 = \"2\", col3 = 'test', \"user\" = \"a person\")\nSELECT a FROM test_table;"
        );
    }
}
