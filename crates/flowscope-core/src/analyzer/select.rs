//! SELECT statement analysis for query projections, FROM clauses, and joins.
//!
//! This module provides `SelectAnalyzer` for handling SELECT statements, including:
//! - FROM clause table and subquery analysis
//! - JOIN processing with condition extraction
//! - Projection column analysis (SELECT list)
//! - Wildcard expansion (SELECT *)
//! - WHERE/HAVING clause filter capture
//! - GROUP BY column tracking
//!
//! The analyzer manages scope for table alias resolution and delegates expression
//! analysis to `ExpressionAnalyzer`.

use super::context::StatementContext;
use super::expression::ExpressionAnalyzer;
use super::helpers::{infer_expr_type, is_simple_column_ref};
use super::query::OutputColumnParams;
use super::Analyzer;
use crate::types::{issue_codes, FilterClauseType, Issue};
use sqlparser::ast::{self, SelectItem, TableFactor, TableWithJoins};
use std::collections::HashSet;
use std::sync::Arc;

/// Analyzes SELECT statements to build column-level lineage graphs.
///
/// `SelectAnalyzer` handles the FROM clause, JOINs, projections, and filter clauses.
/// It manages a scope stack to correctly resolve table aliases in nested queries
/// and subqueries.
///
/// # Example
///
/// ```ignore
/// let mut select_analyzer = SelectAnalyzer::new(analyzer, ctx);
/// select_analyzer.analyze(&select_statement, Some("target_table_id"));
/// ```
pub(crate) struct SelectAnalyzer<'a, 'b> {
    pub(crate) analyzer: &'a mut Analyzer<'b>,
    pub(crate) ctx: &'a mut StatementContext,
}

impl<'a, 'b> SelectAnalyzer<'a, 'b> {
    /// Creates a new SELECT analyzer borrowing the parent analyzer and statement context.
    pub(crate) fn new(analyzer: &'a mut Analyzer<'b>, ctx: &'a mut StatementContext) -> Self {
        Self { analyzer, ctx }
    }

    // --- Forwarding methods to reduce verbose nested access patterns ---

    /// Adds a source table to the lineage graph.
    ///
    /// Forwards to `Analyzer::add_source_table`, providing cleaner call sites.
    pub(crate) fn add_source_table(
        &mut self,
        table_name: &str,
        target_node: Option<&str>,
    ) -> Option<String> {
        self.analyzer
            .add_source_table(self.ctx, table_name, target_node)
    }

    /// Registers a table alias in the current context.
    pub(crate) fn register_table_alias(&mut self, alias: String, canonical: String) {
        self.ctx.table_aliases.insert(alias, canonical);
    }

    /// Marks a table as produced by the current statement.
    pub(crate) fn mark_table_produced(&mut self, canonical: String) {
        self.analyzer
            .produced_tables
            .insert(canonical, self.ctx.statement_index);
    }

    /// Adds columns from schema metadata for a table.
    pub(crate) fn add_table_columns_from_schema(
        &mut self,
        table_canonical: &str,
        table_node_id: &str,
    ) {
        self.analyzer
            .add_table_columns_from_schema(self.ctx, table_canonical, table_node_id);
    }

    /// Resolves a table alias to its canonical name.
    pub(crate) fn resolve_table_alias(&self, alias: Option<&str>) -> Option<String> {
        self.analyzer.resolve_table_alias(self.ctx, alias)
    }

    /// Canonicalizes a table reference using schema and search path.
    pub(crate) fn canonicalize_table_reference(&self, name: &str) -> super::TableResolution {
        self.analyzer.canonicalize_table_reference(name)
    }

    /// Normalizes a table name using the analyzer's settings.
    pub(crate) fn normalize_table_name(&self, name: &str) -> String {
        self.analyzer.normalize_table_name(name)
    }

    /// Analyzes a DML target table (UPDATE/DELETE/MERGE target).
    ///
    /// This helper encapsulates the common pattern for processing target tables:
    /// 1. Adds the table as a source node
    /// 2. Registers table alias if present
    /// 3. Marks the table as produced by this statement
    /// 4. Expands columns from schema metadata
    ///
    /// Returns the canonical table name and node ID if successful.
    pub(crate) fn analyze_dml_target(
        &mut self,
        table_name: &str,
        alias: Option<&ast::TableAlias>,
    ) -> Option<(String, Arc<str>)> {
        let canonical_res = self.add_source_table(table_name, None);
        let canonical = canonical_res
            .clone()
            .unwrap_or_else(|| self.normalize_table_name(table_name));

        // Register alias if present
        if let (Some(a), Some(canonical_name)) = (alias, canonical_res) {
            self.register_table_alias(a.name.to_string(), canonical_name);
        }

        let node_id = self
            .ctx
            .table_node_ids
            .get(&canonical)
            .cloned()
            .unwrap_or_else(|| self.analyzer.relation_node_id(&canonical));

        self.mark_table_produced(canonical.clone());
        self.add_table_columns_from_schema(&canonical, &node_id);

        Some((canonical, node_id))
    }

    // --- End forwarding methods ---

    /// Analyzes a TableFactor as a DML target, returning the node ID if successful.
    ///
    /// If the factor is a simple table reference, processes it as a DML target.
    /// Otherwise, falls back to standard table factor analysis.
    pub(crate) fn analyze_dml_target_factor(&mut self, table: &TableFactor) -> Option<Arc<str>> {
        if let TableFactor::Table { name, alias, .. } = table {
            let table_name = name.to_string();
            self.analyze_dml_target(&table_name, alias.as_ref())
                .map(|(_, node_id)| node_id)
        } else {
            self.analyze_table_factor(table, None);
            None
        }
    }

    /// Analyzes the main relation of a TableWithJoins as a DML target.
    ///
    /// If the main relation is a simple table reference, processes it as a DML target.
    /// Otherwise, falls back to full table-with-joins analysis (including joins).
    ///
    /// Note: This only identifies the DML target from the main relation.
    /// Joins on the target table should be analyzed separately if needed.
    pub(crate) fn analyze_dml_target_from_table_with_joins(
        &mut self,
        table: &TableWithJoins,
    ) -> Option<Arc<str>> {
        if let TableFactor::Table { name, alias, .. } = &table.relation {
            let table_name = name.to_string();
            self.analyze_dml_target(&table_name, alias.as_ref())
                .map(|(_, node_id)| node_id)
        } else {
            self.analyze_table_with_joins(table, None);
            None
        }
    }

    /// Analyzes a SELECT statement, processing FROM, projections, and filters.
    ///
    /// This method:
    /// 1. Pushes a new scope for table alias resolution
    /// 2. Analyzes the FROM clause to register source tables
    /// 3. Processes the SELECT projection for column lineage (if enabled)
    /// 4. Pops the scope when done
    ///
    /// The `target_node` parameter specifies the node ID that data flows into
    /// (e.g., a target table in INSERT...SELECT).
    pub(crate) fn analyze(&mut self, select: &ast::Select, target_node: Option<&str>) {
        // Push a new scope for this SELECT - isolates table resolution
        self.ctx.push_scope();

        // Analyze FROM clause first to register tables and aliases in the current scope
        for table_with_joins in &select.from {
            self.analyze_table_with_joins(table_with_joins, target_node);
        }

        // Analyze columns if column lineage is enabled
        if self.analyzer.column_lineage_enabled {
            self.analyze_select_columns(select, target_node);
        }

        // Pop the scope when done with this SELECT
        self.ctx.pop_scope();
    }

    fn analyze_select_columns(&mut self, select: &ast::Select, target_node: Option<&str>) {
        // Clear any previous grouping context
        self.ctx.clear_grouping();

        // Capture GROUP BY columns so we can identify grouping keys vs aggregates
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

        // Process SELECT projection with aggregation detection
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
                            target_node: target_node.map(|s| s.to_string()),
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
                            target_node: target_node.map(|s| s.to_string()),
                            approximate: false,
                            aggregation,
                        },
                    );
                }
                SelectItem::QualifiedWildcard(name, _) => {
                    let table_name = name.to_string();
                    self.analyzer
                        .expand_wildcard(self.ctx, Some(&table_name), target_node);
                }
                SelectItem::Wildcard(_) => {
                    self.analyzer.expand_wildcard(self.ctx, None, target_node);
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

    /// Analyzes a table reference with its associated joins.
    ///
    /// Processes the main relation and all JOINs, setting up join metadata
    /// (type and condition) for each joined table before analyzing it.
    pub(crate) fn analyze_table_with_joins(
        &mut self,
        table_with_joins: &TableWithJoins,
        target_node: Option<&str>,
    ) {
        // Analyze main relation
        self.analyze_table_factor(&table_with_joins.relation, target_node);

        // Analyze joins - process each joined table but don't create table-to-table edges
        // Join edges don't represent data flow; the column-level edges already show lineage
        for join in &table_with_joins.joins {
            // Convert join operator directly to JoinType enum and extract condition
            let (join_type, join_condition) = Analyzer::convert_join_operator(&join.join_operator);

            self.ctx.current_join_info.join_type = join_type;
            self.ctx.current_join_info.join_condition = join_condition;
            // Keep last_operation for backward compatibility with edge labels
            self.ctx.last_operation = Analyzer::join_type_to_operation(join_type);

            // Analyze the joined table
            self.analyze_table_factor(&join.relation, target_node);

            // Clear join info after processing
            self.ctx.current_join_info.join_type = None;
            self.ctx.current_join_info.join_condition = None;
        }
    }

    /// Analyzes a table factor (table reference, subquery, or nested join).
    ///
    /// Handles different types of table sources:
    /// - Regular tables: adds source node and registers aliases
    /// - Derived tables (subqueries): recursively analyzes the subquery
    /// - Nested joins: recursively processes the join structure
    /// - Table functions, UNNEST, PIVOT/UNPIVOT: logs appropriate warnings
    pub(crate) fn analyze_table_factor(
        &mut self,
        table_factor: &TableFactor,
        target_node: Option<&str>,
    ) {
        match table_factor {
            TableFactor::Table { name, alias, .. } => {
                let table_name = name.to_string();

                let canonical = self
                    .analyzer
                    .add_source_table(self.ctx, &table_name, target_node);

                // Register alias if present (in current scope)
                if let (Some(a), Some(canonical_name)) = (alias, canonical) {
                    self.ctx
                        .register_alias_in_scope(a.name.to_string(), canonical_name);
                }
            }
            TableFactor::Derived {
                subquery, alias, ..
            } => {
                // Subquery - analyze recursively
                self.analyzer.analyze_query(self.ctx, subquery, target_node);

                if let Some(a) = alias {
                    // Register subquery alias (in current scope)
                    self.ctx
                        .register_subquery_alias_in_scope(a.name.to_string());
                }
            }
            TableFactor::NestedJoin {
                table_with_joins, ..
            } => {
                self.analyze_table_with_joins(table_with_joins, target_node);
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
            TableFactor::Function { .. } => {
                // Table-valued function
            }
            TableFactor::UNNEST { .. } => {
                // UNNEST clause
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
            TableFactor::MatchRecognize { .. } => {}
            TableFactor::JsonTable { .. } => {}
        }
    }

    /// Pre-registers table aliases from a table-with-joins structure.
    ///
    /// Used by DELETE and other statements that need aliases registered before
    /// the main analysis pass (e.g., for multi-table deletes where targets may
    /// reference aliases defined in FROM/USING clauses).
    pub(crate) fn register_aliases_in_table_with_joins(
        &mut self,
        table_with_joins: &TableWithJoins,
    ) {
        self.register_aliases_in_table_factor(&table_with_joins.relation);
        for join in &table_with_joins.joins {
            self.register_aliases_in_table_factor(&join.relation);
        }
    }

    /// Pre-registers aliases from a single table factor.
    pub(crate) fn register_aliases_in_table_factor(&mut self, table_factor: &TableFactor) {
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
}
