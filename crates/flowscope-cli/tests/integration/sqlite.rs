//! SQLite integration tests for the flowscope CLI.
//!
//! These tests create temporary SQLite databases and verify the CLI
//! can fetch metadata and analyze queries against them.

use rusqlite::Connection;
use std::fs;
use tempfile::tempdir;

use crate::{assert_has_table, assert_json_has_lineage, assert_no_errors, run_cli_success};

/// Create a test SQLite database with sample tables.
fn create_test_db(path: &std::path::Path) {
    let conn = Connection::open(path).expect("open sqlite db");

    conn.execute_batch(
        r#"
        CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT UNIQUE
        );

        CREATE TABLE orders (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL REFERENCES users(id),
            total REAL NOT NULL,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE order_items (
            id INTEGER PRIMARY KEY,
            order_id INTEGER NOT NULL REFERENCES orders(id),
            product_name TEXT NOT NULL,
            quantity INTEGER NOT NULL,
            price REAL NOT NULL
        );
        "#,
    )
    .expect("create test tables");
}

#[test]
fn test_sqlite_select_star_expansion() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("test.db");
    let sql_path = dir.path().join("query.sql");

    create_test_db(&db_path);

    fs::write(&sql_path, "SELECT * FROM users").expect("write sql file");

    let sqlite_url = format!("sqlite://{}", db_path.display());
    let output = run_cli_success(&[
        "--metadata-url",
        &sqlite_url,
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
    assert_no_errors(&output);
    assert_has_table(&output, "users");

    // Verify the columns were expanded from SELECT *
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("id"), "Expected 'id' column in output");
    assert!(stdout.contains("name"), "Expected 'name' column in output");
    assert!(
        stdout.contains("email"),
        "Expected 'email' column in output"
    );
}

#[test]
fn test_sqlite_join_lineage() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("test.db");
    let sql_path = dir.path().join("query.sql");

    create_test_db(&db_path);

    fs::write(
        &sql_path,
        r#"
        SELECT
            u.name,
            o.total,
            oi.product_name
        FROM users u
        JOIN orders o ON u.id = o.user_id
        JOIN order_items oi ON o.id = oi.order_id
        "#,
    )
    .expect("write sql file");

    let sqlite_url = format!("sqlite://{}", db_path.display());
    let output = run_cli_success(&[
        "--metadata-url",
        &sqlite_url,
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("name"), "Expected 'name' in lineage");
    assert!(stdout.contains("total"), "Expected 'total' in lineage");
    assert!(
        stdout.contains("product_name"),
        "Expected 'product_name' in lineage"
    );
}

#[test]
fn test_sqlite_subquery() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("test.db");
    let sql_path = dir.path().join("query.sql");

    create_test_db(&db_path);

    fs::write(
        &sql_path,
        r#"
        SELECT name, email
        FROM users
        WHERE id IN (
            SELECT user_id FROM orders WHERE total > 100
        )
        "#,
    )
    .expect("write sql file");

    let sqlite_url = format!("sqlite://{}", db_path.display());
    let output = run_cli_success(&[
        "--metadata-url",
        &sqlite_url,
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
}

#[test]
fn test_sqlite_cte() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("test.db");
    let sql_path = dir.path().join("query.sql");

    create_test_db(&db_path);

    fs::write(
        &sql_path,
        r#"
        WITH user_totals AS (
            SELECT user_id, SUM(total) as total_spent
            FROM orders
            GROUP BY user_id
        )
        SELECT u.name, ut.total_spent
        FROM users u
        JOIN user_totals ut ON u.id = ut.user_id
        "#,
    )
    .expect("write sql file");

    let sqlite_url = format!("sqlite://{}", db_path.display());
    let output = run_cli_success(&[
        "--metadata-url",
        &sqlite_url,
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
}

#[test]
fn test_sqlite_aggregate() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("test.db");
    let sql_path = dir.path().join("query.sql");

    create_test_db(&db_path);

    fs::write(
        &sql_path,
        r#"
        SELECT
            u.name,
            COUNT(o.id) as order_count,
            SUM(o.total) as total_spent
        FROM users u
        LEFT JOIN orders o ON u.id = o.user_id
        GROUP BY u.id, u.name
        "#,
    )
    .expect("write sql file");

    let sqlite_url = format!("sqlite://{}", db_path.display());
    let output = run_cli_success(&[
        "--metadata-url",
        &sqlite_url,
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
}

#[test]
fn test_sqlite_skips_tables_with_special_characters() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("test.db");
    let sql_path = dir.path().join("query.sql");

    create_test_db(&db_path);

    let conn = Connection::open(&db_path).expect("open sqlite db");
    conn.execute(
        "CREATE TABLE \"order-items\" (id INTEGER PRIMARY KEY, note TEXT)",
        [],
    )
    .expect("create table with dash");

    fs::write(
        &sql_path,
        r#"
        SELECT id, name
        FROM users
        ORDER BY id
        "#,
    )
    .expect("write sql file");

    let sqlite_url = format!("sqlite://{}", db_path.display());
    let output = run_cli_success(&[
        "--metadata-url",
        &sqlite_url,
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Skipping SQLite table 'order-items'"),
        "Expected warning about skipped table, stderr was: {}",
        stderr
    );
}
