//! Alias visibility and scoping rules per dialect.
//!
//! Generated from scoping_rules.toml

use crate::Dialect;

impl Dialect {
    /// Whether SELECT aliases can be referenced in GROUP BY.
    pub const fn alias_in_group_by(&self) -> bool {
        match self {
            Dialect::Bigquery => true,
            Dialect::Clickhouse => true,
            Dialect::Databricks => true,
            Dialect::Duckdb => true,
            Dialect::Hive => true,
            Dialect::Mssql => false,
            Dialect::Mysql => true,
            Dialect::Postgres => false,
            Dialect::Redshift => true,
            Dialect::Snowflake => false,
            Dialect::Sqlite => true,
            _ => false, // Default: strict (Postgres-like)
        }
    }

    /// Whether SELECT aliases can be referenced in HAVING.
    pub const fn alias_in_having(&self) -> bool {
        match self {
            Dialect::Bigquery => true,
            Dialect::Clickhouse => true,
            Dialect::Databricks => true,
            Dialect::Duckdb => true,
            Dialect::Hive => true,
            Dialect::Mssql => false,
            Dialect::Mysql => true,
            Dialect::Postgres => false,
            Dialect::Redshift => true,
            Dialect::Snowflake => false,
            Dialect::Sqlite => true,
            _ => false,
        }
    }

    /// Whether SELECT aliases can be referenced in ORDER BY.
    pub const fn alias_in_order_by(&self) -> bool {
        match self {
            Dialect::Bigquery => true,
            Dialect::Clickhouse => true,
            Dialect::Databricks => true,
            Dialect::Duckdb => true,
            Dialect::Hive => true,
            Dialect::Mssql => true,
            Dialect::Mysql => true,
            Dialect::Postgres => true,
            Dialect::Redshift => true,
            Dialect::Snowflake => true,
            Dialect::Sqlite => true,
            _ => true, // ORDER BY alias is widely supported
        }
    }

    /// Whether lateral column aliases are supported (referencing earlier SELECT items).
    pub const fn lateral_column_alias(&self) -> bool {
        match self {
            Dialect::Bigquery => true,
            Dialect::Clickhouse => true,
            Dialect::Databricks => true,
            Dialect::Duckdb => true,
            Dialect::Hive => true,
            Dialect::Mssql => false,
            Dialect::Mysql => false,
            Dialect::Postgres => false,
            Dialect::Redshift => false,
            Dialect::Snowflake => true,
            Dialect::Sqlite => false,
            _ => false,
        }
    }
}
