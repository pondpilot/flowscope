//! UTF-8 ↔ UTF-16 encoding conversion utilities for WASM API.
//!
//! JavaScript strings use UTF-16 internally (like Monaco editor), while Rust
//! strings use UTF-8. This module provides conversion functions at the WASM
//! boundary so consumers can work with their native encoding.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Text encoding for offset interpretation.
///
/// When `Utf16` is specified, cursor offsets in requests are interpreted as
/// UTF-16 code units, and span offsets in responses are converted to UTF-16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    /// UTF-8 byte offsets (default, backwards compatible)
    #[default]
    Utf8,
    /// UTF-16 code unit offsets (for Monaco/JavaScript consumers)
    Utf16,
}

/// Convert a UTF-16 code unit offset to a UTF-8 byte offset.
///
/// # Arguments
/// * `sql` - The SQL string (UTF-8 encoded Rust string)
/// * `utf16_offset` - Offset in UTF-16 code units
///
/// # Returns
/// * `Ok(byte_offset)` - The corresponding UTF-8 byte offset
/// * `Err(message)` - If the offset is out of bounds
pub fn utf16_to_utf8_offset(sql: &str, utf16_offset: usize) -> Result<usize, String> {
    let mut utf16_count = 0;
    let mut byte_offset = 0;

    for ch in sql.chars() {
        if utf16_count == utf16_offset {
            return Ok(byte_offset);
        }
        // Characters outside BMP (emoji, etc.) take 2 UTF-16 code units
        utf16_count += ch.len_utf16();
        byte_offset += ch.len_utf8();
    }

    // Handle end-of-string position
    if utf16_count == utf16_offset {
        return Ok(byte_offset);
    }

    Err(format!(
        "UTF-16 offset {} exceeds string length (max: {})",
        utf16_offset, utf16_count
    ))
}

/// Convert a UTF-8 byte offset to a UTF-16 code unit offset.
///
/// # Arguments
/// * `sql` - The SQL string (UTF-8 encoded Rust string)
/// * `utf8_offset` - Offset in UTF-8 bytes
///
/// # Returns
/// * `Ok(utf16_offset)` - The corresponding UTF-16 code unit offset
/// * `Err(message)` - If the offset is out of bounds or not on a character boundary
pub fn utf8_to_utf16_offset(sql: &str, utf8_offset: usize) -> Result<usize, String> {
    if utf8_offset > sql.len() {
        return Err(format!(
            "UTF-8 offset {} exceeds string length {}",
            utf8_offset,
            sql.len()
        ));
    }

    // Validate we're on a character boundary
    if !sql.is_char_boundary(utf8_offset) {
        return Err(format!(
            "UTF-8 offset {} does not land on a character boundary",
            utf8_offset
        ));
    }

    let mut utf16_count = 0;
    let mut byte_count = 0;

    for ch in sql.chars() {
        if byte_count == utf8_offset {
            return Ok(utf16_count);
        }
        byte_count += ch.len_utf8();
        utf16_count += ch.len_utf16();
    }

    // Handle end-of-string position
    if byte_count == utf8_offset {
        return Ok(utf16_count);
    }

    // Should not reach here if is_char_boundary check passed
    Err(format!(
        "UTF-8 offset {} not found (internal error)",
        utf8_offset
    ))
}

/// Recursively convert all span offsets in a JSON value from UTF-8 to UTF-16.
///
/// Walks the JSON tree looking for objects with `start` and `end` number fields
/// (the Span pattern) and converts them using the provided SQL string.
pub fn convert_spans_to_utf16(sql: &str, value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Check if this object looks like a Span (has start and end numbers)
            let is_span = map.get("start").is_some_and(|v| v.is_u64())
                && map.get("end").is_some_and(|v| v.is_u64());

            if is_span {
                if let (Some(Value::Number(start)), Some(Value::Number(end))) =
                    (map.get("start").cloned(), map.get("end").cloned())
                {
                    if let (Some(start_u8), Some(end_u8)) = (start.as_u64(), end.as_u64()) {
                        // Convert offsets, keeping original on error
                        if let Ok(start_u16) = utf8_to_utf16_offset(sql, start_u8 as usize) {
                            map.insert("start".to_string(), Value::Number(start_u16.into()));
                        }
                        if let Ok(end_u16) = utf8_to_utf16_offset(sql, end_u8 as usize) {
                            map.insert("end".to_string(), Value::Number(end_u16.into()));
                        }
                    }
                }
            }

            // Recurse into all values
            for (_, v) in map.iter_mut() {
                convert_spans_to_utf16(sql, v);
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                convert_spans_to_utf16(sql, item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf16_to_utf8_ascii_only() {
        let sql = "SELECT * FROM users";
        // For ASCII, UTF-16 and UTF-8 offsets are identical
        assert_eq!(utf16_to_utf8_offset(sql, 0).unwrap(), 0);
        assert_eq!(utf16_to_utf8_offset(sql, 7).unwrap(), 7);
        assert_eq!(utf16_to_utf8_offset(sql, 19).unwrap(), 19);
    }

    #[test]
    fn test_utf16_to_utf8_multibyte() {
        // '日' is 3 UTF-8 bytes, 1 UTF-16 code unit
        let sql = "SELECT '日本語'";
        // "SELECT '" = 8 chars (ASCII)
        assert_eq!(utf16_to_utf8_offset(sql, 8).unwrap(), 8);
        // After '日' (1 UTF-16 unit = 3 UTF-8 bytes)
        assert_eq!(utf16_to_utf8_offset(sql, 9).unwrap(), 11);
        // After '日本' (2 UTF-16 units = 6 UTF-8 bytes)
        assert_eq!(utf16_to_utf8_offset(sql, 10).unwrap(), 14);
    }

    #[test]
    fn test_utf16_to_utf8_emoji() {
        // '😀' is 4 UTF-8 bytes, 2 UTF-16 code units (surrogate pair)
        let sql = "SELECT '😀'";
        // "SELECT '" = 8 chars
        assert_eq!(utf16_to_utf8_offset(sql, 8).unwrap(), 8);
        // After emoji: 2 UTF-16 code units = 4 UTF-8 bytes
        assert_eq!(utf16_to_utf8_offset(sql, 10).unwrap(), 12);
        // After closing quote
        assert_eq!(utf16_to_utf8_offset(sql, 11).unwrap(), 13);
    }

    #[test]
    fn test_utf16_to_utf8_out_of_bounds() {
        let sql = "SELECT";
        assert!(utf16_to_utf8_offset(sql, 100).is_err());
    }

    #[test]
    fn test_utf8_to_utf16_ascii_only() {
        let sql = "SELECT * FROM users";
        assert_eq!(utf8_to_utf16_offset(sql, 0).unwrap(), 0);
        assert_eq!(utf8_to_utf16_offset(sql, 7).unwrap(), 7);
        assert_eq!(utf8_to_utf16_offset(sql, 19).unwrap(), 19);
    }

    #[test]
    fn test_utf8_to_utf16_multibyte() {
        let sql = "SELECT '日本語'";
        // "SELECT '" = 8 bytes (ASCII)
        assert_eq!(utf8_to_utf16_offset(sql, 8).unwrap(), 8);
        // After '日' (3 UTF-8 bytes = 1 UTF-16 unit)
        assert_eq!(utf8_to_utf16_offset(sql, 11).unwrap(), 9);
        // After '日本' (6 UTF-8 bytes = 2 UTF-16 units)
        assert_eq!(utf8_to_utf16_offset(sql, 14).unwrap(), 10);
    }

    #[test]
    fn test_utf8_to_utf16_emoji() {
        let sql = "SELECT '😀'";
        assert_eq!(utf8_to_utf16_offset(sql, 8).unwrap(), 8);
        // After emoji: 4 UTF-8 bytes = 2 UTF-16 code units
        assert_eq!(utf8_to_utf16_offset(sql, 12).unwrap(), 10);
    }

    #[test]
    fn test_utf8_to_utf16_invalid_boundary() {
        let sql = "日"; // 3 UTF-8 bytes
        assert!(utf8_to_utf16_offset(sql, 1).is_err()); // Middle of character
        assert!(utf8_to_utf16_offset(sql, 2).is_err()); // Middle of character
        assert!(utf8_to_utf16_offset(sql, 3).is_ok()); // End of string
    }

    #[test]
    fn test_convert_spans_to_utf16() {
        // SQL: "SELECT '日本語'"
        // UTF-8 bytes: S(1) E(2) L(3) E(4) C(5) T(6) space(7) '(8) 日(9-11) 本(12-14) 語(15-17) '(18)
        // UTF-16 units: S(1) E(2) L(3) E(4) C(5) T(6) space(7) '(8) 日(9) 本(10) 語(11) '(12)
        let sql = "SELECT '日本語'";
        let mut json = serde_json::json!({
            "result": {
                "span": { "start": 8, "end": 17 },
                "items": [
                    { "span": { "start": 0, "end": 6 } }
                ]
            }
        });

        convert_spans_to_utf16(sql, &mut json);

        // Original: start=8 (byte offset after '), end=17 (byte offset after 語)
        // Converted: start=8 (same for ASCII), end=11 (UTF-16 offset after 語)
        assert_eq!(json["result"]["span"]["start"], 8);
        assert_eq!(json["result"]["span"]["end"], 11);

        // ASCII span unchanged
        assert_eq!(json["result"]["items"][0]["span"]["start"], 0);
        assert_eq!(json["result"]["items"][0]["span"]["end"], 6);
    }

    #[test]
    fn test_roundtrip_conversion() {
        let sql = "SELECT '日本😀語'";

        // Test various positions
        for utf8_pos in [0, 8, 11, 14, 18, 21] {
            if sql.is_char_boundary(utf8_pos) {
                let utf16 = utf8_to_utf16_offset(sql, utf8_pos).unwrap();
                let back = utf16_to_utf8_offset(sql, utf16).unwrap();
                assert_eq!(back, utf8_pos, "Roundtrip failed for position {}", utf8_pos);
            }
        }
    }
}
