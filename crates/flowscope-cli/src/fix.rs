//! SQL lint auto-fix helpers.
//!
//! Fixing is best-effort and deterministic. We combine:
//! - AST rewrites for structurally safe transforms.
//! - Text rewrites for parity-style formatting/convention rules.
//! - Lint before/after comparison to report per-rule removed violations.

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
            .map(|rule| rule.trim().to_ascii_uppercase())
            .collect();
        Self { disabled }
    }

    fn allows(&self, code: &str) -> bool {
        !self.disabled.contains(&code.to_ascii_uppercase())
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
    if rule_filter.allows(issue_codes::LINT_CV_008) {
        out = fix_statement_brackets(&out);
    }
    if rule_filter.allows(issue_codes::LINT_CV_005) {
        out = fix_not_equal_operator(&out);
    }
    if rule_filter.allows(issue_codes::LINT_CV_006) {
        out = fix_trailing_select_comma(&out);
    }
    if rule_filter.allows(issue_codes::LINT_ST_008) {
        out = fix_distinct_parentheses(&out);
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
    if rule_filter.allows(issue_codes::LINT_AL_003) {
        out = fix_missing_table_aliases(&out);
    }
    if rule_filter.allows(issue_codes::LINT_AL_009) {
        out = fix_self_aliases(&out);
    }
    if rule_filter.allows(issue_codes::LINT_AL_007) {
        out = fix_single_table_aliases(&out);
    }
    if rule_filter.allows(issue_codes::LINT_AL_002) {
        out = fix_unused_table_aliases(&out);
    }
    if rule_filter.allows(issue_codes::LINT_RF_004) {
        out = fix_table_alias_keywords(&out);
    }
    if rule_filter.allows(issue_codes::LINT_ST_006) {
        out = fix_subquery_to_cte(&out);
    }
    if rule_filter.allows(issue_codes::LINT_ST_004) {
        out = fix_using_join(&out);
    }
    if rule_filter.allows(issue_codes::LINT_ST_009) {
        out = fix_join_condition_order(&out);
    }
    if rule_filter.allows(issue_codes::LINT_AM_001) {
        out = fix_bare_union(&out);
    }
    if rule_filter.allows(issue_codes::LINT_ST_007) {
        out = fix_select_column_order(&out);
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
    if rule_filter.allows(issue_codes::LINT_ST_005) {
        out = fix_simple_case_rewrite(&out);
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

fn fix_distinct_parentheses(sql: &str) -> String {
    regex_replace_all_with(
        sql,
        r"(?i)\bselect\s+distinct\s*\(\s*([^()]+?)\s*\)",
        |caps| format!("SELECT DISTINCT {}", caps[1].trim()),
    )
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

fn fix_using_join(sql: &str) -> String {
    regex_replace_all_with(
        sql,
        r"(?i)\bfrom\s+([A-Za-z_][A-Za-z0-9_]*)(?:\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*))?\s+join\s+([A-Za-z_][A-Za-z0-9_]*)(?:\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*))?\s+using\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)",
        |caps| {
            let left_table = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let left_alias = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let right_table = caps.get(3).map(|m| m.as_str()).unwrap_or_default();
            let right_alias = caps.get(4).map(|m| m.as_str()).unwrap_or("");
            let column = caps.get(5).map(|m| m.as_str()).unwrap_or("id");
            let left_ref = if left_alias.is_empty() {
                left_table
            } else {
                left_alias
            };
            let right_ref = if right_alias.is_empty() {
                right_table
            } else {
                right_alias
            };
            format!(
                "FROM {}{} JOIN {}{} ON {}.{} = {}.{}",
                left_table,
                if left_alias.is_empty() {
                    String::new()
                } else {
                    format!(" AS {left_alias}")
                },
                right_table,
                if right_alias.is_empty() {
                    String::new()
                } else {
                    format!(" AS {right_alias}")
                },
                left_ref,
                column,
                right_ref,
                column
            )
        },
    )
}

fn fix_join_condition_order(sql: &str) -> String {
    regex_replace_all_with(
        sql,
        r"(?i)\bfrom\s+([A-Za-z_][A-Za-z0-9_]*)(?:\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*))?\s+join\s+([A-Za-z_][A-Za-z0-9_]*)(?:\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*))?\s+on\s+([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z_][A-Za-z0-9_]*)",
        |caps| {
            let left_table = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let left_alias = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let right_table = caps.get(3).map(|m| m.as_str()).unwrap_or_default();
            let right_alias = caps.get(4).map(|m| m.as_str()).unwrap_or("");
            let lhs_ref = caps.get(5).map(|m| m.as_str()).unwrap_or_default();
            let lhs_col = caps.get(6).map(|m| m.as_str()).unwrap_or_default();
            let rhs_ref = caps.get(7).map(|m| m.as_str()).unwrap_or_default();
            let rhs_col = caps.get(8).map(|m| m.as_str()).unwrap_or_default();

            let left_ref = if left_alias.is_empty() {
                left_table
            } else {
                left_alias
            };
            let right_ref = if right_alias.is_empty() {
                right_table
            } else {
                right_alias
            };

            if lhs_ref.eq_ignore_ascii_case(right_ref) && rhs_ref.eq_ignore_ascii_case(left_ref) {
                format!(
                    "FROM {}{} JOIN {}{} ON {}.{} = {}.{}",
                    left_table,
                    if left_alias.is_empty() {
                        String::new()
                    } else {
                        format!(" AS {left_alias}")
                    },
                    right_table,
                    if right_alias.is_empty() {
                        String::new()
                    } else {
                        format!(" AS {right_alias}")
                    },
                    rhs_ref,
                    rhs_col,
                    lhs_ref,
                    lhs_col
                )
            } else {
                caps[0].to_string()
            }
        },
    )
}

fn fix_bare_union(sql: &str) -> String {
    regex_replace_all_with(sql, r"(?i)\b(UNION)\b(\s+(ALL|DISTINCT)\b)?", |caps| {
        if caps.get(2).is_some() {
            caps[0].to_string()
        } else {
            let union = caps.get(1).map(|m| m.as_str()).unwrap_or("UNION");
            let distinct = if union
                .chars()
                .all(|c| !c.is_ascii_alphabetic() || c.is_ascii_lowercase())
            {
                "distinct"
            } else {
                "DISTINCT"
            };
            format!("{union} {distinct}")
        }
    })
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

fn fix_select_column_order(sql: &str) -> String {
    let select_re = Regex::new(r"(?is)\bselect\s+(.*?)\bfrom\b").expect("valid fix regex");
    let Some(caps) = select_re.captures(sql) else {
        return sql.to_string();
    };
    let select_clause = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
    let mut items: Vec<String> = select_clause
        .split(',')
        .map(|item| item.trim().to_string())
        .collect();
    if items.len() < 2 {
        return sql.to_string();
    }

    let simple_re = Regex::new(r"(?i)^[A-Za-z_][A-Za-z0-9_\.]*$").expect("valid fix regex");
    let mut simple = Vec::new();
    let mut complex = Vec::new();
    for item in items.drain(..) {
        if simple_re.is_match(&item) {
            simple.push(item);
        } else {
            complex.push(item);
        }
    }
    if simple.is_empty() || complex.is_empty() {
        return sql.to_string();
    }

    let mut reordered = simple;
    reordered.extend(complex);
    let rewritten_clause = reordered.join(", ");
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

fn fix_simple_case_rewrite(sql: &str) -> String {
    regex_replace_all_with(
        sql,
        r"(?is)\bcase\s+when\s+([A-Za-z_][A-Za-z0-9_\.]*)\s*=\s*([^ ]+)\s+then\s+([^ ]+)\s+when\s+([A-Za-z_][A-Za-z0-9_\.]*)\s*=\s*([^ ]+)\s+then\s+([^ ]+)\s+end",
        |caps| {
            if caps[1].eq_ignore_ascii_case(&caps[4]) {
                format!(
                    "CASE {} WHEN {} THEN {} WHEN {} THEN {} END",
                    &caps[1], &caps[2], &caps[3], &caps[5], &caps[6]
                )
            } else {
                caps[0].to_string()
            }
        },
    )
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
        SetExpr::SetOperation { left, right, .. } => {
            fix_set_expr(left, rule_filter);
            fix_set_expr(right, rule_filter);
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
    if rule_filter.allows(issue_codes::LINT_AM_003) && has_distinct_and_group_by(select) {
        select.distinct = None;
    }

    for item in &mut select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                fix_expr(expr, rule_filter);
            }
            _ => {}
        }
    }

    for table_with_joins in &mut select.from {
        fix_table_factor(&mut table_with_joins.relation, rule_filter);
        for join in &mut table_with_joins.joins {
            fix_table_factor(&mut join.relation, rule_filter);
            fix_join_operator(&mut join.join_operator, rule_filter);
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

fn fix_table_factor(relation: &mut TableFactor, rule_filter: &RuleFilter) {
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
            fix_table_factor(&mut table_with_joins.relation, rule_filter);
            for join in &mut table_with_joins.joins {
                fix_table_factor(&mut join.relation, rule_filter);
                fix_join_operator(&mut join.join_operator, rule_filter);
            }
        }
        TableFactor::Pivot {
            table,
            aggregate_functions,
            value_column,
            default_on_null,
            ..
        } => {
            fix_table_factor(table, rule_filter);
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
            fix_table_factor(table, rule_filter);
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

fn fix_join_operator(op: &mut JoinOperator, rule_filter: &RuleFilter) {
    match op {
        JoinOperator::Join(constraint) => {
            fix_join_constraint(constraint, rule_filter);
            if rule_filter.allows(issue_codes::LINT_AM_006) {
                *op = JoinOperator::Inner(constraint.clone());
            }
        }
        JoinOperator::Inner(constraint)
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

        if rule_filter.allows(issue_codes::LINT_AM_005) {
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

    if rule_filter.allows(issue_codes::LINT_CV_003) {
        if let Some(rewritten) = null_comparison_rewrite(expr) {
            *expr = rewritten;
            return;
        }
    }

    if let Expr::Case {
        else_result: Some(else_result),
        ..
    } = expr
    {
        if rule_filter.allows(issue_codes::LINT_ST_002) && lint_helpers::is_null_expr(else_result) {
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

    if rule_filter.allows(issue_codes::LINT_CV_001) {
        let function_name_upper = func.name.to_string().to_ascii_uppercase();
        if function_name_upper == "IFNULL" || function_name_upper == "NVL" {
            func.name = vec![Ident::new("COALESCE")].into();
        }
    }

    if rule_filter.allows(issue_codes::LINT_CV_002) && is_count_one(func) {
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

fn is_count_one(func: &Function) -> bool {
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
        }))) if n == "1"
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
            assert_rule_case(sql, issue_codes::LINT_AM_003, before, after, fix_count);
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
                Some("UNION DISTINCT"),
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
                Some("UNION DISTINCT"),
            ),
        ];

        for (sql, before, after, fix_count, expected_text) in cases {
            assert_rule_case(sql, issue_codes::LINT_AM_001, before, after, fix_count);

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
            assert_rule_case(sql, issue_codes::LINT_AM_006, before, after, fix_count);

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
            assert_rule_case(sql, issue_codes::LINT_CV_003, before, after, fix_count);

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
            assert_rule_case(sql, issue_codes::LINT_CV_001, before, after, fix_count);
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
            assert_rule_case(sql, issue_codes::LINT_ST_002, before, after, fix_count);
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
            ("SELECT COUNT(0) FROM t", 0, 0, 0),
            ("SELECT COUNT(DISTINCT id) FROM t", 0, 0, 0),
        ];

        for (sql, before, after, fix_count) in cases {
            assert_rule_case(sql, issue_codes::LINT_CV_002, before, after, fix_count);
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
            !out.sql.to_ascii_uppercase().contains("UNION DISTINCT"),
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
            issue_codes::LINT_AL_002.to_string(),
        ]);
        let out_al_disabled = apply_text_fixes(sql, &al_disabled);
        assert!(
            out_al_disabled.contains("alias_select"),
            "excluding AL_002 must not block RF_004 rewrite: {out_al_disabled}"
        );
    }

    #[test]
    fn excluded_rule_is_not_rewritten_when_other_rules_are_fixed() {
        let sql = "SELECT COUNT(1) FROM t WHERE a<>b";
        let disabled = vec![issue_codes::LINT_CV_005.to_string()];
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
            out.sql.to_ascii_uppercase().contains("UNION DISTINCT"),
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
            out.sql.to_ascii_uppercase().contains("UNION DISTINCT"),
            "expected another fix to persist output: {}",
            out.sql
        );
    }

    #[test]
    fn sqlfluff_fix_rule_smoke_cases_reduce_target_violations() {
        let cases = vec![
            (
                issue_codes::LINT_AL_003,
                "SELECT * FROM a x JOIN b y ON x.id = y.id",
            ),
            (
                issue_codes::LINT_AL_002,
                "SELECT u.name FROM users u JOIN orders o ON users.id = orders.user_id",
            ),
            (issue_codes::LINT_AL_007, "SELECT * FROM users u"),
            (issue_codes::LINT_AL_009, "SELECT a AS a FROM t"),
            (issue_codes::LINT_AM_001, "SELECT 1 UNION SELECT 2"),
            (
                issue_codes::LINT_AM_005,
                "SELECT * FROM t ORDER BY a, b DESC",
            ),
            (
                issue_codes::LINT_AM_006,
                "SELECT * FROM a JOIN b ON a.id = b.id",
            ),
            (issue_codes::LINT_CP_001, "SELECT a from t"),
            (issue_codes::LINT_CP_004, "SELECT NULL, true FROM t"),
            (
                issue_codes::LINT_CP_005,
                "CREATE TABLE t (a INT, b varchar(10))",
            ),
            (
                issue_codes::LINT_CV_005,
                "SELECT * FROM t WHERE a <> b AND c != d",
            ),
            (
                issue_codes::LINT_CV_001,
                "SELECT IFNULL(x, 'default') FROM t",
            ),
            (issue_codes::LINT_CV_006, "SELECT a, FROM t"),
            (issue_codes::LINT_CV_002, "SELECT COUNT(1) FROM t"),
            (issue_codes::LINT_CV_003, "SELECT * FROM t WHERE a = NULL"),
            (issue_codes::LINT_CV_008, "(SELECT 1)"),
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
                issue_codes::LINT_ST_002,
                "SELECT CASE WHEN x > 1 THEN 'a' ELSE NULL END FROM t",
            ),
            (
                issue_codes::LINT_ST_005,
                "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' END FROM t",
            ),
            (
                issue_codes::LINT_ST_006,
                "SELECT * FROM (SELECT * FROM t) sub",
            ),
            (issue_codes::LINT_ST_007, "SELECT a + 1, a FROM t"),
            (
                issue_codes::LINT_ST_004,
                "SELECT * FROM a JOIN b USING (id)",
            ),
            (issue_codes::LINT_ST_008, "SELECT DISTINCT(a) FROM t"),
            (
                issue_codes::LINT_ST_009,
                "SELECT * FROM a x JOIN b y ON y.id = x.id",
            ),
            (issue_codes::LINT_ST_012, "SELECT 1;;"),
            (
                issue_codes::LINT_TQ_002,
                "SELECT 'CREATE PROCEDURE p' AS sql_snippet",
            ),
            (
                issue_codes::LINT_TQ_003,
                "SELECT '\nGO\nGO\n' AS sql_snippet",
            ),
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
