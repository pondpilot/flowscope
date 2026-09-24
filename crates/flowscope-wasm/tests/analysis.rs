use flowscope_wasm::{analyze_sql_json, split_statements_json};
use serde_json::Value;

#[test]
fn analyze_sql_json_handles_mssql_go_batch_separators() {
    let request = serde_json::json!({
        "sql": "SELECT 1;\nGO\nSELECT 2;\nGO\n",
        "dialect": "mssql"
    });

    let result: Value = serde_json::from_str(&analyze_sql_json(&request.to_string()))
        .expect("analysis result should be valid JSON");
    let statements = result
        .get("statements")
        .and_then(Value::as_array)
        .expect("analysis result should contain statements");
    let issues = result
        .get("issues")
        .and_then(Value::as_array)
        .expect("analysis result should contain issues");

    assert_eq!(statements.len(), 2);
    assert!(!issues
        .iter()
        .any(|issue| { issue.get("code") == Some(&Value::String("PARSE_ERROR".to_string())) }));
}

#[test]
fn split_statements_json_handles_mssql_go_batch_separators() {
    let sql = "SELECT 1;\nGO\nSELECT 2;\nGO\n";
    let request = serde_json::json!({
        "sql": sql,
        "dialect": "mssql"
    });

    let result: Value = serde_json::from_str(&split_statements_json(&request.to_string()))
        .expect("statement split result should be valid JSON");
    let statements = result
        .get("statements")
        .and_then(Value::as_array)
        .expect("statement split result should contain statements");

    assert_eq!(statements.len(), 2);
    assert_eq!(statements[0]["start"], 0);
    assert_eq!(statements[0]["end"], 8);
    assert_eq!(statements[1]["start"], 13);
    assert_eq!(statements[1]["end"], 21);
}
