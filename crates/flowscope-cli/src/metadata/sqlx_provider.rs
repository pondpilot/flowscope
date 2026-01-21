//! SQLx-based metadata provider for live database introspection.
//!
//! Supports PostgreSQL, MySQL, and SQLite databases.

use anyhow::{anyhow, Context, Result};
use flowscope_core::{ColumnSchema, SchemaMetadata, SchemaTable};
use sqlx::{any::AnyPoolOptions, AnyPool, Row};
use std::sync::Once;
use std::time::Duration;

/// Maximum number of concurrent database connections.
/// CLI tools run sequential queries; 2 connections handles metadata + query execution.
const MAX_CONNECTIONS: u32 = 2;

/// Timeout for acquiring a connection from the pool (seconds).
/// Also serves as an implicit connect timeout since acquisition waits for connection.
const ACQUIRE_TIMEOUT_SECS: u64 = 10;

/// Safe maximum length for MySQL identifier truncation.
/// MySQL limits identifiers to 64 chars by default, max 256 with special configuration.
/// We use 255 as a safe upper bound that works with SQLx Any driver's VARCHAR coercion.
const MYSQL_IDENTIFIER_SAFE_LENGTH: usize = 255;

/// Guard for one-time SQLx driver installation.
static INSTALL_DRIVERS: Once = Once::new();

/// Extract the URL scheme for error messages (avoids exposing credentials).
fn url_scheme(url: &str) -> &str {
    url.split("://").next().unwrap_or("unknown")
}

/// Redact credentials from a database URL for safe error reporting.
///
/// Transforms `postgres://user:password@host/db` into `postgres://<redacted>@host/db`.
/// If no credentials are present, returns a scheme-only identifier.
fn redact_url(url: &str) -> String {
    if let Some((scheme, rest)) = url.split_once("://") {
        // Find the last '@' to handle passwords containing '@' characters
        if let Some(at_pos) = rest.rfind('@') {
            let host_and_path = &rest[at_pos + 1..];
            return format!("{}://<redacted>@{}", scheme, host_and_path);
        }
        // No credentials in URL, but still redact the path for file-based DBs
        if scheme == "sqlite" {
            return format!("{}://<path>", scheme);
        }
        return format!("{}://{}", scheme, rest);
    }
    // Handle sqlite: URLs without :// (e.g., sqlite::memory:, sqlite:path)
    if url.starts_with("sqlite:") {
        return "sqlite:<path>".to_string();
    }
    url_scheme(url).to_string()
}

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
    pub async fn connect(url: &str, schema_filter: Option<String>) -> Result<Self> {
        let db_type = DatabaseType::from_url(url)
            .ok_or_else(|| anyhow!("Unsupported database URL scheme: {}", url_scheme(url)))?;

        // Install SQLx drivers exactly once (thread-safe)
        INSTALL_DRIVERS.call_once(sqlx::any::install_default_drivers);

        // Note: SQLx's AnyPoolOptions doesn't support connect_timeout directly.
        // The acquire_timeout covers the waiting time which includes initial connection.
        let pool = AnyPoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            .acquire_timeout(Duration::from_secs(ACQUIRE_TIMEOUT_SECS))
            .connect(url)
            .await
            .with_context(|| format!("Failed to connect to database: {}", redact_url(url)))?;

        Ok(Self {
            pool,
            db_type,
            schema_filter,
        })
    }

    /// Fetch schema metadata using the appropriate query for the database type.
    pub async fn fetch_schema_async(&self) -> Result<SchemaMetadata> {
        let tables = match self.db_type {
            DatabaseType::Postgres => self.fetch_postgres_schema().await?,
            DatabaseType::Mysql => self.fetch_mysql_schema().await?,
            DatabaseType::Sqlite => self.fetch_sqlite_schema().await?,
        };

        let default_schema = self.resolve_default_schema().await?;

        Ok(SchemaMetadata {
            default_catalog: None,
            default_schema,
            search_path: None,
            case_sensitivity: None,
            tables,
            allow_implied: false,
        })
    }

    /// Fetch schema from PostgreSQL using information_schema.
    async fn fetch_postgres_schema(&self) -> Result<Vec<SchemaTable>> {
        let schema_filter = self.schema_filter.as_deref().unwrap_or("public");

        // Cast to text for SQLx Any driver compatibility (Name type not supported)
        let query = r#"
            SELECT
                c.table_schema::text AS table_schema,
                c.table_name::text AS table_name,
                c.column_name::text AS column_name,
                c.data_type::text AS data_type,
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
    async fn fetch_mysql_schema(&self) -> Result<Vec<SchemaTable>> {
        // For MySQL, if no schema filter is provided, we query the current database.
        // Use LEFT(..., N) to coerce columns to VARCHAR for SQLx Any driver compatibility
        // (information_schema uses longtext which Any driver maps to BLOB and can't decode).
        // See MYSQL_IDENTIFIER_SAFE_LENGTH for the limit rationale.
        let limit = MYSQL_IDENTIFIER_SAFE_LENGTH;
        let query = if self.schema_filter.is_some() {
            format!(
                r#"
                SELECT
                    LEFT(TABLE_SCHEMA, {limit}) as table_schema,
                    LEFT(TABLE_NAME, {limit}) as table_name,
                    LEFT(COLUMN_NAME, {limit}) as column_name,
                    LEFT(DATA_TYPE, {limit}) as data_type,
                    CASE WHEN COLUMN_KEY = 'PRI' THEN 1 ELSE 0 END AS is_primary_key
                FROM information_schema.COLUMNS
                WHERE TABLE_SCHEMA = ?
                ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION
            "#
            )
        } else {
            format!(
                r#"
                SELECT
                    LEFT(TABLE_SCHEMA, {limit}) as table_schema,
                    LEFT(TABLE_NAME, {limit}) as table_name,
                    LEFT(COLUMN_NAME, {limit}) as column_name,
                    LEFT(DATA_TYPE, {limit}) as data_type,
                    CASE WHEN COLUMN_KEY = 'PRI' THEN 1 ELSE 0 END AS is_primary_key
                FROM information_schema.COLUMNS
                WHERE TABLE_SCHEMA = DATABASE()
                ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION
            "#
            )
        };

        let rows = if let Some(ref schema) = self.schema_filter {
            sqlx::query(&query)
                .bind(schema)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query(&query).fetch_all(&self.pool).await?
        };

        self.rows_to_tables(rows)
    }

    /// Validate SQLite table name to prevent injection in PRAGMA queries.
    ///
    /// This validation is intentionally conservative: it only allows alphanumeric
    /// characters, underscores, and dots (for attached databases). While SQLite
    /// supports more exotic identifiers (spaces, quotes, etc.) when properly quoted,
    /// we restrict to safe characters because:
    ///
    /// 1. Table names come from `sqlite_master`, not user input, so exotic names are rare
    /// 2. Conservative validation is safer than complex quoting logic
    /// 3. Most real-world schemas use simple identifiers
    ///
    /// Tables with exotic names will be skipped with a warning on stderr.
    fn validate_sqlite_table_name(name: &str) -> Result<()> {
        if name.is_empty() || name.len() > MYSQL_IDENTIFIER_SAFE_LENGTH {
            return Err(anyhow!("Invalid table name length: {}", name.len()));
        }
        // Allow alphanumeric, underscore, and dot (for attached databases)
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        {
            return Err(anyhow!("Table name contains invalid characters: {}", name));
        }
        Ok(())
    }

    /// Fetch schema from SQLite using sqlite_master and pragma_table_info.
    async fn fetch_sqlite_schema(&self) -> Result<Vec<SchemaTable>> {
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

            // Validate table name before using in dynamic SQL
            if let Err(err) = Self::validate_sqlite_table_name(&table_name) {
                eprintln!(
                    "flowscope: warning: Skipping SQLite table '{table_name}' due to unsupported identifier characters: {err}"
                );
                continue;
            }

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

    /// Determine the default schema that should be used for canonicalization.
    async fn resolve_default_schema(&self) -> Result<Option<String>> {
        if let Some(schema) = &self.schema_filter {
            return Ok(Some(schema.clone()));
        }

        match self.db_type {
            DatabaseType::Mysql => self.fetch_mysql_default_schema().await,
            _ => Ok(None),
        }
    }

    /// Return the currently selected MySQL database (if any) to use as the default schema.
    async fn fetch_mysql_default_schema(&self) -> Result<Option<String>> {
        let schema: Option<String> = sqlx::query_scalar("SELECT DATABASE()")
            .fetch_one(&self.pool)
            .await?;

        Ok(schema)
    }

    /// Convert database rows to SchemaTable structures.
    /// Works for PostgreSQL and MySQL which have similar information_schema layouts.
    fn rows_to_tables(&self, rows: Vec<sqlx::any::AnyRow>) -> Result<Vec<SchemaTable>> {
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
) -> Result<SchemaMetadata> {
    let rt = tokio::runtime::Runtime::new().context("Failed to create async runtime")?;
    rt.block_on(async {
        let provider = SqlxMetadataProvider::connect(url, schema_filter).await?;
        provider.fetch_schema_async().await
    })
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

    #[test]
    fn test_redact_url_with_credentials() {
        // Should redact user:password
        assert_eq!(
            redact_url("postgres://user:password@localhost:5432/mydb"),
            "postgres://<redacted>@localhost:5432/mydb"
        );

        // Should redact even complex passwords
        assert_eq!(
            redact_url("mysql://admin:s3cr3t!@#$@db.example.com/prod"),
            "mysql://<redacted>@db.example.com/prod"
        );
    }

    #[test]
    fn test_redact_url_without_credentials() {
        // No credentials, keep host info
        assert_eq!(
            redact_url("postgres://localhost:5432/mydb"),
            "postgres://localhost:5432/mydb"
        );
    }

    #[test]
    fn test_redact_url_sqlite() {
        // SQLite paths with :// should be redacted
        assert_eq!(
            redact_url("sqlite:///path/to/secret/database.db"),
            "sqlite://<path>"
        );

        // SQLite paths without :// (e.g., sqlite::memory:) should also be redacted
        assert_eq!(redact_url("sqlite::memory:"), "sqlite:<path>");
        assert_eq!(redact_url("sqlite:path/to/db"), "sqlite:<path>");
    }

    #[test]
    fn test_redact_url_invalid() {
        // Invalid URLs should return scheme only
        assert_eq!(redact_url("not-a-url"), "not-a-url");
        assert_eq!(redact_url("unknown"), "unknown");
    }

    #[test]
    fn test_url_scheme() {
        assert_eq!(url_scheme("postgres://localhost/db"), "postgres");
        assert_eq!(url_scheme("mysql://localhost/db"), "mysql");
        assert_eq!(url_scheme("sqlite://path"), "sqlite");
        assert_eq!(url_scheme("not-a-url"), "not-a-url");
    }

    // =========================================================================
    // SQLite Table Name Validation Tests
    // =========================================================================

    #[test]
    fn test_validate_sqlite_table_name_valid_simple() {
        // Simple alphanumeric names should pass
        assert!(SqlxMetadataProvider::validate_sqlite_table_name("users").is_ok());
        assert!(SqlxMetadataProvider::validate_sqlite_table_name("Users").is_ok());
        assert!(SqlxMetadataProvider::validate_sqlite_table_name("USERS").is_ok());
        assert!(SqlxMetadataProvider::validate_sqlite_table_name("users123").is_ok());
        assert!(SqlxMetadataProvider::validate_sqlite_table_name("123users").is_ok());
    }

    #[test]
    fn test_validate_sqlite_table_name_valid_with_underscore() {
        // Underscores are allowed
        assert!(SqlxMetadataProvider::validate_sqlite_table_name("user_accounts").is_ok());
        assert!(SqlxMetadataProvider::validate_sqlite_table_name("_private").is_ok());
        assert!(SqlxMetadataProvider::validate_sqlite_table_name("table_").is_ok());
        assert!(SqlxMetadataProvider::validate_sqlite_table_name("__double__").is_ok());
    }

    #[test]
    fn test_validate_sqlite_table_name_valid_with_dot() {
        // Dots are allowed for attached database syntax (e.g., "main.users")
        assert!(SqlxMetadataProvider::validate_sqlite_table_name("main.users").is_ok());
        assert!(SqlxMetadataProvider::validate_sqlite_table_name("schema.table").is_ok());
        assert!(SqlxMetadataProvider::validate_sqlite_table_name("db.schema.table").is_ok());
    }

    #[test]
    fn test_validate_sqlite_table_name_rejects_empty() {
        let result = SqlxMetadataProvider::validate_sqlite_table_name("");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("length"));
    }

    #[test]
    fn test_validate_sqlite_table_name_rejects_too_long() {
        // Names over 255 chars should be rejected
        let long_name = "a".repeat(256);
        let result = SqlxMetadataProvider::validate_sqlite_table_name(&long_name);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("length"));

        // 255 chars should be OK
        let max_name = "a".repeat(255);
        assert!(SqlxMetadataProvider::validate_sqlite_table_name(&max_name).is_ok());
    }

    #[test]
    fn test_validate_sqlite_table_name_rejects_spaces() {
        let result = SqlxMetadataProvider::validate_sqlite_table_name("user accounts");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid characters"));
    }

    #[test]
    fn test_validate_sqlite_table_name_rejects_quotes() {
        // Single quotes - potential SQL injection
        let result = SqlxMetadataProvider::validate_sqlite_table_name("users'--");
        assert!(result.is_err());

        // Double quotes
        let result = SqlxMetadataProvider::validate_sqlite_table_name("users\"table");
        assert!(result.is_err());

        // Backticks
        let result = SqlxMetadataProvider::validate_sqlite_table_name("users`table");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_sqlite_table_name_rejects_semicolon() {
        // Semicolon could enable statement injection
        let result = SqlxMetadataProvider::validate_sqlite_table_name("users;DROP TABLE");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_sqlite_table_name_rejects_special_chars() {
        // Various special characters that should be rejected
        let invalid_names = [
            "users@domain",
            "users#tag",
            "users$var",
            "users%percent",
            "users&amp",
            "users*star",
            "users(paren",
            "users)paren",
            "users+plus",
            "users=equals",
            "users[bracket",
            "users]bracket",
            "users{brace",
            "users}brace",
            "users|pipe",
            "users\\backslash",
            "users/slash",
            "users?question",
            "users<less",
            "users>greater",
            "users,comma",
            "users:colon",
            "users!bang",
            "users~tilde",
            "users\ttab",
            "users\nnewline",
        ];

        for name in invalid_names {
            let result = SqlxMetadataProvider::validate_sqlite_table_name(name);
            assert!(
                result.is_err(),
                "Expected '{}' to be rejected but it was accepted",
                name
            );
        }
    }

    // =========================================================================
    // MySQL Identifier Length Constant Tests
    // =========================================================================

    #[test]
    fn test_mysql_identifier_safe_length_constant() {
        // Verify the constant is set correctly
        assert_eq!(MYSQL_IDENTIFIER_SAFE_LENGTH, 255);

        // Verify it's within MySQL's documented limits (64 default, 256 max)
        // Using const block to satisfy clippy assertions_on_constants
        const _: () = {
            assert!(MYSQL_IDENTIFIER_SAFE_LENGTH <= 256);
            assert!(MYSQL_IDENTIFIER_SAFE_LENGTH >= 64);
        };
    }

    // =========================================================================
    // Error Message Quality Tests
    // =========================================================================
    // These tests verify that error messages are safe (no credentials) but informative.

    #[test]
    fn test_error_context_uses_redacted_url() {
        // Verify that the redact_url function produces appropriate context
        // for common connection failure scenarios.

        // PostgreSQL with credentials - should show host but not password
        let pg_url = "postgres://admin:super_secret_password@db.example.com:5432/production";
        let redacted = redact_url(pg_url);
        assert!(
            redacted.contains("db.example.com"),
            "Redacted URL should preserve host for debugging"
        );
        assert!(
            !redacted.contains("super_secret_password"),
            "Redacted URL must not contain password"
        );
        assert!(
            !redacted.contains("admin"),
            "Redacted URL should not contain username"
        );

        // MySQL with credentials
        let mysql_url = "mysql://root:mysql_root_pw@mysql.internal:3306/app_db";
        let redacted = redact_url(mysql_url);
        assert!(redacted.contains("mysql.internal"));
        assert!(!redacted.contains("mysql_root_pw"));
        assert!(!redacted.contains("root"));

        // SQLite file path - should not expose filesystem structure
        let sqlite_url = "sqlite:///home/user/secrets/private.db";
        let redacted = redact_url(sqlite_url);
        assert!(!redacted.contains("/home/user/secrets"));
        assert!(redacted.contains("sqlite"));
    }

    #[test]
    fn test_redact_url_with_at_sign_in_password() {
        // Passwords may contain '@' characters, ensure we handle this correctly
        // by using rfind to find the last '@' (the separator)
        let url = "postgres://user:p@ss@word@localhost/db";
        let redacted = redact_url(url);
        assert_eq!(redacted, "postgres://<redacted>@localhost/db");
        assert!(!redacted.contains("p@ss@word"));
    }

    #[test]
    fn test_redact_url_preserves_port_and_database() {
        // Error messages should include port and database for debugging
        let url = "postgres://user:pass@host:5433/mydb?sslmode=require";
        let redacted = redact_url(url);
        assert!(
            redacted.contains("5433"),
            "Port should be preserved for debugging"
        );
        assert!(
            redacted.contains("mydb"),
            "Database name should be preserved for debugging"
        );
    }

    #[tokio::test]
    async fn test_connection_error_includes_redacted_url() {
        // Attempt to connect to a non-existent database and verify
        // the error message includes redacted URL, not credentials.
        let url = "postgres://secret_user:secret_password@nonexistent.invalid:5432/testdb";

        let result = SqlxMetadataProvider::connect(url, None).await;

        let error_message = match result {
            Ok(_) => panic!("Connection to nonexistent host should fail"),
            Err(e) => e.to_string(),
        };

        // The error should mention the host for debugging
        assert!(
            error_message.contains("nonexistent.invalid"),
            "Error should include host for debugging: {}",
            error_message
        );

        // The error should NOT contain credentials
        assert!(
            !error_message.contains("secret_user"),
            "Error must not expose username: {}",
            error_message
        );
        assert!(
            !error_message.contains("secret_password"),
            "Error must not expose password: {}",
            error_message
        );

        // The error should indicate it's a connection failure
        assert!(
            error_message.contains("Failed to connect")
                || error_message.contains("connect")
                || error_message.contains("database"),
            "Error should indicate connection failure: {}",
            error_message
        );
    }
}
