//! SQL Type System for cross-dialect type normalization and compatibility.
//!
//! Generated from type_system.toml
//!
//! This module provides canonical SQL types, type normalization from dialect-specific
//! names to canonical types, implicit cast checking, and dialect-specific type name mapping.

use crate::Dialect;

/// Canonical SQL types for cross-dialect type system.
///
/// These represent the fundamental SQL type categories that can be mapped
/// from various dialect-specific type names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanonicalType {
    Integer,
    Float,
    Text,
    Boolean,
    Timestamp,
    Date,
    Time,
    Binary,
    Json,
    Array,
}

impl CanonicalType {
    /// Returns the canonical type name as a lowercase string.
    pub const fn as_str(&self) -> &'static str {
        match self {
            CanonicalType::Integer => "integer",
            CanonicalType::Float => "float",
            CanonicalType::Text => "text",
            CanonicalType::Boolean => "boolean",
            CanonicalType::Timestamp => "timestamp",
            CanonicalType::Date => "date",
            CanonicalType::Time => "time",
            CanonicalType::Binary => "binary",
            CanonicalType::Json => "json",
            CanonicalType::Array => "array",
        }
    }
}

/// Normalize a type name to its canonical type.
///
/// This function maps any dialect-specific type alias (e.g., "INT64", "VARCHAR",
/// "TIMESTAMPTZ") to its canonical type category.
///
/// # Arguments
///
/// * `type_name` - The type name to normalize (case-insensitive)
///
/// # Returns
///
/// `Some(CanonicalType)` if the type name is recognized, `None` otherwise.
///
/// # Example
///
/// ```ignore
/// use flowscope_core::generated::normalize_type_name;
/// use flowscope_core::generated::CanonicalType;
///
/// assert_eq!(normalize_type_name("INT64"), Some(CanonicalType::Integer));
/// assert_eq!(normalize_type_name("varchar"), Some(CanonicalType::Text));
/// assert_eq!(normalize_type_name("UNKNOWN_TYPE"), None);
/// ```
pub fn normalize_type_name(type_name: &str) -> Option<CanonicalType> {
    let lower = type_name.to_ascii_lowercase();
    match lower.as_str() {
        "array" => Some(CanonicalType::Array),
        "binary" | "varbinary" | "bytea" | "blob" | "bytes" => Some(CanonicalType::Binary),
        "bool" | "boolean" => Some(CanonicalType::Boolean),
        "date" => Some(CanonicalType::Date),
        "float" | "float4" | "float8" | "double" | "real" | "decimal" | "numeric" | "number" => {
            Some(CanonicalType::Float)
        }
        "int" | "int4" | "integer" | "int64" | "bigint" | "smallint" | "tinyint" | "int2"
        | "int8" => Some(CanonicalType::Integer),
        "json" | "jsonb" | "variant" | "object" => Some(CanonicalType::Json),
        "varchar" | "char" | "text" | "string" | "nvarchar" | "nchar" | "character" => {
            Some(CanonicalType::Text)
        }
        "time" | "timetz" => Some(CanonicalType::Time),
        "timestamp" | "timestamptz" | "datetime" | "timestamp_ntz" | "timestamp_ltz"
        | "timestamp_tz" => Some(CanonicalType::Timestamp),
        _ => None,
    }
}

/// Check if a type can be implicitly cast to another type.
///
/// Implicit casts are automatic type conversions that SQL engines perform
/// without requiring explicit CAST expressions. This function returns true
/// if the source type can be implicitly converted to the target type.
///
/// Note: A type can always be implicitly cast to itself (identity cast).
///
/// # Arguments
///
/// * `from` - The source canonical type
/// * `to` - The target canonical type
///
/// # Returns
///
/// `true` if implicit cast is allowed, `false` otherwise.
///
/// # Example
///
/// ```ignore
/// use flowscope_core::generated::{can_implicitly_cast, CanonicalType};
///
/// // Integer can be cast to Float
/// assert!(can_implicitly_cast(CanonicalType::Integer, CanonicalType::Float));
/// // Float cannot be cast to Integer implicitly
/// assert!(!can_implicitly_cast(CanonicalType::Float, CanonicalType::Integer));
/// // Any type can be cast to itself
/// assert!(can_implicitly_cast(CanonicalType::Text, CanonicalType::Text));
/// ```
pub fn can_implicitly_cast(from: CanonicalType, to: CanonicalType) -> bool {
    if from == to {
        return true;
    }
    match from {
        CanonicalType::Boolean => matches!(to, CanonicalType::Text | CanonicalType::Integer),
        CanonicalType::Date => matches!(to, CanonicalType::Timestamp | CanonicalType::Text),
        CanonicalType::Float => matches!(to, CanonicalType::Text),
        CanonicalType::Integer => matches!(to, CanonicalType::Float | CanonicalType::Text),
        CanonicalType::Json => matches!(to, CanonicalType::Text),
        CanonicalType::Time => matches!(to, CanonicalType::Text),
        CanonicalType::Timestamp => matches!(to, CanonicalType::Text),
        _ => false,
    }
}

/// Get the dialect-specific type name for a canonical type.
///
/// This function returns the preferred type name for a given canonical type
/// in a specific SQL dialect.
///
/// # Arguments
///
/// * `dialect` - The SQL dialect
/// * `canonical` - The canonical type
///
/// # Returns
///
/// The dialect-specific type name as a static string.
///
/// # Example
///
/// ```ignore
/// use flowscope_core::generated::{dialect_type_name, CanonicalType};
/// use flowscope_core::Dialect;
///
/// assert_eq!(dialect_type_name(Dialect::Bigquery, CanonicalType::Integer), "INT64");
/// assert_eq!(dialect_type_name(Dialect::Postgres, CanonicalType::Text), "text");
/// ```
pub fn dialect_type_name(dialect: Dialect, canonical: CanonicalType) -> &'static str {
    match canonical {
        CanonicalType::Boolean => match dialect {
            Dialect::Bigquery => "BOOL",
            Dialect::Clickhouse => "Bool",
            Dialect::Databricks => "BOOLEAN",
            Dialect::Duckdb => "BOOLEAN",
            Dialect::Hive => "BOOLEAN",
            Dialect::Mssql => "BIT",
            Dialect::Mysql => "BOOLEAN",
            Dialect::Postgres => "boolean",
            Dialect::Redshift => "BOOLEAN",
            Dialect::Snowflake => "BOOLEAN",
            Dialect::Sqlite => "INTEGER",
            _ => "BOOLEAN",
        },
        CanonicalType::Date => match dialect {
            Dialect::Bigquery => "DATE",
            Dialect::Clickhouse => "Date",
            Dialect::Databricks => "DATE",
            Dialect::Duckdb => "DATE",
            Dialect::Hive => "DATE",
            Dialect::Mssql => "DATE",
            Dialect::Mysql => "DATE",
            Dialect::Postgres => "date",
            Dialect::Redshift => "DATE",
            Dialect::Snowflake => "DATE",
            Dialect::Sqlite => "TEXT",
            _ => "DATE",
        },
        CanonicalType::Float => match dialect {
            Dialect::Bigquery => "FLOAT64",
            Dialect::Clickhouse => "Float64",
            Dialect::Databricks => "DOUBLE",
            Dialect::Duckdb => "DOUBLE",
            Dialect::Hive => "DOUBLE",
            Dialect::Mssql => "FLOAT",
            Dialect::Mysql => "DOUBLE",
            Dialect::Postgres => "double precision",
            Dialect::Redshift => "FLOAT8",
            Dialect::Snowflake => "FLOAT",
            Dialect::Sqlite => "REAL",
            _ => "FLOAT",
        },
        CanonicalType::Integer => match dialect {
            Dialect::Bigquery => "INT64",
            Dialect::Clickhouse => "Int64",
            Dialect::Databricks => "INT",
            Dialect::Duckdb => "INTEGER",
            Dialect::Hive => "INT",
            Dialect::Mssql => "INT",
            Dialect::Mysql => "INT",
            Dialect::Postgres => "integer",
            Dialect::Redshift => "INTEGER",
            Dialect::Snowflake => "INTEGER",
            Dialect::Sqlite => "INTEGER",
            _ => "INTEGER",
        },
        CanonicalType::Text => match dialect {
            Dialect::Bigquery => "STRING",
            Dialect::Clickhouse => "String",
            Dialect::Databricks => "STRING",
            Dialect::Duckdb => "VARCHAR",
            Dialect::Hive => "STRING",
            Dialect::Mssql => "NVARCHAR",
            Dialect::Mysql => "VARCHAR",
            Dialect::Postgres => "text",
            Dialect::Redshift => "VARCHAR",
            Dialect::Snowflake => "VARCHAR",
            Dialect::Sqlite => "TEXT",
            _ => "TEXT",
        },
        CanonicalType::Timestamp => match dialect {
            Dialect::Bigquery => "TIMESTAMP",
            Dialect::Clickhouse => "DateTime",
            Dialect::Databricks => "TIMESTAMP",
            Dialect::Duckdb => "TIMESTAMP",
            Dialect::Hive => "TIMESTAMP",
            Dialect::Mssql => "DATETIME2",
            Dialect::Mysql => "DATETIME",
            Dialect::Postgres => "timestamp",
            Dialect::Redshift => "TIMESTAMP",
            Dialect::Snowflake => "TIMESTAMP_NTZ",
            Dialect::Sqlite => "TEXT",
            _ => "TIMESTAMP",
        },
        CanonicalType::Time => "TIME",
        CanonicalType::Binary => "BINARY",
        CanonicalType::Json => "JSON",
        CanonicalType::Array => "ARRAY",
    }
}
