use flowscope_core::{analyze, AnalyzeRequest, AnalyzeResult};
use wasm_bindgen::prelude::*;

/// Main analysis entry point - accepts JSON request, returns JSON result
/// This function never throws - errors are returned in the result's issues array
#[wasm_bindgen]
pub fn analyze_sql_json(request_json: &str) -> String {
    // Parse the request
    let request: AnalyzeRequest = match serde_json::from_str(request_json) {
        Ok(req) => req,
        Err(e) => {
            let result = AnalyzeResult::from_error(
                "REQUEST_PARSE_ERROR",
                "Invalid request format".to_string(),
            );
            return serde_json::to_string(&result)
                .unwrap_or_else(|_| r#"{"error":"Failed to serialize error result"}"#.to_string());
        }
    };

    // Perform analysis
    let result = analyze(&request);

    // Serialize result
    serde_json::to_string(&result).unwrap_or_else(|_| {
        let error_result = AnalyzeResult::from_error(
            "SERIALIZATION_ERROR",
            "Failed to serialize result".to_string(),
        );
        serde_json::to_string(&error_result)
            .unwrap_or_else(|_| r#"{"error":"Failed to serialize error result"}"#.to_string())
    })
}

/// Legacy simple API - accepts SQL string, returns JSON with table names
/// Kept for backwards compatibility
#[wasm_bindgen]
pub fn analyze_sql(sql_input: &str) -> Result<String, JsValue> {
    // Parse the SQL
    let statements = flowscope_core::parse_sql(sql_input)
        .map_err(|e| JsValue::from_str(&format!("Parse error: {e}")))?;

    // Extract table lineage
    let tables = flowscope_core::extract_tables(&statements);

    // Create result
    let result = flowscope_core::LineageResult::new(tables);

    // Serialize to JSON
    serde_json::to_string(&result)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}")))
}

/// Get version information
#[wasm_bindgen]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_sql_legacy() {
        let sql = "SELECT * FROM users";
        let result = analyze_sql(sql);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("users"));
    }

    #[test]
    fn test_analyze_sql_json_simple() {
        let request = r#"{"sql": "SELECT * FROM users", "dialect": "generic"}"#;
        let result = analyze_sql_json(request);

        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        // Should have statements array
        assert!(parsed["statements"].is_array());
        assert_eq!(parsed["statements"].as_array().unwrap().len(), 1);

        // Should have users table
        let nodes = &parsed["statements"][0]["nodes"];
        assert!(nodes.is_array());

        // Should not have errors
        assert!(!parsed["summary"]["hasErrors"].as_bool().unwrap());
    }

    #[test]
    fn test_analyze_sql_json_with_dialect() {
        let request = r#"{"sql": "SELECT * FROM users", "dialect": "postgres"}"#;
        let result = analyze_sql_json(request);

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(!parsed["summary"]["hasErrors"].as_bool().unwrap());
    }

    #[test]
    fn test_analyze_sql_json_invalid_sql() {
        let request = r#"{"sql": "SELECT * FROM", "dialect": "generic"}"#;
        let result = analyze_sql_json(request);

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["summary"]["hasErrors"].as_bool().unwrap());
        assert!(!parsed["issues"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_analyze_sql_json_invalid_request() {
        let request = r#"{"not_valid": true}"#;
        let result = analyze_sql_json(request);

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["summary"]["hasErrors"].as_bool().unwrap());
    }

    #[test]
    fn test_analyze_sql_json_complex() {
        let request = r#"{
            "sql": "WITH cte AS (SELECT * FROM users) INSERT INTO archive SELECT * FROM cte",
            "dialect": "postgres"
        }"#;
        let result = analyze_sql_json(request);

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(!parsed["summary"]["hasErrors"].as_bool().unwrap());

        // Should have nodes and edges
        let statement = &parsed["statements"][0];
        assert!(!statement["nodes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_get_version() {
        let version = get_version();
        assert!(!version.is_empty());
    }
}
