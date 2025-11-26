//! Input collection and validation for SQL analysis requests.
//!
//! This module handles the parsing and collection of SQL statements from analysis requests,
//! supporting both file-based and inline SQL inputs.

use crate::parser::parse_sql_with_dialect;
use crate::types::{issue_codes, AnalyzeRequest, Issue};
use sqlparser::ast::Statement;

/// A parsed statement alongside optional source metadata.
pub(crate) struct StatementInput {
    /// The parsed SQL statement.
    pub(crate) statement: Statement,
    /// Optional source file name for error reporting and tracing.
    pub(crate) source_name: Option<String>,
}

/// Collects and parses SQL statements from the analysis request.
///
/// This function handles both file-based and inline SQL inputs, combining them into
/// a single ordered list of statements for analysis.
///
/// # Input Sources
///
/// The request can provide SQL through two mechanisms:
///
/// 1. **File sources** (`request.files`): A list of named SQL files with content
/// 2. **Inline SQL** (`request.sql`): Direct SQL text in the request body
///
/// Both sources are processed and combined. At least one must contain valid SQL.
///
/// # Processing Order
///
/// When both sources are present, statements are collected in this order:
///
/// 1. File statements (in the order files appear in the array)
/// 2. Inline SQL statements
///
/// This ordering ensures predictable cross-statement dependency detection,
/// where earlier statements can be referenced by later ones.
///
/// # Error Handling
///
/// Parse errors from individual files or inline SQL are collected as issues
/// rather than failing immediately. This allows partial analysis when some
/// inputs are valid.
///
/// # Returns
///
/// A tuple of `(statements, issues)` where:
/// - `statements`: Successfully parsed statements with source attribution
/// - `issues`: Any validation errors or parse failures encountered
pub(crate) fn collect_statements(request: &AnalyzeRequest) -> (Vec<StatementInput>, Vec<Issue>) {
    let mut issues = Vec::new();
    let mut statements = Vec::new();

    let has_sql = !request.sql.trim().is_empty();
    let has_files = request
        .files
        .as_ref()
        .map(|files| !files.is_empty())
        .unwrap_or(false);

    if !has_sql && !has_files {
        issues.push(Issue::error(
            issue_codes::INVALID_REQUEST,
            "Provide inline SQL or at least one file to analyze",
        ));
        return (Vec::new(), issues);
    }

    // Parse files first (if present)
    if let Some(files) = &request.files {
        for file in files {
            match parse_sql_with_dialect(&file.content, request.dialect) {
                Ok(stmts) => {
                    for stmt in stmts {
                        statements.push(StatementInput {
                            statement: stmt,
                            source_name: Some(file.name.clone()),
                        });
                    }
                }
                Err(e) => issues.push(Issue::error(
                    issue_codes::PARSE_ERROR,
                    format!("Error parsing {}: {}", file.name, e),
                )),
            }
        }
    }

    // Parse inline SQL if present (appended after file statements)
    if has_sql {
        match parse_sql_with_dialect(&request.sql, request.dialect) {
            Ok(stmts) => {
                statements.extend(stmts.into_iter().map(|stmt| StatementInput {
                    statement: stmt,
                    source_name: request.source_name.clone(),
                }));
            }
            Err(e) => {
                issues.push(Issue::error(issue_codes::PARSE_ERROR, e.to_string()));
            }
        }
    }

    (statements, issues)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Dialect, FileSource};

    fn base_request() -> AnalyzeRequest {
        AnalyzeRequest {
            sql: String::new(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        }
    }

    #[test]
    fn collects_file_and_inline_statements() {
        let mut request = base_request();
        request.sql = "SELECT 2".to_string();
        request.source_name = Some("inline.sql".to_string());
        request.files = Some(vec![FileSource {
            name: "file.sql".to_string(),
            content: "SELECT 1".to_string(),
        }]);

        let (statements, issues) = collect_statements(&request);
        assert!(issues.is_empty());
        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].source_name.as_deref(), Some("file.sql"));
        assert_eq!(statements[1].source_name.as_deref(), Some("inline.sql"));
    }

    #[test]
    fn reports_invalid_request_without_inputs() {
        let request = base_request();
        let (_statements, issues) = collect_statements(&request);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::INVALID_REQUEST);
    }
}
