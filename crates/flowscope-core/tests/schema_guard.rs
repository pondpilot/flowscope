use flowscope_core::{AnalyzeRequest, AnalyzeResult};
use schemars::generate::SchemaSettings;
use serde_json::json;

const SNAPSHOT: &str = include_str!("../../../docs/api_schema.json");

fn generate_schema() -> serde_json::Value {
    let settings = SchemaSettings::draft07();
    let generator = settings.into_generator();
    json!({
        "AnalyzeRequest": generator.clone().into_root_schema_for::<AnalyzeRequest>(),
        "AnalyzeResult": generator.into_root_schema_for::<AnalyzeResult>(),
    })
}

#[test]
fn api_schema_snapshot_matches() {
    let generated = generate_schema();

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
    let generated = generate_schema();

    println!(
        "{}",
        serde_json::to_string_pretty(&generated).expect("serialize schema")
    );
}
