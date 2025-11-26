//! Visitor pattern for AST traversal and lineage analysis.
//!
//! This module provides a visitor-based approach to traversing SQL AST nodes
//! and building lineage graphs. It separates traversal logic (the `Visitor` trait)
//! from analysis logic (the `LineageVisitor` implementation).
//!
//! # Design
//!
//! The module uses two naming conventions for methods:
//!
//! - **`visit_*` methods**: Implement the `Visitor` trait for AST traversal.
//!   These handle recursive descent through the AST structure.
//!
//! - **`analyze_*` methods**: Perform specific analysis tasks like identifying
//!   DML targets or registering aliases. These don't follow the visitor pattern
//!   but use the visitor's context to build the lineage graph.

use super::context::StatementContext;
use super::expression::ExpressionAnalyzer;
use super::helpers::{generate_node_id, infer_expr_type, is_simple_column_ref};
use super::query::OutputColumnParams;
use super::Analyzer;
use crate::types::{issue_codes, FilterClauseType, Issue, Node, NodeType};
use sqlparser::ast::{
    self, Cte, Expr, Join, Query, Select, SelectItem, SetExpr, SetOperator, Statement, TableAlias,
    TableFactor, TableWithJoins, Values,
};
use std::collections::HashSet;
use std::sync::Arc;

/// A visitor trait for traversing the SQL AST.
///
/// This trait defines default behavior for visiting nodes (traversing children).
/// Implementors can override specific methods to add custom logic.
pub trait Visitor {
    fn visit_statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Query(query) => self.visit_query(query),
            Statement::Insert(insert) => {
                if let Some(source) = &insert.source {
                    self.visit_query(source);
                }
            }
            Statement::CreateTable(create) => {
                if let Some(query) = &create.query {
                    self.visit_query(query);
                }
            }
            Statement::CreateView { query, .. } => self.visit_query(query),
            _ => {}
        }
    }

    fn visit_query(&mut self, query: &Query) {
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.visit_cte(cte);
            }
        }
        self.visit_set_expr(&query.body);
    }

    fn visit_cte(&mut self, cte: &Cte) {
        self.visit_query(&cte.query);
    }

    fn visit_set_expr(&mut self, set_expr: &SetExpr) {
        match set_expr {
            SetExpr::Select(select) => self.visit_select(select),
            SetExpr::Query(query) => self.visit_query(query),
            SetExpr::SetOperation { left, right, .. } => {
                self.visit_set_expr(left);
                self.visit_set_expr(right);
            }
            SetExpr::Values(values) => self.visit_values(values),
            SetExpr::Insert(stmt) => self.visit_statement(stmt),
            _ => {}
        }
    }

    fn visit_select(&mut self, select: &Select) {
        for from in &select.from {
            self.visit_table_with_joins(from);
        }
        // Default doesn't handle projection/selection as it's analysis specific
    }

    fn visit_table_with_joins(&mut self, table: &TableWithJoins) {
        self.visit_table_factor(&table.relation);
        for join in &table.joins {
            self.visit_join(join);
        }
    }

    fn visit_table_factor(&mut self, table: &TableFactor) {
        match table {
            TableFactor::Derived { subquery, .. } => self.visit_query(subquery),
            TableFactor::NestedJoin {
                table_with_joins, ..
            } => self.visit_table_with_joins(table_with_joins),
            _ => {}
        }
    }

    fn visit_join(&mut self, join: &Join) {
        self.visit_table_factor(&join.relation);
    }

    fn visit_values(&mut self, values: &Values) {
        for row in &values.rows {
            for expr in row {
                self.visit_expr(expr);
            }
        }
    }

    fn visit_expr(&mut self, _expr: &Expr) {}
}

/// Visitor implementation that builds the lineage graph.
///
/// `LineageVisitor` traverses SQL AST nodes and records table relationships,
/// column lineage, and data flow edges. It holds mutable references to the
/// analyzer and statement context, allowing it to modify the lineage graph
/// as it visits each node.
///
/// # Target Node
///
/// The `target_node` field specifies which node in the lineage graph should
/// receive edges from discovered source tables. For example:
/// - In `INSERT INTO target SELECT * FROM source`, `target_node` would be
///   the node ID of `target`, so edges flow from `source` to `target`.
/// - In CTE analysis, each CTE body uses its CTE node as the target.
/// - When `None`, source tables are registered without creating data flow edges.
///
/// # Lifetime Parameters
///
/// - `'a`: Lifetime of the mutable borrows to `Analyzer` and `StatementContext`
/// - `'b`: Lifetime of the schema data held by the `Analyzer`
pub(crate) struct LineageVisitor<'a, 'b> {
    pub(crate) analyzer: &'a mut Analyzer<'b>,
    pub(crate) ctx: &'a mut StatementContext,
    pub(crate) target_node: Option<String>,
}

impl<'a, 'b> LineageVisitor<'a, 'b> {
    /// Creates a visitor for lineage analysis.
    ///
    /// # Arguments
    ///
    /// * `analyzer` - The analyzer instance that owns schema and configuration
    /// * `ctx` - The statement context for tracking nodes, edges, and aliases
    /// * `target_node` - Optional target node ID for data flow edges
    pub(crate) fn new(
        analyzer: &'a mut Analyzer<'b>,
        ctx: &'a mut StatementContext,
        target_node: Option<String>,
    ) -> Self {
        Self {
            analyzer,
            ctx,
            target_node,
        }
    }

    /// Converts an `Arc<str>` node ID to the `Option<String>` format used by the visitor.
    ///
    /// This is a convenience method for the common pattern of passing DML target IDs
    /// (which are `Arc<str>`) to visitor methods that expect `Option<String>`.
    #[inline]
    pub fn target_from_arc(arc: Option<&Arc<str>>) -> Option<String> {
        arc.map(|s| s.to_string())
    }

    /// Updates the target node for subsequent operations.
    ///
    /// This allows reusing a visitor instance when the target changes,
    /// avoiding the need to create multiple visitor instances.
    pub fn set_target_node(&mut self, target: Option<String>) {
        self.target_node = target;
    }

    /// Sets the last operation context (e.g., "JOIN", "UNION").
    ///
    /// This metadata is attached to nodes created during traversal to indicate
    /// how they relate to the query structure.
    pub fn set_last_operation(&mut self, op: Option<String>) {
        self.ctx.last_operation = op;
    }

    /// Adds a source table to the lineage graph.
    ///
    /// Registers the table as a node and creates a data flow edge to the
    /// current target node (if set). Returns the canonical table name if
    /// the table was successfully resolved.
    pub fn add_source_table(&mut self, table_name: &str) -> Option<String> {
        self.analyzer
            .add_source_table(self.ctx, table_name, self.target_node.as_deref())
    }

    /// Analyzes a DML target table (UPDATE/DELETE/MERGE target).
    ///
    /// Unlike `add_source_table`, this method:
    /// - Registers the table as a *produced* table (it will be modified)
    /// - Loads column information from schema if available
    /// - Does not create edges (the table is the target, not a source)
    ///
    /// Returns the canonical name and node ID of the target table.
    pub fn analyze_dml_target(
        &mut self,
        table_name: &str,
        alias: Option<&TableAlias>,
    ) -> Option<(String, Arc<str>)> {
        let canonical_res = self.analyzer.add_source_table(self.ctx, table_name, None);
        let canonical = canonical_res
            .clone()
            .unwrap_or_else(|| self.analyzer.normalize_table_name(table_name));

        // Register alias if present
        if let (Some(a), Some(canonical_name)) = (alias, canonical_res) {
            self.ctx
                .table_aliases
                .insert(a.name.to_string(), canonical_name);
        }

        // Retrieve the node ID from the context - add_source_table already registered it
        // with the correct type (view vs table) via relation_identity
        let node_id = self
            .ctx
            .table_node_ids
            .get(&canonical)
            .cloned()
            .unwrap_or_else(|| self.analyzer.relation_node_id(&canonical));

        self.analyzer
            .produced_tables
            .insert(canonical.clone(), self.ctx.statement_index);
        self.analyzer
            .add_table_columns_from_schema(self.ctx, &canonical, &node_id);

        Some((canonical, node_id))
    }

    /// Analyzes a DML target from a `TableFactor` node.
    ///
    /// Convenience wrapper around `analyze_dml_target` that extracts the table
    /// name from a `TableFactor`. For non-table factors (e.g., subqueries),
    /// falls back to regular visitor traversal.
    ///
    /// Returns the node ID of the target table if it was a simple table reference.
    pub fn analyze_dml_target_factor(&mut self, table: &TableFactor) -> Option<Arc<str>> {
        if let TableFactor::Table { name, alias, .. } = table {
            let table_name = name.to_string();
            self.analyze_dml_target(&table_name, alias.as_ref())
                .map(|(_, node_id)| node_id)
        } else {
            self.visit_table_factor(table);
            None
        }
    }

    /// Analyzes a DML target from a `TableWithJoins` node.
    ///
    /// Extracts the primary relation from the table-with-joins structure.
    /// For UPDATE statements, this is the table being updated (joins are
    /// handled separately as sources).
    ///
    /// Returns the node ID of the target table if it was a simple table reference.
    pub fn analyze_dml_target_from_table_with_joins(
        &mut self,
        table: &TableWithJoins,
    ) -> Option<Arc<str>> {
        if let TableFactor::Table { name, alias, .. } = &table.relation {
            let table_name = name.to_string();
            self.analyze_dml_target(&table_name, alias.as_ref())
                .map(|(_, node_id)| node_id)
        } else {
            self.visit_table_with_joins(table);
            None
        }
    }

    /// Pre-registers table aliases without creating lineage nodes.
    ///
    /// Used for multi-table DELETE statements where target tables may be
    /// specified by alias. By registering aliases first, the analyzer can
    /// resolve target references before processing the FROM clause.
    pub fn register_aliases_in_table_with_joins(&mut self, table_with_joins: &TableWithJoins) {
        self.register_aliases_in_table_factor(&table_with_joins.relation);
        for join in &table_with_joins.joins {
            self.register_aliases_in_table_factor(&join.relation);
        }
    }

    /// Registers aliases from a single table factor.
    ///
    /// Maps table aliases to their canonical names and tracks subquery aliases
    /// for later resolution during column reference analysis.
    fn register_aliases_in_table_factor(&mut self, table_factor: &TableFactor) {
        match table_factor {
            TableFactor::Table {
                name,
                alias: Some(a),
                ..
            } => {
                let canonical = self
                    .analyzer
                    .canonicalize_table_reference(&name.to_string())
                    .canonical;
                self.ctx.table_aliases.insert(a.name.to_string(), canonical);
            }
            TableFactor::Derived { alias: Some(a), .. } => {
                self.ctx.subquery_aliases.insert(a.name.to_string());
            }
            TableFactor::NestedJoin {
                table_with_joins, ..
            } => {
                self.register_aliases_in_table_with_joins(table_with_joins);
            }
            _ => {}
        }
    }

    /// Resolves a table alias to its canonical table name.
    ///
    /// Looks up the alias in the current statement context's alias registry.
    pub fn resolve_table_alias(&self, alias: Option<&str>) -> Option<String> {
        self.analyzer.resolve_table_alias(self.ctx, alias)
    }

    /// Canonicalizes a table reference using the analyzer's search path.
    pub(super) fn canonicalize_table_reference(&self, name: &str) -> super::TableResolution {
        self.analyzer.canonicalize_table_reference(name)
    }
}

impl<'a, 'b> Visitor for LineageVisitor<'a, 'b> {
    fn visit_query(&mut self, query: &Query) {
        // CteAnalyzer logic: 2 passes
        if let Some(with) = &query.with {
            // Pass 1: register all CTE names/nodes
            let mut cte_ids: Vec<(String, Arc<str>)> = Vec::new();
            for cte in &with.cte_tables {
                let cte_name = cte.alias.name.to_string();

                let cte_id = self.ctx.add_node(Node {
                    id: generate_node_id("cte", &cte_name),
                    node_type: NodeType::Cte,
                    label: cte_name.clone().into(),
                    qualified_name: Some(cte_name.clone().into()),
                    expression: None,
                    span: None,
                    metadata: None,
                    resolution_source: None,
                    filters: Vec::new(),
                    join_type: None,
                    join_condition: None,
                    aggregation: None,
                });

                self.ctx
                    .cte_definitions
                    .insert(cte_name.clone(), cte_id.clone());
                self.analyzer.all_ctes.insert(cte_name.clone());
                cte_ids.push((cte_name, cte_id));
            }

            // Pass 2: analyze each CTE body
            for (cte, (_, cte_id)) in with.cte_tables.iter().zip(cte_ids.iter()) {
                // We create a nested visitor for the CTE body to properly handle scope?
                // Actually analyze_query_body in query.rs passes target_node=Some(cte_id)
                // But LineageVisitor has a fixed target_node.
                // We should swap the target node temporarily or create a new visitor.
                // Creating a new visitor is cleaner.

                let mut cte_visitor =
                    LineageVisitor::new(self.analyzer, self.ctx, Some(cte_id.to_string()));
                cte_visitor.visit_query(&cte.query);

                // Capture CTE columns
                let columns = std::mem::take(&mut self.ctx.output_columns);
                self.ctx
                    .cte_columns
                    .insert(cte.alias.name.to_string(), columns);
            }
        }

        self.visit_set_expr(&query.body);
    }

    fn visit_set_expr(&mut self, set_expr: &SetExpr) {
        match set_expr {
            SetExpr::Select(select) => self.visit_select(select),
            SetExpr::Query(query) => self.visit_query(query),
            SetExpr::SetOperation {
                op, left, right, ..
            } => {
                let op_name = match op {
                    SetOperator::Union => "UNION",
                    SetOperator::Intersect => "INTERSECT",
                    SetOperator::Except => "EXCEPT",
                };

                self.visit_set_expr(left);
                self.visit_set_expr(right);

                if self.target_node.is_some() {
                    self.ctx.last_operation = Some(op_name.to_string());
                }
            }
            SetExpr::Values(values) => self.visit_values(values),
            SetExpr::Insert(insert_stmt) => {
                let Statement::Insert(insert) = insert_stmt else {
                    return;
                };
                let target_name = insert.table_name.to_string();
                self.add_source_table(&target_name);
            }
            SetExpr::Table(tbl) => {
                let name = tbl
                    .table_name
                    .as_ref()
                    .map(|n| n.to_string())
                    .unwrap_or_default();
                if !name.is_empty() {
                    self.add_source_table(&name);
                }
            }
            _ => {}
        }
    }

    fn visit_select(&mut self, select: &Select) {
        self.ctx.push_scope();

        for table_with_joins in &select.from {
            self.visit_table_with_joins(table_with_joins);
        }

        if self.analyzer.column_lineage_enabled {
            // Logic from SelectAnalyzer::analyze_select_columns

            self.ctx.clear_grouping();

            match &select.group_by {
                ast::GroupByExpr::Expressions(exprs, _) => {
                    let mut processed_grouping_exprs = HashSet::new();
                    for group_by in exprs {
                        let mut expr_analyzer = ExpressionAnalyzer::new(self.analyzer, self.ctx);
                        let expr_str = expr_analyzer.normalize_group_by_expr(group_by);
                        if !processed_grouping_exprs.insert(expr_str.clone()) {
                            continue;
                        }
                        expr_analyzer.ctx.add_grouping_column(expr_str);
                        expr_analyzer.analyze(group_by);
                    }
                }
                ast::GroupByExpr::All(_) => {
                    self.ctx.has_group_by = true;
                }
            }

            for (idx, item) in select.projection.iter().enumerate() {
                match item {
                    SelectItem::UnnamedExpr(expr) => {
                        let (sources, name, aggregation) = {
                            let ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
                            (
                                ExpressionAnalyzer::extract_column_refs(expr),
                                ea.derive_column_name(expr, idx),
                                ea.detect_aggregation(expr),
                            )
                        };

                        let expr_text = if is_simple_column_ref(expr) {
                            None
                        } else {
                            Some(expr.to_string())
                        };
                        let data_type = infer_expr_type(expr).map(|t| t.to_string());

                        self.analyzer.add_output_column_with_aggregation(
                            self.ctx,
                            OutputColumnParams {
                                name,
                                sources,
                                expression: expr_text,
                                data_type,
                                target_node: self.target_node.clone(),
                                approximate: false,
                                aggregation,
                            },
                        );
                    }
                    SelectItem::ExprWithAlias { expr, alias } => {
                        let (sources, aggregation) = {
                            let ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
                            (
                                ExpressionAnalyzer::extract_column_refs(expr),
                                ea.detect_aggregation(expr),
                            )
                        };

                        let name = alias.value.clone();
                        let expr_text = if is_simple_column_ref(expr) {
                            None
                        } else {
                            Some(expr.to_string())
                        };
                        let data_type = infer_expr_type(expr).map(|t| t.to_string());

                        self.analyzer.add_output_column_with_aggregation(
                            self.ctx,
                            OutputColumnParams {
                                name,
                                sources,
                                expression: expr_text,
                                data_type,
                                target_node: self.target_node.clone(),
                                approximate: false,
                                aggregation,
                            },
                        );
                    }
                    SelectItem::QualifiedWildcard(name, _) => {
                        let table_name = name.to_string();
                        self.analyzer.expand_wildcard(
                            self.ctx,
                            Some(&table_name),
                            self.target_node.as_deref(),
                        );
                    }
                    SelectItem::Wildcard(_) => {
                        self.analyzer
                            .expand_wildcard(self.ctx, None, self.target_node.as_deref());
                    }
                }
            }

            if let Some(ref where_clause) = select.selection {
                let mut ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
                ea.analyze(where_clause);
                ea.capture_filter_predicates(where_clause, FilterClauseType::Where);
            }

            if let Some(ref having) = select.having {
                let mut ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
                ea.analyze(having);
                ea.capture_filter_predicates(having, FilterClauseType::Having);
            }
        }

        self.ctx.pop_scope();
    }

    fn visit_table_with_joins(&mut self, table_with_joins: &TableWithJoins) {
        self.visit_table_factor(&table_with_joins.relation);

        for join in &table_with_joins.joins {
            let (join_type, join_condition) = Analyzer::convert_join_operator(&join.join_operator);
            self.ctx.current_join_info.join_type = join_type;
            self.ctx.current_join_info.join_condition = join_condition;
            self.ctx.last_operation = Analyzer::join_type_to_operation(join_type);

            self.visit_table_factor(&join.relation);

            self.ctx.current_join_info.join_type = None;
            self.ctx.current_join_info.join_condition = None;
        }
    }

    fn visit_table_factor(&mut self, table_factor: &TableFactor) {
        match table_factor {
            TableFactor::Table { name, alias, .. } => {
                let table_name = name.to_string();
                let canonical = self.add_source_table(&table_name);

                if let (Some(a), Some(canonical_name)) = (alias, canonical) {
                    self.ctx
                        .register_alias_in_scope(a.name.to_string(), canonical_name);
                }
            }
            TableFactor::Derived {
                subquery, alias, ..
            } => {
                // Recursive analysis with passed down target_node
                // IMPORTANT: The original code passed `target_node` to subqueries.
                // `self.visit_query` uses `self.target_node`.
                self.visit_query(subquery);

                if let Some(a) = alias {
                    self.ctx
                        .register_subquery_alias_in_scope(a.name.to_string());
                }
            }
            TableFactor::NestedJoin {
                table_with_joins, ..
            } => {
                self.visit_table_with_joins(table_with_joins);
            }
            TableFactor::TableFunction { .. } => {
                self.analyzer.issues.push(
                    Issue::info(
                        issue_codes::UNSUPPORTED_SYNTAX,
                        "Table function lineage not fully tracked",
                    )
                    .with_statement(self.ctx.statement_index),
                );
            }
            TableFactor::Pivot { .. } | TableFactor::Unpivot { .. } => {
                self.analyzer.issues.push(
                    Issue::warning(
                        issue_codes::UNSUPPORTED_SYNTAX,
                        "PIVOT/UNPIVOT lineage not fully supported",
                    )
                    .with_statement(self.ctx.statement_index),
                );
            }
            _ => {}
        }
    }

    fn visit_values(&mut self, values: &Values) {
        let mut expr_analyzer = ExpressionAnalyzer::new(self.analyzer, self.ctx);
        for row in &values.rows {
            for expr in row {
                expr_analyzer.analyze(expr);
            }
        }
    }
}
