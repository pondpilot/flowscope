//! CLI integration tests for templating functionality.

use std::process::Command;
use tempfile::tempdir;

#[test]
fn template_dbt_ref_macro() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("model.sql");

    std::fs::write(&sql_path, "SELECT * FROM {{ ref('users') }}").expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--template",
            "dbt",
            "-f",
            "json",
            sql_path.to_str().expect("sql path"),
        ])
        .output()
        .expect("run CLI");

    assert!(output.status.success(), "CLI should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(r#""label":"users""#) || stdout.contains(r#""label": "users""#),
        "Should detect 'users' table from ref(): {stdout}"
    );
}

#[test]
fn template_dbt_source_macro() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("model.sql");

    std::fs::write(&sql_path, "SELECT * FROM {{ source('raw', 'events') }}").expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--template",
            "dbt",
            "-f",
            "json",
            sql_path.to_str().expect("sql path"),
        ])
        .output()
        .expect("run CLI");

    assert!(output.status.success(), "CLI should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("raw.events"),
        "Should detect 'raw.events' table from source(): {stdout}"
    );
}

#[test]
fn template_jinja_with_variable() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("query.sql");

    std::fs::write(&sql_path, "SELECT * FROM {{ table_name }}").expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--template",
            "jinja",
            "--template-var",
            "table_name=orders",
            "-f",
            "json",
            sql_path.to_str().expect("sql path"),
        ])
        .output()
        .expect("run CLI");

    assert!(output.status.success(), "CLI should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(r#""label":"orders""#) || stdout.contains(r#""label": "orders""#),
        "Should detect 'orders' table from template var: {stdout}"
    );
}

#[test]
fn template_jinja_undefined_var_reports_error() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("query.sql");

    std::fs::write(&sql_path, "SELECT * FROM {{ undefined_table }}").expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--template",
            "jinja",
            "-f",
            "json",
            sql_path.to_str().expect("sql path"),
        ])
        .output()
        .expect("run CLI");

    // Should exit with error code 1 (has_errors)
    assert_eq!(
        output.status.code(),
        Some(1),
        "Should report template error"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("TEMPLATE_ERROR"),
        "Should report TEMPLATE_ERROR: {stdout}"
    );
}

#[test]
fn template_dbt_complex_model() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("model.sql");

    std::fs::write(
        &sql_path,
        r#"{{ config(materialized='table') }}

WITH stg AS (
    SELECT * FROM {{ ref('stg_orders') }}
)
SELECT id, amount FROM stg"#,
    )
    .expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--template",
            "dbt",
            "-f",
            "json",
            sql_path.to_str().expect("sql path"),
        ])
        .output()
        .expect("run CLI");

    assert!(output.status.success(), "CLI should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("stg_orders"),
        "Should detect 'stg_orders' from ref(): {stdout}"
    );
}
