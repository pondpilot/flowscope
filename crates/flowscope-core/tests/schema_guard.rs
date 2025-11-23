use flowscope_core::{AnalyzeRequest, AnalyzeResult};
use schemars::schema_for;
use serde_json::json;

const SNAPSHOT: &str = include_str!("../../../docs/api_schema.json");

#[test]
fn api_schema_snapshot_matches() {
    let generated = json!({
        "AnalyzeRequest": schema_for!(AnalyzeRequest),
        "AnalyzeResult": schema_for!(AnalyzeResult),
    });

    let expected: serde_json::Value =
        serde_json::from_str(SNAPSHOT).expect("invalid bundled API schema snapshot");

    assert_eq!(
        generated, expected,
        "API schema changed. Regenerate docs/api_schema.json if this is intentional."
    );
}

#[test]
#[ignore]
fn regenerate_api_schema_snapshot() {
    let generated = json!({
        "AnalyzeRequest": schema_for!(AnalyzeRequest),
        "AnalyzeResult": schema_for!(AnalyzeResult),
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&generated).expect("serialize schema")
    );
}
