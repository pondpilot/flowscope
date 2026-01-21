//! Input collection and validation for SQL analysis requests.
//!
//! This module handles the parsing and collection of SQL statements from analysis requests,
//! supporting both file-based and inline SQL inputs.

use crate::parser::parse_sql_with_dialect;
use crate::types::{issue_codes, AnalyzeRequest, Dialect, Issue, Span};
use sqlparser::ast::Statement;
use std::borrow::Cow;
use std::ops::Range;
use std::rc::Rc;
use thiserror::Error;

#[cfg(feature = "templating")]
use crate::templater::{template_sql, TemplateMode};

/// Maximum iterations allowed when merging statement ranges to prevent infinite loops
/// on malformed SQL input.
const MAX_MERGE_ITERATIONS: usize = 10_000;

/// Creates an issue for a template rendering error.
#[cfg(feature = "templating")]
fn template_error_issue(
    error: &crate::templater::TemplateError,
    source_name: Option<&str>,
) -> Issue {
    let message = match source_name {
        Some(name) => format!("Template error in {name}: {error}"),
        None => format!("Template error: {error}"),
    };
    let mut issue = Issue::error(issue_codes::TEMPLATE_ERROR, message);
    if let Some(name) = source_name {
        issue = issue.with_source_name(name);
    }
    issue
}

/// Applies template preprocessing to SQL if configured.
///
/// Returns the (possibly transformed) SQL and whether templating was applied.
#[cfg(feature = "templating")]
fn apply_template<'a>(
    sql: &'a str,
    config: Option<&crate::templater::TemplateConfig>,
) -> Result<Cow<'a, str>, crate::templater::TemplateError> {
    match config {
        Some(cfg) if cfg.mode != TemplateMode::Raw => {
            let rendered = template_sql(sql, cfg)?;
            Ok(Cow::Owned(rendered))
        }
        _ => Ok(Cow::Borrowed(sql)),
    }
}

/// Errors that can occur when aligning statement ranges.
#[derive(Debug, Error)]
enum RangeAlignmentError {
    /// No ranges provided when statements were expected.
    #[error("no ranges provided when {0} statements were expected")]
    NoRanges(usize),
    /// Fewer ranges than statements (cannot split a range).
    #[error("fewer ranges ({0}) than statements ({1}), cannot split ranges")]
    FewerRangesThanStatements(usize, usize),
    /// Failed to merge ranges to match statement count.
    #[error("failed to merge ranges to match statement count")]
    MergeFailed,
    /// Iteration limit exceeded during merge (possible infinite loop).
    #[error("iteration limit ({0}) exceeded during merge, possible infinite loop")]
    IterationLimitExceeded(usize),
    /// A range extends beyond the source SQL bounds.
    #[error("range end ({0}) exceeds source SQL length ({1})")]
    OutOfBounds(usize, usize),
    /// Invalid range where start exceeds end.
    #[error("invalid range: start ({0}) > end ({1})")]
    InvalidRange(usize, usize),
}

/// Context for parsing SQL from a single source.
struct ParseContext<'a> {
    /// The full SQL buffer to parse.
    ///
    /// Uses `Cow` to support both borrowed SQL (from request) and owned SQL
    /// (from template rendering).
    source_sql: Cow<'a, str>,
    /// Optional source file name for error reporting.
    ///
    /// Wrapped in `Rc` so multiple `StatementInput` instances can share
    /// the same name without additional allocations.
    source_name: Option<Rc<String>>,
    /// SQL dialect for parsing.
    dialect: Dialect,
}

/// A parsed statement alongside optional source metadata.
pub(crate) struct StatementInput<'a> {
    /// The parsed SQL statement.
    pub(crate) statement: Statement,
    /// Optional source file name for error reporting and tracing.
    ///
    /// Uses `Rc<String>` to avoid repeated heap allocations when the same file
    /// contains multiple statements. All statements from a single file share
    /// the same `Rc`, so cloning is just a reference count increment.
    pub(crate) source_name: Option<Rc<String>>,
    /// The full SQL buffer this statement came from.
    ///
    /// Uses `Cow` to support both borrowed SQL (from request) and owned SQL
    /// (from template rendering). When templated, each statement owns its
    /// copy of the rendered SQL; when not templated, all statements from the
    /// same source share a borrowed reference.
    pub(crate) source_sql: Cow<'a, str>,
    /// Byte range of the statement within `source_sql`.
    pub(crate) source_range: Range<usize>,
}

/// Collects and parses SQL statements from the analysis request.
///
/// This function handles both file-based and inline SQL inputs, combining them into
/// a single ordered list of statements for analysis.
///
/// # Input Sources
///
/// The request can provide SQL through two mechanisms:
///
/// 1. **File sources** (`request.files`): A list of named SQL files with content
/// 2. **Inline SQL** (`request.sql`): Direct SQL text in the request body
///
/// Both sources are processed and combined. At least one must contain valid SQL.
///
/// # Processing Order
///
/// When both sources are present, statements are collected in this order:
///
/// 1. File statements (in the order files appear in the array)
/// 2. Inline SQL statements
///
/// This ordering ensures predictable cross-statement dependency detection,
/// where earlier statements can be referenced by later ones.
///
/// # Error Handling
///
/// Parse errors from individual files or inline SQL are collected as issues
/// rather than failing immediately. This allows partial analysis when some
/// inputs are valid.
///
/// # Returns
///
/// A tuple of `(statements, issues)` where:
/// - `statements`: Successfully parsed statements with source attribution
/// - `issues`: Any validation errors or parse failures encountered
pub(crate) fn collect_statements<'a>(
    request: &'a AnalyzeRequest,
) -> (Vec<StatementInput<'a>>, Vec<Issue>) {
    let mut issues = Vec::new();
    let mut statements = Vec::new();

    let has_sql = !request.sql.trim().is_empty();
    let has_files = request
        .files
        .as_ref()
        .map(|files| !files.is_empty())
        .unwrap_or(false);

    if !has_sql && !has_files {
        issues.push(Issue::error(
            issue_codes::INVALID_REQUEST,
            "Provide inline SQL or at least one file to analyze",
        ));
        return (Vec::new(), issues);
    }

    // Parse files first (if present)
    if let Some(files) = &request.files {
        for file in files {
            // Apply templating if configured
            #[cfg(feature = "templating")]
            let source_sql: Cow<'_, str> = {
                match apply_template(&file.content, request.template_config.as_ref()) {
                    Ok(sql) => sql,
                    Err(e) => {
                        issues.push(template_error_issue(&e, Some(&file.name)));
                        continue; // Skip this file but continue with others
                    }
                }
            };
            #[cfg(not(feature = "templating"))]
            let source_sql: Cow<'_, str> = Cow::Borrowed(file.content.as_str());

            let ctx = ParseContext {
                source_sql,
                source_name: Some(Rc::new(file.name.clone())),
                dialect: request.dialect,
            };
            let (file_stmts, file_issues) = parse_statements_individually(&ctx);
            statements.extend(file_stmts);
            issues.extend(file_issues);
        }
    }

    // Parse inline SQL if present (appended after file statements)
    if has_sql {
        // Apply templating if configured
        #[cfg(feature = "templating")]
        let source_sql: Cow<'_, str> = {
            match apply_template(&request.sql, request.template_config.as_ref()) {
                Ok(sql) => sql,
                Err(e) => {
                    issues.push(template_error_issue(&e, request.source_name.as_deref()));
                    return (statements, issues); // Return what we have so far
                }
            }
        };
        #[cfg(not(feature = "templating"))]
        let source_sql: Cow<'_, str> = Cow::Borrowed(request.sql.as_str());

        let ctx = ParseContext {
            source_sql,
            source_name: request.source_name.clone().map(Rc::new),
            dialect: request.dialect,
        };
        let (inline_stmts, inline_issues) = parse_statements_individually(&ctx);
        statements.extend(inline_stmts);
        issues.extend(inline_issues);
    }

    (statements, issues)
}

/// Parses SQL from a single buffer with best-effort error handling.
///
/// The parser first tries to process the entire buffer so statements containing
/// embedded semicolons (e.g. procedures) remain intact. If that fails, it
/// falls back to parsing semicolon-delimited slices individually so later
/// statements can still be analyzed.
fn parse_statements_individually<'a>(
    ctx: &ParseContext<'a>,
) -> (Vec<StatementInput<'a>>, Vec<Issue>) {
    let statement_ranges = compute_statement_ranges(&ctx.source_sql);

    match parse_full_sql_buffer(ctx, &statement_ranges) {
        Ok(statements) => (statements, Vec::new()),
        Err(fallback_error) => {
            let (statements, mut issues) =
                parse_statement_ranges_best_effort(ctx, statement_ranges);

            // Surface the fallback reason to users so they understand why
            // best-effort parsing was used
            if let Some(error) = fallback_error {
                let source_info = ctx
                    .source_name
                    .as_deref()
                    .map(|n| format!(" in {n}"))
                    .unwrap_or_default();
                let message = format!(
                    "Full SQL parsing failed{source_info}, using best-effort mode: {error}"
                );
                let mut issue = Issue::warning(issue_codes::PARSE_ERROR, message);
                if let Some(name) = ctx.source_name.as_deref() {
                    issue = issue.with_source_name(name);
                }
                issues.insert(0, issue);
            }

            (statements, issues)
        }
    }
}

/// Attempts full SQL buffer parsing with statement range alignment.
///
/// Returns:
/// - `Ok(statements)` if parsing and range alignment succeeded
/// - `Err(None)` if SQL parsing failed (no specific error to report)
/// - `Err(Some(error))` if range alignment failed (error should be surfaced)
fn parse_full_sql_buffer<'a>(
    ctx: &ParseContext<'a>,
    statement_ranges: &[Range<usize>],
) -> Result<Vec<StatementInput<'a>>, Option<RangeAlignmentError>> {
    let parsed = parse_sql_with_dialect(&ctx.source_sql, ctx.dialect).map_err(|_| None)?;

    if parsed.is_empty() {
        return Ok(Vec::new());
    }

    let aligned_ranges = match align_statement_ranges(
        &ctx.source_sql,
        statement_ranges,
        ctx.dialect,
        parsed.len(),
    ) {
        Ok(ranges) => ranges,
        Err(e) => {
            #[cfg(feature = "tracing")]
            tracing::debug!(
                source = ?ctx.source_name.as_deref(),
                error = %e,
                "Failed to align statement ranges, falling back to best-effort parsing"
            );
            return Err(Some(e));
        }
    };

    let mut statements = Vec::with_capacity(parsed.len());
    for (stmt, range) in parsed.into_iter().zip(aligned_ranges.into_iter()) {
        statements.push(StatementInput {
            statement: stmt,
            source_name: ctx.source_name.clone(),
            source_sql: ctx.source_sql.clone(),
            source_range: range,
        });
    }

    Ok(statements)
}

fn align_statement_ranges(
    source_sql: &str,
    statement_ranges: &[Range<usize>],
    dialect: Dialect,
    statement_count: usize,
) -> Result<Vec<Range<usize>>, RangeAlignmentError> {
    if statement_count == 0 {
        return Ok(Vec::new());
    }

    if statement_ranges.is_empty() {
        return Err(RangeAlignmentError::NoRanges(statement_count));
    }

    if statement_ranges.len() == statement_count {
        return Ok(statement_ranges.to_vec());
    }

    if statement_ranges.len() < statement_count {
        return Err(RangeAlignmentError::FewerRangesThanStatements(
            statement_ranges.len(),
            statement_count,
        ));
    }

    merge_statement_ranges(source_sql, statement_ranges, dialect, statement_count)
}

/// Re-aligns semicolon-delimited ranges with the statements parsed by `sqlparser`.
///
/// This is necessary because `sqlparser` may parse a single statement that contains
/// multiple semicolons (e.g., a `CREATE PROCEDURE` block). In such cases, our
/// naive `compute_statement_ranges` will produce more ranges than `sqlparser` produces
/// statements. This function greedily merges consecutive ranges until the resulting
/// SQL snippet successfully parses as a single statement, ensuring each parsed AST
/// node is mapped to its correct, complete source text.
fn merge_statement_ranges(
    source_sql: &str,
    statement_ranges: &[Range<usize>],
    dialect: Dialect,
    statement_count: usize,
) -> Result<Vec<Range<usize>>, RangeAlignmentError> {
    let mut merged = Vec::with_capacity(statement_count);
    let mut range_index = 0usize;

    // Process each expected statement, greedily merging ranges as needed
    for _ in 0..statement_count {
        // Ensure we have ranges left to process
        if range_index >= statement_ranges.len() {
            return Err(RangeAlignmentError::MergeFailed);
        }

        let mut statement_iterations = 0usize;

        // Start with the current range; we'll extend it if needed
        let mut current_range = statement_ranges[range_index].clone();
        range_index += 1;

        // Keep extending the range until we get exactly one parsed statement
        loop {
            statement_iterations += 1;
            if statement_iterations > MAX_MERGE_ITERATIONS {
                return Err(RangeAlignmentError::IterationLimitExceeded(
                    MAX_MERGE_ITERATIONS,
                ));
            }

            // Validate range boundaries
            if current_range.start > current_range.end {
                return Err(RangeAlignmentError::InvalidRange(
                    current_range.start,
                    current_range.end,
                ));
            }
            if current_range.end > source_sql.len() {
                return Err(RangeAlignmentError::OutOfBounds(
                    current_range.end,
                    source_sql.len(),
                ));
            }

            let snippet = &source_sql[current_range.clone()];
            match parse_sql_with_dialect(snippet, dialect) {
                // Found exactly one statement - this range is complete
                Ok(parsed) if parsed.len() == 1 => {
                    merged.push(current_range);
                    break;
                }
                // Either parsed multiple statements, zero statements, or failed to parse.
                // Extend the range by including the next semicolon-delimited segment and retry.
                _ => {
                    if range_index >= statement_ranges.len() {
                        return Err(RangeAlignmentError::MergeFailed);
                    }
                    // Merge current range with the next range
                    current_range = current_range.start..statement_ranges[range_index].end;
                    range_index += 1;
                }
            }
        }
    }

    // Verify we consumed all ranges - if not, the merge logic is incorrect
    if range_index != statement_ranges.len() {
        return Err(RangeAlignmentError::MergeFailed);
    }

    Ok(merged)
}

/// Parses SQL slices defined by `statement_ranges`, recording parse errors per slice.
fn parse_statement_ranges_best_effort<'a>(
    ctx: &ParseContext<'a>,
    statement_ranges: Vec<Range<usize>>,
) -> (Vec<StatementInput<'a>>, Vec<Issue>) {
    let mut statements = Vec::new();
    let mut issues = Vec::new();

    let source_sql_ref: &str = &ctx.source_sql;

    for range in statement_ranges {
        // Skip invalid ranges
        if range.start > range.end || range.end > source_sql_ref.len() {
            continue;
        }

        let statement_sql = &source_sql_ref[range.clone()];

        match parse_sql_with_dialect(statement_sql, ctx.dialect) {
            Ok(parsed) => {
                // Typically one statement per range, but handle multiple if present
                for stmt in parsed {
                    statements.push(StatementInput {
                        statement: stmt,
                        source_name: ctx.source_name.clone(),
                        source_sql: ctx.source_sql.clone(),
                        source_range: range.clone(),
                    });
                }
            }
            Err(e) => {
                // Record the parse error but continue with remaining statements
                let message = match ctx.source_name.as_deref() {
                    Some(name) => format!("Parse error in {name}: {e}"),
                    None => format!("Parse error: {e}"),
                };

                let mut issue = Issue::error(issue_codes::PARSE_ERROR, message)
                    .with_span(Span::new(range.start, range.end));
                if let Some(name) = ctx.source_name.as_deref() {
                    issue = issue.with_source_name(name);
                }
                issues.push(issue);
            }
        }
    }

    (statements, issues)
}

pub(crate) fn split_statement_spans(sql: &str) -> Vec<Span> {
    compute_statement_ranges(sql)
        .into_iter()
        .map(|range| Span::new(range.start, range.end))
        .collect()
}

fn compute_statement_ranges(sql: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    if sql.is_empty() {
        return ranges;
    }

    let mut start = 0usize;
    let mut i = 0usize;
    let len = sql.len();

    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backtick = false;
    let mut in_bracket = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut dollar_delimiter: Option<String> = None;

    while i < len {
        if let Some(delim) = &dollar_delimiter {
            if sql[i..].starts_with(delim) {
                i += delim.len();
                dollar_delimiter = None;
            } else {
                let (_, advance) = next_char(sql, i);
                i += advance;
            }
            continue;
        }

        if in_line_comment {
            let (ch, advance) = next_char(sql, i);
            i += advance;
            if ch == '\n' || ch == '\r' {
                in_line_comment = false;
            }
            continue;
        }

        if in_block_comment {
            if starts_with_at(sql, i, "*/") {
                i += 2;
                in_block_comment = false;
            } else {
                let (_, advance) = next_char(sql, i);
                i += advance;
            }
            continue;
        }

        if in_single_quote {
            let (ch, advance) = next_char(sql, i);
            i += advance;
            if ch == '\'' {
                if let Some((next, next_len)) = char_at(sql, i) {
                    if next == '\'' {
                        i += next_len;
                    } else {
                        in_single_quote = false;
                    }
                } else {
                    in_single_quote = false;
                }
            }
            continue;
        }

        if in_double_quote {
            let (ch, advance) = next_char(sql, i);
            i += advance;
            if ch == '"' {
                if let Some((next, next_len)) = char_at(sql, i) {
                    if next == '"' {
                        i += next_len;
                    } else {
                        in_double_quote = false;
                    }
                } else {
                    in_double_quote = false;
                }
            }
            continue;
        }

        if in_backtick {
            let (ch, advance) = next_char(sql, i);
            i += advance;
            if ch == '`' {
                if let Some((next, next_len)) = char_at(sql, i) {
                    if next == '`' {
                        i += next_len;
                    } else {
                        in_backtick = false;
                    }
                } else {
                    in_backtick = false;
                }
            }
            continue;
        }

        if in_bracket {
            let (ch, advance) = next_char(sql, i);
            i += advance;
            if ch == ']' {
                if let Some((next, next_len)) = char_at(sql, i) {
                    if next == ']' {
                        i += next_len;
                    } else {
                        in_bracket = false;
                    }
                } else {
                    in_bracket = false;
                }
            }
            continue;
        }

        let (ch, advance) = next_char(sql, i);
        match ch {
            '\'' => {
                in_single_quote = true;
                i += advance;
                continue;
            }
            '"' => {
                in_double_quote = true;
                i += advance;
                continue;
            }
            '`' => {
                in_backtick = true;
                i += advance;
                continue;
            }
            '[' => {
                in_bracket = true;
                i += advance;
                continue;
            }
            '-' => {
                if starts_with_at(sql, i + advance, "-") {
                    in_line_comment = true;
                    i += advance + 1;
                    continue;
                }
            }
            '#' => {
                in_line_comment = true;
                i += advance;
                continue;
            }
            '/' => {
                if starts_with_at(sql, i + advance, "*") {
                    in_block_comment = true;
                    i += advance + 1;
                    continue;
                }
            }
            '$' => {
                if let Some((delim, end_idx)) = detect_dollar_quote(sql, i) {
                    dollar_delimiter = Some(delim);
                    i = end_idx;
                    continue;
                }
            }
            ';' => {
                push_statement_range(&mut ranges, sql, start, i);
                start = i + advance;
            }
            _ => {}
        }

        i += advance;
    }

    push_statement_range(&mut ranges, sql, start, len);
    ranges
}

fn detect_dollar_quote(sql: &str, start: usize) -> Option<(String, usize)> {
    let len = sql.len();
    if start + 1 >= len {
        return None;
    }

    let mut idx = start + 1;
    while idx < len {
        let (ch, advance) = next_char(sql, idx);
        idx += advance;
        if ch == '$' {
            let delimiter = sql[start..idx].to_string();
            return Some((delimiter, idx));
        }
        if !(ch == '_' || ch.is_ascii_alphanumeric()) {
            return None;
        }
    }

    None
}

fn starts_with_at(sql: &str, index: usize, pattern: &str) -> bool {
    if index >= sql.len() {
        return false;
    }
    if !sql.is_char_boundary(index) {
        return false;
    }
    sql[index..].starts_with(pattern)
}

fn next_char(sql: &str, index: usize) -> (char, usize) {
    debug_assert!(sql.is_char_boundary(index));
    let mut iter = sql[index..].char_indices();
    let (_, ch) = iter.next().expect("index should point to a char boundary");
    let advance = ch.len_utf8();
    (ch, advance)
}

fn char_at(sql: &str, index: usize) -> Option<(char, usize)> {
    if index >= sql.len() {
        return None;
    }
    if !sql.is_char_boundary(index) {
        return None;
    }
    let mut iter = sql[index..].char_indices();
    let (_, ch) = iter.next().expect("index should point to a char boundary");
    let advance = ch.len_utf8();
    Some((ch, advance))
}

fn push_statement_range(ranges: &mut Vec<Range<usize>>, sql: &str, start: usize, end: usize) {
    if let Some(range) = trim_statement_range(sql, start, end) {
        ranges.push(range);
    }
}

fn trim_statement_range(sql: &str, start: usize, end: usize) -> Option<Range<usize>> {
    if start >= end {
        return None;
    }

    let mut s = start;
    let mut e = end;

    let bytes = sql.as_bytes();

    while s < e {
        if s + 1 < e {
            let first = bytes[s];
            let second = bytes[s + 1];
            if first == b'-' && second == b'-' {
                s = skip_line_comment(bytes, s + 2, e);
                continue;
            }
            if first == b'/' && second == b'*' {
                s = skip_block_comment(bytes, s + 2, e);
                continue;
            }
        }

        let b = bytes[s];
        match b {
            b'#' => {
                s = skip_line_comment(bytes, s + 1, e);
            }
            b' ' | b'\t' | b'\r' | b'\n' => {
                s += 1;
            }
            _ => break,
        }
    }

    while s < e {
        let b = bytes[e - 1];
        match b {
            b' ' | b'\t' | b'\r' | b'\n' => {
                e -= 1;
            }
            _ => break,
        }
    }

    if s >= e {
        return None;
    }

    Some(s..e)
}

fn skip_line_comment(bytes: &[u8], mut index: usize, end: usize) -> usize {
    while index < end {
        let byte = bytes[index];
        index += 1;
        if byte == b'\n' || byte == b'\r' {
            break;
        }
    }
    index
}

fn skip_block_comment(bytes: &[u8], mut index: usize, end: usize) -> usize {
    while index < end {
        if index + 1 < end && bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return index + 2;
        }
        index += 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Dialect, FileSource};

    fn base_request() -> AnalyzeRequest {
        AnalyzeRequest {
            sql: String::new(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
            #[cfg(feature = "templating")]
            template_config: None,
        }
    }

    #[test]
    fn collects_file_and_inline_statements() {
        let mut request = base_request();
        request.sql = "SELECT 2".to_string();
        request.source_name = Some("inline.sql".to_string());
        request.files = Some(vec![FileSource {
            name: "file.sql".to_string(),
            content: "SELECT 1".to_string(),
        }]);

        let (statements, issues) = collect_statements(&request);
        assert!(issues.is_empty());
        assert_eq!(statements.len(), 2);
        assert_eq!(
            statements[0].source_name.as_deref().map(String::as_str),
            Some("file.sql")
        );
        assert_eq!(
            statements[0].source_sql[statements[0].source_range.clone()].trim(),
            "SELECT 1"
        );
        assert_eq!(
            statements[1].source_name.as_deref().map(String::as_str),
            Some("inline.sql")
        );
        assert_eq!(
            statements[1].source_sql[statements[1].source_range.clone()].trim(),
            "SELECT 2"
        );
    }

    #[test]
    fn reports_invalid_request_without_inputs() {
        let request = base_request();
        let (_statements, issues) = collect_statements(&request);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::INVALID_REQUEST);
    }

    #[test]
    fn statement_ranges_respect_strings() {
        let sql = "SELECT ';' as value;SELECT 2;";
        let ranges = compute_statement_ranges(sql);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&sql[ranges[0].clone()], "SELECT ';' as value");
        assert_eq!(&sql[ranges[1].clone()], "SELECT 2");
    }

    #[test]
    fn statement_ranges_skip_comments() {
        let sql = "SELECT 1; -- comment; still comment\nSELECT 2; /* block; comment */ SELECT 3;";
        let ranges = compute_statement_ranges(sql);
        assert_eq!(ranges.len(), 3);
        assert_eq!(&sql[ranges[0].clone()], "SELECT 1");
        assert_eq!(&sql[ranges[1].clone()], "SELECT 2");
        assert_eq!(&sql[ranges[2].clone()], "SELECT 3");
    }

    #[test]
    fn statement_ranges_handle_dollar_quoting() {
        let sql = "DO $$ BEGIN RAISE NOTICE ';'; END $$; SELECT 1;";
        let ranges = compute_statement_ranges(sql);
        assert_eq!(ranges.len(), 2);
        assert_eq!(
            &sql[ranges[0].clone()],
            "DO $$ BEGIN RAISE NOTICE ';'; END $$"
        );
        assert_eq!(&sql[ranges[1].clone()], "SELECT 1");
    }

    #[test]
    fn parses_procedure_with_inner_semicolons() {
        let mut request = base_request();
        request.dialect = Dialect::Snowflake;
        request.sql = r#"
            CREATE PROCEDURE demo()
            LANGUAGE SQL
            AS
            BEGIN
                SELECT 'a';
                SELECT 'b';
                RETURN 'done';
            END;
            SELECT 1;
        "#
        .to_string();

        let (statements, issues) = collect_statements(&request);
        assert!(issues.is_empty(), "Expected no issues, got {issues:?}");
        assert_eq!(
            statements.len(),
            2,
            "Expected procedure and trailing select"
        );
        assert!(matches!(
            statements[0].statement,
            Statement::CreateProcedure { .. }
        ));
        let procedure_source = &statements[0].source_sql[statements[0].source_range.clone()];
        assert!(
            procedure_source.contains("SELECT 'b';") && procedure_source.contains("RETURN 'done';"),
            "Procedure source should include entire body: {procedure_source:?}"
        );
        assert!(matches!(statements[1].statement, Statement::Query(_)));
    }

    #[test]
    fn best_effort_parsing_continues_after_error() {
        // SQL with valid statement, invalid statement, then valid statement
        let mut request = base_request();
        request.sql = r#"
            SELECT 1 FROM users;
            SELECT FROM;
            SELECT 2 FROM orders;
        "#
        .to_string();

        let (statements, issues) = collect_statements(&request);

        // Should have parsed 2 valid statements
        assert_eq!(statements.len(), 2, "Expected 2 valid statements");

        // Should have 1 parse error for the invalid statement
        assert_eq!(issues.len(), 1, "Expected 1 parse error");
        assert_eq!(issues[0].code, issue_codes::PARSE_ERROR);

        // The error should have a span pointing to the invalid statement
        assert!(issues[0].span.is_some(), "Error should have span info");
    }

    #[test]
    fn best_effort_parsing_with_file_source() {
        let mut request = base_request();
        request.files = Some(vec![FileSource {
            name: "test.sql".to_string(),
            content: r#"
                SELECT a FROM t1;
                INVALID SYNTAX HERE;
                SELECT b FROM t2;
            "#
            .to_string(),
        }]);

        let (statements, issues) = collect_statements(&request);

        assert_eq!(statements.len(), 2, "Expected 2 valid statements");
        assert_eq!(issues.len(), 1, "Expected 1 parse error");

        // Error message should include file name
        assert!(
            issues[0].message.contains("test.sql"),
            "Error should mention file name"
        );

        // Issue should have source_name set
        assert_eq!(
            issues[0].source_name.as_deref(),
            Some("test.sql"),
            "Issue should have source_name set"
        );
    }

    #[test]
    fn best_effort_parsing_multiple_errors() {
        let mut request = base_request();
        request.sql = r#"
            SELECT 1;
            BROKEN STATEMENT 1;
            SELECT 2;
            BROKEN STATEMENT 2;
            SELECT 3;
        "#
        .to_string();

        let (statements, issues) = collect_statements(&request);

        assert_eq!(statements.len(), 3, "Expected 3 valid statements");
        assert_eq!(issues.len(), 2, "Expected 2 parse errors");
    }

    #[test]
    fn empty_sql_returns_no_statements() {
        let sql = "";
        let ranges = compute_statement_ranges(sql);
        assert!(ranges.is_empty(), "Empty SQL should produce no ranges");
    }

    #[test]
    fn whitespace_only_sql_returns_no_statements() {
        let sql = "   \n\t\r\n   ";
        let ranges = compute_statement_ranges(sql);
        assert!(
            ranges.is_empty(),
            "Whitespace-only SQL should produce no ranges"
        );
    }

    #[test]
    fn comments_only_sql_returns_no_statements() {
        let sql = "-- just a comment\n/* another comment */";
        let ranges = compute_statement_ranges(sql);
        assert!(
            ranges.is_empty(),
            "Comments-only SQL should produce no ranges"
        );
    }

    #[test]
    fn empty_inline_sql_with_valid_file() {
        let mut request = base_request();
        request.sql = "   ".to_string(); // whitespace only
        request.files = Some(vec![FileSource {
            name: "file.sql".to_string(),
            content: "SELECT 1".to_string(),
        }]);

        let (statements, issues) = collect_statements(&request);
        assert!(issues.is_empty());
        assert_eq!(statements.len(), 1);
        assert_eq!(
            statements[0].source_name.as_deref().map(String::as_str),
            Some("file.sql")
        );
    }

    #[test]
    fn statement_ranges_handle_unicode_identifiers() {
        // Test with multi-byte Unicode characters (Japanese, emoji, etc.)
        let sql = "SELECT '日本語' AS 名前; SELECT '🎉' AS emoji;";
        let ranges = compute_statement_ranges(sql);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&sql[ranges[0].clone()], "SELECT '日本語' AS 名前");
        assert_eq!(&sql[ranges[1].clone()], "SELECT '🎉' AS emoji");
    }

    #[test]
    fn statement_ranges_handle_unicode_in_strings() {
        // Ensure semicolons inside Unicode strings are not treated as delimiters
        let sql = "SELECT '你好;世界' AS greeting; SELECT 2;";
        let ranges = compute_statement_ranges(sql);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&sql[ranges[0].clone()], "SELECT '你好;世界' AS greeting");
        assert_eq!(&sql[ranges[1].clone()], "SELECT 2");
    }

    #[test]
    fn statement_ranges_handle_mixed_ascii_unicode() {
        // Mix of ASCII and various Unicode scripts
        let sql = "SELECT 'café' AS drink; SELECT 'naïve' AS word; SELECT 'Müller' AS name;";
        let ranges = compute_statement_ranges(sql);
        assert_eq!(ranges.len(), 3);
        assert_eq!(&sql[ranges[0].clone()], "SELECT 'café' AS drink");
        assert_eq!(&sql[ranges[1].clone()], "SELECT 'naïve' AS word");
        assert_eq!(&sql[ranges[2].clone()], "SELECT 'Müller' AS name");
    }

    #[test]
    fn unicode_sql_parses_correctly() {
        // End-to-end test: ensure Unicode SQL parses and produces correct ranges
        let mut request = base_request();
        request.sql = "SELECT '日本' AS country; SELECT 'émoji: 🚀' AS test;".to_string();

        let (statements, issues) = collect_statements(&request);
        assert!(issues.is_empty(), "Expected no issues, got {issues:?}");
        assert_eq!(statements.len(), 2);

        // Verify the source ranges correctly capture the Unicode content
        let first_sql = &statements[0].source_sql[statements[0].source_range.clone()];
        let second_sql = &statements[1].source_sql[statements[1].source_range.clone()];
        assert!(
            first_sql.contains("日本"),
            "First statement should contain Japanese: {first_sql}"
        );
        assert!(
            second_sql.contains("🚀"),
            "Second statement should contain rocket emoji: {second_sql}"
        );
    }
}
