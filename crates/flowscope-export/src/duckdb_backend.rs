//! DuckDB backend implementation.

use crate::ExportError;
use flowscope_core::AnalyzeResult;

pub fn export(_result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    Err(ExportError::UnsupportedFormat("Not yet implemented"))
}
