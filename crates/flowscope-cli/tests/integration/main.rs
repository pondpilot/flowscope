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

/// Parse CLI output as JSON.
pub fn parse_json_output(output: &Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "Expected valid JSON output, but parsing failed: {}\nOutput was: {}",
            e, stdout
        )
    })
}

/// Assert that the CLI output is valid JSON with expected lineage structure.
///
/// Parses the output as JSON and verifies it contains either a `columns` or `edges`
/// field, indicating valid lineage data. This is more robust than string matching
/// which could give false positives on error messages containing these words.
pub fn assert_json_has_lineage(output: &Output) {
    let json = parse_json_output(output);

    // Check for expected lineage structure fields
    let has_columns = json.get("columns").is_some();
    let has_edges = json.get("edges").is_some();
    let has_statements = json.get("statements").is_some();
    let has_global_lineage = json.get("globalLineage").is_some();

    assert!(
        has_columns || has_edges || has_statements || has_global_lineage,
        "Expected JSON with lineage structure (columns/edges/statements/globalLineage), got: {:?}",
        json
    );
}

/// Assert that the lineage output contains a specific table by label.
pub fn assert_has_table(output: &Output, table_name: &str) {
    let json = parse_json_output(output);

    let has_table = find_node_by_label(&json, table_name, "table");
    assert!(
        has_table,
        "Expected to find table '{}' in lineage output",
        table_name
    );
}

/// Assert that the lineage output contains a specific column by label.
///
/// Reserved for future column-level integration tests when we add more granular
/// schema introspection validation.
#[allow(dead_code)]
pub fn assert_has_column(output: &Output, column_name: &str) {
    let json = parse_json_output(output);

    let has_column = find_node_by_label(&json, column_name, "column");
    assert!(
        has_column,
        "Expected to find column '{}' in lineage output",
        column_name
    );
}

/// Assert that the lineage output contains at least N data flow edges.
///
/// Reserved for future tests validating edge count in complex multi-statement scenarios.
#[allow(dead_code)]
pub fn assert_has_min_edges(output: &Output, min_count: usize) {
    let json = parse_json_output(output);

    let edge_count = count_edges(&json);
    assert!(
        edge_count >= min_count,
        "Expected at least {} edges, found {}",
        min_count,
        edge_count
    );
}

/// Assert that the lineage output has no errors.
pub fn assert_no_errors(output: &Output) {
    let json = parse_json_output(output);

    if let Some(summary) = json.get("summary") {
        if let Some(issue_count) = summary.get("issueCount") {
            if let Some(errors) = issue_count.get("errors") {
                let error_count = errors.as_u64().unwrap_or(0);
                assert!(
                    error_count == 0,
                    "Expected no errors, found {} errors",
                    error_count
                );
            }
        }
    }
}

/// Assert that the SELECT * was expanded to the expected columns.
///
/// Reserved for future tests validating wildcard expansion with live database metadata.
#[allow(dead_code)]
pub fn assert_star_expansion(output: &Output, expected_columns: &[&str]) {
    let json = parse_json_output(output);

    for col in expected_columns {
        let found = find_node_by_label(&json, col, "column");
        assert!(found, "Expected column '{}' from SELECT * expansion", col);
    }
}

/// Find a node by label and type in the JSON output.
fn find_node_by_label(json: &serde_json::Value, label: &str, node_type: &str) -> bool {
    // Check in statements array
    if let Some(statements) = json.get("statements").and_then(|s| s.as_array()) {
        for stmt in statements {
            if let Some(nodes) = stmt.get("nodes").and_then(|n| n.as_array()) {
                for node in nodes {
                    let node_label = node.get("label").and_then(|l| l.as_str()).unwrap_or("");
                    let ntype = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if node_label == label && ntype == node_type {
                        return true;
                    }
                }
            }
        }
    }

    // Check in globalLineage.nodes
    if let Some(global) = json.get("globalLineage") {
        if let Some(nodes) = global.get("nodes").and_then(|n| n.as_array()) {
            for node in nodes {
                let node_label = node.get("label").and_then(|l| l.as_str()).unwrap_or("");
                let ntype = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if node_label == label && ntype == node_type {
                    return true;
                }
            }
        }
    }

    false
}

/// Count the number of edges in the lineage output.
fn count_edges(json: &serde_json::Value) -> usize {
    let mut count = 0;

    // Count in statements
    if let Some(statements) = json.get("statements").and_then(|s| s.as_array()) {
        for stmt in statements {
            if let Some(edges) = stmt.get("edges").and_then(|e| e.as_array()) {
                count += edges.len();
            }
        }
    }

    count
}
