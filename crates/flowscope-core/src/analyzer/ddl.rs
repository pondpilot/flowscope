//! DDL statement analysis for CREATE TABLE, CREATE VIEW, and CREATE TABLE AS statements.
//!
//! This module handles Data Definition Language statements, managing implied schema
//! generation from DDL definitions, conflict detection with imported schema, and
//! creating the appropriate nodes and edges in the lineage graph.

use super::context::StatementContext;
use super::helpers::{
    extract_simple_name, generate_edge_id, generate_node_id, split_qualified_identifiers,
};
use super::{Analyzer, SchemaTableEntry};
use crate::types::{
    issue_codes, ColumnSchema, Edge, EdgeType, Issue, Node, NodeType, SchemaOrigin, SchemaTable,
};
use chrono::Utc;
use sqlparser::ast::{ColumnDef, ObjectName, Query};

impl<'a> Analyzer<'a> {
    /// Helper to register implied schema from CREATE TABLE/VIEW/CTAS statements.
    /// Handles conflict detection with imported schema and marks tables as known even when
    /// implied schema capture is disabled.
    pub(super) fn register_implied_schema(
        &mut self,
        ctx: &StatementContext,
        canonical: &str,
        columns: Vec<ColumnSchema>,
        is_temporary: bool,
        statement_type: &str, // "TABLE", "VIEW", or "CREATE TABLE AS"
    ) {
        // Always treat a newly created object as known so subsequent statements can resolve it.
        self.known_tables.insert(canonical.to_string());

        // Check for conflict with imported schema
        if self.imported_tables.contains(canonical) {
            if let Some(imported_entry) = self.schema_tables.get(canonical) {
                let imported_cols: std::collections::HashSet<_> = imported_entry
                    .table
                    .columns
                    .iter()
                    .map(|c| &c.name)
                    .collect();
                let ddl_cols: std::collections::HashSet<_> =
                    columns.iter().map(|c| &c.name).collect();

                if imported_cols != ddl_cols {
                    self.issues.push(
                        Issue::warning(
                            issue_codes::SCHEMA_CONFLICT,
                            format!(
                                "{} for '{}' conflicts with imported schema. Using imported schema (imported has {} columns, {} has {} columns)",
                                statement_type,
                                canonical,
                                imported_cols.len(),
                                statement_type,
                                ddl_cols.len()
                            ),
                        )
                        .with_statement(ctx.statement_index),
                    );
                }
            }
            // Don't overwrite imported schema
            return;
        }

        // If implied capture is disabled or there are no columns, avoid persisting schema.
        if !self.allow_implied() || columns.is_empty() {
            return;
        }

        // Parse canonical name into parts
        let parts = split_qualified_identifiers(canonical);
        let (catalog, schema, table_name) = match parts.as_slice() {
            [catalog, schema, table] => {
                (Some(catalog.clone()), Some(schema.clone()), table.clone())
            }
            [schema, table] => (None, Some(schema.clone()), table.clone()),
            [table] => (None, None, table.clone()),
            _ => (None, None, extract_simple_name(canonical)),
        };

        self.schema_tables.insert(
            canonical.to_string(),
            SchemaTableEntry {
                table: SchemaTable {
                    catalog,
                    schema,
                    name: table_name,
                    columns,
                },
                origin: SchemaOrigin::Implied,
                source_statement_idx: Some(ctx.statement_index),
                updated_at: Utc::now(),
                temporary: is_temporary,
            },
        );
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

        self.all_relations.insert(canonical.clone());
        self.produced_tables
            .insert(canonical.clone(), ctx.statement_index);

        // Analyze source query
        self.analyze_query(ctx, query, Some(&target_id));

        // Capture output columns from the query to store as implied schema
        let output_columns: Vec<ColumnSchema> = ctx
            .output_columns
            .iter()
            .map(|col| ColumnSchema {
                name: col.name.clone(),
                data_type: col.data_type.clone(),
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
            .filter(|n| n.id != target_id)
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
        columns: &[ColumnDef],
        is_temporary: bool,
    ) {
        let target_name = name.to_string();

        let resolution = self.canonicalize_table_reference(&target_name);
        let canonical = resolution.canonical.clone();

        // Store schema info for subsequent statements, but only if no imported schema exists.
        // If an implied schema already exists, replace it (to handle CREATE OR REPLACE TABLE).

        let column_schemas: Vec<ColumnSchema> = columns
            .iter()
            .map(|c| ColumnSchema {
                name: c.name.value.clone(),

                data_type: Some(c.data_type.to_string()),
            })
            .collect();

        // Register implied schema using helper
        self.register_implied_schema(ctx, &canonical, column_schemas, is_temporary, "DDL");

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

        if self.schema_tables.contains_key(&canonical) {
            self.add_table_columns_from_schema(ctx, &canonical, &node_id);
        }

        self.all_relations.insert(canonical.clone());

        self.produced_tables.insert(canonical, ctx.statement_index);
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

        self.all_relations.insert(canonical.clone());
        self.produced_views.insert(canonical.clone());
        self.produced_tables
            .insert(canonical.clone(), ctx.statement_index);

        // Analyze source query
        self.analyze_query(ctx, query, Some(&target_id));

        // Capture output columns from the query to store as implied schema
        let output_columns: Vec<ColumnSchema> = ctx
            .output_columns
            .iter()
            .map(|col| ColumnSchema {
                name: col.name.clone(),
                data_type: col.data_type.clone(),
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
            .filter(|n| n.id != target_id)
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
}
