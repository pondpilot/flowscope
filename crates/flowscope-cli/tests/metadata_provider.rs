//! Integration tests for the live database metadata provider.
//!
//! These tests are marked with `#[ignore]` by default because they require
//! real database connections. To run them:
//!
//! ```bash
//! # PostgreSQL test
//! DATABASE_URL=postgres://user:pass@localhost/testdb cargo test -p flowscope-cli test_postgres_metadata -- --ignored
//!
//! # SQLite test (uses in-memory database)
//! cargo test -p flowscope-cli test_sqlite_metadata -- --ignored
//! ```

#![cfg(feature = "metadata-provider")]

use std::process::Command;
use tempfile::tempdir;

/// Test metadata fetching from a PostgreSQL database.
///
/// Requires `DATABASE_URL` environment variable to be set.
#[test]
#[ignore = "Requires a PostgreSQL database connection"]
fn test_postgres_metadata_provider() {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for this test");

    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("query.sql");

    // Write a simple SELECT * query that needs schema resolution
    std::fs::write(&sql_path, "SELECT * FROM users").expect("write sql");

    let status = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--metadata-url",
            &database_url,
            "-f",
            "json",
            sql_path.to_str().expect("sql path"),
        ])
        .status()
        .expect("run CLI");

    assert!(status.success(), "CLI should succeed with metadata URL");
}

/// Test metadata fetching from a MySQL database.
///
/// Requires `MYSQL_URL` environment variable to be set.
#[test]
#[ignore = "Requires a MySQL database connection"]
fn test_mysql_metadata_provider() {
    let database_url = std::env::var("MYSQL_URL").expect("MYSQL_URL must be set for this test");

    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("query.sql");

    std::fs::write(&sql_path, "SELECT * FROM users").expect("write sql");

    let status = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--metadata-url",
            &database_url,
            "-f",
            "json",
            sql_path.to_str().expect("sql path"),
        ])
        .status()
        .expect("run CLI");

    assert!(
        status.success(),
        "CLI should succeed with MySQL metadata URL"
    );
}

/// Test metadata fetching from a SQLite database.
///
/// This test creates a temporary SQLite database with test tables.
#[test]
#[ignore = "Requires SQLite and creates a temporary database"]
fn test_sqlite_metadata_provider() {
    let dir = tempdir().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let sql_path = dir.path().join("query.sql");

    // Create a SQLite database with some tables using the sqlite3 CLI
    let setup_result = Command::new("sqlite3")
        .arg(&db_path)
        .arg(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT);
             CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER, total REAL);",
        )
        .status();

    if setup_result.is_err() {
        eprintln!("sqlite3 CLI not available, skipping test");
        return;
    }

    // Write a query that uses SELECT *
    std::fs::write(
        &sql_path,
        "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
    )
    .expect("write sql");

    let sqlite_url = format!("sqlite://{}", db_path.display());

    let status = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--metadata-url",
            &sqlite_url,
            "-f",
            "json",
            sql_path.to_str().expect("sql path"),
        ])
        .status()
        .expect("run CLI");

    assert!(
        status.success(),
        "CLI should succeed with SQLite metadata URL"
    );
}

/// Test that --metadata-url and --schema can coexist (metadata-url takes precedence).
#[test]
#[ignore = "Requires a PostgreSQL database connection"]
fn test_metadata_url_takes_precedence() {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for this test");

    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("query.sql");
    let schema_path = dir.path().join("schema.sql");

    // Write a minimal schema file
    std::fs::write(&schema_path, "CREATE TABLE dummy (id INT);").expect("write schema");
    std::fs::write(&sql_path, "SELECT * FROM users").expect("write sql");

    // Both flags provided - metadata-url should take precedence
    let status = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--metadata-url",
            &database_url,
            "-s",
            schema_path.to_str().expect("schema path"),
            "-f",
            "json",
            sql_path.to_str().expect("sql path"),
        ])
        .status()
        .expect("run CLI");

    assert!(
        status.success(),
        "CLI should succeed when both metadata-url and schema are provided"
    );
}

/// Test metadata-schema filter option.
#[test]
#[ignore = "Requires a PostgreSQL database connection"]
fn test_metadata_schema_filter() {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for this test");

    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("query.sql");

    std::fs::write(&sql_path, "SELECT * FROM users").expect("write sql");

    let status = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--metadata-url",
            &database_url,
            "--metadata-schema",
            "public",
            "-f",
            "json",
            sql_path.to_str().expect("sql path"),
        ])
        .status()
        .expect("run CLI");

    assert!(
        status.success(),
        "CLI should succeed with metadata-schema filter"
    );
}
