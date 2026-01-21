//! Query analysis for SELECT statements, CTEs, and subqueries.
//!
//! This module handles the analysis of query expressions including SELECT projections,
//! FROM clauses, JOINs, WHERE/HAVING filters, and wildcard expansion. It builds the
//! column-level lineage graph by tracking data flow from source columns to output columns.

use super::context::{ColumnRef, OutputColumn, PendingWildcard, StatementContext};
use super::helpers::{generate_column_node_id, generate_edge_id, normalize_schema_type};
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

/// Represents the information needed to add an expanded column during wildcard expansion.
struct ExpandedColumnInfo {
    name: String,
    table_canonical: String,
    data_type: Option<String>,
}

impl ExpandedColumnInfo {
    /// Creates column info from schema metadata or output columns.
    fn new(name: String, table_canonical: String, data_type: Option<String>) -> Self {
        Self {
            name,
            table_canonical,
            data_type,
        }
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
    ) -> Option<String> {
        // Resolve the table reference (CTE or regular table)
        let (canonical, node_id) = self.resolve_table_reference(ctx, table_name)?;

        // Create edge to target if specified
        self.create_source_edge(ctx, &node_id, target_node);

        Some(canonical)
    }

    /// Resolves a table reference, handling CTEs and regular tables.
    ///
    /// Returns the canonical name and node ID for the resolved table.
    fn resolve_table_reference(
        &mut self,
        ctx: &mut StatementContext,
        table_name: &str,
    ) -> Option<(String, std::sync::Arc<str>)> {
        // Check if this is a CTE reference
        if ctx.cte_definitions.contains_key(table_name) {
            return self.resolve_cte_reference(ctx, table_name);
        }

        // Regular table or view
        self.resolve_regular_table(ctx, table_name)
    }

    /// Resolves a CTE reference and registers it in scope.
    fn resolve_cte_reference(
        &mut self,
        ctx: &mut StatementContext,
        cte_name: &str,
    ) -> Option<(String, std::sync::Arc<str>)> {
        let cte_id = ctx.cte_definitions.get(cte_name)?.clone();
        self.apply_join_metadata_to_existing_node(ctx, &cte_id);
        ctx.register_table_in_scope(cte_name.to_string(), cte_id.clone());
        Some((cte_name.to_string(), cte_id))
    }

    fn apply_join_metadata_to_existing_node(&self, ctx: &mut StatementContext, node_id: &Arc<str>) {
        let join_type = ctx.current_join_info.join_type;
        let join_condition = ctx.current_join_info.join_condition.as_deref();

        if join_type.is_none() && join_condition.is_none() {
            return;
        }

        if let Some(node) = ctx
            .nodes
            .iter_mut()
            .find(|node| node.id.as_ref() == node_id.as_ref())
        {
            if node.join_type.is_none() {
                node.join_type = join_type;
            }
            if node.join_condition.is_none() {
                if let Some(condition) = join_condition {
                    node.join_condition = Some(condition.into());
                }
            }
        }
    }

    /// Resolves a regular table or view reference.
    fn resolve_regular_table(
        &mut self,
        ctx: &mut StatementContext,
        table_name: &str,
    ) -> Option<(String, std::sync::Arc<str>)> {
        let resolution = self.canonicalize_table_reference(table_name);
        let canonical = resolution.canonical.clone();

        let (id, node_type) = self.relation_identity(&canonical);
        let is_known = self.is_table_known(&canonical, resolution.matched_schema);
        let resolution_source = self.determine_resolution_source(&canonical, is_known);

        // Create node if not already present
        if !ctx.node_ids.contains(&id) {
            self.create_table_node(ctx, &canonical, &id, node_type, is_known, resolution_source);
        }

        self.tracker
            .record_consumed(&canonical, ctx.statement_index);
        ctx.register_table_in_scope(canonical.clone(), id.clone());

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
        id: &std::sync::Arc<str>,
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
            expression: None,
            span: None,
            metadata,
            resolution_source,
            filters: Vec::new(),
            join_type: ctx.current_join_info.join_type,
            join_condition: ctx
                .current_join_info
                .join_condition
                .as_deref()
                .map(Into::into),
            aggregation: None,
        });
    }

    /// Creates a data flow edge from source to target.
    fn create_source_edge(
        &mut self,
        ctx: &mut StatementContext,
        source_id: &std::sync::Arc<str>,
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
                    expression: None,
                    span: None,
                    metadata: None,
                    resolution_source: None,
                    filters: Vec::new(),
                    join_type: None,
                    join_condition: None,
                    aggregation: None,
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

    pub(crate) fn expand_wildcard(
        &mut self,
        ctx: &mut StatementContext,
        table_qualifier: Option<&str>,
        target_node: Option<&str>,
    ) {
        // Resolve table qualifier to canonical name
        let tables_to_expand: Vec<String> = if let Some(qualifier) = table_qualifier {
            let resolved = self.resolve_table_alias(ctx, Some(qualifier));
            resolved.into_iter().collect()
        } else {
            // Expand all tables in current scope (not global table_node_ids)
            // This ensures SELECT * only expands tables from the current query's FROM clause
            ctx.tables_in_current_scope()
        };

        for table_canonical in tables_to_expand {
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
                                table_canonical.clone(),
                                col.data_type.as_ref().map(|dt| normalize_schema_type(dt)),
                            )
                        })
                        .collect()
                })
                .or_else(|| {
                    ctx.aliased_subquery_columns
                        .get(&table_canonical)
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
                                                table_canonical.clone(),
                                                col.data_type.clone(),
                                            )
                                        })
                                        .collect(),
                                )
                            }
                        })
                });

            if let Some(columns) = columns_to_add {
                // Expand from schema - NOT approximate
                for col_info in columns {
                    let sources = vec![ColumnRef {
                        table: Some(col_info.table_canonical),
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
                    if let Some(source_node_id) = ctx.table_node_ids.get(&table_canonical).cloned()
                    {
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
        // Use scope-based resolution: only consider tables in the current scope
        let tables_in_scope = ctx.tables_in_current_scope();

        if tables_in_scope.is_empty() {
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

        // If only one table in scope, assume column belongs to it
        if tables_in_scope.len() == 1 {
            return Some(tables_in_scope[0].clone());
        }

        let normalized_col = self.normalize_identifier(column);

        // Collect candidates using CTE output columns and schema metadata
        // Only consider tables that are actually in the current scope
        let mut candidate_tables: Vec<String> = Vec::new();
        for table_canonical in &tables_in_scope {
            // Check aliased subquery columns (CTEs and derived tables)
            if let Some(cte_cols) = ctx.aliased_subquery_columns.get(table_canonical) {
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

        match candidate_tables.len() {
            1 => candidate_tables.first().cloned(),
            0 => {
                // No candidates found - if there's only one table in scope, use it
                // (the column might exist but not be in our schema)
                if tables_in_scope.len() == 1 {
                    return Some(tables_in_scope[0].clone());
                }
                // Multiple tables but column not found in any - ambiguous
                let mut sorted_tables = tables_in_scope.clone();
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

        // Create column node
        let col_node = Node {
            id: node_id.clone(),
            node_type: NodeType::Column,
            label: normalized_name.clone().into(),
            qualified_name: None, // Will be set if we have target table
            expression: params.expression.as_deref().map(Into::into),
            span: None,
            metadata: params.data_type.as_ref().map(|dt| {
                let mut m = HashMap::new();
                m.insert("data_type".to_string(), json!(dt));
                m
            }),
            resolution_source: None,
            filters: Vec::new(),
            join_type: None,
            join_condition: None,
            aggregation: params.aggregation,
        };
        ctx.add_node(col_node);

        // Create ownership edge if we have a target
        if let Some(target) = params.target_node {
            let edge_id = generate_edge_id(&target, &node_id);
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
        for source in &params.sources {
            let resolved_table =
                self.resolve_column_table(ctx, source.table.as_deref(), &source.column);
            if let Some(ref table_canonical) = resolved_table {
                let mut source_col_id = None;

                // Try to find existing node ID if it's a known aliased subquery (CTE or derived table)
                if let Some(cte_cols) = ctx.aliased_subquery_columns.get(table_canonical) {
                    let normalized_source_col = self.normalize_identifier(&source.column);
                    if let Some(col) = cte_cols.iter().find(|c| c.name == normalized_source_col) {
                        source_col_id = Some(col.node_id.clone());
                    }
                }

                // Determine the node ID for the owning table/CTE
                let table_node_id = ctx
                    .table_node_ids
                    .get(table_canonical)
                    .cloned()
                    .or_else(|| ctx.cte_definitions.get(table_canonical).cloned())
                    .unwrap_or_else(|| self.relation_node_id(table_canonical));

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
                    expression: None,
                    span: None,
                    metadata: None,
                    resolution_source: None,
                    filters: Vec::new(),
                    join_type: None,
                    join_condition: None,
                    aggregation: None,
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
                        approximate: if params.approximate { Some(true) } else { None },
                    });
                }
            }
        }

        // Record output column
        ctx.output_columns.push(OutputColumn {
            name: normalized_name,
            data_type: params.data_type,
            node_id,
        });
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
        // Collect pending filters to avoid borrow issues
        let pending: Vec<(String, Vec<crate::types::FilterPredicate>)> =
            ctx.pending_filters.drain().collect();

        for (table_canonical, filters) in pending {
            // Find the node for this table
            if let Some(node) = ctx
                .nodes
                .iter_mut()
                .find(|n| n.qualified_name.as_deref() == Some(&table_canonical))
            {
                node.filters.extend(filters);
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
            expression: None,
            span: None,
            metadata: None,
            resolution_source: Some(ResolutionSource::Implied),
            filters: Vec::new(),
            join_type: None,
            join_condition: None,
            aggregation: None,
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
