//! Schema loading from DDL files.

use anyhow::{bail, Context, Result};
use flowscope_core::{
    analyze, AnalyzeRequest, ColumnSchema, Dialect, FileSource, SchemaMetadata, SchemaTable,
    Severity,
};
use std::path::Path;

/// Load schema from a DDL file containing CREATE TABLE statements.
///
/// Parses the DDL using flowscope-core's analyzer to extract table/column definitions.
pub fn load_schema_from_ddl(path: &Path, dialect: Dialect) -> Result<SchemaMetadata> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read schema file: {}", path.display()))?;

    parse_schema_ddl(&content, dialect)
}

/// Parse DDL content to extract schema metadata.
fn parse_schema_ddl(content: &str, dialect: Dialect) -> Result<SchemaMetadata> {
    let request = AnalyzeRequest {
        sql: String::new(),
        files: Some(vec![FileSource {
            name: "schema.sql".to_string(),
            content: content.to_string(),
        }]),
        dialect,
        source_name: None,
        options: None,
        schema: Some(SchemaMetadata {
            allow_implied: true,
            ..Default::default()
        }),
        #[cfg(feature = "templating")]
        template_config: None,
    };

    let result = analyze(&request);

    // Check for parsing errors
    if result.summary.has_errors {
        let error_messages: Vec<String> = result
            .issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
            .map(|issue| format!("[{}] {}", issue.code, issue.message))
            .collect();

        bail!("Failed to parse schema DDL:\n{}", error_messages.join("\n"));
    }

    // Extract schema from resolved_schema
    let resolved = result
        .resolved_schema
        .context("Schema DDL produced no table definitions")?;

    let schema = SchemaMetadata {
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: resolved
            .tables
            .into_iter()
            .map(|t| SchemaTable {
                catalog: t.catalog,
                schema: t.schema,
                name: t.name,
                columns: t
                    .columns
                    .into_iter()
                    .map(|c| ColumnSchema {
                        name: c.name,
                        data_type: c.data_type,
                        is_primary_key: c.is_primary_key,
                        foreign_key: c.foreign_key,
                    })
                    .collect(),
            })
            .collect(),
        allow_implied: false, // Don't allow further implied tables in main analysis
    };

    Ok(schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_ddl() {
        let ddl = r#"
            CREATE TABLE users (
                id INT,
                name VARCHAR(100),
                email VARCHAR(255)
            );
        "#;

        let schema = parse_schema_ddl(ddl, Dialect::Generic).unwrap();
        assert!(!schema.tables.is_empty());

        let users_table = schema.tables.iter().find(|t| t.name == "users");
        assert!(users_table.is_some());

        let table = users_table.unwrap();
        assert_eq!(table.columns.len(), 3);
    }

    #[test]
    fn test_parse_multiple_tables() {
        let ddl = r#"
            CREATE TABLE users (id INT, name VARCHAR);
            CREATE TABLE orders (id INT, user_id INT, total DECIMAL);
        "#;

        let schema = parse_schema_ddl(ddl, Dialect::Generic).unwrap();
        assert!(schema.tables.len() >= 2);
    }

    #[test]
    fn test_parse_invalid_ddl_returns_error() {
        let ddl = "THIS IS NOT VALID SQL AT ALL ;;;";

        let result = parse_schema_ddl(ddl, Dialect::Generic);
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Failed to parse schema DDL"),
            "Expected error message to mention parsing failure, got: {}",
            err_msg
        );
    }
}
