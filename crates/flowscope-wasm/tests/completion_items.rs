use flowscope_wasm::completion_items_json;
use serde_json::Value;

#[test]
fn completion_items_json_filters_by_qualifier() {
    let request = serde_json::json!({
        "sql": "SELECT t. FROM users t",
        "dialect": "duckdb",
        "cursorOffset": 9,
        "schema": {
            "defaultSchema": "public",
            "allowImplied": true,
            "tables": [
                {
                    "schema": "public",
                    "name": "users",
                    "columns": [
                        { "name": "id", "dataType": "integer" },
                        { "name": "email", "dataType": "varchar" }
                    ]
                },
                {
                    "schema": "public",
                    "name": "orders",
                    "columns": [
                        { "name": "total", "dataType": "integer" }
                    ]
                }
            ]
        }
    });

    let result_json = completion_items_json(&request.to_string());
    let value: Value = serde_json::from_str(&result_json).expect("valid completion items JSON");

    let items = value
        .get("items")
        .and_then(|items| items.as_array())
        .expect("items array");

    assert!(!items.is_empty());
    assert!(items
        .iter()
        .all(|item| item.get("category") == Some(&Value::String("column".to_string()))));
    assert!(items
        .iter()
        .any(|item| item.get("label") == Some(&Value::String("id".to_string()))));
    assert!(!items
        .iter()
        .any(|item| item.get("label") == Some(&Value::String("total".to_string()))));
}

#[test]
fn completion_items_json_filters_by_schema() {
    let sql = "SELECT public.";
    let request = serde_json::json!({
        "sql": sql,
        "dialect": "duckdb",
        "cursorOffset": sql.len(),
        "schema": {
            "defaultSchema": "public",
            "allowImplied": true,
            "tables": [
                {
                    "schema": "public",
                    "name": "users",
                    "columns": [
                        { "name": "id", "dataType": "integer" }
                    ]
                },
                {
                    "schema": "analytics",
                    "name": "events",
                    "columns": [
                        { "name": "id", "dataType": "integer" }
                    ]
                }
            ]
        }
    });

    let result_json = completion_items_json(&request.to_string());
    let value: Value = serde_json::from_str(&result_json).expect("valid completion items JSON");

    let items = value
        .get("items")
        .and_then(|items| items.as_array())
        .expect("items array");

    assert!(!items.is_empty());
    assert!(items
        .iter()
        .all(|item| { item.get("category") == Some(&Value::String("schemaTable".to_string())) }));
    assert!(items
        .iter()
        .any(|item| item.get("label") == Some(&Value::String("public.users".to_string()))));
    assert!(!items
        .iter()
        .any(|item| item.get("label") == Some(&Value::String("analytics.events".to_string()))));
}
