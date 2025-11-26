//! Input collection and validation for SQL analysis requests.
//!
//! This module handles the parsing and collection of SQL statements from analysis requests,
//! supporting both file-based and inline SQL inputs.

use crate::parser::parse_sql_with_dialect;
use crate::types::{issue_codes, AnalyzeRequest, Issue};
use sqlparser::ast::Statement;
use std::ops::Range;

/// A parsed statement alongside optional source metadata.
pub(crate) struct StatementInput<'a> {
    /// The parsed SQL statement.
    pub(crate) statement: Statement,
    /// Optional source file name for error reporting and tracing.
    pub(crate) source_name: Option<String>,
    /// The full SQL buffer this statement came from.
    pub(crate) source_sql: &'a str,
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
            let source_sql = file.content.as_str();
            let statement_ranges = compute_statement_ranges(source_sql);
            match parse_sql_with_dialect(source_sql, request.dialect) {
                Ok(stmts) => {
                    let ranges = align_statement_ranges(stmts.len(), &statement_ranges, source_sql);
                    for (stmt, range) in stmts.into_iter().zip(ranges) {
                        statements.push(StatementInput {
                            statement: stmt,
                            source_name: Some(file.name.clone()),
                            source_sql,
                            source_range: range,
                        });
                    }
                }
                Err(e) => issues.push(Issue::error(
                    issue_codes::PARSE_ERROR,
                    format!("Error parsing {}: {}", file.name, e),
                )),
            }
        }
    }

    // Parse inline SQL if present (appended after file statements)
    if has_sql {
        let source_sql = request.sql.as_str();
        let statement_ranges = compute_statement_ranges(source_sql);
        match parse_sql_with_dialect(source_sql, request.dialect) {
            Ok(stmts) => {
                let ranges = align_statement_ranges(stmts.len(), &statement_ranges, source_sql);
                statements.extend(stmts.into_iter().zip(ranges).map(|(stmt, range)| {
                    StatementInput {
                        statement: stmt,
                        source_name: request.source_name.clone(),
                        source_sql,
                        source_range: range,
                    }
                }));
            }
            Err(e) => {
                issues.push(Issue::error(issue_codes::PARSE_ERROR, e.to_string()));
            }
        }
    }

    (statements, issues)
}

fn align_statement_ranges(
    statement_count: usize,
    ranges: &[Range<usize>],
    source_sql: &str,
) -> Vec<Range<usize>> {
    if statement_count == 0 {
        return Vec::new();
    }

    if ranges.len() >= statement_count {
        return ranges.iter().take(statement_count).cloned().collect();
    }

    let mut aligned: Vec<Range<usize>> = ranges.to_vec();
    let fallback = 0..source_sql.len();
    while aligned.len() < statement_count {
        aligned.push(fallback.clone());
    }
    aligned
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
        assert_eq!(statements[0].source_name.as_deref(), Some("file.sql"));
        assert_eq!(
            statements[0].source_sql[statements[0].source_range.clone()].trim(),
            "SELECT 1"
        );
        assert_eq!(statements[1].source_name.as_deref(), Some("inline.sql"));
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
}
