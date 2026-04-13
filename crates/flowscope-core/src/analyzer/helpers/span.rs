//! Utilities for finding identifier spans in SQL text.
//!
//! This module provides functions to locate identifiers in SQL source code
//! for error reporting. Since sqlparser doesn't expose AST node locations,
//! we use text search to find approximate positions.

use super::{split_qualified_identifiers, unquote_identifier};
use crate::types::Span;

/// Finds the byte offset span of an identifier in SQL text.
///
/// Searches for the identifier as a whole word (not part of another identifier).
/// Returns the first match found, or `None` if not found.
///
/// # Arguments
///
/// * `sql` - The SQL source text
/// * `identifier` - The identifier to find (table name, column name, etc.)
/// * `search_start` - Byte offset to start searching from (for multi-statement SQL)
///
/// # Example
///
/// ```ignore
/// let sql = "SELECT * FROM users WHERE id = 1";
/// let span = find_identifier_span(sql, "users", 0);
/// assert_eq!(span, Some(Span { start: 14, end: 19 }));
/// ```
pub fn find_identifier_span(sql: &str, identifier: &str, search_start: usize) -> Option<Span> {
    if identifier.is_empty() || search_start >= sql.len() {
        return None;
    }
    if !sql.is_char_boundary(search_start) {
        #[cfg(feature = "tracing")]
        tracing::warn!(
            search_start,
            sql_len = sql.len(),
            "find_identifier_span: search_start is not on a UTF-8 char boundary"
        );
        return None;
    }

    let search_text = &sql[search_start..];
    find_qualified_name(search_text, identifier)
        .map(|(start, end)| Span::new(search_start + start, search_start + end))
}

/// Finds every occurrence of an identifier in SQL text.
///
/// Returns all non-overlapping word-boundary matches (case-insensitive) within
/// `[search_start, search_end)`. The `search_end` bound lets callers scope the
/// scan to a single statement. Strings inside single-quoted or dollar-quoted
/// SQL literals and inside block/line/hash comments are skipped so `-- users`,
/// `# users`, or `'users'` do not produce false positives.
///
/// This is intentionally a textual scan rather than an AST walk: sqlparser
/// does not preserve per-occurrence source positions, and a text scan handles
/// the common case (every spelling of a table/CTE/view name) correctly. Alias
/// shadowing on column references is out of scope here and is handled by
/// populating `name_spans` only on table-like node types.
pub fn find_all_identifier_spans(
    sql: &str,
    identifier: &str,
    search_start: usize,
    search_end: usize,
) -> Vec<Span> {
    let mut spans = Vec::new();
    if identifier.is_empty() || search_start >= search_end || search_end > sql.len() {
        return spans;
    }
    if !sql.is_char_boundary(search_start) || !sql.is_char_boundary(search_end) {
        #[cfg(feature = "tracing")]
        tracing::warn!(
            search_start,
            search_end,
            sql_len = sql.len(),
            "find_all_identifier_spans: search range is not on UTF-8 char boundaries"
        );
        return spans;
    }

    let scope = &sql[search_start..search_end];
    let mut cursor = 0usize;
    while let Some(occurrence) = find_identifier_occurrence(scope, identifier, cursor) {
        spans.push(Span::new(
            search_start + occurrence.full_start,
            search_start + occurrence.full_end,
        ));
        cursor = occurrence.full_end;
    }
    spans
}

/// Finds the next relation occurrence and returns both its full span and the
/// span of the final identifier component.
///
/// The full span includes any qualification and quoting as written in the SQL.
/// The returned name span points at the inner content of the final identifier
/// component so callers can highlight just the node name (for example, `orders`
/// in `sales.orders`, or `my.table` in `"my.schema"."my.table"`). String
/// literals and comments are skipped.
pub fn find_relation_occurrence_spans(
    sql: &str,
    identifier: &str,
    search_start: usize,
) -> Option<(Span, Span)> {
    if identifier.is_empty() || search_start >= sql.len() {
        return None;
    }
    if !sql.is_char_boundary(search_start) {
        #[cfg(feature = "tracing")]
        tracing::warn!(
            search_start,
            sql_len = sql.len(),
            "find_relation_occurrence_spans: search_start is not on a UTF-8 char boundary"
        );
        return None;
    }

    let search_text = &sql[search_start..];
    let occurrence = find_identifier_occurrence(search_text, identifier, 0)?;
    Some((
        Span::new(
            search_start + occurrence.full_start,
            search_start + occurrence.full_end,
        ),
        Span::new(
            search_start + occurrence.tail_start,
            search_start + occurrence.tail_end,
        ),
    ))
}

/// Finds the span of a CTE body (the parenthesized subquery after `AS`) given
/// the span of the CTE name.
///
/// Starting after `name_span.end`, skips whitespace/comments and an optional
/// `AS` keyword, then locates the matching parenthesis pair and returns its
/// span (including the parentheses themselves). Returns `None` if the body
/// cannot be located — for example if the SQL has already been rewritten.
pub fn find_cte_body_span(sql: &str, name_span: Span) -> Option<Span> {
    if name_span.end > sql.len() || !sql.is_char_boundary(name_span.end) {
        #[cfg(feature = "tracing")]
        tracing::warn!(
            end = name_span.end,
            sql_len = sql.len(),
            "find_cte_body_span: name_span.end is not on a UTF-8 char boundary"
        );
        return None;
    }

    let bytes = sql.as_bytes();

    // Skip whitespace / comments after the CTE name.
    let mut pos = skip_whitespace_and_comments(sql, name_span.end);

    // Optional column list: `cte_name(col1, col2) AS (...)`.
    if pos < bytes.len() && bytes[pos] == b'(' {
        let list_end = find_matching_paren(bytes, pos)?;
        pos = skip_whitespace_and_comments(sql, list_end + 1);
    }

    // Required `AS` keyword.
    pos = consume_ascii_keyword(sql, pos, "AS")?;

    // PostgreSQL materialization modifiers:
    // `AS MATERIALIZED (...)` / `AS NOT MATERIALIZED (...)`.
    if let Some(after_not) = consume_ascii_keyword(sql, pos, "NOT") {
        if let Some(after_materialized) = consume_ascii_keyword(sql, after_not, "MATERIALIZED") {
            pos = after_materialized;
        }
    } else if let Some(after_materialized) = consume_ascii_keyword(sql, pos, "MATERIALIZED") {
        pos = after_materialized;
    }

    if pos >= bytes.len() || bytes[pos] != b'(' {
        return None;
    }

    let body_end = find_matching_paren(bytes, pos)?;
    Some(Span::new(pos, body_end + 1))
}

/// Given the byte offset of an opening `(`, finds the byte offset of its
/// matching `)`. Respects string literals and comments so parentheses inside
/// them do not affect depth. Operates on bytes so non-ASCII content in the
/// SQL (identifiers, comments, string literals) does not cause panics on
/// UTF-8 boundary slicing.
fn find_matching_paren(bytes: &[u8], open: usize) -> Option<usize> {
    if open >= bytes.len() || bytes[open] != b'(' {
        return None;
    }
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        if let Some(skip_to) = skip_string_or_comment(bytes, i) {
            debug_assert!(
                skip_to > i,
                "skip_string_or_comment must advance past the current index"
            );
            if skip_to <= i {
                return None;
            }
            i = skip_to;
            continue;
        }
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// If `pos` is the start of a string literal or SQL comment, returns the byte
/// offset immediately after it. Otherwise returns `None`.
///
/// Handles block comments (`/* */`), line comments (`--`, `#`), single-quoted
/// strings, and PostgreSQL dollar-quoted strings (`$$...$$`, `$tag$...$tag$`).
/// All delimiters are ASCII, so operating on raw bytes is safe and sidesteps
/// any UTF-8 char-boundary concerns when the caller's cursor advances bytewise.
fn skip_string_or_comment(bytes: &[u8], pos: usize) -> Option<usize> {
    if pos >= bytes.len() {
        return None;
    }
    // Block comment `/* ... */`.
    if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
        let mut i = pos + 2;
        while i + 1 < bytes.len() {
            if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                return Some(i + 2);
            }
            i += 1;
        }
        return Some(bytes.len());
    }
    // Line comment `-- ... \n`.
    if pos + 1 < bytes.len() && bytes[pos] == b'-' && bytes[pos + 1] == b'-' {
        let mut i = pos + 2;
        while i < bytes.len() {
            if bytes[i] == b'\n' {
                return Some(i + 1);
            }
            i += 1;
        }
        return Some(bytes.len());
    }
    // MySQL/Hive hash comment `# ... \n`.
    if bytes[pos] == b'#' {
        let mut i = pos + 1;
        while i < bytes.len() {
            if bytes[i] == b'\n' {
                return Some(i + 1);
            }
            i += 1;
        }
        return Some(bytes.len());
    }
    // Single-quoted string literal, with SQL `''` escape.
    if bytes[pos] == b'\'' {
        let mut i = pos + 1;
        while i < bytes.len() {
            if bytes[i] == b'\'' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                return Some(i + 1);
            }
            i += 1;
        }
        return Some(bytes.len());
    }
    // PostgreSQL dollar-quoted string literal.
    if bytes[pos] == b'$' {
        return skip_dollar_quoted_string(bytes, pos);
    }
    None
}

fn skip_dollar_quoted_string(bytes: &[u8], pos: usize) -> Option<usize> {
    if pos >= bytes.len() || bytes[pos] != b'$' {
        return None;
    }

    let mut tag_end = pos + 1;
    while tag_end < bytes.len() {
        match bytes[tag_end] {
            b'$' => {
                let delimiter = &bytes[pos..=tag_end];
                let search_start = tag_end + 1;
                let mut i = search_start;
                while i + delimiter.len() <= bytes.len() {
                    if &bytes[i..i + delimiter.len()] == delimiter {
                        return Some(i + delimiter.len());
                    }
                    i += 1;
                }
                return Some(bytes.len());
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' => tag_end += 1,
            _ => return None,
        }
    }
    None
}

/// Finds the span of a CTE definition name in SQL text.
///
/// Matches `WITH name`, `WITH RECURSIVE name`, or `, name` patterns and returns the span for `name`.
/// Handles SQL comments between keywords and identifiers.
/// Uses string operations instead of regex for performance.
pub fn find_cte_definition_span(sql: &str, identifier: &str, search_start: usize) -> Option<Span> {
    if identifier.is_empty() || search_start >= sql.len() {
        return None;
    }

    let search_text = &sql[search_start..];

    // Find CTE anchors: "WITH" keyword or comma separator
    let mut pos = 0;
    while pos < search_text.len() {
        // Look for "WITH" keyword (case-insensitive, word boundary)
        if let Some(with_pos) = find_keyword_case_insensitive(&search_text[pos..], "WITH") {
            let after_with = pos + with_pos + 4;
            // Skip whitespace and comments after WITH
            let after_ws = skip_whitespace_and_comments(search_text, after_with);

            // Check for optional RECURSIVE keyword
            let after_recursive = if let Some(rec_pos) =
                find_keyword_case_insensitive(&search_text[after_ws..], "RECURSIVE")
            {
                if rec_pos == 0 {
                    // RECURSIVE found immediately after whitespace
                    skip_whitespace_and_comments(search_text, after_ws + 9)
                } else {
                    after_ws
                }
            } else {
                after_ws
            };

            // Try to match the identifier at this position
            if let Some((start, end)) =
                match_identifier_at(search_text, after_recursive, identifier)
            {
                return Some(Span::new(search_start + start, search_start + end));
            }
            pos = after_recursive.max(after_with);
            continue;
        }

        // Look for comma separator
        if let Some(comma_pos) = search_text[pos..].find(',') {
            let after_comma = pos + comma_pos + 1;
            // Skip whitespace and comments after comma
            let after_ws = skip_whitespace_and_comments(search_text, after_comma);
            if let Some((start, end)) = match_identifier_at(search_text, after_ws, identifier) {
                return Some(Span::new(search_start + start, search_start + end));
            }
            pos = after_comma;
            continue;
        }

        break;
    }

    None
}

/// Finds the span of a derived table alias in SQL text.
///
/// Matches `) alias` or `) AS alias` patterns and returns the span for `alias`.
/// Handles SQL comments between the closing paren and the alias.
/// Uses string operations instead of regex for performance.
pub fn find_derived_table_alias_span(
    sql: &str,
    identifier: &str,
    search_start: usize,
) -> Option<Span> {
    if identifier.is_empty() || search_start >= sql.len() {
        return None;
    }

    let search_text = &sql[search_start..];

    // Find closing paren anchors
    let mut pos = 0;
    while pos < search_text.len() {
        if let Some(paren_pos) = search_text[pos..].find(')') {
            let after_paren = pos + paren_pos + 1;
            // Skip whitespace and comments
            let ws_end = skip_whitespace_and_comments(search_text, after_paren);

            if ws_end >= search_text.len() {
                pos = after_paren;
                continue;
            }

            // Check for optional "AS" keyword (must be followed by whitespace or comment, not "ASC")
            let after_as = if search_text[ws_end..].to_ascii_uppercase().starts_with("AS") {
                let potential_as_end = ws_end + 2;
                let is_standalone_as = potential_as_end >= search_text.len()
                    || search_text.as_bytes()[potential_as_end].is_ascii_whitespace()
                    || search_text[potential_as_end..].starts_with("/*")
                    || search_text[potential_as_end..].starts_with("--");
                if is_standalone_as {
                    skip_whitespace_and_comments(search_text, potential_as_end)
                } else {
                    ws_end
                }
            } else {
                ws_end
            };

            if let Some((start, end)) = match_identifier_at(search_text, after_as, identifier) {
                return Some(Span::new(search_start + start, search_start + end));
            }
            pos = after_paren;
            continue;
        }
        break;
    }

    None
}

/// Finds a keyword case-insensitively with word boundary check.
fn find_keyword_case_insensitive(text: &str, keyword: &str) -> Option<usize> {
    let text_upper = text.to_ascii_uppercase();
    let mut search_pos = 0;

    while let Some(pos) = text_upper[search_pos..].find(keyword) {
        let abs_pos = search_pos + pos;
        // Check word boundary before
        let before_ok = abs_pos == 0 || !is_identifier_char_before(text, abs_pos);
        // Check word boundary after
        let after_pos = abs_pos + keyword.len();
        let after_ok = after_pos >= text.len() || !is_identifier_char_at(text, after_pos);

        if before_ok && after_ok {
            return Some(abs_pos);
        }
        search_pos = abs_pos + 1;
    }
    None
}

/// Skips whitespace and SQL comments (block `/* */` and line `-- \n`).
/// Returns the position after all whitespace and comments.
fn skip_whitespace_and_comments(text: &str, pos: usize) -> usize {
    let mut current = pos;

    loop {
        if current >= text.len() {
            return current;
        }

        let remaining = &text[current..];

        // Skip whitespace first
        let ws_chars: usize = remaining
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(|c| c.len_utf8())
            .sum();
        if ws_chars > 0 {
            current += ws_chars;
            continue;
        }

        // Check for block comment /* ... */
        if let Some(after_open) = remaining.strip_prefix("/*") {
            if let Some(end) = after_open.find("*/") {
                current += 2 + end + 2; // Skip /* + content + */
                continue;
            } else {
                // Unclosed comment - skip to end
                return text.len();
            }
        }

        // Check for line comment -- ... \n
        if remaining.starts_with("--") {
            if let Some(newline) = remaining.find('\n') {
                current += newline + 1;
                continue;
            } else {
                // No newline - comment goes to end
                return text.len();
            }
        }

        // No more whitespace or comments
        break;
    }

    current
}

/// Matches an identifier at the given position (case-insensitive, handles quoting).
fn match_identifier_at(text: &str, pos: usize, identifier: &str) -> Option<(usize, usize)> {
    if pos >= text.len() {
        return None;
    }

    let remaining = &text[pos..];
    let ident_upper = identifier.to_ascii_uppercase();

    // Check for quoted variants first
    for (open, close) in [("\"", "\""), ("`", "`"), ("[", "]")] {
        if remaining.starts_with(open) {
            let after_open = open.len();
            if remaining[after_open..]
                .to_ascii_uppercase()
                .starts_with(&ident_upper)
            {
                let ident_end = after_open + identifier.len();
                if remaining[ident_end..].starts_with(close) {
                    return Some((pos + after_open, pos + ident_end));
                }
            }
        }
    }

    // Check for unquoted identifier with word boundary
    if remaining.to_ascii_uppercase().starts_with(&ident_upper) {
        let end_pos = identifier.len();
        // Ensure word boundary after identifier.
        let after_ok = end_pos >= remaining.len() || !is_identifier_char_at(remaining, end_pos);
        if after_ok {
            return Some((pos, pos + identifier.len()));
        }
    }

    None
}

/// Finds an identifier occurrence in text, skipping comments and string literals.
fn find_qualified_name(text: &str, qualified_name: &str) -> Option<(usize, usize)> {
    find_identifier_occurrence(text, qualified_name, 0)
        .map(|occurrence| (occurrence.full_start, occurrence.full_end))
}

#[derive(Debug, Clone, Copy)]
struct IdentifierOccurrence {
    full_start: usize,
    full_end: usize,
    tail_start: usize,
    tail_end: usize,
}

#[derive(Debug, Clone, Copy)]
struct ParsedIdentifierPart {
    content_start: usize,
    content_end: usize,
    full_end: usize,
}

fn find_identifier_occurrence(
    text: &str,
    identifier: &str,
    search_start: usize,
) -> Option<IdentifierOccurrence> {
    if identifier.is_empty() || search_start >= text.len() || !text.is_char_boundary(search_start) {
        return None;
    }

    let target_parts = parse_identifier_target_parts(identifier)?;
    let bytes = text.as_bytes();
    let mut cursor = search_start;
    while cursor < bytes.len() {
        if let Some(skip_to) = skip_string_or_comment(bytes, cursor) {
            debug_assert!(skip_to > cursor);
            if skip_to <= cursor {
                return None;
            }
            cursor = skip_to;
            continue;
        }

        if let Some(occurrence) = match_identifier_occurrence_at(text, cursor, &target_parts) {
            return Some(occurrence);
        }

        cursor = advance_scan_cursor(text, cursor)?;
    }

    None
}

fn parse_identifier_target_parts(identifier: &str) -> Option<Vec<String>> {
    let target_parts: Vec<String> = split_qualified_identifiers(identifier)
        .into_iter()
        .map(|part| unquote_identifier(&part))
        .collect();
    (!target_parts.is_empty() && target_parts.iter().all(|part| !part.is_empty()))
        .then_some(target_parts)
}

fn match_identifier_occurrence_at(
    text: &str,
    start: usize,
    target_parts: &[String],
) -> Option<IdentifierOccurrence> {
    if !text.is_char_boundary(start) {
        return None;
    }

    let before_ok = start == 0 || !is_identifier_char_before(text, start);
    if !before_ok {
        return None;
    }

    let (full_end, tail_start, tail_end) = match_identifier_sequence_at(text, start, target_parts)?;
    let after_ok = full_end == text.len() || !is_identifier_char_at(text, full_end);
    after_ok.then_some(IdentifierOccurrence {
        full_start: start,
        full_end,
        tail_start,
        tail_end,
    })
}

fn advance_scan_cursor(text: &str, cursor: usize) -> Option<usize> {
    if !text.is_char_boundary(cursor) {
        return Some(cursor + 1);
    }
    let ch = text.get(cursor..)?.chars().next()?;
    Some(cursor + ch.len_utf8())
}

fn match_identifier_sequence_at(
    text: &str,
    start: usize,
    parts: &[String],
) -> Option<(usize, usize, usize)> {
    if parts.is_empty() {
        return None;
    }

    let bytes = text.as_bytes();
    let mut current = start;
    let mut tail = None;

    for (idx, part) in parts.iter().enumerate() {
        let parsed = match_identifier_part_at(text, current, part)?;

        if idx == parts.len() - 1 {
            tail = Some((parsed.content_start, parsed.content_end));
        }

        current = parsed.full_end;
        if idx < parts.len() - 1 {
            current = skip_whitespace_and_comments(text, current);
            if current >= bytes.len() || bytes[current] != b'.' {
                return None;
            }
            current = skip_whitespace_and_comments(text, current + 1);
        }
    }

    let (tail_start, tail_end) = tail?;
    Some((current, tail_start, tail_end))
}

fn match_identifier_part_at(
    text: &str,
    start: usize,
    target_part: &str,
) -> Option<ParsedIdentifierPart> {
    if start >= text.len() || !text.is_char_boundary(start) || target_part.is_empty() {
        return None;
    }

    let bytes = text.as_bytes();
    match bytes[start] {
        b'"' | b'`' | b'[' | b'\'' => {
            let (close_quote, content_start) = match bytes[start] {
                b'"' => (b'"', start + 1),
                b'`' => (b'`', start + 1),
                b'[' => (b']', start + 1),
                b'\'' => (b'\'', start + 1),
                _ => unreachable!(),
            };

            let mut i = content_start;
            while i < bytes.len() {
                if bytes[i] == close_quote {
                    if i + 1 < bytes.len() && bytes[i + 1] == close_quote {
                        i += 2;
                        continue;
                    }
                    let candidate = text.get(content_start..i)?;
                    if !candidate.eq_ignore_ascii_case(target_part) {
                        return None;
                    }
                    return Some(ParsedIdentifierPart {
                        content_start,
                        content_end: i,
                        full_end: i + 1,
                    });
                }
                i += 1;
            }
            None
        }
        _ if target_part.chars().all(is_identifier_char) => {
            let mut end = start;
            for ch in text.get(start..)?.chars() {
                if !is_identifier_char(ch) {
                    break;
                }
                end += ch.len_utf8();
            }
            let candidate = text.get(start..end)?;
            candidate
                .eq_ignore_ascii_case(target_part)
                .then_some(ParsedIdentifierPart {
                    content_start: start,
                    content_end: end,
                    full_end: end,
                })
        }
        _ => {
            let end = start + target_part.len();
            let candidate = text.get(start..end)?;
            candidate
                .eq_ignore_ascii_case(target_part)
                .then_some(ParsedIdentifierPart {
                    content_start: start,
                    content_end: end,
                    full_end: end,
                })
        }
    }
}

fn consume_ascii_keyword(text: &str, pos: usize, keyword: &str) -> Option<usize> {
    let pos = skip_whitespace_and_comments(text, pos);
    let remaining = text.get(pos..)?;
    if find_keyword_case_insensitive(remaining, keyword) != Some(0) {
        return None;
    }
    Some(skip_whitespace_and_comments(text, pos + keyword.len()))
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn is_identifier_char_before(text: &str, byte_offset: usize) -> bool {
    text.get(..byte_offset)
        .and_then(|prefix| prefix.chars().next_back())
        .is_some_and(is_identifier_char)
}

fn is_identifier_char_at(text: &str, byte_offset: usize) -> bool {
    text.get(byte_offset..)
        .and_then(|suffix| suffix.chars().next())
        .is_some_and(is_identifier_char)
}

/// Calculates the byte offset for a given line and column in SQL text.
///
/// This is useful for converting line:column positions (from parse errors)
/// to byte offsets for the Span type.
///
/// # Arguments
///
/// * `sql` - The SQL source text
/// * `line` - Line number (1-indexed)
/// * `column` - Column number (1-indexed)
pub fn line_col_to_offset(sql: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }

    let bytes = sql.as_bytes();
    let mut current_line = 1;
    let mut offset = 0;

    // Advance `offset` to the start of the requested line.
    while current_line < line {
        let remaining = bytes.get(offset..)?;
        let newline_pos = remaining.iter().position(|&b| b == b'\n')?;
        offset += newline_pos + 1;
        current_line += 1;
    }

    let line_start = offset;
    let remaining = bytes.get(line_start..)?;
    let line_len = remaining
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(remaining.len());
    let line_end = line_start + line_len;
    let line_slice = &sql[line_start..line_end];

    // sqlparser reports columns in characters, so iterate char_indices to convert
    // the 1-based column into a byte offset.
    let mut current_column = 1;
    for (rel_offset, _) in line_slice.char_indices() {
        if current_column == column {
            return Some(line_start + rel_offset);
        }
        current_column += 1;
    }

    if column == current_column {
        return Some(line_end);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_identifier_span_simple() {
        let sql = "SELECT * FROM users WHERE id = 1";
        let span = find_identifier_span(sql, "users", 0);
        assert_eq!(span, Some(Span::new(14, 19)));
    }

    #[test]
    fn test_find_identifier_span_case_insensitive() {
        let sql = "SELECT * FROM Users WHERE id = 1";
        let span = find_identifier_span(sql, "users", 0);
        assert!(span.is_some());
    }

    #[test]
    fn test_find_identifier_span_qualified() {
        let sql = "SELECT * FROM public.users";
        let span = find_identifier_span(sql, "public.users", 0);
        assert_eq!(span, Some(Span::new(14, 26)));
    }

    #[test]
    fn test_find_identifier_span_qualified_with_quotes() {
        let sql = r#"SELECT * FROM "Public"."Users""#;
        let span = find_identifier_span(sql, "public.users", 0);
        assert_eq!(span, Some(Span::new(14, 30)));
        let span = span.expect("quoted qualified span");
        assert_eq!(&sql[span.start..span.end], r#""Public"."Users""#);
    }

    #[test]
    fn test_find_identifier_span_with_offset() {
        let sql = "SELECT 1; SELECT * FROM users";
        let span = find_identifier_span(sql, "users", 10);
        assert_eq!(span, Some(Span::new(24, 29)));
    }

    #[test]
    fn test_find_identifier_span_not_found() {
        let sql = "SELECT * FROM users";
        let span = find_identifier_span(sql, "orders", 0);
        assert_eq!(span, None);
    }

    #[test]
    fn test_find_identifier_word_boundary() {
        let sql = "SELECT users_id FROM users";
        // Should find "users" as whole word, not "users" in "users_id"
        let span = find_identifier_span(sql, "users", 0);
        assert!(span.is_some());
        let span = span.unwrap();
        // Should match the standalone "users", not the one in "users_id"
        assert_eq!(&sql[span.start..span.end].to_lowercase(), "users");
    }

    #[test]
    fn test_find_cte_definition_span_single() {
        let sql = "WITH my_cte AS (SELECT 1) SELECT * FROM my_cte";
        let span = find_cte_definition_span(sql, "my_cte", 0);
        assert_eq!(span, Some(Span::new(5, 11)));
    }

    #[test]
    fn test_find_cte_definition_span_multiple() {
        let sql = "WITH cte1 AS (SELECT 1), cte2 AS (SELECT 2) SELECT * FROM cte1, cte2";
        let first_span = find_cte_definition_span(sql, "cte1", 0).expect("cte1 span");
        assert_eq!(first_span, Span::new(5, 9));

        let second_span = find_cte_definition_span(sql, "cte2", first_span.end).expect("cte2 span");
        assert_eq!(second_span, Span::new(25, 29));
    }

    #[test]
    fn test_find_derived_table_alias_span() {
        let sql = "SELECT * FROM (SELECT 1) AS derived";
        let span = find_derived_table_alias_span(sql, "derived", 0);
        assert_eq!(span, Some(Span::new(28, 35)));
        let span = span.expect("derived span");
        assert_eq!(&sql[span.start..span.end], "derived");
    }

    #[test]
    fn test_find_cte_definition_span_quoted() {
        // Double-quoted identifier
        let sql = r#"WITH "MyCte" AS (SELECT 1) SELECT * FROM "MyCte""#;
        let span = find_cte_definition_span(sql, "MyCte", 0);
        assert!(span.is_some(), "should find quoted CTE");
        let span = span.unwrap();
        assert_eq!(&sql[span.start..span.end], "MyCte");

        // Backtick-quoted identifier
        let sql = "WITH `my_cte` AS (SELECT 1) SELECT * FROM `my_cte`";
        let span = find_cte_definition_span(sql, "my_cte", 0);
        assert!(span.is_some(), "should find backtick-quoted CTE");
        let span = span.unwrap();
        assert_eq!(&sql[span.start..span.end], "my_cte");

        // Bracket-quoted identifier
        let sql = "WITH [my_cte] AS (SELECT 1) SELECT * FROM [my_cte]";
        let span = find_cte_definition_span(sql, "my_cte", 0);
        assert!(span.is_some(), "should find bracket-quoted CTE");
        let span = span.unwrap();
        assert_eq!(&sql[span.start..span.end], "my_cte");
    }

    #[test]
    fn test_find_derived_table_alias_span_without_as() {
        // Derived table without AS keyword
        let sql = "SELECT * FROM (SELECT 1) derived";
        let span = find_derived_table_alias_span(sql, "derived", 0);
        assert!(span.is_some(), "should find derived alias without AS");
        let span = span.unwrap();
        assert_eq!(&sql[span.start..span.end], "derived");
    }

    #[test]
    fn test_find_derived_table_alias_span_multiple() {
        let sql = "SELECT * FROM (SELECT 1) AS a, (SELECT 2) AS b";
        let first_span = find_derived_table_alias_span(sql, "a", 0).expect("first derived span");
        assert_eq!(&sql[first_span.start..first_span.end], "a");

        let second_span =
            find_derived_table_alias_span(sql, "b", first_span.end).expect("second derived span");
        assert_eq!(&sql[second_span.start..second_span.end], "b");
    }

    #[test]
    fn test_find_derived_table_alias_span_quoted() {
        let sql = r#"SELECT * FROM (SELECT 1) AS "Derived""#;
        let span = find_derived_table_alias_span(sql, "Derived", 0);
        assert!(span.is_some(), "should find quoted derived alias");
        let span = span.unwrap();
        assert_eq!(&sql[span.start..span.end], "Derived");
    }

    #[test]
    fn test_line_col_to_offset_single_line() {
        let sql = "SELECT * FROM users";
        assert_eq!(line_col_to_offset(sql, 1, 1), Some(0));
        assert_eq!(line_col_to_offset(sql, 1, 8), Some(7));
    }

    #[test]
    fn test_line_col_to_offset_multi_line() {
        let sql = "SELECT *\nFROM users\nWHERE id = 1";
        assert_eq!(line_col_to_offset(sql, 1, 1), Some(0));
        assert_eq!(line_col_to_offset(sql, 2, 1), Some(9));
        assert_eq!(line_col_to_offset(sql, 3, 1), Some(20));
    }

    #[test]
    fn test_line_col_to_offset_unicode_columns() {
        let sql = "SELECT μ, FROM users";
        // Column 11 should point at the 'F' byte even though the line includes a multi-byte char.
        assert_eq!(line_col_to_offset(sql, 1, 11), Some("SELECT μ, ".len()));
        // Column 12 moves one character to the right (the 'R').
        assert_eq!(line_col_to_offset(sql, 1, 12), Some("SELECT μ, F".len()));
    }

    #[test]
    fn test_line_col_to_offset_invalid() {
        let sql = "SELECT * FROM users";
        assert_eq!(line_col_to_offset(sql, 0, 1), None);
        assert_eq!(line_col_to_offset(sql, 1, 0), None);
        assert_eq!(line_col_to_offset(sql, 5, 1), None);
    }

    #[test]
    fn test_find_identifier_empty() {
        let sql = "SELECT * FROM users";
        assert_eq!(find_identifier_span(sql, "", 0), None);
        assert_eq!(find_identifier_span("", "users", 0), None);
    }

    // ============================================================================
    // Regression tests for prior code review findings
    // ============================================================================

    // Issue 1: WITH RECURSIVE not supported
    #[test]
    fn test_find_cte_definition_span_recursive() {
        let sql = "WITH RECURSIVE my_cte AS (SELECT 1 UNION ALL SELECT 2) SELECT * FROM my_cte";
        let span = find_cte_definition_span(sql, "my_cte", 0);
        assert!(
            span.is_some(),
            "should find CTE name after RECURSIVE keyword"
        );
        let span = span.unwrap();
        assert_eq!(&sql[span.start..span.end], "my_cte");
    }

    #[test]
    fn test_find_cte_definition_span_recursive_multiple() {
        let sql = "WITH RECURSIVE cte1 AS (SELECT 1), cte2 AS (SELECT 2) SELECT * FROM cte1, cte2";
        let first_span = find_cte_definition_span(sql, "cte1", 0);
        assert!(
            first_span.is_some(),
            "should find first CTE after RECURSIVE"
        );
        let first_span = first_span.unwrap();
        assert_eq!(&sql[first_span.start..first_span.end], "cte1");

        let second_span = find_cte_definition_span(sql, "cte2", first_span.end);
        assert!(second_span.is_some(), "should find second CTE after comma");
        let second_span = second_span.unwrap();
        assert_eq!(&sql[second_span.start..second_span.end], "cte2");
    }

    // Issue 2: Bounds checking - search_start at end of string
    #[test]
    fn test_find_cte_definition_span_search_start_at_end() {
        let sql = "WITH cte AS (SELECT 1) SELECT * FROM cte";
        // Search starting at the very end should return None, not panic
        let span = find_cte_definition_span(sql, "cte", sql.len());
        assert_eq!(span, None);
    }

    #[test]
    fn test_find_derived_table_alias_search_start_at_end() {
        let sql = "SELECT * FROM (SELECT 1) AS derived";
        // Search starting at the very end should return None, not panic
        let span = find_derived_table_alias_span(sql, "derived", sql.len());
        assert_eq!(span, None);
    }

    #[test]
    fn test_find_derived_table_alias_paren_at_end() {
        // Edge case: closing paren at end with no alias
        let sql = "SELECT * FROM (SELECT 1)";
        let span = find_derived_table_alias_span(sql, "anything", 0);
        assert_eq!(span, None);
    }

    // Issue 3: Word boundary logic - underscore handling
    #[test]
    fn test_word_boundary_underscore_prefix() {
        let sql = "SELECT * FROM _users";
        // Should find "_users" as identifier, not fail to match
        let span = find_identifier_span(sql, "_users", 0);
        assert!(
            span.is_some(),
            "should find identifier starting with underscore"
        );
    }

    #[test]
    fn test_word_boundary_underscore_suffix_no_match() {
        let sql = "SELECT * FROM users_table";
        // Should NOT match "users" because it's followed by underscore
        let span = find_identifier_span(sql, "users", 0);
        // This tests the bug: the current code may incorrectly match "users" within "users_table"
        // because of operator precedence: `!x && y != z` instead of `!(x || y == z)`
        assert!(
            span.is_none() || {
                let s = span.unwrap();
                // If it matched, verify it's the whole word not a prefix
                s.end == s.start + "users".len()
                    && (s.end >= sql.len()
                        || !sql.as_bytes()[s.end].is_ascii_alphanumeric()
                            && sql.as_bytes()[s.end] != b'_')
            },
            "should not match 'users' as part of 'users_table'"
        );
    }

    #[test]
    fn test_word_boundary_unicode_suffix_no_match() {
        let sql = "SELECT * FROM 表名";
        // Should NOT match "表" because it's followed by a Unicode identifier char.
        let span = find_identifier_span(sql, "表", 0);
        assert_eq!(span, None);
    }

    #[test]
    fn test_word_boundary_unicode_search_offset_no_partial_match() {
        let sql = "SELECT 表 FROM 表名";
        let first = find_identifier_span(sql, "表", 0).expect("first 表");
        let second = find_identifier_span(sql, "表", first.end);
        assert_eq!(second, None);
    }

    #[test]
    fn test_cte_name_with_underscore_suffix_no_match() {
        // When searching for "cte" it should not match "cte_name"
        let sql = "WITH cte_name AS (SELECT 1) SELECT * FROM cte_name";
        let span = find_cte_definition_span(sql, "cte", 0);
        assert!(
            span.is_none(),
            "should not match 'cte' as part of 'cte_name'"
        );
    }

    // Issue 4: Comments not handled
    #[test]
    fn test_find_cte_definition_span_with_block_comment() {
        let sql = "WITH /* comment */ my_cte AS (SELECT 1) SELECT * FROM my_cte";
        let span = find_cte_definition_span(sql, "my_cte", 0);
        assert!(span.is_some(), "should find CTE name after block comment");
        let span = span.unwrap();
        assert_eq!(&sql[span.start..span.end], "my_cte");
    }

    #[test]
    fn test_find_cte_definition_span_with_line_comment() {
        let sql = "WITH -- comment\nmy_cte AS (SELECT 1) SELECT * FROM my_cte";
        let span = find_cte_definition_span(sql, "my_cte", 0);
        assert!(span.is_some(), "should find CTE name after line comment");
        let span = span.unwrap();
        assert_eq!(&sql[span.start..span.end], "my_cte");
    }

    #[test]
    fn test_find_derived_table_alias_with_comment() {
        let sql = "SELECT * FROM (SELECT 1) /* comment */ AS derived";
        let span = find_derived_table_alias_span(sql, "derived", 0);
        assert!(span.is_some(), "should find alias after block comment");
        let span = span.unwrap();
        assert_eq!(&sql[span.start..span.end], "derived");
    }

    // Issue 5: String literals may contain false matches
    #[test]
    fn test_find_cte_definition_not_in_string_literal() {
        // The CTE name "cte" appears in a string literal first, then as actual CTE
        let sql = "WITH cte AS (SELECT 'cte' AS name) SELECT * FROM cte";
        let span = find_cte_definition_span(sql, "cte", 0);
        assert!(span.is_some(), "should find CTE definition");
        let span = span.unwrap();
        // Should find the definition at position 5, not the string literal
        assert_eq!(
            span.start, 5,
            "should find CTE definition, not string literal"
        );
        assert_eq!(&sql[span.start..span.end], "cte");
    }

    #[test]
    fn test_find_derived_alias_not_in_string_literal() {
        // The alias appears in a string literal inside the subquery
        let sql = "SELECT * FROM (SELECT 'derived' AS name) AS derived";
        let span = find_derived_table_alias_span(sql, "derived", 0);
        assert!(span.is_some(), "should find derived alias");
        let span = span.unwrap();
        // Should find the actual alias after the closing paren, not the string
        assert_eq!(&sql[span.start..span.end], "derived");
        // The alias position should be after the closing paren
        assert!(
            span.start > sql.find(')').unwrap(),
            "span should be after closing paren"
        );
    }

    // Issue 6: Edge cases for empty/malformed inputs
    #[test]
    fn test_find_cte_definition_empty_identifier() {
        let sql = "WITH cte AS (SELECT 1) SELECT * FROM cte";
        let span = find_cte_definition_span(sql, "", 0);
        assert_eq!(span, None, "empty identifier should return None");
    }

    #[test]
    fn test_find_derived_table_alias_empty_identifier() {
        let sql = "SELECT * FROM (SELECT 1) AS derived";
        let span = find_derived_table_alias_span(sql, "", 0);
        assert_eq!(span, None, "empty identifier should return None");
    }

    #[test]
    fn test_find_cte_definition_empty_sql() {
        let span = find_cte_definition_span("", "cte", 0);
        assert_eq!(span, None, "empty SQL should return None");
    }

    #[test]
    fn test_find_derived_table_alias_empty_sql() {
        let span = find_derived_table_alias_span("", "derived", 0);
        assert_eq!(span, None, "empty SQL should return None");
    }

    #[test]
    fn test_find_cte_definition_search_start_beyond_bounds() {
        let sql = "WITH cte AS (SELECT 1)";
        let span = find_cte_definition_span(sql, "cte", sql.len() + 100);
        assert_eq!(span, None, "search_start beyond bounds should return None");
    }

    #[test]
    fn test_find_derived_table_alias_search_start_beyond_bounds() {
        let sql = "SELECT * FROM (SELECT 1) AS derived";
        let span = find_derived_table_alias_span(sql, "derived", sql.len() + 100);
        assert_eq!(span, None, "search_start beyond bounds should return None");
    }

    // Additional edge case: identifier at very end of SQL
    #[test]
    fn test_find_cte_at_end_of_sql() {
        let sql = "WITH x AS (SELECT 1) SELECT * FROM x";
        let span = find_cte_definition_span(sql, "x", 0);
        assert!(span.is_some());
        let span = span.unwrap();
        assert_eq!(&sql[span.start..span.end], "x");
    }

    // Test for potential panic in match_identifier_at with short remaining text
    #[test]
    fn test_match_identifier_at_short_remaining() {
        let sql = "WITH a AS (SELECT 1) SELECT * FROM a";
        let span = find_cte_definition_span(sql, "a", 0);
        assert!(span.is_some());
        let span = span.unwrap();
        assert_eq!(&sql[span.start..span.end], "a");
    }

    #[test]
    fn test_find_all_identifier_spans_multiple_refs() {
        let sql = "SELECT * FROM users u WHERE u.id IN (SELECT id FROM users)";
        let spans = find_all_identifier_spans(sql, "users", 0, sql.len());
        assert_eq!(spans.len(), 2);
        for span in &spans {
            assert_eq!(&sql[span.start..span.end], "users");
        }
    }

    #[test]
    fn test_find_all_identifier_spans_cte_declaration_and_refs() {
        let sql = "WITH a AS (SELECT 1) SELECT a.x FROM a";
        let spans = find_all_identifier_spans(sql, "a", 0, sql.len());
        // `a` appears three times: declaration, qualifier in `a.x`, and `FROM a`.
        assert_eq!(spans.len(), 3);
        assert!(spans
            .iter()
            .all(|s| &sql[s.start..s.end] == "a" && s.end > s.start));
    }

    #[test]
    fn test_find_all_identifier_spans_ignores_string_literals_and_comments() {
        let sql = "SELECT * FROM users WHERE name = 'users' -- users\n/* users */";
        let spans = find_all_identifier_spans(sql, "users", 0, sql.len());
        // Only the `FROM users` occurrence should match.
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 14);
        assert_eq!(spans[0].end, 19);
    }

    #[test]
    fn test_find_all_identifier_spans_word_boundary() {
        let sql = "SELECT * FROM users_archive, users";
        let spans = find_all_identifier_spans(sql, "users", 0, sql.len());
        // `users_archive` must not match.
        assert_eq!(spans.len(), 1);
        assert_eq!(&sql[spans[0].start..spans[0].end], "users");
    }

    #[test]
    fn test_find_all_identifier_spans_case_insensitive() {
        let sql = "SELECT * FROM Users JOIN USERS u ON u.id = Users.id";
        let spans = find_all_identifier_spans(sql, "users", 0, sql.len());
        assert_eq!(spans.len(), 3);
    }

    #[test]
    fn test_find_all_identifier_spans_respects_search_bounds() {
        let sql = "users users users";
        let spans = find_all_identifier_spans(sql, "users", 6, 12);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 6);
        assert_eq!(spans[0].end, 11);
    }

    #[test]
    fn test_find_cte_body_span_simple() {
        let sql = "WITH a AS (SELECT 1) SELECT * FROM a";
        // The name span for `a` at offset 5.
        let name_span = Span::new(5, 6);
        let body = find_cte_body_span(sql, name_span).expect("body span");
        assert_eq!(&sql[body.start..body.end], "(SELECT 1)");
    }

    #[test]
    fn test_find_cte_body_span_nested_parens() {
        let sql = "WITH a AS (SELECT (1 + 2) AS x) SELECT * FROM a";
        let name_span = Span::new(5, 6);
        let body = find_cte_body_span(sql, name_span).expect("body span");
        assert_eq!(&sql[body.start..body.end], "(SELECT (1 + 2) AS x)");
    }

    #[test]
    fn test_find_cte_body_span_paren_in_string_literal() {
        let sql = "WITH a AS (SELECT ')' AS c) SELECT * FROM a";
        let name_span = Span::new(5, 6);
        let body = find_cte_body_span(sql, name_span).expect("body span");
        assert_eq!(&sql[body.start..body.end], "(SELECT ')' AS c)");
    }

    #[test]
    fn test_find_cte_body_span_missing_paren_returns_none() {
        // No parenthesized body after the name.
        let sql = "WITH a AS SELECT 1";
        let name_span = Span::new(5, 6);
        assert_eq!(find_cte_body_span(sql, name_span), None);
    }

    #[test]
    fn test_find_cte_body_span_with_whitespace_and_comment() {
        let sql = "WITH a  /* note */ AS  (SELECT 1) SELECT * FROM a";
        let name_span = Span::new(5, 6);
        let body = find_cte_body_span(sql, name_span).expect("body span");
        assert_eq!(&sql[body.start..body.end], "(SELECT 1)");
    }

    #[test]
    fn test_find_cte_body_span_with_column_list() {
        let sql = "WITH a(x, y) AS (SELECT 1, 2) SELECT * FROM a";
        let name_span = Span::new(5, 6);
        let body = find_cte_body_span(sql, name_span).expect("body span");
        assert_eq!(&sql[body.start..body.end], "(SELECT 1, 2)");
    }

    #[test]
    fn test_find_cte_body_span_with_materialized_modifier() {
        let sql = "WITH a AS MATERIALIZED (SELECT 1) SELECT * FROM a";
        let name_span = Span::new(5, 6);
        let body = find_cte_body_span(sql, name_span).expect("body span");
        assert_eq!(&sql[body.start..body.end], "(SELECT 1)");
    }

    #[test]
    fn test_find_cte_body_span_with_not_materialized_modifier() {
        let sql = "WITH a AS NOT MATERIALIZED (SELECT 1) SELECT * FROM a";
        let name_span = Span::new(5, 6);
        let body = find_cte_body_span(sql, name_span).expect("body span");
        assert_eq!(&sql[body.start..body.end], "(SELECT 1)");
    }

    #[test]
    fn test_find_identifier_span_skips_string_literal_before_match() {
        let sql = "SELECT 'users' AS x FROM users";
        let span = find_identifier_span(sql, "users", 0).expect("users span");
        assert_eq!(&sql[span.start..span.end], "users");
        assert_eq!(span, Span::new(25, 30));
    }

    #[test]
    fn test_find_relation_occurrence_spans_quoted_identifier_with_embedded_dots() {
        let sql = "SELECT * FROM \"my.schema\".\"my.table\"";
        let (full_span, name_span) =
            find_relation_occurrence_spans(sql, "\"my.schema\".\"my.table\"", 0)
                .expect("relation span");
        assert_eq!(
            &sql[full_span.start..full_span.end],
            "\"my.schema\".\"my.table\""
        );
        assert_eq!(&sql[name_span.start..name_span.end], "my.table");
    }

    // ============================================================================
    // UTF-8 safety regression tests: multi-byte characters in comments, string
    // literals, and around the scan region must not cause panics. Prior to the
    // byte-based refactor, the helpers would `sql[pos..]` with a cursor advancing
    // one byte at a time, which panics when `pos` lands inside a multi-byte char.
    // ============================================================================

    #[test]
    fn test_find_all_identifier_spans_skips_non_ascii_comment() {
        // A block comment containing a multi-byte character (µ, 2 bytes in UTF-8)
        // previously caused a panic because the byte-indexed cursor slicing into
        // `sql[pos..]` would land inside the multi-byte sequence.
        let sql = "SELECT * /* µ µµµ */ FROM users WHERE id = 1";
        let spans = find_all_identifier_spans(sql, "users", 0, sql.len());
        assert_eq!(spans.len(), 1);
        assert_eq!(&sql[spans[0].start..spans[0].end], "users");
    }

    #[test]
    fn test_find_all_identifier_spans_skips_non_ascii_line_comment() {
        let sql = "SELECT * FROM users -- é comment\nJOIN users u";
        let spans = find_all_identifier_spans(sql, "users", 0, sql.len());
        assert_eq!(spans.len(), 2);
    }

    #[test]
    fn test_find_all_identifier_spans_skips_non_ascii_string_literal() {
        // Multi-byte char inside a string literal.
        let sql = "SELECT 'héllo users' FROM users";
        let spans = find_all_identifier_spans(sql, "users", 0, sql.len());
        // The `users` inside the string literal must not match; only the real FROM reference.
        assert_eq!(spans.len(), 1);
        assert_eq!(&sql[spans[0].start..spans[0].end], "users");
    }

    #[test]
    fn test_find_cte_body_span_with_non_ascii_body_contents() {
        let sql = "WITH a AS (SELECT 'µ' AS x, (1 + 2) AS y) SELECT * FROM a";
        let name_span = Span::new(5, 6);
        let body = find_cte_body_span(sql, name_span).expect("body span");
        assert_eq!(
            &sql[body.start..body.end],
            "(SELECT 'µ' AS x, (1 + 2) AS y)"
        );
    }

    #[test]
    fn test_find_all_identifier_spans_non_ascii_between_occurrences() {
        // Multi-byte chars between identifier occurrences stress the scan cursor.
        let sql = "SELECT users.id -- µ\nFROM users /* ñ */ JOIN users";
        let spans = find_all_identifier_spans(sql, "users", 0, sql.len());
        assert_eq!(spans.len(), 3);
    }

    #[test]
    fn test_find_all_identifier_spans_ignores_hash_comments() {
        let sql = "SELECT 1 # users\nFROM users";
        let spans = find_all_identifier_spans(sql, "users", 0, sql.len());
        assert_eq!(spans.len(), 1);
        assert_eq!(&sql[spans[0].start..spans[0].end], "users");
        assert_eq!(spans[0], Span::new(22, 27));
    }

    #[test]
    fn test_find_identifier_span_skips_dollar_quoted_string_literal() {
        let sql = "SELECT $$users$$ AS x FROM users";
        let span = find_identifier_span(sql, "users", 0).expect("users span");
        assert_eq!(&sql[span.start..span.end], "users");
        assert_eq!(span, Span::new(27, 32));
    }

    #[test]
    fn test_find_cte_body_span_with_dollar_quoted_string() {
        let sql = "WITH a AS (SELECT $$)$$ AS x) SELECT * FROM a";
        let name_span = Span::new(5, 6);
        let body = find_cte_body_span(sql, name_span).expect("body span");
        assert_eq!(&sql[body.start..body.end], "(SELECT $$)$$ AS x)");
    }
}
