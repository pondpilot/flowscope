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
}

#[cfg(feature = "duckdb")]
impl From<duckdb::Error> for ExportError {
    fn from(e: duckdb::Error) -> Self {
        ExportError::Database(e.to_string())
    }
}
