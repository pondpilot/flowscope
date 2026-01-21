//! MySQL integration tests for the flowscope CLI.
//!
//! These tests require a MySQL database running on localhost:3307.
//! Use `just test-integration-mysql` to run with Docker-managed MySQL.
//!
//! The database is expected to have test tables created by the just recipe.

use std::fs;
use tempfile::tempdir;

use crate::{assert_json_has_lineage, run_cli_success};

/// Get the MySQL connection URL from environment.
/// Falls back to the default test database URL.
fn mysql_url() -> String {
    std::env::var("TEST_MYSQL_URL")
        .unwrap_or_else(|_| "mysql://flowscope:flowscope@localhost:3307/flowscope".to_string())
}

#[test]
fn test_mysql_select_star_expansion() {
    let dir = tempdir().expect("create temp dir");
    let sql_path = dir.path().join("query.sql");

    fs::write(&sql_path, "SELECT * FROM users").expect("write sql file");

    let output = run_cli_success(&[
        "--metadata-url",
        &mysql_url(),
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("id"), "Expected 'id' column in output");
    assert!(stdout.contains("name"), "Expected 'name' column in output");
    assert!(
        stdout.contains("email"),
        "Expected 'email' column in output"
    );
}

#[test]
fn test_mysql_join_lineage() {
    let dir = tempdir().expect("create temp dir");
    let sql_path = dir.path().join("query.sql");

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

    let output = run_cli_success(&[
        "--metadata-url",
        &mysql_url(),
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
fn test_mysql_cte() {
    let dir = tempdir().expect("create temp dir");
    let sql_path = dir.path().join("query.sql");

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

    let output = run_cli_success(&[
        "--metadata-url",
        &mysql_url(),
        "-d",
        "mysql",
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
}

#[test]
fn test_mysql_subquery() {
    let dir = tempdir().expect("create temp dir");
    let sql_path = dir.path().join("query.sql");

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

    let output = run_cli_success(&[
        "--metadata-url",
        &mysql_url(),
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
}

#[test]
fn test_mysql_aggregate() {
    let dir = tempdir().expect("create temp dir");
    let sql_path = dir.path().join("query.sql");

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

    let output = run_cli_success(&[
        "--metadata-url",
        &mysql_url(),
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
}

#[test]
fn test_mysql_window_function() {
    let dir = tempdir().expect("create temp dir");
    let sql_path = dir.path().join("query.sql");

    fs::write(
        &sql_path,
        r#"
        SELECT
            name,
            total,
            ROW_NUMBER() OVER (PARTITION BY user_id ORDER BY created_at DESC) as rn
        FROM users u
        JOIN orders o ON u.id = o.user_id
        "#,
    )
    .expect("write sql file");

    let output = run_cli_success(&[
        "--metadata-url",
        &mysql_url(),
        "-d",
        "mysql",
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
}
