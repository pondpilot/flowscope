//! Database export for FlowScope analysis results.
//!
//! Exports `AnalyzeResult` to queryable database formats (DuckDB, SQLite).

mod error;
mod schema;
mod writer;

#[cfg(feature = "duckdb")]
mod duckdb_backend;

pub use error::ExportError;

use flowscope_core::AnalyzeResult;

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// DuckDB database file
    DuckDB,
}

/// Export analysis result to a database file.
///
/// Returns raw bytes of the database file.
pub fn export(result: &AnalyzeResult, format: Format) -> Result<Vec<u8>, ExportError> {
    match format {
        #[cfg(feature = "duckdb")]
        Format::DuckDB => duckdb_backend::export(result),
        #[cfg(not(feature = "duckdb"))]
        Format::DuckDB => Err(ExportError::UnsupportedFormat("DuckDB feature not enabled")),
    }
}

/// Export analysis result to DuckDB format.
#[cfg(feature = "duckdb")]
pub fn export_duckdb(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    duckdb_backend::export(result)
}
