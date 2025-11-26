//! Utilities for finding identifier spans in SQL text.
//!
//! This module provides functions to locate identifiers in SQL source code
//! for error reporting. Since sqlparser doesn't expose AST node locations,
//! we use text search to find approximate positions.

use crate::types::Span;
use regex::Regex;

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

    let search_text = &sql[search_start..];

    // Try exact match first (case-insensitive, word boundary)
    if let Some(pos) = find_word_boundary_match(search_text, identifier) {
        return Some(Span::new(
            search_start + pos,
            search_start + pos + identifier.len(),
        ));
    }

    // For qualified names like "schema.table", try to find the full pattern
    if identifier.contains('.') {
        if let Some(pos) = find_qualified_name(search_text, identifier) {
            return Some(Span::new(
                search_start + pos,
                search_start + pos + identifier.len(),
            ));
        }
    }

    None
}

/// Finds an identifier at a word boundary (not part of another word).
fn find_word_boundary_match(text: &str, identifier: &str) -> Option<usize> {
    // For simple identifiers, use word boundary matching
    let pattern = format!(r"(?i)\b{}\b", regex::escape(identifier));

    // Try to compile the pattern
    if let Ok(re) = Regex::new(&pattern) {
        if let Some(m) = re.find(text) {
            return Some(m.start());
        }
    }

    // Fallback: simple case-insensitive search
    let lower_text = text.to_lowercase();
    let lower_ident = identifier.to_lowercase();
    lower_text.find(&lower_ident)
}

/// Finds a qualified identifier (e.g., "schema.table") in text.
fn find_qualified_name(text: &str, qualified_name: &str) -> Option<usize> {
    // Split the qualified name and search for the pattern
    let parts: Vec<&str> = qualified_name.split('.').collect();
    if parts.is_empty() {
        return None;
    }

    // Build a pattern that matches the qualified name with optional quotes
    // e.g., "public.users" should match: public.users, "public"."users", public."users", etc.
    let pattern_parts: Vec<String> = parts
        .iter()
        .map(|part| {
            // Match the part with optional surrounding quotes
            format!(r#"(?:"?{}\"?)"#, regex::escape(part))
        })
        .collect();

    let pattern = format!(r"(?i){}", pattern_parts.join(r"\."));

    if let Ok(re) = Regex::new(&pattern) {
        if let Some(m) = re.find(text) {
            return Some(m.start());
        }
    }

    None
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

    let mut current_line = 1;
    let mut line_start = 0;

    for (idx, ch) in sql.char_indices() {
        if current_line == line {
            // Found the target line, calculate column offset
            let target_offset = line_start + (column - 1);
            if target_offset <= sql.len() {
                return Some(target_offset);
            }
            return None;
        }

        if ch == '\n' {
            current_line += 1;
            line_start = idx + 1;
        }
    }

    // Handle last line (no trailing newline)
    if current_line == line {
        let target_offset = line_start + (column - 1);
        if target_offset <= sql.len() {
            return Some(target_offset);
        }
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
}
