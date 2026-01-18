use flowscope_core::{
    analyze, completion_context, completion_items, split_statements, AnalyzeRequest, AnalyzeResult,
    CompletionContext, CompletionItemsResult, CompletionRequest, StatementSplitRequest,
    StatementSplitResult,
};
use flowscope_export::{
    export_csv_bundle as export_csv_bundle_internal, export_html as export_html_internal,
    export_json as export_json_internal, export_mermaid as export_mermaid_internal,
    export_sql as export_sql_internal, export_xlsx as export_xlsx_internal, ExportFormat,
    ExportNaming, MermaidView,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Helper for WASM JSON request/response handling with consistent error handling.
///
/// This reduces boilerplate across WASM entry points by handling:
/// - JSON deserialization of the request
/// - Invoking the handler function
/// - JSON serialization of the result
/// - Error handling at each step with appropriate error result types
fn handle_wasm_json_request<Req, Res, ErrRes, F, E>(
    request_json: &str,
    handler: F,
    error_constructor: E,
) -> String
where
    Req: DeserializeOwned,
    Res: Serialize,
    ErrRes: Serialize,
    F: FnOnce(Req) -> Res,
    E: Fn(String) -> ErrRes,
{
    // Parse request
    let request: Req = match serde_json::from_str(request_json) {
        Ok(req) => req,
        Err(e) => {
            let error_result = error_constructor(format!("Invalid request format: {e}"));
            return serde_json::to_string(&error_result)
                .unwrap_or_else(|_| r#"{"error":"Failed to serialize error result"}"#.to_string());
        }
    };

    // Call handler
    let result = handler(request);

    // Serialize result
    serde_json::to_string(&result).unwrap_or_else(|_| {
        let error_result = error_constructor("Failed to serialize result".to_string());
        serde_json::to_string(&error_result)
            .unwrap_or_else(|_| r#"{"error":"Failed to serialize error result"}"#.to_string())
    })
}

/// Request payload for export_to_duckdb_sql.
#[derive(Deserialize)]
struct ExportRequest {
    /// The analysis result to export
    result: AnalyzeResult,
    /// Optional schema name to prefix all tables/views
    #[serde(default)]
    schema: Option<String>,
}

#[derive(Deserialize)]
struct ExportJsonRequest {
    result: AnalyzeResult,
    #[serde(default)]
    compact: bool,
}

#[derive(Deserialize)]
struct ExportMermaidRequest {
    result: AnalyzeResult,
    #[serde(default)]
    view: MermaidViewRequest,
}

#[derive(Deserialize)]
struct ExportHtmlRequest {
    result: AnalyzeResult,
    #[serde(default = "default_project_name")]
    project_name: String,
    #[serde(default)]
    exported_at: Option<String>,
}

#[derive(Deserialize)]
struct ExportCsvRequest {
    result: AnalyzeResult,
}

#[derive(Deserialize)]
struct ExportXlsxRequest {
    result: AnalyzeResult,
}

#[derive(Deserialize)]
struct ExportFilenameRequest {
    #[serde(default = "default_project_name")]
    project_name: String,
    #[serde(default)]
    exported_at: Option<String>,
    format: ExportFormatRequest,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ExportFormatRequest {
    Json {
        #[serde(default)]
        compact: bool,
    },
    Mermaid {
        #[serde(default)]
        view: MermaidViewRequest,
    },
    Html,
    Sql,
    Csv,
    Xlsx,
    Duckdb,
    Png,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum MermaidViewRequest {
    #[default]
    Table,
    Script,
    Column,
    Hybrid,
    All,
}

impl From<MermaidViewRequest> for MermaidView {
    fn from(view: MermaidViewRequest) -> Self {
        match view {
            MermaidViewRequest::Table => MermaidView::Table,
            MermaidViewRequest::Script => MermaidView::Script,
            MermaidViewRequest::Column => MermaidView::Column,
            MermaidViewRequest::Hybrid => MermaidView::Hybrid,
            MermaidViewRequest::All => MermaidView::All,
        }
    }
}

fn default_project_name() -> String {
    "lineage".to_string()
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
    handle_wasm_json_request(
        request_json,
        |req: AnalyzeRequest| analyze(&req),
        |msg| AnalyzeResult::from_error("INVALID_REQUEST", msg),
    )
}

/// Compute completion context for a cursor position.
/// Returns JSON-serialized CompletionContext.
#[wasm_bindgen]
pub fn completion_context_json(request_json: &str) -> String {
    handle_wasm_json_request(
        request_json,
        |req: CompletionRequest| completion_context(&req),
        CompletionContext::from_error,
    )
}

/// Compute ranked completion items for a cursor position.
#[wasm_bindgen]
pub fn completion_items_json(request_json: &str) -> String {
    handle_wasm_json_request(
        request_json,
        |req: CompletionRequest| completion_items(&req),
        CompletionItemsResult::from_error,
    )
}

/// Split SQL into statement spans.
#[wasm_bindgen]
pub fn split_statements_json(request_json: &str) -> String {
    handle_wasm_json_request(
        request_json,
        |req: StatementSplitRequest| split_statements(&req),
        StatementSplitResult::from_error,
    )
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
    let request: ExportRequest = serde_json::from_str(request_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid request JSON: {e}")))?;

    export_sql_internal(&request.result, request.schema.as_deref())
        .map_err(|e| JsValue::from_str(&format!("Export error: {e}")))
}

#[wasm_bindgen]
pub fn export_json(request_json: &str) -> Result<String, JsValue> {
    let request: ExportJsonRequest = serde_json::from_str(request_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid request JSON: {e}")))?;

    export_json_internal(&request.result, request.compact)
        .map_err(|e| JsValue::from_str(&format!("Export error: {e}")))
}

#[wasm_bindgen]
pub fn export_mermaid(request_json: &str) -> Result<String, JsValue> {
    let request: ExportMermaidRequest = serde_json::from_str(request_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid request JSON: {e}")))?;

    export_mermaid_internal(&request.result, request.view.into())
        .map_err(|e| JsValue::from_str(&format!("Export error: {e}")))
}

#[wasm_bindgen]
pub fn export_html(request_json: &str) -> Result<String, JsValue> {
    let request: ExportHtmlRequest = serde_json::from_str(request_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid request JSON: {e}")))?;
    let exported_at = parse_exported_at(request.exported_at.as_deref())?;

    export_html_internal(&request.result, &request.project_name, exported_at)
        .map_err(|e| JsValue::from_str(&format!("Export error: {e}")))
}

#[wasm_bindgen]
pub fn export_csv_bundle(request_json: &str) -> Result<Vec<u8>, JsValue> {
    let request: ExportCsvRequest = serde_json::from_str(request_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid request JSON: {e}")))?;

    export_csv_bundle_internal(&request.result)
        .map_err(|e| JsValue::from_str(&format!("Export error: {e}")))
}

#[wasm_bindgen]
pub fn export_xlsx(request_json: &str) -> Result<Vec<u8>, JsValue> {
    let request: ExportXlsxRequest = serde_json::from_str(request_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid request JSON: {e}")))?;

    export_xlsx_internal(&request.result)
        .map_err(|e| JsValue::from_str(&format!("Export error: {e}")))
}

#[wasm_bindgen]
pub fn export_filename(request_json: &str) -> Result<String, JsValue> {
    let request: ExportFilenameRequest = serde_json::from_str(request_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid request JSON: {e}")))?;
    let exported_at = parse_exported_at(request.exported_at.as_deref())?;

    let naming = ExportNaming::with_exported_at(request.project_name, exported_at);
    let format = match request.format {
        ExportFormatRequest::Json { compact } => ExportFormat::Json { compact },
        ExportFormatRequest::Mermaid { view } => ExportFormat::Mermaid { view: view.into() },
        ExportFormatRequest::Html => ExportFormat::Html,
        ExportFormatRequest::Sql => ExportFormat::Sql { schema: false },
        ExportFormatRequest::Csv => ExportFormat::CsvBundle,
        ExportFormatRequest::Xlsx => ExportFormat::Xlsx,
        ExportFormatRequest::Duckdb => ExportFormat::DuckDb,
        ExportFormatRequest::Png => ExportFormat::Png,
    };

    Ok(naming.filename(format))
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
    export_sql_internal(&result, None).map_err(|e| JsValue::from_str(&format!("Export error: {e}")))
}

fn parse_exported_at(value: Option<&str>) -> Result<chrono::DateTime<chrono::Utc>, JsValue> {
    if let Some(raw) = value {
        let parsed = chrono::DateTime::parse_from_rfc3339(raw)
            .map_err(|e| JsValue::from_str(&format!("Invalid exported_at: {e}")))?;
        Ok(parsed.with_timezone(&chrono::Utc))
    } else {
        Ok(chrono::Utc::now())
    }
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
