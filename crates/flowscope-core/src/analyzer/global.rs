use super::helpers::{generate_node_id, parse_canonical_name};
use super::Analyzer;
use crate::types::{
    EdgeType, GlobalEdge, GlobalLineage, GlobalNode, IssueCount, NodeType, StatementRef, Summary,
};
use std::collections::HashMap;

impl<'a> Analyzer<'a> {
    pub(super) fn build_result(&self) -> crate::AnalyzeResult {
        let global_lineage = self.build_global_lineage();
        let summary = self.build_summary(&global_lineage);

        crate::AnalyzeResult {
            statements: self.statement_lineages.clone(),
            global_lineage,
            issues: self.issues.clone(),
            summary,
        }
    }

    pub(super) fn build_global_lineage(&self) -> GlobalLineage {
        let mut global_nodes: HashMap<String, GlobalNode> = HashMap::new();
        let mut global_edges: Vec<GlobalEdge> = Vec::new();

        // Collect all nodes from all statements
        for lineage in &self.statement_lineages {
            for node in &lineage.nodes {
                let canonical = node.qualified_name.clone().unwrap_or(node.label.clone());
                let canonical_name = parse_canonical_name(&canonical);

                global_nodes
                    .entry(node.id.clone())
                    .and_modify(|existing| {
                        existing.statement_refs.push(StatementRef {
                            statement_index: lineage.statement_index,
                            node_id: Some(node.id.clone()),
                        });
                    })
                    .or_insert_with(|| GlobalNode {
                        id: node.id.clone(),
                        node_type: node.node_type,
                        label: node.label.clone(),
                        canonical_name,
                        statement_refs: vec![StatementRef {
                            statement_index: lineage.statement_index,
                            node_id: Some(node.id.clone()),
                        }],
                        metadata: None,
                    });
            }

            // Collect edges
            for edge in &lineage.edges {
                global_edges.push(GlobalEdge {
                    id: edge.id.clone(),
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    edge_type: edge.edge_type,
                    producer_statement: Some(StatementRef {
                        statement_index: lineage.statement_index,
                        node_id: None,
                    }),
                    consumer_statement: None,
                    metadata: None,
                });
            }
        }

        // Detect cross-statement edges
        for (table_name, consumers) in &self.consumed_tables {
            if let Some(&producer_idx) = self.produced_tables.get(table_name) {
                for &consumer_idx in consumers {
                    if consumer_idx > producer_idx {
                        // This is a cross-statement dependency
                        let edge_id = format!("cross_{producer_idx}_{consumer_idx}");
                        global_edges.push(GlobalEdge {
                            id: edge_id,
                            from: generate_node_id("table", table_name),
                            to: generate_node_id("table", table_name),
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

        GlobalLineage {
            nodes: global_nodes.into_values().collect(),
            edges: global_edges,
        }
    }

    pub(super) fn build_summary(&self, global_lineage: &GlobalLineage) -> Summary {
        let error_count = self
            .issues
            .iter()
            .filter(|i| i.severity == crate::Severity::Error)
            .count();
        let warning_count = self
            .issues
            .iter()
            .filter(|i| i.severity == crate::Severity::Warning)
            .count();
        let info_count = self
            .issues
            .iter()
            .filter(|i| i.severity == crate::Severity::Info)
            .count();

        let table_count = global_lineage
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Table || n.node_type == NodeType::Cte)
            .count();

        let column_count = global_lineage
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .count();

        Summary {
            statement_count: self.statement_lineages.len(),
            table_count,
            column_count,
            issue_count: IssueCount {
                errors: error_count,
                warnings: warning_count,
                infos: info_count,
            },
            has_errors: error_count > 0,
        }
    }
}
