//! JSON output formatting.

use flowscope_core::AnalyzeResult;

/// Format the analysis result as JSON.
///
/// If `compact` is true, outputs minified JSON without whitespace.
pub fn format_json(result: &AnalyzeResult, compact: bool) -> String {
    if compact {
        serde_json::to_string(result).expect("serialization cannot fail")
    } else {
        serde_json::to_string_pretty(result).expect("serialization cannot fail")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flowscope_core::{analyze, AnalyzeRequest, Dialect};

    #[test]
    fn test_json_pretty() {
        let result = analyze(&AnalyzeRequest {
            sql: "SELECT * FROM users".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        });

        let json = format_json(&result, false);
        assert!(json.contains('\n'));
        assert!(json.contains("summary"));
    }

    #[test]
    fn test_json_compact() {
        let result = analyze(&AnalyzeRequest {
            sql: "SELECT * FROM users".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        });

        let json = format_json(&result, true);
        assert!(!json.starts_with("{\n"));
    }
}
