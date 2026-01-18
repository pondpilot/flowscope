//! Error types for the export crate.

use thiserror::Error;

/// Errors that can occur during database export.
#[derive(Debug, Error)]
pub enum ExportError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(&'static str),

    #[error("Invalid schema name: {0}")]
    InvalidSchema(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("CSV export error: {0}")]
    Csv(String),

    #[error("Archive export error: {0}")]
    Archive(String),

    #[error("XLSX export error: {0}")]
    Xlsx(String),

    #[error("HTML export error: {0}")]
    Html(String),
}

#[cfg(feature = "duckdb")]
impl From<duckdb::Error> for ExportError {
    fn from(e: duckdb::Error) -> Self {
        ExportError::Database(e.to_string())
    }
}
