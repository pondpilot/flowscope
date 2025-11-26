use super::helpers::parse_canonical_name;
use super::Analyzer;
use crate::types::{
    EdgeType, GlobalEdge, GlobalLineage, GlobalNode, IssueCount, NodeType, ResolvedColumnSchema,
    ResolvedSchemaMetadata, ResolvedSchemaTable, StatementRef, Summary,
};
use std::collections::HashMap;
use std::sync::Arc;

impl<'a> Analyzer<'a> {
    pub(super) fn build_result(&self) -> crate::AnalyzeResult {
        let global_lineage = self.build_global_lineage();
        let summary = self.build_summary(&global_lineage);
        let resolved_schema = self.build_resolved_schema();

        crate::AnalyzeResult {
            statements: self.statement_lineages.clone(),
            global_lineage,
            issues: self.issues.clone(),
            summary,
            resolved_schema,
        }
    }

    fn build_resolved_schema(&self) -> Option<ResolvedSchemaMetadata> {
        if self.schema_tables.is_empty() {
            return None;
        }

        let mut tables: Vec<ResolvedSchemaTable> = self
            .schema_tables
            .values()
            .map(|entry| {
                let columns: Vec<ResolvedColumnSchema> = entry
                    .table
                    .columns
                    .iter()
                    .map(|col| ResolvedColumnSchema {
                        name: col.name.clone(),
                        data_type: col.data_type.clone(),
                        origin: Some(entry.origin),
                    })
                    .collect();

                ResolvedSchemaTable {
                    catalog: entry.table.catalog.clone(),
                    schema: entry.table.schema.clone(),
                    name: entry.table.name.clone(),
                    columns,
                    origin: entry.origin,
                    source_statement_index: entry.source_statement_idx,
                    updated_at: entry.updated_at.to_rfc3339(),
                    temporary: if entry.temporary { Some(true) } else { None },
                }
            })
            .collect();

        // Sort by name for consistent output
        tables.sort_by(|a, b| a.name.cmp(&b.name));

        Some(ResolvedSchemaMetadata { tables })
    }

    pub(super) fn build_global_lineage(&self) -> GlobalLineage {
        let mut global_nodes: HashMap<Arc<str>, GlobalNode> = HashMap::new();
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
                        resolution_source: node.resolution_source,
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
                            id: edge_id.into(),
                            from: self.relation_node_id(table_name),
                            to: self.relation_node_id(table_name),
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
            .filter(|n| n.node_type.is_table_or_view())
            .count();

        let cte_count = global_lineage
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Cte)
            .count();

        let column_count = global_lineage
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .count();

        // Aggregate join count from all statements
        let join_count: usize = self.statement_lineages.iter().map(|s| s.join_count).sum();

        // Calculate project-level complexity from global lineage
        // Uses table/CTE counts since GlobalNode doesn't track per-node join info
        let filter_count: usize = self
            .statement_lineages
            .iter()
            .flat_map(|s| s.nodes.iter())
            .map(|n| n.filters.len())
            .sum();

        let complexity_score =
            calculate_global_complexity(table_count, cte_count, join_count, filter_count);

        Summary {
            statement_count: self.statement_lineages.len(),
            table_count: table_count + cte_count, // Keep combined for backwards compat
            column_count,
            join_count,
            complexity_score,
            issue_count: IssueCount {
                errors: error_count,
                warnings: warning_count,
                infos: info_count,
            },
            has_errors: error_count > 0,
        }
    }
}

/// Calculate complexity score for project-level summary.
///
/// Returns a score from 1-100 based on structural complexity indicators.
/// The weights reflect typical query maintenance and comprehension burden:
/// - Tables (5): Base data sources add moderate complexity
/// - CTEs (8): Higher than tables since they introduce intermediate logic
/// - Joins (10): Highest weight as joins significantly increase query complexity
///   and are common sources of performance issues and logical errors
/// - Filters (2): Low weight since WHERE clauses are straightforward but add
///   some cognitive load when numerous
fn calculate_global_complexity(
    table_count: usize,
    cte_count: usize,
    join_count: usize,
    filter_count: usize,
) -> u8 {
    const TABLE_WEIGHT: usize = 5;
    const CTE_WEIGHT: usize = 8;
    const JOIN_WEIGHT: usize = 10;
    const FILTER_WEIGHT: usize = 2;

    let raw_score = table_count * TABLE_WEIGHT
        + cte_count * CTE_WEIGHT
        + join_count * JOIN_WEIGHT
        + filter_count * FILTER_WEIGHT;

    raw_score.clamp(1, 100) as u8
}
