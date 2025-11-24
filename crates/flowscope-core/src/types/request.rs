//! Request types for the SQL lineage analysis API.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::CaseSensitivity;

/// A request to analyze SQL for data lineage.
///
/// This is the main entry point for the analysis API. It accepts SQL code along with
/// optional dialect and schema information to produce accurate lineage graphs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeRequest {
    /// The SQL code to analyze (UTF-8 string, multi-statement supported)
    pub sql: String,

    /// Optional list of source files to analyze (alternative to single `sql` field)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileSource>>,

    /// SQL dialect
    pub dialect: Dialect,

    /// Optional source name (file path or script identifier) for grouping
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,

    /// Optional analysis options
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<AnalysisOptions>,

    /// Optional schema metadata for accurate column resolution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileSource {
    pub name: String,
    pub content: String,
}

/// SQL dialect for parsing and analysis.
///
/// Different dialects have different syntax rules and identifier normalization behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    #[default]
    Generic,
    Postgres,
    Snowflake,
    Bigquery,
}

impl Dialect {
    pub fn to_sqlparser_dialect(&self) -> Box<dyn sqlparser::dialect::Dialect> {
        use sqlparser::dialect::{
            BigQueryDialect, GenericDialect, PostgreSqlDialect, SnowflakeDialect,
        };
        match self {
            Self::Generic => Box::new(GenericDialect {}),
            Self::Postgres => Box::new(PostgreSqlDialect {}),
            Self::Snowflake => Box::new(SnowflakeDialect {}),
            Self::Bigquery => Box::new(BigQueryDialect {}),
        }
    }

    /// Get the case sensitivity behavior for this dialect
    pub fn default_case_sensitivity(&self) -> CaseSensitivity {
        match self {
            Dialect::Postgres => CaseSensitivity::Lower,
            Dialect::Snowflake => CaseSensitivity::Upper,
            Dialect::Bigquery => CaseSensitivity::Exact,
            Dialect::Generic => CaseSensitivity::Lower,
        }
    }
}

/// Graph detail level for visualization.
///
/// Controls the granularity of the lineage graph returned by the analyzer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum GraphDetailLevel {
    /// Script/file level: show relationships between scripts through shared tables
    Script,
    /// Table level: show tables and their relationships (default)
    #[default]
    Table,
    /// Column level: show individual columns as separate graph nodes
    Column,
}

/// Options controlling the analysis behavior.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisOptions {
    /// Enable column-level lineage (Phase 2+, default true when implemented)
    #[serde(default)]
    pub enable_column_lineage: Option<bool>,

    /// Preferred graph detail level for visualization (does not affect analysis)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_detail_level: Option<GraphDetailLevel>,
}

/// Schema metadata for accurate column and table resolution.
///
/// When provided, allows the analyzer to resolve ambiguous references and
/// produce more accurate lineage information.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct SchemaMetadata {
    /// Default catalog applied to unqualified identifiers
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_catalog: Option<String>,

    /// Default schema applied to unqualified identifiers
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_schema: Option<String>,

    /// Ordered list mirroring database search_path behavior
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_path: Option<Vec<SchemaNamespaceHint>>,

    /// Override for identifier normalization (default 'dialect')
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_sensitivity: Option<CaseSensitivity>,

    /// Canonical table representations
    #[serde(default)]
    pub tables: Vec<SchemaTable>,

    /// Global toggle for implied schema capture (default: true)
    /// When false, only imported schema is used; workload DDL is ignored
    #[serde(default = "default_allow_implied", skip_serializing_if = "is_true")]
    pub allow_implied: bool,
}

fn default_allow_implied() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SchemaNamespaceHint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    pub schema: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SchemaTable {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub name: String,
    #[serde(default)]
    pub columns: Vec<ColumnSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSchema {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_request_serialization() {
        let request = AnalyzeRequest {
            sql: "SELECT * FROM users".to_string(),
            files: None,
            dialect: Dialect::Postgres,
            source_name: None,
            options: None,
            schema: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"dialect\":\"postgres\""));

        let deserialized: AnalyzeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.dialect, Dialect::Postgres);
    }

    #[test]
    fn test_dialect_case_sensitivity() {
        assert_eq!(
            Dialect::Postgres.default_case_sensitivity(),
            CaseSensitivity::Lower
        );
        assert_eq!(
            Dialect::Snowflake.default_case_sensitivity(),
            CaseSensitivity::Upper
        );
        assert_eq!(
            Dialect::Bigquery.default_case_sensitivity(),
            CaseSensitivity::Exact
        );
    }

    #[test]
    fn test_schema_metadata_deserialization() {
        let json = r#"{
            "defaultSchema": "public",
            "tables": [
                {
                    "name": "users",
                    "columns": [
                        { "name": "id" },
                        { "name": "email", "dataType": "varchar" }
                    ]
                }
            ]
        }"#;

        let schema: SchemaMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(schema.default_schema, Some("public".to_string()));
        assert_eq!(schema.tables.len(), 1);
        assert_eq!(schema.tables[0].columns.len(), 2);
    }
}
