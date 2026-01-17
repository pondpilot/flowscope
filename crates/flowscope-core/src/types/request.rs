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

/// A request to compute completion context at a cursor position.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompletionRequest {
    /// The SQL code to analyze (UTF-8 string, multi-statement supported)
    pub sql: String,

    /// SQL dialect
    pub dialect: Dialect,

    /// Byte offset of the cursor in the SQL string
    pub cursor_offset: usize,

    /// Optional schema metadata for accurate column resolution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaMetadata>,
}

/// A request to split SQL into statement spans.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatementSplitRequest {
    /// The SQL code to split (UTF-8 string, multi-statement supported)
    pub sql: String,

    /// SQL dialect (currently unused; reserved for future dialect-specific splitting).
    ///
    /// The current implementation uses a universal tokenizer that handles common SQL
    /// constructs (strings, comments, dollar-quoting) across all dialects.
    #[serde(default)]
    pub dialect: Dialect,
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
    Ansi,
    Bigquery,
    Clickhouse,
    Databricks,
    Duckdb,
    Hive,
    Mssql,
    Mysql,
    Postgres,
    Redshift,
    Snowflake,
    Sqlite,
}

impl Dialect {
    pub fn to_sqlparser_dialect(&self) -> Box<dyn sqlparser::dialect::Dialect> {
        use sqlparser::dialect::{
            AnsiDialect, BigQueryDialect, ClickHouseDialect, DatabricksDialect, DuckDbDialect,
            GenericDialect, HiveDialect, MsSqlDialect, MySqlDialect, PostgreSqlDialect,
            RedshiftSqlDialect, SQLiteDialect, SnowflakeDialect,
        };
        match self {
            Self::Generic => Box::new(GenericDialect {}),
            Self::Ansi => Box::new(AnsiDialect {}),
            Self::Bigquery => Box::new(BigQueryDialect {}),
            Self::Clickhouse => Box::new(ClickHouseDialect {}),
            Self::Databricks => Box::new(DatabricksDialect {}),
            Self::Duckdb => Box::new(DuckDbDialect {}),
            Self::Hive => Box::new(HiveDialect {}),
            Self::Mssql => Box::new(MsSqlDialect {}),
            Self::Mysql => Box::new(MySqlDialect {}),
            Self::Postgres => Box::new(PostgreSqlDialect {}),
            Self::Redshift => Box::new(RedshiftSqlDialect {}),
            Self::Snowflake => Box::new(SnowflakeDialect {}),
            Self::Sqlite => Box::new(SQLiteDialect {}),
        }
    }

    /// Get the case sensitivity behavior for this dialect.
    ///
    /// Uses generated rules from `specs/dialect-semantics/dialects.json`.
    pub fn default_case_sensitivity(&self) -> CaseSensitivity {
        use crate::generated::NormalizationStrategy;
        match self.normalization_strategy() {
            NormalizationStrategy::Lowercase => CaseSensitivity::Lower,
            NormalizationStrategy::Uppercase => CaseSensitivity::Upper,
            NormalizationStrategy::CaseSensitive => CaseSensitivity::Exact,
            // CaseInsensitive dialects use lowercase folding for comparison
            NormalizationStrategy::CaseInsensitive => CaseSensitivity::Lower,
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

    /// Hide CTEs from output, creating bypass edges (A→CTE→B becomes A→B)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hide_ctes: Option<bool>,
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
    /// True if this column is a primary key (or part of composite PK)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_primary_key: Option<bool>,
    /// Foreign key reference if this column references another table
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreign_key: Option<ForeignKeyRef>,
}

/// A foreign key reference to another table's column.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyRef {
    /// The referenced table name (may be qualified)
    pub table: String,
    /// The referenced column name
    pub column: String,
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
        // Postgres folds to lowercase
        assert_eq!(
            Dialect::Postgres.default_case_sensitivity(),
            CaseSensitivity::Lower
        );
        // Snowflake folds to uppercase
        assert_eq!(
            Dialect::Snowflake.default_case_sensitivity(),
            CaseSensitivity::Upper
        );
        // BigQuery is case-insensitive (uses lowercase for comparison)
        // Note: BigQuery has custom normalization for tables vs columns
        assert_eq!(
            Dialect::Bigquery.default_case_sensitivity(),
            CaseSensitivity::Lower
        );
        // MySQL is case-sensitive
        assert_eq!(
            Dialect::Mysql.default_case_sensitivity(),
            CaseSensitivity::Exact
        );
        // SQLite is case-insensitive (corrected from old Exact)
        assert_eq!(
            Dialect::Sqlite.default_case_sensitivity(),
            CaseSensitivity::Lower
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
