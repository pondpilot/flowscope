//! Case sensitivity rules per dialect.
//!
//! Generated from dialects.json and normalization_overrides.toml

use crate::Dialect;

/// Normalization strategy for identifier handling.
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
            Dialect::Bigquery => &[
                "_FILE_NAME",
                "_PARTITIONDATE",
                "_PARTITIONTIME",
                "_TABLE_SUFFIX",
            ],
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
