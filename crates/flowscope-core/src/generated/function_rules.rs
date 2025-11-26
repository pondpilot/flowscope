//! Function argument handling rules per dialect.
//!
//! Generated from dialect_behavior.toml

use crate::Dialect;

/// Get argument indices to skip for a function in a specific dialect.
/// These are typically unit/part literals that shouldn't be treated as column references.
pub fn skip_args_for_function(dialect: Dialect, func_name: &str) -> &'static [usize] {
    let func_lower = func_name.to_lowercase();
    match func_lower.as_str() {
        "datediff" => match dialect {
            Dialect::Bigquery => &[],
            Dialect::Databricks => &[],
            Dialect::Duckdb => &[],
            Dialect::Hive => &[],
            Dialect::Mssql => &[0],
            Dialect::Mysql => &[],
            Dialect::Redshift => &[0],
            Dialect::Snowflake => &[0],
            _ => &[],
        },
        "date_add" => match dialect {
            Dialect::Bigquery => &[],
            Dialect::Hive => &[],
            Dialect::Mysql => &[],
            Dialect::Postgres => &[],
            Dialect::Snowflake => &[0],
            _ => &[],
        },
        "date_part" => match dialect {
            Dialect::Postgres => &[0],
            Dialect::Redshift => &[0],
            Dialect::Snowflake => &[0],
            _ => &[],
        },
        "date_trunc" => match dialect {
            Dialect::Bigquery => &[1],
            Dialect::Databricks => &[0],
            Dialect::Duckdb => &[0],
            Dialect::Postgres => &[0],
            Dialect::Redshift => &[0],
            Dialect::Snowflake => &[0],
            _ => &[],
        },
        "extract" => &[0],
        "timestamp_add" => match dialect {
            Dialect::Bigquery => &[1],
            Dialect::Snowflake => &[0],
            _ => &[],
        },
        "timestamp_sub" => match dialect {
            Dialect::Bigquery => &[1],
            _ => &[],
        },
        _ => &[],
    }
}

/// NULL ordering behavior in ORDER BY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullOrdering {
    /// NULLs sort as larger than all other values (NULLS LAST for ASC)
    NullsAreLarge,
    /// NULLs sort as smaller than all other values (NULLS FIRST for ASC)
    NullsAreSmall,
    /// NULLs always sort last regardless of ASC/DESC
    NullsAreLast,
}

impl Dialect {
    /// Get the default NULL ordering behavior for this dialect.
    pub const fn null_ordering(&self) -> NullOrdering {
        match self {
            Dialect::Bigquery => NullOrdering::NullsAreSmall,
            Dialect::Clickhouse => NullOrdering::NullsAreLast,
            Dialect::Databricks => NullOrdering::NullsAreSmall,
            Dialect::Duckdb => NullOrdering::NullsAreLast,
            Dialect::Hive => NullOrdering::NullsAreSmall,
            Dialect::Mssql => NullOrdering::NullsAreSmall,
            Dialect::Mysql => NullOrdering::NullsAreSmall,
            Dialect::Postgres => NullOrdering::NullsAreLarge,
            Dialect::Redshift => NullOrdering::NullsAreLarge,
            Dialect::Snowflake => NullOrdering::NullsAreLarge,
            Dialect::Sqlite => NullOrdering::NullsAreSmall,
            _ => NullOrdering::NullsAreLast,
        }
    }

    /// Whether this dialect supports implicit UNNEST (no CROSS JOIN needed).
    pub const fn supports_implicit_unnest(&self) -> bool {
        matches!(self, Dialect::Bigquery | Dialect::Redshift)
    }
}
