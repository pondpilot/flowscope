//! PostgreSQL integration tests for the flowscope CLI.
//!
//! These tests require a PostgreSQL database running on localhost:5433.
//! Use `just test-integration-postgres` to run with Docker-managed Postgres.
//!
//! The database is expected to have test tables created by the just recipe.

use std::fs;
use tempfile::tempdir;

use crate::{assert_json_has_lineage, run_cli_success};

/// Get the PostgreSQL connection URL from environment.
/// Falls back to the default test database URL.
fn postgres_url() -> String {
    std::env::var("TEST_POSTGRES_URL")
        .unwrap_or_else(|_| "postgres://flowscope:flowscope@localhost:5433/flowscope".to_string())
}

#[test]
fn test_postgres_select_star_expansion() {
    let dir = tempdir().expect("create temp dir");
    let sql_path = dir.path().join("query.sql");

    fs::write(&sql_path, "SELECT * FROM users").expect("write sql file");

    let output = run_cli_success(&[
        "--metadata-url",
        &postgres_url(),
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
fn test_postgres_schema_qualified() {
    let dir = tempdir().expect("create temp dir");
    let sql_path = dir.path().join("query.sql");

    fs::write(&sql_path, "SELECT * FROM public.users").expect("write sql file");

    let output = run_cli_success(&[
        "--metadata-url",
        &postgres_url(),
        "--metadata-schema",
        "public",
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
}

#[test]
fn test_postgres_join_lineage() {
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
        &postgres_url(),
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
fn test_postgres_cte() {
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
        &postgres_url(),
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
}

#[test]
fn test_postgres_window_function() {
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
        &postgres_url(),
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
}

#[test]
fn test_postgres_aggregate_with_filter() {
    let dir = tempdir().expect("create temp dir");
    let sql_path = dir.path().join("query.sql");

    fs::write(
        &sql_path,
        r#"
        SELECT
            u.name,
            COUNT(*) as total_orders,
            COUNT(*) FILTER (WHERE o.total > 100) as large_orders
        FROM users u
        LEFT JOIN orders o ON u.id = o.user_id
        GROUP BY u.id, u.name
        "#,
    )
    .expect("write sql file");

    let output = run_cli_success(&[
        "--metadata-url",
        &postgres_url(),
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
}

#[test]
fn test_postgres_lateral_join() {
    let dir = tempdir().expect("create temp dir");
    let sql_path = dir.path().join("query.sql");

    fs::write(
        &sql_path,
        r#"
        SELECT u.name, recent.total
        FROM users u
        CROSS JOIN LATERAL (
            SELECT total
            FROM orders o
            WHERE o.user_id = u.id
            ORDER BY created_at DESC
            LIMIT 1
        ) recent
        "#,
    )
    .expect("write sql file");

    let output = run_cli_success(&[
        "--metadata-url",
        &postgres_url(),
        "-d",
        "postgres",
        "-f",
        "json",
        sql_path.to_str().unwrap(),
    ]);

    assert_json_has_lineage(&output);
}
