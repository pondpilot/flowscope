//! Case sensitivity rules per dialect.
//!
//! Generated from dialects.json and normalization_overrides.toml
//!
//! This module defines how SQL identifiers (table names, column names, etc.)
//! should be normalized for comparison. Different SQL dialects have different
//! rules for identifier case sensitivity.

use std::borrow::Cow;

use crate::Dialect;

/// Normalization strategy for identifier handling.
///
/// SQL dialects differ in how they handle identifier case. This enum represents
/// the different strategies used for normalizing identifiers during analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizationStrategy {
    /// Fold to lowercase (Postgres, Redshift)
    Lowercase,
    /// Fold to uppercase (Snowflake, Oracle)
    Uppercase,
    /// Case-insensitive comparison without folding
    CaseInsensitive,
    /// Case-sensitive, preserve exactly
    CaseSensitive,
}

impl NormalizationStrategy {
    /// Applies this normalization strategy to a string.
    ///
    /// Returns a `Cow<str>` to avoid allocation when no transformation is needed
    /// (i.e., for `CaseSensitive` strategy or when the string is already in the
    /// correct case).
    ///
    /// For `CaseInsensitive`, lowercase folding is used as the canonical form.
    ///
    /// # Example
    ///
    /// ```
    /// use std::borrow::Cow;
    /// use flowscope_core::generated::NormalizationStrategy;
    ///
    /// let strategy = NormalizationStrategy::Lowercase;
    /// assert_eq!(strategy.apply("MyTable"), "mytable");
    ///
    /// // CaseSensitive returns a borrowed reference (no allocation)
    /// let strategy = NormalizationStrategy::CaseSensitive;
    /// assert!(matches!(strategy.apply("MyTable"), Cow::Borrowed(_)));
    /// ```
    pub fn apply<'a>(&self, s: &'a str) -> Cow<'a, str> {
        match self {
            Self::CaseSensitive => Cow::Borrowed(s),
            Self::Lowercase | Self::CaseInsensitive => {
                // Optimization: only allocate if the string contains uppercase chars
                if s.chars().any(|c| c.is_uppercase()) {
                    Cow::Owned(s.to_lowercase())
                } else {
                    Cow::Borrowed(s)
                }
            }
            Self::Uppercase => {
                // Optimization: only allocate if the string contains lowercase chars
                if s.chars().any(|c| c.is_lowercase()) {
                    Cow::Owned(s.to_uppercase())
                } else {
                    Cow::Borrowed(s)
                }
            }
        }
    }
}

impl Dialect {
    /// Get the normalization strategy for this dialect.
    pub const fn normalization_strategy(&self) -> NormalizationStrategy {
        match self {
            Dialect::Bigquery => NormalizationStrategy::CaseInsensitive,
            Dialect::Clickhouse => NormalizationStrategy::CaseSensitive,
            Dialect::Databricks => NormalizationStrategy::CaseInsensitive,
            Dialect::Duckdb => NormalizationStrategy::CaseInsensitive,
            Dialect::Hive => NormalizationStrategy::CaseInsensitive,
            Dialect::Mssql => NormalizationStrategy::CaseInsensitive,
            Dialect::Mysql => NormalizationStrategy::CaseSensitive,
            Dialect::Postgres => NormalizationStrategy::Lowercase,
            Dialect::Redshift => NormalizationStrategy::CaseInsensitive,
            Dialect::Snowflake => NormalizationStrategy::Uppercase,
            Dialect::Sqlite => NormalizationStrategy::CaseInsensitive,
            Dialect::Generic => NormalizationStrategy::CaseInsensitive,
            Dialect::Ansi => NormalizationStrategy::Uppercase,
        }
    }

    /// Returns true if this dialect has custom normalization logic
    /// that cannot be captured by a simple strategy.
    pub const fn has_custom_normalization(&self) -> bool {
        matches!(self, Dialect::Bigquery)
    }

    /// Get pseudocolumns for this dialect (implicit columns like _PARTITIONTIME).
    pub fn pseudocolumns(&self) -> &'static [&'static str] {
        match self {
            Dialect::Bigquery => &["_FILE_NAME", "_PARTITIONDATE", "_PARTITIONTIME", "_TABLE_SUFFIX"],
            Dialect::Snowflake => &["LEVEL"],
            _ => &[],
        }
    }

    /// Get the identifier quote characters for this dialect.
    /// Note: Some dialects use paired quotes (like SQLite's []) which are represented
    /// as single characters here - the opening bracket.
    pub fn identifier_quotes(&self) -> &'static [&'static str] {
        match self {
            Dialect::Bigquery => &["`"],
            Dialect::Clickhouse => &["\"", "`"],
            Dialect::Databricks => &["`"],
            Dialect::Duckdb => &["\""],
            Dialect::Hive => &["`"],
            Dialect::Mssql => &["[", "\""],
            Dialect::Mysql => &["`"],
            Dialect::Postgres => &["\""],
            Dialect::Redshift => &["\""],
            Dialect::Snowflake => &["\""],
            Dialect::Sqlite => &["\"", "[", "`"],
            _ => &["\""],
        }
    }
}
