//! SQLx-based metadata provider for live database introspection.
//!
//! Supports PostgreSQL, MySQL, and SQLite databases.

use flowscope_core::{ColumnSchema, SchemaMetadata, SchemaTable};
use sqlx::{AnyPool, Row};
use std::error::Error;

use super::MetadataProvider;

/// Database type inferred from connection URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    Postgres,
    Mysql,
    Sqlite,
}

impl DatabaseType {
    /// Infer database type from a connection URL.
    pub fn from_url(url: &str) -> Option<Self> {
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            Some(Self::Postgres)
        } else if url.starts_with("mysql://") || url.starts_with("mariadb://") {
            Some(Self::Mysql)
        } else if url.starts_with("sqlite://") || url.starts_with("sqlite:") {
            Some(Self::Sqlite)
        } else {
            None
        }
    }
}

/// A metadata provider that uses SQLx to connect to databases and
/// query their system catalogs for schema information.
pub struct SqlxMetadataProvider {
    pool: AnyPool,
    db_type: DatabaseType,
    schema_filter: Option<String>,
}

impl SqlxMetadataProvider {
    /// Create a new provider by connecting to the database at the given URL.
    ///
    /// # Arguments
    /// * `url` - Database connection URL (e.g., `postgres://user:pass@host/db`)
    /// * `schema_filter` - Optional schema name to filter tables (e.g., `public`)
    ///
    /// # Errors
    /// Returns an error if the connection fails or the URL scheme is not supported.
    pub async fn connect(
        url: &str,
        schema_filter: Option<String>,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let db_type = DatabaseType::from_url(url)
            .ok_or_else(|| format!("Unsupported database URL scheme: {url}"))?;

        // Install the SQLx any driver for the detected database type
        match db_type {
            DatabaseType::Postgres => {
                sqlx::any::install_default_drivers();
            }
            DatabaseType::Mysql => {
                sqlx::any::install_default_drivers();
            }
            DatabaseType::Sqlite => {
                sqlx::any::install_default_drivers();
            }
        }

        let pool = AnyPool::connect(url).await?;

        Ok(Self {
            pool,
            db_type,
            schema_filter,
        })
    }

    /// Fetch schema metadata using the appropriate query for the database type.
    pub async fn fetch_schema_async(&self) -> Result<SchemaMetadata, Box<dyn Error + Send + Sync>> {
        let tables = match self.db_type {
            DatabaseType::Postgres => self.fetch_postgres_schema().await?,
            DatabaseType::Mysql => self.fetch_mysql_schema().await?,
            DatabaseType::Sqlite => self.fetch_sqlite_schema().await?,
        };

        Ok(SchemaMetadata {
            default_catalog: None,
            default_schema: self.schema_filter.clone(),
            search_path: None,
            case_sensitivity: None,
            tables,
            allow_implied: false,
        })
    }

    /// Fetch schema from PostgreSQL using information_schema.
    async fn fetch_postgres_schema(
        &self,
    ) -> Result<Vec<SchemaTable>, Box<dyn Error + Send + Sync>> {
        let schema_filter = self.schema_filter.as_deref().unwrap_or("public");

        let query = r#"
            SELECT
                table_schema,
                table_name,
                column_name,
                data_type,
                CASE WHEN pk.column_name IS NOT NULL THEN true ELSE false END AS is_primary_key
            FROM information_schema.columns c
            LEFT JOIN (
                SELECT kcu.table_schema, kcu.table_name, kcu.column_name
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                    ON tc.constraint_name = kcu.constraint_name
                    AND tc.table_schema = kcu.table_schema
                WHERE tc.constraint_type = 'PRIMARY KEY'
            ) pk ON c.table_schema = pk.table_schema
                AND c.table_name = pk.table_name
                AND c.column_name = pk.column_name
            WHERE c.table_schema = $1
            ORDER BY c.table_schema, c.table_name, c.ordinal_position
        "#;

        let rows = sqlx::query(query)
            .bind(schema_filter)
            .fetch_all(&self.pool)
            .await?;

        self.rows_to_tables(rows)
    }

    /// Fetch schema from MySQL using information_schema.
    async fn fetch_mysql_schema(&self) -> Result<Vec<SchemaTable>, Box<dyn Error + Send + Sync>> {
        // For MySQL, if no schema filter is provided, we query the current database
        let query = if self.schema_filter.is_some() {
            r#"
                SELECT
                    TABLE_SCHEMA as table_schema,
                    TABLE_NAME as table_name,
                    COLUMN_NAME as column_name,
                    DATA_TYPE as data_type,
                    CASE WHEN COLUMN_KEY = 'PRI' THEN 1 ELSE 0 END AS is_primary_key
                FROM information_schema.COLUMNS
                WHERE TABLE_SCHEMA = ?
                ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION
            "#
        } else {
            r#"
                SELECT
                    TABLE_SCHEMA as table_schema,
                    TABLE_NAME as table_name,
                    COLUMN_NAME as column_name,
                    DATA_TYPE as data_type,
                    CASE WHEN COLUMN_KEY = 'PRI' THEN 1 ELSE 0 END AS is_primary_key
                FROM information_schema.COLUMNS
                WHERE TABLE_SCHEMA = DATABASE()
                ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION
            "#
        };

        let rows = if let Some(ref schema) = self.schema_filter {
            sqlx::query(query)
                .bind(schema)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query(query).fetch_all(&self.pool).await?
        };

        self.rows_to_tables(rows)
    }

    /// Fetch schema from SQLite using sqlite_master and pragma_table_info.
    async fn fetch_sqlite_schema(&self) -> Result<Vec<SchemaTable>, Box<dyn Error + Send + Sync>> {
        // First, get all table names
        let tables_query = r#"
            SELECT name FROM sqlite_master
            WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
            ORDER BY name
        "#;

        let table_rows = sqlx::query(tables_query).fetch_all(&self.pool).await?;

        let mut tables = Vec::new();

        for table_row in table_rows {
            let table_name: String = table_row.get("name");

            // Get column info for each table using pragma_table_info
            // Note: We need to use dynamic SQL here since pragma_table_info is a table-valued function
            let columns_query = format!("PRAGMA table_info('{}')", table_name.replace('\'', "''"));

            let column_rows = sqlx::query(&columns_query).fetch_all(&self.pool).await?;

            let columns: Vec<ColumnSchema> = column_rows
                .iter()
                .map(|row| {
                    let name: String = row.get("name");
                    let data_type: String = row.get("type");
                    let pk: i32 = row.get("pk");

                    ColumnSchema {
                        name,
                        data_type: if data_type.is_empty() {
                            None
                        } else {
                            Some(data_type)
                        },
                        is_primary_key: if pk > 0 { Some(true) } else { None },
                        foreign_key: None,
                    }
                })
                .collect();

            tables.push(SchemaTable {
                catalog: None,
                schema: None, // SQLite doesn't have schemas in the same way
                name: table_name,
                columns,
            });
        }

        Ok(tables)
    }

    /// Convert database rows to SchemaTable structures.
    /// Works for PostgreSQL and MySQL which have similar information_schema layouts.
    fn rows_to_tables(
        &self,
        rows: Vec<sqlx::any::AnyRow>,
    ) -> Result<Vec<SchemaTable>, Box<dyn Error + Send + Sync>> {
        use std::collections::HashMap;

        // Group columns by (schema, table)
        let mut table_map: HashMap<(String, String), Vec<ColumnSchema>> = HashMap::new();

        for row in rows {
            let table_schema: String = row.get("table_schema");
            let table_name: String = row.get("table_name");
            let column_name: String = row.get("column_name");
            let data_type: String = row.get("data_type");

            // Handle is_primary_key which might be bool or int depending on database
            let is_primary_key = self.get_primary_key_from_row(&row);

            let column = ColumnSchema {
                name: column_name,
                data_type: Some(data_type),
                is_primary_key: if is_primary_key { Some(true) } else { None },
                foreign_key: None,
            };

            table_map
                .entry((table_schema, table_name))
                .or_default()
                .push(column);
        }

        // Convert to Vec<SchemaTable>
        let mut tables: Vec<SchemaTable> = table_map
            .into_iter()
            .map(|((schema, name), columns)| SchemaTable {
                catalog: None,
                schema: Some(schema),
                name,
                columns,
            })
            .collect();

        // Sort for deterministic output
        tables.sort_by(|a, b| {
            let schema_cmp = a.schema.cmp(&b.schema);
            if schema_cmp == std::cmp::Ordering::Equal {
                a.name.cmp(&b.name)
            } else {
                schema_cmp
            }
        });

        Ok(tables)
    }

    /// Extract primary key status from a row, handling different database representations.
    fn get_primary_key_from_row(&self, row: &sqlx::any::AnyRow) -> bool {
        // Try to get as bool first (PostgreSQL), then as integer (MySQL)
        if let Ok(val) = row.try_get::<bool, _>("is_primary_key") {
            return val;
        }
        if let Ok(val) = row.try_get::<i32, _>("is_primary_key") {
            return val != 0;
        }
        if let Ok(val) = row.try_get::<i64, _>("is_primary_key") {
            return val != 0;
        }
        false
    }
}

impl MetadataProvider for SqlxMetadataProvider {
    fn fetch_schema(&self) -> Result<SchemaMetadata, Box<dyn Error + Send + Sync>> {
        // Create a runtime to run the async code synchronously
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(self.fetch_schema_async())
    }
}

/// Connect to a database and fetch its schema.
///
/// This is the main entry point for CLI usage.
///
/// # Arguments
/// * `url` - Database connection URL
/// * `schema_filter` - Optional schema name to filter tables
///
/// # Returns
/// The fetched schema metadata, or an error if connection/query fails.
pub fn fetch_metadata_from_database(
    url: &str,
    schema_filter: Option<String>,
) -> Result<SchemaMetadata, Box<dyn Error + Send + Sync>> {
    let rt = tokio::runtime::Runtime::new()?;
    let provider = rt.block_on(SqlxMetadataProvider::connect(url, schema_filter))?;
    provider.fetch_schema()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_type_from_url() {
        assert_eq!(
            DatabaseType::from_url("postgres://localhost/db"),
            Some(DatabaseType::Postgres)
        );
        assert_eq!(
            DatabaseType::from_url("postgresql://localhost/db"),
            Some(DatabaseType::Postgres)
        );
        assert_eq!(
            DatabaseType::from_url("mysql://localhost/db"),
            Some(DatabaseType::Mysql)
        );
        assert_eq!(
            DatabaseType::from_url("mariadb://localhost/db"),
            Some(DatabaseType::Mysql)
        );
        assert_eq!(
            DatabaseType::from_url("sqlite://path/to/db"),
            Some(DatabaseType::Sqlite)
        );
        assert_eq!(
            DatabaseType::from_url("sqlite::memory:"),
            Some(DatabaseType::Sqlite)
        );
        assert_eq!(DatabaseType::from_url("unknown://localhost/db"), None);
    }
}
