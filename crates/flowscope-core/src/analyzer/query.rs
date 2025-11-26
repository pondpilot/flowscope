//! Query analysis for SELECT statements, CTEs, and subqueries.
//!
//! This module handles the analysis of query expressions including SELECT projections,
//! FROM clauses, JOINs, WHERE/HAVING filters, and wildcard expansion. It builds the
//! column-level lineage graph by tracking data flow from source columns to output columns.

use super::context::{ColumnRef, OutputColumn, StatementContext};
use super::helpers::{generate_column_node_id, generate_edge_id};
use super::visitor::{LineageVisitor, Visitor};
use super::Analyzer;
use crate::types::{
    issue_codes, AggregationInfo, Edge, EdgeType, Issue, JoinType, Node, NodeType, ResolutionSource,
};
use serde_json::json;
use sqlparser::ast::{self, Query, SetExpr};
use std::collections::HashMap;

/// Represents the information needed to add an expanded column during wildcard expansion.
struct ExpandedColumnInfo {
    name: String,
    table_canonical: String,
    resolved_table_canonical: String,
    data_type: Option<String>,
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

    pub(super) fn add_source_table(
        &mut self,
        ctx: &mut StatementContext,
        table_name: &str,
        target_node: Option<&str>,
    ) -> Option<String> {
        let canonical_for_alias: Option<String>;

        // Check if this is a CTE reference
        let node_id = if ctx.cte_definitions.contains_key(table_name) {
            canonical_for_alias = Some(table_name.to_string());
            let cte_id = ctx.cte_definitions.get(table_name).cloned();
            if let Some(ref id) = cte_id {
                // Register CTE in current scope for resolution
                ctx.register_table_in_scope(table_name.to_string(), id.clone());
            }
            cte_id
        } else {
            // Regular table or view
            let resolution = self.canonicalize_table_reference(table_name);
            let canonical = resolution.canonical.clone();
            canonical_for_alias = Some(canonical.clone());

            // Use relation_identity to get correct node ID and type (view vs table)
            let (id, node_type) = self.relation_identity(&canonical);

            let exists_in_schema = resolution.matched_schema;
            let produced = self.produced_tables.contains_key(&canonical);
            let is_known = exists_in_schema || produced || self.known_tables.is_empty();

            // Determine resolution source based on schema entry
            let resolution_source = if let Some(entry) = self.schema_tables.get(&canonical) {
                match entry.origin {
                    crate::types::SchemaOrigin::Imported => Some(ResolutionSource::Imported),
                    crate::types::SchemaOrigin::Implied => Some(ResolutionSource::Implied),
                }
            } else if !is_known {
                Some(ResolutionSource::Unknown)
            } else {
                None
            };

            // Check if already added
            if !ctx.node_ids.contains(&id) {
                let mut metadata = None;
                if !is_known {
                    let mut meta = HashMap::new();
                    meta.insert("placeholder".to_string(), json!(true));
                    metadata = Some(meta);
                    self.issues.push(
                        Issue::warning(
                            issue_codes::UNRESOLVED_REFERENCE,
                            format!("Table '{canonical}' could not be resolved using provided schema metadata or search path"),
                        )
                        .with_statement(ctx.statement_index),
                    );
                }

                // Get join type directly from context (already converted from AST)
                let join_type = ctx.current_join_info.join_type;
                let join_condition = ctx
                    .current_join_info
                    .join_condition
                    .as_deref()
                    .map(Into::into);

                ctx.add_node(Node {
                    id: id.clone(),
                    node_type,
                    label: crate::analyzer::helpers::extract_simple_name(&canonical).into(),
                    qualified_name: Some(canonical.clone().into()),
                    expression: None,
                    span: None,
                    metadata,
                    resolution_source,
                    filters: Vec::new(),
                    join_type,
                    join_condition,
                    aggregation: None,
                });
            }

            self.all_relations.insert(canonical.clone());
            self.consumed_tables
                .entry(canonical.clone())
                .or_default()
                .push(ctx.statement_index);

            // Track table node ID for column ownership and register in current scope
            ctx.register_table_in_scope(canonical, id.clone());

            Some(id)
        };

        // Create edge to target if specified
        if let (Some(target), Some(source_id)) = (target_node, node_id.clone()) {
            let edge_id = generate_edge_id(&source_id, target);
            if !ctx.edge_ids.contains(&edge_id) {
                ctx.add_edge(Edge {
                    id: edge_id,
                    from: source_id,
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
        }

        canonical_for_alias
    }

    pub(super) fn add_table_columns_from_schema(
        &mut self,
        ctx: &mut StatementContext,
        table_canonical: &str,
        table_node_id: &str,
    ) {
        if let Some(schema_entry) = self.schema_tables.get(table_canonical) {
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
            // Expand all tables in scope
            ctx.table_node_ids.keys().cloned().collect()
        };

        for table_canonical in tables_to_expand {
            // First collect column info to avoid borrow conflict
            let columns_to_add: Option<Vec<ExpandedColumnInfo>> = self
                .schema_tables
                .get(&table_canonical)
                .map(|schema_entry| {
                    schema_entry
                        .table
                        .columns
                        .iter()
                        .map(|col| ExpandedColumnInfo {
                            name: col.name.clone(),
                            table_canonical: table_canonical.clone(),
                            resolved_table_canonical: table_canonical.clone(),
                            data_type: col.data_type.clone(),
                        })
                        .collect()
                });

            if let Some(columns) = columns_to_add {
                // Expand from schema - NOT approximate
                for col_info in columns {
                    let sources = vec![ColumnRef {
                        table: Some(col_info.table_canonical),
                        column: col_info.name.clone(),
                        resolved_table: Some(col_info.resolved_table_canonical),
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
                self.issues.push(
                    Issue::info(
                        issue_codes::APPROXIMATE_LINEAGE,
                        format!("SELECT * from '{table_canonical}' - column list unknown without schema metadata"),
                    )
                    .with_statement(ctx.statement_index),
                );

                // If there's a target node, create an approximate edge from source table to target
                if let Some(target) = target_node {
                    if let Some(source_node_id) = ctx.table_node_ids.get(&table_canonical) {
                        let edge_id = generate_edge_id(source_node_id, target);
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

        // If no tables in current scope, fall back to global (shouldn't happen normally)
        let tables_in_scope = if tables_in_scope.is_empty() {
            ctx.table_node_ids.keys().cloned().collect::<Vec<_>>()
        } else {
            tables_in_scope
        };

        // If only one table in scope, assume column belongs to it
        if tables_in_scope.len() == 1 {
            return Some(tables_in_scope[0].clone());
        }

        let normalized_col = self.normalize_identifier(column);

        // Collect candidates using CTE output columns and schema metadata
        // Only consider tables that are actually in the current scope
        let mut candidate_tables: Vec<String> = Vec::new();
        for table_canonical in &tables_in_scope {
            // Check CTE columns
            if let Some(cte_cols) = ctx.cte_columns.get(table_canonical) {
                if cte_cols.iter().any(|c| c.name == normalized_col) {
                    candidate_tables.push(table_canonical.clone());
                    continue;
                }
            }

            // Check schema metadata
            if let Some(schema_entry) = self.schema_tables.get(table_canonical) {
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
                self.issues.push(
                    Issue::warning(
                        issue_codes::UNRESOLVED_REFERENCE,
                        format!(
                            "Column '{}' is ambiguous across tables in scope: {}",
                            column,
                            tables_in_scope.join(", ")
                        ),
                    )
                    .with_statement(ctx.statement_index),
                );
                None
            }
            _ => {
                // Column exists in multiple tables in scope — require explicit qualifier.
                self.issues.push(
                    Issue::warning(
                        issue_codes::UNRESOLVED_REFERENCE,
                        format!(
                            "Column '{}' exists in multiple tables in scope: {}. Qualify the column to disambiguate.",
                            column,
                            candidate_tables.join(", ")
                        ),
                    )
                    .with_statement(ctx.statement_index),
                );
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

                // Try to find existing node ID if it's a known CTE
                if let Some(cte_cols) = ctx.cte_columns.get(table_canonical) {
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
            sources: params.sources,
            expression: params.expression,
            data_type: params.data_type,
            node_id,
        });
    }

    /// Convert an AST JoinOperator to JoinType enum, also extracting the join condition.
    pub(super) fn convert_join_operator(
        op: &ast::JoinOperator,
    ) -> (Option<JoinType>, Option<String>) {
        match op {
            ast::JoinOperator::Inner(constraint) => (
                Some(JoinType::Inner),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::LeftOuter(constraint) => (
                Some(JoinType::Left),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::RightOuter(constraint) => (
                Some(JoinType::Right),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::FullOuter(constraint) => (
                Some(JoinType::Full),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::CrossJoin => (Some(JoinType::Cross), None),
            ast::JoinOperator::LeftSemi(constraint) => (
                Some(JoinType::LeftSemi),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::RightSemi(constraint) => (
                Some(JoinType::RightSemi),
                Self::extract_join_condition(constraint),
            ),
            ast::JoinOperator::LeftAnti(constraint) => (
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
}
