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

    /// Filter predicates (WHERE clause conditions) that affect this table's rows
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<FilterPredicate>,

    /// For table nodes that are JOINed: the type of join used to include this table.
    /// None for the main FROM table, Some(JoinType) for joined tables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_type: Option<JoinType>,

    /// For table nodes that are JOINed: the join condition (ON clause).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_condition: Option<String>,
}

impl Node {
    /// Create a new table node with required fields.
    pub fn table(id: String, label: String) -> Self {
        Self {
            id,
            node_type: NodeType::Table,
            label,
            qualified_name: None,
            expression: None,
            span: None,
            metadata: None,
            resolution_source: None,
            filters: Vec::new(),
            join_type: None,
            join_condition: None,
        }
    }

    /// Create a new CTE node with required fields.
    pub fn cte(id: String, label: String) -> Self {
        Self {
            id,
            node_type: NodeType::Cte,
            label,
            qualified_name: None,
            expression: None,
            span: None,
            metadata: None,
            resolution_source: None,
            filters: Vec::new(),
            join_type: None,
            join_condition: None,
        }
    }

    /// Create a new column node with required fields.
    pub fn column(id: String, label: String) -> Self {
        Self {
            id,
            node_type: NodeType::Column,
            label,
            qualified_name: None,
            expression: None,
            span: None,
            metadata: None,
            resolution_source: None,
            filters: Vec::new(),
            join_type: None,
            join_condition: None,
        }
    }

    /// Set the qualified name.
    pub fn with_qualified_name(mut self, name: impl Into<String>) -> Self {
        self.qualified_name = Some(name.into());
        self
    }

    /// Set the expression.
    pub fn with_expression(mut self, expr: impl Into<String>) -> Self {
        self.expression = Some(expr.into());
        self
    }

    /// Set the metadata.
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Set the resolution source.
    pub fn with_resolution_source(mut self, source: ResolutionSource) -> Self {
        self.resolution_source = Some(source);
        self
    }

    /// Set the join type.
    pub fn with_join_type(mut self, join_type: JoinType) -> Self {
        self.join_type = Some(join_type);
        self
    }

    /// Set the join condition.
    pub fn with_join_condition(mut self, condition: impl Into<String>) -> Self {
        self.join_condition = Some(condition.into());
        self
    }
}

impl Edge {
    /// Create a new edge with required fields.
    pub fn new(id: String, from: String, to: String, edge_type: EdgeType) -> Self {
        Self {
            id,
            from,
            to,
            edge_type,
            expression: None,
            operation: None,
            join_type: None,
            join_condition: None,
            metadata: None,
            approximate: None,
        }
    }

    /// Create a data flow edge.
    pub fn data_flow(id: String, from: String, to: String) -> Self {
        Self::new(id, from, to, EdgeType::DataFlow)
    }

    /// Create a derivation edge.
    pub fn derivation(id: String, from: String, to: String) -> Self {
        Self::new(id, from, to, EdgeType::Derivation)
    }

    /// Create an ownership edge.
    pub fn ownership(id: String, from: String, to: String) -> Self {
        Self::new(id, from, to, EdgeType::Ownership)
    }

    /// Set the expression.
    pub fn with_expression(mut self, expr: impl Into<String>) -> Self {
        self.expression = Some(expr.into());
        self
    }

    /// Set the operation.
    pub fn with_operation(mut self, op: impl Into<String>) -> Self {
        self.operation = Some(op.into());
        self
    }

    /// Set the join type.
    pub fn with_join_type(mut self, join_type: JoinType) -> Self {
        self.join_type = Some(join_type);
        self
    }

    /// Set the join condition.
    pub fn with_join_condition(mut self, condition: impl Into<String>) -> Self {
        self.join_condition = Some(condition.into());
        self
    }

    /// Mark as approximate lineage.
    pub fn approximate(mut self) -> Self {
        self.approximate = Some(true);
        self
    }
}

/// A filter predicate from a WHERE, HAVING, or JOIN ON clause.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FilterPredicate {
    /// The SQL expression text of the predicate
    pub expression: String,

    /// Where this filter appears in the query
    pub clause_type: FilterClauseType,
}

/// The type of SQL clause where a filter predicate appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FilterClauseType {
    /// FROM ... WHERE clause
    Where,
    /// HAVING clause (after GROUP BY)
    Having,
    /// JOIN ... ON clause
    JoinOn,
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

    /// Optional: specific join type for JOIN edges (INNER, LEFT, RIGHT, FULL, CROSS, etc.)
    /// Note: For table-level visualization, the frontend typically reads join info from the
    /// target Node's join_type/join_condition fields. These Edge fields are preserved for
    /// column-level lineage edges and future use cases where edge-level join context is needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_type: Option<JoinType>,

    /// Optional: join condition expression (ON clause)
    /// See join_type comment for usage notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_condition: Option<String>,

    /// Extensible metadata for future use
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,

    /// True if this edge represents approximate/uncertain lineage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approximate: Option<bool>,
}

/// The type of SQL JOIN operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JoinType {
    /// INNER JOIN - only matching rows from both tables
    Inner,
    /// LEFT OUTER JOIN - all rows from left table, matching from right
    Left,
    /// RIGHT OUTER JOIN - all rows from right table, matching from left
    Right,
    /// FULL OUTER JOIN - all rows from both tables
    Full,
    /// CROSS JOIN - cartesian product
    Cross,
    /// LEFT SEMI JOIN - rows from left that have match in right
    LeftSemi,
    /// RIGHT SEMI JOIN - rows from right that have match in left
    RightSemi,
    /// LEFT ANTI JOIN - rows from left that have no match in right
    LeftAnti,
    /// RIGHT ANTI JOIN - rows from right that have no match in left
    RightAnti,
    /// CROSS APPLY (SQL Server)
    CrossApply,
    /// OUTER APPLY (SQL Server)
    OuterApply,
    /// AS OF JOIN (time-series)
    AsOf,
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
                    filters: Vec::new(),
                    join_type: None,
                    join_condition: None,
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
