use std::process::Command;

use tempfile::tempdir;

#[test]
fn analyzes_multiple_mssql_files_with_trailing_go() {
    let dir = tempdir().expect("temp dir");
    let first = dir.path().join("first.sql");
    let second = dir.path().join("second.sql");
    let sql = "SELECT 1;\nGO\nSELECT 2;\nGO\n";
    std::fs::write(&first, sql).expect("write first SQL file");
    std::fs::write(&second, sql).expect("write second SQL file");

    let output = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args(["-d", "mssql", "-f", "json"])
        .arg(&first)
        .arg(&second)
        .output()
        .expect("run CLI");
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid CLI JSON output");
    let statements = result["statements"].as_array().expect("statements array");
    assert_eq!(statements.len(), 4);
    for (index, statement) in statements.iter().enumerate() {
        assert_eq!(statement["statementIndex"], index);
        let source = statement["sourceName"].as_str().expect("source name");
        let expected = if index < 2 { "first.sql" } else { "second.sql" };
        assert!(source.ends_with(expected), "unexpected source: {source}");
        let offset = if index % 2 == 0 { 0 } else { 13 };
        assert_eq!(statement["span"]["start"], offset);
        assert_eq!(statement["span"]["end"], offset + 8);
    }
    assert!(!result["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .any(|issue| issue["code"] == "PARSE_ERROR"));
}
