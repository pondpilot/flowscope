//! Data writing utilities.

use flowscope_core::AnalyzeResult;

/// Write analysis result data to database.
pub fn write_data<W: DatabaseWriter>(
    _writer: &mut W,
    _result: &AnalyzeResult,
) -> Result<(), crate::ExportError> {
    Ok(())
}

/// Trait for database backends.
pub trait DatabaseWriter {
    fn execute(&mut self, sql: &str) -> Result<(), crate::ExportError>;
}
