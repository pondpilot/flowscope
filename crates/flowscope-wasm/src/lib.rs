use flowscope_core::{analyze, AnalyzeRequest, AnalyzeResult};
use flowscope_export::export_sql;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

/// Request payload for export_to_duckdb_sql.
#[derive(Deserialize)]
struct ExportRequest {
    /// The analysis result to export
    result: AnalyzeResult,
    /// Optional schema name to prefix all tables/views
    #[serde(default)]
    schema: Option<String>,
}

/// Enable tracing logs to the browser console (requires `tracing` feature).
#[wasm_bindgen]
pub fn enable_tracing() {
    #[cfg(feature = "tracing")]
    {
        let _ = tracing_wasm::set_as_global_default();
    }
}

/// Install panic hook for better error messages in browser console
#[wasm_bindgen]
pub fn set_panic_hook() {
    console_error_panic_hook::set_once();
}

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
                format!("Invalid request format: {e}"),
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

/// Export analysis result to SQL statements for DuckDB-WASM.
///
/// Takes a JSON object with:
/// - `result`: The AnalyzeResult to export
/// - `schema` (optional): Schema name to prefix all tables/views (e.g., "lineage")
///
/// Returns SQL statements (DDL + INSERT) that can be executed by duckdb-wasm.
///
/// This is the WASM-compatible export path - generates SQL text that
/// duckdb-wasm can execute to create a queryable database in the browser.
#[wasm_bindgen]
pub fn export_to_duckdb_sql(request_json: &str) -> Result<String, JsValue> {
    // Parse the export request
    let request: ExportRequest = serde_json::from_str(request_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid request JSON: {e}")))?;

    // Generate SQL with optional schema prefix
    export_sql(&request.result, request.schema.as_deref())
        .map_err(|e| JsValue::from_str(&format!("Export error: {e}")))
}

/// Analyze SQL and export to DuckDB SQL statements in one step.
///
/// Convenience function that combines analyze_sql_json + export_to_duckdb_sql.
/// Takes a JSON AnalyzeRequest and returns SQL statements for duckdb-wasm.
///
/// Note: This function does not support the schema parameter. Use
/// analyze_sql_json + export_to_duckdb_sql separately for schema support.
#[wasm_bindgen]
pub fn analyze_and_export_sql(request_json: &str) -> Result<String, JsValue> {
    // Parse the request
    let request: AnalyzeRequest = serde_json::from_str(request_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid request format: {e}")))?;

    // Perform analysis
    let result = analyze(&request);

    // Check for critical errors
    if result.summary.has_errors {
        let issues: Vec<String> = result
            .issues
            .iter()
            .filter(|i| i.severity == flowscope_core::types::Severity::Error)
            .map(|i| i.message.clone())
            .collect();
        return Err(JsValue::from_str(&format!(
            "Analysis failed: {}",
            issues.join("; ")
        )));
    }

    // Generate SQL (no schema prefix in this convenience function)
    export_sql(&result, None).map_err(|e| JsValue::from_str(&format!("Export error: {e}")))
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

    // Note: Tests for export_to_duckdb_sql and analyze_and_export_sql cannot run
    // on native targets because they return Result<_, JsValue> which only works
    // on wasm32. These functions are tested via wasm-pack test.
}
