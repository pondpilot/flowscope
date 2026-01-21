//! Integration tests for flowscope CLI with real databases.
//!
//! These tests verify the CLI works correctly with live database connections.
//! Run with: `just test-integration`
//!
//! These tests are behind the `integration-tests` feature flag and won't run
//! with regular `cargo test`. Use the just recipes to run them.

#![cfg(feature = "integration-tests")]

mod mysql;
mod postgres;
mod sqlite;

use std::process::{Command, Output};

/// Run the flowscope CLI with the given arguments and return the output.
pub fn run_cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args(args)
        .output()
        .expect("failed to execute flowscope CLI")
}

/// Run the flowscope CLI and assert it succeeds.
pub fn run_cli_success(args: &[&str]) -> Output {
    let output = run_cli(args);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!(
            "CLI failed with status {:?}\nstderr: {}\nstdout: {}",
            output.status.code(),
            stderr,
            stdout
        );
    }
    output
}

/// Assert that the CLI output is valid JSON with expected lineage structure.
///
/// Parses the output as JSON and verifies it contains either a `columns` or `edges`
/// field, indicating valid lineage data. This is more robust than string matching
/// which could give false positives on error messages containing these words.
pub fn assert_json_has_lineage(output: &Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse as JSON to verify it's actually valid JSON, not just text containing keywords
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "Expected valid JSON output, but parsing failed: {}\nOutput was: {}",
            e, stdout
        )
    });

    // Check for expected lineage structure fields
    let has_columns = json.get("columns").is_some();
    let has_edges = json.get("edges").is_some();

    assert!(
        has_columns || has_edges,
        "Expected JSON with 'columns' or 'edges' field, got: {}",
        stdout
    );
}
