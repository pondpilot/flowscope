//! Parse strategies for hybrid SQL completion.
//!
//! This module provides multiple strategies for parsing incomplete SQL,
//! trying them in order of cost until one succeeds.
//!
//! Note: This module assumes ASCII SQL keywords and identifiers. Non-ASCII
//! characters in identifiers are handled correctly, but keyword matching
//! is ASCII-only for consistent cross-dialect behavior.

// We intentionally create Vec<Range<usize>> with single elements for synthetic ranges
#![allow(clippy::single_range_in_vec_init)]

use std::ops::Range;

use sqlparser::ast::Statement;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, Tokenizer};

use crate::analyzer::helpers::line_col_to_offset;
use crate::types::{Dialect, ParseStrategy};

/// Maximum number of truncation attempts to prevent DoS with pathological SQL.
const MAX_TRUNCATION_ATTEMPTS: usize = 50;

/// Maximum number of parentheses to fix to prevent excessive allocation.
/// Reasonable SQL rarely has more than 20 levels of nesting.
const MAX_PAREN_FIXES: usize = 20;

/// Length of the "FROM" keyword in bytes (ASCII).
const FROM_KEYWORD_LENGTH: usize = 4;

/// Type alias for SQL fix functions that return fixed SQL and synthetic ranges.
///
/// The dialect parameter enables token-aware keyword matching, which correctly
/// handles keywords inside string literals and comments.
type SqlFixFn = fn(&str, usize, Dialect) -> Option<(String, Vec<Range<usize>>)>;

/// Represents a word token with its position in the original SQL string.
#[derive(Debug, Clone)]
struct WordPosition {
    /// Byte offset where the word starts in the original SQL
    start: usize,
    /// The normalized (uppercase) value of the word
    value_upper: String,
}

/// Tokenize SQL and extract word token positions.
///
/// Uses sqlparser's tokenizer to properly handle string literals and comments.
/// Returns None if tokenization fails (e.g., for incomplete SQL).
fn tokenize_word_positions(sql: &str, dialect: Dialect) -> Option<Vec<WordPosition>> {
    let dialect_impl = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(&*dialect_impl, sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut positions = Vec::new();
    for token_with_span in tokens {
        if let Token::Word(word) = &token_with_span.token {
            // Convert line/column to byte offset
            let start = line_col_to_offset(
                sql,
                token_with_span.span.start.line as usize,
                token_with_span.span.start.column as usize,
            )?;
            positions.push(WordPosition {
                start,
                value_upper: word.value.to_uppercase(),
            });
        }
    }
    Some(positions)
}

/// Find all positions of a keyword in SQL using token-aware matching.
///
/// This function respects SQL syntax by only matching keywords that appear as
/// actual Word tokens, not inside string literals or comments.
///
/// Falls back to byte-level matching if tokenization fails (e.g., incomplete SQL).
///
/// Returns byte indices into the original string where the keyword starts.
///
/// Note: Production code should use `find_keyword_positions_with_dialect` for
/// accurate dialect-specific tokenization. This generic wrapper is kept for tests.
#[cfg(test)]
fn find_keyword_positions(sql: &str, keyword: &str) -> Vec<usize> {
    find_keyword_positions_with_dialect(sql, keyword, Dialect::Generic)
}

/// Find keyword positions with a specific dialect for tokenization.
fn find_keyword_positions_with_dialect(sql: &str, keyword: &str, dialect: Dialect) -> Vec<usize> {
    let keyword_upper = keyword.to_uppercase();

    // Try token-aware search first
    if let Some(word_positions) = tokenize_word_positions(sql, dialect) {
        return word_positions
            .into_iter()
            .filter(|wp| wp.value_upper == keyword_upper)
            .map(|wp| wp.start)
            .collect();
    }

    // Fall back to byte-level search for incomplete/unparseable SQL
    find_keyword_positions_fallback(sql, keyword)
}

/// Byte-level keyword search fallback for when tokenization fails.
///
/// This is less accurate (can match keywords inside string literals) but works
/// with incomplete SQL that the tokenizer cannot handle.
fn find_keyword_positions_fallback(sql: &str, keyword: &str) -> Vec<usize> {
    let sql_bytes = sql.as_bytes();
    let kw_bytes = keyword.as_bytes();
    let kw_len = kw_bytes.len();

    if kw_len == 0 || sql_bytes.len() < kw_len {
        return Vec::new();
    }

    let mut positions = Vec::new();
    for i in 0..=sql_bytes.len() - kw_len {
        let matches = sql_bytes[i..i + kw_len]
            .iter()
            .zip(kw_bytes)
            .all(|(s, k)| s.eq_ignore_ascii_case(k));

        if matches {
            positions.push(i);
        }
    }
    positions
}

/// Find the last position of a keyword in SQL using token-aware matching.
///
/// This function respects SQL syntax by only matching keywords that appear as
/// actual Word tokens, not inside string literals or comments.
///
/// Returns the byte index of the last occurrence, or None if not found.
///
/// Note: Production code should use `rfind_keyword_with_dialect` for
/// accurate dialect-specific tokenization. This generic wrapper is kept for tests.
#[cfg(test)]
fn rfind_keyword(sql: &str, keyword: &str) -> Option<usize> {
    rfind_keyword_with_dialect(sql, keyword, Dialect::Generic)
}

/// Find the last keyword position with a specific dialect for tokenization.
///
/// Uses reverse iteration to find the last match efficiently without
/// iterating through all positions.
fn rfind_keyword_with_dialect(sql: &str, keyword: &str, dialect: Dialect) -> Option<usize> {
    let keyword_upper = keyword.to_uppercase();

    // Try token-aware search first
    if let Some(word_positions) = tokenize_word_positions(sql, dialect) {
        // Use rfind() to iterate from the end and find the last match efficiently
        return word_positions
            .into_iter()
            .rfind(|wp| wp.value_upper == keyword_upper)
            .map(|wp| wp.start);
    }

    // Fall back to byte-level search for incomplete/unparseable SQL
    rfind_keyword_fallback(sql, keyword)
}

/// Byte-level reverse keyword search fallback for when tokenization fails.
fn rfind_keyword_fallback(sql: &str, keyword: &str) -> Option<usize> {
    let sql_bytes = sql.as_bytes();
    let kw_bytes = keyword.as_bytes();
    let kw_len = kw_bytes.len();

    if kw_len == 0 || sql_bytes.len() < kw_len {
        return None;
    }

    for i in (0..=sql_bytes.len() - kw_len).rev() {
        let matches = sql_bytes[i..i + kw_len]
            .iter()
            .zip(kw_bytes)
            .all(|(s, k)| s.eq_ignore_ascii_case(k));

        if matches {
            return Some(i);
        }
    }
    None
}

/// Check if SQL ends with a keyword (case-insensitive, allowing trailing whitespace).
fn ends_with_keyword(sql: &str, keyword: &str) -> bool {
    let trimmed = sql.trim_end();
    let kw_bytes = keyword.as_bytes();
    let kw_len = kw_bytes.len();

    if trimmed.len() < kw_len {
        return false;
    }

    let start = trimmed.len() - kw_len;
    trimmed.as_bytes()[start..]
        .iter()
        .zip(kw_bytes)
        .all(|(s, k)| s.eq_ignore_ascii_case(k))
}

/// Result of a successful parse attempt
#[derive(Debug, Clone)]
pub(crate) struct ParseResult {
    /// Parsed SQL statements
    pub statements: Vec<Statement>,
    /// Strategy that succeeded (reserved for future diagnostic/optimization use)
    #[allow(dead_code)]
    pub strategy: ParseStrategy,
    /// Byte ranges of synthetic (added) content to ignore during extraction
    /// (reserved for future use when filtering AST nodes that were synthesized)
    #[allow(dead_code)]
    pub synthetic_ranges: Vec<Range<usize>>,
}

/// Try to parse SQL for completion context extraction.
///
/// Attempts multiple strategies in order of cost until one succeeds:
/// 1. Full parse (complete SQL)
/// 2. Truncated parse (cut at cursor position)
/// 3. Complete statements only (semicolon-terminated before cursor)
/// 4. With minimal fixes (patch incomplete SQL)
pub(crate) fn try_parse_for_completion(
    sql: &str,
    cursor_offset: usize,
    dialect: Dialect,
) -> Option<ParseResult> {
    // Strategy 1: Try full parse
    if let Some(stmts) = try_full_parse(sql, dialect) {
        return Some(ParseResult {
            statements: stmts,
            strategy: ParseStrategy::FullParse,
            synthetic_ranges: vec![],
        });
    }

    // Strategy 2: Try truncated parse
    if let Some(stmts) = try_truncated_parse(sql, cursor_offset, dialect) {
        return Some(ParseResult {
            statements: stmts,
            strategy: ParseStrategy::Truncated,
            synthetic_ranges: vec![],
        });
    }

    // Strategy 3: Try complete statements only
    if let Some(stmts) = try_complete_statements(sql, cursor_offset, dialect) {
        return Some(ParseResult {
            statements: stmts,
            strategy: ParseStrategy::CompleteStatementsOnly,
            synthetic_ranges: vec![],
        });
    }

    // Strategy 4: Try with minimal fixes
    if let Some((stmts, synthetic)) = try_with_fixes(sql, cursor_offset, dialect) {
        return Some(ParseResult {
            statements: stmts,
            strategy: ParseStrategy::WithFixes,
            synthetic_ranges: synthetic,
        });
    }

    None
}

/// Strategy 1: Parse complete SQL as-is
pub fn try_full_parse(sql: &str, dialect: Dialect) -> Option<Vec<Statement>> {
    if sql.trim().is_empty() {
        return None;
    }

    let dialect_impl = dialect.to_sqlparser_dialect();
    Parser::parse_sql(&*dialect_impl, sql)
        .ok()
        .filter(|stmts| !stmts.is_empty())
}

/// Strategy 2: Truncate SQL at a safe point before cursor
pub fn try_truncated_parse(
    sql: &str,
    cursor_offset: usize,
    dialect: Dialect,
) -> Option<Vec<Statement>> {
    if cursor_offset == 0 || cursor_offset > sql.len() {
        return None;
    }

    let dialect_impl = dialect.to_sqlparser_dialect();
    let before_cursor = &sql[..cursor_offset.min(sql.len())];

    // Try progressively shorter truncations until we find one that parses
    // Limit attempts to prevent DoS with pathological SQL
    let candidates = find_truncation_candidates(before_cursor, dialect);
    for truncation in candidates.into_iter().take(MAX_TRUNCATION_ATTEMPTS) {
        if truncation == 0 {
            continue;
        }

        let truncated = &sql[..truncation];
        if truncated.trim().is_empty() {
            continue;
        }

        if let Ok(stmts) = Parser::parse_sql(&*dialect_impl, truncated) {
            if !stmts.is_empty() {
                return Some(stmts);
            }
        }
    }

    None
}

/// Strategy 3: Parse only complete statements before cursor
pub fn try_complete_statements(
    sql: &str,
    cursor_offset: usize,
    dialect: Dialect,
) -> Option<Vec<Statement>> {
    // Find the last semicolon before cursor
    let before_cursor = &sql[..cursor_offset.min(sql.len())];
    let last_semicolon = before_cursor.rfind(';')?;

    let complete_portion = &sql[..=last_semicolon];
    if complete_portion.trim().is_empty() {
        return None;
    }

    let dialect_impl = dialect.to_sqlparser_dialect();
    Parser::parse_sql(&*dialect_impl, complete_portion)
        .ok()
        .filter(|stmts| !stmts.is_empty())
}

/// Strategy 4: Apply minimal fixes to make SQL parseable
pub fn try_with_fixes(
    sql: &str,
    cursor_offset: usize,
    dialect: Dialect,
) -> Option<(Vec<Statement>, Vec<Range<usize>>)> {
    let dialect_impl = dialect.to_sqlparser_dialect();

    // Try fixes in order of likelihood
    let fixes: Vec<SqlFixFn> = vec![
        fix_trailing_comma,
        fix_unclosed_parens,
        fix_incomplete_select,
        fix_incomplete_from,
        fix_unclosed_string,
    ];

    for fix in fixes {
        if let Some((fixed_sql, synthetic)) = fix(sql, cursor_offset, dialect) {
            if let Ok(stmts) = Parser::parse_sql(&*dialect_impl, &fixed_sql) {
                if !stmts.is_empty() {
                    return Some((stmts, synthetic));
                }
            }
        }
    }

    None
}

/// Generate candidate truncation points from longest to shortest.
/// These are positions where SQL might be syntactically complete.
fn find_truncation_candidates(sql: &str, dialect: Dialect) -> Vec<usize> {
    let mut candidates = Vec::new();
    let bytes = sql.as_bytes();

    // SQL keywords that often mark clause boundaries where truncation might work
    let keywords = [
        "WHERE",
        "GROUP",
        "HAVING",
        "ORDER",
        "LIMIT",
        "OFFSET",
        "UNION",
        "EXCEPT",
        "INTERSECT",
    ];

    // Find positions right before keywords (truncating before the keyword)
    // Use token-aware matching to skip keywords inside strings/comments
    for kw in &keywords {
        for abs_pos in find_keyword_positions_with_dialect(sql, kw, dialect) {
            // Make sure it's a word boundary (preceded by whitespace)
            if abs_pos > 0 && bytes[abs_pos - 1].is_ascii_whitespace() {
                candidates.push(abs_pos);
            }
        }
    }

    // Also try truncating at word boundaries going backwards
    // Only consider positions that are valid UTF-8 character boundaries
    let mut pos = sql.len();
    while pos > 0 {
        let byte = bytes[pos - 1];

        // Only process ASCII bytes to avoid UTF-8 boundary issues
        // Non-ASCII bytes (high bit set) are part of multi-byte sequences
        if byte.is_ascii() {
            let ch = byte as char;

            // After alphanumeric/identifier chars could be a valid truncation point
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == ')' || ch == '"' || ch == '\'' {
                candidates.push(pos);
            }
        }

        pos -= 1;
    }

    // Sort by position descending (try longer truncations first)
    candidates.sort_by(|a, b| b.cmp(a));
    candidates.dedup();
    candidates
}

/// Fix: Remove trailing comma
fn fix_trailing_comma(
    sql: &str,
    _cursor_offset: usize,
    dialect: Dialect,
) -> Option<(String, Vec<Range<usize>>)> {
    // Look for patterns like "SELECT a, FROM" or "SELECT a, b, FROM"
    let trimmed = sql.trim_end();

    // Simple case: trailing comma before FROM
    // Find "FROM" keyword and verify it's preceded by whitespace
    if let Some(from_pos) = rfind_keyword_with_dialect(trimmed, "FROM", dialect) {
        // Ensure FROM is preceded by whitespace (it's a word boundary)
        if from_pos > 0 && trimmed.as_bytes()[from_pos - 1].is_ascii_whitespace() {
            let before_from = trimmed[..from_pos].trim_end();
            if let Some(without_comma) = before_from.strip_suffix(',') {
                // Normalize spacing: trim any leading whitespace after FROM and add exactly one space
                let after_from = &trimmed[from_pos + FROM_KEYWORD_LENGTH..];
                let after_from_trimmed = after_from.trim_start();
                let fixed = if after_from_trimmed.is_empty() {
                    format!("{} FROM", without_comma)
                } else {
                    format!("{} FROM {}", without_comma, after_from_trimmed)
                };
                return Some((fixed, vec![]));
            }
        }
    }

    None
}

/// Fix: Close unclosed parentheses
fn fix_unclosed_parens(
    sql: &str,
    _cursor_offset: usize,
    _dialect: Dialect,
) -> Option<(String, Vec<Range<usize>>)> {
    let open = sql.chars().filter(|&c| c == '(').count();
    let close = sql.chars().filter(|&c| c == ')').count();

    if open > close {
        let missing = open - close;
        // Limit allocation to prevent DoS with pathological input
        if missing > MAX_PAREN_FIXES {
            return None;
        }
        let suffix = ")".repeat(missing);
        let synthetic_start = sql.len();
        let fixed = format!("{}{}", sql, suffix);
        return Some((fixed, vec![synthetic_start..synthetic_start + missing]));
    }

    None
}

/// Fix: Add placeholder after incomplete SELECT
fn fix_incomplete_select(
    sql: &str,
    _cursor_offset: usize,
    dialect: Dialect,
) -> Option<(String, Vec<Range<usize>>)> {
    // Look for "SELECT FROM" without anything between
    // Use token-aware matching to skip keywords in strings/comments

    // Find SELECT keyword
    let positions = find_keyword_positions_with_dialect(sql, "SELECT", dialect);
    if let Some(&select_pos) = positions.first() {
        let after_select_start = select_pos + 6;
        if after_select_start <= sql.len() {
            let after_select = &sql[after_select_start..];

            // Check if FROM follows immediately (with only whitespace)
            let from_positions = find_keyword_positions_with_dialect(after_select, "FROM", dialect);
            if let Some(&from_rel_pos) = from_positions.first() {
                let between = after_select[..from_rel_pos].trim();
                if between.is_empty() {
                    // Insert "1 " after SELECT
                    let insert_pos = after_select_start;
                    let mut fixed = sql.to_string();
                    fixed.insert_str(insert_pos, " 1");
                    return Some((fixed, vec![insert_pos..insert_pos + 2]));
                }
            }
        }
    }

    None
}

/// Fix: Add dummy table after incomplete FROM
fn fix_incomplete_from(
    sql: &str,
    _cursor_offset: usize,
    _dialect: Dialect,
) -> Option<(String, Vec<Range<usize>>)> {
    let trimmed = sql.trim_end();

    // Check if SQL ends with FROM (possibly with whitespace)
    // Use ASCII case-insensitive matching (ends_with_keyword is byte-level,
    // but this is safe because we're checking the actual end of SQL, not
    // searching for a keyword that could be inside a string)
    if ends_with_keyword(trimmed, "FROM") {
        let suffix = " _dummy_";
        let synthetic_start = sql.len();
        let fixed = format!("{}{}", sql, suffix);
        return Some((fixed, vec![synthetic_start..synthetic_start + suffix.len()]));
    }

    None
}

/// Fix: Close unclosed string literal
fn fix_unclosed_string(
    sql: &str,
    _cursor_offset: usize,
    _dialect: Dialect,
) -> Option<(String, Vec<Range<usize>>)> {
    // Count quotes
    let single_quotes = sql.chars().filter(|&c| c == '\'').count();
    let double_quotes = sql.chars().filter(|&c| c == '"').count();

    if single_quotes % 2 != 0 {
        let synthetic_start = sql.len();
        let fixed = format!("{}'", sql);
        return Some((fixed, vec![synthetic_start..synthetic_start + 1]));
    }

    if double_quotes % 2 != 0 {
        let synthetic_start = sql.len();
        let fixed = format!("{}\"", sql);
        return Some((fixed, vec![synthetic_start..synthetic_start + 1]));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_keyword_skips_string_literals() {
        // "WHERE" inside a string literal should NOT be matched
        let sql = "SELECT 'WHERE is fun' FROM users";
        let positions = find_keyword_positions(sql, "WHERE");
        assert!(
            positions.is_empty(),
            "Should not find WHERE inside string literal, found at: {:?}",
            positions
        );

        // But actual WHERE keyword should be found
        let sql2 = "SELECT 'text' FROM users WHERE id = 1";
        let positions2 = find_keyword_positions(sql2, "WHERE");
        assert_eq!(positions2.len(), 1);
        assert_eq!(&sql2[positions2[0]..positions2[0] + 5], "WHERE");
    }

    #[test]
    fn test_find_keyword_skips_comments() {
        // "WHERE" inside a comment should NOT be matched
        let sql = "SELECT * FROM users -- WHERE is commented out";
        let positions = find_keyword_positions(sql, "WHERE");
        assert!(
            positions.is_empty(),
            "Should not find WHERE inside line comment"
        );

        // Block comment
        let sql2 = "SELECT * /* WHERE */ FROM users";
        let positions2 = find_keyword_positions(sql2, "WHERE");
        assert!(
            positions2.is_empty(),
            "Should not find WHERE inside block comment"
        );
    }

    #[test]
    fn test_find_keyword_case_insensitive() {
        let sql = "select * from users where id = 1";
        let positions = find_keyword_positions(sql, "WHERE");
        assert_eq!(positions.len(), 1);
        assert_eq!(&sql[positions[0]..positions[0] + 5], "where");
    }

    #[test]
    fn test_find_keyword_handles_unicode_prefix() {
        let sql = "SELECT μ, FROM users";
        let positions = find_keyword_positions(sql, "FROM");
        assert_eq!(positions, vec!["SELECT μ, ".len()]);
    }

    #[test]
    fn test_rfind_keyword_token_aware() {
        // Should find the actual FROM keyword, not the one in the string
        let sql = "SELECT 'FROM somewhere' FROM users";
        let pos = rfind_keyword(sql, "FROM");
        assert!(pos.is_some());
        let pos = pos.unwrap();
        assert_eq!(&sql[pos..pos + 4], "FROM");
        // The FROM in 'FROM somewhere' is at position 8, actual FROM is at 24
        assert!(pos > 20, "Should find actual FROM, not one in string");
    }

    #[test]
    fn test_rfind_keyword_handles_unicode_prefix() {
        let sql = "SELECT μ, FROM users";
        let pos = rfind_keyword(sql, "FROM").expect("should find FROM");
        assert_eq!(
            pos,
            "SELECT μ, ".len(),
            "should account for multi-byte chars"
        );
    }

    #[test]
    fn test_full_parse_valid_sql() {
        let sql = "SELECT * FROM users WHERE id = 1";
        let result = try_full_parse(sql, Dialect::Generic);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_full_parse_invalid_sql() {
        let sql = "SELECT * FROM";
        let result = try_full_parse(sql, Dialect::Generic);
        assert!(result.is_none());
    }

    #[test]
    fn test_truncated_parse() {
        let sql = "SELECT * FROM users WHERE ";
        let result = try_truncated_parse(sql, sql.len(), Dialect::Generic);
        assert!(result.is_some());
    }

    #[test]
    fn test_complete_statements_only() {
        let sql = "SELECT 1; SELECT * FROM";
        let result = try_complete_statements(sql, sql.len(), Dialect::Generic);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_fix_trailing_comma() {
        let sql = "SELECT a, FROM users";
        let result = try_with_fixes(sql, sql.len(), Dialect::Generic);
        assert!(result.is_some());
    }

    #[test]
    fn test_fix_unclosed_parens() {
        let sql = "SELECT COUNT(* FROM users";
        let result = fix_unclosed_parens(sql, sql.len(), Dialect::Generic);
        assert!(result.is_some());
        let (fixed, synthetic) = result.unwrap();
        assert!(fixed.ends_with(')'));
        assert_eq!(synthetic.len(), 1);
    }

    #[test]
    fn test_fix_incomplete_select() {
        let sql = "SELECT FROM users";
        let result = fix_incomplete_select(sql, sql.len(), Dialect::Generic);
        assert!(result.is_some());
        let (fixed, synthetic) = result.unwrap();
        assert!(fixed.contains("1"));
        assert_eq!(synthetic.len(), 1);
    }

    #[test]
    fn test_fix_incomplete_from() {
        let sql = "SELECT * FROM";
        let result = fix_incomplete_from(sql, sql.len(), Dialect::Generic);
        assert!(result.is_some());
        let (fixed, _) = result.unwrap();
        assert!(fixed.contains("_dummy_"));
    }

    #[test]
    fn test_fix_unclosed_string() {
        let sql = "SELECT 'hello";
        let result = fix_unclosed_string(sql, sql.len(), Dialect::Generic);
        assert!(result.is_some());
        let (fixed, _) = result.unwrap();
        assert!(fixed.ends_with('\''));
    }

    #[test]
    fn test_try_parse_for_completion_valid() {
        let sql = "SELECT * FROM users";
        let result = try_parse_for_completion(sql, sql.len(), Dialect::Generic);
        assert!(result.is_some());
        assert_eq!(result.unwrap().strategy, ParseStrategy::FullParse);
    }

    #[test]
    fn test_try_parse_for_completion_truncated() {
        let sql = "SELECT * FROM users WHERE id = ";
        let result = try_parse_for_completion(sql, sql.len(), Dialect::Generic);
        assert!(result.is_some());
        // Should fall back to truncated
        assert!(matches!(
            result.unwrap().strategy,
            ParseStrategy::Truncated | ParseStrategy::FullParse
        ));
    }

    #[test]
    fn test_try_parse_for_completion_with_fixes() {
        // "SELECT FROM users" actually parses in sqlparser 0.59 (empty projection is valid)
        // Use a truly invalid SQL that requires fixes
        let sql = "SELECT * FROM";
        let result = try_parse_for_completion(sql, sql.len(), Dialect::Generic);
        assert!(result.is_some());
        assert_eq!(result.unwrap().strategy, ParseStrategy::WithFixes);
    }
}
