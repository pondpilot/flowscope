//! Function argument handling rules per dialect.
//!
//! Generated from dialect_behavior.toml
//!
//! This module provides dialect-aware rules for function argument handling,
//! particularly for date/time functions where certain arguments are keywords
//! (like `YEAR`, `MONTH`) rather than column references.

use crate::Dialect;

/// Returns argument indices to skip when extracting column references from a function call.
///
/// Certain SQL functions take keyword arguments (e.g., `DATEDIFF(YEAR, start, end)` in Snowflake)
/// that should not be treated as column references during lineage analysis. This function
/// returns the indices of such arguments for the given function and dialect.
///
/// # Arguments
///
/// * `dialect` - The SQL dialect being analyzed
/// * `func_name` - The function name (case-insensitive, underscore-insensitive)
///
/// # Returns
///
/// A slice of argument indices (0-based) to skip. Returns an empty slice for
/// unknown functions or functions without skip rules.
///
/// # Example
///
/// ```ignore
/// // In Snowflake, DATEDIFF takes a unit as the first argument
/// let skip = skip_args_for_function(Dialect::Snowflake, "DATEDIFF");
/// assert_eq!(skip, &[0]); // Skip first argument (the unit)
///
/// // Both DATEADD and DATE_ADD match the same rules
/// let skip1 = skip_args_for_function(Dialect::Snowflake, "DATEADD");
/// let skip2 = skip_args_for_function(Dialect::Snowflake, "DATE_ADD");
/// assert_eq!(skip1, skip2);
/// ```
pub fn skip_args_for_function(dialect: Dialect, func_name: &str) -> &'static [usize] {
    // Normalize: lowercase and remove underscores to handle both DATEADD and DATE_ADD variants
    let func_normalized: String = func_name
        .chars()
        .filter(|c| *c != '_')
        .map(|c| c.to_ascii_lowercase())
        .collect();
    match func_normalized.as_str() {
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
        "dateadd" => match dialect {
            Dialect::Bigquery => &[],
            Dialect::Hive => &[],
            Dialect::Mssql => &[0],
            Dialect::Mysql => &[],
            Dialect::Postgres => &[],
            Dialect::Snowflake => &[0],
            _ => &[],
        },
        "datepart" => match dialect {
            Dialect::Postgres => &[0],
            Dialect::Redshift => &[0],
            Dialect::Snowflake => &[0],
            _ => &[],
        },
        "datetrunc" => match dialect {
            Dialect::Bigquery => &[1],
            Dialect::Databricks => &[0],
            Dialect::Duckdb => &[0],
            Dialect::Postgres => &[0],
            Dialect::Redshift => &[0],
            Dialect::Snowflake => &[0],
            _ => &[],
        },
        "extract" => &[0],
        "timestampadd" => match dialect {
            Dialect::Bigquery => &[1],
            Dialect::Snowflake => &[0],
            _ => &[],
        },
        "timestampsub" => match dialect {
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
