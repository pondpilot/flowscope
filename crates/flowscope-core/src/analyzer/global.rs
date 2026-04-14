use super::helpers::{generate_node_id, parse_canonical_name};
use super::Analyzer;
use crate::types::{
    Edge, EdgeType, IssueCount, Node, NodeType, ResolvedColumnSchema, ResolvedSchemaMetadata,
    ResolvedSchemaTable, Span, StatementMeta, Summary,
};
use serde_json::{Map as JsonMap, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
#[cfg(feature = "tracing")]
use tracing::debug;

const STATEMENT_FILTERS_METADATA_KEY: &str = "statementFilters";

impl<'a> Analyzer<'a> {
    pub(super) fn build_result(&self) -> crate::AnalyzeResult {
        // Apply CTE filtering if requested
        let hide_ctes = self
            .request
            .options
            .as_ref()
            .and_then(|o| o.hide_ctes)
            .unwrap_or(false);

        let statement_lineages = if hide_ctes {
            let mut filtered = self.statement_lineages.clone();
            for lineage in &mut filtered {
                super::transform::filter_cte_nodes(lineage);
            }
            filtered
        } else {
            self.statement_lineages.clone()
        };

        let (statements, nodes, edges) = self.flatten_lineages(statement_lineages);
        let summary = self.build_summary(&nodes);
        let resolved_schema = self.build_resolved_schema();

        crate::AnalyzeResult {
            statements,
            nodes,
            edges,
            issues: self.issues.clone(),
            summary,
            resolved_schema,
        }
    }

    fn build_resolved_schema(&self) -> Option<ResolvedSchemaMetadata> {
        if self.schema.is_empty() {
            return None;
        }

        let mut tables: Vec<ResolvedSchemaTable> = self
            .schema
            .all_entries()
            .map(|entry| {
                let columns: Vec<ResolvedColumnSchema> = entry
                    .table
                    .columns
                    .iter()
                    .map(|col| ResolvedColumnSchema {
                        name: col.name.clone(),
                        data_type: col.data_type.clone(),
                        origin: Some(entry.origin),
                        is_primary_key: col.is_primary_key,
                        foreign_key: col.foreign_key.clone(),
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
                    constraints: entry.constraints.clone(),
                }
            })
            .collect();

        // Sort by name for consistent output
        tables.sort_by(|a, b| a.name.cmp(&b.name));

        Some(ResolvedSchemaMetadata { tables })
    }

    /// Flatten per-statement lineages into a single top-level graph.
    ///
    /// Nodes that share a canonical identity across statements (e.g. the same
    /// table read by two queries) are merged into a single `Node` whose
    /// `statement_ids` lists every statement that references it. Self-join
    /// instances remain distinct (their local IDs already encode the lexical
    /// occurrence) so their `name_spans` map back to the correct relation use.
    /// Edges are deduplicated by `(from, to, kind)` with `statement_ids`
    /// accumulating every statement that produced the edge.
    fn flatten_lineages(
        &self,
        lineages: Vec<crate::types::StatementLineage>,
    ) -> (Vec<StatementMeta>, Vec<Node>, Vec<Edge>) {
        let mut statement_metas: Vec<StatementMeta> = Vec::with_capacity(lineages.len());
        let mut flat_nodes: HashMap<Arc<str>, Node> = HashMap::new();
        let mut node_insertion_order: Vec<Arc<str>> = Vec::new();
        let mut flat_edges: Vec<Edge> = Vec::new();
        let mut edge_index: HashMap<(Arc<str>, Arc<str>, &'static str), usize> = HashMap::new();
        let mut local_to_global_id: HashMap<Arc<str>, Arc<str>> = HashMap::new();

        for lineage in lineages {
            // Identify nodes whose IDs must stay statement-scoped:
            // - CTEs and derived tables (their IDs encode the statement index)
            // - Self-join instance nodes (their IDs hash canonical+alias+scope,
            //   so they differ from the canonical-only `relation_identity` ID).
            //   Without preserving these, two self-join instances of the same
            //   table collapse into one node, losing the distinction between
            //   `users a` and `users b` in `FROM users a JOIN users b`.
            let mut statement_scoped_relation_ids: HashSet<Arc<str>> = lineage
                .nodes
                .iter()
                .filter(|node| node.node_type == NodeType::Cte)
                .map(|node| node.id.clone())
                .collect();
            for node in &lineage.nodes {
                if matches!(node.node_type, NodeType::Table | NodeType::View) {
                    let canonical = node
                        .qualified_name
                        .clone()
                        .unwrap_or_else(|| node.label.clone());
                    let canonical_id = self.tracker.relation_identity(&canonical).0;
                    if node.id != canonical_id {
                        statement_scoped_relation_ids.insert(node.id.clone());
                    }
                }
            }
            let statement_scoped_column_ids: HashSet<Arc<str>> = lineage
                .edges
                .iter()
                .filter(|edge| {
                    edge.edge_type == EdgeType::Ownership
                        && statement_scoped_relation_ids.contains(&edge.from)
                })
                .map(|edge| edge.to.clone())
                .collect();

            let statement_index = lineage.statement_index;
            let (meta, lineage_nodes, lineage_edges) = lineage.into_meta_and_graph();

            for node in lineage_nodes {
                let canonical = node
                    .qualified_name
                    .clone()
                    .unwrap_or_else(|| node.label.clone());
                let canonical_name = parse_canonical_name(&canonical);
                let preserve_statement_scope = statement_scoped_column_ids.contains(&node.id);
                let global_id = self.global_node_id(&node, &canonical, preserve_statement_scope);
                local_to_global_id.insert(node.id.clone(), global_id.clone());

                match flat_nodes.entry(global_id.clone()) {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        merge_node_into(e.get_mut(), node, statement_index);
                    }
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        let mut initial = Node {
                            id: global_id.clone(),
                            statement_ids: vec![statement_index],
                            canonical_name: Some(canonical_name),
                            ..node
                        };
                        record_statement_filters(&mut initial, statement_index);
                        // name_spans / filters / resolution_source / aggregation
                        // all travel from the source node via the spread above.
                        normalize_name_spans(&mut initial);
                        slot.insert(initial);
                        node_insertion_order.push(global_id);
                    }
                }
            }

            for edge in lineage_edges {
                let from = local_to_global_id
                    .get(&edge.from)
                    .cloned()
                    .unwrap_or_else(|| {
                        #[cfg(feature = "tracing")]
                        debug!(
                            edge_id = %edge.id,
                            node_id = %edge.from,
                            "edge source not in local-to-global mapping, using local ID"
                        );
                        edge.from.clone()
                    });
                let to = local_to_global_id
                    .get(&edge.to)
                    .cloned()
                    .unwrap_or_else(|| {
                        #[cfg(feature = "tracing")]
                        debug!(
                            edge_id = %edge.id,
                            node_id = %edge.to,
                            "edge target not in local-to-global mapping, using local ID"
                        );
                        edge.to.clone()
                    });

                let kind = edge_kind(edge.edge_type);
                let key = (from.clone(), to.clone(), kind);
                if let Some(&idx) = edge_index.get(&key) {
                    let existing = &mut flat_edges[idx];
                    if !existing.statement_ids.contains(&statement_index) {
                        existing.statement_ids.push(statement_index);
                    }
                } else {
                    let mut remapped = Edge {
                        from: from.clone(),
                        to: to.clone(),
                        statement_ids: vec![statement_index],
                        ..edge
                    };
                    // Preserve the edge's local ID. Nothing persistent depends on
                    // statement-local IDs post-build, so reuse is safe.
                    // Clear any stale statement_ids carried over from `edge`.
                    remapped.statement_ids = vec![statement_index];
                    edge_index.insert(key, flat_edges.len());
                    flat_edges.push(remapped);
                }
            }

            statement_metas.push(meta);
            // local_to_global mapping is valid only within the current
            // statement; clear it between statements so local IDs from
            // statement N don't bleed into statement N+1.
            local_to_global_id.clear();
        }

        // Append tracker-derived cross-statement edges (producer/consumer).
        //
        // Unlike intra-statement edges, cross-statement edges are not deduped
        // by `(from, to, kind)`: a self-loop on a shared table may appear in
        // multiple distinct producer/consumer pairs, and collapsing them would
        // lose the ordered `[producer, consumer]` semantics advertised by
        // `CrossStatementTracker::build_cross_statement_edges`. Each tracker
        // edge already has a unique ID derived from `(table, producer, consumer)`;
        // dedup by that ID only to guard against accidental re-emission.
        let mut cross_edge_ids: HashSet<Arc<str>> = HashSet::new();
        for edge in self.tracker.build_cross_statement_edges() {
            if cross_edge_ids.insert(edge.id.clone()) {
                flat_edges.push(edge);
            }
        }

        // Build the ordered node list and drop edges that reference nodes we
        // discarded (e.g. ambiguous-column pruning may leave an edge whose
        // target has no matching node in the flat set).
        let mut nodes: Vec<Node> = node_insertion_order
            .into_iter()
            .filter_map(|id| flat_nodes.remove(&id))
            .collect();

        // Sort name_spans and statement_ids for stable output.
        for node in &mut nodes {
            node.statement_ids.sort_unstable();
            node.statement_ids.dedup();
            node.name_spans.sort_by_key(|s: &Span| (s.start, s.end));
            node.name_spans.dedup();
        }

        let node_ids: HashSet<&Arc<str>> = nodes.iter().map(|n| &n.id).collect();

        #[cfg(feature = "tracing")]
        let edges_before = flat_edges.len();

        flat_edges.retain(|edge| node_ids.contains(&edge.from) && node_ids.contains(&edge.to));

        #[cfg(feature = "tracing")]
        if flat_edges.len() < edges_before {
            debug!(
                removed = edges_before - flat_edges.len(),
                "removed orphaned edges from flattened lineage"
            );
        }

        for edge in &mut flat_edges {
            edge.statement_ids.sort_unstable();
            edge.statement_ids.dedup();
        }

        (statement_metas, nodes, flat_edges)
    }

    fn global_node_id(
        &self,
        node: &Node,
        canonical: &Arc<str>,
        preserve_statement_scope: bool,
    ) -> Arc<str> {
        match node.node_type {
            NodeType::Table | NodeType::View => {
                let canonical_id = self.tracker.relation_identity(canonical).0;
                // Self-join instance nodes have IDs hashed from canonical+alias+scope
                // and differ from the canonical-only ID. Keep their local ID so
                // the two instances of `users a` / `users b` stay as separate
                // nodes in the flat graph.
                if node.id == canonical_id {
                    canonical_id
                } else {
                    node.id.clone()
                }
            }
            // CTEs and derived tables are statement-scoped in the global graph.
            // Their IDs already encode the statement index (via generate_statement_scoped_node_id),
            // so same-named CTEs in different statements remain distinct global nodes.
            NodeType::Cte => node.id.clone(),
            // Columns owned by statement-scoped CTE/derived-table nodes (or
            // self-join instance nodes) must stay local too. Otherwise
            // identical qualified names (e.g. `org.id`) reconnect distinct
            // statements/instances through a shared global column node.
            NodeType::Column if preserve_statement_scope => node.id.clone(),
            NodeType::Column if node.qualified_name.is_some() => {
                generate_node_id("column", canonical)
            }
            _ => node.id.clone(),
        }
    }

    pub(super) fn build_summary(&self, nodes: &[Node]) -> Summary {
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

        let table_count = nodes
            .iter()
            .filter(|n| n.node_type.is_table_or_view())
            .count();
        let cte_count = nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Cte)
            .count();
        let column_count = nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .count();

        // Aggregate join count from all statements
        let join_count: usize = self.statement_lineages.iter().map(|s| s.join_count).sum();

        // Calculate project-level complexity from flat lineage.
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

fn edge_kind(edge_type: crate::types::EdgeType) -> &'static str {
    match edge_type {
        crate::types::EdgeType::Ownership => "ownership",
        crate::types::EdgeType::DataFlow => "data_flow",
        crate::types::EdgeType::Derivation => "derivation",
        crate::types::EdgeType::JoinDependency => "join_dependency",
        crate::types::EdgeType::CrossStatement => "cross_statement",
    }
}

/// Merge an additional statement's worth of node data into an already-inserted
/// flat node.
///
/// Precedence rules:
/// - **First-wins** for `node_type`, `label`, and `expression`: the earliest
///   statement to emit the node defines these and incoming values are
///   discarded.
/// - **None-fill** for `qualified_name`, `span`, `body_span`,
///   `resolution_source`, `aggregation`, and `metadata`: existing non-`None`
///   values are preserved, but incoming values fill in gaps when the
///   existing slot is still `None`.
/// - **Accumulate** for `statement_ids`, `name_spans`, and `filters`: every
///   non-duplicate entry from the incoming node is appended. Final ordering
///   and de-duplication for `statement_ids` / `name_spans` is applied in
///   `flatten_lineages`.
fn merge_node_into(existing: &mut Node, incoming: Node, statement_index: usize) {
    if !existing.statement_ids.contains(&statement_index) {
        existing.statement_ids.push(statement_index);
    }

    record_statement_filters_from_slice(existing, statement_index, &incoming.filters);

    for span in incoming.name_spans {
        if !existing.name_spans.contains(&span) {
            existing.name_spans.push(span);
        }
    }
    // If the incoming node carries a plain `span` but existing has no
    // name_spans yet, preserve it as a fallback occurrence. This keeps
    // parity with `Node::all_name_spans` for types that only populate
    // `span` (e.g., columns).
    if existing.span.is_none() {
        existing.span = incoming.span;
    }
    if existing.body_span.is_none() {
        existing.body_span = incoming.body_span;
    }
    if existing.qualified_name.is_none() {
        existing.qualified_name = incoming.qualified_name;
    }
    if existing.resolution_source.is_none() {
        existing.resolution_source = incoming.resolution_source;
    }
    if existing.aggregation.is_none() {
        existing.aggregation = incoming.aggregation;
    }
    for filter in incoming.filters {
        if !existing
            .filters
            .iter()
            .any(|f| f.expression == filter.expression && f.clause_type == filter.clause_type)
        {
            existing.filters.push(filter);
        }
    }
    if existing.metadata.is_none() {
        existing.metadata = incoming.metadata;
    }
}

fn normalize_name_spans(node: &mut Node) {
    node.name_spans.sort_by_key(|s: &Span| (s.start, s.end));
    node.name_spans.dedup();
}

fn record_statement_filters(node: &mut Node, statement_index: usize) {
    let filters = node.filters.clone();
    record_statement_filters_from_slice(node, statement_index, &filters);
}

fn record_statement_filters_from_slice(
    node: &mut Node,
    statement_index: usize,
    filters: &[crate::types::FilterPredicate],
) {
    if filters.is_empty() {
        return;
    }

    let metadata = node.metadata.get_or_insert_with(HashMap::new);
    let entry = metadata
        .entry(STATEMENT_FILTERS_METADATA_KEY.to_string())
        .or_insert_with(|| Value::Object(JsonMap::new()));

    if !entry.is_object() {
        *entry = Value::Object(JsonMap::new());
    }

    if let Value::Object(statement_filters) = entry {
        let serialized = serde_json::to_value(filters).unwrap_or(Value::Array(Vec::new()));
        statement_filters.insert(statement_index.to_string(), serialized);
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
