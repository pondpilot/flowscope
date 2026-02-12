use std::process::Command;

use tempfile::tempdir;

/// SQL that triggers LINT_AM_001 (bare UNION).
const SQL_WITH_VIOLATIONS: &str = "SELECT 1 UNION SELECT 2";

/// Clean SQL with no lint violations.
const SQL_CLEAN: &str = "SELECT 1";
/// Invalid SQL used to verify parser/analysis errors fail lint mode.
const SQL_INVALID: &str = "SELECT FROM";

#[test]
fn test_lint_clean_file() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("clean.sql");
    std::fs::write(&sql_path, SQL_CLEAN).expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args(["--lint", sql_path.to_str().expect("sql path")])
        .output()
        .expect("run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Expected exit 0, got: {stdout}");
    assert!(stdout.contains("PASS"), "Expected PASS in output: {stdout}");
    assert!(
        stdout.contains("0 violations"),
        "Expected 0 violations: {stdout}"
    );
}

#[test]
fn test_lint_file_with_violations() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("bad.sql");
    std::fs::write(&sql_path, SQL_WITH_VIOLATIONS).expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args(["--lint", sql_path.to_str().expect("sql path")])
        .output()
        .expect("run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit 1, got: {stdout}"
    );
    assert!(stdout.contains("FAIL"), "Expected FAIL in output: {stdout}");
    assert!(
        stdout.contains("LINT_AM_001"),
        "Expected LINT_AM_001: {stdout}"
    );
    assert!(
        stdout.contains("1 violations"),
        "Expected 1 violation: {stdout}"
    );
}

#[test]
fn test_lint_invalid_sql_fails() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("invalid.sql");
    std::fs::write(&sql_path, SQL_INVALID).expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args(["--lint", sql_path.to_str().expect("sql path")])
        .output()
        .expect("run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit 1 for invalid SQL, got: {stdout}"
    );
    assert!(stdout.contains("FAIL"), "Expected FAIL in output: {stdout}");
    assert!(
        stdout.contains("1 file failed"),
        "Expected failed summary for invalid SQL: {stdout}"
    );
}

#[test]
fn test_lint_exclude_rules() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("excluded.sql");
    std::fs::write(&sql_path, SQL_WITH_VIOLATIONS).expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--lint",
            "--exclude-rules",
            "LINT_AM_001",
            sql_path.to_str().expect("sql path"),
        ])
        .output()
        .expect("run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "Expected exit 0 when rule excluded, got: {stdout}"
    );
    assert!(
        stdout.contains("PASS"),
        "Expected PASS when rule excluded: {stdout}"
    );
}

#[test]
fn test_lint_fix_respects_exclude_rules() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("excluded_fix.sql");
    std::fs::write(&sql_path, SQL_WITH_VIOLATIONS).expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--lint",
            "--fix",
            "--exclude-rules",
            "LINT_AM_001",
            sql_path.to_str().expect("sql path"),
        ])
        .output()
        .expect("run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "Expected exit 0 when excluded rule is the only violation, got: {stdout}"
    );

    let after = std::fs::read_to_string(&sql_path).expect("read SQL after fix");
    assert_eq!(
        after, SQL_WITH_VIOLATIONS,
        "Expected file to remain unchanged when fix rule is excluded"
    );
}

#[test]
fn test_lint_fix_excluded_rule_not_rewritten_when_other_fixes_apply() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("excluded_mixed_fix.sql");
    let sql = "SELECT COUNT(1) FROM t WHERE a<>b";
    std::fs::write(&sql_path, sql).expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--lint",
            "--fix",
            "--exclude-rules",
            "LINT_CV_005",
            sql_path.to_str().expect("sql path"),
        ])
        .output()
        .expect("run CLI");

    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit 1 due remaining non-excluded violations"
    );

    let after = std::fs::read_to_string(&sql_path).expect("read SQL after fix");
    assert!(
        after.contains("COUNT(*)"),
        "Expected non-excluded fix to apply: {after}"
    );
    assert!(
        after.contains("<>"),
        "Expected excluded CV_005 to remain '<>' (not '!='): {after}"
    );
    assert!(
        !after.contains("!="),
        "Expected excluded CV_005 to avoid not-equal rewrite: {after}"
    );
}

#[test]
fn test_lint_output_file_has_no_ansi_sequences() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("bad.sql");
    let report_path = dir.path().join("lint.txt");
    std::fs::write(&sql_path, SQL_WITH_VIOLATIONS).expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--lint",
            "--output",
            report_path.to_str().expect("report path"),
            sql_path.to_str().expect("sql path"),
        ])
        .output()
        .expect("run CLI");

    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit 1 for violations"
    );

    let report = std::fs::read_to_string(report_path).expect("read lint report");
    assert!(
        !report.contains('\u{1b}'),
        "Expected no ANSI escape sequences in output file: {report}"
    );
}

#[test]
fn test_lint_json_format() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("json.sql");
    std::fs::write(&sql_path, SQL_WITH_VIOLATIONS).expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--lint",
            "--format",
            "json",
            sql_path.to_str().expect("sql path"),
        ])
        .output()
        .expect("run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit 1 for violations: {stdout}"
    );

    // Validate it's valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Expected valid JSON output");
    let arr = parsed.as_array().expect("Expected JSON array");
    assert_eq!(arr.len(), 1);
    assert!(!arr[0]["violations"].as_array().unwrap().is_empty());
}

#[test]
fn test_lint_unsupported_format() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("test.sql");
    std::fs::write(&sql_path, SQL_CLEAN).expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--lint",
            "--format",
            "html",
            sql_path.to_str().expect("sql path"),
        ])
        .output()
        .expect("run CLI");

    assert_eq!(
        output.status.code(),
        Some(66),
        "Expected exit 66 for unsupported format"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("only supports"),
        "Expected helpful error message: {stderr}"
    );
}

#[test]
fn test_lint_unsupported_format_fails_before_fix_mutation() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("test.sql");
    std::fs::write(&sql_path, SQL_WITH_VIOLATIONS).expect("write sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--lint",
            "--fix",
            "--format",
            "html",
            sql_path.to_str().expect("sql path"),
        ])
        .output()
        .expect("run CLI");

    assert_eq!(
        output.status.code(),
        Some(66),
        "Expected exit 66 for unsupported lint format"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("only supports"),
        "Expected helpful error message: {stderr}"
    );

    let after = std::fs::read_to_string(&sql_path).expect("read SQL after failed command");
    assert_eq!(
        after, SQL_WITH_VIOLATIONS,
        "Expected file to remain unchanged for unsupported format"
    );
}

#[test]
fn test_lint_stdin() {
    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args(["--lint"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(SQL_WITH_VIOLATIONS.as_bytes())
                .unwrap();
            child.wait_with_output()
        })
        .expect("run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit 1 for stdin violations: {stdout}"
    );
    assert!(
        stdout.contains("LINT_AM_001"),
        "Expected LINT_AM_001 from stdin: {stdout}"
    );
}

#[test]
fn test_lint_multiple_files() {
    let dir = tempdir().expect("temp dir");
    let clean_path = dir.path().join("clean.sql");
    let bad_path = dir.path().join("bad.sql");
    std::fs::write(&clean_path, SQL_CLEAN).expect("write clean sql");
    std::fs::write(&bad_path, SQL_WITH_VIOLATIONS).expect("write bad sql");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "--lint",
            clean_path.to_str().expect("clean path"),
            bad_path.to_str().expect("bad path"),
        ])
        .output()
        .expect("run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit 1 when any file fails: {stdout}"
    );
    assert!(
        stdout.contains("PASS"),
        "Expected PASS for clean file: {stdout}"
    );
    assert!(
        stdout.contains("FAIL"),
        "Expected FAIL for bad file: {stdout}"
    );
    assert!(
        stdout.contains("1 file passed"),
        "Expected 1 file passed: {stdout}"
    );
    assert!(
        stdout.contains("1 file failed"),
        "Expected 1 file failed: {stdout}"
    );
}

#[test]
fn test_lint_directory_recursively() {
    let dir = tempdir().expect("temp dir");
    let nested = dir.path().join("nested");
    std::fs::create_dir_all(&nested).expect("create nested directory");

    let clean_path = dir.path().join("clean.sql");
    let bad_path = nested.join("bad.sql");
    let ignored = nested.join("notes.txt");

    std::fs::write(&clean_path, SQL_CLEAN).expect("write clean sql");
    std::fs::write(&bad_path, SQL_WITH_VIOLATIONS).expect("write bad sql");
    std::fs::write(&ignored, SQL_WITH_VIOLATIONS).expect("write ignored file");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args(["--lint", dir.path().to_str().expect("dir path")])
        .output()
        .expect("run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit 1 when one discovered SQL file fails: {stdout}"
    );
    assert!(
        stdout.contains("1 file passed"),
        "Expected one clean SQL file in recursive lint output: {stdout}"
    );
    assert!(
        stdout.contains("1 file failed"),
        "Expected one failing SQL file in recursive lint output: {stdout}"
    );
    assert!(
        stdout.contains("LINT_AM_001"),
        "Expected lint violation from nested SQL file: {stdout}"
    );
    assert!(
        !stdout.contains("notes.txt"),
        "Expected non-sql files to be ignored: {stdout}"
    );
}
