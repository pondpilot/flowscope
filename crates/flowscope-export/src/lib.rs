//! Database export for FlowScope analysis results.
//!
//! Exports `AnalyzeResult` to queryable database formats (DuckDB, SQLite).
//!
//! Two export modes are available:
//! - **Binary export** (`export_duckdb`): Creates a DuckDB database file (native only)
//! - **SQL export** (`export_sql`): Generates DDL + INSERT statements (WASM-compatible)

mod error;
mod schema;
mod sql_backend;

#[cfg(feature = "duckdb")]
mod duckdb_backend;

pub use error::ExportError;

use flowscope_core::AnalyzeResult;

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// DuckDB database file (native only)
    DuckDB,
    /// SQL statements (DDL + INSERT) for duckdb-wasm
    Sql,
}

/// Export analysis result to a database file.
///
/// Returns raw bytes of the database file.
/// Only works with `Format::DuckDB` when the `duckdb` feature is enabled.
pub fn export(result: &AnalyzeResult, format: Format) -> Result<Vec<u8>, ExportError> {
    match format {
        #[cfg(feature = "duckdb")]
        Format::DuckDB => duckdb_backend::export(result),
        #[cfg(not(feature = "duckdb"))]
        Format::DuckDB => Err(ExportError::UnsupportedFormat("DuckDB feature not enabled")),
        Format::Sql => Ok(sql_backend::export_sql(result, None)?.into_bytes()),
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
