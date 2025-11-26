//! Cross-statement lineage tracking.
//!
//! This module provides [`CrossStatementTracker`], which manages the relationships
//! between statements in a multi-statement workload. It tracks which statements
//! produce tables (via CREATE/INSERT) and which consume them (via SELECT/JOIN).
//!
//! # Architecture
//!
//! The tracker maintains producer-consumer relationships between statements:
//!
//! - **Producers**: Statements that create or modify tables (CREATE TABLE, INSERT INTO, etc.)
//! - **Consumers**: Statements that read from tables (SELECT, JOIN, etc.)
//!
//! When a table is produced by statement N and consumed by statement M (where M > N),
//! a cross-statement edge is created to represent the data flow dependency.
//!
//! # View vs Table Distinction
//!
//! The tracker distinguishes between views and tables because they have different
//! semantics in lineage graphs:
//!
//! - Tables represent physical data storage
//! - Views represent logical transformations that are expanded at query time
//!
//! This distinction affects node ID generation and the type of lineage edges created.
//!
//! # Thread Safety
//!
//! `CrossStatementTracker` is designed for single-threaded use within an analysis pass.
//! Each analysis pass should create a fresh tracker instance.

use crate::types::{EdgeType, GlobalEdge, NodeType, StatementRef};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::helpers::generate_node_id;

/// Tracks cross-statement dependencies for multi-statement lineage.
///
/// `CrossStatementTracker` is responsible for building the dependency graph between
/// SQL statements in a multi-statement workload. It enables detection of data flow
/// patterns like ETL pipelines where one statement's output becomes another's input.
///
/// # Responsibilities
///
/// - **Producer tracking**: Records which statements create/modify tables
/// - **Consumer tracking**: Records which statements read from tables
/// - **Edge generation**: Creates cross-statement edges for the global lineage graph
/// - **Type distinction**: Maintains separate tracking for views vs tables
///
/// # Invariants
///
/// - `produced_views` is always a subset of `produced_tables` (views are tables)
/// - Cross-statement edges are only created when consumer index > producer index
/// - `all_relations` contains the union of all produced and consumed tables
///
/// # Example
///
/// ```ignore
/// let mut tracker = CrossStatementTracker::new();
///
/// // Statement 0 creates a staging table
/// tracker.record_produced("staging.raw_data", 0);
///
/// // Statement 1 reads from the staging table
/// tracker.record_consumed("staging.raw_data", 1);
///
/// // Generate cross-statement edges
/// let edges = tracker.build_cross_statement_edges();
/// assert_eq!(edges.len(), 1);
/// ```
pub(crate) struct CrossStatementTracker {
    /// Maps table canonical name -> statement index that produced it.
    ///
    /// Only tracks the most recent producer (later statements overwrite earlier ones).
    pub(crate) produced_tables: HashMap<String, usize>,
    /// Canonical names that were produced via CREATE VIEW.
    ///
    /// Used to determine node type (view vs table) for ID generation.
    pub(crate) produced_views: HashSet<String>,
    /// Maps table canonical name -> list of statement indices that consume it.
    ///
    /// A single table can be consumed by multiple statements.
    pub(crate) consumed_tables: HashMap<String, Vec<usize>>,
    /// All discovered tables and views across statements (for global lineage).
    ///
    /// Union of all produced and consumed relations.
    pub(crate) all_relations: HashSet<String>,
    /// All discovered CTEs across statements.
    ///
    /// CTEs are tracked separately as they have different scoping rules.
    pub(crate) all_ctes: HashSet<String>,
}

impl CrossStatementTracker {
    /// Creates a new cross-statement tracker with empty state.
    pub(crate) fn new() -> Self {
        Self {
            produced_tables: HashMap::new(),
            produced_views: HashSet::new(),
            consumed_tables: HashMap::new(),
            all_relations: HashSet::new(),
            all_ctes: HashSet::new(),
        }
    }

    /// Records that a table was produced by a statement.
    ///
    /// This should be called for CREATE TABLE, INSERT INTO (creating), and similar DDL.
    /// If the same table is produced by multiple statements, the later one wins.
    pub(crate) fn record_produced(&mut self, canonical: &str, statement_index: usize) {
        self.produced_tables
            .insert(canonical.to_string(), statement_index);
        self.all_relations.insert(canonical.to_string());
    }

    /// Records that a view was produced by a statement.
    ///
    /// Views are tracked separately to ensure correct node type in lineage graphs.
    /// This also calls `record_produced` internally.
    pub(crate) fn record_view_produced(&mut self, canonical: &str, statement_index: usize) {
        self.produced_views.insert(canonical.to_string());
        self.record_produced(canonical, statement_index);
    }

    /// Records that a table was consumed by a statement.
    ///
    /// A single table can be consumed by multiple statements, and a single statement
    /// can consume multiple tables. All consumer indices are tracked.
    pub(crate) fn record_consumed(&mut self, canonical: &str, statement_index: usize) {
        self.consumed_tables
            .entry(canonical.to_string())
            .or_default()
            .push(statement_index);
        self.all_relations.insert(canonical.to_string());
    }

    /// Records a CTE definition for global tracking.
    ///
    /// CTEs are tracked separately from tables/views as they have statement-scoped lifetime.
    pub(crate) fn record_cte(&mut self, cte_name: &str) {
        self.all_ctes.insert(cte_name.to_string());
    }

    /// Checks if a canonical name refers to a view.
    #[cfg(test)]
    pub(crate) fn is_view(&self, canonical: &str) -> bool {
        self.produced_views.contains(canonical)
    }

    /// Checks if a table was produced by an earlier statement.
    ///
    /// Used to determine if a table reference is to a locally-created table
    /// (as opposed to an external table from imported schema).
    pub(crate) fn was_produced(&self, canonical: &str) -> bool {
        self.produced_tables.contains_key(canonical)
    }

    /// Gets the statement index that produced a table, if any.
    #[cfg(test)]
    pub(crate) fn producer_index(&self, canonical: &str) -> Option<usize> {
        self.produced_tables.get(canonical).copied()
    }

    /// Removes a table from tracking (for DROP statements).
    ///
    /// This removes the table from both `produced_tables` and `produced_views`.
    /// Note: Does not remove from `all_relations` as the table was still referenced.
    pub(crate) fn remove(&mut self, canonical: &str) {
        self.produced_tables.remove(canonical);
        self.produced_views.remove(canonical);
    }

    /// Returns the correct node ID and type for a relation (view vs table).
    ///
    /// Views get IDs prefixed with "view_", tables get "table_".
    /// This ensures consistent node identification across the lineage graph.
    pub(crate) fn relation_identity(&self, canonical: &str) -> (Arc<str>, NodeType) {
        if self.produced_views.contains(canonical) {
            (generate_node_id("view", canonical), NodeType::View)
        } else {
            (generate_node_id("table", canonical), NodeType::Table)
        }
    }

    /// Returns the node ID for a relation.
    ///
    /// Convenience method that calls `relation_identity` and returns just the ID.
    pub(crate) fn relation_node_id(&self, canonical: &str) -> Arc<str> {
        self.relation_identity(canonical).0
    }

    /// Builds cross-statement edges for the global lineage graph.
    ///
    /// Detects when a table produced by statement N is consumed by statement M (where M > N)
    /// and creates appropriate `CrossStatement` edges. These edges represent data flow
    /// between statements in a multi-statement workload.
    ///
    /// # Edge Direction
    ///
    /// Cross-statement edges are self-referential on the table node (from/to are the same),
    /// with `producer_statement` and `consumer_statement` metadata indicating the flow direction.
    pub(crate) fn build_cross_statement_edges(&self) -> Vec<GlobalEdge> {
        let mut edges = Vec::new();

        for (table_name, consumers) in &self.consumed_tables {
            if let Some(&producer_idx) = self.produced_tables.get(table_name) {
                for &consumer_idx in consumers {
                    if consumer_idx > producer_idx {
                        let edge_id = format!("cross_{producer_idx}_{consumer_idx}");
                        let node_id = self.relation_node_id(table_name);

                        edges.push(GlobalEdge {
                            id: edge_id.into(),
                            from: node_id.clone(),
                            to: node_id,
                            edge_type: EdgeType::CrossStatement,
                            producer_statement: Some(StatementRef {
                                statement_index: producer_idx,
                                node_id: None,
                            }),
                            consumer_statement: Some(StatementRef {
                                statement_index: consumer_idx,
                                node_id: None,
                            }),
                            metadata: None,
                        });
                    }
                }
            }
        }

        edges
    }
}

impl Default for CrossStatementTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_produced_consumed() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_produced("public.users", 0);
        tracker.record_consumed("public.users", 1);
        tracker.record_consumed("public.users", 2);

        assert!(tracker.was_produced("public.users"));
        assert_eq!(tracker.producer_index("public.users"), Some(0));
        assert_eq!(
            tracker.consumed_tables.get("public.users"),
            Some(&vec![1, 2])
        );
    }

    #[test]
    fn test_view_vs_table() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_produced("public.my_table", 0);
        tracker.record_view_produced("public.my_view", 1);

        assert!(!tracker.is_view("public.my_table"));
        assert!(tracker.is_view("public.my_view"));

        let (table_id, table_type) = tracker.relation_identity("public.my_table");
        assert!(table_id.starts_with("table_"));
        assert_eq!(table_type, NodeType::Table);

        let (view_id, view_type) = tracker.relation_identity("public.my_view");
        assert!(view_id.starts_with("view_"));
        assert_eq!(view_type, NodeType::View);
    }

    #[test]
    fn test_cross_statement_edges() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_produced("staging.temp", 0);
        tracker.record_consumed("staging.temp", 1);
        tracker.record_consumed("staging.temp", 2);

        let edges = tracker.build_cross_statement_edges();
        assert_eq!(edges.len(), 2);

        assert!(edges
            .iter()
            .all(|e| e.edge_type == EdgeType::CrossStatement));
        assert!(edges.iter().any(
            |e| e.producer_statement.as_ref().unwrap().statement_index == 0
                && e.consumer_statement.as_ref().unwrap().statement_index == 1
        ));
        assert!(edges.iter().any(
            |e| e.producer_statement.as_ref().unwrap().statement_index == 0
                && e.consumer_statement.as_ref().unwrap().statement_index == 2
        ));
    }

    #[test]
    fn test_remove() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_view_produced("public.temp_view", 0);
        assert!(tracker.is_view("public.temp_view"));

        tracker.remove("public.temp_view");
        assert!(!tracker.is_view("public.temp_view"));
        assert!(!tracker.was_produced("public.temp_view"));
    }

    #[test]
    fn test_no_cross_statement_edges_for_unconsumed_table() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_produced("staging.temp", 0);
        // No consumers recorded

        let edges = tracker.build_cross_statement_edges();
        assert!(edges.is_empty());
    }

    #[test]
    fn test_no_cross_statement_edges_for_external_table() {
        let mut tracker = CrossStatementTracker::new();

        // Table consumed but never produced (external table)
        tracker.record_consumed("external.source", 0);
        tracker.record_consumed("external.source", 1);

        let edges = tracker.build_cross_statement_edges();
        assert!(edges.is_empty());
    }

    #[test]
    fn test_no_edge_when_consumer_before_producer() {
        let mut tracker = CrossStatementTracker::new();

        // Statement 1 produces the table
        tracker.record_produced("staging.temp", 1);
        // Statement 0 consumes it (before it's produced - shouldn't create edge)
        tracker.record_consumed("staging.temp", 0);

        let edges = tracker.build_cross_statement_edges();
        assert!(edges.is_empty());
    }

    #[test]
    fn test_multiple_tables_cross_statement() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_produced("staging.a", 0);
        tracker.record_produced("staging.b", 1);
        tracker.record_consumed("staging.a", 2);
        tracker.record_consumed("staging.b", 2);

        let edges = tracker.build_cross_statement_edges();
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn test_record_cte() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_cte("my_cte");
        tracker.record_cte("another_cte");

        assert!(tracker.all_ctes.contains("my_cte"));
        assert!(tracker.all_ctes.contains("another_cte"));
        assert_eq!(tracker.all_ctes.len(), 2);
    }

    #[test]
    fn test_all_relations_tracking() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_produced("staging.a", 0);
        tracker.record_consumed("external.b", 1);
        tracker.record_view_produced("staging.v", 2);

        assert!(tracker.all_relations.contains("staging.a"));
        assert!(tracker.all_relations.contains("external.b"));
        assert!(tracker.all_relations.contains("staging.v"));
        assert_eq!(tracker.all_relations.len(), 3);
    }

    #[test]
    fn test_default_trait() {
        let tracker = CrossStatementTracker::default();
        assert!(tracker.produced_tables.is_empty());
        assert!(tracker.consumed_tables.is_empty());
        assert!(tracker.produced_views.is_empty());
    }

    #[test]
    fn test_relation_node_id() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_produced("public.users", 0);
        tracker.record_view_produced("public.user_view", 1);

        let table_id = tracker.relation_node_id("public.users");
        let view_id = tracker.relation_node_id("public.user_view");

        assert!(table_id.starts_with("table_"));
        assert!(view_id.starts_with("view_"));
        assert_ne!(table_id, view_id);
    }

    #[test]
    fn test_cross_statement_edge_attributes() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_produced("staging.temp", 0);
        tracker.record_consumed("staging.temp", 1);

        let edges = tracker.build_cross_statement_edges();
        assert_eq!(edges.len(), 1);

        let edge = &edges[0];
        assert!(edge.id.starts_with("cross_"));
        assert_eq!(edge.from, edge.to); // Self-referencing edge on the table node
        assert!(edge.producer_statement.is_some());
        assert!(edge.consumer_statement.is_some());
        assert!(edge.metadata.is_none());
    }

    #[test]
    fn test_producer_overwrite() {
        let mut tracker = CrossStatementTracker::new();

        // First producer
        tracker.record_produced("staging.data", 0);
        assert_eq!(tracker.producer_index("staging.data"), Some(0));

        // Second producer overwrites
        tracker.record_produced("staging.data", 2);
        assert_eq!(tracker.producer_index("staging.data"), Some(2));
    }

    #[test]
    fn test_same_statement_producer_consumer() {
        let mut tracker = CrossStatementTracker::new();

        // Statement 0 both produces and consumes (e.g., INSERT INTO ... SELECT FROM same table)
        tracker.record_produced("staging.data", 0);
        tracker.record_consumed("staging.data", 0);

        let edges = tracker.build_cross_statement_edges();
        // No edge because consumer index (0) is not > producer index (0)
        assert!(edges.is_empty());
    }

    #[test]
    fn test_remove_preserves_all_relations() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_produced("staging.temp", 0);
        assert!(tracker.all_relations.contains("staging.temp"));

        tracker.remove("staging.temp");
        // all_relations should still contain the table (it was referenced)
        assert!(tracker.all_relations.contains("staging.temp"));
    }

    #[test]
    fn test_remove_nonexistent_table() {
        let mut tracker = CrossStatementTracker::new();

        // Removing a table that was never recorded should not panic
        tracker.remove("nonexistent.table");
        assert!(!tracker.was_produced("nonexistent.table"));
    }

    #[test]
    fn test_view_edge_type() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_view_produced("analytics.user_summary", 0);
        tracker.record_consumed("analytics.user_summary", 1);

        let edges = tracker.build_cross_statement_edges();
        assert_eq!(edges.len(), 1);

        let edge = &edges[0];
        // Edge should reference view node ID
        assert!(edge.from.starts_with("view_"));
        assert_eq!(edge.edge_type, EdgeType::CrossStatement);
    }

    #[test]
    fn test_complex_etl_pattern() {
        let mut tracker = CrossStatementTracker::new();

        // ETL pipeline: source -> staging -> mart
        // Statement 0: CREATE TABLE staging.raw FROM external.source
        tracker.record_consumed("external.source", 0);
        tracker.record_produced("staging.raw", 0);

        // Statement 1: CREATE TABLE staging.cleaned FROM staging.raw
        tracker.record_consumed("staging.raw", 1);
        tracker.record_produced("staging.cleaned", 1);

        // Statement 2: CREATE TABLE mart.final FROM staging.cleaned
        tracker.record_consumed("staging.cleaned", 2);
        tracker.record_produced("mart.final", 2);

        let edges = tracker.build_cross_statement_edges();
        // Should have 2 cross-statement edges:
        // - staging.raw: 0 -> 1
        // - staging.cleaned: 1 -> 2
        assert_eq!(edges.len(), 2);

        // Get node IDs for verification
        let raw_node_id = tracker.relation_node_id("staging.raw");
        let cleaned_node_id = tracker.relation_node_id("staging.cleaned");

        // Verify edge details using node IDs
        let raw_edge = edges.iter().find(|e| e.from == raw_node_id);
        let cleaned_edge = edges.iter().find(|e| e.from == cleaned_node_id);

        assert!(raw_edge.is_some());
        assert!(cleaned_edge.is_some());

        let raw_edge = raw_edge.unwrap();
        assert_eq!(
            raw_edge
                .producer_statement
                .as_ref()
                .unwrap()
                .statement_index,
            0
        );
        assert_eq!(
            raw_edge
                .consumer_statement
                .as_ref()
                .unwrap()
                .statement_index,
            1
        );
    }

    #[test]
    fn test_multiple_consumers_same_table() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_produced("shared.data", 0);
        tracker.record_consumed("shared.data", 1);
        tracker.record_consumed("shared.data", 2);
        tracker.record_consumed("shared.data", 3);

        let edges = tracker.build_cross_statement_edges();
        // Should have 3 edges (one for each consumer)
        assert_eq!(edges.len(), 3);

        // All edges should have producer_statement.statement_index == 0
        for edge in &edges {
            assert_eq!(edge.producer_statement.as_ref().unwrap().statement_index, 0);
        }
    }

    #[test]
    fn test_unknown_relation_identity() {
        let tracker = CrossStatementTracker::new();

        // Relation that was never recorded should default to table
        let (id, node_type) = tracker.relation_identity("unknown.table");
        assert!(id.starts_with("table_"));
        assert_eq!(node_type, NodeType::Table);
    }

    #[test]
    fn test_duplicate_cte_recording() {
        let mut tracker = CrossStatementTracker::new();

        tracker.record_cte("my_cte");
        tracker.record_cte("my_cte"); // Duplicate

        // Should only have one entry (HashSet deduplication)
        assert_eq!(tracker.all_ctes.len(), 1);
    }

    #[test]
    fn test_edge_id_uniqueness() {
        let mut tracker = CrossStatementTracker::new();

        // Multiple tables with multiple consumers
        tracker.record_produced("table_a", 0);
        tracker.record_produced("table_b", 1);
        tracker.record_consumed("table_a", 2);
        tracker.record_consumed("table_b", 2);
        tracker.record_consumed("table_a", 3);

        let edges = tracker.build_cross_statement_edges();
        assert_eq!(edges.len(), 3);

        // All edge IDs should be unique
        let ids: Vec<_> = edges.iter().map(|e| &e.id).collect();
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique_ids.len());
    }
}
