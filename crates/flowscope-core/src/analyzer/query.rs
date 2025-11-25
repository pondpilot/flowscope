//! Query analysis for SELECT statements, CTEs, and subqueries.
//!
//! This module handles the analysis of query expressions including SELECT projections,
//! FROM clauses, JOINs, WHERE/HAVING filters, and wildcard expansion. It builds the
//! column-level lineage graph by tracking data flow from source columns to output columns.

use super::context::{ColumnRef, OutputColumn, StatementContext};
use super::functions;
use super::helpers::{
    generate_column_node_id, generate_edge_id, generate_node_id, infer_expr_type,
    is_simple_column_ref,
};
use super::Analyzer;
use crate::types::{
    issue_codes, AggregationInfo, Edge, EdgeType, FilterClauseType, Issue, JoinType, Node,
    NodeType, ResolutionSource,
};
use serde_json::json;
use sqlparser::ast::{
    self, Expr, FunctionArg, FunctionArgExpr, Query, SelectItem, SetExpr, Statement, TableFactor,
    TableWithJoins,
};
use std::collections::{HashMap, HashSet};

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
        // First analyze CTEs
        let empty_vec = vec![];
        let ctes = query
            .with
            .as_ref()
            .map(|w| &w.cte_tables)
            .unwrap_or(&empty_vec);

        // Pass 1: register all CTE names/nodes up front to allow forward and mutual references.
        let mut cte_ids: Vec<(String, String)> = Vec::new();
        for cte in ctes {
            let cte_name = cte.alias.name.to_string();

            // Create CTE node
            let cte_id = ctx.add_node(Node {
                id: generate_node_id("cte", &cte_name),
                node_type: NodeType::Cte,
                label: cte_name.clone(),
                qualified_name: Some(cte_name.clone()),
                expression: None,
                span: None,
                metadata: None,
                resolution_source: None,
                filters: Vec::new(),
                join_type: None,
                join_condition: None,
                aggregation: None,
            });

            // Register CTE for resolution
            ctx.cte_definitions.insert(cte_name.clone(), cte_id.clone());
            self.all_ctes.insert(cte_name.clone());
            cte_ids.push((cte_name, cte_id));
        }

        // Pass 2: analyze each CTE body now that all aliases are in scope.
        for (cte, (_, cte_id)) in ctes.iter().zip(cte_ids.iter()) {
            self.analyze_query_body(ctx, &cte.query.body, Some(cte_id));

            // Capture CTE columns for lineage linking
            let columns = std::mem::take(&mut ctx.output_columns);
            ctx.cte_columns.insert(cte.alias.name.to_string(), columns);
        }

        // Analyze main query body
        self.analyze_query_body(ctx, &query.body, target_node);
    }

    pub(super) fn analyze_query_body(
        &mut self,
        ctx: &mut StatementContext,
        body: &SetExpr,
        target_node: Option<&str>,
    ) {
        match body {
            SetExpr::Select(select) => {
                self.analyze_select(ctx, select, target_node);
            }
            SetExpr::Query(query) => {
                self.analyze_query(ctx, query, target_node);
            }
            SetExpr::SetOperation {
                op, left, right, ..
            } => {
                let op_name = match op {
                    ast::SetOperator::Union => "UNION",
                    ast::SetOperator::Intersect => "INTERSECT",
                    ast::SetOperator::Except => "EXCEPT",
                };

                // Analyze both branches
                self.analyze_query_body(ctx, left, target_node);
                self.analyze_query_body(ctx, right, target_node);

                // Track operation for edges
                if target_node.is_some() {
                    ctx.last_operation = Some(op_name.to_string());
                }
            }
            SetExpr::Values(values) => {
                // Analyze expressions in VALUES clause
                for row in &values.rows {
                    for expr in row {
                        self.analyze_expression(ctx, expr);
                    }
                }
            }
            SetExpr::Insert(insert_stmt) => {
                // Nested INSERT statement - analyze it
                if let Statement::Insert(ref insert) = *insert_stmt {
                    self.analyze_insert_stmt(ctx, insert, target_node);
                }
            }
            SetExpr::Update(_) => {
                // Update statement
            }
            SetExpr::Table(tbl) => {
                // TABLE statement - just references a table
                let name = tbl
                    .table_name
                    .as_ref()
                    .map(|n| n.to_string())
                    .unwrap_or_default();
                if !name.is_empty() {
                    self.add_source_table(ctx, &name, target_node);
                }
            }
        }
    }

    fn analyze_select(
        &mut self,
        ctx: &mut StatementContext,
        select: &ast::Select,
        target_node: Option<&str>,
    ) {
        // Push a new scope for this SELECT - isolates table resolution
        ctx.push_scope();

        // Analyze FROM clause first to register tables and aliases in the current scope
        for table_with_joins in &select.from {
            self.analyze_table_with_joins(ctx, table_with_joins, target_node);
        }

        // Analyze columns if column lineage is enabled
        if self.column_lineage_enabled {
            self.analyze_select_columns(ctx, select, target_node);
        }

        // Pop the scope when done with this SELECT
        ctx.pop_scope();
    }

    fn analyze_select_columns(
        &mut self,
        ctx: &mut StatementContext,
        select: &ast::Select,
        target_node: Option<&str>,
    ) {
        // Clear any previous grouping context
        ctx.clear_grouping();

        // First, capture GROUP BY columns so we can identify grouping keys vs aggregates
        match &select.group_by {
            ast::GroupByExpr::Expressions(exprs, _) => {
                for group_by in exprs {
                    // Normalize the expression string for comparison
                    let expr_str = self.normalize_group_by_expr(group_by);
                    ctx.add_grouping_column(expr_str);
                    self.analyze_expression(ctx, group_by);
                }
            }
            ast::GroupByExpr::All(_) => {
                // GROUP BY ALL - all non-aggregate columns are grouping keys
                // We can't determine this statically, so we mark has_group_by
                ctx.has_group_by = true;
            }
        }

        // Process SELECT projection with aggregation detection
        for (idx, item) in select.projection.iter().enumerate() {
            match item {
                SelectItem::UnnamedExpr(expr) => {
                    let sources = self.extract_column_refs(expr);
                    let name = self.derive_column_name(expr, idx);
                    let expr_text = if is_simple_column_ref(expr) {
                        None
                    } else {
                        Some(expr.to_string())
                    };
                    let aggregation = self.detect_aggregation(ctx, expr);
                    let data_type = infer_expr_type(expr).map(|t| t.to_string());
                    self.add_output_column_with_aggregation(
                        ctx,
                        OutputColumnParams {
                            name: name.clone(),
                            sources,
                            expression: expr_text,
                            data_type,
                            target_node: target_node.map(|s| s.to_string()),
                            approximate: false,
                            aggregation,
                        },
                    );
                }
                SelectItem::ExprWithAlias { expr, alias } => {
                    let sources = self.extract_column_refs(expr);
                    let name = alias.value.clone();
                    let expr_text = if is_simple_column_ref(expr) {
                        None
                    } else {
                        Some(expr.to_string())
                    };
                    let aggregation = self.detect_aggregation(ctx, expr);
                    let data_type = infer_expr_type(expr).map(|t| t.to_string());
                    self.add_output_column_with_aggregation(
                        ctx,
                        OutputColumnParams {
                            name: name.clone(),
                            sources,
                            expression: expr_text,
                            data_type,
                            target_node: target_node.map(|s| s.to_string()),
                            approximate: false,
                            aggregation,
                        },
                    );
                }
                SelectItem::QualifiedWildcard(name, _) => {
                    // table.*
                    let table_name = name.to_string();
                    self.expand_wildcard(ctx, Some(&table_name), target_node);
                }
                SelectItem::Wildcard(_) => {
                    // SELECT *
                    self.expand_wildcard(ctx, None, target_node);
                }
            }
        }

        // Also extract column refs from WHERE for completeness
        if let Some(ref where_clause) = select.selection {
            self.analyze_expression(ctx, where_clause);
            // Capture WHERE predicates for table nodes
            self.capture_filter_predicates(ctx, where_clause, FilterClauseType::Where);
        }

        if let Some(ref having) = select.having {
            self.analyze_expression(ctx, having);
            // Capture HAVING predicates for table nodes
            self.capture_filter_predicates(ctx, having, FilterClauseType::Having);
        }
    }

    pub(super) fn analyze_expression(&mut self, ctx: &mut StatementContext, expr: &Expr) {
        // 1. Traverse for subqueries
        self.visit_expression_for_subqueries(ctx, expr);
        // 2. Validate columns
        self.extract_column_refs_for_validation(ctx, expr);
    }

    fn visit_expression_for_subqueries(&mut self, ctx: &mut StatementContext, expr: &Expr) {
        match expr {
            Expr::Subquery(query) => self.analyze_query(ctx, query, None),
            Expr::InSubquery { subquery, .. } => self.analyze_query(ctx, subquery, None),
            Expr::Exists { subquery, .. } => self.analyze_query(ctx, subquery, None),
            Expr::BinaryOp { left, right, .. } => {
                self.visit_expression_for_subqueries(ctx, left);
                self.visit_expression_for_subqueries(ctx, right);
            }
            Expr::UnaryOp { expr, .. } => self.visit_expression_for_subqueries(ctx, expr),
            Expr::Nested(expr) => self.visit_expression_for_subqueries(ctx, expr),
            Expr::Case {
                operand,
                conditions,
                results,
                else_result,
            } => {
                if let Some(op) = operand {
                    self.visit_expression_for_subqueries(ctx, op);
                }
                for cond in conditions {
                    self.visit_expression_for_subqueries(ctx, cond);
                }
                for res in results {
                    self.visit_expression_for_subqueries(ctx, res);
                }
                if let Some(el) = else_result {
                    self.visit_expression_for_subqueries(ctx, el);
                }
            }
            Expr::Function(func) => {
                if let ast::FunctionArguments::List(args) = &func.args {
                    for arg in &args.args {
                        match arg {
                            FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
                            | FunctionArg::Named {
                                arg: FunctionArgExpr::Expr(e),
                                ..
                            } => self.visit_expression_for_subqueries(ctx, e),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn analyze_insert_stmt(
        &mut self,
        ctx: &mut StatementContext,
        insert: &ast::Insert,
        _target_node: Option<&str>,
    ) {
        // For nested INSERT in SetExpr, analyze similarly
        let target_name = insert.table_name.to_string();
        self.add_source_table(ctx, &target_name, _target_node);
    }

    pub(super) fn analyze_table_with_joins(
        &mut self,
        ctx: &mut StatementContext,
        table_with_joins: &TableWithJoins,
        target_node: Option<&str>,
    ) {
        // Analyze main relation
        self.analyze_table_factor(ctx, &table_with_joins.relation, target_node);

        // Analyze joins - process each joined table but don't create table-to-table edges
        // Join edges don't represent data flow; the column-level edges already show lineage
        for join in &table_with_joins.joins {
            // Convert join operator directly to JoinType enum and extract condition
            let (join_type, join_condition) = Self::convert_join_operator(&join.join_operator);

            ctx.current_join_info.join_type = join_type;
            ctx.current_join_info.join_condition = join_condition;
            // Keep last_operation for backward compatibility with edge labels
            ctx.last_operation = Self::join_type_to_operation(join_type);

            // Analyze the joined table
            self.analyze_table_factor(ctx, &join.relation, target_node);

            // Clear join info after processing
            ctx.current_join_info.join_type = None;
            ctx.current_join_info.join_condition = None;
        }
    }

    pub(super) fn analyze_table_factor(
        &mut self,
        ctx: &mut StatementContext,
        table_factor: &TableFactor,
        target_node: Option<&str>,
    ) {
        match table_factor {
            TableFactor::Table { name, alias, .. } => {
                let table_name = name.to_string();

                let canonical = self.add_source_table(ctx, &table_name, target_node);

                // Register alias if present (in current scope)
                if let (Some(a), Some(canonical_name)) = (alias, canonical) {
                    ctx.register_alias_in_scope(a.name.to_string(), canonical_name);
                }
            }
            TableFactor::Derived {
                subquery, alias, ..
            } => {
                // Subquery - analyze recursively
                self.analyze_query(ctx, subquery, target_node);

                if let Some(a) = alias {
                    // Register subquery alias (in current scope)
                    ctx.register_subquery_alias_in_scope(a.name.to_string());
                }
            }
            TableFactor::NestedJoin {
                table_with_joins, ..
            } => {
                self.analyze_table_with_joins(ctx, table_with_joins, target_node);
            }
            TableFactor::TableFunction { .. } => {
                self.issues.push(
                    Issue::info(
                        issue_codes::UNSUPPORTED_SYNTAX,
                        "Table function lineage not fully tracked",
                    )
                    .with_statement(ctx.statement_index),
                );
            }
            TableFactor::Function { .. } => {
                // Table-valued function
            }
            TableFactor::UNNEST { .. } => {
                // UNNEST clause
            }
            TableFactor::Pivot { .. } | TableFactor::Unpivot { .. } => {
                self.issues.push(
                    Issue::warning(
                        issue_codes::UNSUPPORTED_SYNTAX,
                        "PIVOT/UNPIVOT lineage not fully supported",
                    )
                    .with_statement(ctx.statement_index),
                );
            }
            TableFactor::MatchRecognize { .. } => {}
            TableFactor::JsonTable { .. } => {}
        }
    }

    pub(super) fn register_aliases_in_table_with_joins(
        &self,
        ctx: &mut StatementContext,
        table_with_joins: &TableWithJoins,
    ) {
        self.register_aliases_in_table_factor(ctx, &table_with_joins.relation);
        for join in &table_with_joins.joins {
            self.register_aliases_in_table_factor(ctx, &join.relation);
        }
    }

    pub(super) fn register_aliases_in_table_factor(
        &self,
        ctx: &mut StatementContext,
        table_factor: &TableFactor,
    ) {
        match table_factor {
            TableFactor::Table {
                name,
                alias: Some(a),
                ..
            } => {
                let canonical = self
                    .canonicalize_table_reference(&name.to_string())
                    .canonical;
                ctx.table_aliases.insert(a.name.to_string(), canonical);
            }
            TableFactor::Derived { alias: Some(a), .. } => {
                ctx.subquery_aliases.insert(a.name.to_string());
            }
            TableFactor::NestedJoin {
                table_with_joins, ..
            } => {
                self.register_aliases_in_table_with_joins(ctx, table_with_joins);
            }
            _ => {}
        }
    }

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
            // Regular table
            let resolution = self.canonicalize_table_reference(table_name);
            let canonical = resolution.canonical.clone();
            canonical_for_alias = Some(canonical.clone());
            let id = generate_node_id("table", &canonical);

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
                let join_condition = ctx.current_join_info.join_condition.clone();

                ctx.add_node(Node {
                    id: id.clone(),
                    node_type: NodeType::Table,
                    label: crate::analyzer::helpers::extract_simple_name(&canonical),
                    qualified_name: Some(canonical.clone()),
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

            self.all_tables.insert(canonical.clone());
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
            // Avoid self-loops (source == target) unless explicitly desired?
            // Usually in UPDATE t SET ... FROM t, we don't want a loop unless needed.
            // But for lineage, showing the table depends on itself is accurate for UPDATE/MERGE.
            let edge_id = generate_edge_id(&source_id, target);
            if !ctx.edge_ids.contains(&edge_id) {
                ctx.add_edge(Edge {
                    id: edge_id,
                    from: source_id,
                    to: target.to_string(),
                    edge_type: EdgeType::DataFlow,
                    expression: None,
                    operation: ctx.last_operation.clone(),
                    join_type: ctx.current_join_info.join_type,
                    join_condition: ctx.current_join_info.join_condition.clone(),
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
                    label: col.name.clone(),
                    qualified_name: Some(format!("{}.{}", table_canonical, col.name)),
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
                        from: table_node_id.to_string(),
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

    fn expand_wildcard(
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
                                to: target.to_string(),
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

    fn resolve_column_table(
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

    fn normalize_group_by_expr(&self, expr: &Expr) -> String {
        match expr {
            Expr::Identifier(ident) => self.normalize_identifier(&ident.value),
            Expr::CompoundIdentifier(parts) => {
                // Use the full qualified name
                parts
                    .iter()
                    .map(|p| self.normalize_identifier(&p.value))
                    .collect::<Vec<_>>()
                    .join(".")
            }
            Expr::Nested(inner) => {
                // Unwrap parentheses for matching: GROUP BY (col) should match SELECT col
                self.normalize_group_by_expr(inner)
            }
            _ => {
                // For complex expressions, use the string representation
                expr.to_string().to_lowercase()
            }
        }
    }

    fn detect_aggregation(&self, ctx: &StatementContext, expr: &Expr) -> Option<AggregationInfo> {
        if ctx.has_group_by {
            // Check if this expression is a grouping key
            let expr_normalized = self.normalize_group_by_expr(expr);
            if ctx.is_grouping_column(&expr_normalized) {
                return Some(AggregationInfo {
                    is_grouping_key: true,
                    function: None,
                    distinct: None,
                });
            }
        }

        // Check if the expression contains an aggregate function
        if let Some(agg_call) = self.find_aggregate_function(expr) {
            return Some(AggregationInfo {
                is_grouping_key: false,
                function: Some(agg_call.function),
                distinct: if agg_call.distinct { Some(true) } else { None },
            });
        }

        // Expression in a GROUP BY query but neither grouping key nor aggregate
        // This could be a constant or an error in the query - we don't flag it
        None
    }

    fn find_aggregate_function(&self, expr: &Expr) -> Option<functions::AggregateCall> {
        match expr {
            Expr::Function(func) => self.check_function_for_aggregate(func),
            Expr::BinaryOp { left, right, .. } => self
                .find_aggregate_function(left)
                .or_else(|| self.find_aggregate_function(right)),
            Expr::UnaryOp { expr, .. } | Expr::Nested(expr) | Expr::Cast { expr, .. } => {
                self.find_aggregate_function(expr)
            }
            Expr::Case {
                operand,
                conditions,
                results,
                else_result,
            } => self.find_aggregate_in_case(operand, conditions, results, else_result),
            _ => None,
        }
    }

    fn check_function_for_aggregate(
        &self,
        func: &ast::Function,
    ) -> Option<functions::AggregateCall> {
        let func_name = func.name.to_string();

        if functions::is_aggregate_function(&func_name) {
            let distinct = matches!(
                &func.args,
                ast::FunctionArguments::List(args) if args.duplicate_treatment == Some(ast::DuplicateTreatment::Distinct)
            );
            return Some(functions::AggregateCall {
                function: func_name.to_uppercase(),
                distinct,
            });
        }

        // Not an aggregate itself, check arguments for nested aggregates
        self.find_aggregate_in_function_args(&func.args)
    }

    fn find_aggregate_in_function_args(
        &self,
        args: &ast::FunctionArguments,
    ) -> Option<functions::AggregateCall> {
        if let ast::FunctionArguments::List(arg_list) = args {
            for arg in &arg_list.args {
                let expr = match arg {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => Some(e),
                    FunctionArg::Named {
                        arg: FunctionArgExpr::Expr(e),
                        ..
                    } => Some(e),
                    _ => None,
                };
                if let Some(e) = expr {
                    if let Some(agg) = self.find_aggregate_function(e) {
                        return Some(agg);
                    }
                }
            }
        }
        None
    }

    fn find_aggregate_in_case(
        &self,
        operand: &Option<Box<Expr>>,
        conditions: &[Expr],
        results: &[Expr],
        else_result: &Option<Box<Expr>>,
    ) -> Option<functions::AggregateCall> {
        // Check operand (for CASE expr WHEN ...)
        if let Some(op) = operand {
            if let Some(agg) = self.find_aggregate_function(op) {
                return Some(agg);
            }
        }

        // Check WHEN conditions
        for cond in conditions {
            if let Some(agg) = self.find_aggregate_function(cond) {
                return Some(agg);
            }
        }

        // Check THEN results
        for result in results {
            if let Some(agg) = self.find_aggregate_function(result) {
                return Some(agg);
            }
        }

        // Check ELSE result
        if let Some(else_r) = else_result {
            if let Some(agg) = self.find_aggregate_function(else_r) {
                return Some(agg);
            }
        }

        None
    }

    pub(super) fn extract_column_refs(&self, expr: &Expr) -> Vec<ColumnRef> {
        let mut refs = Vec::new();
        Self::collect_column_refs(expr, &mut refs);
        refs
    }

    fn collect_column_refs(expr: &Expr, refs: &mut Vec<ColumnRef>) {
        match expr {
            Expr::Identifier(ident) => {
                refs.push(ColumnRef {
                    table: None,
                    column: ident.value.clone(),
                    resolved_table: None,
                });
            }
            Expr::CompoundIdentifier(parts) => {
                if parts.len() >= 2 {
                    let table = parts[..parts.len() - 1]
                        .iter()
                        .map(|i| i.value.as_str())
                        .collect::<Vec<_>>()
                        .join(".");
                    let column = parts.last().unwrap().value.clone();
                    refs.push(ColumnRef {
                        table: Some(table),
                        column,
                        resolved_table: None,
                    });
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::collect_column_refs(left, refs);
                Self::collect_column_refs(right, refs);
            }
            Expr::UnaryOp { expr, .. } => {
                Self::collect_column_refs(expr, refs);
            }
            Expr::Function(func) => {
                let func_name = func.name.to_string();
                match &func.args {
                    ast::FunctionArguments::List(arg_list) => {
                        for (idx, arg) in arg_list.args.iter().enumerate() {
                            // Check if this argument should be skipped (e.g., date unit keywords)
                            if functions::should_skip_function_arg(&func_name, idx) {
                                continue;
                            }
                            match arg {
                                FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => {
                                    Self::collect_column_refs(e, refs);
                                }
                                FunctionArg::Named {
                                    arg: FunctionArgExpr::Expr(e),
                                    ..
                                } => {
                                    Self::collect_column_refs(e, refs);
                                }
                                _ => {}
                            }
                        }
                    }
                    ast::FunctionArguments::Subquery(_) => {}
                    ast::FunctionArguments::None => {}
                }
            }
            Expr::Case {
                operand,
                conditions,
                results,
                else_result,
            } => {
                if let Some(op) = operand {
                    Self::collect_column_refs(op, refs);
                }
                for cond in conditions {
                    Self::collect_column_refs(cond, refs);
                }
                for res in results {
                    Self::collect_column_refs(res, refs);
                }
                if let Some(el) = else_result {
                    Self::collect_column_refs(el, refs);
                }
            }
            Expr::Cast { expr, .. } => {
                Self::collect_column_refs(expr, refs);
            }
            Expr::Nested(inner) => {
                Self::collect_column_refs(inner, refs);
            }
            Expr::Subquery(_) => {
                // Subquery columns are handled separately
            }
            Expr::InList { expr, list, .. } => {
                Self::collect_column_refs(expr, refs);
                for item in list {
                    Self::collect_column_refs(item, refs);
                }
            }
            Expr::Between {
                expr, low, high, ..
            } => {
                Self::collect_column_refs(expr, refs);
                Self::collect_column_refs(low, refs);
                Self::collect_column_refs(high, refs);
            }
            Expr::IsNull(e) | Expr::IsNotNull(e) => {
                Self::collect_column_refs(e, refs);
            }
            Expr::IsFalse(e) | Expr::IsNotFalse(e) | Expr::IsTrue(e) | Expr::IsNotTrue(e) => {
                Self::collect_column_refs(e, refs);
            }
            Expr::Like { expr, pattern, .. } | Expr::ILike { expr, pattern, .. } => {
                Self::collect_column_refs(expr, refs);
                Self::collect_column_refs(pattern, refs);
            }
            Expr::Tuple(exprs) => {
                for e in exprs {
                    Self::collect_column_refs(e, refs);
                }
            }
            Expr::Extract { expr, .. } => {
                Self::collect_column_refs(expr, refs);
            }
            _ => {
                // Other expressions don't contain column references or are handled elsewhere
            }
        }
    }

    fn derive_column_name(&self, expr: &Expr, index: usize) -> String {
        match expr {
            Expr::Identifier(ident) => ident.value.clone(),
            Expr::CompoundIdentifier(parts) => parts
                .last()
                .map(|i| i.value.clone())
                .unwrap_or_else(|| format!("col_{index}")),
            Expr::Function(func) => func.name.to_string().to_lowercase(),
            _ => format!("col_{index}"),
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
    ///
    /// This function is central to building the column-level lineage graph.
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
            label: normalized_name.clone(),
            qualified_name: None, // Will be set if we have target table
            expression: params.expression.clone(),
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

        // Create ownership edge if we have a target (table/CTE being written to)
        if let Some(target) = params.target_node {
            let edge_id = generate_edge_id(&target, &node_id);
            if !ctx.edge_ids.contains(&edge_id) {
                ctx.add_edge(Edge {
                    id: edge_id,
                    from: target.to_string(),
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
                    .unwrap_or_else(|| generate_node_id("table", table_canonical));

                // Fallback to generating a new ID (standard table/CTE column)
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
                    label: source.column.clone(),
                    qualified_name: Some(format!("{}.{}", table_canonical, source.column)),
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
                        expression: params.expression.clone(),
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

    /// Capture filter predicates from a WHERE/HAVING expression and attach to table nodes.
    /// This splits the expression by AND to localize predicates to specific tables,
    /// so each table only shows the filters that directly reference it.
    pub(super) fn capture_filter_predicates(
        &mut self,
        ctx: &mut StatementContext,
        expr: &Expr,
        clause_type: FilterClauseType,
    ) {
        // Split by AND and process each predicate separately
        let predicates = Self::split_by_and(expr);

        for predicate in predicates {
            // Extract column references from this specific predicate
            let column_refs = self.extract_column_refs(predicate);

            // Find unique tables referenced in this predicate
            let mut affected_tables: HashSet<String> = HashSet::new();
            for col_ref in &column_refs {
                if let Some(table_canonical) =
                    self.resolve_column_table(ctx, col_ref.table.as_deref(), &col_ref.column)
                {
                    affected_tables.insert(table_canonical);
                }
            }

            // If we couldn't resolve columns to specific tables (e.g., columns from
            // functions without clear table references, or ambiguous column names),
            // apply the filter to all tables in the current scope as a conservative
            // fallback. This may be imprecise for complex multi-table expressions,
            // but ensures the filter is captured rather than lost.
            if affected_tables.is_empty() && !column_refs.is_empty() {
                for table in ctx.tables_in_current_scope() {
                    affected_tables.insert(table);
                }
            }

            // Add this specific predicate to affected table nodes
            let filter_text = predicate.to_string();
            for table_canonical in &affected_tables {
                ctx.add_filter_for_table(table_canonical, filter_text.clone(), clause_type);
            }
        }
    }

    /// Split an expression by top-level AND operator into individual predicates.
    /// For example: `a = 1 AND b = 2 AND c = 3` becomes [`a = 1`, `b = 2`, `c = 3`]
    fn split_by_and(expr: &Expr) -> Vec<&Expr> {
        let mut predicates = Vec::new();
        Self::collect_and_predicates(expr, &mut predicates);
        predicates
    }

    fn collect_and_predicates<'b>(expr: &'b Expr, predicates: &mut Vec<&'b Expr>) {
        match expr {
            Expr::BinaryOp {
                left,
                op: ast::BinaryOperator::And,
                right,
            } => {
                Self::collect_and_predicates(left, predicates);
                Self::collect_and_predicates(right, predicates);
            }
            _ => {
                predicates.push(expr);
            }
        }
    }

    /// Apply pending filters to table nodes before finalizing the statement.
    /// This should be called after all analysis is complete for a statement.
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
