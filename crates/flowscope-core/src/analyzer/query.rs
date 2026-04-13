//! Query analysis for SELECT statements, CTEs, and subqueries.
//!
//! This module handles the analysis of query expressions including SELECT projections,
//! FROM clauses, JOINs, WHERE/HAVING filters, and wildcard expansion. It builds the
//! column-level lineage graph by tracking data flow from source columns to output columns.

use super::context::{ColumnRef, OutputColumn, PendingWildcard, StatementContext};
use super::helpers::{
    generate_column_node_id, generate_edge_id, generate_node_id, normalize_schema_type,
};
use super::visitor::{LineageVisitor, Visitor};
use super::Analyzer;
use crate::types::{
    issue_codes, AggregationInfo, Edge, EdgeType, Issue, JoinType, Node, NodeType,
    ResolutionSource, SchemaOrigin,
};
use serde_json::json;
use sqlparser::ast::{self, Query, SetExpr};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
#[cfg(feature = "tracing")]
use tracing::debug;

/// Represents the information needed to add an expanded column during wildcard expansion.
struct ExpandedColumnInfo {
    name: String,
    data_type: Option<String>,
}

impl ExpandedColumnInfo {
    /// Creates column info from schema metadata or output columns.
    fn new(name: String, data_type: Option<String>) -> Self {
        Self { name, data_type }
    }
}

/// Parameters for adding an output column.
pub(super) struct OutputColumnParams {
    pub name: String,
    pub sources: Vec<ColumnRef>,
    pub expression: Option<String>,
    pub data_type: Option<String>,
    pub target_node: Option<String>,
    pub approximate: bool,
    pub aggregation: Option<AggregationInfo>,
}

impl<'a> Analyzer<'a> {
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all, fields(has_target = target_node.is_some())))]
    pub(super) fn analyze_query(
        &mut self,
        ctx: &mut StatementContext,
        query: &Query,
        target_node: Option<&str>,
    ) {
        let mut visitor = LineageVisitor::new(self, ctx, target_node.map(|s| s.to_string()));
        visitor.visit_query(query);
    }

    pub(super) fn analyze_query_body(
        &mut self,
        ctx: &mut StatementContext,
        body: &SetExpr,
        target_node: Option<&str>,
    ) {
        let mut visitor = LineageVisitor::new(self, ctx, target_node.map(|s| s.to_string()));
        visitor.visit_set_expr(body);
    }

    // --- Shared Methods used by SelectAnalyzer, ExpressionAnalyzer, and Statements ---

    /// Adds a source table to the lineage graph.
    ///
    /// This is the main entry point for table resolution and node creation.
    /// Returns the canonical table name for alias registration.
    pub(super) fn add_source_table(
        &mut self,
        ctx: &mut StatementContext,
        table_name: &str,
        target_node: Option<&str>,
        alias: Option<&str>,
    ) -> Option<String> {
        // Resolve the table reference (CTE or regular table)
        let (canonical, node_id) = self.resolve_table_reference(ctx, table_name, alias)?;

        // Create edge to target if specified
        self.create_source_edge(ctx, &node_id, target_node);

        if let Some(span) = self.locate_relation_name_span(ctx, table_name) {
            ctx.add_name_span(&node_id, span);
        }

        Some(canonical)
    }

    /// Resolves a table reference, handling CTEs and regular tables.
    ///
    /// Returns the canonical name and node ID for the resolved table.
    fn resolve_table_reference(
        &mut self,
        ctx: &mut StatementContext,
        table_name: &str,
        alias: Option<&str>,
    ) -> Option<(String, Arc<str>)> {
        // Check if this is a CTE reference
        if ctx.cte_definitions.contains_key(table_name) {
            return self.resolve_cte_reference(ctx, table_name, alias);
        }

        // Regular table or view
        self.resolve_regular_table(ctx, table_name, alias)
    }

    /// Resolves a CTE reference and registers it in scope.
    ///
    /// For CTE self-joins (`FROM cte a JOIN cte b`), each alias gets a distinct
    /// instance node with its own column set. Non-self-join references reuse the
    /// CTE definition node for simpler graphs and correct CTE bypass in hide_ctes.
    fn resolve_cte_reference(
        &mut self,
        ctx: &mut StatementContext,
        cte_name: &str,
        alias: Option<&str>,
    ) -> Option<(String, Arc<str>)> {
        let cte_id = ctx.cte_definitions.get(cte_name)?.clone();

        // Only create a separate instance node for CTE self-joins (when the
        // CTE is already present in any enclosing scope, not just the current one,
        // so that nested subqueries like `FROM (SELECT ... FROM cte a JOIN cte b)`
        // are handled correctly).
        let is_self_join = ctx
            .scope_stack
            .iter()
            .any(|scope| scope.tables.contains_key(cte_name));

        let node_id = if is_self_join {
            let alias_key = alias.unwrap_or(cte_name);
            let scope_id = ctx.current_scope_id().unwrap_or_default();
            let instance_key = format!(
                "statement_{}::scope_{}::{cte_name}::{alias_key}",
                ctx.statement_index, scope_id
            );
            let instance_id = generate_node_id("cte", &instance_key);
            if !ctx.node_ids.contains(&instance_id) {
                ctx.add_node(Node {
                    id: instance_id.clone(),
                    node_type: NodeType::Cte,
                    label: cte_name.to_string().into(),
                    qualified_name: Some(cte_name.to_string().into()),
                    ..Default::default()
                });
                if ctx.current_join_info.join_type.is_some() {
                    ctx.joined_table_info
                        .insert(instance_id.clone(), ctx.current_join_info.clone());
                }
                // Connect CTE definition to its reference instance so that
                // filter_cte_nodes can trace the full data flow chain.
                let edge_id = generate_edge_id(&cte_id, &instance_id);
                ctx.add_edge(Edge::data_flow(
                    edge_id,
                    cte_id.clone(),
                    instance_id.clone(),
                ));
            }
            self.materialize_cte_reference_columns(ctx, cte_name, alias_key, &instance_id);
            instance_id
        } else {
            self.apply_join_metadata_to_existing_node(ctx, &cte_id);
            cte_id
        };

        ctx.register_table_in_scope(cte_name.to_string(), node_id.clone());
        let instance_key = alias.unwrap_or(cte_name).to_string();
        ctx.register_alias_instance(instance_key, cte_name.to_string(), node_id.clone());

        Some((cte_name.to_string(), node_id))
    }

    fn materialize_cte_reference_columns(
        &mut self,
        ctx: &mut StatementContext,
        cte_name: &str,
        alias: &str,
        instance_node_id: &Arc<str>,
    ) {
        if ctx.has_subquery_columns_in_current_scope(alias) {
            return;
        }

        let Some(source_columns) = ctx
            .resolve_subquery_columns(cte_name)
            .map(|cols| cols.to_vec())
        else {
            return;
        };

        let mut instance_columns = Vec::with_capacity(source_columns.len());
        for source_col in source_columns {
            let instance_col_id =
                generate_column_node_id(Some(instance_node_id.as_ref()), &source_col.name);

            let column_node = Node {
                id: instance_col_id.clone(),
                node_type: NodeType::Column,
                label: source_col.name.clone().into(),
                qualified_name: Some(format!("{cte_name}.{}", source_col.name).into()),
                metadata: source_col.data_type.as_ref().map(|dt| {
                    let mut m = HashMap::new();
                    m.insert("data_type".to_string(), json!(dt));
                    m
                }),
                ..Default::default()
            };
            ctx.add_node(column_node);

            let ownership_edge_id = generate_edge_id(instance_node_id.as_ref(), &instance_col_id);
            if !ctx.edge_ids.contains(&ownership_edge_id) {
                ctx.add_edge(Edge {
                    id: ownership_edge_id,
                    from: instance_node_id.clone(),
                    to: instance_col_id.clone(),
                    edge_type: EdgeType::Ownership,
                    expression: None,
                    operation: None,
                    join_type: None,
                    join_condition: None,
                    metadata: None,
                    approximate: None,
                });
            }

            let flow_edge_id = generate_edge_id(source_col.node_id.as_ref(), &instance_col_id);
            if !ctx.edge_ids.contains(&flow_edge_id) {
                ctx.add_edge(Edge {
                    id: flow_edge_id,
                    from: source_col.node_id.clone(),
                    to: instance_col_id.clone(),
                    edge_type: EdgeType::DataFlow,
                    expression: None,
                    operation: None,
                    join_type: None,
                    join_condition: None,
                    metadata: None,
                    approximate: None,
                });
            }

            instance_columns.push(OutputColumn {
                name: source_col.name,
                data_type: source_col.data_type,
                node_id: instance_col_id,
            });
        }

        ctx.register_subquery_columns_in_scope(alias.to_string(), instance_columns);
    }

    fn apply_join_metadata_to_existing_node(&self, ctx: &mut StatementContext, node_id: &Arc<str>) {
        let join_type = ctx.current_join_info.join_type;
        let join_condition = ctx.current_join_info.join_condition.clone();

        if join_type.is_none() && join_condition.is_none() {
            return;
        }

        // Record join info in context map (only if not already recorded)
        ctx.joined_table_info
            .entry(node_id.clone())
            .or_insert_with(|| ctx.current_join_info.clone());
    }

    /// Resolves a regular table or view reference.
    ///
    /// When a table with the same canonical name already exists in the current scope
    /// (self-join), a new node with an alias-specific ID is created. Otherwise the
    /// standard canonical-based node ID is used for backward compatibility.
    fn resolve_regular_table(
        &mut self,
        ctx: &mut StatementContext,
        table_name: &str,
        alias: Option<&str>,
    ) -> Option<(String, Arc<str>)> {
        let resolution = self.canonicalize_table_reference(table_name);
        let canonical = resolution.canonical.clone();

        // Check if this canonical table already has a node in the current scope.
        // If so, this is a self-join and we need a separate instance node.
        let is_self_join = ctx
            .current_scope()
            .is_some_and(|scope| scope.tables.contains_key(&canonical));

        let (id, node_type) = if is_self_join {
            // Self-join: generate alias-specific node ID
            let alias_key = alias.unwrap_or(table_name);
            let scope_id = ctx.current_scope_id().unwrap_or_default();
            self.tracker
                .relation_instance_identity(&canonical, alias_key, scope_id)
        } else {
            self.relation_identity(&canonical)
        };

        let is_known = self.is_table_known(&canonical, resolution.matched_schema);
        let resolution_source = self.determine_resolution_source(&canonical, is_known);

        // Create node if not already present
        if !ctx.node_ids.contains(&id) {
            self.create_table_node(ctx, &canonical, &id, node_type, is_known, resolution_source);
        }

        self.tracker
            .record_consumed(&canonical, ctx.statement_index);
        ctx.register_table_in_scope(canonical.clone(), id.clone());

        // Register instance for alias-aware resolution.
        // Unaliased tables are addressable by both their simple and fully-qualified
        // name so `public.employees.id` resolves to the unaliased side of a self-join.
        if let Some(alias) = alias {
            ctx.register_alias_instance(alias.to_string(), canonical.clone(), id.clone());
        } else {
            let simple_name = crate::analyzer::helpers::extract_simple_name(&canonical);
            ctx.register_alias_instance(simple_name.to_string(), canonical.clone(), id.clone());
            if simple_name != canonical {
                ctx.register_alias_instance(canonical.clone(), canonical.clone(), id.clone());
            }
        }

        Some((canonical, id))
    }

    /// Determines if a table is considered "known" to avoid false unresolved warnings.
    ///
    /// A table is known if any of:
    /// - `matched_schema`: Found in imported or implied schema
    /// - `produced`: Created by an earlier statement in the workload (CREATE TABLE, etc.)
    /// - No tables known at all: When we have zero knowledge, be permissive to avoid false warnings
    fn is_table_known(&self, canonical: &str, matched_schema: bool) -> bool {
        let produced = self.tracker.was_produced(canonical);
        let no_tables_known = self.schema.has_no_known_tables();
        matched_schema || produced || no_tables_known
    }

    /// Determines the resolution source for a table.
    fn determine_resolution_source(
        &self,
        canonical: &str,
        is_known: bool,
    ) -> Option<ResolutionSource> {
        if let Some(entry) = self.schema.get(canonical) {
            match entry.origin {
                SchemaOrigin::Imported => Some(ResolutionSource::Imported),
                SchemaOrigin::Implied => Some(ResolutionSource::Implied),
            }
        } else if !is_known {
            Some(ResolutionSource::Unknown)
        } else {
            None
        }
    }

    /// Creates a table node and adds it to the context.
    fn create_table_node(
        &mut self,
        ctx: &mut StatementContext,
        canonical: &str,
        id: &Arc<str>,
        node_type: NodeType,
        is_known: bool,
        resolution_source: Option<ResolutionSource>,
    ) {
        let metadata = if is_known {
            None
        } else {
            let mut issue = Issue::warning(
                issue_codes::UNRESOLVED_REFERENCE,
                format!(
                    "Table '{canonical}' could not be resolved using provided schema metadata or search path"
                ),
            )
            .with_statement(ctx.statement_index);
            // Attach span if we can find the table name in the SQL
            if let Some(span) = self.find_span(canonical) {
                issue = issue.with_span(span);
            }
            self.issues.push(issue);
            let mut meta = HashMap::new();
            meta.insert("placeholder".to_string(), json!(true));
            Some(meta)
        };

        ctx.add_node(Node {
            id: id.clone(),
            node_type,
            label: crate::analyzer::helpers::extract_simple_name(canonical).into(),
            qualified_name: Some(canonical.to_string().into()),
            metadata,
            resolution_source,
            ..Default::default()
        });

        // Record join metadata in context map (not on the node)
        if ctx.current_join_info.join_type.is_some() {
            ctx.joined_table_info
                .insert(id.clone(), ctx.current_join_info.clone());
        }
    }

    /// Creates a data flow edge from source to target.
    fn create_source_edge(
        &mut self,
        ctx: &mut StatementContext,
        source_id: &Arc<str>,
        target_node: Option<&str>,
    ) {
        let Some(target) = target_node else { return };

        let edge_id = generate_edge_id(source_id, target);
        if ctx.edge_ids.contains(&edge_id) {
            return;
        }

        ctx.add_edge(Edge {
            id: edge_id,
            from: source_id.clone(),
            to: target.to_string().into(),
            edge_type: EdgeType::DataFlow,
            expression: None,
            operation: ctx.last_operation.as_deref().map(Into::into),
            join_type: ctx.current_join_info.join_type,
            join_condition: ctx
                .current_join_info
                .join_condition
                .as_deref()
                .map(Into::into),
            metadata: None,
            approximate: None,
        });
    }

    pub(super) fn add_table_columns_from_schema(
        &mut self,
        ctx: &mut StatementContext,
        table_canonical: &str,
        table_node_id: &str,
    ) {
        if let Some(schema_entry) = self.schema.get(table_canonical) {
            // We must clone columns to avoid borrowing self while iterating
            let columns = schema_entry.table.columns.clone();
            for col in columns {
                let col_node_id = generate_column_node_id(Some(table_node_id), &col.name);

                // Add column node
                let col_node = Node {
                    id: col_node_id.clone(),
                    node_type: NodeType::Column,
                    label: col.name.clone().into(),
                    qualified_name: Some(format!("{}.{}", table_canonical, col.name).into()),
                    ..Default::default()
                };
                ctx.add_node(col_node);

                // Add ownership edge from table to column
                let edge_id = generate_edge_id(table_node_id, &col_node_id);
                if !ctx.edge_ids.contains(&edge_id) {
                    ctx.add_edge(Edge {
                        id: edge_id,
                        from: table_node_id.to_string().into(),
                        to: col_node_id,
                        edge_type: EdgeType::Ownership,
                        expression: None,
                        operation: None,
                        join_type: None,
                        join_condition: None,
                        metadata: None,
                        approximate: None,
                    });
                }
            }
        }
    }

    fn wildcard_sources_in_current_scope(&self, ctx: &StatementContext) -> Vec<(String, String)> {
        let Some(scope) = ctx.current_scope() else {
            return Vec::new();
        };

        let mut sources_by_node_id: HashMap<Arc<str>, (String, String)> = HashMap::new();
        for (qualifier, instance) in &scope.alias_instances {
            let should_replace = sources_by_node_id
                .get(&instance.node_id)
                .map(|(_, existing_qualifier)| {
                    self.should_prefer_instance_qualifier(
                        existing_qualifier,
                        qualifier,
                        &instance.canonical,
                    )
                })
                .unwrap_or(true);

            if should_replace {
                sources_by_node_id.insert(
                    instance.node_id.clone(),
                    (instance.canonical.clone(), qualifier.clone()),
                );
            }
        }

        sources_by_node_id.into_values().collect()
    }

    /// Decide which qualifier to keep when two alias keys map to the same node ID.
    ///
    /// For unaliased tables, `alias_instances` contains entries under both the
    /// simple name (`"employees"`) and the canonical name (`"public.employees"`).
    /// When expanding wildcards we only want one entry per node. This heuristic
    /// picks the canonical (fully-qualified) name when available, falling back to
    /// the longer qualifier when both are user-defined aliases (longer names tend
    /// to be more descriptive and stable across refactors).
    fn should_prefer_instance_qualifier(
        &self,
        existing_qualifier: &str,
        candidate_qualifier: &str,
        canonical: &str,
    ) -> bool {
        // Prefer the canonical (fully-qualified) form when one matches
        if existing_qualifier == canonical {
            return false;
        }
        if candidate_qualifier == canonical {
            return true;
        }
        // Between two user-defined aliases, prefer the longer (more descriptive) one.
        // When lengths are equal, use lexicographic order for deterministic output.
        match candidate_qualifier.len().cmp(&existing_qualifier.len()) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => candidate_qualifier < existing_qualifier,
        }
    }

    pub(crate) fn expand_wildcard(
        &mut self,
        ctx: &mut StatementContext,
        table_qualifier: Option<&str>,
        target_node: Option<&str>,
    ) {
        // Resolve wildcard sources as (canonical, qualifier) pairs so repeated
        // relation instances in self-joins are expanded independently.
        let tables_to_expand: Vec<(String, String)> = if let Some(qualifier) = table_qualifier {
            let resolved = self.resolve_table_alias(ctx, Some(qualifier));
            resolved
                .into_iter()
                .map(|canonical| (canonical, qualifier.to_string()))
                .collect()
        } else {
            self.wildcard_sources_in_current_scope(ctx)
        };

        for (table_canonical, source_qualifier) in tables_to_expand {
            // First collect column info to avoid borrow conflict
            let columns_to_add: Option<Vec<ExpandedColumnInfo>> = self
                .schema
                .get(&table_canonical)
                .map(|schema_entry| {
                    schema_entry
                        .table
                        .columns
                        .iter()
                        .map(|col| {
                            ExpandedColumnInfo::new(
                                col.name.clone(),
                                col.data_type.as_ref().map(|dt| normalize_schema_type(dt)),
                            )
                        })
                        .collect()
                })
                .or_else(|| {
                    ctx.resolve_subquery_columns(&table_canonical)
                        .and_then(|cte_cols| {
                            // Only return Some if there are actual columns.
                            // An empty column list means the CTE used SELECT * without schema,
                            // since valid SQL CTEs always produce at least one column.
                            // Note: A future improvement could use an enum like CteColumns::Known(Vec)
                            // vs CteColumns::Unknown to make this distinction explicit.
                            if cte_cols.is_empty() {
                                None
                            } else {
                                Some(
                                    cte_cols
                                        .iter()
                                        .map(|col| {
                                            ExpandedColumnInfo::new(
                                                col.name.clone(),
                                                col.data_type.clone(),
                                            )
                                        })
                                        .collect(),
                                )
                            }
                        })
                });

            if let Some(columns) = columns_to_add {
                // Expand from schema - NOT approximate.
                for col_info in columns {
                    let sources = vec![ColumnRef {
                        table: Some(source_qualifier.clone()),
                        column: col_info.name.clone(),
                    }];
                    self.add_output_column(
                        ctx,
                        &col_info.name,
                        sources,
                        None,
                        col_info.data_type,
                        target_node,
                        false,
                    );
                }
            } else {
                // No schema available - emit approximate lineage warning
                // Create a table-to-table edge marked as approximate
                let mut issue = Issue::info(
                    issue_codes::APPROXIMATE_LINEAGE,
                    format!("SELECT * from '{table_canonical}' - column list unknown without schema metadata"),
                )
                .with_statement(ctx.statement_index);
                if let Some(span) = self.find_span(&table_canonical) {
                    issue = issue.with_span(span);
                }
                self.issues.push(issue);

                // If there's a target node, create an approximate edge from source table to target
                // and record the pending wildcard for backward inference
                if let Some(target) = target_node {
                    // Prefer instance-aware lookup for the qualifier used during expansion.
                    // The canonical fallback covers edge cases where instance tracking is
                    // unavailable (for example, some derived-table paths).
                    let source_node_id = self
                        .resolve_instance_node_id(ctx, &source_qualifier)
                        .or_else(|| ctx.table_node_ids.get(&table_canonical).cloned());
                    if let Some(source_node_id) = source_node_id {
                        let edge_id = generate_edge_id(&source_node_id, target);
                        if !ctx.edge_ids.contains(&edge_id) {
                            ctx.add_edge(Edge {
                                id: edge_id,
                                from: source_node_id.clone(),
                                to: target.to_string().into(),
                                edge_type: EdgeType::DataFlow,
                                expression: None,
                                operation: None,
                                join_type: None,
                                join_condition: None,
                                metadata: None,
                                approximate: Some(true),
                            });
                        }

                        // Find the CTE/alias name from the node ID for backward inference
                        // Use the cte_node_to_name reverse mapping for efficient lookup
                        let target_alias_name =
                            ctx.cte_node_to_name.get(&Arc::from(target)).cloned();

                        // Record pending wildcard for backward column inference
                        if let Some(alias_name) = target_alias_name {
                            ctx.pending_wildcards.push(PendingWildcard {
                                source_canonical: table_canonical.clone(),
                                target_name: alias_name,
                                source_node_id,
                            });
                        }
                    }
                }
            }
        }
    }

    pub(super) fn resolve_table_alias(
        &self,
        ctx: &StatementContext,
        qualifier: Option<&str>,
    ) -> Option<String> {
        match qualifier {
            Some(q) => {
                // Check scopes in reverse order (innermost first) for correct shadowing
                for scope in ctx.scope_stack.iter().rev() {
                    if let Some(canonical) = scope.aliases.get(q) {
                        return Some(canonical.clone());
                    }
                }

                // Fallback to global map (legacy/loose scoping)
                if let Some(canonical) = ctx.table_aliases.get(q) {
                    Some(canonical.clone())
                } else if ctx.cte_definitions.contains_key(q) {
                    // CTE reference
                    Some(q.to_string())
                } else if ctx.subquery_aliases.contains(q) {
                    // Subquery alias - no canonical name
                    None
                } else {
                    // Treat as table name
                    Some(self.canonicalize_table_reference(q).canonical)
                }
            }
            None => None,
        }
    }

    /// Resolve a qualifier to its instance node ID.
    ///
    /// Unlike `resolve_table_alias` which returns the canonical name, this
    /// returns the node ID for the specific alias instance. Essential for
    /// self-joins where `e1` and `e2` map to different nodes.
    pub(super) fn resolve_instance_node_id(
        &self,
        ctx: &StatementContext,
        qualifier: &str,
    ) -> Option<Arc<str>> {
        // Try instance-aware lookup first
        if let Some(instance) = ctx.resolve_alias_instance(qualifier) {
            return Some(instance.node_id.clone());
        }
        // Fallback: resolve alias to canonical, then look up in table_node_ids
        let canonical = self.resolve_table_alias(ctx, Some(qualifier))?;
        ctx.table_node_ids
            .get(&canonical)
            .cloned()
            .or_else(|| ctx.cte_definitions.get(&canonical).cloned())
    }

    fn candidate_tables_for_column(&self, ctx: &StatementContext, column: &str) -> Vec<String> {
        let tables_in_scope = ctx.tables_in_current_scope();
        let normalized_col = self.normalize_identifier(column);
        let mut candidate_tables: Vec<String> = Vec::new();

        for table_canonical in &tables_in_scope {
            // Check aliased subquery columns (CTEs and derived tables)
            if let Some(cte_cols) = ctx.resolve_subquery_columns(table_canonical) {
                if cte_cols.iter().any(|c| c.name == normalized_col) {
                    candidate_tables.push(table_canonical.clone());
                    continue;
                }
            }

            // Check schema metadata
            if let Some(schema_entry) = self.schema.get(table_canonical) {
                if schema_entry
                    .table
                    .columns
                    .iter()
                    .any(|c| self.normalize_identifier(&c.name) == normalized_col)
                {
                    candidate_tables.push(table_canonical.clone());
                }
            }
        }

        candidate_tables
    }

    /// Resolve the canonical source table for filter routing without treating
    /// multiple instances of the same relation as separate sources.
    pub(super) fn resolve_filter_column_table(
        &self,
        ctx: &StatementContext,
        qualifier: Option<&str>,
        column: &str,
    ) -> Option<String> {
        if let Some(q) = qualifier {
            return self.resolve_table_alias(ctx, Some(q));
        }

        let relation_instances = ctx.relation_instances_in_current_scope();
        if relation_instances.is_empty() {
            return None;
        }

        if relation_instances.len() == 1 {
            return Some(relation_instances[0].canonical.clone());
        }

        let candidate_tables = self.candidate_tables_for_column(ctx, column);
        if candidate_tables.len() == 1 {
            candidate_tables.into_iter().next()
        } else {
            None
        }
    }

    pub(crate) fn resolve_column_table(
        &mut self,
        ctx: &StatementContext,
        qualifier: Option<&str>,
        column: &str,
    ) -> Option<String> {
        // If qualifier provided, use standard resolution
        if let Some(q) = qualifier {
            return self.resolve_table_alias(ctx, Some(q));
        }

        // No qualifier - try to find which table owns this column
        // Use scope-based resolution: only consider tables in the current scope.
        // Instance-aware counting is required so self-joins remain ambiguous.
        let relation_instances = ctx.relation_instances_in_current_scope();

        if relation_instances.is_empty() {
            let mut issue = Issue::warning(
                issue_codes::UNRESOLVED_REFERENCE,
                format!("Column '{column}' referenced but no tables are currently in scope"),
            )
            .with_statement(ctx.statement_index);
            if let Some(span) = self.find_span(column) {
                issue = issue.with_span(span);
            }
            self.issues.push(issue);
            return None;
        }

        // If only one relation instance is in scope, assume column belongs to it.
        if relation_instances.len() == 1 {
            return Some(relation_instances[0].canonical.clone());
        }

        let candidate_tables = self.candidate_tables_for_column(ctx, column);

        match candidate_tables.len() {
            1 => {
                let canonical = candidate_tables.first().cloned().unwrap();
                if ctx.relation_instance_count_in_current_scope(&canonical) == 1 {
                    Some(canonical)
                } else {
                    let mut relation_names: Vec<_> = relation_instances
                        .iter()
                        .map(|instance| instance.canonical.clone())
                        .collect();
                    relation_names.sort();
                    let mut issue = Issue::warning(
                        issue_codes::UNRESOLVED_REFERENCE,
                        format!(
                            "Column '{}' exists in multiple tables in scope: {}. Qualify the column to disambiguate.",
                            column,
                            relation_names.join(", ")
                        ),
                    )
                    .with_statement(ctx.statement_index);
                    if let Some(span) = self.find_span(column) {
                        issue = issue.with_span(span);
                    }
                    self.issues.push(issue);
                    None
                }
            }
            0 => {
                // No candidates found - if there's only one relation instance in scope, use it
                // (the column might exist but not be in our schema)
                if relation_instances.len() == 1 {
                    return Some(relation_instances[0].canonical.clone());
                }
                // Multiple tables but column not found in any - ambiguous
                let mut sorted_tables: Vec<_> = relation_instances
                    .iter()
                    .map(|instance| instance.canonical.clone())
                    .collect();
                sorted_tables.sort();
                let mut issue = Issue::warning(
                    issue_codes::UNRESOLVED_REFERENCE,
                    format!(
                        "Column '{}' is ambiguous across tables in scope: {}",
                        column,
                        sorted_tables.join(", ")
                    ),
                )
                .with_statement(ctx.statement_index);
                if let Some(span) = self.find_span(column) {
                    issue = issue.with_span(span);
                }
                self.issues.push(issue);
                None
            }
            _ => {
                // Column exists in multiple tables in scope — require explicit qualifier.
                let mut sorted_candidates = candidate_tables.clone();
                sorted_candidates.sort();
                let mut issue = Issue::warning(
                    issue_codes::UNRESOLVED_REFERENCE,
                    format!(
                        "Column '{}' exists in multiple tables in scope: {}. Qualify the column to disambiguate.",
                        column,
                        sorted_candidates.join(", ")
                    ),
                )
                .with_statement(ctx.statement_index);
                if let Some(span) = self.find_span(column) {
                    issue = issue.with_span(span);
                }
                self.issues.push(issue);
                None
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_output_column(
        &mut self,
        ctx: &mut StatementContext,
        name: &str,
        sources: Vec<ColumnRef>,
        expression: Option<String>,
        data_type: Option<String>,
        target_node: Option<&str>,
        approximate: bool,
    ) {
        self.add_output_column_with_aggregation(
            ctx,
            OutputColumnParams {
                name: name.to_string(),
                sources,
                expression,
                data_type,
                target_node: target_node.map(|s| s.to_string()),
                approximate,
                aggregation: None,
            },
        );
    }

    /// Adds an output column and its associated nodes and edges to the statement context.
    pub(super) fn add_output_column_with_aggregation(
        &mut self,
        ctx: &mut StatementContext,
        params: OutputColumnParams,
    ) {
        let normalized_name = self.normalize_identifier(&params.name);
        let node_id = generate_column_node_id(params.target_node.as_deref(), &normalized_name);
        let ownership_edge_id = params
            .target_node
            .as_deref()
            .map(|target| generate_edge_id(target, &node_id));

        // Create column node
        let col_node = Node {
            id: node_id.clone(),
            node_type: NodeType::Column,
            label: normalized_name.clone().into(),
            qualified_name: None, // Will be set if we have target table
            expression: params.expression.as_deref().map(Into::into),
            metadata: params.data_type.as_ref().map(|dt| {
                let mut m = HashMap::new();
                m.insert("data_type".to_string(), json!(dt));
                m
            }),
            aggregation: params.aggregation,
            ..Default::default()
        };
        ctx.add_node(col_node);

        // Create ownership edge if we have a target
        if let Some(target) = params.target_node {
            let edge_id = ownership_edge_id
                .clone()
                .expect("ownership edge ID should exist when target node exists");
            if !ctx.edge_ids.contains(&edge_id) {
                ctx.add_edge(Edge {
                    id: edge_id,
                    from: target.to_string().into(),
                    to: node_id.clone(),
                    edge_type: EdgeType::Ownership,
                    expression: None,
                    operation: None,
                    join_type: None,
                    join_condition: None,
                    metadata: None,
                    approximate: None,
                });
            }
        }

        // Create data flow edges from source columns
        let mut resolved_sources = 0usize;
        for source in &params.sources {
            let resolved_table =
                self.resolve_column_table(ctx, source.table.as_deref(), &source.column);
            if let Some(ref table_canonical) = resolved_table {
                resolved_sources += 1;
                let mut source_col_id = None;

                // Try alias-specific subquery columns first, then canonical CTE/derived columns.
                if let Some(cte_cols) = source
                    .table
                    .as_deref()
                    .and_then(|qualifier| ctx.resolve_subquery_columns(qualifier))
                    .or_else(|| ctx.resolve_subquery_columns(table_canonical))
                {
                    let normalized_source_col = self.normalize_identifier(&source.column);
                    if let Some(col) = cte_cols.iter().find(|c| c.name == normalized_source_col) {
                        source_col_id = Some(col.node_id.clone());
                    }
                }

                // Determine the node ID for the owning table/CTE.
                // Use instance-aware lookup when a qualifier (alias) is available
                // so that self-join aliases resolve to their own graph node.
                let table_node_id = if source_col_id.is_some() {
                    source
                        .table
                        .as_deref()
                        .and_then(|q| self.resolve_instance_node_id(ctx, q))
                        .or_else(|| ctx.table_node_ids.get(table_canonical).cloned())
                        .or_else(|| ctx.cte_definitions.get(table_canonical).cloned())
                        .unwrap_or_else(|| self.relation_node_id(table_canonical))
                } else {
                    ctx.cte_definitions
                        .get(table_canonical)
                        .cloned()
                        .or_else(|| {
                            source
                                .table
                                .as_deref()
                                .and_then(|q| self.resolve_instance_node_id(ctx, q))
                        })
                        .or_else(|| ctx.table_node_ids.get(table_canonical).cloned())
                        .unwrap_or_else(|| self.relation_node_id(table_canonical))
                };

                // Fallback to generating a new ID
                let source_col_id = source_col_id.unwrap_or_else(|| {
                    generate_column_node_id(
                        Some(&table_node_id),
                        &self.normalize_identifier(&source.column),
                    )
                });

                // Check if source column exists in schema
                self.validate_column(ctx, table_canonical, &source.column);

                // Create source column node if not exists
                let source_col_node = Node {
                    id: source_col_id.clone(),
                    node_type: NodeType::Column,
                    label: source.column.clone().into(),
                    qualified_name: Some(format!("{}.{}", table_canonical, source.column).into()),
                    ..Default::default()
                };
                ctx.add_node(source_col_node);

                // Create ownership edge from table to source column
                let ownership_edge_id = generate_edge_id(&table_node_id, &source_col_id);
                if !ctx.edge_ids.contains(&ownership_edge_id) {
                    ctx.add_edge(Edge {
                        id: ownership_edge_id,
                        from: table_node_id,
                        to: source_col_id.clone(),
                        edge_type: EdgeType::Ownership,
                        expression: None,
                        operation: None,
                        join_type: None,
                        join_condition: None,
                        metadata: None,
                        approximate: None,
                    });
                }

                // Create data flow edge from source to output
                let edge_type = if params.expression.is_some() {
                    EdgeType::Derivation
                } else {
                    EdgeType::DataFlow
                };
                let flow_edge_id = generate_edge_id(&source_col_id, &node_id);
                if !ctx.edge_ids.contains(&flow_edge_id) {
                    ctx.add_edge(Edge {
                        id: flow_edge_id,
                        from: source_col_id,
                        to: node_id.clone(),
                        edge_type,
                        expression: params.expression.as_deref().map(Into::into),
                        operation: None,
                        join_type: None,
                        join_condition: None,
                        metadata: None,
                        approximate: params.approximate.then_some(true),
                    });
                }
            }
        }

        // Source-less projections like COUNT(*) or SELECT 1 still depend on the
        // row set produced by the base relations in the current scope. Record a
        // relation-level dependency to the output column so the base tables are
        // connected in statement lineage and downstream graph rendering.
        if resolved_sources == 0 && params.sources.is_empty() {
            // Base relations are those not introduced via JOIN (i.e., the driving
            // table in the FROM clause). We check the joined_table_info map
            // instead of reading from node fields.
            let base_node_ids: HashSet<&Arc<str>> = ctx
                .nodes
                .iter()
                .filter(|node| !ctx.joined_table_info.contains_key(&node.id))
                .map(|node| &node.id)
                .collect();

            let base_relation_ids: Vec<_> = ctx
                .relation_instances_in_current_scope()
                .into_iter()
                .filter(|instance| base_node_ids.contains(&instance.node_id))
                .map(|instance| instance.node_id)
                .collect();

            // Derivation when the projection computes a value (COUNT(*), 1 + 1);
            // DataFlow when it merely forwards rows without transformation.
            let edge_type = if params.expression.is_some() {
                EdgeType::Derivation
            } else {
                EdgeType::DataFlow
            };

            for relation_id in base_relation_ids {
                let flow_edge_id = generate_edge_id(&relation_id, &node_id);
                if !ctx.edge_ids.contains(&flow_edge_id) {
                    ctx.add_edge(Edge {
                        id: flow_edge_id,
                        from: relation_id,
                        to: node_id.clone(),
                        edge_type,
                        expression: params.expression.as_deref().map(Into::into),
                        operation: None,
                        join_type: None,
                        join_condition: None,
                        metadata: None,
                        approximate: params.approximate.then_some(true),
                    });
                }
            }
        }

        // Drop bare unresolved projections when the column is provably ambiguous
        // in a self-join context (e.g., `SELECT id FROM employees e1 JOIN employees e2`).
        //
        // Conditions (all must hold):
        //   1. No source was resolved (resolved_sources == 0)
        //   2. There are declared sources (the column isn't purely computed)
        //   3. No expression wrapper (it mirrors the source column directly)
        //   4. Exactly one source, unqualified, with a name matching the output
        //   5. Not inside a table-function scope (those have dialect-provided columns)
        //   6. The column is provably ambiguous across relation instances
        //
        // When schema metadata is incomplete we keep the output column and return
        // partial lineage rather than silently dropping it.
        let should_drop_unresolved_projection = resolved_sources == 0
            && !params.sources.is_empty()
            && params.expression.is_none()
            && params.sources.len() == 1
            && params.sources[0].table.is_none()
            && self.normalize_identifier(&params.sources[0].column) == normalized_name
            && !ctx.current_scope_has_table_function_relation()
            && self.is_definitely_ambiguous_unqualified_column(ctx, &params.sources[0].column);
        if should_drop_unresolved_projection {
            #[cfg(feature = "tracing")]
            debug!(
                column = %normalized_name,
                node_id = %node_id,
                "dropping ambiguous unqualified column from output (no resolved sources)"
            );
            ctx.remove_node_by_id(&node_id);

            if let Some(edge_id) = ownership_edge_id {
                ctx.edges.retain(|edge| edge.id != edge_id);
                ctx.edge_ids.remove(&edge_id);
            }
            return;
        }

        // Record output column
        ctx.output_columns.push(OutputColumn {
            name: normalized_name,
            data_type: params.data_type,
            node_id,
        });
    }

    fn is_definitely_ambiguous_unqualified_column(
        &self,
        ctx: &StatementContext,
        column: &str,
    ) -> bool {
        let relation_instances = ctx.relation_instances_in_current_scope();
        if relation_instances.len() <= 1 {
            return false;
        }

        let normalized_col = self.normalize_identifier(column);
        let mut candidate_tables = HashSet::new();
        for table_canonical in ctx.tables_in_current_scope() {
            if ctx
                .resolve_subquery_columns(&table_canonical)
                .is_some_and(|cte_cols| cte_cols.iter().any(|c| c.name == normalized_col))
            {
                candidate_tables.insert(table_canonical.clone());
                continue;
            }

            if self
                .schema
                .get(&table_canonical)
                .is_some_and(|schema_entry| {
                    schema_entry
                        .table
                        .columns
                        .iter()
                        .any(|c| self.normalize_identifier(&c.name) == normalized_col)
                })
            {
                candidate_tables.insert(table_canonical);
            }
        }

        if candidate_tables.len() > 1 {
            return true;
        }

        if let Some(canonical) = candidate_tables.into_iter().next() {
            return ctx.relation_instance_count_in_current_scope(&canonical) > 1;
        }

        relation_instances
            .iter()
            .map(|instance| instance.canonical.as_str())
            .collect::<HashSet<_>>()
            .len()
            == 1
    }

    /// Convert an AST JoinOperator to JoinType enum, also extracting the join condition.
    pub(super) fn convert_join_operator(
        op: &ast::JoinOperator,
    ) -> (Option<JoinType>, Option<String>) {
        match op {
            ast::JoinOperator::Join(constraint) | ast::JoinOperator::Inner(constraint) => (
                Some(JoinType::Inner),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::Left(constraint) | ast::JoinOperator::LeftOuter(constraint) => (
                Some(JoinType::Left),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::Right(constraint) | ast::JoinOperator::RightOuter(constraint) => (
                Some(JoinType::Right),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::FullOuter(constraint) => (
                Some(JoinType::Full),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::CrossJoin(_) => (Some(JoinType::Cross), None),
            ast::JoinOperator::Semi(constraint) | ast::JoinOperator::LeftSemi(constraint) => (
                Some(JoinType::LeftSemi),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::RightSemi(constraint) => (
                Some(JoinType::RightSemi),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::Anti(constraint) | ast::JoinOperator::LeftAnti(constraint) => (
                Some(JoinType::LeftAnti),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::RightAnti(constraint) => (
                Some(JoinType::RightAnti),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::CrossApply => (Some(JoinType::CrossApply), None),
            ast::JoinOperator::OuterApply => (Some(JoinType::OuterApply), None),
            ast::JoinOperator::AsOf { constraint, .. } => (
                Some(JoinType::AsOf),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::StraightJoin(constraint) => (
                Some(JoinType::Inner),
                Self::extract_join_condition(constraint),
            ),
        }
    }

    /// Convert JoinType enum to operation string for edge labels.
    pub(super) fn join_type_to_operation(join_type: Option<JoinType>) -> Option<String> {
        join_type.map(|jt| {
            match jt {
                JoinType::Inner => "INNER_JOIN",
                JoinType::Left => "LEFT_JOIN",
                JoinType::Right => "RIGHT_JOIN",
                JoinType::Full => "FULL_JOIN",
                JoinType::Cross => "CROSS_JOIN",
                JoinType::LeftSemi => "LEFT_SEMI_JOIN",
                JoinType::RightSemi => "RIGHT_SEMI_JOIN",
                JoinType::LeftAnti => "LEFT_ANTI_JOIN",
                JoinType::RightAnti => "RIGHT_ANTI_JOIN",
                JoinType::CrossApply => "CROSS_APPLY",
                JoinType::OuterApply => "OUTER_APPLY",
                JoinType::AsOf => "AS_OF_JOIN",
            }
            .to_string()
        })
    }

    /// Extract the join condition expression from a JoinConstraint
    fn extract_join_condition(constraint: &ast::JoinConstraint) -> Option<String> {
        match constraint {
            ast::JoinConstraint::On(expr) => Some(expr.to_string()),
            ast::JoinConstraint::Using(columns) => {
                let col_names: Vec<String> = columns.iter().map(|c| c.to_string()).collect();
                Some(format!("USING ({})", col_names.join(", ")))
            }
            ast::JoinConstraint::Natural => Some("NATURAL".to_string()),
            ast::JoinConstraint::None => None,
        }
    }

    /// Apply pending filters to table nodes before finalizing the statement.
    pub(super) fn apply_pending_filters(&self, ctx: &mut StatementContext) {
        let instance_pending: Vec<(Arc<str>, Vec<crate::types::FilterPredicate>)> =
            ctx.pending_instance_filters.drain().collect();
        let canonical_pending: Vec<(String, Vec<crate::types::FilterPredicate>)> =
            ctx.pending_filters.drain().collect();

        // Build indexes for O(1) lookups instead of scanning nodes per filter
        let mut id_index: HashMap<Arc<str>, usize> = HashMap::with_capacity(ctx.nodes.len());
        let mut canonical_index: HashMap<Arc<str>, Vec<usize>> =
            HashMap::with_capacity(ctx.nodes.len());
        for (i, node) in ctx.nodes.iter().enumerate() {
            id_index.insert(node.id.clone(), i);
            if let Some(ref qn) = node.qualified_name {
                canonical_index.entry(qn.clone()).or_default().push(i);
            }
        }

        // Apply instance-targeted filters (precise, keyed by node ID)
        for (node_id, filters) in instance_pending {
            if let Some(&idx) = id_index.get(&node_id) {
                ctx.nodes[idx].filters.extend(filters);
            } else {
                #[cfg(feature = "tracing")]
                debug!(
                    %node_id,
                    filter_count = filters.len(),
                    "instance filter target node not found, filters dropped"
                );
            }
        }

        // Apply canonical-keyed filters to ALL matching nodes (important for
        // self-joins where unqualified predicates should apply to every instance
        // of the same canonical table).
        for (table_canonical, filters) in canonical_pending {
            if let Some(indices) = canonical_index.get(table_canonical.as_str()) {
                for &idx in indices {
                    ctx.nodes[idx].filters.extend(filters.clone());
                }
            }
        }
    }

    /// Maximum recursion depth for backward column inference.
    /// Prevents stack overflow on pathological or cyclic queries.
    const MAX_INFERENCE_DEPTH: usize = 20;

    /// Propagates inferred columns backward through SELECT * chains.
    ///
    /// When columns are referenced from a CTE that was created via SELECT *,
    /// this traces the chain back to the source table and creates column nodes.
    /// This enables column-level lineage even when schema metadata is unavailable.
    ///
    /// # Algorithm Overview
    ///
    /// 1. **Group wildcards by target**: Wildcards are grouped by their `target_name`
    ///    (the CTE or derived table alias that receives the `SELECT *` columns).
    ///
    /// 2. **Build node index**: Creates an O(1) lookup map from node ID to node
    ///    to avoid repeated linear scans when collecting owned columns.
    ///
    /// 3. **Propagate columns**: For each target, finds its owned columns and
    ///    creates corresponding columns on the source tables. The `source_canonical`
    ///    field in `PendingWildcard` matches the `target_name` of upstream wildcards,
    ///    enabling recursive chain propagation.
    ///
    /// 4. **Cycle detection**: Uses `visited_pairs` to track (target, source) pairs
    ///    and prevent infinite recursion on cyclic references.
    pub(super) fn propagate_inferred_columns(&mut self, ctx: &mut StatementContext) {
        if ctx.pending_wildcards.is_empty() {
            return;
        }

        // Build map: target_name -> Vec<PendingWildcard>
        let mut wildcards_by_target: HashMap<String, Vec<PendingWildcard>> = HashMap::new();
        for pw in ctx.pending_wildcards.drain(..) {
            wildcards_by_target
                .entry(pw.target_name.clone())
                .or_default()
                .push(pw);
        }

        // Build node ID -> index lookup for O(1) node access
        // This avoids O(N) linear scans in collect_owned_columns
        let node_index: HashMap<Arc<str>, usize> = ctx
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();

        // Track visited target/source pairs to prevent cycles
        let mut visited_pairs: HashSet<(String, String)> = HashSet::new();

        for (target_name, wildcards) in &wildcards_by_target {
            let Some(cte_node_id) = self.lookup_inference_target_node(ctx, target_name) else {
                continue;
            };

            let owned_columns = self.collect_owned_columns(ctx, &cte_node_id, &node_index);
            if owned_columns.is_empty() {
                continue;
            }

            for wildcard in wildcards {
                self.propagate_wildcard_columns(
                    ctx,
                    target_name,
                    wildcard,
                    &owned_columns,
                    &wildcards_by_target,
                    &mut visited_pairs,
                    0, // Start at depth 0
                );
            }
        }
    }

    /// Locates the node ID to use as the inference target for a wildcard.
    fn lookup_inference_target_node(
        &self,
        ctx: &StatementContext,
        target_name: &str,
    ) -> Option<Arc<str>> {
        if let Some(node_id) = ctx.cte_definitions.get(target_name) {
            return Some(node_id.clone());
        }

        ctx.cte_node_to_name
            .iter()
            .find_map(|(node_id, name)| (name == target_name).then(|| node_id.clone()))
    }

    /// Collects column information owned by a CTE node.
    ///
    /// Uses the provided `node_index` for O(1) node lookups instead of linear scans.
    fn collect_owned_columns(
        &self,
        ctx: &StatementContext,
        cte_node_id: &Arc<str>,
        node_index: &HashMap<Arc<str>, usize>,
    ) -> Vec<(String, Option<String>, Arc<str>)> {
        ctx.edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Ownership && e.from == *cte_node_id)
            .filter_map(|e| {
                // O(1) lookup via index instead of O(N) linear scan
                node_index.get(&e.to).and_then(|&idx| {
                    let n = &ctx.nodes[idx];
                    (n.node_type == NodeType::Column).then(|| {
                        let data_type = n
                            .metadata
                            .as_ref()
                            .and_then(|m| m.get("data_type"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        (n.label.to_string(), data_type, n.id.clone())
                    })
                })
            })
            .collect()
    }

    /// Propagates columns through a single wildcard, creating source columns and edges.
    #[allow(clippy::too_many_arguments)]
    fn propagate_wildcard_columns(
        &mut self,
        ctx: &mut StatementContext,
        target_name: &str,
        wildcard: &PendingWildcard,
        columns: &[(String, Option<String>, Arc<str>)],
        wildcards_by_target: &HashMap<String, Vec<PendingWildcard>>,
        visited_pairs: &mut HashSet<(String, String)>,
        depth: usize,
    ) {
        // Enforce recursion depth limit to prevent stack overflow
        if depth >= Self::MAX_INFERENCE_DEPTH {
            return;
        }

        let pair = (target_name.to_string(), wildcard.source_canonical.clone());
        if !visited_pairs.insert(pair.clone()) {
            return;
        }

        let source_columns: Vec<_> = columns
            .iter()
            .filter_map(|(col_name, data_type, target_col_id)| {
                self.create_inferred_column_with_edge(
                    ctx,
                    &wildcard.source_canonical,
                    &wildcard.source_node_id,
                    col_name,
                    data_type.clone(),
                    target_col_id,
                )
            })
            .collect();

        // Recursively propagate if this source is itself a target of another wildcard
        if let Some(upstream_wildcards) = wildcards_by_target.get(&wildcard.source_canonical) {
            for upstream_wildcard in upstream_wildcards {
                self.propagate_wildcard_columns(
                    ctx,
                    &wildcard.source_canonical,
                    upstream_wildcard,
                    &source_columns,
                    wildcards_by_target,
                    visited_pairs,
                    depth + 1,
                );
            }
        }

        visited_pairs.remove(&pair);
    }

    /// Creates an inferred source column and a data flow edge to the target column.
    ///
    /// Returns the column info tuple for recursive propagation, or None if creation failed.
    fn create_inferred_column_with_edge(
        &mut self,
        ctx: &mut StatementContext,
        source_canonical: &str,
        source_node_id: &Arc<str>,
        column_name: &str,
        data_type: Option<String>,
        target_col_id: &Arc<str>,
    ) -> Option<(String, Option<String>, Arc<str>)> {
        let src_id = self.create_inferred_source_column(
            ctx,
            source_canonical,
            source_node_id,
            column_name,
            data_type.clone(),
        )?;

        let edge_id = generate_edge_id(&src_id, target_col_id);
        if !ctx.edge_ids.contains(&edge_id) {
            ctx.add_edge(Edge {
                id: edge_id,
                from: src_id.clone(),
                to: target_col_id.clone(),
                edge_type: EdgeType::DataFlow,
                expression: None,
                operation: None,
                join_type: None,
                join_condition: None,
                metadata: None,
                approximate: None,
            });
        }

        Some((column_name.to_string(), data_type, src_id))
    }

    /// Creates an inferred column node on a source table.
    ///
    /// This is used during backward inference to add column nodes to source tables
    /// that were referenced via SELECT * but lacked schema metadata.
    ///
    /// Returns the column node ID (whether newly created or already existing).
    fn create_inferred_source_column(
        &mut self,
        ctx: &mut StatementContext,
        source_canonical: &str,
        source_node_id: &Arc<str>,
        column_name: &str,
        data_type: Option<String>,
    ) -> Option<Arc<str>> {
        let col_node_id = generate_column_node_id(Some(source_node_id), column_name);

        if ctx.node_ids.contains(&col_node_id) {
            // Already exists, return the ID for edge creation
            return Some(col_node_id);
        }

        // Create column node with Implied resolution source
        ctx.add_node(Node {
            id: col_node_id.clone(),
            node_type: NodeType::Column,
            label: column_name.to_string().into(),
            qualified_name: Some(format!("{}.{}", source_canonical, column_name).into()),
            resolution_source: Some(ResolutionSource::Implied),
            ..Default::default()
        });

        // Create ownership edge: table -> column
        let edge_id = generate_edge_id(source_node_id, &col_node_id);
        if !ctx.edge_ids.contains(&edge_id) {
            ctx.add_edge(Edge {
                id: edge_id,
                from: source_node_id.clone(),
                to: col_node_id.clone(),
                edge_type: EdgeType::Ownership,
                expression: None,
                operation: None,
                join_type: None,
                join_condition: None,
                metadata: None,
                approximate: None,
            });
        }

        // Record in source_table_columns for implied schema
        ctx.record_source_column(source_canonical, column_name, data_type);

        Some(col_node_id)
    }
}
