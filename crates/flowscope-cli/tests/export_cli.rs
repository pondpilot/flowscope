use std::process::Command;

use tempfile::tempdir;

#[test]
fn exports_csv_archive_to_file() {
    let dir = tempdir().expect("temp dir");
    let sql_path = dir.path().join("input.sql");
    let output_path = dir.path().join("lineage.csv.zip");

    std::fs::write(
        &sql_path,
        "SELECT u.id, o.total FROM users u JOIN orders o ON u.id = o.user_id",
    )
    .expect("write sql");

    let status = Command::new(env!("CARGO_BIN_EXE_flowscope"))
        .args([
            "-f",
            "csv",
            "-o",
            output_path.to_str().expect("output path"),
            sql_path.to_str().expect("sql path"),
        ])
        .status()
        .expect("run CLI");

    assert!(status.success());
    let metadata = std::fs::metadata(&output_path).expect("output exists");
    assert!(metadata.len() > 0);
}
