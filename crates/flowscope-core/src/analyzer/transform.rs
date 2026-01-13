//! Graph transformation utilities for post-processing lineage results.
//!
//! This module provides functions to transform lineage graphs after analysis,
//! such as filtering out certain node types while preserving connectivity.

use crate::types::{Edge, EdgeType, NodeType, StatementLineage};
use std::collections::{HashMap, HashSet};

/// Remove CTE nodes (and their columns) from lineage and create bypass edges.
///
/// When A → CTE → B exists, this creates A → B directly. Handles chained CTEs
/// (A → CTE1 → CTE2 → B becomes A → B) through transitive edge resolution.
///
/// Fan-in/fan-out is also handled: if A → CTE, B → CTE, CTE → C, CTE → D,
/// the result is A → C, A → D, B → C, B → D.
///
/// Column-level lineage for CTE-owned columns is also bypassed by removing
/// column nodes owned by CTEs and reconnecting their incoming and outgoing
/// edges to preserve dataflow.
pub fn filter_cte_nodes(lineage: &mut StatementLineage) {
    // 1. Identify CTE node IDs (owned strings for simplicity)
    let mut removable_ids: HashSet<String> = lineage
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Cte)
        .map(|n| n.id.as_ref().to_string())
        .collect();

    if removable_ids.is_empty() {
        return;
    }

    // 2. Include column nodes owned by CTEs (via ownership edges)
    for edge in &lineage.edges {
        if edge.edge_type == EdgeType::Ownership && removable_ids.contains(edge.from.as_ref()) {
            removable_ids.insert(edge.to.as_ref().to_string());
        }
    }

    // 3. Build adjacency maps
    let mut incoming: HashMap<String, Vec<String>> = HashMap::new();
    let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();

    for edge in &lineage.edges {
        if edge.edge_type == EdgeType::Ownership {
            continue;
        }
        incoming
            .entry(edge.to.as_ref().to_string())
            .or_default()
            .push(edge.from.as_ref().to_string());
        outgoing
            .entry(edge.from.as_ref().to_string())
            .or_default()
            .push(edge.to.as_ref().to_string());
    }

    // 4. For each removable node, recursively find non-removable sources/targets
    fn find_sources(
        node: &str,
        removable: &HashSet<String>,
        incoming: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
    ) -> Vec<String> {
        if !visited.insert(node.to_string()) {
            return vec![];
        }

        if !removable.contains(node) {
            return vec![node.to_string()];
        }

        incoming
            .get(node)
            .map(|sources| {
                sources
                    .iter()
                    .flat_map(|s| find_sources(s, removable, incoming, visited))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn find_targets(
        node: &str,
        removable: &HashSet<String>,
        outgoing: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
    ) -> Vec<String> {
        if !visited.insert(node.to_string()) {
            return vec![];
        }

        if !removable.contains(node) {
            return vec![node.to_string()];
        }

        outgoing
            .get(node)
            .map(|targets| {
                targets
                    .iter()
                    .flat_map(|t| find_targets(t, removable, outgoing, visited))
                    .collect()
            })
            .unwrap_or_default()
    }

    // 5. Build bypass edges from non-removable sources to non-removable targets, preserving
    // edge metadata from the outgoing edge. For each removable node, pair every incoming edge
    // (src -> removable) with every outgoing edge (removable -> tgt), cloning the outgoing
    // edge's metadata but replacing `from`/`to` with the source/target. This keeps expressions,
    // operations, join info, and approximate flags intact.
    #[derive(PartialEq, Eq)]
    struct EdgeKey {
        from: String,
        to: String,
        edge_type: EdgeType,
        expression: Option<String>,
        operation: Option<String>,
        join_type: Option<crate::types::JoinType>,
        join_condition: Option<String>,
        approximate: Option<bool>,
    }

    impl std::hash::Hash for EdgeKey {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.from.hash(state);
            self.to.hash(state);
            match self.edge_type {
                EdgeType::Ownership => 0u8.hash(state),
                EdgeType::DataFlow => 1u8.hash(state),
                EdgeType::Derivation => 2u8.hash(state),
                EdgeType::CrossStatement => 3u8.hash(state),
            }
            self.expression.hash(state);
            self.operation.hash(state);
            match self.join_type {
                None => 255u8.hash(state),
                Some(crate::types::JoinType::Inner) => 0u8.hash(state),
                Some(crate::types::JoinType::Left) => 1u8.hash(state),
                Some(crate::types::JoinType::Right) => 2u8.hash(state),
                Some(crate::types::JoinType::Full) => 3u8.hash(state),
                Some(crate::types::JoinType::Cross) => 4u8.hash(state),
                Some(crate::types::JoinType::LeftSemi) => 5u8.hash(state),
                Some(crate::types::JoinType::RightSemi) => 6u8.hash(state),
                Some(crate::types::JoinType::LeftAnti) => 7u8.hash(state),
                Some(crate::types::JoinType::RightAnti) => 8u8.hash(state),
                Some(crate::types::JoinType::CrossApply) => 9u8.hash(state),
                Some(crate::types::JoinType::OuterApply) => 10u8.hash(state),
                Some(crate::types::JoinType::AsOf) => 11u8.hash(state),
            }
            self.join_condition.hash(state);
            self.approximate.hash(state);
        }
    }

    let mut bypass_edges: HashMap<EdgeKey, Edge> = HashMap::new();

    // Pre-group incoming/outgoing edges by target/source for efficiency
    let mut incoming_edges: HashMap<String, Vec<&Edge>> = HashMap::new();
    let mut outgoing_edges: HashMap<String, Vec<&Edge>> = HashMap::new();

    for edge in &lineage.edges {
        if edge.edge_type == EdgeType::Ownership {
            continue;
        }
        incoming_edges
            .entry(edge.to.as_ref().to_string())
            .or_default()
            .push(edge);
        outgoing_edges
            .entry(edge.from.as_ref().to_string())
            .or_default()
            .push(edge);
    }

    for removable_id in &removable_ids {
        let sources = find_sources(removable_id, &removable_ids, &incoming, &mut HashSet::new());
        let targets = find_targets(removable_id, &removable_ids, &outgoing, &mut HashSet::new());

        let incomings = incoming_edges
            .get(removable_id)
            .cloned()
            .unwrap_or_default();
        let outgoings = outgoing_edges
            .get(removable_id)
            .cloned()
            .unwrap_or_default();

        for src in &sources {
            for tgt in &targets {
                if src == tgt {
                    continue;
                }

                // If we don't have concrete edges, fall back to generic data_flow
                if incomings.is_empty() || outgoings.is_empty() {
                    let key = EdgeKey {
                        from: src.clone(),
                        to: tgt.clone(),
                        edge_type: EdgeType::DataFlow,
                        expression: None,
                        operation: None,
                        join_type: None,
                        join_condition: None,
                        approximate: None,
                    };
                    bypass_edges.entry(key).or_insert_with(|| {
                        Edge::data_flow(format!("edge_{}_{}", src, tgt), src.clone(), tgt.clone())
                    });
                    continue;
                }

                for out_edge in &outgoings {
                    for in_edge in &incomings {
                        let edge_type = if out_edge.edge_type == EdgeType::Derivation
                            || in_edge.edge_type == EdgeType::Derivation
                        {
                            EdgeType::Derivation
                        } else {
                            out_edge.edge_type
                        };

                        let expression = out_edge
                            .expression
                            .clone()
                            .or_else(|| in_edge.expression.clone());
                        let operation = out_edge
                            .operation
                            .clone()
                            .or_else(|| in_edge.operation.clone());
                        let join_type = out_edge.join_type.or(in_edge.join_type);
                        let join_condition = out_edge
                            .join_condition
                            .clone()
                            .or_else(|| in_edge.join_condition.clone());
                        let approximate = match (out_edge.approximate, in_edge.approximate) {
                            (Some(true), _) | (_, Some(true)) => Some(true),
                            _ => None,
                        };
                        let metadata = out_edge
                            .metadata
                            .clone()
                            .or_else(|| in_edge.metadata.clone());

                        let key = EdgeKey {
                            from: src.clone(),
                            to: tgt.clone(),
                            edge_type,
                            expression: expression.as_ref().map(|v| v.to_string()),
                            operation: operation.as_ref().map(|v| v.to_string()),
                            join_type,
                            join_condition: join_condition.as_ref().map(|v| v.to_string()),
                            approximate,
                        };

                        bypass_edges.entry(key).or_insert_with(|| Edge {
                            id: format!("edge_{}_{}", src, tgt).into(),
                            from: src.clone().into(),
                            to: tgt.clone().into(),
                            edge_type,
                            expression,
                            operation,
                            join_type,
                            join_condition,
                            metadata,
                            approximate,
                        });
                    }
                }
            }
        }
    }

    // 6. Preserve edges between non-removable nodes
    for edge in &lineage.edges {
        if !removable_ids.contains(edge.from.as_ref()) && !removable_ids.contains(edge.to.as_ref())
        {
            let key = EdgeKey {
                from: edge.from.to_string(),
                to: edge.to.to_string(),
                edge_type: edge.edge_type,
                expression: edge.expression.as_ref().map(|v| v.to_string()),
                operation: edge.operation.as_ref().map(|v| v.to_string()),
                join_type: edge.join_type,
                join_condition: edge.join_condition.as_ref().map(|v| v.to_string()),
                approximate: edge.approximate,
            };
            bypass_edges.entry(key).or_insert_with(|| edge.clone());
        }
    }

    // 7. Create the final edge list
    let new_edges: Vec<Edge> = bypass_edges.into_values().collect();

    // 8. Remove CTE and CTE-owned column nodes and update edges
    lineage
        .nodes
        .retain(|n| !removable_ids.contains(n.id.as_ref()));
    lineage.edges = new_edges;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Node;

    fn make_table(id: &str) -> Node {
        Node::table(id, id.rsplit(':').next().unwrap_or(id))
    }

    fn make_cte(id: &str) -> Node {
        Node::cte(id, id.rsplit(':').next().unwrap_or(id))
    }

    fn make_column(id: &str, label: &str) -> Node {
        Node::column(id, label)
    }

    fn make_edge(from: &str, to: &str) -> Edge {
        Edge::data_flow(format!("edge_{}_{}", from, to), from, to)
    }

    #[test]
    fn test_single_cte_bypass() {
        // A → CTE → B should become A → B
        let mut lineage = StatementLineage {
            statement_index: 0,
            statement_type: "SELECT".to_string(),
            source_name: None,
            nodes: vec![
                make_table("table:a"),
                make_cte("cte:temp"),
                make_table("table:b"),
            ],
            edges: vec![
                make_edge("table:a", "cte:temp"),
                make_edge("cte:temp", "table:b"),
            ],
            span: None,
            join_count: 0,
            complexity_score: 1,
        };

        filter_cte_nodes(&mut lineage);

        assert_eq!(lineage.nodes.len(), 2);
        assert!(lineage.nodes.iter().all(|n| n.node_type != NodeType::Cte));
        assert_eq!(lineage.edges.len(), 1);
        assert_eq!(lineage.edges[0].from.as_ref(), "table:a");
        assert_eq!(lineage.edges[0].to.as_ref(), "table:b");
    }

    #[test]
    fn test_chained_ctes() {
        // A → CTE1 → CTE2 → B should become A → B
        let mut lineage = StatementLineage {
            statement_index: 0,
            statement_type: "SELECT".to_string(),
            source_name: None,
            nodes: vec![
                make_table("table:a"),
                make_cte("cte:temp1"),
                make_cte("cte:temp2"),
                make_table("table:b"),
            ],
            edges: vec![
                make_edge("table:a", "cte:temp1"),
                make_edge("cte:temp1", "cte:temp2"),
                make_edge("cte:temp2", "table:b"),
            ],
            span: None,
            join_count: 0,
            complexity_score: 1,
        };

        filter_cte_nodes(&mut lineage);

        assert_eq!(lineage.nodes.len(), 2);
        assert!(lineage.nodes.iter().all(|n| n.node_type != NodeType::Cte));
        assert_eq!(lineage.edges.len(), 1);
        assert_eq!(lineage.edges[0].from.as_ref(), "table:a");
        assert_eq!(lineage.edges[0].to.as_ref(), "table:b");
    }

    #[test]
    fn test_fan_in_fan_out() {
        // A → CTE, B → CTE, CTE → C, CTE → D
        // Should become: A→C, A→D, B→C, B→D
        let mut lineage = StatementLineage {
            statement_index: 0,
            statement_type: "SELECT".to_string(),
            source_name: None,
            nodes: vec![
                make_table("table:a"),
                make_table("table:b"),
                make_cte("cte:temp"),
                make_table("table:c"),
                make_table("table:d"),
            ],
            edges: vec![
                make_edge("table:a", "cte:temp"),
                make_edge("table:b", "cte:temp"),
                make_edge("cte:temp", "table:c"),
                make_edge("cte:temp", "table:d"),
            ],
            span: None,
            join_count: 0,
            complexity_score: 1,
        };

        filter_cte_nodes(&mut lineage);

        assert_eq!(lineage.nodes.len(), 4);
        assert!(lineage.nodes.iter().all(|n| n.node_type != NodeType::Cte));

        // Should have 4 edges: A→C, A→D, B→C, B→D
        assert_eq!(lineage.edges.len(), 4);

        let edge_set: HashSet<(String, String)> = lineage
            .edges
            .iter()
            .map(|e| (e.from.to_string(), e.to.to_string()))
            .collect();

        assert!(edge_set.contains(&("table:a".to_string(), "table:c".to_string())));
        assert!(edge_set.contains(&("table:a".to_string(), "table:d".to_string())));
        assert!(edge_set.contains(&("table:b".to_string(), "table:c".to_string())));
        assert!(edge_set.contains(&("table:b".to_string(), "table:d".to_string())));
    }

    #[test]
    fn test_no_ctes_passthrough() {
        // When there are no CTEs, lineage should be unchanged
        let mut lineage = StatementLineage {
            statement_index: 0,
            statement_type: "SELECT".to_string(),
            source_name: None,
            nodes: vec![make_table("table:a"), make_table("table:b")],
            edges: vec![make_edge("table:a", "table:b")],
            span: None,
            join_count: 0,
            complexity_score: 1,
        };

        let original_nodes = lineage.nodes.len();
        let original_edges = lineage.edges.len();

        filter_cte_nodes(&mut lineage);

        assert_eq!(lineage.nodes.len(), original_nodes);
        assert_eq!(lineage.edges.len(), original_edges);
    }

    #[test]
    fn test_isolated_cte_removed() {
        // CTE with no edges should just be removed
        let mut lineage = StatementLineage {
            statement_index: 0,
            statement_type: "SELECT".to_string(),
            source_name: None,
            nodes: vec![
                make_table("table:a"),
                make_cte("cte:orphan"),
                make_table("table:b"),
            ],
            edges: vec![make_edge("table:a", "table:b")],
            span: None,
            join_count: 0,
            complexity_score: 1,
        };

        filter_cte_nodes(&mut lineage);

        assert_eq!(lineage.nodes.len(), 2);
        assert!(lineage.nodes.iter().all(|n| n.node_type != NodeType::Cte));
        assert_eq!(lineage.edges.len(), 1);
    }

    #[test]
    fn test_cte_column_lineage_bypassed() {
        // Column lineage through a CTE should bypass both the CTE node and its columns.
        let mut lineage = StatementLineage {
            statement_index: 0,
            statement_type: "SELECT".to_string(),
            source_name: None,
            nodes: vec![
                make_table("table:source"),
                make_cte("cte:temp"),
                make_table("table:dest"),
                make_column("column:source_col", "src"),
                make_column("column:cte_col", "temp"),
                make_column("column:dest_col", "dst"),
            ],
            edges: vec![
                // Ownership edges
                Edge::ownership("own_src", "table:source", "column:source_col"),
                Edge::ownership("own_cte", "cte:temp", "column:cte_col"),
                Edge::ownership("own_dst", "table:dest", "column:dest_col"),
                // Column-level flow through the CTE
                make_edge("column:source_col", "column:cte_col"),
                make_edge("column:cte_col", "column:dest_col"),
            ],
            span: None,
            join_count: 0,
            complexity_score: 1,
        };

        filter_cte_nodes(&mut lineage);

        // CTE node and its column should be removed
        assert!(lineage
            .nodes
            .iter()
            .all(|n| n.node_type != NodeType::Cte && n.id.as_ref() != "column:cte_col"));

        // Remaining nodes should include source/dest tables and columns
        let node_ids: HashSet<_> = lineage.nodes.iter().map(|n| n.id.as_ref()).collect();
        assert!(node_ids.contains("table:source"));
        assert!(node_ids.contains("table:dest"));
        assert!(node_ids.contains("column:source_col"));
        assert!(node_ids.contains("column:dest_col"));

        // Bypass edge should connect source column directly to dest column, and no edge should reference the CTE
        let edges: HashSet<(String, String)> = lineage
            .edges
            .iter()
            .map(|e| (e.from.to_string(), e.to.to_string()))
            .collect();

        assert!(edges.contains(&(
            "column:source_col".to_string(),
            "column:dest_col".to_string()
        )));
        assert!(!edges
            .iter()
            .any(|(from, to)| from.contains("cte") || to.contains("cte")));
    }

    #[test]
    fn test_cte_derivation_metadata_preserved() {
        // Ensure derivation/approximate metadata survives bypass.
        let mut lineage = StatementLineage {
            statement_index: 0,
            statement_type: "SELECT".to_string(),
            source_name: None,
            nodes: vec![
                make_table("table:src"),
                make_cte("cte:temp"),
                make_table("table:dst"),
            ],
            edges: vec![
                make_edge("table:src", "cte:temp"),
                Edge {
                    id: "edge_cte_to_dst".into(),
                    from: "cte:temp".into(),
                    to: "table:dst".into(),
                    edge_type: EdgeType::Derivation,
                    expression: Some("foo + bar".into()),
                    operation: Some("AGGREGATE".into()),
                    join_type: None,
                    join_condition: None,
                    metadata: None,
                    approximate: Some(true),
                },
            ],
            span: None,
            join_count: 0,
            complexity_score: 1,
        };

        filter_cte_nodes(&mut lineage);

        assert!(lineage.nodes.iter().all(|n| n.node_type != NodeType::Cte));
        let edge = lineage
            .edges
            .iter()
            .find(|e| e.from.as_ref() == "table:src" && e.to.as_ref() == "table:dst")
            .expect("bypass edge exists");
        assert_eq!(edge.edge_type, EdgeType::Derivation);
        assert_eq!(edge.expression.as_deref(), Some("foo + bar"));
        assert_eq!(edge.operation.as_deref(), Some("AGGREGATE"));
        assert_eq!(edge.approximate, Some(true));
    }
}
