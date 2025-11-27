//! DDL statement analysis for CREATE TABLE, CREATE VIEW, and CREATE TABLE AS statements.
//!
//! This module handles Data Definition Language statements, managing implied schema
//! generation from DDL definitions, conflict detection with imported schema, and
//! creating the appropriate nodes and edges in the lineage graph.

use super::context::StatementContext;
use super::helpers::{
    build_column_schemas_with_constraints, extract_simple_name, generate_edge_id, generate_node_id,
};
use super::Analyzer;
use crate::types::{ColumnSchema, Edge, EdgeType, Node, NodeType, TableConstraintInfo};
use sqlparser::ast::{ObjectName, Query, TableConstraint};

impl<'a> Analyzer<'a> {
    /// Helper to register implied schema from CREATE TABLE/VIEW/CTAS statements.
    ///
    /// Delegates to the schema registry and collects any conflict warnings.
    pub(super) fn register_implied_schema(
        &mut self,
        ctx: &StatementContext,
        canonical: &str,
        columns: Vec<ColumnSchema>,
        is_temporary: bool,
        statement_type: &str,
    ) {
        if let Some(mut issue) = self.schema.register_implied(
            canonical,
            columns,
            is_temporary,
            statement_type,
            ctx.statement_index,
        ) {
            // Attach span if we can find the table name in the SQL
            if let Some(span) = self.find_span(canonical) {
                issue = issue.with_span(span);
            }
            self.issues.push(issue);
        }
    }

    pub(super) fn analyze_create_table_as(
        &mut self,
        ctx: &mut StatementContext,
        table_name: &ObjectName,
        query: &Query,
        is_temporary: bool,
    ) {
        let target_name = table_name.to_string();
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

        let projection_checkpoint = ctx.projection_checkpoint();
        // Analyze source query
        self.analyze_query(ctx, query, Some(&target_id));

        // Capture output columns from the query to store as implied schema
        let projection_columns = ctx.take_output_columns_since(projection_checkpoint);
        let output_columns: Vec<ColumnSchema> = projection_columns
            .iter()
            .map(|col| ColumnSchema {
                name: col.name.clone(),
                data_type: col.data_type.clone(),
                is_primary_key: None,
                foreign_key: None,
            })
            .collect();

        // Register implied schema using helper
        self.register_implied_schema(
            ctx,
            &canonical,
            output_columns,
            is_temporary,
            "CREATE TABLE AS",
        );

        // Create edges from all source tables to target
        let source_nodes: Vec<_> = ctx
            .nodes
            .iter()
            .filter(|n| n.id != target_id && n.node_type.is_table_like())
            .map(|n| n.id.clone())
            .collect();

        for source_id in source_nodes {
            let edge_id = generate_edge_id(&source_id, &target_id);
            if !ctx.edge_ids.contains(&edge_id) {
                ctx.add_edge(Edge {
                    id: edge_id,
                    from: source_id,
                    to: target_id.clone(),
                    edge_type: EdgeType::DataFlow,
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

    pub(super) fn analyze_create_table(
        &mut self,
        ctx: &mut StatementContext,
        name: &ObjectName,
        columns: &[sqlparser::ast::ColumnDef],
        table_constraints: &[TableConstraint],
        is_temporary: bool,
    ) {
        let target_name = name.to_string();

        let resolution = self.canonicalize_table_reference(&target_name);
        let canonical = resolution.canonical.clone();

        let (column_schemas, table_constraint_infos) =
            build_column_schemas_with_constraints(columns, table_constraints);

        // Register implied schema using helper
        self.register_implied_schema_with_constraints(
            ctx,
            &canonical,
            column_schemas,
            table_constraint_infos,
            is_temporary,
            "DDL",
        );

        // Create target table node

        let node_id = generate_node_id("table", &canonical);

        ctx.add_node(Node {
            id: node_id.clone(),
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

        // Create column nodes immediately from schema (either imported or from CREATE TABLE)
        if self.schema.is_known(&canonical) {
            self.add_table_columns_from_schema(ctx, &canonical, &node_id);
        }

        self.tracker
            .record_produced(&canonical, ctx.statement_index);
    }

    pub(super) fn analyze_create_view(
        &mut self,
        ctx: &mut StatementContext,
        name: &ObjectName,
        query: &Query,
        is_temporary: bool,
    ) {
        let target_name = name.to_string();
        let canonical = self.normalize_table_name(&target_name);

        // Create target view node
        let target_id = ctx.add_node(Node {
            id: generate_node_id("view", &canonical),
            node_type: NodeType::View,
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
            .record_view_produced(&canonical, ctx.statement_index);

        let projection_checkpoint = ctx.projection_checkpoint();
        // Analyze source query
        self.analyze_query(ctx, query, Some(&target_id));

        // Capture output columns from the query to store as implied schema
        let projection_columns = ctx.take_output_columns_since(projection_checkpoint);
        let output_columns: Vec<ColumnSchema> = projection_columns
            .iter()
            .map(|col| ColumnSchema {
                name: col.name.clone(),
                data_type: col.data_type.clone(),
                is_primary_key: None,
                foreign_key: None,
            })
            .collect();

        // Register implied schema using helper
        self.register_implied_schema(
            ctx,
            &canonical,
            output_columns,
            is_temporary,
            "VIEW definition",
        );

        // Create edges from all source tables to target
        let source_nodes: Vec<_> = ctx
            .nodes
            .iter()
            .filter(|n| n.id != target_id && n.node_type.is_table_like())
            .map(|n| n.id.clone())
            .collect();

        for source_id in source_nodes {
            let edge_id = generate_edge_id(&source_id, &target_id);
            if !ctx.edge_ids.contains(&edge_id) {
                ctx.add_edge(Edge {
                    id: edge_id,
                    from: source_id,
                    to: target_id.clone(),
                    edge_type: EdgeType::DataFlow,
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

    /// Helper to register implied schema with constraint information.
    pub(super) fn register_implied_schema_with_constraints(
        &mut self,
        ctx: &StatementContext,
        canonical: &str,
        columns: Vec<ColumnSchema>,
        constraints: Vec<TableConstraintInfo>,
        is_temporary: bool,
        statement_type: &str,
    ) {
        if let Some(mut issue) = self.schema.register_implied_with_constraints(
            canonical,
            columns,
            constraints,
            is_temporary,
            statement_type,
            ctx.statement_index,
        ) {
            if let Some(span) = self.find_span(canonical) {
                issue = issue.with_span(span);
            }
            self.issues.push(issue);
        }
    }
}
