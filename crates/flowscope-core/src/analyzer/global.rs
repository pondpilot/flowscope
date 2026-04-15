use super::helpers::{generate_node_id, parse_canonical_name};
use super::Analyzer;
use crate::types::{
    Edge, EdgeType, IssueCount, Node, NodeType, ResolvedColumnSchema, ResolvedSchemaMetadata,
    ResolvedSchemaTable, Span, StatementMeta, Summary, STATEMENT_AGGREGATIONS_METADATA_KEY,
    STATEMENT_FILTERS_METADATA_KEY,
};
use serde_json::{Map as JsonMap, Value};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
#[cfg(feature = "tracing")]
use tracing::debug;

const OCCURRENCE_SPANS_METADATA_KEY: &str = "occurrenceSpans";
const OCCURRENCE_STATEMENT_IDS_METADATA_KEY: &str = "occurrenceStatementIds";
const OCCURRENCE_SOURCE_NAMES_METADATA_KEY: &str = "occurrenceSourceNames";
const BODY_SPANS_METADATA_KEY: &str = "bodySpans";
const BODY_STATEMENT_IDS_METADATA_KEY: &str = "bodyStatementIds";
const BODY_SOURCE_NAMES_METADATA_KEY: &str = "bodySourceNames";

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
        let mut state = FlattenState::default();

        for lineage in lineages {
            let scoped = self.collect_statement_scoped_ids(&lineage);
            let statement_index = lineage.statement_index;
            let (meta, lineage_nodes, lineage_edges) = lineage.into_meta_and_graph();
            let statement_source_name = meta.source_name.clone();

            self.merge_lineage_nodes(
                &mut state,
                lineage_nodes,
                &scoped,
                statement_index,
                statement_source_name.as_deref(),
            );
            merge_lineage_edges(&mut state, lineage_edges, statement_index);

            statement_metas.push(meta);
            // local_to_global mapping is valid only within the current
            // statement; clear it between statements so local IDs from
            // statement N don't bleed into statement N+1.
            state.local_to_global_id.clear();
        }

        self.append_cross_statement_edges(&mut state.flat_edges);

        let nodes = finalize_nodes(&mut state.flat_nodes, state.node_insertion_order);
        let edges = finalize_edges(state.flat_edges, &nodes);

        (statement_metas, nodes, edges)
    }

    /// Collect the set of node IDs that must stay statement-scoped during
    /// flattening.
    ///
    /// Three sources feed this set:
    /// - CTEs (their IDs already encode the statement index).
    /// - Tables/views whose ID differs from the canonical identity ID —
    ///   these are self-join instance nodes whose IDs hash
    ///   canonical+alias+scope. Without preserving them, two self-join
    ///   instances of the same table collapse into one node, losing the
    ///   distinction between `users a` and `users b` in
    ///   `FROM users a JOIN users b`.
    /// - Columns owned by any of the above via `EdgeType::Ownership`.
    fn collect_statement_scoped_ids(&self, lineage: &crate::types::StatementLineage) -> ScopedIds {
        let mut relation_ids: HashSet<Arc<str>> = lineage
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
                    relation_ids.insert(node.id.clone());
                }
            }
        }

        let column_ids: HashSet<Arc<str>> = lineage
            .edges
            .iter()
            .filter(|edge| {
                edge.edge_type == EdgeType::Ownership && relation_ids.contains(&edge.from)
            })
            .map(|edge| edge.to.clone())
            .collect();

        ScopedIds { column_ids }
    }

    /// Merge one statement's worth of nodes into the flat graph.
    fn merge_lineage_nodes(
        &self,
        state: &mut FlattenState,
        lineage_nodes: Vec<Node>,
        scoped: &ScopedIds,
        statement_index: usize,
        source_name: Option<&str>,
    ) {
        for node in lineage_nodes {
            let canonical = node
                .qualified_name
                .clone()
                .unwrap_or_else(|| node.label.clone());
            let canonical_name = parse_canonical_name(&canonical);
            let preserve_statement_scope = scoped.column_ids.contains(&node.id);
            let global_id = self.global_node_id(&node, &canonical, preserve_statement_scope);
            state
                .local_to_global_id
                .insert(node.id.clone(), global_id.clone());

            match state.flat_nodes.entry(global_id.clone()) {
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    merge_node_into(e.get_mut(), node, statement_index, source_name);
                }
                std::collections::hash_map::Entry::Vacant(slot) => {
                    let mut initial = Node {
                        id: global_id.clone(),
                        statement_ids: vec![statement_index],
                        canonical_name: Some(canonical_name),
                        ..node
                    };
                    record_statement_filters(&mut initial, statement_index);
                    record_statement_aggregation(&mut initial, statement_index);
                    record_occurrences(&mut initial, statement_index, source_name);
                    record_body_span(&mut initial, statement_index, source_name);
                    // name_spans / filters / resolution_source / aggregation
                    // all travel from the source node via the spread above.
                    normalize_name_spans(&mut initial);
                    slot.insert(initial);
                    state.node_insertion_order.push(global_id);
                }
            }
        }
    }

    /// Append tracker-derived cross-statement edges to `flat_edges`.
    ///
    /// Unlike intra-statement edges, cross-statement edges are not deduped
    /// by `(from, to, kind)`: a self-loop on a shared table may appear in
    /// multiple distinct producer/consumer pairs, and collapsing them would
    /// lose the ordered `[producer, consumer]` semantics advertised by
    /// `CrossStatementTracker::build_cross_statement_edges`. Each tracker
    /// edge already has a unique ID derived from `(table, producer, consumer)`;
    /// dedup by that ID only to guard against accidental re-emission.
    fn append_cross_statement_edges(&self, flat_edges: &mut Vec<Edge>) {
        let mut cross_edge_ids: HashSet<Arc<str>> = HashSet::new();
        for edge in self.tracker.build_cross_statement_edges() {
            if cross_edge_ids.insert(edge.id.clone()) {
                flat_edges.push(edge);
            }
        }
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

/// Mutable state threaded through the flattening pipeline.
#[derive(Default)]
struct FlattenState {
    flat_nodes: HashMap<Arc<str>, Node>,
    node_insertion_order: Vec<Arc<str>>,
    flat_edges: Vec<Edge>,
    edge_index: HashMap<EdgeIndexKey, usize>,
    edge_ids: HashSet<Arc<str>>,
    local_to_global_id: HashMap<Arc<str>, Arc<str>>,
}

#[derive(Hash, PartialEq, Eq)]
struct EdgeIndexKey {
    from: Arc<str>,
    to: Arc<str>,
    kind: &'static str,
    expression: Option<String>,
    operation: Option<String>,
    join_type: &'static str,
    join_condition: Option<String>,
    approximate: Option<bool>,
}

/// Per-statement scoped-id classification computed before consuming a lineage.
struct ScopedIds {
    /// Column IDs that must remain statement-scoped during flattening.
    column_ids: HashSet<Arc<str>>,
}

/// Merge one statement's worth of edges into the flat graph, deduping by the
/// edge's structural identity plus statement-relevant metadata and
/// accumulating `statement_ids`.
fn merge_lineage_edges(state: &mut FlattenState, lineage_edges: Vec<Edge>, statement_index: usize) {
    for edge in lineage_edges {
        let from = state
            .local_to_global_id
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
        let to = state
            .local_to_global_id
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

        let key = edge_index_key(&edge, &from, &to);
        if let Some(&idx) = state.edge_index.get(&key) {
            let existing = &mut state.flat_edges[idx];
            if !existing.statement_ids.contains(&statement_index) {
                existing.statement_ids.push(statement_index);
            }
        } else {
            let edge_id = if state.edge_ids.insert(edge.id.clone()) {
                edge.id.clone()
            } else {
                let unique_id = flat_edge_id(&key);
                state.edge_ids.insert(unique_id.clone());
                unique_id
            };
            // The `statement_ids: vec![statement_index]` field in the struct
            // literal overrides any value carried in `..edge`, so no stale
            // statement_ids can bleed through.
            let remapped = Edge {
                id: edge_id,
                from: from.clone(),
                to: to.clone(),
                statement_ids: vec![statement_index],
                ..edge
            };
            state.edge_index.insert(key, state.flat_edges.len());
            state.flat_edges.push(remapped);
        }
    }
}

/// Build the final ordered node list, sort/dedup `statement_ids` and
/// `name_spans`, and drain the insertion map.
fn finalize_nodes(
    flat_nodes: &mut HashMap<Arc<str>, Node>,
    insertion_order: Vec<Arc<str>>,
) -> Vec<Node> {
    let mut nodes: Vec<Node> = insertion_order
        .into_iter()
        .filter_map(|id| flat_nodes.remove(&id))
        .collect();

    for node in &mut nodes {
        node.statement_ids.sort_unstable();
        node.statement_ids.dedup();
        node.name_spans.sort_by_key(|s: &Span| (s.start, s.end));
        node.name_spans.dedup();
    }

    nodes
}

/// Drop edges referencing discarded nodes (e.g. ambiguous-column pruning may
/// leave an edge whose endpoint has no matching node in the flat set) and
/// sort/dedup each edge's `statement_ids`.
fn finalize_edges(mut flat_edges: Vec<Edge>, nodes: &[Node]) -> Vec<Edge> {
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

    flat_edges
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

fn join_type_key(join_type: Option<crate::types::JoinType>) -> &'static str {
    match join_type {
        None => "",
        Some(crate::types::JoinType::Inner) => "INNER",
        Some(crate::types::JoinType::Left) => "LEFT",
        Some(crate::types::JoinType::Right) => "RIGHT",
        Some(crate::types::JoinType::Full) => "FULL",
        Some(crate::types::JoinType::Cross) => "CROSS",
        Some(crate::types::JoinType::LeftSemi) => "LEFT_SEMI",
        Some(crate::types::JoinType::RightSemi) => "RIGHT_SEMI",
        Some(crate::types::JoinType::LeftAnti) => "LEFT_ANTI",
        Some(crate::types::JoinType::RightAnti) => "RIGHT_ANTI",
        Some(crate::types::JoinType::CrossApply) => "CROSS_APPLY",
        Some(crate::types::JoinType::OuterApply) => "OUTER_APPLY",
        Some(crate::types::JoinType::AsOf) => "AS_OF",
    }
}

fn edge_index_key(edge: &Edge, from: &Arc<str>, to: &Arc<str>) -> EdgeIndexKey {
    EdgeIndexKey {
        from: from.clone(),
        to: to.clone(),
        kind: edge_kind(edge.edge_type),
        expression: edge.expression.as_ref().map(|value| value.to_string()),
        operation: edge.operation.as_ref().map(|value| value.to_string()),
        join_type: join_type_key(edge.join_type),
        join_condition: edge.join_condition.as_ref().map(|value| value.to_string()),
        approximate: edge.approximate,
    }
}

fn flat_edge_id(key: &EdgeIndexKey) -> Arc<str> {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    format!("edge_{:016x}", hasher.finish()).into()
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
fn merge_node_into(
    existing: &mut Node,
    incoming: Node,
    statement_index: usize,
    source_name: Option<&str>,
) {
    let incoming_aggregation = incoming.aggregation.clone();
    record_statement_filters_from_slice(existing, statement_index, &incoming.filters);
    record_statement_aggregation_from_option(existing, statement_index, incoming_aggregation);
    record_occurrences_from_node(existing, &incoming, statement_index, source_name);
    record_body_span_from_node(existing, &incoming, statement_index, source_name);

    if !existing.statement_ids.contains(&statement_index) {
        existing.statement_ids.push(statement_index);
    }

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
    if !filters.is_empty() {
        record_statement_filters_from_slice(node, statement_index, &filters);
    }
}

fn record_statement_aggregation(node: &mut Node, statement_index: usize) {
    if node.aggregation.is_some() {
        record_statement_aggregation_from_option(node, statement_index, node.aggregation.clone());
    }
}

fn record_occurrences(node: &mut Node, statement_index: usize, source_name: Option<&str>) {
    let occurrence_source = node.clone();
    record_occurrences_from_node(node, &occurrence_source, statement_index, source_name);
}

fn record_occurrences_from_node(
    node: &mut Node,
    occurrence_source: &Node,
    statement_index: usize,
    source_name: Option<&str>,
) {
    append_occurrence_records(
        node,
        &occurrence_source.all_name_spans(),
        statement_index,
        source_name,
    );
}

fn append_occurrence_records(
    node: &mut Node,
    spans: &[Span],
    statement_index: usize,
    source_name: Option<&str>,
) {
    if spans.is_empty() {
        return;
    }

    let metadata = node.metadata.get_or_insert_with(HashMap::new);
    ensure_array(metadata, OCCURRENCE_SPANS_METADATA_KEY);
    ensure_array(metadata, OCCURRENCE_STATEMENT_IDS_METADATA_KEY);
    ensure_array(metadata, OCCURRENCE_SOURCE_NAMES_METADATA_KEY);

    for span in spans {
        append_to_array(
            metadata,
            OCCURRENCE_SPANS_METADATA_KEY,
            serde_json::to_value(span).unwrap_or(Value::Null),
        );
        append_to_array(
            metadata,
            OCCURRENCE_STATEMENT_IDS_METADATA_KEY,
            Value::from(statement_index as u64),
        );
        append_to_array(
            metadata,
            OCCURRENCE_SOURCE_NAMES_METADATA_KEY,
            match source_name {
                Some(value) => Value::String(value.to_string()),
                None => Value::Null,
            },
        );
    }
}

fn record_body_span(node: &mut Node, statement_index: usize, source_name: Option<&str>) {
    let body_source = node.clone();
    record_body_span_from_node(node, &body_source, statement_index, source_name);
}

fn record_body_span_from_node(
    node: &mut Node,
    body_source: &Node,
    statement_index: usize,
    source_name: Option<&str>,
) {
    let Some(body_span) = body_source.body_span else {
        return;
    };

    let metadata = node.metadata.get_or_insert_with(HashMap::new);
    ensure_array(metadata, BODY_SPANS_METADATA_KEY);
    ensure_array(metadata, BODY_STATEMENT_IDS_METADATA_KEY);
    ensure_array(metadata, BODY_SOURCE_NAMES_METADATA_KEY);

    append_to_array(
        metadata,
        BODY_SPANS_METADATA_KEY,
        serde_json::to_value(body_span).unwrap_or(Value::Null),
    );
    append_to_array(
        metadata,
        BODY_STATEMENT_IDS_METADATA_KEY,
        Value::from(statement_index as u64),
    );
    append_to_array(
        metadata,
        BODY_SOURCE_NAMES_METADATA_KEY,
        match source_name {
            Some(value) => Value::String(value.to_string()),
            None => Value::Null,
        },
    );
}

fn ensure_array(metadata: &mut HashMap<String, Value>, key: &str) {
    let entry = metadata
        .entry(key.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if !entry.is_array() {
        *entry = Value::Array(Vec::new());
    }
}

fn append_to_array(metadata: &mut HashMap<String, Value>, key: &str, value: Value) {
    if let Some(Value::Array(values)) = metadata.get_mut(key) {
        values.push(value);
    }
}

fn record_statement_aggregation_from_option(
    node: &mut Node,
    statement_index: usize,
    aggregation: Option<crate::types::AggregationInfo>,
) {
    if aggregation.is_none()
        && node.aggregation.is_none()
        && !has_statement_aggregation_tracking(node)
    {
        return;
    }

    ensure_statement_aggregation_tracking(node);
    insert_statement_aggregation(node, statement_index, aggregation);
}

fn has_statement_aggregation_tracking(node: &Node) -> bool {
    node.metadata
        .as_ref()
        .and_then(|metadata| metadata.get(STATEMENT_AGGREGATIONS_METADATA_KEY))
        .is_some_and(Value::is_object)
}

fn ensure_statement_aggregation_tracking(node: &mut Node) {
    if has_statement_aggregation_tracking(node) {
        return;
    }

    let existing_statements = node.statement_ids.clone();
    let existing_aggregation = node.aggregation.clone();
    for statement_id in existing_statements {
        insert_statement_aggregation(node, statement_id, existing_aggregation.clone());
    }
}

fn insert_statement_aggregation(
    node: &mut Node,
    statement_index: usize,
    aggregation: Option<crate::types::AggregationInfo>,
) {
    let metadata = node.metadata.get_or_insert_with(HashMap::new);
    let entry = metadata
        .entry(STATEMENT_AGGREGATIONS_METADATA_KEY.to_string())
        .or_insert_with(|| Value::Object(JsonMap::new()));

    if !entry.is_object() {
        *entry = Value::Object(JsonMap::new());
    }

    if let Value::Object(statement_aggregations) = entry {
        let serialized = match aggregation {
            Some(value) => {
                serde_json::to_value(value).expect("AggregationInfo serialization is infallible")
            }
            None => Value::Null,
        };
        statement_aggregations.insert(statement_index.to_string(), serialized);
    }
}

fn record_statement_filters_from_slice(
    node: &mut Node,
    statement_index: usize,
    filters: &[crate::types::FilterPredicate],
) {
    if filters.is_empty() && node.filters.is_empty() && !has_statement_filter_tracking(node) {
        return;
    }

    ensure_statement_filter_tracking(node);
    insert_statement_filters(node, statement_index, filters);
}

fn has_statement_filter_tracking(node: &Node) -> bool {
    node.metadata
        .as_ref()
        .and_then(|metadata| metadata.get(STATEMENT_FILTERS_METADATA_KEY))
        .is_some_and(Value::is_object)
}

fn ensure_statement_filter_tracking(node: &mut Node) {
    if has_statement_filter_tracking(node) {
        return;
    }

    let existing_statements = node.statement_ids.clone();
    let existing_filters = node.filters.clone();
    for statement_id in existing_statements {
        insert_statement_filters(node, statement_id, &existing_filters);
    }
}

fn insert_statement_filters(
    node: &mut Node,
    statement_index: usize,
    filters: &[crate::types::FilterPredicate],
) {
    let serialized =
        serde_json::to_value(filters).expect("FilterPredicate serialization is infallible");

    let metadata = node.metadata.get_or_insert_with(HashMap::new);
    let entry = metadata
        .entry(STATEMENT_FILTERS_METADATA_KEY.to_string())
        .or_insert_with(|| Value::Object(JsonMap::new()));

    if !entry.is_object() {
        *entry = Value::Object(JsonMap::new());
    }

    if let Value::Object(statement_filters) = entry {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::StatementLineage;
    use crate::{AggregationInfo, AnalyzeRequest, Dialect, EdgeType, JoinType};

    fn make_request() -> AnalyzeRequest {
        AnalyzeRequest {
            sql: String::new(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
            #[cfg(feature = "templating")]
            template_config: None,
        }
    }

    fn make_column(local_id: &str, qualified_name: &str, span: Span) -> Node {
        Node {
            id: local_id.into(),
            node_type: NodeType::Column,
            label: qualified_name
                .rsplit('.')
                .next()
                .unwrap_or(qualified_name)
                .into(),
            qualified_name: Some(qualified_name.into()),
            span: Some(span),
            ..Default::default()
        }
    }

    #[test]
    fn flatten_keeps_distinct_edge_metadata_per_statement() {
        let request = make_request();
        let analyzer = Analyzer::new(&request);

        let stmt_one = StatementLineage {
            statement_index: 0,
            statement_type: "INSERT".to_string(),
            source_name: Some("one.sql".to_string()),
            nodes: vec![
                make_column("src_col_stmt_0", "shared.source.id", Span::new(0, 2)),
                make_column("dst_col_stmt_0", "shared.target.id", Span::new(3, 5)),
            ],
            edges: vec![Edge {
                id: "edge_local_0".into(),
                from: "src_col_stmt_0".into(),
                to: "dst_col_stmt_0".into(),
                edge_type: EdgeType::DataFlow,
                expression: None,
                operation: None,
                join_type: Some(JoinType::Inner),
                join_condition: Some("a.id = b.id".into()),
                metadata: None,
                approximate: None,
                statement_ids: Vec::new(),
            }],
            span: None,
            join_count: 1,
            complexity_score: 1,
            resolved_sql: None,
        };

        let stmt_two = StatementLineage {
            statement_index: 1,
            statement_type: "INSERT".to_string(),
            source_name: Some("two.sql".to_string()),
            nodes: vec![
                make_column("src_col_stmt_1", "shared.source.id", Span::new(10, 12)),
                make_column("dst_col_stmt_1", "shared.target.id", Span::new(13, 15)),
            ],
            edges: vec![Edge {
                id: "edge_local_1".into(),
                from: "src_col_stmt_1".into(),
                to: "dst_col_stmt_1".into(),
                edge_type: EdgeType::DataFlow,
                expression: None,
                operation: None,
                join_type: Some(JoinType::Left),
                join_condition: Some("a.id = b.id".into()),
                metadata: None,
                approximate: None,
                statement_ids: Vec::new(),
            }],
            span: None,
            join_count: 1,
            complexity_score: 1,
            resolved_sql: None,
        };

        let (_statements, _nodes, edges) = analyzer.flatten_lineages(vec![stmt_one, stmt_two]);

        assert_eq!(
            edges.len(),
            2,
            "semantic variants must not collapse into one edge"
        );
        assert!(edges.iter().any(|edge| {
            edge.join_type == Some(JoinType::Inner) && edge.statement_ids == vec![0]
        }));
        assert!(edges.iter().any(|edge| {
            edge.join_type == Some(JoinType::Left) && edge.statement_ids == vec![1]
        }));
        assert_ne!(
            edges[0].id, edges[1].id,
            "distinct variants need distinct edge ids"
        );
    }

    #[test]
    fn flatten_records_occurrence_metadata_for_shared_columns() {
        let request = make_request();
        let analyzer = Analyzer::new(&request);

        let stmt_one = StatementLineage {
            statement_index: 0,
            statement_type: "SELECT".to_string(),
            source_name: Some("models/a.sql".to_string()),
            nodes: vec![make_column(
                "col_stmt_0",
                "shared.users.id",
                Span::new(5, 7),
            )],
            edges: Vec::new(),
            span: None,
            join_count: 0,
            complexity_score: 1,
            resolved_sql: None,
        };
        let stmt_two = StatementLineage {
            statement_index: 1,
            statement_type: "SELECT".to_string(),
            source_name: Some("models/b.sql".to_string()),
            nodes: vec![make_column(
                "col_stmt_1",
                "shared.users.id",
                Span::new(25, 27),
            )],
            edges: Vec::new(),
            span: None,
            join_count: 0,
            complexity_score: 1,
            resolved_sql: None,
        };

        let (_statements, nodes, _edges) = analyzer.flatten_lineages(vec![stmt_one, stmt_two]);
        let node = nodes
            .iter()
            .find(|node| node.qualified_name.as_deref() == Some("shared.users.id"))
            .expect("shared column node");

        assert_eq!(node.statement_ids, vec![0, 1]);

        let occurrence_spans = node
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get(OCCURRENCE_SPANS_METADATA_KEY))
            .and_then(|value| value.as_array())
            .expect("occurrence spans");
        assert_eq!(occurrence_spans.len(), 2);

        let occurrence_statement_ids = node
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get(OCCURRENCE_STATEMENT_IDS_METADATA_KEY))
            .and_then(|value| value.as_array())
            .expect("occurrence statement ids");
        assert_eq!(occurrence_statement_ids.len(), 2);
        assert_eq!(occurrence_statement_ids[0].as_u64(), Some(0));
        assert_eq!(occurrence_statement_ids[1].as_u64(), Some(1));

        let occurrence_source_names = node
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get(OCCURRENCE_SOURCE_NAMES_METADATA_KEY))
            .and_then(|value| value.as_array())
            .expect("occurrence source names");
        assert_eq!(occurrence_source_names.len(), 2);
        assert_eq!(occurrence_source_names[0].as_str(), Some("models/a.sql"));
        assert_eq!(occurrence_source_names[1].as_str(), Some("models/b.sql"));
    }

    #[test]
    fn flatten_records_statement_scoped_aggregation_metadata() {
        let request = make_request();
        let analyzer = Analyzer::new(&request);

        let stmt_one = StatementLineage {
            statement_index: 0,
            statement_type: "SELECT".to_string(),
            source_name: Some("models/counts.sql".to_string()),
            nodes: vec![Node {
                id: "col_stmt_0".into(),
                node_type: NodeType::Column,
                label: "c".into(),
                qualified_name: Some("analytics.metrics.c".into()),
                aggregation: Some(AggregationInfo {
                    is_grouping_key: false,
                    function: Some("COUNT".to_string()),
                    distinct: None,
                }),
                span: Some(Span::new(7, 8)),
                ..Default::default()
            }],
            edges: Vec::new(),
            span: None,
            join_count: 0,
            complexity_score: 1,
            resolved_sql: None,
        };
        let stmt_two = StatementLineage {
            statement_index: 1,
            statement_type: "SELECT".to_string(),
            source_name: Some("models/read_counts.sql".to_string()),
            nodes: vec![Node {
                id: "col_stmt_1".into(),
                node_type: NodeType::Column,
                label: "c".into(),
                qualified_name: Some("analytics.metrics.c".into()),
                span: Some(Span::new(30, 31)),
                ..Default::default()
            }],
            edges: Vec::new(),
            span: None,
            join_count: 0,
            complexity_score: 1,
            resolved_sql: None,
        };

        let (_statements, nodes, _edges) = analyzer.flatten_lineages(vec![stmt_one, stmt_two]);
        let node = nodes
            .iter()
            .find(|node| node.qualified_name.as_deref() == Some("analytics.metrics.c"))
            .expect("shared column node");

        assert_eq!(node.statement_ids, vec![0, 1]);
        assert_eq!(
            node.aggregation_for_statement(0)
                .and_then(|aggregation| aggregation.function),
            Some("COUNT".to_string())
        );
        assert!(node.aggregation_for_statement(1).is_none());

        let per_statement = node
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get(STATEMENT_AGGREGATIONS_METADATA_KEY))
            .and_then(|value| value.as_object())
            .expect("statement aggregations metadata");
        assert!(per_statement
            .get("0")
            .and_then(|value| value.as_object())
            .is_some());
        assert!(per_statement
            .get("1")
            .is_some_and(serde_json::Value::is_null));
    }

    #[test]
    fn flatten_records_statement_scoped_empty_filters() {
        let request = make_request();
        let analyzer = Analyzer::new(&request);

        let stmt_one = StatementLineage {
            statement_index: 0,
            statement_type: "SELECT".to_string(),
            source_name: Some("models/filtered.sql".to_string()),
            nodes: vec![Node {
                id: generate_node_id("table", "public.users"),
                node_type: NodeType::Table,
                label: "users".into(),
                qualified_name: Some("public.users".into()),
                filters: vec![crate::FilterPredicate {
                    expression: "active = true".to_string(),
                    clause_type: crate::FilterClauseType::Where,
                }],
                ..Default::default()
            }],
            edges: Vec::new(),
            span: None,
            join_count: 0,
            complexity_score: 1,
            resolved_sql: None,
        };
        let stmt_two = StatementLineage {
            statement_index: 1,
            statement_type: "SELECT".to_string(),
            source_name: Some("models/plain.sql".to_string()),
            nodes: vec![Node {
                id: generate_node_id("table", "public.users"),
                node_type: NodeType::Table,
                label: "users".into(),
                qualified_name: Some("public.users".into()),
                ..Default::default()
            }],
            edges: Vec::new(),
            span: None,
            join_count: 0,
            complexity_score: 1,
            resolved_sql: None,
        };

        let (_statements, nodes, _edges) = analyzer.flatten_lineages(vec![stmt_one, stmt_two]);
        let node = nodes
            .iter()
            .find(|node| node.qualified_name.as_deref() == Some("public.users"))
            .expect("shared table node");

        assert_eq!(node.filters_for_statement(0).len(), 1);
        assert!(node.filters_for_statement(1).is_empty());

        let per_statement = node
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get(STATEMENT_FILTERS_METADATA_KEY))
            .and_then(|value| value.as_object())
            .expect("statement filters metadata");
        assert_eq!(
            per_statement
                .get("1")
                .and_then(|value| value.as_array())
                .map(std::vec::Vec::len),
            Some(0)
        );
    }
}
