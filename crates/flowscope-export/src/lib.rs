//! Database export for FlowScope analysis results.
//!
//! Exports `AnalyzeResult` to queryable database formats (DuckDB, SQLite).
//!
//! Two export modes are available:
//! - **Binary export** (`export_duckdb`): Creates a DuckDB database file (native only)
//! - **SQL export** (`export_sql`): Generates DDL + INSERT statements (WASM-compatible)

mod csv;
mod error;
mod extract;
mod html;
mod json;
mod mermaid;
mod naming;
mod schema;
mod sql_backend;
mod xlsx;

#[cfg(feature = "duckdb")]
mod duckdb_backend;

pub use error::ExportError;
pub use extract::{ColumnMapping, ScriptInfo, TableDependency, TableInfo, TableType};
pub use mermaid::MermaidView;
pub use naming::ExportNaming;

use flowscope_core::AnalyzeResult;

/// Supported export formats for filenames and UI integrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    DuckDb,
    Sql { schema: bool },
    Json { compact: bool },
    Mermaid { view: MermaidView },
    Html,
    CsvBundle,
    Xlsx,
    Png,
}

pub type Format = ExportFormat;

/// Export analysis result to a database file.
///
/// Returns raw bytes of the database file.
/// Only works with `ExportFormat::DuckDb` when the `duckdb` feature is enabled.
pub fn export(result: &AnalyzeResult, format: ExportFormat) -> Result<Vec<u8>, ExportError> {
    match format {
        #[cfg(feature = "duckdb")]
        ExportFormat::DuckDb => duckdb_backend::export(result),
        #[cfg(not(feature = "duckdb"))]
        ExportFormat::DuckDb => Err(ExportError::UnsupportedFormat("DuckDB feature not enabled")),
        ExportFormat::Sql { .. } => Ok(sql_backend::export_sql(result, None)?.into_bytes()),
        ExportFormat::Json { compact } => Ok(json::export_json(result, compact)?.into_bytes()),
        ExportFormat::Mermaid { view } => Ok(mermaid::export_mermaid(result, view).into_bytes()),
        ExportFormat::Html => {
            Ok(html::export_html(result, "FlowScope", chrono::Utc::now()).into_bytes())
        }
        ExportFormat::CsvBundle => csv::export_csv_bundle(result),
        ExportFormat::Xlsx => xlsx::export_xlsx(result),
        ExportFormat::Png => Err(ExportError::UnsupportedFormat("PNG export is UI-only")),
    }
}

/// Export analysis result to DuckDB format.
///
/// Requires the `duckdb` feature (native only, not WASM-compatible).
#[cfg(feature = "duckdb")]
pub fn export_duckdb(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    duckdb_backend::export(result)
}

/// Export analysis result as SQL statements.
///
/// Returns DDL (CREATE TABLE/VIEW) + INSERT statements that can be
/// executed by duckdb-wasm in the browser.
///
/// This is the WASM-compatible export path.
///
/// If `schema` is provided, all tables and views will be prefixed with that schema
/// (e.g., "myschema.tablename") and a `CREATE SCHEMA IF NOT EXISTS` statement will be added.
pub fn export_sql(result: &AnalyzeResult, schema: Option<&str>) -> Result<String, ExportError> {
    sql_backend::export_sql(result, schema)
}

pub fn export_json(result: &AnalyzeResult, compact: bool) -> Result<String, ExportError> {
    json::export_json(result, compact)
}

pub fn export_mermaid(result: &AnalyzeResult, view: MermaidView) -> Result<String, ExportError> {
    Ok(mermaid::export_mermaid(result, view))
}

pub fn export_csv_bundle(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    csv::export_csv_bundle(result)
}

pub fn export_xlsx(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    xlsx::export_xlsx(result)
}

pub fn export_html(
    result: &AnalyzeResult,
    project_name: &str,
    exported_at: chrono::DateTime<chrono::Utc>,
) -> Result<String, ExportError> {
    Ok(html::export_html(result, project_name, exported_at))
}
