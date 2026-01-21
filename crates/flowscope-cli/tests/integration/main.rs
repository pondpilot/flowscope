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

/// Assert that the CLI output contains expected JSON lineage data.
pub fn assert_json_has_lineage(output: &Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"columns\"") || stdout.contains("\"edges\""),
        "Expected JSON lineage output, got: {}",
        stdout
    );
}
