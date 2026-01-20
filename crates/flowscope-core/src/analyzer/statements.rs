//! Statement-level analysis for DML operations (INSERT, UPDATE, DELETE, MERGE).
//!
//! This module handles Data Manipulation Language statements, analyzing data flow
//! between source and target tables. It delegates to specialized modules for DDL
//! and query analysis while managing the overall statement context and lineage graph.

use super::complexity;
use super::context::StatementContext;
use super::expression::ExpressionAnalyzer;
use super::helpers::{
    classify_query_type, extract_simple_name, generate_edge_id, generate_node_id,
};
use super::visitor::{LineageVisitor, Visitor};
use super::Analyzer;
use crate::error::ParseError;
use crate::types::{
    issue_codes, Edge, EdgeType, Issue, JoinType, Node, NodeType, Span, StatementLineage,
};
use sqlparser::ast::{
    self, Assignment, Expr, FromTable, MergeAction, MergeClause, MergeInsertKind, ObjectName,
    Statement, TableFactor, TableWithJoins, UpdateTableFromKind,
};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;
#[cfg(feature = "tracing")]
use tracing::{info, info_span};

/// Information about a join node for dependency edge construction.
struct JoinNodeInfo {
    /// Node ID of the joined table
    node_id: Arc<str>,
    /// Type of join (INNER, LEFT, etc.)
    join_type: Option<JoinType>,
    /// Join condition expression (e.g., "a.id = b.id")
    join_condition: Option<Arc<str>>,
}

impl<'a> Analyzer<'a> {
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, statement), fields(index, source = source_name.as_deref())))]
    pub(super) fn analyze_statement(
        &mut self,
        index: usize,
        statement: &Statement,
        source_name: Option<String>,
        source_range: Range<usize>,
    ) -> Result<StatementLineage, ParseError> {
        let mut ctx = StatementContext::new(index);

        let statement_type = match statement {
            Statement::Query(query) => {
                ctx.ensure_output_node();
                self.analyze_query(&mut ctx, query, None);
                classify_query_type(query)
            }
            Statement::Insert(insert) => {
                self.analyze_insert(&mut ctx, insert);
                "INSERT".to_string()
            }
            Statement::CreateTable(create) => {
                if let Some(query) = &create.query {
                    self.analyze_create_table_as(&mut ctx, &create.name, query, create.temporary);
                    "CREATE_TABLE_AS".to_string()
                } else {
                    self.analyze_create_table(
                        &mut ctx,
                        &create.name,
                        &create.columns,
                        &create.constraints,
                        create.temporary,
                    );
                    "CREATE_TABLE".to_string()
                }
            }
            Statement::CreateView {
                name,
                query,
                temporary,
                ..
            } => {
                self.analyze_create_view(&mut ctx, name, query, *temporary);
                "CREATE_VIEW".to_string()
            }
            Statement::Update {
                table,
                assignments,
                from,
                selection,
                ..
            } => {
                self.analyze_update(&mut ctx, table, assignments, from, selection);
                "UPDATE".to_string()
            }
            Statement::Delete(delete) => {
                self.analyze_delete(
                    &mut ctx,
                    &delete.tables,
                    &delete.from,
                    &delete.using,
                    &delete.selection,
                );
                "DELETE".to_string()
            }
            Statement::Merge {
                into,
                table,
                source,
                on,
                clauses,
                ..
            } => {
                self.analyze_merge(&mut ctx, *into, table, source, on, clauses);
                "MERGE".to_string()
            }
            Statement::Drop {
                object_type, names, ..
            } => {
                self.analyze_drop(&mut ctx, object_type, names);
                "DROP".to_string()
            }
            // Statements that are recognized but don't produce lineage
            // (admin, session, and metadata operations)
            Statement::AlterTable { .. } => "ALTER_TABLE".to_string(),
            Statement::AlterView { .. } => "ALTER_VIEW".to_string(),
            Statement::AlterIndex { .. } => "ALTER_INDEX".to_string(),
            Statement::AlterSchema(_) => "ALTER_SCHEMA".to_string(),
            Statement::AlterRole { .. } => "ALTER_ROLE".to_string(),
            Statement::Grant { .. } => "GRANT".to_string(),
            Statement::Revoke { .. } => "REVOKE".to_string(),
            Statement::Set(_) => "SET".to_string(),
            Statement::ShowVariable { .. } | Statement::ShowVariables { .. } => "SHOW".to_string(),
            Statement::Truncate { .. } => "TRUNCATE".to_string(),
            Statement::Comment { .. } => "COMMENT".to_string(),
            Statement::Explain { .. } | Statement::ExplainTable { .. } => "EXPLAIN".to_string(),
            Statement::Analyze { .. } => "ANALYZE".to_string(),
            Statement::Call(_) => "CALL".to_string(),
            Statement::Use(_) => "USE".to_string(),
            Statement::StartTransaction { .. }
            | Statement::Commit { .. }
            | Statement::Rollback { .. }
            | Statement::Savepoint { .. } => "TRANSACTION".to_string(),
            Statement::CreateIndex(_) => "CREATE_INDEX".to_string(),
            Statement::CreateSchema { .. } => "CREATE_SCHEMA".to_string(),
            Statement::CreateDatabase { .. } => "CREATE_DATABASE".to_string(),
            Statement::CreateRole { .. } => "CREATE_ROLE".to_string(),
            Statement::CreateFunction { .. } => "CREATE_FUNCTION".to_string(),
            Statement::CreateProcedure { .. } => "CREATE_PROCEDURE".to_string(),
            Statement::CreateTrigger { .. } => "CREATE_TRIGGER".to_string(),
            Statement::CreateType { .. } => "CREATE_TYPE".to_string(),
            Statement::CreateSequence { .. } => "CREATE_SEQUENCE".to_string(),
            Statement::CreateExtension { .. } => "CREATE_EXTENSION".to_string(),
            Statement::DropFunction { .. } => "DROP_FUNCTION".to_string(),
            Statement::DropProcedure { .. } => "DROP_PROCEDURE".to_string(),
            Statement::DropTrigger { .. } => "DROP_TRIGGER".to_string(),
            Statement::Copy { .. } => "COPY".to_string(),
            Statement::CopyIntoSnowflake { .. } => "COPY".to_string(),
            _ => {
                self.issues.push(
                    Issue::warning(
                        issue_codes::UNSUPPORTED_SYNTAX,
                        "Statement type not fully supported for lineage analysis",
                    )
                    .with_statement(index),
                );
                "UNKNOWN".to_string()
            }
        };

        // Apply pending filter predicates to table nodes before finalizing
        self.apply_pending_filters(&mut ctx);
        self.add_join_dependency_edges(&mut ctx);

        // Register implied schema for source tables referenced in the query
        self.register_source_tables_schema(&ctx);

        // Calculate statement-level stats
        let join_count = complexity::count_joins(&ctx.nodes);
        let complexity_score = complexity::calculate_complexity(&ctx.nodes);

        Ok(StatementLineage {
            statement_index: index,
            statement_type,
            source_name,
            nodes: ctx.nodes,
            edges: ctx.edges,
            span: Some(Span::new(source_range.start, source_range.end)),
            join_count,
            complexity_score,
        })
    }

    fn add_join_dependency_edges(&self, ctx: &mut StatementContext) {
        let output_node_id = match ctx.output_node_id.as_ref() {
            Some(node_id) => node_id.clone(),
            None => return,
        };

        let output_column_ids: HashSet<_> = ctx
            .edges
            .iter()
            .filter(|edge| edge.edge_type == EdgeType::Ownership && edge.from == output_node_id)
            .map(|edge| edge.to.clone())
            .collect();

        if output_column_ids.is_empty() {
            return;
        }

        let mut table_columns: HashMap<Arc<str>, Vec<Arc<str>>> = HashMap::new();
        for edge in &ctx.edges {
            if edge.edge_type == EdgeType::Ownership {
                table_columns
                    .entry(edge.from.clone())
                    .or_default()
                    .push(edge.to.clone());
            }
        }

        let join_nodes: Vec<JoinNodeInfo> = ctx
            .nodes
            .iter()
            .filter(|node| node.node_type.is_table_like() && node.join_type.is_some())
            .map(|node| JoinNodeInfo {
                node_id: node.id.clone(),
                join_type: node.join_type,
                join_condition: node.join_condition.clone(),
            })
            .collect();

        for join_info in join_nodes {
            let JoinNodeInfo {
                node_id,
                join_type,
                join_condition,
            } = join_info;
            let owned_columns = table_columns.get(&node_id).cloned().unwrap_or_default();

            let contributes_to_output = !owned_columns.is_empty()
                && ctx.edges.iter().any(|edge| {
                    matches!(edge.edge_type, EdgeType::DataFlow | EdgeType::Derivation)
                        && owned_columns.iter().any(|col| col == &edge.from)
                        && output_column_ids.contains(&edge.to)
                });

            if contributes_to_output {
                continue;
            }

            let edge_key = format!("join_dependency:{node_id}");
            let edge_id = generate_edge_id(&edge_key, output_node_id.as_ref());
            if ctx.edge_ids.contains(&edge_id) {
                continue;
            }

            ctx.add_edge(Edge {
                id: edge_id,
                from: node_id,
                to: output_node_id.clone(),
                edge_type: EdgeType::JoinDependency,
                expression: None,
                operation: None,
                join_type,
                join_condition,
                metadata: None,
                approximate: None,
            });
        }
    }

    pub(super) fn analyze_insert(&mut self, ctx: &mut StatementContext, insert: &ast::Insert) {
        let target_name = insert.table.to_string();
        let canonical = self.normalize_table_name(&target_name);

        // Create target table node
        let target_id = ctx.add_node(Node {
            id: generate_node_id("table", &canonical),
            node_type: NodeType::Table,
            label: extract_simple_name(&target_name).into(),
            qualified_name: Some(canonical.clone().into()),
            expression: None,
            span: None,
            metadata: None,
            resolution_source: None,
            filters: Vec::new(),
            join_type: None,
            join_condition: None,
            aggregation: None,
        });

        self.tracker
            .record_produced(&canonical, ctx.statement_index);

        // Analyze source - check the body of the insert
        if let Some(ref source_body) = insert.source {
            self.analyze_query_body(ctx, &source_body.body, Some(&target_id));
        }
    }

    pub(super) fn analyze_update(
        &mut self,
        ctx: &mut StatementContext,
        table: &TableWithJoins,
        assignments: &[Assignment],
        from: &Option<UpdateTableFromKind>,
        selection: &Option<Expr>,
    ) {
        let target_node_id = {
            let mut visitor = LineageVisitor::new(self, ctx, None);

            // 1. Analyze the target table
            visitor.analyze_dml_target_from_table_with_joins(table)
        };

        // 2. Analyze FROM clause (Postgres style) and joins in target table structure
        {
            let target = LineageVisitor::target_from_arc(target_node_id.as_ref());
            let mut visitor = LineageVisitor::new(self, ctx, target);

            if let Some(from_kind) = from {
                match from_kind {
                    UpdateTableFromKind::BeforeSet(tables) => {
                        for t in tables {
                            visitor.visit_table_with_joins(t);
                        }
                    }
                    UpdateTableFromKind::AfterSet(tables) => {
                        for t in tables {
                            visitor.visit_table_with_joins(t);
                        }
                    }
                }
            }

            for join in &table.joins {
                visitor.set_last_operation(Some("JOIN".to_string()));
                visitor.visit_table_factor(&join.relation);
            }
        }

        // 3. Analyze assignments (SET clause)
        let mut expr_analyzer = ExpressionAnalyzer::new(self, ctx);
        for assignment in assignments {
            expr_analyzer.analyze(&assignment.value);
        }

        // 4. Analyze selection (WHERE clause)
        if let Some(expr) = selection {
            expr_analyzer.analyze(expr);
        }
    }

    pub(super) fn analyze_delete(
        &mut self,
        ctx: &mut StatementContext,
        tables: &[ObjectName],
        from: &FromTable,
        using: &Option<Vec<TableWithJoins>>,
        selection: &Option<Expr>,
    ) {
        let mut target_ids: Vec<Arc<str>> = Vec::new();

        // Scope for visitor usage
        {
            let mut visitor = LineageVisitor::new(self, ctx, None);

            // Pre-register aliases from sources so multi-table deletes can resolve targets.
            match from {
                FromTable::WithFromKeyword(ts) | FromTable::WithoutKeyword(ts) => {
                    for t in ts {
                        visitor.register_aliases_in_table_with_joins(t);
                    }
                }
            }
            if let Some(us) = using {
                for t in us {
                    visitor.register_aliases_in_table_with_joins(t);
                }
            }

            // 1. Identify targets
            if !tables.is_empty() {
                // Multi-table delete - targets may reference aliases
                for obj in tables {
                    let name = obj.to_string();
                    let target_canonical = visitor
                        .resolve_table_alias(Some(&name))
                        .unwrap_or_else(|| visitor.canonicalize_table_reference(&name).canonical);

                    if let Some((_canonical, node_id)) =
                        visitor.analyze_dml_target(&target_canonical, None)
                    {
                        #[cfg(feature = "tracing")]
                        info!(target: "analyzer", "DELETE target identified: {} (ID: {})", _canonical, node_id);
                        target_ids.push(node_id);
                    }
                }
            } else {
                // Standard SQL: first table in FROM is target
                let ts = match from {
                    FromTable::WithFromKeyword(ts) | FromTable::WithoutKeyword(ts) => ts,
                };
                if let Some(first) = ts.first() {
                    if let TableFactor::Table { name, alias, .. } = &first.relation {
                        let name_str = name.to_string();
                        if let Some((_canonical, node_id)) =
                            visitor.analyze_dml_target(&name_str, alias.as_ref())
                        {
                            #[cfg(feature = "tracing")]
                            info!(target: "analyzer", "DELETE target identified: {} (ID: {})", _canonical, node_id);
                            target_ids.push(node_id);
                        }
                    }
                }
            }
        }
        // 2. Analyze sources (FROM + USING)
        let sources: Vec<&[TableWithJoins]> = {
            let from_tables = match from {
                FromTable::WithFromKeyword(ts) | FromTable::WithoutKeyword(ts) => ts.as_slice(),
            };
            let mut sources = vec![from_tables];
            if let Some(us) = using {
                sources.push(us.as_slice());
            }
            sources
        };

        if target_ids.is_empty() {
            let mut visitor = LineageVisitor::new(self, ctx, None);
            for ts in sources {
                for t in ts {
                    visitor.visit_table_with_joins(t);
                }
            }
        } else {
            for target_id in &target_ids {
                let mut visitor = LineageVisitor::new(self, ctx, Some(target_id.to_string()));
                for ts in &sources {
                    for t in *ts {
                        visitor.visit_table_with_joins(t);
                    }
                }
            }
        }

        // 3. Analyze selection
        if let Some(expr) = selection {
            let mut expr_analyzer = ExpressionAnalyzer::new(self, ctx);
            expr_analyzer.analyze(expr);
        }
    }

    pub(super) fn analyze_merge(
        &mut self,
        ctx: &mut StatementContext,
        _into: bool,
        table: &TableFactor,
        source: &TableFactor,
        on: &Expr,
        clauses: &[MergeClause],
    ) {
        // 1. Analyze Target Table and 2. Analyze Source Table (USING clause)
        let mut visitor = LineageVisitor::new(self, ctx, None);
        let target_id = visitor.analyze_dml_target_factor(table);

        visitor.set_target_node(LineageVisitor::target_from_arc(target_id.as_ref()));
        visitor.visit_table_factor(source);

        // 3. Analyze ON predicate
        let mut expr_analyzer = ExpressionAnalyzer::new(self, ctx);
        expr_analyzer.analyze(on);

        // 4. Analyze MERGE clauses
        for clause in clauses {
            match &clause.action {
                MergeAction::Update { assignments } => {
                    // Analyze assignments in UPDATE clause
                    for assignment in assignments {
                        expr_analyzer.analyze(&assignment.value);
                    }
                }
                MergeAction::Insert(insert_expr) => {
                    // Analyze INSERT clause
                    match &insert_expr.kind {
                        MergeInsertKind::Values(values) => {
                            // VALUES clause with rows
                            for row in &values.rows {
                                for value in row {
                                    expr_analyzer.analyze(value);
                                }
                            }
                        }
                        MergeInsertKind::Row => {
                            // ROW keyword - no explicit values to analyze here
                        }
                    }
                }
                MergeAction::Delete => {
                    // DELETE has no additional expressions
                }
            }

            // Analyze the predicate for this clause (WHEN MATCHED ... AND <predicate>)
            if let Some(ref predicate) = clause.predicate {
                expr_analyzer.analyze(predicate);
            }
        }
    }

    pub(super) fn analyze_drop(
        &mut self,
        _ctx: &mut StatementContext,
        object_type: &ast::ObjectType,
        names: &[ObjectName],
    ) {
        // Handle DROP TABLE/VIEW to remove implied schema entries (only if allow_implied is true)
        if self.allow_implied()
            && matches!(object_type, ast::ObjectType::Table | ast::ObjectType::View)
        {
            for name in names {
                let table_name = name.to_string();
                let canonical = self.normalize_table_name(&table_name);

                // Only remove if it's an implied entry (not imported)
                self.schema.remove_implied(&canonical);
                self.tracker.remove(&canonical);
            }
        }
    }
}
