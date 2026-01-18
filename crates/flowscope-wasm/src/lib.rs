mod encoding;

use encoding::{convert_spans_to_utf16, utf16_to_utf8_offset, Encoding};
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

// WASM wrapper types with encoding support
// These wrap the core request types and add optional encoding field

/// WASM wrapper for AnalyzeRequest with encoding support.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WasmAnalyzeRequest {
    #[serde(flatten)]
    inner: AnalyzeRequest,
    /// Text encoding for span offsets in the response.
    /// When `utf16`, all span offsets are converted to UTF-16 code units.
    #[serde(default)]
    encoding: Encoding,
}

/// WASM wrapper for CompletionRequest with encoding support.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WasmCompletionRequest {
    sql: String,
    dialect: flowscope_core::Dialect,
    /// Cursor offset - interpreted based on `encoding` field.
    cursor_offset: usize,
    #[serde(default)]
    schema: Option<flowscope_core::SchemaMetadata>,
    /// Text encoding for cursor offset and response spans.
    #[serde(default)]
    encoding: Encoding,
}

/// WASM wrapper for StatementSplitRequest with encoding support.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WasmStatementSplitRequest {
    #[serde(flatten)]
    inner: StatementSplitRequest,
    /// Text encoding for span offsets in the response.
    #[serde(default)]
    encoding: Encoding,
}

/// Helper for processing completion requests with encoding support.
///
/// Handles the common pattern of:
/// 1. Parsing WasmCompletionRequest
/// 2. Converting cursor offset if UTF-16
/// 3. Building core CompletionRequest
/// 4. Calling the provided handler
/// 5. Serializing and converting spans if needed
fn handle_completion_request<T, F>(
    request_json: &str,
    handler: F,
    make_error: impl Fn(String) -> T,
) -> String
where
    T: serde::Serialize,
    F: FnOnce(&CompletionRequest) -> T,
{
    // Parse request
    let wasm_req: WasmCompletionRequest = match serde_json::from_str(request_json) {
        Ok(req) => req,
        Err(e) => {
            let err = make_error(format!("Invalid request format: {e}"));
            return serde_json::to_string(&err)
                .unwrap_or_else(|_| r#"{"error":"Failed to serialize error result"}"#.to_string());
        }
    };

    let encoding = wasm_req.encoding;
    let sql = wasm_req.sql.clone();

    // Convert cursor_offset if UTF-16
    let cursor_offset = if encoding == Encoding::Utf16 {
        match utf16_to_utf8_offset(&sql, wasm_req.cursor_offset) {
            Ok(offset) => offset,
            Err(e) => {
                let err = make_error(e);
                return serde_json::to_string(&err)
                    .unwrap_or_else(|_| r#"{"error":"Failed to serialize error result"}"#.to_string());
            }
        }
    } else {
        wasm_req.cursor_offset
    };

    // Build core request
    let core_req = CompletionRequest {
        sql: wasm_req.sql,
        dialect: wasm_req.dialect,
        cursor_offset,
        schema: wasm_req.schema,
    };

    // Call handler
    let result = handler(&core_req);

    // Serialize result
    let mut json_value = match serde_json::to_value(&result) {
        Ok(v) => v,
        Err(_) => {
            let err = make_error("Failed to serialize result".to_string());
            return serde_json::to_string(&err)
                .unwrap_or_else(|_| r#"{"error":"Failed to serialize error result"}"#.to_string());
        }
    };

    // Convert spans if UTF-16 encoding requested
    if encoding == Encoding::Utf16 {
        convert_spans_to_utf16(&sql, &mut json_value);
    }

    serde_json::to_string(&json_value)
        .unwrap_or_else(|_| r#"{"error":"Failed to serialize result"}"#.to_string())
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
///
/// Supports optional `encoding` field in request:
/// - `"utf8"` (default): All span offsets are UTF-8 byte offsets
/// - `"utf16"`: All span offsets are converted to UTF-16 code units
#[wasm_bindgen]
pub fn analyze_sql_json(request_json: &str) -> String {
    // Parse request to extract encoding
    let wasm_req: WasmAnalyzeRequest = match serde_json::from_str(request_json) {
        Ok(req) => req,
        Err(e) => {
            let error_result = AnalyzeResult::from_error("INVALID_REQUEST", format!("Invalid request format: {e}"));
            return serde_json::to_string(&error_result)
                .unwrap_or_else(|_| r#"{"error":"Failed to serialize error result"}"#.to_string());
        }
    };

    let encoding = wasm_req.encoding;
    let sql = wasm_req.inner.sql.clone();

    // Call handler
    let result = analyze(&wasm_req.inner);

    // Serialize result
    let mut json_value = match serde_json::to_value(&result) {
        Ok(v) => v,
        Err(_) => {
            let error_result = AnalyzeResult::from_error("INVALID_REQUEST", "Failed to serialize result");
            return serde_json::to_string(&error_result)
                .unwrap_or_else(|_| r#"{"error":"Failed to serialize error result"}"#.to_string());
        }
    };

    // Convert spans if UTF-16 encoding requested
    if encoding == Encoding::Utf16 {
        convert_spans_to_utf16(&sql, &mut json_value);
    }

    serde_json::to_string(&json_value)
        .unwrap_or_else(|_| r#"{"error":"Failed to serialize result"}"#.to_string())
}

/// Compute completion context for a cursor position.
/// Returns JSON-serialized CompletionContext.
///
/// Supports optional `encoding` field in request:
/// - `"utf8"` (default): cursor_offset is UTF-8 bytes, spans are UTF-8 bytes
/// - `"utf16"`: cursor_offset is UTF-16 code units, spans are UTF-16 code units
#[wasm_bindgen]
pub fn completion_context_json(request_json: &str) -> String {
    handle_completion_request(
        request_json,
        completion_context,
        CompletionContext::from_error,
    )
}

/// Compute ranked completion items for a cursor position.
///
/// Supports optional `encoding` field in request:
/// - `"utf8"` (default): cursor_offset is UTF-8 bytes, spans are UTF-8 bytes
/// - `"utf16"`: cursor_offset is UTF-16 code units, spans are UTF-16 code units
#[wasm_bindgen]
pub fn completion_items_json(request_json: &str) -> String {
    handle_completion_request(
        request_json,
        completion_items,
        CompletionItemsResult::from_error,
    )
}

/// Split SQL into statement spans.
///
/// Supports optional `encoding` field in request:
/// - `"utf8"` (default): All span offsets are UTF-8 byte offsets
/// - `"utf16"`: All span offsets are converted to UTF-16 code units
#[wasm_bindgen]
pub fn split_statements_json(request_json: &str) -> String {
    // Parse request to extract encoding
    let wasm_req: WasmStatementSplitRequest = match serde_json::from_str(request_json) {
        Ok(req) => req,
        Err(e) => {
            return serde_json::to_string(&StatementSplitResult::from_error(format!(
                "Invalid request format: {e}"
            )))
            .unwrap_or_else(|_| r#"{"error":"Failed to serialize error result"}"#.to_string());
        }
    };

    let encoding = wasm_req.encoding;
    let sql = wasm_req.inner.sql.clone();

    // Call handler
    let result = split_statements(&wasm_req.inner);

    // Serialize result
    let mut json_value = match serde_json::to_value(&result) {
        Ok(v) => v,
        Err(_) => {
            return serde_json::to_string(&StatementSplitResult::from_error("Failed to serialize result"))
                .unwrap_or_else(|_| r#"{"error":"Failed to serialize error result"}"#.to_string());
        }
    };

    // Convert spans if UTF-16 encoding requested
    if encoding == Encoding::Utf16 {
        convert_spans_to_utf16(&sql, &mut json_value);
    }

    serde_json::to_string(&json_value)
        .unwrap_or_else(|_| r#"{"error":"Failed to serialize result"}"#.to_string())
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
