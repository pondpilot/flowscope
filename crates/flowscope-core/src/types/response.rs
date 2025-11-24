//! Response types for the SQL lineage analysis API.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::common::{Issue, IssueCount, Span, Summary};

/// The result of analyzing SQL for data lineage.
///
/// Contains per-statement lineage graphs, a global lineage graph spanning all statements,
/// any issues encountered during analysis, and summary statistics.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct AnalyzeResult {
    /// Per-statement lineage analysis results
    pub statements: Vec<StatementLineage>,

    /// Global lineage graph spanning all statements
    pub global_lineage: GlobalLineage,

    /// All issues encountered during analysis
    pub issues: Vec<Issue>,

    /// Summary statistics
    pub summary: Summary,

    /// Effective schema used during analysis (imported + implied)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_schema: Option<ResolvedSchemaMetadata>,
}

impl AnalyzeResult {
    /// Create an error result with a single issue.
    /// Useful for returning errors from WASM boundary or other entry points.
    pub fn from_error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            statements: Vec::new(),
            global_lineage: GlobalLineage::default(),
            issues: vec![Issue::error(code, message)],
            summary: Summary {
                statement_count: 0,
                table_count: 0,
                column_count: 0,
                issue_count: IssueCount {
                    errors: 1,
                    warnings: 0,
                    infos: 0,
                },
                has_errors: true,
            },
            resolved_schema: None,
        }
    }
}

/// Lineage information for a single SQL statement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatementLineage {
    /// Zero-based index of the statement in the input SQL
    pub statement_index: usize,

    /// Type of SQL statement
    pub statement_type: String,

    /// Optional source name (file path or script identifier) for grouping
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,

    /// All nodes in the lineage graph for this statement
    pub nodes: Vec<Node>,

    /// All edges connecting nodes in the lineage graph
    pub edges: Vec<Edge>,

    /// Optional span of the entire statement in source SQL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// A node in the lineage graph (table, CTE, or column).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// Stable content-based hash ID
    pub id: String,

    /// Node type
    #[serde(rename = "type")]
    pub node_type: NodeType,

    /// Human-readable label (short name)
    pub label: String,

    /// Fully qualified name when available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,

    /// SQL expression text for computed columns
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,

    /// Source location in original SQL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,

    /// Extensible metadata for future use
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,

    /// How this table was resolved (imported, implied, or unknown)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_source: Option<ResolutionSource>,
}

/// The type of a node in the lineage graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    /// A database table
    Table,
    /// A Common Table Expression (WITH clause)
    Cte,
    /// A column
    Column,
}

/// An edge connecting two nodes in the lineage graph.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    /// Stable content-based hash ID
    pub id: String,

    /// Source node ID
    pub from: String,

    /// Target node ID
    pub to: String,

    /// Edge type
    #[serde(rename = "type")]
    pub edge_type: EdgeType,

    /// Optional: SQL expression if this edge represents a transformation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,

    /// Optional: operation label ('JOIN', 'UNION', 'AGGREGATE', etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,

    /// Extensible metadata for future use
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,

    /// True if this edge represents approximate/uncertain lineage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approximate: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    /// Table/CTE owns columns
    Ownership,
    /// Data flows from one column to another
    DataFlow,
    /// Output derived from inputs (with transformation)
    Derivation,
    /// Cross-statement dependency
    CrossStatement,
}

/// Global lineage graph spanning all statements in the analyzed SQL.
///
/// Provides a unified view of data flow across multiple statements.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct GlobalLineage {
    /// All unique nodes across all statements
    pub nodes: Vec<GlobalNode>,
    /// All edges representing cross-statement data flow
    pub edges: Vec<GlobalEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GlobalNode {
    /// Stable ID derived from canonical identifier
    pub id: String,

    /// Node type
    #[serde(rename = "type")]
    pub node_type: NodeType,

    /// Human-readable label
    pub label: String,

    /// Canonical name for cross-statement matching
    pub canonical_name: CanonicalName,

    /// References to statements that use this node
    pub statement_refs: Vec<StatementRef>,

    /// Extensible metadata
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,

    /// How this table was resolved (imported, implied, or unknown)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_source: Option<ResolutionSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalName {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
}

impl CanonicalName {
    pub fn table(catalog: Option<String>, schema: Option<String>, name: String) -> Self {
        Self {
            catalog,
            schema,
            name,
            column: None,
        }
    }

    pub fn to_qualified_string(&self) -> String {
        let mut parts = Vec::new();
        if let Some(ref cat) = self.catalog {
            parts.push(cat.as_str());
        }
        if let Some(ref sch) = self.schema {
            parts.push(sch.as_str());
        }
        parts.push(&self.name);
        if let Some(ref col) = self.column {
            parts.push(col.as_str());
        }
        parts.join(".")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatementRef {
    /// Statement index in the original request
    pub statement_index: usize,
    /// ID of the local node inside that statement graph (if available)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GlobalEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub edge_type: EdgeType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_statement: Option<StatementRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumer_statement: Option<StatementRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Resolved schema metadata showing the effective schema used during analysis.
///
/// Combines imported (user-provided) and implied (inferred from DDL) schema.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSchemaMetadata {
    /// All tables used during analysis (imported + implied)
    pub tables: Vec<ResolvedSchemaTable>,
}

/// A table in the resolved schema with origin metadata.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSchemaTable {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub name: String,
    pub columns: Vec<ResolvedColumnSchema>,

    /// Origin of this table's schema information
    pub origin: SchemaOrigin,

    /// For implied tables: which statement created it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_statement_index: Option<usize>,

    /// Timestamp when this entry was created/updated (ISO 8601)
    pub updated_at: String,

    /// True if this is a temporary table
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporary: Option<bool>,
}

/// A column in the resolved schema with origin tracking.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedColumnSchema {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,

    /// Column-level origin (can differ from table origin in future merging)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<SchemaOrigin>,
}

/// The origin of schema information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SchemaOrigin {
    /// User-provided schema
    Imported,
    /// Inferred from DDL in workload
    Implied,
}

/// How a table reference was resolved during analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ResolutionSource {
    /// Resolved from user-provided schema
    Imported,
    /// Resolved from inferred DDL schema
    Implied,
    /// Could not be resolved
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_result_serialization() {
        let result = AnalyzeResult {
            statements: vec![StatementLineage {
                statement_index: 0,
                statement_type: "SELECT".to_string(),
                source_name: None,
                nodes: vec![Node {
                    id: "tbl_123".to_string(),
                    node_type: NodeType::Table,
                    label: "users".to_string(),
                    qualified_name: Some("public.users".to_string()),
                    expression: None,
                    span: None,
                    metadata: None,
                    resolution_source: None,
                }],
                edges: vec![],
                span: None,
            }],
            global_lineage: GlobalLineage::default(),
            issues: vec![],
            summary: Summary::default(),
            resolved_schema: None,
        };

        let json = serde_json::to_string_pretty(&result).unwrap();
        assert!(json.contains("\"type\": \"table\"") || json.contains("\"type\":\"table\""));
        assert!(
            json.contains("\"statementType\": \"SELECT\"")
                || json.contains("\"statementType\":\"SELECT\"")
        );

        let deserialized: AnalyzeResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.statements.len(), 1);
        assert_eq!(
            deserialized.statements[0].nodes[0].node_type,
            NodeType::Table
        );
    }

    #[test]
    fn test_canonical_name() {
        let name = CanonicalName::table(
            Some("catalog".to_string()),
            Some("schema".to_string()),
            "table".to_string(),
        );
        assert_eq!(name.to_qualified_string(), "catalog.schema.table");

        let simple = CanonicalName::table(None, None, "users".to_string());
        assert_eq!(simple.to_qualified_string(), "users");
    }
}
