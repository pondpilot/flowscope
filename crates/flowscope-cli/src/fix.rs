//! SQL lint auto-fix helpers.
//!
//! Fixing is best-effort and deterministic. We combine:
//! - AST rewrites for structurally safe transforms.
//! - Text rewrites for parity-style formatting/convention rules.
//! - Lint before/after comparison to report per-rule removed violations.

use flowscope_core::linter::config::canonicalize_rule_code;
use flowscope_core::{
    analyze, issue_codes, linter::helpers as lint_helpers, parse_sql_with_dialect, AnalysisOptions,
    AnalyzeRequest, Dialect, LintConfig, ParseError,
};
use regex::{Captures, Regex};
use sqlparser::ast::*;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[must_use]
pub struct FixCounts {
    /// Per-rule fix counts, ordered by rule code for deterministic output.
    by_rule: BTreeMap<String, usize>,
}

impl FixCounts {
    pub fn total(&self) -> usize {
        self.by_rule.values().sum()
    }

    pub fn add(&mut self, code: &str, count: usize) {
        if count == 0 {
            return;
        }
        *self.by_rule.entry(code.to_string()).or_insert(0) += count;
    }

    pub fn get(&self, code: &str) -> usize {
        self.by_rule.get(code).copied().unwrap_or(0)
    }

    fn from_removed(before: &BTreeMap<String, usize>, after: &BTreeMap<String, usize>) -> Self {
        let mut out = Self::default();
        for (code, before_count) in before {
            let after_count = after.get(code).copied().unwrap_or(0);
            if *before_count > after_count {
                out.add(code, before_count - after_count);
            }
        }
        out
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub struct FixOutcome {
    pub sql: String,
    pub counts: FixCounts,
    pub changed: bool,
    pub skipped_due_to_comments: bool,
    pub skipped_due_to_regression: bool,
}

#[derive(Debug, Clone, Default)]
struct RuleFilter {
    disabled: HashSet<String>,
}

impl RuleFilter {
    fn new(disabled_rules: &[String]) -> Self {
        let disabled = disabled_rules
            .iter()
            .filter_map(|rule| {
                let trimmed = rule.trim();
                if trimmed.is_empty() {
                    return None;
                }
                Some(
                    canonicalize_rule_code(trimmed).unwrap_or_else(|| trimmed.to_ascii_uppercase()),
                )
            })
            .collect();
        Self { disabled }
    }

    fn allows(&self, code: &str) -> bool {
        let canonical =
            canonicalize_rule_code(code).unwrap_or_else(|| code.trim().to_ascii_uppercase());
        !self.disabled.contains(&canonical)
    }
}

/// Apply deterministic lint fixes to a SQL document.
///
/// Notes:
/// - If comment markers are detected, auto-fix is skipped to avoid losing
///   comments when rendering SQL from AST.
/// - Parse errors are returned so callers can decide whether to continue linting.
pub fn apply_lint_fixes(
    sql: &str,
    dialect: Dialect,
    disabled_rules: &[String],
) -> Result<FixOutcome, ParseError> {
    let rule_filter = RuleFilter::new(disabled_rules);

    if contains_comment_markers(sql, dialect) {
        return Ok(FixOutcome {
            sql: sql.to_string(),
            counts: FixCounts::default(),
            changed: false,
            skipped_due_to_comments: true,
            skipped_due_to_regression: false,
        });
    }

    let before_counts = lint_rule_counts(sql, dialect, disabled_rules);
    let mut statements = parse_sql_with_dialect(sql, dialect)?;
    for stmt in &mut statements {
        fix_statement(stmt, &rule_filter);
    }

    let mut fixed_sql = render_statements(&statements, sql);
    fixed_sql = apply_text_fixes(&fixed_sql, &rule_filter);

    let after_counts = lint_rule_counts(&fixed_sql, dialect, disabled_rules);
    let counts = FixCounts::from_removed(&before_counts, &after_counts);

    if counts.total() == 0 {
        // Regression guard: no rules improved. Flag as regression if total violations
        // actually increased (text-fix side effects introduced new violations).
        let before_total: usize = before_counts.values().sum();
        let after_total: usize = after_counts.values().sum();
        return Ok(FixOutcome {
            sql: sql.to_string(),
            counts,
            changed: false,
            skipped_due_to_comments: false,
            skipped_due_to_regression: after_total > before_total,
        });
    }
    let changed = fixed_sql != sql;

    Ok(FixOutcome {
        sql: fixed_sql,
        counts,
        changed,
        skipped_due_to_comments: false,
        skipped_due_to_regression: false,
    })
}

/// Check whether SQL contains comment markers outside of string literals.
///
/// Scans character-by-character so that `'--'` inside a string literal is
/// not mistaken for an actual comment.
fn contains_comment_markers(sql: &str, dialect: Dialect) -> bool {
    let bytes = sql.as_bytes();
    let mut in_single = false;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\'' {
            in_single = !in_single;
            i += 1;
            continue;
        }

        if in_single {
            i += 1;
            continue;
        }

        // Line comment: --
        if b == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            return true;
        }

        // Block comment: /*
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            return true;
        }

        // MySQL hash comment: #
        if matches!(dialect, Dialect::Mysql) && b == b'#' {
            return true;
        }

        i += 1;
    }

    false
}

fn render_statements(statements: &[Statement], original: &str) -> String {
    let mut rendered = statements
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(";\n");

    if statements.len() > 1 || original.trim_end().ends_with(';') {
        rendered.push(';');
    }

    rendered
}

fn lint_rule_counts(
    sql: &str,
    dialect: Dialect,
    disabled_rules: &[String],
) -> BTreeMap<String, usize> {
    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect,
        source_name: None,
        options: Some(AnalysisOptions {
            lint: Some(LintConfig {
                enabled: true,
                disabled_rules: disabled_rules.to_vec(),
                rule_configs: std::collections::BTreeMap::new(),
            }),
            ..Default::default()
        }),
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    };

    let mut counts = BTreeMap::new();
    for issue in analyze(&request)
        .issues
        .into_iter()
        .filter(|issue| issue.code.starts_with("LINT_"))
    {
        *counts.entry(issue.code).or_insert(0usize) += 1;
    }
    counts
}

fn apply_text_fixes(sql: &str, rule_filter: &RuleFilter) -> String {
    let mut out = sql.to_string();

    if rule_filter.allows(issue_codes::LINT_JJ_001) {
        out = fix_jinja_padding(&out);
    }
    if rule_filter.allows(issue_codes::LINT_ST_012) {
        out = fix_consecutive_semicolons(&out);
    }
    if rule_filter.allows(issue_codes::LINT_CV_007) {
        out = fix_statement_brackets(&out);
    }
    if rule_filter.allows(issue_codes::LINT_CV_001) {
        out = fix_not_equal_operator(&out);
    }
    if rule_filter.allows(issue_codes::LINT_CV_003) {
        out = fix_trailing_select_comma(&out);
    }
    if rule_filter.allows(issue_codes::LINT_LT_013) {
        out = fix_leading_blank_lines(&out);
    }
    if rule_filter.allows(issue_codes::LINT_LT_015) {
        out = fix_excessive_blank_lines(&out);
    }
    if rule_filter.allows(issue_codes::LINT_LT_001) {
        out = fix_operator_spacing(&out);
    }
    if rule_filter.allows(issue_codes::LINT_LT_004) {
        out = fix_comma_spacing(&out);
    }
    if rule_filter.allows(issue_codes::LINT_LT_006) {
        out = fix_function_spacing(&out);
    }
    if rule_filter.allows(issue_codes::LINT_LT_005) {
        out = fix_long_lines(&out);
    }
    if rule_filter.allows(issue_codes::LINT_LT_011) {
        out = fix_set_operator_layout(&out);
    }
    if rule_filter.allows(issue_codes::LINT_LT_007) {
        out = fix_cte_bracket(&out);
    }
    if rule_filter.allows(issue_codes::LINT_LT_008) {
        out = fix_cte_newline(&out);
    }
    if rule_filter.allows(issue_codes::LINT_LT_009) {
        out = fix_select_target_newline(&out);
    }
    if rule_filter.allows(issue_codes::LINT_LT_014) {
        out = fix_keyword_newlines(&out);
    }
    if rule_filter.allows(issue_codes::LINT_AL_001) {
        out = fix_missing_table_aliases(&out);
    }
    if rule_filter.allows(issue_codes::LINT_AL_009) {
        out = fix_self_aliases(&out);
    }
    if rule_filter.allows(issue_codes::LINT_AL_007) {
        out = fix_single_table_aliases(&out);
    }
    if rule_filter.allows(issue_codes::LINT_AL_005) {
        out = fix_unused_table_aliases(&out);
    }
    if rule_filter.allows(issue_codes::LINT_RF_004) {
        out = fix_table_alias_keywords(&out);
    }
    if rule_filter.allows(issue_codes::LINT_ST_005) {
        out = fix_subquery_to_cte(&out);
    }
    if rule_filter.allows(issue_codes::LINT_RF_003) {
        out = fix_mixed_reference_qualification(&out);
    }
    if rule_filter.allows(issue_codes::LINT_RF_006) {
        out = fix_references_quoting(&out);
    }
    if rule_filter.allows(issue_codes::LINT_CP_001) {
        out = fix_case_style_consistency(&out);
    }
    if rule_filter.allows(issue_codes::LINT_TQ_002) {
        out = fix_tsql_procedure_begin_end(&out);
    }
    if rule_filter.allows(issue_codes::LINT_TQ_003) {
        out = fix_tsql_empty_batches(&out);
    }
    if rule_filter.allows(issue_codes::LINT_LT_012) {
        out = fix_trailing_newline(&out);
    }

    out
}

fn regex_replace_all(sql: &str, pattern: &str, replacement: &str) -> String {
    match Regex::new(pattern) {
        Ok(re) => re.replace_all(sql, replacement).into_owned(),
        Err(_) => sql.to_string(),
    }
}

fn regex_replace_all_with<F>(sql: &str, pattern: &str, replacement: F) -> String
where
    F: FnMut(&Captures<'_>) -> String,
{
    match Regex::new(pattern) {
        Ok(re) => re.replace_all(sql, replacement).into_owned(),
        Err(_) => sql.to_string(),
    }
}

fn fix_jinja_padding(sql: &str) -> String {
    let out = regex_replace_all(sql, r"\{\{\s*([^{}]+?)\s*\}\}", "{{ $1 }}");
    regex_replace_all(&out, r"\{%\s*([^%]+?)\s*%\}", "{% $1 %}")
}

fn fix_consecutive_semicolons(sql: &str) -> String {
    regex_replace_all(sql, r";\s*;+", ";")
}

fn fix_statement_brackets(sql: &str) -> String {
    let trimmed = sql.trim();
    if trimmed.starts_with('(')
        && trimmed.ends_with(')')
        && trimmed[1..trimmed.len() - 1]
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("select")
    {
        return trimmed[1..trimmed.len() - 1].trim().to_string();
    }
    sql.to_string()
}

fn fix_not_equal_operator(sql: &str) -> String {
    replace_outside_single_quotes(sql, |segment| segment.replace("<>", "!="))
}

fn fix_trailing_select_comma(sql: &str) -> String {
    regex_replace_all(sql, r"(?i),\s*(from\b)", " $1")
}

fn replace_outside_single_quotes<F>(sql: &str, mut transform: F) -> String
where
    F: FnMut(&str) -> String,
{
    let mut out = String::with_capacity(sql.len());
    let mut outside = String::new();
    let mut chars = sql.chars().peekable();
    let mut in_single = false;

    while let Some(ch) = chars.next() {
        if in_single {
            out.push(ch);
            if ch == '\'' {
                if matches!(chars.peek(), Some('\'')) {
                    out.push(chars.next().expect("peek confirmed quote"));
                } else {
                    in_single = false;
                }
            }
            continue;
        }

        if ch == '\'' {
            if !outside.is_empty() {
                out.push_str(&transform(&outside));
                outside.clear();
            }
            out.push(ch);
            in_single = true;
            continue;
        }

        outside.push(ch);
    }

    if !outside.is_empty() {
        out.push_str(&transform(&outside));
    }

    out
}

fn replace_outside_quoted_regions_and_comments<F>(sql: &str, mut transform: F) -> String
where
    F: FnMut(&str) -> String,
{
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Outside,
        SingleQuote,
        DoubleQuote,
        BacktickQuote,
        BracketQuote,
        LineComment,
        BlockComment,
    }

    let mut out = String::with_capacity(sql.len());
    let mut outside = String::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut mode = Mode::Outside;
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        let next = chars.get(i + 1).copied();

        match mode {
            Mode::Outside => {
                if ch == '\'' {
                    if !outside.is_empty() {
                        out.push_str(&transform(&outside));
                        outside.clear();
                    }
                    out.push(ch);
                    mode = Mode::SingleQuote;
                    i += 1;
                    continue;
                }
                if ch == '"' {
                    if !outside.is_empty() {
                        out.push_str(&transform(&outside));
                        outside.clear();
                    }
                    out.push(ch);
                    mode = Mode::DoubleQuote;
                    i += 1;
                    continue;
                }
                if ch == '`' {
                    if !outside.is_empty() {
                        out.push_str(&transform(&outside));
                        outside.clear();
                    }
                    out.push(ch);
                    mode = Mode::BacktickQuote;
                    i += 1;
                    continue;
                }
                if ch == '[' {
                    if !outside.is_empty() {
                        out.push_str(&transform(&outside));
                        outside.clear();
                    }
                    out.push(ch);
                    mode = Mode::BracketQuote;
                    i += 1;
                    continue;
                }
                if ch == '-' && next == Some('-') {
                    if !outside.is_empty() {
                        out.push_str(&transform(&outside));
                        outside.clear();
                    }
                    out.push('-');
                    out.push('-');
                    mode = Mode::LineComment;
                    i += 2;
                    continue;
                }
                if ch == '/' && next == Some('*') {
                    if !outside.is_empty() {
                        out.push_str(&transform(&outside));
                        outside.clear();
                    }
                    out.push('/');
                    out.push('*');
                    mode = Mode::BlockComment;
                    i += 2;
                    continue;
                }

                outside.push(ch);
                i += 1;
            }
            Mode::SingleQuote => {
                out.push(ch);
                if ch == '\'' {
                    if next == Some('\'') {
                        out.push('\'');
                        i += 2;
                    } else {
                        mode = Mode::Outside;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            Mode::DoubleQuote => {
                out.push(ch);
                if ch == '"' {
                    if next == Some('"') {
                        out.push('"');
                        i += 2;
                    } else {
                        mode = Mode::Outside;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            Mode::BacktickQuote => {
                out.push(ch);
                if ch == '`' {
                    if next == Some('`') {
                        out.push('`');
                        i += 2;
                    } else {
                        mode = Mode::Outside;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            Mode::BracketQuote => {
                out.push(ch);
                if ch == ']' {
                    if next == Some(']') {
                        out.push(']');
                        i += 2;
                    } else {
                        mode = Mode::Outside;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            Mode::LineComment => {
                out.push(ch);
                i += 1;
                if ch == '\n' {
                    mode = Mode::Outside;
                }
            }
            Mode::BlockComment => {
                out.push(ch);
                if ch == '*' && next == Some('/') {
                    out.push('/');
                    mode = Mode::Outside;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        }
    }

    if !outside.is_empty() {
        out.push_str(&transform(&outside));
    }

    out
}

fn fix_leading_blank_lines(sql: &str) -> String {
    regex_replace_all(sql, r"^\s*\n+", "")
}

fn fix_excessive_blank_lines(sql: &str) -> String {
    regex_replace_all(sql, r"\n\s*\n\s*\n+", "\n\n")
}

fn fix_operator_spacing(sql: &str) -> String {
    replace_outside_single_quotes(sql, |segment| {
        let out = regex_replace_all(segment, r"(?i)([A-Za-z0-9_])=([A-Za-z0-9_'])", "$1 = $2");
        let out = regex_replace_all(&out, r"(?i)([A-Za-z0-9_])!=([A-Za-z0-9_'])", "$1 != $2");
        let out = regex_replace_all(&out, r"(?i)([A-Za-z0-9_])<([A-Za-z0-9_'])", "$1 < $2");
        let out = regex_replace_all(&out, r"(?i)([A-Za-z0-9_])>([A-Za-z0-9_'])", "$1 > $2");
        let out = regex_replace_all(&out, r"(?i)([A-Za-z0-9_])\+([A-Za-z0-9_'])", "$1 + $2");
        regex_replace_all(&out, r"(?i)([A-Za-z0-9_])-([A-Za-z0-9_'])", "$1 - $2")
    })
}

fn fix_comma_spacing(sql: &str) -> String {
    replace_outside_single_quotes(sql, |segment| {
        let out = regex_replace_all(segment, r"\s+,", ",");
        regex_replace_all(&out, r",\s*", ", ")
    })
}

fn fix_function_spacing(sql: &str) -> String {
    regex_replace_all_with(sql, r"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\s+\(", |caps| {
        let token = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        if is_sql_keyword(token) {
            caps[0].to_string()
        } else {
            format!("{token}(")
        }
    })
}

/// Maximum line length before `fix_long_lines` will attempt to split.
const MAX_LINE_LENGTH: usize = 300;
/// Target split position when breaking long lines.
const LINE_SPLIT_TARGET: usize = 280;

fn fix_long_lines(sql: &str) -> String {
    let mut out = String::new();
    for (idx, line) in sql.lines().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        if line.len() <= MAX_LINE_LENGTH {
            out.push_str(line);
            continue;
        }

        let mut remaining = line.trim_start();
        let mut first_segment = true;
        while remaining.len() > MAX_LINE_LENGTH {
            let probe = remaining
                .char_indices()
                .take_while(|(i, _)| *i <= LINE_SPLIT_TARGET)
                .map(|(i, _)| i)
                .last()
                .unwrap_or(LINE_SPLIT_TARGET.min(remaining.len()));
            let split_at = remaining[..probe].rfind(' ').unwrap_or(probe);
            if !first_segment {
                out.push('\n');
            }
            out.push_str(remaining[..split_at].trim_end());
            out.push('\n');
            remaining = remaining[split_at..].trim_start();
            first_segment = false;
        }
        out.push_str(remaining);
    }
    out
}

fn fix_set_operator_layout(sql: &str) -> String {
    regex_replace_all(
        sql,
        r"(?i)\s+(UNION(?:\s+ALL)?|INTERSECT|EXCEPT)\s+",
        "\n$1\n",
    )
}

fn fix_cte_bracket(sql: &str) -> String {
    regex_replace_all(
        sql,
        r"(?i)\bwith\s+([A-Za-z_][A-Za-z0-9_]*)\s+as\s+select\b",
        "WITH $1 AS (SELECT",
    )
}

fn fix_cte_newline(sql: &str) -> String {
    regex_replace_all(sql, r"(?i)\)\s+(SELECT\s+\*)", ")\n$1")
}

fn fix_select_target_newline(sql: &str) -> String {
    let re = Regex::new(r"(?is)\bselect\b(.*?)\bfrom\b").expect("valid fix regex");
    if let Some(caps) = re.captures(sql) {
        let clause = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let comma_count = clause.matches(',').count();
        if comma_count >= 4 {
            return regex_replace_all(sql, r"(?i)\s+from\b", "\nFROM");
        }
    }
    sql.to_string()
}

fn fix_keyword_newlines(sql: &str) -> String {
    replace_outside_quoted_regions_and_comments(sql, |segment| {
        let out = regex_replace_all(segment, r"(?i)\s+(FROM)\b", "\n$1");
        let out = regex_replace_all(&out, r"(?i)\s+(WHERE)\b", "\n$1");
        let out = regex_replace_all(&out, r"(?i)\s+(GROUP BY)\b", "\n$1");
        regex_replace_all(&out, r"(?i)\s+(ORDER BY)\b", "\n$1")
    })
}

fn fix_self_aliases(sql: &str) -> String {
    regex_replace_all_with(
        sql,
        r"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\s+AS\s+([A-Za-z_][A-Za-z0-9_]*)\b",
        |caps| {
            if caps[1].eq_ignore_ascii_case(&caps[2]) {
                caps[1].to_string()
            } else {
                caps[0].to_string()
            }
        },
    )
}

fn fix_missing_table_aliases(sql: &str) -> String {
    let mut alias_idx = 1usize;
    let out = regex_replace_all_with(
        sql,
        r"(?i)\bfrom\s+([A-Za-z_][A-Za-z0-9_\.]*)(\s+(?:join|where|group\s+by|order\s+by|having|limit|offset|$))",
        |caps| {
            let alias = format!("t{}", alias_idx);
            alias_idx += 1;
            format!("FROM {} AS {}{}", &caps[1], alias, &caps[2])
        },
    );
    regex_replace_all_with(
        &out,
        r"(?i)\bjoin\s+([A-Za-z_][A-Za-z0-9_\.]*)(\s+(?:on|using|join|where|group\s+by|order\s+by|having|limit|offset|$))",
        |caps| {
            let alias = format!("t{}", alias_idx);
            alias_idx += 1;
            format!("JOIN {} AS {}{}", &caps[1], alias, &caps[2])
        },
    )
}

fn fix_single_table_aliases(sql: &str) -> String {
    let table_refs = Regex::new(r"(?i)\b(?:from|join)\s+[A-Za-z_][A-Za-z0-9_\.]*\b")
        .expect("valid fix regex")
        .find_iter(sql)
        .count();
    if table_refs != 1 {
        return sql.to_string();
    }

    let re = Regex::new(
        r"(?i)\bfrom\s+([A-Za-z_][A-Za-z0-9_\.]*)\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*)\b",
    )
    .expect("valid fix regex");
    let Some(caps) = re.captures(sql) else {
        return sql.to_string();
    };
    let table = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
    let alias = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
    if is_sql_keyword(alias) {
        return sql.to_string();
    }
    let out = re.replace(sql, format!("FROM {table}")).into_owned();
    regex_replace_all(
        &out,
        &format!(r"(?i)\b{}\.", regex::escape(alias)),
        &format!("{table}."),
    )
}

fn fix_unused_table_aliases(sql: &str) -> String {
    let alias_re = Regex::new(
        r"(?i)\b(?:from|join)\s+[A-Za-z_][A-Za-z0-9_\.]*\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*)\b",
    )
    .expect("valid fix regex");
    let aliases: Vec<String> = alias_re
        .captures_iter(sql)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .collect();

    let mut out = sql.to_string();
    let generated_alias_re = Regex::new(r"^t\d+$").expect("valid fix regex");
    for alias in aliases {
        if is_sql_keyword(&alias) {
            continue;
        }
        if generated_alias_re.is_match(&alias) {
            continue;
        }
        let usage =
            Regex::new(&format!(r"(?i)\b{}\.", regex::escape(&alias))).expect("valid fix regex");
        if usage.is_match(&out) {
            continue;
        }
        let alias_decl = Regex::new(&format!(
            r"(?i)\b(from|join)\s+([A-Za-z_][A-Za-z0-9_\.]*)\s+(?:as\s+)?{}\b",
            regex::escape(&alias)
        ))
        .expect("valid fix regex");
        out = alias_decl.replace_all(&out, "$1 $2").into_owned();
    }
    out
}

fn is_sql_keyword(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "ALL"
            | "ALTER"
            | "AND"
            | "ANY"
            | "AS"
            | "ASC"
            | "BEGIN"
            | "BETWEEN"
            | "BOOLEAN"
            | "BY"
            | "CASE"
            | "CAST"
            | "CHECK"
            | "COLUMN"
            | "CONSTRAINT"
            | "CREATE"
            | "CROSS"
            | "DEFAULT"
            | "DELETE"
            | "DESC"
            | "DISTINCT"
            | "DROP"
            | "ELSE"
            | "END"
            | "EXCEPT"
            | "EXISTS"
            | "FALSE"
            | "FETCH"
            | "FOR"
            | "FOREIGN"
            | "FROM"
            | "FULL"
            | "GROUP"
            | "HAVING"
            | "IF"
            | "IN"
            | "INDEX"
            | "INNER"
            | "INSERT"
            | "INT"
            | "INTEGER"
            | "INTERSECT"
            | "INTO"
            | "IS"
            | "JOIN"
            | "KEY"
            | "LEFT"
            | "LIKE"
            | "LIMIT"
            | "NOT"
            | "NULL"
            | "OFFSET"
            | "ON"
            | "OR"
            | "ORDER"
            | "OUTER"
            | "OVER"
            | "PARTITION"
            | "PRIMARY"
            | "REFERENCES"
            | "RIGHT"
            | "SELECT"
            | "SET"
            | "TABLE"
            | "TEXT"
            | "THEN"
            | "TRUE"
            | "UNION"
            | "UNIQUE"
            | "UPDATE"
            | "USING"
            | "VALUES"
            | "VARCHAR"
            | "VIEW"
            | "WHEN"
            | "WHERE"
            | "WINDOW"
            | "WITH"
    )
}

fn fix_table_alias_keywords(sql: &str) -> String {
    regex_replace_all_with(
        sql,
        r"(?i)\b(from|join)\s+([A-Za-z_][A-Za-z0-9_\.]*)\s+as\s+(select|from|where|group|order|join|on)\b",
        |caps| {
            format!(
                "{} {} AS alias_{}",
                &caps[1],
                &caps[2],
                &caps[3].to_ascii_lowercase()
            )
        },
    )
}

fn fix_subquery_to_cte(sql: &str) -> String {
    let re = Regex::new(
        r"(?is)\bselect\s+\*\s+from\s+\(\s*(select\s+[^)]*)\)\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*)\b",
    )
    .expect("valid fix regex");
    let Some(caps) = re.captures(sql) else {
        return sql.to_string();
    };
    let subquery = caps.get(1).map(|m| m.as_str()).unwrap_or_default().trim();
    let alias = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
    if subquery.is_empty() || alias.is_empty() {
        return sql.to_string();
    }
    format!("WITH {alias} AS ({subquery}) SELECT * FROM {alias}")
}

fn fix_mixed_reference_qualification(sql: &str) -> String {
    let from_re = Regex::new(
        r"(?i)\bfrom\s+([A-Za-z_][A-Za-z0-9_\.]*)(?:\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*))?",
    )
    .expect("valid fix regex");
    let Some(from_caps) = from_re.captures(sql) else {
        return sql.to_string();
    };
    let table_name = from_caps.get(1).map(|m| m.as_str()).unwrap_or_default();
    let alias = from_caps.get(2).map(|m| m.as_str()).unwrap_or_default();
    let prefix = if alias.is_empty() {
        table_name.rsplit('.').next().unwrap_or(table_name)
    } else {
        alias
    };
    if prefix.is_empty() {
        return sql.to_string();
    }

    let select_re = Regex::new(r"(?is)\bselect\s+(.*?)\bfrom\b").expect("valid fix regex");
    let Some(select_caps) = select_re.captures(sql) else {
        return sql.to_string();
    };
    let select_clause = select_caps.get(1).map(|m| m.as_str()).unwrap_or_default();
    let items: Vec<String> = select_clause
        .split(',')
        .map(|item| item.trim().to_string())
        .collect();
    let has_qualified = items.iter().any(|item| {
        Regex::new(r"(?i)^[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*$")
            .expect("valid fix regex")
            .is_match(item)
    });
    let has_unqualified = items.iter().any(|item| {
        Regex::new(r"(?i)^[A-Za-z_][A-Za-z0-9_]*$")
            .expect("valid fix regex")
            .is_match(item)
    });
    if !(has_qualified && has_unqualified) {
        return sql.to_string();
    }

    let rewritten_items: Vec<String> = items
        .into_iter()
        .map(|item| {
            if Regex::new(r"(?i)^[A-Za-z_][A-Za-z0-9_]*$")
                .expect("valid fix regex")
                .is_match(&item)
            {
                format!("{prefix}.{item}")
            } else {
                item
            }
        })
        .collect();
    let rewritten_clause = rewritten_items.join(", ");
    select_re
        .replace(sql, format!("SELECT {rewritten_clause} FROM"))
        .into_owned()
}

fn fix_references_quoting(sql: &str) -> String {
    replace_outside_single_quotes(sql, |segment| {
        regex_replace_all_with(segment, r#""([A-Za-z_][A-Za-z0-9_]*)""#, |caps| {
            let ident = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            if can_unquote_identifier_safely(ident) {
                ident.to_string()
            } else {
                caps[0].to_string()
            }
        })
    })
}

fn can_unquote_identifier_safely(identifier: &str) -> bool {
    let mut chars = identifier.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    let starts_ok = first.is_ascii_lowercase() || first == '_';
    let rest_ok = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');

    starts_ok && rest_ok && !is_sql_keyword(identifier)
}

/// Lowercase SQL keywords while preserving identifiers and string literals.
///
/// Only tokens that match `is_sql_keyword` are lowered; everything else is
/// kept as-is. Content inside quoted literals/identifiers is never touched.
fn fix_case_style_consistency(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if in_single {
            out.push(chars[i]);
            if chars[i] == '\'' {
                if i + 1 < chars.len() && chars[i + 1] == '\'' {
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                in_single = false;
            }
            i += 1;
            continue;
        }

        if in_double {
            out.push(chars[i]);
            if chars[i] == '"' {
                if i + 1 < chars.len() && chars[i + 1] == '"' {
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                in_double = false;
            }
            i += 1;
            continue;
        }

        if chars[i] == '\'' {
            in_single = true;
            out.push(chars[i]);
            i += 1;
            continue;
        }

        if chars[i] == '"' {
            in_double = true;
            out.push(chars[i]);
            i += 1;
            continue;
        }

        // Collect word tokens and lowercase only if they are SQL keywords
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            if is_sql_keyword(&token) {
                out.push_str(&token.to_ascii_lowercase());
            } else {
                out.push_str(&token);
            }
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }
    out
}

fn fix_tsql_procedure_begin_end(sql: &str) -> String {
    regex_replace_all_with(
        sql,
        r"(?i)create\s+(?:proc|procedure)\s+([A-Za-z_][A-Za-z0-9_]*)(\s*')",
        |caps| format!("CREATE PROCEDURE {} BEGIN END{}", &caps[1], &caps[2]),
    )
}

fn fix_tsql_empty_batches(sql: &str) -> String {
    regex_replace_all(sql, r"(?im)(\n\s*GO\s*){2,}", "\nGO\n")
}

fn fix_trailing_newline(sql: &str) -> String {
    if sql.contains('\n') && !sql.ends_with('\n') {
        return format!("{sql}\n");
    }
    sql.to_string()
}

fn fix_statement(stmt: &mut Statement, rule_filter: &RuleFilter) {
    match stmt {
        Statement::Query(query) => fix_query(query, rule_filter),
        Statement::Insert(insert) => {
            if let Some(source) = insert.source.as_mut() {
                fix_query(source, rule_filter);
            }
        }
        Statement::CreateView { query, .. } => fix_query(query, rule_filter),
        Statement::CreateTable(create) => {
            if let Some(query) = create.query.as_mut() {
                fix_query(query, rule_filter);
            }
        }
        _ => {}
    }
}

fn fix_query(query: &mut Query, rule_filter: &RuleFilter) {
    if let Some(with) = query.with.as_mut() {
        for cte in &mut with.cte_tables {
            fix_query(&mut cte.query, rule_filter);
        }
    }

    fix_set_expr(query.body.as_mut(), rule_filter);

    if let Some(order_by) = query.order_by.as_mut() {
        fix_order_by(order_by, rule_filter);
    }

    if let Some(limit_clause) = query.limit_clause.as_mut() {
        fix_limit_clause(limit_clause, rule_filter);
    }

    if let Some(fetch) = query.fetch.as_mut() {
        if let Some(quantity) = fetch.quantity.as_mut() {
            fix_expr(quantity, rule_filter);
        }
    }
}

fn fix_set_expr(body: &mut SetExpr, rule_filter: &RuleFilter) {
    match body {
        SetExpr::Select(select) => fix_select(select, rule_filter),
        SetExpr::Query(query) => fix_query(query, rule_filter),
        SetExpr::SetOperation {
            op,
            set_quantifier,
            left,
            right,
        } => {
            fix_set_expr(left, rule_filter);
            fix_set_expr(right, rule_filter);

            if rule_filter.allows(issue_codes::LINT_AM_002)
                && matches!(op, SetOperator::Union)
                && matches!(set_quantifier, SetQuantifier::None | SetQuantifier::ByName)
            {
                *set_quantifier = if matches!(set_quantifier, SetQuantifier::ByName) {
                    SetQuantifier::DistinctByName
                } else {
                    SetQuantifier::Distinct
                };
            }
        }
        SetExpr::Values(values) => {
            for row in &mut values.rows {
                for expr in row {
                    fix_expr(expr, rule_filter);
                }
            }
        }
        SetExpr::Insert(stmt)
        | SetExpr::Update(stmt)
        | SetExpr::Delete(stmt)
        | SetExpr::Merge(stmt) => fix_statement(stmt, rule_filter),
        _ => {}
    }
}

fn fix_select(select: &mut Select, rule_filter: &RuleFilter) {
    if rule_filter.allows(issue_codes::LINT_AM_001) && has_distinct_and_group_by(select) {
        select.distinct = None;
    }

    if rule_filter.allows(issue_codes::LINT_ST_008) {
        rewrite_distinct_parenthesized_projection(select);
    }

    for item in &mut select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                fix_expr(expr, rule_filter);
            }
            _ => {}
        }
    }

    if rule_filter.allows(issue_codes::LINT_ST_006) {
        if let Some(first_simple_idx) = select.projection.iter().position(is_simple_projection_item)
        {
            if first_simple_idx > 0 {
                let mut prefix = select
                    .projection
                    .drain(0..first_simple_idx)
                    .collect::<Vec<_>>();
                select.projection.append(&mut prefix);
            }
        }
    }

    let has_where_clause = select.selection.is_some();

    for table_with_joins in &mut select.from {
        if rule_filter.allows(issue_codes::LINT_CV_008) {
            rewrite_right_join_to_left(table_with_joins);
        }

        fix_table_factor(
            &mut table_with_joins.relation,
            rule_filter,
            has_where_clause,
        );

        let mut left_ref = table_factor_reference_name(&table_with_joins.relation);

        for join in &mut table_with_joins.joins {
            let right_ref = table_factor_reference_name(&join.relation);
            if rule_filter.allows(issue_codes::LINT_ST_007) {
                rewrite_using_join_constraint(
                    &mut join.join_operator,
                    left_ref.as_deref(),
                    right_ref.as_deref(),
                );
            }
            if rule_filter.allows(issue_codes::LINT_ST_009) {
                rewrite_join_condition_order(
                    &mut join.join_operator,
                    right_ref.as_deref(),
                    left_ref.as_deref(),
                );
            }

            fix_table_factor(&mut join.relation, rule_filter, has_where_clause);
            fix_join_operator(&mut join.join_operator, rule_filter, has_where_clause);

            if right_ref.is_some() {
                left_ref = right_ref;
            }
        }
    }

    if let Some(prewhere) = select.prewhere.as_mut() {
        fix_expr(prewhere, rule_filter);
    }

    if let Some(selection) = select.selection.as_mut() {
        fix_expr(selection, rule_filter);
    }

    if let Some(having) = select.having.as_mut() {
        fix_expr(having, rule_filter);
    }

    if let Some(qualify) = select.qualify.as_mut() {
        fix_expr(qualify, rule_filter);
    }

    if let GroupByExpr::Expressions(exprs, _) = &mut select.group_by {
        for expr in exprs {
            fix_expr(expr, rule_filter);
        }
    }

    for expr in &mut select.cluster_by {
        fix_expr(expr, rule_filter);
    }

    for expr in &mut select.distribute_by {
        fix_expr(expr, rule_filter);
    }

    for expr in &mut select.sort_by {
        fix_expr(&mut expr.expr, rule_filter);
    }

    for lateral_view in &mut select.lateral_views {
        fix_expr(&mut lateral_view.lateral_view, rule_filter);
    }

    if let Some(connect_by) = select.connect_by.as_mut() {
        fix_expr(&mut connect_by.condition, rule_filter);
        for relationship in &mut connect_by.relationships {
            fix_expr(relationship, rule_filter);
        }
    }
}

fn rewrite_distinct_parenthesized_projection(select: &mut Select) {
    if !matches!(select.distinct, Some(Distinct::Distinct)) {
        return;
    }

    if select.projection.len() != 1 {
        return;
    }

    if let SelectItem::UnnamedExpr(expr) = &mut select.projection[0] {
        if let Expr::Nested(inner) = expr {
            *expr = inner.as_ref().clone();
        }
    }
}

fn has_distinct_and_group_by(select: &Select) -> bool {
    let has_distinct = matches!(
        select.distinct,
        Some(Distinct::Distinct) | Some(Distinct::On(_))
    );
    let has_group_by = match &select.group_by {
        GroupByExpr::All(_) => true,
        GroupByExpr::Expressions(exprs, _) => !exprs.is_empty(),
    };
    has_distinct && has_group_by
}

fn rewrite_right_join_to_left(table_with_joins: &mut TableWithJoins) {
    while let Some(index) = table_with_joins
        .joins
        .iter()
        .position(|join| rewritable_right_join(&join.join_operator))
    {
        rewrite_right_join_at_index(table_with_joins, index);
    }
}

fn rewrite_right_join_at_index(table_with_joins: &mut TableWithJoins, index: usize) {
    let mut suffix = table_with_joins.joins.split_off(index);
    let mut join = suffix.remove(0);

    let old_operator = std::mem::replace(
        &mut join.join_operator,
        JoinOperator::CrossJoin(JoinConstraint::None),
    );
    let Some(new_operator) = rewritten_left_join_operator(old_operator) else {
        table_with_joins.joins.push(join);
        table_with_joins.joins.append(&mut suffix);
        return;
    };

    let previous_relation = std::mem::replace(&mut table_with_joins.relation, join.relation);
    let prefix_joins = std::mem::take(&mut table_with_joins.joins);

    join.relation = if prefix_joins.is_empty() {
        previous_relation
    } else {
        TableFactor::NestedJoin {
            table_with_joins: Box::new(TableWithJoins {
                relation: previous_relation,
                joins: prefix_joins,
            }),
            alias: None,
        }
    };
    join.join_operator = new_operator;

    table_with_joins.joins.push(join);
    table_with_joins.joins.append(&mut suffix);
}

fn rewritable_right_join(operator: &JoinOperator) -> bool {
    matches!(
        operator,
        JoinOperator::Right(_)
            | JoinOperator::RightOuter(_)
            | JoinOperator::RightSemi(_)
            | JoinOperator::RightAnti(_)
    )
}

fn rewritten_left_join_operator(operator: JoinOperator) -> Option<JoinOperator> {
    match operator {
        JoinOperator::Right(constraint) => Some(JoinOperator::Left(constraint)),
        JoinOperator::RightOuter(constraint) => Some(JoinOperator::LeftOuter(constraint)),
        JoinOperator::RightSemi(constraint) => Some(JoinOperator::LeftSemi(constraint)),
        JoinOperator::RightAnti(constraint) => Some(JoinOperator::LeftAnti(constraint)),
        _ => None,
    }
}

fn is_simple_projection_item(item: &SelectItem) -> bool {
    match item {
        SelectItem::UnnamedExpr(Expr::Identifier(_))
        | SelectItem::UnnamedExpr(Expr::CompoundIdentifier(_)) => true,
        SelectItem::ExprWithAlias { expr, .. } => {
            matches!(expr, Expr::Identifier(_) | Expr::CompoundIdentifier(_))
        }
        _ => false,
    }
}

fn table_factor_reference_name(relation: &TableFactor) -> Option<String> {
    match relation {
        TableFactor::Table { name, alias, .. } => {
            if let Some(alias) = alias {
                Some(alias.name.value.clone())
            } else {
                name.0
                    .last()
                    .and_then(|part| part.as_ident())
                    .map(|ident| ident.value.clone())
            }
        }
        _ => None,
    }
}

fn rewrite_using_join_constraint(
    join_operator: &mut JoinOperator,
    left_ref: Option<&str>,
    right_ref: Option<&str>,
) {
    let (Some(left_ref), Some(right_ref)) = (left_ref, right_ref) else {
        return;
    };

    let Some(constraint) = join_constraint_mut(join_operator) else {
        return;
    };

    let JoinConstraint::Using(columns) = constraint else {
        return;
    };

    if columns.is_empty() {
        return;
    }

    let mut combined: Option<Expr> = None;
    for object_name in columns.iter() {
        let Some(column_ident) = object_name
            .0
            .last()
            .and_then(|part| part.as_ident())
            .cloned()
        else {
            continue;
        };

        let equality = Expr::BinaryOp {
            left: Box::new(Expr::CompoundIdentifier(vec![
                Ident::new(left_ref),
                column_ident.clone(),
            ])),
            op: BinaryOperator::Eq,
            right: Box::new(Expr::CompoundIdentifier(vec![
                Ident::new(right_ref),
                column_ident,
            ])),
        };

        combined = Some(match combined {
            Some(prev) => Expr::BinaryOp {
                left: Box::new(prev),
                op: BinaryOperator::And,
                right: Box::new(equality),
            },
            None => equality,
        });
    }

    if let Some(on_expr) = combined {
        *constraint = JoinConstraint::On(on_expr);
    }
}

fn rewrite_join_condition_order(
    join_operator: &mut JoinOperator,
    current_source: Option<&str>,
    previous_source: Option<&str>,
) {
    let (Some(current_source), Some(previous_source)) = (current_source, previous_source) else {
        return;
    };

    let current_source = current_source.to_ascii_uppercase();
    let previous_source = previous_source.to_ascii_uppercase();

    let Some(constraint) = join_constraint_mut(join_operator) else {
        return;
    };

    let JoinConstraint::On(on_expr) = constraint else {
        return;
    };

    rewrite_reversed_join_pairs(on_expr, &current_source, &previous_source);
}

fn rewrite_reversed_join_pairs(expr: &mut Expr, current_source: &str, previous_source: &str) {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            if *op == BinaryOperator::Eq {
                let left_prefix = expr_qualified_prefix(left);
                let right_prefix = expr_qualified_prefix(right);
                if left_prefix.as_deref() == Some(current_source)
                    && right_prefix.as_deref() == Some(previous_source)
                {
                    std::mem::swap(left, right);
                }
            }

            rewrite_reversed_join_pairs(left, current_source, previous_source);
            rewrite_reversed_join_pairs(right, current_source, previous_source);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::IsTrue(inner)
        | Expr::IsNotTrue(inner)
        | Expr::IsFalse(inner)
        | Expr::IsNotFalse(inner)
        | Expr::IsUnknown(inner)
        | Expr::IsNotUnknown(inner)
        | Expr::Cast { expr: inner, .. } => {
            rewrite_reversed_join_pairs(inner, current_source, previous_source)
        }
        Expr::InList {
            expr: target, list, ..
        } => {
            rewrite_reversed_join_pairs(target, current_source, previous_source);
            for item in list {
                rewrite_reversed_join_pairs(item, current_source, previous_source);
            }
        }
        Expr::Between {
            expr: target,
            low,
            high,
            ..
        } => {
            rewrite_reversed_join_pairs(target, current_source, previous_source);
            rewrite_reversed_join_pairs(low, current_source, previous_source);
            rewrite_reversed_join_pairs(high, current_source, previous_source);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand {
                rewrite_reversed_join_pairs(operand, current_source, previous_source);
            }
            for case_when in conditions {
                rewrite_reversed_join_pairs(
                    &mut case_when.condition,
                    current_source,
                    previous_source,
                );
                rewrite_reversed_join_pairs(&mut case_when.result, current_source, previous_source);
            }
            if let Some(else_result) = else_result {
                rewrite_reversed_join_pairs(else_result, current_source, previous_source);
            }
        }
        _ => {}
    }
}

fn expr_qualified_prefix(expr: &Expr) -> Option<String> {
    match expr {
        Expr::CompoundIdentifier(parts) if parts.len() > 1 => {
            parts.first().map(|ident| ident.value.to_ascii_uppercase())
        }
        Expr::Nested(inner)
        | Expr::UnaryOp { expr: inner, .. }
        | Expr::Cast { expr: inner, .. } => expr_qualified_prefix(inner),
        _ => None,
    }
}

fn fix_table_factor(relation: &mut TableFactor, rule_filter: &RuleFilter, has_where_clause: bool) {
    match relation {
        TableFactor::Table {
            args, with_hints, ..
        } => {
            if let Some(args) = args {
                for arg in &mut args.args {
                    fix_function_arg(arg, rule_filter);
                }
            }
            for hint in with_hints {
                fix_expr(hint, rule_filter);
            }
        }
        TableFactor::Derived { subquery, .. } => fix_query(subquery, rule_filter),
        TableFactor::TableFunction { expr, .. } => fix_expr(expr, rule_filter),
        TableFactor::Function { args, .. } => {
            for arg in args {
                fix_function_arg(arg, rule_filter);
            }
        }
        TableFactor::UNNEST { array_exprs, .. } => {
            for expr in array_exprs {
                fix_expr(expr, rule_filter);
            }
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            if rule_filter.allows(issue_codes::LINT_CV_008) {
                rewrite_right_join_to_left(table_with_joins);
            }

            fix_table_factor(
                &mut table_with_joins.relation,
                rule_filter,
                has_where_clause,
            );

            let mut left_ref = table_factor_reference_name(&table_with_joins.relation);

            for join in &mut table_with_joins.joins {
                let right_ref = table_factor_reference_name(&join.relation);
                if rule_filter.allows(issue_codes::LINT_ST_007) {
                    rewrite_using_join_constraint(
                        &mut join.join_operator,
                        left_ref.as_deref(),
                        right_ref.as_deref(),
                    );
                }
                if rule_filter.allows(issue_codes::LINT_ST_009) {
                    rewrite_join_condition_order(
                        &mut join.join_operator,
                        right_ref.as_deref(),
                        left_ref.as_deref(),
                    );
                }

                fix_table_factor(&mut join.relation, rule_filter, has_where_clause);
                fix_join_operator(&mut join.join_operator, rule_filter, has_where_clause);

                if right_ref.is_some() {
                    left_ref = right_ref;
                }
            }
        }
        TableFactor::Pivot {
            table,
            aggregate_functions,
            value_column,
            default_on_null,
            ..
        } => {
            fix_table_factor(table, rule_filter, has_where_clause);
            for func in aggregate_functions {
                fix_expr(&mut func.expr, rule_filter);
            }
            for expr in value_column {
                fix_expr(expr, rule_filter);
            }
            if let Some(expr) = default_on_null {
                fix_expr(expr, rule_filter);
            }
        }
        TableFactor::Unpivot {
            table,
            value,
            columns,
            ..
        } => {
            fix_table_factor(table, rule_filter, has_where_clause);
            fix_expr(value, rule_filter);
            for column in columns {
                fix_expr(&mut column.expr, rule_filter);
            }
        }
        TableFactor::JsonTable { json_expr, .. } => fix_expr(json_expr, rule_filter),
        TableFactor::OpenJsonTable { json_expr, .. } => fix_expr(json_expr, rule_filter),
        _ => {}
    }
}

fn fix_join_operator(op: &mut JoinOperator, rule_filter: &RuleFilter, has_where_clause: bool) {
    match op {
        JoinOperator::Join(constraint)
        | JoinOperator::Inner(constraint)
        | JoinOperator::Left(constraint)
        | JoinOperator::LeftOuter(constraint)
        | JoinOperator::Right(constraint)
        | JoinOperator::RightOuter(constraint)
        | JoinOperator::FullOuter(constraint)
        | JoinOperator::CrossJoin(constraint)
        | JoinOperator::Semi(constraint)
        | JoinOperator::LeftSemi(constraint)
        | JoinOperator::RightSemi(constraint)
        | JoinOperator::Anti(constraint)
        | JoinOperator::LeftAnti(constraint)
        | JoinOperator::RightAnti(constraint)
        | JoinOperator::StraightJoin(constraint) => fix_join_constraint(constraint, rule_filter),
        JoinOperator::AsOf {
            match_condition,
            constraint,
        } => {
            fix_expr(match_condition, rule_filter);
            fix_join_constraint(constraint, rule_filter);
        }
        JoinOperator::CrossApply | JoinOperator::OuterApply => {}
    }

    if rule_filter.allows(issue_codes::LINT_AM_008)
        && !has_where_clause
        && operator_requires_join_condition(op)
        && !join_constraint_is_explicit(op)
    {
        *op = JoinOperator::CrossJoin(JoinConstraint::None);
        return;
    }

    if rule_filter.allows(issue_codes::LINT_AM_005) {
        if let JoinOperator::Join(constraint) = op {
            *op = JoinOperator::Inner(constraint.clone());
        }
    }
}

fn operator_requires_join_condition(join_operator: &JoinOperator) -> bool {
    matches!(
        join_operator,
        JoinOperator::Join(_)
            | JoinOperator::Inner(_)
            | JoinOperator::Left(_)
            | JoinOperator::LeftOuter(_)
            | JoinOperator::Right(_)
            | JoinOperator::RightOuter(_)
            | JoinOperator::FullOuter(_)
            | JoinOperator::StraightJoin(_)
    )
}

fn join_constraint_is_explicit(join_operator: &JoinOperator) -> bool {
    let Some(constraint) = join_constraint(join_operator) else {
        return false;
    };

    matches!(
        constraint,
        JoinConstraint::On(_) | JoinConstraint::Using(_) | JoinConstraint::Natural
    )
}

fn join_constraint_mut(join_operator: &mut JoinOperator) -> Option<&mut JoinConstraint> {
    match join_operator {
        JoinOperator::Join(constraint)
        | JoinOperator::Inner(constraint)
        | JoinOperator::Left(constraint)
        | JoinOperator::LeftOuter(constraint)
        | JoinOperator::Right(constraint)
        | JoinOperator::RightOuter(constraint)
        | JoinOperator::FullOuter(constraint)
        | JoinOperator::CrossJoin(constraint)
        | JoinOperator::Semi(constraint)
        | JoinOperator::LeftSemi(constraint)
        | JoinOperator::RightSemi(constraint)
        | JoinOperator::Anti(constraint)
        | JoinOperator::LeftAnti(constraint)
        | JoinOperator::RightAnti(constraint)
        | JoinOperator::StraightJoin(constraint) => Some(constraint),
        JoinOperator::AsOf { constraint, .. } => Some(constraint),
        JoinOperator::CrossApply | JoinOperator::OuterApply => None,
    }
}

fn join_constraint(join_operator: &JoinOperator) -> Option<&JoinConstraint> {
    match join_operator {
        JoinOperator::Join(constraint)
        | JoinOperator::Inner(constraint)
        | JoinOperator::Left(constraint)
        | JoinOperator::LeftOuter(constraint)
        | JoinOperator::Right(constraint)
        | JoinOperator::RightOuter(constraint)
        | JoinOperator::FullOuter(constraint)
        | JoinOperator::CrossJoin(constraint)
        | JoinOperator::Semi(constraint)
        | JoinOperator::LeftSemi(constraint)
        | JoinOperator::RightSemi(constraint)
        | JoinOperator::Anti(constraint)
        | JoinOperator::LeftAnti(constraint)
        | JoinOperator::RightAnti(constraint)
        | JoinOperator::StraightJoin(constraint) => Some(constraint),
        JoinOperator::AsOf { constraint, .. } => Some(constraint),
        JoinOperator::CrossApply | JoinOperator::OuterApply => None,
    }
}

fn fix_join_constraint(constraint: &mut JoinConstraint, rule_filter: &RuleFilter) {
    if let JoinConstraint::On(expr) = constraint {
        fix_expr(expr, rule_filter);
    }
}

fn fix_order_by(order_by: &mut OrderBy, rule_filter: &RuleFilter) {
    if let OrderByKind::Expressions(exprs) = &mut order_by.kind {
        for order_expr in exprs.iter_mut() {
            fix_expr(&mut order_expr.expr, rule_filter);
        }

        if rule_filter.allows(issue_codes::LINT_AM_003) {
            let has_explicit = exprs
                .iter()
                .any(|order_expr| order_expr.options.asc.is_some());
            let has_implicit = exprs
                .iter()
                .any(|order_expr| order_expr.options.asc.is_none());

            if has_explicit && has_implicit {
                for order_expr in exprs.iter_mut() {
                    if order_expr.options.asc.is_none() {
                        order_expr.options.asc = Some(true);
                    }
                }
            }
        }
    }

    if let Some(interpolate) = order_by.interpolate.as_mut() {
        if let Some(exprs) = interpolate.exprs.as_mut() {
            for expr in exprs {
                if let Some(inner) = expr.expr.as_mut() {
                    fix_expr(inner, rule_filter);
                }
            }
        }
    }
}

fn fix_limit_clause(limit_clause: &mut LimitClause, rule_filter: &RuleFilter) {
    match limit_clause {
        LimitClause::LimitOffset {
            limit,
            offset,
            limit_by,
        } => {
            if let Some(limit) = limit {
                fix_expr(limit, rule_filter);
            }
            if let Some(offset) = offset {
                fix_expr(&mut offset.value, rule_filter);
            }
            for expr in limit_by {
                fix_expr(expr, rule_filter);
            }
        }
        LimitClause::OffsetCommaLimit { offset, limit } => {
            fix_expr(offset, rule_filter);
            fix_expr(limit, rule_filter);
        }
    }
}

fn fix_expr(expr: &mut Expr, rule_filter: &RuleFilter) {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            fix_expr(left, rule_filter);
            fix_expr(right, rule_filter);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::IsTrue(inner)
        | Expr::IsNotTrue(inner)
        | Expr::IsFalse(inner)
        | Expr::IsNotFalse(inner)
        | Expr::IsUnknown(inner)
        | Expr::IsNotUnknown(inner) => fix_expr(inner, rule_filter),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand.as_mut() {
                fix_expr(operand, rule_filter);
            }
            for case_when in conditions {
                fix_expr(&mut case_when.condition, rule_filter);
                fix_expr(&mut case_when.result, rule_filter);
            }
            if let Some(else_result) = else_result.as_mut() {
                fix_expr(else_result, rule_filter);
            }
        }
        Expr::Function(func) => fix_function(func, rule_filter),
        Expr::Cast { expr: inner, .. } => fix_expr(inner, rule_filter),
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => {
            fix_expr(inner, rule_filter);
            fix_query(subquery, rule_filter);
        }
        Expr::Subquery(subquery) | Expr::Exists { subquery, .. } => {
            fix_query(subquery, rule_filter)
        }
        Expr::Between {
            expr: target,
            low,
            high,
            ..
        } => {
            fix_expr(target, rule_filter);
            fix_expr(low, rule_filter);
            fix_expr(high, rule_filter);
        }
        Expr::InList {
            expr: target, list, ..
        } => {
            fix_expr(target, rule_filter);
            for item in list {
                fix_expr(item, rule_filter);
            }
        }
        Expr::Tuple(items) => {
            for item in items {
                fix_expr(item, rule_filter);
            }
        }
        _ => {}
    }

    if rule_filter.allows(issue_codes::LINT_CV_001) {
        if let Some(rewritten) = null_comparison_rewrite(expr) {
            *expr = rewritten;
            return;
        }
    }

    if rule_filter.allows(issue_codes::LINT_ST_004) {
        if let Some(rewritten) = nested_case_rewrite(expr) {
            *expr = rewritten;
        }
    }

    if rule_filter.allows(issue_codes::LINT_ST_002) {
        if let Some(rewritten) = simple_case_rewrite(expr) {
            *expr = rewritten;
        }
    }

    if let Expr::Case {
        else_result: Some(else_result),
        ..
    } = expr
    {
        if rule_filter.allows(issue_codes::LINT_ST_001) && lint_helpers::is_null_expr(else_result) {
            if let Expr::Case { else_result, .. } = expr {
                *else_result = None;
            }
        }
    }
}

fn fix_function(func: &mut Function, rule_filter: &RuleFilter) {
    if let FunctionArguments::List(arg_list) = &mut func.args {
        for arg in &mut arg_list.args {
            fix_function_arg(arg, rule_filter);
        }
        for clause in &mut arg_list.clauses {
            match clause {
                FunctionArgumentClause::OrderBy(order_by_exprs) => {
                    for order_by_expr in order_by_exprs {
                        fix_expr(&mut order_by_expr.expr, rule_filter);
                    }
                }
                FunctionArgumentClause::Limit(expr) => fix_expr(expr, rule_filter),
                _ => {}
            }
        }
    }

    if let Some(filter) = func.filter.as_mut() {
        fix_expr(filter, rule_filter);
    }

    for order_expr in &mut func.within_group {
        fix_expr(&mut order_expr.expr, rule_filter);
    }

    if rule_filter.allows(issue_codes::LINT_CV_002) {
        let function_name_upper = func.name.to_string().to_ascii_uppercase();
        if function_name_upper == "IFNULL" || function_name_upper == "NVL" {
            func.name = vec![Ident::new("COALESCE")].into();
        }
    }

    if rule_filter.allows(issue_codes::LINT_CV_004) && is_count_rowcount_numeric_literal(func) {
        if let FunctionArguments::List(arg_list) = &mut func.args {
            arg_list.args[0] = FunctionArg::Unnamed(FunctionArgExpr::Wildcard);
        }
    }
}

fn fix_function_arg(arg: &mut FunctionArg, rule_filter: &RuleFilter) {
    match arg {
        FunctionArg::Named { arg, .. }
        | FunctionArg::ExprNamed { arg, .. }
        | FunctionArg::Unnamed(arg) => {
            if let FunctionArgExpr::Expr(expr) = arg {
                fix_expr(expr, rule_filter);
            }
        }
    }
}

fn is_count_rowcount_numeric_literal(func: &Function) -> bool {
    if !func.name.to_string().eq_ignore_ascii_case("COUNT") {
        return false;
    }

    let FunctionArguments::List(arg_list) = &func.args else {
        return false;
    };

    if arg_list.duplicate_treatment.is_some() || !arg_list.clauses.is_empty() {
        return false;
    }

    if arg_list.args.len() != 1 {
        return false;
    }

    matches!(
        &arg_list.args[0],
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(ValueWithSpan {
            value: Value::Number(n, _),
            ..
        }))) if n == "1" || n == "0"
    )
}

fn null_comparison_rewrite(expr: &Expr) -> Option<Expr> {
    let Expr::BinaryOp { left, op, right } = expr else {
        return None;
    };

    let target = if lint_helpers::is_null_expr(right) {
        left.as_ref().clone()
    } else if lint_helpers::is_null_expr(left) {
        right.as_ref().clone()
    } else {
        return None;
    };

    match op {
        BinaryOperator::Eq => Some(Expr::IsNull(Box::new(target))),
        BinaryOperator::NotEq => Some(Expr::IsNotNull(Box::new(target))),
        _ => None,
    }
}

fn simple_case_rewrite(expr: &Expr) -> Option<Expr> {
    let Expr::Case {
        case_token,
        operand: None,
        conditions,
        else_result,
        end_token,
    } = expr
    else {
        return None;
    };

    if conditions.len() < 2 {
        return None;
    }

    let mut common_operand: Option<Expr> = None;
    let mut rewritten_conditions = Vec::with_capacity(conditions.len());

    for case_when in conditions {
        let (operand_expr, value_expr) =
            split_case_when_equality(&case_when.condition, common_operand.as_ref())?;

        if common_operand.is_none() {
            common_operand = Some(operand_expr);
        }

        rewritten_conditions.push(CaseWhen {
            condition: value_expr,
            result: case_when.result.clone(),
        });
    }

    Some(Expr::Case {
        case_token: case_token.clone(),
        operand: Some(Box::new(common_operand?)),
        conditions: rewritten_conditions,
        else_result: else_result.clone(),
        end_token: end_token.clone(),
    })
}

fn split_case_when_equality(
    condition: &Expr,
    expected_operand: Option<&Expr>,
) -> Option<(Expr, Expr)> {
    let Expr::BinaryOp { left, op, right } = condition else {
        return None;
    };

    if *op != BinaryOperator::Eq {
        return None;
    }

    if let Some(expected) = expected_operand {
        if exprs_equivalent(left, expected) {
            return Some((left.as_ref().clone(), right.as_ref().clone()));
        }
        if exprs_equivalent(right, expected) {
            return Some((right.as_ref().clone(), left.as_ref().clone()));
        }
        return None;
    }

    if simple_case_operand_candidate(left) {
        return Some((left.as_ref().clone(), right.as_ref().clone()));
    }
    if simple_case_operand_candidate(right) {
        return Some((right.as_ref().clone(), left.as_ref().clone()));
    }

    None
}

fn simple_case_operand_candidate(expr: &Expr) -> bool {
    matches!(expr, Expr::Identifier(_) | Expr::CompoundIdentifier(_))
}

fn exprs_equivalent(left: &Expr, right: &Expr) -> bool {
    format!("{left}") == format!("{right}")
}

fn nested_case_rewrite(expr: &Expr) -> Option<Expr> {
    let Expr::Case {
        case_token,
        operand: outer_operand,
        conditions: outer_conditions,
        else_result: Some(outer_else),
        end_token,
    } = expr
    else {
        return None;
    };

    if outer_conditions.is_empty() {
        return None;
    }

    let Expr::Case {
        operand: inner_operand,
        conditions: inner_conditions,
        else_result: inner_else,
        ..
    } = nested_case_expr(outer_else.as_ref())?
    else {
        return None;
    };

    if inner_conditions.is_empty() {
        return None;
    }

    if !case_operands_match(outer_operand.as_deref(), inner_operand.as_deref()) {
        return None;
    }

    let mut merged_conditions = outer_conditions.clone();
    merged_conditions.extend(inner_conditions.iter().cloned());

    Some(Expr::Case {
        case_token: case_token.clone(),
        operand: outer_operand.clone(),
        conditions: merged_conditions,
        else_result: inner_else.clone(),
        end_token: end_token.clone(),
    })
}

fn nested_case_expr(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Case { .. } => Some(expr),
        Expr::Nested(inner) => nested_case_expr(inner),
        _ => None,
    }
}

fn case_operands_match(outer: Option<&Expr>, inner: Option<&Expr>) -> bool {
    match (outer, inner) {
        (None, None) => true,
        (Some(left), Some(right)) => format!("{left}") == format!("{right}"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flowscope_core::{analyze, issue_codes, AnalysisOptions, AnalyzeRequest, LintConfig};

    fn lint_rule_count(sql: &str, code: &str) -> usize {
        let request = AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: Some(AnalysisOptions {
                lint: Some(LintConfig {
                    enabled: true,
                    disabled_rules: vec![],
                    rule_configs: std::collections::BTreeMap::new(),
                }),
                ..Default::default()
            }),
            schema: None,
            #[cfg(feature = "templating")]
            template_config: None,
        };

        analyze(&request)
            .issues
            .iter()
            .filter(|issue| issue.code == code)
            .count()
    }

    fn fix_count_for_code(counts: &FixCounts, code: &str) -> usize {
        counts.get(code)
    }

    fn assert_rule_case(
        sql: &str,
        code: &str,
        expected_before: usize,
        expected_after: usize,
        expected_fix_count: usize,
    ) {
        let before = lint_rule_count(sql, code);
        assert_eq!(
            before, expected_before,
            "unexpected initial lint count for {code} in SQL: {sql}"
        );

        let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
        assert!(
            !out.skipped_due_to_comments,
            "test SQL should not be skipped"
        );
        assert_eq!(
            fix_count_for_code(&out.counts, code),
            expected_fix_count,
            "unexpected fix count for {code} in SQL: {sql}"
        );

        if expected_fix_count > 0 {
            assert!(out.changed, "expected SQL to change for {code}: {sql}");
        }

        let after = lint_rule_count(&out.sql, code);
        assert_eq!(
            after, expected_after,
            "unexpected lint count after fix for {code}. SQL: {}",
            out.sql
        );

        let second_pass = apply_lint_fixes(&out.sql, Dialect::Generic, &[]).unwrap_or_else(|err| {
            panic!("second pass failed for SQL:\n{}\nerror: {err:?}", out.sql);
        });
        assert_eq!(
            fix_count_for_code(&second_pass.counts, code),
            0,
            "expected idempotent second pass for {code}"
        );
    }

    #[test]
    fn sqlfluff_am003_cases_are_fixed() {
        let cases = [
            ("SELECT DISTINCT col FROM t GROUP BY col", 1, 0, 1),
            (
                "SELECT * FROM (SELECT DISTINCT a FROM t GROUP BY a) AS sub",
                1,
                0,
                1,
            ),
            (
                "WITH cte AS (SELECT DISTINCT a FROM t GROUP BY a) SELECT * FROM cte",
                1,
                0,
                1,
            ),
            (
                "CREATE VIEW v AS SELECT DISTINCT a FROM t GROUP BY a",
                1,
                0,
                1,
            ),
            (
                "INSERT INTO target SELECT DISTINCT a FROM t GROUP BY a",
                1,
                0,
                1,
            ),
            (
                "SELECT a FROM t UNION ALL SELECT DISTINCT b FROM t2 GROUP BY b",
                1,
                0,
                1,
            ),
            ("SELECT a, b FROM t", 0, 0, 0),
        ];

        for (sql, before, after, fix_count) in cases {
            assert_rule_case(sql, issue_codes::LINT_AM_001, before, after, fix_count);
        }
    }

    #[test]
    fn sqlfluff_am001_cases_are_fixed_or_unchanged() {
        let cases = [
            (
                "SELECT a, b FROM tbl UNION SELECT c, d FROM tbl1",
                1,
                0,
                1,
                Some("DISTINCT SELECT"),
            ),
            (
                "SELECT a, b FROM tbl UNION ALL SELECT c, d FROM tbl1",
                0,
                0,
                0,
                None,
            ),
            (
                "SELECT a, b FROM tbl UNION DISTINCT SELECT c, d FROM tbl1",
                0,
                0,
                0,
                None,
            ),
            (
                "select a, b from tbl union select c, d from tbl1",
                1,
                0,
                1,
                Some("DISTINCT SELECT"),
            ),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_AM_002, before, after, fix_count);

            if let Some(expected) = expected_text {
                let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
                assert!(
                    out.sql.to_ascii_uppercase().contains(expected),
                    "expected {expected:?} in fixed SQL, got: {}",
                    out.sql
                );
            }
        }
    }

    #[test]
    fn sqlfluff_am005_cases_are_fixed_or_unchanged() {
        let cases = [
            (
                "SELECT * FROM t ORDER BY a, b DESC",
                1,
                0,
                1,
                Some("ORDER BY A ASC, B DESC"),
            ),
            (
                "SELECT * FROM t ORDER BY a DESC, b",
                1,
                0,
                1,
                Some("ORDER BY A DESC, B ASC"),
            ),
            (
                "SELECT * FROM t ORDER BY a DESC, b NULLS LAST",
                1,
                0,
                1,
                Some("ORDER BY A DESC, B ASC NULLS LAST"),
            ),
            ("SELECT * FROM t ORDER BY a, b", 0, 0, 0, None),
            ("SELECT * FROM t ORDER BY a ASC, b DESC", 0, 0, 0, None),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_AM_003, before, after, fix_count);

            if let Some(expected) = expected_text {
                let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
                assert!(
                    out.sql.to_ascii_uppercase().contains(expected),
                    "expected {expected:?} in fixed SQL, got: {}",
                    out.sql
                );
            }
        }
    }

    #[test]
    fn sqlfluff_am006_cases_are_fixed_or_unchanged() {
        let cases = [
            (
                "SELECT a FROM t JOIN u ON t.id = u.id",
                1,
                0,
                1,
                Some("INNER JOIN"),
            ),
            (
                "SELECT a FROM t JOIN u ON t.id = u.id JOIN v ON u.id = v.id",
                2,
                0,
                2,
                Some("INNER JOIN U"),
            ),
            ("SELECT a FROM t INNER JOIN u ON t.id = u.id", 0, 0, 0, None),
            ("SELECT a FROM t LEFT JOIN u ON t.id = u.id", 0, 0, 0, None),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_AM_005, before, after, fix_count);

            if let Some(expected) = expected_text {
                let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
                assert!(
                    out.sql.to_ascii_uppercase().contains(expected),
                    "expected {expected:?} in fixed SQL, got: {}",
                    out.sql
                );
            }
        }
    }

    #[test]
    fn sqlfluff_am009_cases_are_fixed_or_unchanged() {
        let cases = [
            (
                "SELECT foo.a, bar.b FROM foo INNER JOIN bar",
                1,
                0,
                1,
                Some("CROSS JOIN BAR"),
            ),
            (
                "SELECT foo.a, bar.b FROM foo LEFT JOIN bar",
                1,
                0,
                1,
                Some("CROSS JOIN BAR"),
            ),
            (
                "SELECT foo.a, bar.b FROM foo JOIN bar WHERE foo.a = bar.a OR foo.x = 3",
                0,
                0,
                0,
                None,
            ),
            ("SELECT foo.a, bar.b FROM foo CROSS JOIN bar", 0, 0, 0, None),
            (
                "SELECT foo.id, bar.id FROM foo LEFT JOIN bar USING (id)",
                0,
                0,
                0,
                None,
            ),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_AM_008, before, after, fix_count);

            if let Some(expected) = expected_text {
                let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
                assert!(
                    out.sql.to_ascii_uppercase().contains(expected),
                    "expected {expected:?} in fixed SQL, got: {}",
                    out.sql
                );
            }
        }
    }

    #[test]
    fn sqlfluff_st005_cases_are_fixed_or_unchanged() {
        let cases = [
            (
                "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' END FROM t",
                1,
                0,
                1,
                Some("CASE X WHEN 1 THEN 'A' WHEN 2 THEN 'B' END"),
            ),
            (
                "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' ELSE 'c' END FROM t",
                1,
                0,
                1,
                Some("CASE X WHEN 1 THEN 'A' WHEN 2 THEN 'B' ELSE 'C' END"),
            ),
            (
                "SELECT CASE WHEN x = 1 THEN 'a' WHEN y = 2 THEN 'b' END FROM t",
                0,
                0,
                0,
                None,
            ),
            (
                "SELECT CASE x WHEN 1 THEN 'a' WHEN 2 THEN 'b' END FROM t",
                0,
                0,
                0,
                None,
            ),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_ST_002, before, after, fix_count);

            if let Some(expected) = expected_text {
                let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
                assert!(
                    out.sql.to_ascii_uppercase().contains(expected),
                    "expected {expected:?} in fixed SQL, got: {}",
                    out.sql
                );
            }
        }
    }

    #[test]
    fn sqlfluff_st006_cases_are_fixed_or_unchanged() {
        let cases = [
            ("SELECT a + 1, a FROM t", 1, 0, 1, Some("SELECT A, A + 1")),
            (
                "SELECT a + 1, b + 2, a FROM t",
                1,
                0,
                1,
                Some("SELECT A, A + 1, B + 2"),
            ),
            (
                "SELECT a + 1, b AS b_alias FROM t",
                1,
                0,
                1,
                Some("SELECT B AS B_ALIAS, A + 1"),
            ),
            ("SELECT a, b + 1 FROM t", 0, 0, 0, None),
            ("SELECT a + 1, b + 2 FROM t", 0, 0, 0, None),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_ST_006, before, after, fix_count);

            if let Some(expected) = expected_text {
                let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
                assert!(
                    out.sql.to_ascii_uppercase().contains(expected),
                    "expected {expected:?} in fixed SQL, got: {}",
                    out.sql
                );
            }
        }
    }

    #[test]
    fn sqlfluff_st008_cases_are_fixed_or_unchanged() {
        let cases = [
            (
                "SELECT DISTINCT(a) FROM t",
                1,
                0,
                1,
                Some("SELECT DISTINCT A"),
            ),
            ("SELECT DISTINCT a FROM t", 0, 0, 0, None),
            ("SELECT a FROM t", 0, 0, 0, None),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_ST_008, before, after, fix_count);

            if let Some(expected) = expected_text {
                let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
                assert!(
                    out.sql.to_ascii_uppercase().contains(expected),
                    "expected {expected:?} in fixed SQL, got: {}",
                    out.sql
                );
            }
        }
    }

    #[test]
    fn sqlfluff_st009_cases_are_fixed_or_unchanged() {
        let cases = [
            (
                "SELECT foo.a, bar.b FROM foo LEFT JOIN bar ON bar.a = foo.a",
                1,
                0,
                1,
                Some("ON FOO.A = BAR.A"),
            ),
            (
                "SELECT foo.a, foo.b, bar.c FROM foo LEFT JOIN bar ON bar.a = foo.a AND bar.b = foo.b",
                1,
                0,
                1,
                Some("ON FOO.A = BAR.A AND FOO.B = BAR.B"),
            ),
            (
                "SELECT foo.a, bar.b FROM foo LEFT JOIN bar ON foo.a = bar.a",
                0,
                0,
                0,
                None,
            ),
            (
                "SELECT foo.a, bar.b FROM foo LEFT JOIN bar ON bar.b = a",
                0,
                0,
                0,
                None,
            ),
            (
                "SELECT foo.a, bar.b FROM foo AS x LEFT JOIN bar AS y ON y.a = x.a",
                1,
                0,
                1,
                Some("ON X.A = Y.A"),
            ),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_ST_009, before, after, fix_count);

            if let Some(expected) = expected_text {
                let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
                assert!(
                    out.sql.to_ascii_uppercase().contains(expected),
                    "expected {expected:?} in fixed SQL, got: {}",
                    out.sql
                );
            }
        }
    }

    #[test]
    fn sqlfluff_st007_cases_are_fixed_or_unchanged() {
        let cases = [
            (
                "SELECT * FROM a JOIN b USING (id)",
                1,
                0,
                1,
                Some("ON A.ID = B.ID"),
            ),
            (
                "SELECT * FROM a AS x JOIN b AS y USING (id)",
                1,
                0,
                1,
                Some("ON X.ID = Y.ID"),
            ),
            (
                "SELECT * FROM a JOIN b USING (id, tenant_id)",
                1,
                0,
                1,
                Some("ON A.ID = B.ID AND A.TENANT_ID = B.TENANT_ID"),
            ),
            ("SELECT * FROM a JOIN b ON a.id = b.id", 0, 0, 0, None),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_ST_007, before, after, fix_count);

            if let Some(expected) = expected_text {
                let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
                assert!(
                    out.sql.to_ascii_uppercase().contains(expected),
                    "expected {expected:?} in fixed SQL, got: {}",
                    out.sql
                );
            }
        }
    }

    #[test]
    fn sqlfluff_st004_cases_are_fixed_or_unchanged() {
        let cases = [
            (
                "SELECT CASE WHEN species = 'Rat' THEN 'Squeak' ELSE CASE WHEN species = 'Dog' THEN 'Woof' END END AS sound FROM mytable",
                1,
                0,
                1,
                Some("WHEN 'DOG' THEN 'WOOF'"),
            ),
            (
                "SELECT CASE WHEN species = 'Rat' THEN 'Squeak' ELSE CASE WHEN species = 'Dog' THEN 'Woof' WHEN species = 'Mouse' THEN 'Squeak' ELSE 'Other' END END AS sound FROM mytable",
                1,
                0,
                1,
                Some("WHEN 'MOUSE' THEN 'SQUEAK' ELSE 'OTHER' END"),
            ),
            (
                "SELECT CASE WHEN species = 'Rat' THEN CASE WHEN colour = 'Black' THEN 'Growl' WHEN colour = 'Grey' THEN 'Squeak' END END AS sound FROM mytable",
                0,
                0,
                0,
                None,
            ),
            (
                "SELECT CASE WHEN day_of_month IN (11, 12, 13) THEN 'TH' ELSE CASE MOD(day_of_month, 10) WHEN 1 THEN 'ST' WHEN 2 THEN 'ND' WHEN 3 THEN 'RD' ELSE 'TH' END END AS ordinal_suffix FROM calendar",
                0,
                0,
                0,
                None,
            ),
            (
                "SELECT CASE x WHEN 0 THEN 'zero' WHEN 5 THEN 'five' ELSE CASE x WHEN 10 THEN 'ten' WHEN 20 THEN 'twenty' ELSE 'other' END END FROM tab_a",
                1,
                0,
                1,
                Some("WHEN 20 THEN 'TWENTY' ELSE 'OTHER' END"),
            ),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_ST_004, before, after, fix_count);

            if let Some(expected) = expected_text {
                let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
                assert!(
                    out.sql.to_ascii_uppercase().contains(expected),
                    "expected {expected:?} in fixed SQL, got: {}",
                    out.sql
                );
            }
        }
    }

    #[test]
    fn sqlfluff_cv003_cases_are_fixed_or_unchanged() {
        let cases = [
            ("SELECT a FROM foo WHERE a IS NULL", 0, 0, 0, None),
            ("SELECT a FROM foo WHERE a IS NOT NULL", 0, 0, 0, None),
            (
                "SELECT a FROM foo WHERE a <> NULL",
                1,
                0,
                1,
                Some("WHERE A IS NOT NULL"),
            ),
            (
                "SELECT a FROM foo WHERE a <> NULL AND b != NULL AND c = 'foo'",
                2,
                0,
                2,
                Some("A IS NOT NULL AND B IS NOT NULL"),
            ),
            (
                "SELECT a FROM foo WHERE a = NULL",
                1,
                0,
                1,
                Some("WHERE A IS NULL"),
            ),
            (
                "SELECT a FROM foo WHERE a=NULL",
                1,
                0,
                1,
                Some("WHERE A IS NULL"),
            ),
            (
                "SELECT a FROM foo WHERE a = b OR (c > d OR e = NULL)",
                1,
                0,
                1,
                Some("OR E IS NULL"),
            ),
            ("UPDATE table1 SET col = NULL WHERE col = ''", 0, 0, 0, None),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_CV_005, before, after, fix_count);

            if let Some(expected) = expected_text {
                let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
                assert!(
                    out.sql.to_ascii_uppercase().contains(expected),
                    "expected {expected:?} in fixed SQL, got: {}",
                    out.sql
                );
            }
        }
    }

    #[test]
    fn sqlfluff_cv001_cases_are_fixed_or_unchanged() {
        let cases = [
            ("SELECT coalesce(foo, 0) AS bar FROM baz", 0, 0, 0),
            ("SELECT ifnull(foo, 0) AS bar FROM baz", 1, 0, 1),
            ("SELECT nvl(foo, 0) AS bar FROM baz", 1, 0, 1),
            (
                "SELECT CASE WHEN x IS NULL THEN 'default' ELSE x END FROM t",
                0,
                0,
                0,
            ),
        ];

        for (sql, before, after, fix_count) in cases {
            assert_rule_case(sql, issue_codes::LINT_CV_002, before, after, fix_count);
        }
    }

    #[test]
    fn sqlfluff_cv008_cases_are_fixed_or_unchanged() {
        let cases = [
            (
                "SELECT * FROM a RIGHT JOIN b ON a.id = b.id",
                1,
                0,
                1,
                Some("FROM B LEFT JOIN"),
            ),
            (
                "SELECT a.id FROM a JOIN b ON a.id = b.id RIGHT JOIN c ON b.id = c.id",
                1,
                0,
                1,
                Some("FROM C LEFT JOIN"),
            ),
            (
                "SELECT a.id FROM a RIGHT JOIN b ON a.id = b.id RIGHT JOIN c ON b.id = c.id",
                2,
                0,
                2,
                Some("FROM C LEFT JOIN"),
            ),
            ("SELECT * FROM a LEFT JOIN b ON a.id = b.id", 0, 0, 0, None),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_CV_008, before, after, fix_count);

            if let Some(expected) = expected_text {
                let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
                assert!(
                    out.sql.to_ascii_uppercase().contains(expected),
                    "expected {expected:?} in fixed SQL, got: {}",
                    out.sql
                );
            }
        }
    }

    #[test]
    fn sqlfluff_st002_cases_are_fixed_or_unchanged() {
        let cases = [
            ("SELECT CASE WHEN x > 1 THEN 'a' ELSE NULL END FROM t", 1, 0, 1),
            (
                "SELECT CASE name WHEN 'cat' THEN 'meow' WHEN 'dog' THEN 'woof' ELSE NULL END FROM t",
                1,
                0,
                1,
            ),
            (
                "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' WHEN x = 3 THEN 'c' ELSE NULL END FROM t",
                1,
                0,
                1,
            ),
            (
                "SELECT CASE WHEN x > 0 THEN CASE WHEN y > 0 THEN 'pos' ELSE NULL END ELSE NULL END FROM t",
                2,
                0,
                2,
            ),
            (
                "SELECT * FROM t WHERE (CASE WHEN x > 0 THEN 1 ELSE NULL END) IS NOT NULL",
                1,
                0,
                1,
            ),
            (
                "WITH cte AS (SELECT CASE WHEN x > 0 THEN 'yes' ELSE NULL END AS flag FROM t) SELECT * FROM cte",
                1,
                0,
                1,
            ),
            ("SELECT CASE WHEN x > 1 THEN 'a' END FROM t", 0, 0, 0),
            (
                "SELECT CASE name WHEN 'cat' THEN 'meow' ELSE UPPER(name) END FROM t",
                0,
                0,
                0,
            ),
            ("SELECT CASE WHEN x > 1 THEN 'a' ELSE 'b' END FROM t", 0, 0, 0),
        ];

        for (sql, before, after, fix_count) in cases {
            assert_rule_case(sql, issue_codes::LINT_ST_001, before, after, fix_count);
        }
    }

    #[test]
    fn count_style_cases_are_fixed_or_unchanged() {
        let cases = [
            ("SELECT COUNT(1) FROM t", 1, 0, 1),
            (
                "SELECT col FROM t GROUP BY col HAVING COUNT(1) > 5",
                1,
                0,
                1,
            ),
            (
                "SELECT * FROM t WHERE id IN (SELECT COUNT(1) FROM t2 GROUP BY col)",
                1,
                0,
                1,
            ),
            ("SELECT COUNT(1), COUNT(1) FROM t", 2, 0, 2),
            (
                "WITH cte AS (SELECT COUNT(1) AS cnt FROM t) SELECT * FROM cte",
                1,
                0,
                1,
            ),
            ("SELECT COUNT(*) FROM t", 0, 0, 0),
            ("SELECT COUNT(id) FROM t", 0, 0, 0),
            ("SELECT COUNT(0) FROM t", 1, 0, 1),
            ("SELECT COUNT(DISTINCT id) FROM t", 0, 0, 0),
        ];

        for (sql, before, after, fix_count) in cases {
            assert_rule_case(sql, issue_codes::LINT_CV_004, before, after, fix_count);
        }
    }

    #[test]
    fn skips_files_with_comments() {
        let sql = "-- keep this comment\nSELECT COUNT(1) FROM t";
        let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
        assert!(!out.changed);
        assert!(out.skipped_due_to_comments);
        assert_eq!(out.sql, sql);
    }

    #[test]
    fn skips_files_with_mysql_hash_comments() {
        let sql = "# keep this comment\nSELECT COUNT(1) FROM t";
        let out = apply_lint_fixes(sql, Dialect::Mysql, &[]).expect("fix result");
        assert!(!out.changed);
        assert!(out.skipped_due_to_comments);
        assert_eq!(out.sql, sql);
    }

    #[test]
    fn does_not_collapse_independent_select_statements() {
        let sql = "SELECT 1; SELECT 2;";
        let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
        assert!(
            !out.sql.to_ascii_uppercase().contains("DISTINCT SELECT"),
            "auto-fix must preserve statement boundaries: {}",
            out.sql
        );
        let parsed = parse_sql_with_dialect(&out.sql, Dialect::Generic).expect("parse fixed sql");
        assert_eq!(
            parsed.len(),
            2,
            "auto-fix should preserve two independent statements"
        );
    }

    #[test]
    fn subquery_to_cte_text_fix_applies() {
        let fixed = fix_subquery_to_cte("SELECT * FROM (SELECT 1) sub");
        assert_eq!(fixed, "WITH sub AS (SELECT 1) SELECT * FROM sub");
    }

    #[test]
    fn text_fix_pipeline_converts_subquery_to_cte() {
        let fixed = apply_text_fixes("SELECT * FROM (SELECT 1) sub", &RuleFilter::default());
        assert!(
            fixed.to_ascii_uppercase().contains("WITH SUB AS"),
            "expected CTE rewrite, got: {fixed}"
        );
    }

    #[test]
    fn distinct_parentheses_fix_preserves_valid_sql() {
        let sql = "SELECT DISTINCT(a) FROM t";
        let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
        assert!(
            !out.sql.contains("a)"),
            "unexpected dangling parenthesis after fix: {}",
            out.sql
        );
        parse_sql_with_dialect(&out.sql, Dialect::Generic).expect("fixed SQL should parse");
    }

    #[test]
    fn not_equal_fix_does_not_rewrite_string_literals() {
        let sql = "SELECT '<>' AS x, a<>b, c!=d FROM t";
        let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
        assert!(
            out.sql.contains("'<>'"),
            "string literal should remain unchanged: {}",
            out.sql
        );
        assert!(
            out.sql.contains("a != b"),
            "operator usage should still be normalized: {}",
            out.sql
        );
    }

    #[test]
    fn case_style_fix_does_not_rewrite_double_quoted_identifiers() {
        let fixed = fix_case_style_consistency("SELECT \"FROM\", \"CamelCase\" FROM t");
        assert!(
            fixed.contains("\"FROM\""),
            "keyword-like quoted identifier should remain unchanged: {fixed}"
        );
        assert!(
            fixed.contains("\"CamelCase\""),
            "case-sensitive quoted identifier should remain unchanged: {fixed}"
        );
    }

    #[test]
    fn spacing_fixes_do_not_rewrite_single_quoted_literals() {
        let operator_fixed = fix_operator_spacing("SELECT a=1, 'x=y' FROM t");
        assert!(
            operator_fixed.contains("'x=y'"),
            "operator spacing must not mutate literals: {operator_fixed}"
        );
        assert!(
            operator_fixed.contains("a = 1"),
            "operator spacing should still apply: {operator_fixed}"
        );

        let comma_fixed = fix_comma_spacing("SELECT a,b, 'x,y' FROM t");
        assert!(
            comma_fixed.contains("'x,y'"),
            "comma spacing must not mutate literals: {comma_fixed}"
        );
        assert!(
            comma_fixed.contains("a, b"),
            "comma spacing should still apply: {comma_fixed}"
        );
    }

    #[test]
    fn keyword_newline_fix_does_not_rewrite_literals_or_quoted_identifiers() {
        let sql = "SELECT COUNT(1), 'hello FROM world', \"x WHERE y\" FROM t WHERE a = 1";
        let fixed = fix_keyword_newlines(sql);
        assert!(
            fixed.contains("'hello FROM world'"),
            "single-quoted literal should remain unchanged: {fixed}"
        );
        assert!(
            fixed.contains("\"x WHERE y\""),
            "double-quoted identifier should remain unchanged: {fixed}"
        );
        assert!(
            !fixed.contains("hello\nFROM world"),
            "keyword newline fix must not inject newlines into literals: {fixed}"
        );
        assert!(
            fixed.contains("\nFROM t"),
            "FROM clause should still be normalized: {fixed}"
        );
        assert!(
            fixed.contains("\nWHERE a = 1"),
            "WHERE clause should still be normalized: {fixed}"
        );
    }

    #[test]
    fn alias_keyword_fix_respects_rf_004_rule_filter() {
        let sql = "select a from users as select";

        let rf_disabled = RuleFilter::new(&[
            issue_codes::LINT_LT_014.to_string(),
            issue_codes::LINT_RF_004.to_string(),
        ]);
        let out_rf_disabled = apply_text_fixes(sql, &rf_disabled);
        assert_eq!(
            out_rf_disabled, sql,
            "excluding RF_004 should block alias-keyword rewrite"
        );

        let al_disabled = RuleFilter::new(&[
            issue_codes::LINT_LT_014.to_string(),
            issue_codes::LINT_AL_005.to_string(),
        ]);
        let out_al_disabled = apply_text_fixes(sql, &al_disabled);
        assert!(
            out_al_disabled.contains("alias_select"),
            "excluding AL_005 must not block RF_004 rewrite: {out_al_disabled}"
        );
    }

    #[test]
    fn excluded_rule_is_not_rewritten_when_other_rules_are_fixed() {
        let sql = "SELECT COUNT(1) FROM t WHERE a<>b";
        let disabled = vec![issue_codes::LINT_CV_001.to_string()];
        let out = apply_lint_fixes(sql, Dialect::Generic, &disabled).expect("fix result");
        assert!(
            out.sql.contains("COUNT(*)"),
            "expected COUNT style fix: {}",
            out.sql
        );
        assert!(
            out.sql.contains("<>"),
            "excluded CV_005 should remain '<>' (not '!='): {}",
            out.sql
        );
        assert!(
            !out.sql.contains("!="),
            "excluded CV_005 should not be rewritten to '!=': {}",
            out.sql
        );
    }

    #[test]
    fn references_quoting_fix_keeps_reserved_identifier_quotes() {
        let sql = "SELECT \"FROM\" FROM t UNION SELECT 2";
        let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
        assert!(
            out.sql.contains("\"FROM\""),
            "reserved identifier must remain quoted: {}",
            out.sql
        );
        assert!(
            out.sql.to_ascii_uppercase().contains("DISTINCT SELECT"),
            "expected another fix to persist output: {}",
            out.sql
        );
    }

    #[test]
    fn references_quoting_fix_keeps_case_sensitive_identifier_quotes() {
        let sql = "SELECT \"CamelCase\" FROM t UNION SELECT 2";
        let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
        assert!(
            out.sql.contains("\"CamelCase\""),
            "case-sensitive identifier must remain quoted: {}",
            out.sql
        );
        assert!(
            out.sql.to_ascii_uppercase().contains("DISTINCT SELECT"),
            "expected another fix to persist output: {}",
            out.sql
        );
    }

    #[test]
    fn sqlfluff_fix_rule_smoke_cases_reduce_target_violations() {
        let cases = vec![
            (
                issue_codes::LINT_AL_001,
                "SELECT * FROM a x JOIN b y ON x.id = y.id",
            ),
            (
                issue_codes::LINT_AL_005,
                "SELECT u.name FROM users u JOIN orders o ON users.id = orders.user_id",
            ),
            (issue_codes::LINT_AL_009, "SELECT a AS a FROM t"),
            (issue_codes::LINT_AM_002, "SELECT 1 UNION SELECT 2"),
            (
                issue_codes::LINT_AM_003,
                "SELECT * FROM t ORDER BY a, b DESC",
            ),
            (
                issue_codes::LINT_AM_005,
                "SELECT * FROM a JOIN b ON a.id = b.id",
            ),
            (
                issue_codes::LINT_AM_008,
                "SELECT foo.a, bar.b FROM foo INNER JOIN bar",
            ),
            (issue_codes::LINT_CP_001, "SELECT a from t"),
            (issue_codes::LINT_CP_004, "SELECT NULL, true FROM t"),
            (
                issue_codes::LINT_CP_005,
                "CREATE TABLE t (a INT, b varchar(10))",
            ),
            (
                issue_codes::LINT_CV_001,
                "SELECT * FROM t WHERE a <> b AND c != d",
            ),
            (
                issue_codes::LINT_CV_002,
                "SELECT IFNULL(x, 'default') FROM t",
            ),
            (issue_codes::LINT_CV_003, "SELECT a, FROM t"),
            (issue_codes::LINT_CV_004, "SELECT COUNT(1) FROM t"),
            (issue_codes::LINT_CV_005, "SELECT * FROM t WHERE a = NULL"),
            (issue_codes::LINT_CV_007, "(SELECT 1)"),
            (issue_codes::LINT_JJ_001, "SELECT '{{foo}}' AS templated"),
            (issue_codes::LINT_LT_001, "SELECT payload->>'id' FROM t"),
            (issue_codes::LINT_LT_002, "SELECT a\n   , b\nFROM t"),
            (issue_codes::LINT_LT_003, "SELECT a +\n b FROM t"),
            (issue_codes::LINT_LT_004, "SELECT a,b FROM t"),
            (issue_codes::LINT_LT_006, "SELECT COUNT (1) FROM t"),
            (
                issue_codes::LINT_LT_007,
                "SELECT 'WITH cte AS SELECT 1' AS sql_snippet",
            ),
            (issue_codes::LINT_LT_009, "SELECT a,b,c,d,e FROM t"),
            (issue_codes::LINT_LT_010, "SELECT\nDISTINCT a\nFROM t"),
            (
                issue_codes::LINT_LT_011,
                "SELECT 1 UNION SELECT 2\nUNION SELECT 3",
            ),
            (issue_codes::LINT_LT_012, "SELECT 1\nFROM t"),
            (issue_codes::LINT_LT_013, "\n\nSELECT 1"),
            (issue_codes::LINT_LT_014, "SELECT a FROM t\nWHERE a=1"),
            (issue_codes::LINT_LT_015, "SELECT 1\n\n\nFROM t"),
            (issue_codes::LINT_RF_003, "SELECT a.id, id2 FROM a"),
            (issue_codes::LINT_RF_006, "SELECT \"good_name\" FROM t"),
            (
                issue_codes::LINT_ST_001,
                "SELECT CASE WHEN x > 1 THEN 'a' ELSE NULL END FROM t",
            ),
            (
                issue_codes::LINT_ST_004,
                "SELECT CASE WHEN species = 'Rat' THEN 'Squeak' ELSE CASE WHEN species = 'Dog' THEN 'Woof' END END FROM mytable",
            ),
            (
                issue_codes::LINT_ST_002,
                "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' END FROM t",
            ),
            (
                issue_codes::LINT_ST_005,
                "SELECT * FROM t JOIN (SELECT * FROM u) sub ON t.id = sub.id",
            ),
            (issue_codes::LINT_ST_006, "SELECT a + 1, a FROM t"),
            (
                issue_codes::LINT_ST_007,
                "SELECT * FROM a JOIN b USING (id)",
            ),
            (issue_codes::LINT_ST_008, "SELECT DISTINCT(a) FROM t"),
            (
                issue_codes::LINT_ST_009,
                "SELECT * FROM a x JOIN b y ON y.id = x.id",
            ),
            (issue_codes::LINT_ST_012, "SELECT 1;;"),
        ];

        for (code, sql) in cases {
            let before = lint_rule_count(sql, code);
            assert!(before > 0, "expected {code} to trigger before fix: {sql}");
            let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
            assert!(
                !out.skipped_due_to_comments,
                "test SQL should not be skipped: {sql}"
            );
            let after = lint_rule_count(&out.sql, code);
            assert!(
                after < before || out.sql != sql,
                "expected {code} count to decrease or SQL to be rewritten. before={before} after={after}\ninput={sql}\noutput={}",
                out.sql
            );
        }
    }
}
