//! Statement-level analysis for DML operations (INSERT, UPDATE, DELETE, MERGE).
//!
//! This module handles Data Manipulation Language statements, analyzing data flow
//! between source and target tables. It delegates to specialized modules for DDL
//! and query analysis while managing the overall statement context and lineage graph.

use super::complexity;
use super::context::StatementContext;
use super::expression::ExpressionAnalyzer;
use super::helpers::{classify_query_type, extract_simple_name, generate_node_id};
use super::select::SelectAnalyzer;
use super::Analyzer;
use crate::error::ParseError;
use crate::types::{issue_codes, Issue, Node, NodeType, StatementLineage};
use sqlparser::ast::{
    self, Assignment, Expr, FromTable, MergeAction, MergeClause, MergeInsertKind, ObjectName,
    Statement, TableFactor, TableWithJoins,
};
#[cfg(feature = "tracing")]
use tracing::{info, info_span};

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_statement(
        &mut self,
        index: usize,
        statement: &Statement,
        source_name: Option<String>,
    ) -> Result<StatementLineage, ParseError> {
        let mut ctx = StatementContext::new(index);

        let statement_type = match statement {
            Statement::Query(query) => {
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
                returning: _,
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
            _ => {
                self.issues.push(
                    Issue::warning(
                        issue_codes::UNSUPPORTED_SYNTAX,
                        "Statement type not fully supported for lineage analysis".to_string(),
                    )
                    .with_statement(index),
                );
                "UNKNOWN".to_string()
            }
        };

        // Apply pending filter predicates to table nodes before finalizing
        self.apply_pending_filters(&mut ctx);

        // Calculate statement-level stats
        let join_count = complexity::count_joins(&ctx.nodes);
        let complexity_score = complexity::calculate_complexity(&ctx.nodes);

        Ok(StatementLineage {
            statement_index: index,
            statement_type,
            source_name,
            nodes: ctx.nodes,
            edges: ctx.edges,
            span: None,
            join_count,
            complexity_score,
        })
    }

    pub(super) fn analyze_insert(&mut self, ctx: &mut StatementContext, insert: &ast::Insert) {
        let target_name = insert.table_name.to_string();
        let canonical = self.normalize_table_name(&target_name);

        // Create target table node
        let target_id = ctx.add_node(Node {
            id: generate_node_id("table", &canonical),
            node_type: NodeType::Table,
            label: extract_simple_name(&target_name),
            qualified_name: Some(canonical.clone()),
            expression: None,
            span: None,
            metadata: None,
            resolution_source: None,
            filters: Vec::new(),
            join_type: None,
            join_condition: None,
            aggregation: None,
        });

        self.all_tables.insert(canonical.clone());
        self.produced_tables.insert(canonical, ctx.statement_index);

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
        from: &Option<TableWithJoins>,
        selection: &Option<Expr>,
    ) {
        let mut select_analyzer = SelectAnalyzer::new(self, ctx);

        // 1. Analyze the target table
        let target_node_id = select_analyzer.analyze_dml_target_from_table_with_joins(table);

        // 2. Analyze FROM clause (Postgres style)
        if let Some(from_table) = from {
            select_analyzer.analyze_table_with_joins(from_table, target_node_id.as_deref());
        }

        // Also analyze the joins in the target table structure itself
        for join in &table.joins {
            let join_type = "JOIN";
            select_analyzer.ctx.last_operation = Some(join_type.to_string());
            select_analyzer.analyze_table_factor(&join.relation, target_node_id.as_deref());
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
        let mut target_ids = Vec::new();

        // Scope for SelectAnalyzer usage
        {
            let mut select_analyzer = SelectAnalyzer::new(self, ctx);

            // Pre-register aliases from sources so multi-table deletes can resolve targets.
            match from {
                FromTable::WithFromKeyword(ts) | FromTable::WithoutKeyword(ts) => {
                    for t in ts {
                        select_analyzer.register_aliases_in_table_with_joins(t);
                    }
                }
            }
            if let Some(us) = using {
                for t in us {
                    select_analyzer.register_aliases_in_table_with_joins(t);
                }
            }

            // 1. Identify targets
            if !tables.is_empty() {
                // Multi-table delete - targets may reference aliases
                for obj in tables {
                    let name = obj.to_string();
                    let target_canonical = select_analyzer
                        .resolve_table_alias(Some(&name))
                        .unwrap_or_else(|| {
                            select_analyzer
                                .canonicalize_table_reference(&name)
                                .canonical
                        });

                    if let Some((_canonical, node_id)) =
                        select_analyzer.analyze_dml_target(&target_canonical, None)
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
                            select_analyzer.analyze_dml_target(&name_str, alias.as_ref())
                        {
                            #[cfg(feature = "tracing")]
                            info!(target: "analyzer", "DELETE target identified: {} (ID: {})", _canonical, node_id);
                            target_ids.push(node_id);
                        }
                    }
                }
            }

            // 2. Analyze sources (FROM + USING)
            // Helper to avoid borrow checker issues by re-using select_analyzer
            let mut analyze_sources = |ts: &[TableWithJoins]| {
                for t in ts {
                    if target_ids.is_empty() {
                        select_analyzer.analyze_table_with_joins(t, None);
                    } else {
                        for target_id in &target_ids {
                            select_analyzer.analyze_table_with_joins(t, Some(target_id));
                        }
                    }
                }
            };

            match from {
                FromTable::WithFromKeyword(ts) | FromTable::WithoutKeyword(ts) => {
                    analyze_sources(ts);
                }
            }
            if let Some(us) = using {
                analyze_sources(us);
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
        let mut select_analyzer = SelectAnalyzer::new(self, ctx);

        // 1. Analyze Target Table
        let target_id = select_analyzer.analyze_dml_target_factor(table);

        // 2. Analyze Source Table (USING clause)
        select_analyzer.analyze_table_factor(source, target_id.as_deref());

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
                if !self.imported_tables.contains(&canonical) {
                    self.schema_tables.remove(&canonical);
                    self.known_tables.remove(&canonical);
                    self.produced_tables.remove(&canonical);
                }
            }
        }
    }
}
