//! LINT_CV_010: Consistent usage of preferred quotes for quoted literals.
//!
//! SQLFluff CV10 parity: detects inconsistent quoting of string literals in
//! dialects where both single and double quotes denote strings (BigQuery,
//! Databricks/SparkSQL, Hive, MySQL).  Applies Black-style quote
//! normalization for autofixes.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::Statement;
use std::ops::Range;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreferredStyle {
    Consistent,
    SingleQuotes,
    DoubleQuotes,
}

impl PreferredStyle {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_CV_010, "preferred_quoted_literal_style")
            .unwrap_or("consistent")
            .to_ascii_lowercase()
            .as_str()
        {
            "single_quotes" | "single" => Self::SingleQuotes,
            "double_quotes" | "double" => Self::DoubleQuotes,
            _ => Self::Consistent,
        }
    }
}

pub struct ConventionQuotedLiterals {
    preferred_style: PreferredStyle,
    force_enable: bool,
}

impl ConventionQuotedLiterals {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            preferred_style: PreferredStyle::from_config(config),
            force_enable: config
                .rule_option_bool(issue_codes::LINT_CV_010, "force_enable")
                .unwrap_or(false),
        }
    }

    /// Dialects where both single and double quotes denote string literals.
    fn is_double_quote_string_dialect(dialect: Dialect) -> bool {
        matches!(
            dialect,
            Dialect::Bigquery | Dialect::Databricks | Dialect::Hive | Dialect::Mysql
        )
    }
}

impl Default for ConventionQuotedLiterals {
    fn default() -> Self {
        Self {
            preferred_style: PreferredStyle::Consistent,
            force_enable: false,
        }
    }
}

// ---------------------------------------------------------------------------
// LintRule impl
// ---------------------------------------------------------------------------

impl BuiltinLintRule for ConventionQuotedLiterals {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_010
    }

    fn name(&self) -> &'static str {
        "Quoted literals style"
    }

    fn description(&self) -> &'static str {
        "Consistent usage of preferred quotes for quoted literals."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let dialect = ctx.dialect();
        if !self.force_enable && !Self::is_double_quote_string_dialect(dialect) {
            return Vec::new();
        }

        let sql = ctx.statement_sql();
        let masked_sql = contains_template_tags(sql).then(|| mask_templated_areas(sql));
        let scan_sql = masked_sql.as_deref().unwrap_or(sql);
        let literals = scan_string_literals(scan_sql);
        let template_ranges = template_tag_ranges(ctx.sql);
        if template_ranges.iter().any(|range| {
            ctx.statement_range.start >= range.start && ctx.statement_range.end <= range.end
        }) {
            return Vec::new();
        }

        if literals.is_empty() {
            return Vec::new();
        }

        // Determine effective preferred style.
        let preferred = match self.preferred_style {
            PreferredStyle::Consistent => {
                // Derive from the first literal's quote character.
                let first = &literals[0];
                if first.quote_char == '"' {
                    PreferredStyle::DoubleQuotes
                } else {
                    PreferredStyle::SingleQuotes
                }
            }
            other => other,
        };

        let (pref_char, alt_char) = match preferred {
            PreferredStyle::SingleQuotes => ('\'', '"'),
            PreferredStyle::DoubleQuotes => ('"', '\''),
            PreferredStyle::Consistent => unreachable!(),
        };

        let message = match preferred {
            PreferredStyle::SingleQuotes => "Use single quotes for quoted literals.",
            PreferredStyle::DoubleQuotes => "Use double quotes for quoted literals.",
            PreferredStyle::Consistent => unreachable!(),
        };

        let mut issues = Vec::new();
        for lit in &literals {
            let absolute_start = ctx.statement_range.start + lit.start;
            let absolute_end = ctx.statement_range.start + lit.end;
            if template_ranges
                .iter()
                .any(|range| absolute_start >= range.start && absolute_end <= range.end)
            {
                continue;
            }

            let replacement = normalize_literal(sql, lit, pref_char, alt_char);
            let mismatch = lit.quote_char != pref_char;
            let template_mismatch = mismatch && literal_contains_template(sql, lit);
            if replacement.is_none() && !template_mismatch {
                continue;
            }

            let mut issue = Issue::info(issue_codes::LINT_CV_010, message)
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(lit.start, lit.end));

            if let Some(replacement) = replacement {
                issue = issue.with_autofix_edits(
                    IssueAutofixApplicability::Safe,
                    vec![IssuePatchEdit::new(
                        ctx.span_from_statement_offset(lit.start, lit.end),
                        replacement,
                    )],
                );
            }

            issues.push(issue);
        }

        issues
    }
}

// ---------------------------------------------------------------------------
// String literal scanner
// ---------------------------------------------------------------------------

/// A string literal found in the raw SQL source.
#[derive(Debug)]
struct StringLiteral {
    /// Byte offset of the start of the literal (including any prefix).
    start: usize,
    /// Byte offset one past the end of the literal.
    end: usize,
    /// The quote character used: `'` or `"`.
    quote_char: char,
    /// Whether this is a triple-quoted string.
    is_triple: bool,
    /// Whether the literal has a prefix (r, b, R, B).
    prefix: Option<u8>,
}

/// Scan raw SQL for string literals (single-quoted, double-quoted, with
/// optional BigQuery prefixes).  Skips comments, dollar-quoted strings,
/// and date/time constructor strings (DATE'...', TIME'...', etc.).
fn scan_string_literals(sql: &str) -> Vec<StringLiteral> {
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut result = Vec::new();
    let mut i = 0;

    while i < len {
        // Skip Jinja templated tags.
        if let Some(close_marker) = template_close_marker_at(bytes, i) {
            i = skip_template_tag(bytes, i, close_marker);
            continue;
        }

        // Skip line comments.
        if i + 1 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' {
            i += 2;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        // Skip block comments.
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }

        // Skip dollar-quoted strings ($$...$$).
        if bytes[i] == b'$' && i + 1 < len && bytes[i + 1] == b'$' {
            i += 2;
            while i + 1 < len && !(bytes[i] == b'$' && bytes[i + 1] == b'$') {
                i += 1;
            }
            i += 2;
            continue;
        }

        // Check for date/time constructor keywords preceding a quote.
        // E.g. DATE'...', TIME'...', TIMESTAMP'...', DATETIME'...'
        // These are SQL typed literals, not normal string literals.
        if (bytes[i] == b'\'' || bytes[i] == b'"') && is_preceded_by_type_keyword(sql, i) {
            // Skip the typed literal.
            let q = bytes[i];
            i += 1;
            while i < len && bytes[i] != q {
                if bytes[i] == b'\\' && i + 1 < len {
                    i += 1;
                }
                i += 1;
            }
            if i < len {
                i += 1; // skip closing quote
            }
            continue;
        }

        // Check for prefix characters (r, b, R, B) before a quote.
        let prefix: Option<u8>;
        let quote_start: usize;
        if (bytes[i] == b'r' || bytes[i] == b'R' || bytes[i] == b'b' || bytes[i] == b'B')
            && i + 1 < len
            && (bytes[i + 1] == b'\'' || bytes[i + 1] == b'"')
        {
            // Make sure this isn't part of an identifier.
            if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_') {
                i += 1;
                continue;
            }
            prefix = Some(bytes[i]);
            quote_start = i + 1;
        } else if bytes[i] == b'\'' || bytes[i] == b'"' {
            prefix = None;
            quote_start = i;
        } else {
            i += 1;
            continue;
        }

        let q = bytes[quote_start];
        let literal_start = if prefix.is_some() { i } else { quote_start };

        // Check for triple quote.
        let is_triple =
            quote_start + 2 < len && bytes[quote_start + 1] == q && bytes[quote_start + 2] == q;

        if is_triple {
            let mut j = quote_start + 3;
            loop {
                if j + 2 >= len {
                    // Unterminated triple quote -- skip.
                    i = len;
                    break;
                }
                if let Some(close_marker) = template_close_marker_at(bytes, j) {
                    j = skip_template_tag(bytes, j, close_marker);
                    continue;
                }
                if bytes[j] == q && bytes[j + 1] == q && bytes[j + 2] == q {
                    let end = j + 3;
                    result.push(StringLiteral {
                        start: literal_start,
                        end,
                        quote_char: q as char,
                        is_triple: true,
                        prefix,
                    });
                    i = end;
                    break;
                }
                if bytes[j] == b'\\' {
                    j += 1; // skip escaped char
                }
                j += 1;
            }
        } else {
            // Single-quoted literal.
            let mut j = quote_start + 1;
            while j < len {
                if let Some(close_marker) = template_close_marker_at(bytes, j) {
                    j = skip_template_tag(bytes, j, close_marker);
                    continue;
                }
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == q {
                    // Check for escaped quote ('' or "").
                    if j + 1 < len && bytes[j + 1] == q {
                        j += 2;
                        continue;
                    }
                    break;
                }
                j += 1;
            }
            if j >= len {
                // Unterminated string, skip.
                i = len;
                continue;
            }
            let end = j + 1;
            result.push(StringLiteral {
                start: literal_start,
                end,
                quote_char: q as char,
                is_triple: false,
                prefix,
            });
            i = end;
        }
    }

    result
}

fn literal_contains_template(sql: &str, lit: &StringLiteral) -> bool {
    let raw = &sql[lit.start..lit.end];
    raw.contains("{{") || raw.contains("{%") || raw.contains("{#")
}

fn contains_template_tags(sql: &str) -> bool {
    sql.contains("{{") || sql.contains("{%") || sql.contains("{#")
}

fn mask_templated_areas(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut index = 0usize;

    while let Some((open_index, close_marker)) = find_next_template_open(sql, index) {
        out.push_str(&sql[index..open_index]);
        let marker_start = open_index + 2;
        if let Some(close_offset) = sql[marker_start..].find(close_marker) {
            let close_index = marker_start + close_offset + close_marker.len();
            out.push_str(&mask_non_newlines(&sql[open_index..close_index]));
            index = close_index;
        } else {
            out.push_str(&mask_non_newlines(&sql[open_index..]));
            return out;
        }
    }

    out.push_str(&sql[index..]);
    out
}

fn find_next_template_open(sql: &str, from: usize) -> Option<(usize, &'static str)> {
    let rest = sql.get(from..)?;
    let candidates = [("{{", "}}"), ("{%", "%}"), ("{#", "#}")];

    candidates
        .into_iter()
        .filter_map(|(open, close)| rest.find(open).map(|offset| (from + offset, close)))
        .min_by_key(|(index, _)| *index)
}

fn mask_non_newlines(segment: &str) -> String {
    segment
        .chars()
        .map(|ch| if ch == '\n' { '\n' } else { ' ' })
        .collect()
}

fn template_tag_ranges(sql: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut index = 0usize;

    while let Some((open_index, close_marker)) = find_next_template_open(sql, index) {
        let marker_start = open_index + 2;
        let end = if let Some(close_offset) = sql[marker_start..].find(close_marker) {
            marker_start + close_offset + close_marker.len()
        } else {
            sql.len()
        };
        ranges.push(open_index..end);
        index = end;
    }

    ranges
}

fn template_close_marker_at(bytes: &[u8], index: usize) -> Option<&'static [u8]> {
    if index + 1 >= bytes.len() || bytes[index] != b'{' {
        return None;
    }
    match bytes[index + 1] {
        b'{' => Some(b"}}"),
        b'%' => Some(b"%}"),
        b'#' => Some(b"#}"),
        _ => None,
    }
}

fn skip_template_tag(bytes: &[u8], start: usize, close_marker: &[u8]) -> usize {
    let mut i = start + 2; // skip opening "{x"
    while i + close_marker.len() <= bytes.len() {
        if bytes[i..].starts_with(close_marker) {
            return i + close_marker.len();
        }
        i += 1;
    }
    bytes.len()
}

/// Check if position `pos` is preceded by a type keyword like DATE, TIME,
/// TIMESTAMP, DATETIME, INTERVAL (ignoring case and optional whitespace).
fn is_preceded_by_type_keyword(sql: &str, pos: usize) -> bool {
    let before = &sql[..pos];
    let trimmed = before.trim_end();
    let lower = trimmed.to_ascii_lowercase();
    for kw in &["date", "time", "timestamp", "datetime", "interval"] {
        if lower.ends_with(kw) {
            // Make sure it's a complete keyword (not part of a longer identifier).
            let prefix_len = trimmed.len() - kw.len();
            if prefix_len == 0 {
                return true;
            }
            let prev_byte = trimmed.as_bytes()[prefix_len - 1];
            if !prev_byte.is_ascii_alphanumeric() && prev_byte != b'_' {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Quote normalization (Black-style)
// ---------------------------------------------------------------------------

/// Attempt to normalize a string literal to the preferred quote style.
/// Returns `Some(replacement)` if the literal should be changed, `None` if
/// it's already correct or conversion would increase escaping.
fn normalize_literal(
    sql: &str,
    lit: &StringLiteral,
    pref_char: char,
    alt_char: char,
) -> Option<String> {
    let raw = &sql[lit.start..lit.end];
    let prefix_str = match lit.prefix {
        Some(_) => &raw[..1],
        None => "",
    };
    let value_part = &raw[prefix_str.len()..]; // the part starting with quote(s)

    if lit.is_triple {
        let pref_triple = format!("{0}{0}{0}", pref_char);
        let alt_triple = format!("{0}{0}{0}", alt_char);

        if value_part.starts_with(&pref_triple) {
            // Already preferred triple -- nothing to do.
            return None;
        }

        if !value_part.starts_with(&alt_triple) {
            // Neither preferred nor alternate triple -- skip.
            return None;
        }

        // Body is between the triple quotes.
        let body = &value_part[3..value_part.len() - 3];

        // Converting triple quotes can require extra escaping; avoid fixes
        // that introduce escapes compared to the original.
        if body.ends_with(pref_char) {
            return None;
        }
        if body.contains(&pref_triple) {
            return None;
        }

        let result = format!("{}{}{}{}", prefix_str, pref_triple, body, pref_triple);
        if result == raw {
            return None;
        }
        return Some(result);
    }

    // Single-quoted literal.
    if value_part.starts_with(pref_char) {
        // Already preferred quote.  Check if we can remove unnecessary escapes.
        let body = &value_part[1..value_part.len() - 1];
        let new_body = remove_unnecessary_escapes(body, pref_char, alt_char);
        if new_body == body {
            return None;
        }
        let result = format!("{}{}{}{}", prefix_str, pref_char, new_body, pref_char);
        if result == raw {
            return None;
        }
        return Some(result);
    }

    if !value_part.starts_with(alt_char) {
        return None;
    }

    // Currently using alternate quotes.
    let body = &value_part[1..value_part.len() - 1];
    let is_raw = lit.prefix.map(|p| p == b'r' || p == b'R').unwrap_or(false);

    if is_raw {
        // Raw strings: do not modify the body.  Can only convert if the
        // body doesn't contain unescaped preferred quotes.
        if body.contains(pref_char) {
            // Check if ALL occurrences are escaped.
            let has_unescaped = has_unescaped_char(body, pref_char as u8);
            if has_unescaped {
                return None;
            }
        }
        let result = format!("{}{}{}{}", prefix_str, pref_char, body, pref_char);
        if result == raw {
            return None;
        }
        return Some(result);
    }

    // Non-raw: apply Black-style normalization.
    // 1. Remove unnecessary escapes of the new (preferred) quote.
    let body_cleaned = remove_unnecessary_escapes(body, alt_char, pref_char);

    // 2. Add escapes for unescaped preferred quotes, remove escapes for
    //    alternate quotes.
    let new_body = convert_quotes_in_body(&body_cleaned, pref_char as u8, alt_char as u8);

    // Compare escape counts.
    let orig_escapes = body_cleaned.matches('\\').count();
    let new_escapes = new_body.matches('\\').count();

    if new_escapes > orig_escapes {
        // Would introduce more escaping -- keep original but remove
        // unnecessary escapes.
        if body_cleaned != body {
            let result = format!("{}{}{}{}", prefix_str, alt_char, body_cleaned, alt_char);
            return Some(result);
        }
        return None;
    }

    let result = format!("{}{}{}{}", prefix_str, pref_char, new_body, pref_char);
    if result == raw {
        return None;
    }
    Some(result)
}

/// Remove unnecessary escapes of `other_char` in a body quoted with
/// `quote_char`.  E.g. in a double-quoted string, `\'` is unnecessary.
fn remove_unnecessary_escapes(body: &str, _quote_char: char, other_char: char) -> String {
    let bytes = body.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == other_char as u8 {
                // Check if this backslash is itself escaped.
                // Count preceding backslashes.
                let preceding_backslashes = count_preceding_backslashes(&result);
                if preceding_backslashes.is_multiple_of(2) {
                    // This \other is unnecessary -- remove the backslash.
                    result.push(next);
                    i += 2;
                    continue;
                }
            }
            result.push(bytes[i]);
            result.push(next);
            i += 2;
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }

    String::from_utf8(result).unwrap_or_else(|_| body.to_string())
}

/// Convert body from alt_char quoting to pref_char quoting:
/// - Escape unescaped pref_char occurrences
/// - Unescape escaped alt_char occurrences
fn convert_quotes_in_body(body: &str, pref: u8, alt: u8) -> String {
    let bytes = body.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == alt {
                // Unescape: \alt -> alt (when we're switching to pref quoting).
                let preceding = count_preceding_backslashes(&result);
                if preceding.is_multiple_of(2) {
                    result.push(alt);
                    i += 2;
                    continue;
                }
            }
            result.push(bytes[i]);
            result.push(next);
            i += 2;
        } else if bytes[i] == pref {
            // Escape unescaped preferred quote.
            let preceding = count_preceding_backslashes(&result);
            if preceding.is_multiple_of(2) {
                result.push(b'\\');
            }
            result.push(pref);
            i += 1;
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }

    String::from_utf8(result).unwrap_or_else(|_| body.to_string())
}

fn count_preceding_backslashes(buf: &[u8]) -> usize {
    buf.iter().rev().take_while(|&&b| b == b'\\').count()
}

fn has_unescaped_char(body: &str, ch: u8) -> bool {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == ch {
            return true;
        }
        i += 1;
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run_with_dialect(sql: &str, dialect: Dialect) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionQuotedLiterals::default();
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(
                    statement,
                    &RuleContext::new(sql, 0..sql.len(), index).with_dialect(dialect),
                )
            })
            .collect()
    }

    fn run(sql: &str) -> Vec<Issue> {
        run_with_dialect(sql, Dialect::Bigquery)
    }

    fn run_with_config(sql: &str, dialect: Dialect, config: &LintConfig) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionQuotedLiterals::from_config(config);
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(
                    statement,
                    &RuleContext::new(sql, 0..sql.len(), index).with_dialect(dialect),
                )
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

    fn apply_all_issue_autofixes(sql: &str, issues: &[Issue]) -> String {
        let mut out = sql.to_string();
        let mut edits = Vec::new();
        for issue in issues {
            if let Some(autofix) = &issue.autofix {
                edits.extend(autofix.edits.clone());
            }
        }
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        for edit in edits.into_iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        out
    }

    fn make_config(style: &str) -> LintConfig {
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.quoted_literals".to_string(),
                serde_json::json!({"preferred_quoted_literal_style": style}),
            )]),
        }
    }

    fn make_config_force(style: &str) -> LintConfig {
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.quoted_literals".to_string(),
                serde_json::json!({
                    "preferred_quoted_literal_style": style,
                    "force_enable": true,
                }),
            )]),
        }
    }

    // --- Dialect gating ---

    #[test]
    fn no_issue_for_ansi_dialect() {
        let issues = run_with_dialect("SELECT 'abc', \"def\"", Dialect::Ansi);
        assert!(issues.is_empty(), "CV10 should not fire for ANSI dialect");
    }

    #[test]
    fn no_issue_for_postgres_dialect() {
        let issues = run_with_dialect("SELECT 'abc'", Dialect::Postgres);
        assert!(issues.is_empty());
    }

    #[test]
    fn force_enable_works_for_postgres() {
        let config = make_config_force("single_quotes");
        let issues = run_with_config("SELECT 'abc'", Dialect::Postgres, &config);
        assert!(issues.is_empty(), "single-quoted only should pass");
    }

    // --- BigQuery consistent mode ---

    #[test]
    fn consistent_mode_flags_mixed_quotes() {
        let sql = "SELECT\n    \"some string\",\n    'some string'";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("autofix");
        assert_eq!(fixed, "SELECT\n    \"some string\",\n    \"some string\"");
    }

    #[test]
    fn consistent_mode_no_issue_for_single_style() {
        let issues = run("SELECT 'abc', 'def'");
        assert!(issues.is_empty());
    }

    #[test]
    fn consistent_mode_no_issue_for_double_style() {
        let issues = run("SELECT \"abc\", \"def\"");
        assert!(issues.is_empty());
    }

    // --- Explicit double_quotes preference ---

    #[test]
    fn double_pref_flags_single_quoted() {
        let config = make_config("double_quotes");
        let sql = "SELECT 'abc'";
        let issues = run_with_config(sql, Dialect::Bigquery, &config);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("autofix");
        assert_eq!(fixed, "SELECT \"abc\"");
    }

    #[test]
    fn double_pref_passes_double_quoted() {
        let config = make_config("double_quotes");
        let issues = run_with_config("SELECT \"abc\"", Dialect::Bigquery, &config);
        assert!(issues.is_empty());
    }

    // --- Single quotes preference ---

    #[test]
    fn single_pref_flags_double_quoted() {
        let config = make_config("single_quotes");
        let sql = "SELECT \"abc\"";
        let issues = run_with_config(sql, Dialect::Bigquery, &config);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("autofix");
        assert_eq!(fixed, "SELECT 'abc'");
    }

    // --- Empty strings ---

    #[test]
    fn double_pref_passes_empty_double() {
        let config = make_config("double_quotes");
        let issues = run_with_config("SELECT \"\"", Dialect::Bigquery, &config);
        assert!(issues.is_empty());
    }

    #[test]
    fn double_pref_flags_empty_single() {
        let config = make_config("double_quotes");
        let sql = "SELECT ''";
        let issues = run_with_config(sql, Dialect::Bigquery, &config);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("autofix");
        assert_eq!(fixed, "SELECT \"\"");
    }

    // --- Date constructor strings are ignored ---

    #[test]
    fn date_constructor_ignored_consistent() {
        let sql = "SELECT\n    \"quoted string\",\n    DATE'some string'";
        let issues = run(sql);
        assert!(
            issues.is_empty(),
            "DATE'...' should not count as single-quoted literal"
        );
    }

    #[test]
    fn date_constructor_ignored_double_pref() {
        let config = make_config("double_quotes");
        let issues = run_with_config("SELECT\n    DATE'some string'", Dialect::Bigquery, &config);
        assert!(issues.is_empty());
    }

    // --- Dollar-quoted strings are ignored ---

    #[test]
    fn dollar_quoted_ignored() {
        let config = make_config_force("single_quotes");
        let sql = "SELECT\n    'some string',\n    $$some_other_string$$";
        let issues = run_with_config(sql, Dialect::Postgres, &config);
        assert!(issues.is_empty());
    }

    // --- String prefix handling (BigQuery r/b prefixes) ---

    #[test]
    fn bigquery_prefixes_double_pref() {
        let config = make_config("double_quotes");
        let sql = "SELECT\n    r'some_string',\n    b'some_string',\n    R'some_string',\n    B'some_string'";
        let issues = run_with_config(sql, Dialect::Bigquery, &config);
        assert_eq!(issues.len(), 4);
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(
            fixed,
            "SELECT\n    r\"some_string\",\n    b\"some_string\",\n    R\"some_string\",\n    B\"some_string\""
        );
    }

    #[test]
    fn bigquery_prefixes_consistent_mode() {
        // r'...' and b"..." -- consistent mode derives single from first literal.
        let sql = "SELECT\n    r'some_string',\n    b\"some_string\"";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("autofix");
        assert_eq!(fixed, "SELECT\n    r'some_string',\n    b'some_string'");
    }

    // --- Escaping ---

    #[test]
    fn unnecessary_escaping_removed() {
        let config = make_config("double_quotes");
        let sql =
            "SELECT\n    'unnecessary \\\"\\\"escaping',\n    \"unnecessary \\'\\' escaping\"";
        let issues = run_with_config(sql, Dialect::Bigquery, &config);
        assert_eq!(issues.len(), 2);
    }

    // --- Hive, MySQL, SparkSQL dialects ---

    #[test]
    fn hive_dialect_supported() {
        let sql = "SELECT\n    \"some string\",\n    'some string'";
        let issues = run_with_dialect(sql, Dialect::Hive);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn mysql_dialect_supported() {
        let sql = "SELECT\n    \"some string\",\n    'some string'";
        let issues = run_with_dialect(sql, Dialect::Mysql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn sparksql_dialect_supported() {
        // SparkSQL maps to Databricks.
        let sql = "SELECT\n    \"some string\",\n    'some string'";
        let issues = run_with_dialect(sql, Dialect::Databricks);
        assert_eq!(issues.len(), 1);
    }

    // --- Triple-quoted strings ---

    #[test]
    fn triple_quotes_preferred_passes() {
        let config = make_config("double_quotes");
        let issues = run_with_config("SELECT \"\"\"some_string\"\"\"", Dialect::Bigquery, &config);
        assert!(issues.is_empty());
    }

    #[test]
    fn triple_quotes_alternate_fails_and_fixes() {
        let config = make_config("double_quotes");
        let sql = "SELECT '''some_string'''";
        let issues = run_with_config(sql, Dialect::Bigquery, &config);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("autofix");
        assert_eq!(fixed, "SELECT \"\"\"some_string\"\"\"");
    }

    #[test]
    fn scanner_ignores_quotes_inside_fully_templated_tags() {
        let literals = scan_string_literals("SELECT {{ \"'a_non_lintable_string'\" }}");
        assert!(literals.is_empty());
    }

    #[test]
    fn scanner_keeps_outer_literal_when_template_is_inside_literal() {
        let literals = scan_string_literals("SELECT '{{ \"a string\" }}'");
        assert_eq!(literals.len(), 1);
        assert_eq!(literals[0].quote_char, '\'');
    }

    #[test]
    fn emits_per_literal_issues_for_partially_fixable_raw_literals() {
        let config = make_config("double_quotes");
        let sql = "SELECT\n    r'Tricky \"quote',\n    r'Not-so-tricky \\\"quote'";
        let issues = run_with_config(sql, Dialect::Bigquery, &config);
        assert_eq!(issues.len(), 1);
        let fixable: Vec<_> = issues
            .iter()
            .filter(|issue| issue.autofix.is_some())
            .collect();
        assert_eq!(fixable.len(), 1);
        let fixed = apply_issue_autofix(sql, fixable[0]).expect("autofix");
        assert_eq!(
            fixed,
            "SELECT\n    r'Tricky \"quote',\n    r\"Not-so-tricky \\\"quote\""
        );
    }

    #[test]
    fn templated_mismatch_is_reported_even_when_unfixable() {
        let config = make_config("double_quotes");
        let sql = "SELECT '{{ \"a string\" }}'";
        let issues = run_with_config(sql, Dialect::Bigquery, &config);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].autofix.is_none());
    }

    #[test]
    fn triple_quote_fix_skips_literals_that_require_extra_escape() {
        let sql = "SELECT\n    '''abc\"''',\n    '''abc\" '''";
        let literals = scan_string_literals(sql);
        assert_eq!(literals.len(), 2);

        let first = normalize_literal(sql, &literals[0], '"', '\'');
        let second = normalize_literal(sql, &literals[1], '"', '\'');

        assert!(
            first.is_none(),
            "first triple literal should stay unfixable"
        );
        assert_eq!(second.as_deref(), Some("\"\"\"abc\" \"\"\""));
    }
}
