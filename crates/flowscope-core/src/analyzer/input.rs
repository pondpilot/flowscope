use crate::parser::parse_sql_with_dialect;
use crate::types::{issue_codes, AnalyzeRequest, Issue};
use sqlparser::ast::Statement;

/// A parsed statement alongside optional source metadata.
pub(crate) struct StatementInput {
    pub(crate) statement: Statement,
    pub(crate) source_name: Option<String>,
}

/// Collect statements from the request, returning any issues encountered along the way.
/// Request validation happens here so downstream analysis can assume a well-formed workload.
pub(crate) fn collect_statements(request: &AnalyzeRequest) -> (Vec<StatementInput>, Vec<Issue>) {
    let mut issues = Vec::new();

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
        let mut statements = Vec::new();

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

        if !statements.is_empty() {
            return (statements, issues);
        }
    }

    // Fallback to inline SQL
    if has_sql {
        match parse_sql_with_dialect(&request.sql, request.dialect) {
            Ok(stmts) => {
                let statements = stmts
                    .into_iter()
                    .map(|stmt| StatementInput {
                        statement: stmt,
                        source_name: request.source_name.clone(),
                    })
                    .collect();
                (statements, issues)
            }
            Err(e) => {
                issues.push(Issue::error(issue_codes::PARSE_ERROR, e.to_string()));
                (Vec::new(), issues)
            }
        }
    } else {
        (Vec::new(), issues)
    }
}
