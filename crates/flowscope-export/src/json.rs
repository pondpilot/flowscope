use flowscope_core::AnalyzeResult;

use crate::ExportError;

pub fn export_json(result: &AnalyzeResult, compact: bool) -> Result<String, ExportError> {
    if compact {
        serde_json::to_string(result).map_err(|err| ExportError::Serialization(err.to_string()))
    } else {
        serde_json::to_string_pretty(result)
            .map_err(|err| ExportError::Serialization(err.to_string()))
    }
}
