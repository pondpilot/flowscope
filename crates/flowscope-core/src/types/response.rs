//! Response types for the SQL lineage analysis API.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use super::common::{Issue, IssueCount, Span, Summary};
use super::request::ForeignKeyRef;

/// The result of analyzing SQL for data lineage.
///
/// Contains a single flat lineage graph spanning all statements, per-statement
/// metadata, any issues encountered during analysis, and summary statistics.
/// Each `Node` / `Edge` records the `statementIds` it participates in, so
/// consumers can filter down to a single statement or aggregate across all of
/// them without maintaining parallel collections.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeResult {
    /// Per-statement metadata (type, span, complexity, resolved SQL).
    /// The graph itself lives in the top-level `nodes` / `edges`.
    pub statements: Vec<StatementMeta>,

    /// All nodes in the lineage graph. Nodes shared across statements
    /// (for example, a table read by two queries) appear once with
    /// `statementIds` listing every statement they participate in.
    pub nodes: Vec<Node>,

    /// All edges in the lineage graph. Intra-statement edges carry a single
    /// entry in `statementIds`; `EdgeType::CrossStatement` edges connect nodes
    /// whose statement groups differ.
    pub edges: Vec<Edge>,

    /// All issues encountered during analysis
    pub issues: Vec<Issue>,

    /// Summary statistics
    pub summary: Summary,

    /// Effective schema used during analysis (imported + implied)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_schema: Option<ResolvedSchemaMetadata>,
}

/// The result of splitting SQL into statement spans.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatementSplitResult {
    /// Byte ranges for each statement in the input SQL.
    pub statements: Vec<Span>,
    /// Error message if the request could not be processed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl StatementSplitResult {
    pub fn from_error(message: impl Into<String>) -> Self {
        Self {
            statements: Vec::new(),
            error: Some(message.into()),
        }
    }
}

impl AnalyzeResult {
    /// Iterate over nodes that participate in the given statement index.
    pub fn nodes_in_statement(&self, statement_index: usize) -> impl Iterator<Item = &Node> {
        self.nodes
            .iter()
            .filter(move |n| n.statement_ids.contains(&statement_index))
    }

    /// Iterate over edges that participate in the given statement index.
    pub fn edges_in_statement(&self, statement_index: usize) -> impl Iterator<Item = &Edge> {
        self.edges
            .iter()
            .filter(move |e| e.statement_ids.contains(&statement_index))
    }

    /// Create an error result with a single issue.
    /// Useful for returning errors from WASM boundary or other entry points.
    pub fn from_error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            statements: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            issues: vec![Issue::error(code, message)],
            summary: Summary {
                statement_count: 0,
                table_count: 0,
                column_count: 0,
                join_count: 0,
                complexity_score: 1,
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

/// Per-statement metadata. The lineage graph itself is shared in
/// `AnalyzeResult.nodes` / `.edges`; this struct only carries facts about the
/// statement as a whole.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatementMeta {
    /// Zero-based index of the statement in the input SQL
    pub statement_index: usize,

    /// Type of SQL statement
    pub statement_type: String,

    /// Optional source name (file path or script identifier) for grouping
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,

    /// Optional span of the entire statement in source SQL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,

    /// Number of JOIN operations in this statement
    pub join_count: usize,

    /// Complexity score (1-100) based on query structure
    pub complexity_score: u8,

    /// Resolved/compiled SQL after template expansion (e.g., dbt Jinja rendering).
    /// Only present when templating was run in non-raw mode. May contain sensitive
    /// values from template variables (e.g., database credentials).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_sql: Option<String>,
}

/// Crate-private per-statement analysis intermediate.
///
/// Populated during analysis and merged into `AnalyzeResult.nodes` / `.edges`
/// by `Analyzer::build_result`. Not part of the public API.
#[derive(Debug, Clone)]
pub(crate) struct StatementLineage {
    pub statement_index: usize,
    pub statement_type: String,
    pub source_name: Option<String>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub span: Option<Span>,
    pub join_count: usize,
    pub complexity_score: u8,
    pub resolved_sql: Option<String>,
}

impl StatementLineage {
    /// Split this per-statement container into its public metadata and the
    /// graph fragment (nodes, edges) that will be merged at the top level.
    pub(crate) fn into_meta_and_graph(self) -> (StatementMeta, Vec<Node>, Vec<Edge>) {
        let Self {
            statement_index,
            statement_type,
            source_name,
            nodes,
            edges,
            span,
            join_count,
            complexity_score,
            resolved_sql,
        } = self;
        (
            StatementMeta {
                statement_index,
                statement_type,
                source_name,
                span,
                join_count,
                complexity_score,
                resolved_sql,
            },
            nodes,
            edges,
        )
    }
}

/// A node in the lineage graph (table, CTE, or column).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// Stable content-based hash ID
    #[serde(deserialize_with = "super::serde_utils::deserialize_arc_str")]
    pub id: Arc<str>,

    /// Node type
    #[serde(rename = "type")]
    pub node_type: NodeType,

    /// Human-readable label (short name)
    #[serde(deserialize_with = "super::serde_utils::deserialize_arc_str")]
    pub label: Arc<str>,

    /// Fully qualified display name when available.
    ///
    /// This is a cosmetic string intended for UI rendering. It is **not** a
    /// stable identity — prefer `canonical_name` for cross-statement matching,
    /// schema joins, or any equality comparison that must survive dialect
    /// quoting, casing, or alias differences.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::serde_utils::deserialize_option_arc_str"
    )]
    pub qualified_name: Option<Arc<str>>,

    /// Structured canonical identity (catalog.schema.name[.column]) used to
    /// match the same entity across statements. Only populated for nodes
    /// whose identity is globally meaningful — table-likes and columns owned
    /// by them. Statement-scoped nodes (CTEs, CTE columns, self-join instance
    /// columns) omit this.
    ///
    /// This is the authoritative identity for cross-statement matching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_name: Option<CanonicalName>,

    /// Zero-based indices of every statement this node participates in.
    ///
    /// Invariants:
    /// - Always has at least one entry.
    /// - Sorted ascending and deduplicated.
    /// - A node shared across statements (e.g. a table referenced by two
    ///   queries) lists every statement that references it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statement_ids: Vec<usize>,

    /// SQL expression text for computed columns
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::serde_utils::deserialize_option_arc_str"
    )]
    pub expression: Option<Arc<str>>,

    /// Source location in original SQL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,

    /// Source locations for this node's own relation-name occurrences.
    ///
    /// Ordered by lexical occurrence (left-to-right in the SQL text). Includes
    /// the declaration plus relation occurrences we can associate with the
    /// node (for example, a CTE name after `WITH` and each `FROM cte_name` /
    /// `JOIN cte_name` usage). Self-joins intentionally produce distinct node
    /// instances (one per lexical occurrence), each carrying its own
    /// single-entry `name_spans`, so repeated table names map to the correct
    /// node.
    ///
    /// Populated for table, view, and CTE nodes only. Column qualifier occurrences
    /// are not yet included, so column nodes omit this field and callers should
    /// fall back to `span` (use `Node::all_name_spans` for a unified view).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub name_spans: Vec<Span>,

    /// For CTE nodes: the source location of the CTE body (the parenthesized
    /// subquery after `AS`). Enables the UI to highlight the definition body
    /// separately from the CTE name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_span: Option<Span>,

    /// Extensible metadata for future use
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "super::serde_utils::serialize_option_json_map_sorted"
    )]
    pub metadata: Option<HashMap<String, serde_json::Value>>,

    /// How this table was resolved (imported, implied, or unknown)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_source: Option<ResolutionSource>,

    /// Filter predicates (WHERE clause conditions) that affect this table's rows
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<FilterPredicate>,

    /// For column nodes: aggregation information if this column is aggregated or a grouping key.
    /// Presence indicates the query uses GROUP BY; the fields indicate the column's role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregation: Option<AggregationInfo>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            id: Arc::from(""),
            node_type: NodeType::default(),
            label: Arc::from(""),
            qualified_name: None,
            canonical_name: None,
            statement_ids: Vec::new(),
            expression: None,
            span: None,
            name_spans: Vec::new(),
            body_span: None,
            metadata: None,
            resolution_source: None,
            filters: Vec::new(),
            aggregation: None,
        }
    }
}

impl Node {
    fn metadata_span_array(&self, key: &str) -> Vec<Span> {
        self.metadata
            .as_ref()
            .and_then(|metadata| metadata.get(key))
            .and_then(serde_json::Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| serde_json::from_value::<Span>(entry.clone()).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Create a new table node with required fields.
    pub fn table(id: impl Into<Arc<str>>, label: impl Into<Arc<str>>) -> Self {
        Self {
            id: id.into(),
            node_type: NodeType::Table,
            label: label.into(),
            ..Default::default()
        }
    }

    /// Create a new CTE node with required fields.
    pub fn cte(id: impl Into<Arc<str>>, label: impl Into<Arc<str>>) -> Self {
        Self {
            id: id.into(),
            node_type: NodeType::Cte,
            label: label.into(),
            ..Default::default()
        }
    }

    /// Create a new column node with required fields.
    pub fn column(id: impl Into<Arc<str>>, label: impl Into<Arc<str>>) -> Self {
        Self {
            id: id.into(),
            node_type: NodeType::Column,
            label: label.into(),
            ..Default::default()
        }
    }

    /// Returns all name occurrence spans, falling back to `span` for node
    /// types that don't populate `name_spans` (currently column nodes). This
    /// lets callers treat the two fields uniformly without branching on
    /// `node_type`.
    #[must_use]
    pub fn all_name_spans(&self) -> Vec<Span> {
        let occurrence_spans = self.metadata_span_array("occurrenceSpans");
        if !occurrence_spans.is_empty() {
            occurrence_spans
        } else if !self.name_spans.is_empty() {
            self.name_spans.clone()
        } else {
            self.span.into_iter().collect()
        }
    }

    /// Set the aggregation info.
    pub fn with_aggregation(mut self, aggregation: AggregationInfo) -> Self {
        self.aggregation = Some(aggregation);
        self
    }

    /// Set the qualified name.
    pub fn with_qualified_name(mut self, name: impl Into<Arc<str>>) -> Self {
        self.qualified_name = Some(name.into());
        self
    }

    /// Set the expression.
    pub fn with_expression(mut self, expr: impl Into<Arc<str>>) -> Self {
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

    /// Return the filter predicates recorded for `statement_index`.
    ///
    /// When the flattened graph merges a node across statements, per-statement
    /// `FilterPredicate`s are preserved in `metadata[STATEMENT_FILTERS_METADATA_KEY]`
    /// as an object keyed by statement index. This helper looks up that entry
    /// and falls back to the aggregated `filters` field when no per-statement
    /// data is recorded (e.g. nodes that only appear in one statement).
    #[must_use]
    pub fn filters_for_statement(&self, statement_index: usize) -> Vec<FilterPredicate> {
        if let Some(metadata) = self.metadata.as_ref() {
            if let Some(per_stmt) = metadata.get(STATEMENT_FILTERS_METADATA_KEY) {
                if let Some(entry) = per_stmt.get(statement_index.to_string()) {
                    if let Ok(filters) =
                        serde_json::from_value::<Vec<FilterPredicate>>(entry.clone())
                    {
                        return filters;
                    }
                }
            }
        }
        self.filters.clone()
    }

    /// Return the aggregation metadata recorded for `statement_index`.
    ///
    /// When the flattened graph merges a node across statements, per-statement
    /// `AggregationInfo` is preserved in
    /// `metadata[STATEMENT_AGGREGATIONS_METADATA_KEY]` as an object keyed by
    /// statement index. Entries may be explicit JSON `null` to indicate that a
    /// shared node is *not* aggregated in that statement. This helper respects
    /// that distinction and falls back to the plain `aggregation` field only
    /// when no per-statement data is recorded.
    #[must_use]
    pub fn aggregation_for_statement(&self, statement_index: usize) -> Option<AggregationInfo> {
        if let Some(metadata) = self.metadata.as_ref() {
            if let Some(per_stmt) = metadata.get(STATEMENT_AGGREGATIONS_METADATA_KEY) {
                if let Some(entry) = per_stmt.get(statement_index.to_string()) {
                    return if entry.is_null() {
                        None
                    } else {
                        serde_json::from_value::<AggregationInfo>(entry.clone()).ok()
                    };
                }
            }
        }
        self.aggregation.clone()
    }
}

/// Metadata key under `Node::metadata` that stores per-statement filter
/// predicates for nodes merged across statements. The value is a JSON object
/// keyed by statement index (as a string); each entry is a serialized
/// `Vec<FilterPredicate>`. Callers should treat this key as reserved and use
/// [`Node::filters_for_statement`] instead of reading it directly.
pub const STATEMENT_FILTERS_METADATA_KEY: &str = "statementFilters";

/// Metadata key under `Node::metadata` that stores per-statement aggregation
/// metadata for nodes merged across statements. The value is a JSON object
/// keyed by statement index (as a string); each entry is either a serialized
/// `AggregationInfo` or explicit `null` when that statement has no
/// aggregation. Callers should treat this key as reserved and use
/// [`Node::aggregation_for_statement`] instead of reading it directly.
pub const STATEMENT_AGGREGATIONS_METADATA_KEY: &str = "statementAggregations";

/// An edge connecting two nodes in the lineage graph.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    /// Stable content-based hash ID
    #[serde(deserialize_with = "super::serde_utils::deserialize_arc_str")]
    pub id: Arc<str>,

    /// Source node ID
    #[serde(deserialize_with = "super::serde_utils::deserialize_arc_str")]
    pub from: Arc<str>,

    /// Target node ID
    #[serde(deserialize_with = "super::serde_utils::deserialize_arc_str")]
    pub to: Arc<str>,

    /// Edge type
    #[serde(rename = "type")]
    pub edge_type: EdgeType,

    /// Optional: SQL expression if this edge represents a transformation
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::serde_utils::deserialize_option_arc_str"
    )]
    pub expression: Option<Arc<str>>,

    /// Optional: operation label ('JOIN', 'UNION', 'AGGREGATE', etc.)
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::serde_utils::deserialize_option_arc_str"
    )]
    pub operation: Option<Arc<str>>,

    /// Optional: specific join type for JOIN edges (INNER, LEFT, RIGHT, FULL, CROSS, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_type: Option<JoinType>,

    /// Optional: join condition expression (ON clause)
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::serde_utils::deserialize_option_arc_str"
    )]
    pub join_condition: Option<Arc<str>>,

    /// Extensible metadata for future use
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "super::serde_utils::serialize_option_json_map_sorted"
    )]
    pub metadata: Option<HashMap<String, serde_json::Value>>,

    /// True if this edge represents approximate/uncertain lineage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approximate: Option<bool>,

    /// Zero-based indices of the statement(s) this edge participates in.
    ///
    /// Invariants:
    /// - Intra-statement edges (Ownership, DataFlow, Derivation, JoinDependency)
    ///   list every statement in which the same structural `(from, to, kind)`
    ///   edge appears, sorted ascending and deduplicated.
    /// - `EdgeType::CrossStatement` edges are not merged across
    ///   producer/consumer pairs: each edge carries exactly
    ///   `[producer_index, consumer_index]` in that order, and the same
    ///   `(from, to)` self-loop may appear multiple times with different
    ///   pairs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statement_ids: Vec<usize>,
}

impl Edge {
    /// Create a new edge with required fields.
    pub fn new(
        id: impl Into<Arc<str>>,
        from: impl Into<Arc<str>>,
        to: impl Into<Arc<str>>,
        edge_type: EdgeType,
    ) -> Self {
        Self {
            id: id.into(),
            from: from.into(),
            to: to.into(),
            edge_type,
            expression: None,
            operation: None,
            join_type: None,
            join_condition: None,
            metadata: None,
            approximate: None,
            statement_ids: Vec::new(),
        }
    }

    /// Create a data flow edge.
    pub fn data_flow(
        id: impl Into<Arc<str>>,
        from: impl Into<Arc<str>>,
        to: impl Into<Arc<str>>,
    ) -> Self {
        Self::new(id, from, to, EdgeType::DataFlow)
    }

    /// Create a derivation edge.
    pub fn derivation(
        id: impl Into<Arc<str>>,
        from: impl Into<Arc<str>>,
        to: impl Into<Arc<str>>,
    ) -> Self {
        Self::new(id, from, to, EdgeType::Derivation)
    }

    /// Create an ownership edge.
    pub fn ownership(
        id: impl Into<Arc<str>>,
        from: impl Into<Arc<str>>,
        to: impl Into<Arc<str>>,
    ) -> Self {
        Self::new(id, from, to, EdgeType::Ownership)
    }

    /// Set the expression.
    pub fn with_expression(mut self, expr: impl Into<Arc<str>>) -> Self {
        self.expression = Some(expr.into());
        self
    }

    /// Set the operation.
    pub fn with_operation(mut self, op: impl Into<Arc<str>>) -> Self {
        self.operation = Some(op.into());
        self
    }

    /// Set the join type.
    pub fn with_join_type(mut self, join_type: JoinType) -> Self {
        self.join_type = Some(join_type);
        self
    }

    /// Set the join condition.
    pub fn with_join_condition(mut self, condition: impl Into<Arc<str>>) -> Self {
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

/// Information about aggregation applied to a column.
///
/// This tracks when a column is the result of an aggregation operation (like SUM, COUNT, AVG),
/// which indicates a cardinality reduction (1:many collapse) in the data flow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AggregationInfo {
    /// True if this column is a GROUP BY key (preserves row identity within groups)
    pub is_grouping_key: bool,

    /// The aggregation function used (e.g., "SUM", "COUNT", "AVG")
    /// None if this is a grouping key or non-aggregated column
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,

    /// True if this aggregation uses DISTINCT (e.g., COUNT(DISTINCT col))
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distinct: Option<bool>,
}

/// The type of a node in the lineage graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    /// A database table.
    ///
    /// This is also the `Default` variant used by `Node::default()`, so callers
    /// using `Node { node_type: ..., ..Default::default() }` must explicitly set
    /// `node_type` or they will silently get a table node.
    #[default]
    Table,
    /// A database view (CREATE VIEW)
    View,
    /// A Common Table Expression (WITH clause)
    Cte,
    /// A virtual output node for SELECT statements
    Output,
    /// A column
    Column,
}

impl NodeType {
    /// Returns true if this is a table-like node (table, view, or CTE).
    /// These nodes can contain columns and appear in FROM clauses.
    pub fn is_table_like(self) -> bool {
        matches!(self, NodeType::Table | NodeType::View | NodeType::Cte)
    }

    /// Returns true if this is a relation-like node that can be a source or sink in lineage.
    ///
    /// Includes table-like nodes plus Output nodes (virtual sinks for SELECT statements).
    /// Use this when building lineage graphs where Output nodes participate as targets.
    pub fn is_relation(self) -> bool {
        matches!(
            self,
            NodeType::Table | NodeType::View | NodeType::Cte | NodeType::Output
        )
    }

    /// Returns true if this is a table or view (excludes CTEs).
    /// Use this when you need to distinguish persistent relations from CTEs.
    pub fn is_table_or_view(self) -> bool {
        matches!(self, NodeType::Table | NodeType::View)
    }
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
    /// Join-only dependency from a source to output
    JoinDependency,
    /// Cross-statement dependency
    CrossStatement,
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

    /// Table-level constraints (composite PKs, FKs, etc.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<TableConstraintInfo>,
}

/// Information about a table-level constraint (composite PK, FK, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TableConstraintInfo {
    /// Type of constraint
    pub constraint_type: ConstraintType,
    /// Columns involved in this constraint
    pub columns: Vec<String>,
    /// For FK: the referenced table
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referenced_table: Option<String>,
    /// For FK: the referenced columns
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referenced_columns: Option<Vec<String>>,
}

/// Type of table constraint.
///
/// This enum is marked `#[non_exhaustive]` to allow adding constraint types
/// (e.g., CHECK, EXCLUDE) in the future without breaking API compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConstraintType {
    PrimaryKey,
    ForeignKey,
    Unique,
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

    /// True if this column is a primary key (or part of composite PK)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_primary_key: Option<bool>,

    /// Foreign key reference if this column references another table
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreign_key: Option<ForeignKeyRef>,
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
            statements: vec![StatementMeta {
                statement_index: 0,
                statement_type: "SELECT".to_string(),
                source_name: None,
                span: None,
                join_count: 0,
                complexity_score: 5,
                resolved_sql: None,
            }],
            nodes: vec![Node {
                id: "tbl_123".to_string().into(),
                node_type: NodeType::Table,
                label: "users".to_string().into(),
                qualified_name: Some("public.users".to_string().into()),
                canonical_name: Some(CanonicalName::table(
                    None,
                    Some("public".to_string()),
                    "users".to_string(),
                )),
                statement_ids: vec![0],
                ..Default::default()
            }],
            edges: vec![],
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
        assert_eq!(deserialized.nodes.len(), 1);
        assert_eq!(deserialized.nodes[0].node_type, NodeType::Table);
        assert_eq!(deserialized.nodes[0].statement_ids, vec![0]);
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
