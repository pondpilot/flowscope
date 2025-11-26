//! Error types for SQL parsing and analysis.
//!
//! # Error Handling Strategy
//!
//! This crate uses two complementary error handling patterns:
//!
//! - [`ParseError`]: Fatal errors that prevent SQL parsing. Returned as `Result<T, ParseError>`
//!   and stop processing of the affected statement.
//!
//! - [`crate::types::Issue`]: Non-fatal warnings and errors collected during analysis
//!   (e.g., unresolved table references, missing columns). These are accumulated in a
//!   vector and returned alongside successful analysis results, allowing partial lineage
//!   extraction even when some references cannot be resolved.
//!
//! This separation allows the analyzer to be resilient: parsing must succeed, but
//! analysis can continue with incomplete information while reporting issues.

use crate::types::Dialect;
use regex::Regex;
use std::fmt;
use std::sync::OnceLock;
#[cfg(feature = "tracing")]
use tracing::trace;

/// Error encountered during SQL parsing.
///
/// This error preserves structured information from the underlying parser
/// including position information when available.
#[derive(Debug, Clone)]
pub struct ParseError {
    /// Human-readable error message.
    pub message: String,
    /// Byte offset where the error occurred, if available.
    pub position: Option<Position>,
    /// The SQL dialect being parsed when the error occurred.
    pub dialect: Option<Dialect>,
    /// The specific category of parse error.
    pub kind: ParseErrorKind,
}

/// Position information for a parse error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Line number (1-indexed).
    pub line: usize,
    /// Column number (1-indexed).
    pub column: usize,
}

/// Category of parse error for programmatic handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParseErrorKind {
    /// Unexpected token or character in input.
    #[default]
    SyntaxError,
    /// Missing required clause or keyword.
    MissingClause,
    /// Invalid or unexpected end of input.
    UnexpectedEof,
    /// Feature not supported by the current dialect.
    UnsupportedFeature,
    /// Lexer/tokenization error.
    LexerError,
}

impl ParseError {
    /// Creates a new parse error with just a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            position: None,
            dialect: None,
            kind: ParseErrorKind::SyntaxError,
        }
    }

    /// Creates a parse error with position information.
    pub fn with_position(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            message: message.into(),
            position: Some(Position { line, column }),
            dialect: None,
            kind: ParseErrorKind::SyntaxError,
        }
    }

    /// Adds dialect context to the error.
    pub fn with_dialect(mut self, dialect: Dialect) -> Self {
        self.dialect = Some(dialect);
        self
    }

    /// Sets the error kind.
    pub fn with_kind(mut self, kind: ParseErrorKind) -> Self {
        self.kind = kind;
        self
    }

    /// Parses position from sqlparser error message format.
    ///
    /// sqlparser uses format like "Expected ..., found ... at Line: X, Column: Y"
    ///
    /// # Implementation Note
    ///
    /// This parsing is coupled to the `sqlparser` crate's error message format.
    /// Uses regex for robust parsing that handles various whitespace and formatting
    /// variations. Gracefully returns `None` when the expected format is not found.
    fn parse_position_from_message(message: &str) -> Option<Position> {
        // Use a static regex for performance - compiled once on first use
        static POSITION_REGEX: OnceLock<Regex> = OnceLock::new();
        let re = POSITION_REGEX.get_or_init(|| {
            // Match "Line: <number>" followed by optional comma/whitespace, then "Column: <number>"
            // Handles variations like "Line: 1, Column: 5" or "Line:1,Column:5"
            Regex::new(r"Line:\s*(\d+)\s*,\s*Column:\s*(\d+)").expect("Invalid regex pattern")
        });

        let result = re.captures(message).and_then(|caps| {
            let line: usize = caps.get(1)?.as_str().parse().ok()?;
            let column: usize = caps.get(2)?.as_str().parse().ok()?;
            Some(Position { line, column })
        });

        #[cfg(feature = "tracing")]
        if result.is_none() && (message.contains("Line") || message.contains("Column")) {
            trace!(
                "Failed to parse position from error message that appears to contain position info: {}",
                message
            );
        }

        result
    }

    /// Determines the error kind from the message content.
    ///
    /// # Implementation Note
    ///
    /// Like [`Self::parse_position_from_message`], this function relies on patterns
    /// in `sqlparser` error messages and may need updates if those messages change.
    fn infer_kind_from_message(message: &str) -> ParseErrorKind {
        let lower = message.to_lowercase();
        if lower.contains("unexpected end") || lower.contains("eof") {
            ParseErrorKind::UnexpectedEof
        } else if lower.contains("expected") {
            ParseErrorKind::MissingClause
        } else if lower.contains("not supported") || lower.contains("unsupported") {
            ParseErrorKind::UnsupportedFeature
        } else if lower.contains("lexer") || lower.contains("token") {
            ParseErrorKind::LexerError
        } else {
            ParseErrorKind::SyntaxError
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Parse error")?;

        if let Some(dialect) = self.dialect {
            write!(f, " ({dialect:?})")?;
        }

        if let Some(pos) = self.position {
            write!(f, " at line {}, column {}", pos.line, pos.column)?;
        }

        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for ParseError {}

impl From<sqlparser::parser::ParserError> for ParseError {
    fn from(err: sqlparser::parser::ParserError) -> Self {
        let message = err.to_string();
        let position = Self::parse_position_from_message(&message);
        let kind = Self::infer_kind_from_message(&message);

        Self {
            message,
            position,
            dialect: None,
            kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_position_from_message() {
        let msg = "Expected SELECT, found 'INSERT' at Line: 1, Column: 5";
        let pos = ParseError::parse_position_from_message(msg);
        assert_eq!(pos, Some(Position { line: 1, column: 5 }));
    }

    #[test]
    fn test_parse_position_no_position() {
        let msg = "Unexpected token";
        let pos = ParseError::parse_position_from_message(msg);
        assert_eq!(pos, None);
    }

    #[test]
    fn test_parse_position_no_whitespace() {
        let msg = "Error at Line:1,Column:5";
        let pos = ParseError::parse_position_from_message(msg);
        assert_eq!(pos, Some(Position { line: 1, column: 5 }));
    }

    #[test]
    fn test_parse_position_extra_whitespace() {
        let msg = "Error at Line:  42 ,  Column:   99";
        let pos = ParseError::parse_position_from_message(msg);
        assert_eq!(
            pos,
            Some(Position {
                line: 42,
                column: 99
            })
        );
    }

    #[test]
    fn test_parse_position_large_numbers() {
        let msg = "Error at Line: 99999, Column: 88888";
        let pos = ParseError::parse_position_from_message(msg);
        assert_eq!(
            pos,
            Some(Position {
                line: 99999,
                column: 88888
            })
        );
    }

    #[test]
    fn test_parse_position_malformed_non_numeric_line() {
        let msg = "Error at Line: abc, Column: 5";
        let pos = ParseError::parse_position_from_message(msg);
        assert_eq!(pos, None);
    }

    #[test]
    fn test_parse_position_malformed_non_numeric_column() {
        let msg = "Error at Line: 1, Column: xyz";
        let pos = ParseError::parse_position_from_message(msg);
        assert_eq!(pos, None);
    }

    #[test]
    fn test_parse_position_malformed_empty_values() {
        let msg = "Error at Line: , Column: ";
        let pos = ParseError::parse_position_from_message(msg);
        assert_eq!(pos, None);
    }

    #[test]
    fn test_parse_position_partial_line_only() {
        let msg = "Error at Line: 5";
        let pos = ParseError::parse_position_from_message(msg);
        assert_eq!(pos, None);
    }

    #[test]
    fn test_parse_position_partial_column_only() {
        let msg = "Error at Column: 5";
        let pos = ParseError::parse_position_from_message(msg);
        assert_eq!(pos, None);
    }

    #[test]
    fn test_parse_position_reversed_order() {
        // If format changes to Column first, it should fail gracefully
        let msg = "Error at Column: 5, Line: 1";
        let pos = ParseError::parse_position_from_message(msg);
        assert_eq!(pos, None);
    }

    #[test]
    fn test_parse_position_negative_numbers() {
        // Negative numbers shouldn't match (regex only matches digits)
        let msg = "Error at Line: -1, Column: -5";
        let pos = ParseError::parse_position_from_message(msg);
        assert_eq!(pos, None);
    }

    #[test]
    fn test_infer_kind_eof() {
        let kind = ParseError::infer_kind_from_message("Unexpected end of input");
        assert_eq!(kind, ParseErrorKind::UnexpectedEof);
    }

    #[test]
    fn test_infer_kind_expected() {
        let kind = ParseError::infer_kind_from_message("Expected SELECT keyword");
        assert_eq!(kind, ParseErrorKind::MissingClause);
    }

    #[test]
    fn test_infer_kind_unsupported() {
        let kind = ParseError::infer_kind_from_message("Feature not supported");
        assert_eq!(kind, ParseErrorKind::UnsupportedFeature);

        let kind = ParseError::infer_kind_from_message("This is unsupported");
        assert_eq!(kind, ParseErrorKind::UnsupportedFeature);
    }

    #[test]
    fn test_infer_kind_lexer() {
        let kind = ParseError::infer_kind_from_message("Lexer error: invalid character");
        assert_eq!(kind, ParseErrorKind::LexerError);

        let kind = ParseError::infer_kind_from_message("Invalid token at position 5");
        assert_eq!(kind, ParseErrorKind::LexerError);
    }

    #[test]
    fn test_infer_kind_default() {
        let kind = ParseError::infer_kind_from_message("Something went wrong");
        assert_eq!(kind, ParseErrorKind::SyntaxError);
    }

    #[test]
    fn test_display_with_position() {
        let err = ParseError::with_position("Unexpected token", 10, 5);
        assert_eq!(
            err.to_string(),
            "Parse error at line 10, column 5: Unexpected token"
        );
    }

    #[test]
    fn test_display_with_dialect() {
        let err = ParseError::new("Bad syntax").with_dialect(Dialect::Postgres);
        assert_eq!(err.to_string(), "Parse error (Postgres): Bad syntax");
    }

    #[test]
    fn test_display_with_dialect_and_position() {
        let err = ParseError::with_position("Bad syntax", 1, 5).with_dialect(Dialect::Snowflake);
        assert_eq!(
            err.to_string(),
            "Parse error (Snowflake) at line 1, column 5: Bad syntax"
        );
    }

    #[test]
    fn test_from_parser_error() {
        // Simulate a sqlparser error message
        let message = "Expected expression, found EOF at Line: 3, Column: 12";
        let pos = ParseError::parse_position_from_message(message);
        assert_eq!(
            pos,
            Some(Position {
                line: 3,
                column: 12
            })
        );
    }

    #[test]
    fn test_with_kind_builder() {
        let err = ParseError::new("Error")
            .with_kind(ParseErrorKind::UnexpectedEof)
            .with_dialect(Dialect::Postgres);
        assert_eq!(err.kind, ParseErrorKind::UnexpectedEof);
        assert_eq!(err.dialect, Some(Dialect::Postgres));
    }

    #[test]
    fn test_error_trait() {
        let err = ParseError::new("Test error");
        let _: &dyn std::error::Error = &err;
    }
}
