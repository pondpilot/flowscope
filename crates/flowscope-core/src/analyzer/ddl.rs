//! DDL statement analysis for CREATE TABLE, CREATE VIEW, and CREATE TABLE AS statements.
//!
//! This module handles Data Definition Language statements, managing implied schema
//! generation from DDL definitions, conflict detection with imported schema, and
//! creating the appropriate nodes and edges in the lineage graph.

use super::context::StatementContext;
use super::helpers::{
    build_column_schemas_with_constraints, extract_simple_name, generate_node_id,
};
use super::Analyzer;
use crate::types::{
    ColumnSchema, ConstraintType, ForeignKeyRef, Node, NodeType, TableConstraintInfo,
};
use sqlparser::ast::{ObjectName, Query, TableConstraint};
use std::collections::BTreeMap;

/// Statement type used when registering source tables (tables being read from).
/// Source tables are always used in a SELECT-like context, regardless of the
/// outer statement type (SELECT, CREATE TABLE AS, INSERT INTO...SELECT, etc.).
const SOURCE_TABLE_STATEMENT_TYPE: &str = "SELECT";

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

    /// Register implied schema for source tables referenced in a query.
    ///
    /// This captures the tables and columns referenced in SELECT, CREATE TABLE AS,
    /// CREATE VIEW, and other query-containing statements. Unlike `register_implied_schema`
    /// which captures target tables, this captures source tables.
    pub(super) fn register_source_tables_schema(&mut self, ctx: &StatementContext) {
        if !self.allow_implied() {
            return;
        }

        for (canonical, columns) in &ctx.source_table_columns {
            // Skip CTEs and subqueries (they're not real tables)
            if ctx.cte_definitions.contains_key(canonical)
                || ctx.subquery_aliases.contains(canonical)
            {
                continue;
            }

            // Skip if already in imported schema (user-provided takes precedence)
            if self.schema.is_imported(canonical) {
                continue;
            }

            // Skip if table was seeded from DDL (CREATE TABLE) during pre-collection.
            // DDL-seeded tables have complete column definitions and should not be
            // overwritten by partial column sets discovered during query analysis.
            if self.schema.is_ddl_seeded(canonical) {
                continue;
            }

            // Sort columns by name for deterministic output
            let mut column_list: Vec<_> = columns.iter().collect();
            column_list.sort_by(|a, b| a.0.cmp(b.0));

            let column_schemas: Vec<ColumnSchema> = column_list
                .into_iter()
                .map(|(name, data_type)| {
                    // Look up any implied FK relationship for this column
                    let foreign_key = ctx
                        .implied_foreign_keys
                        .get(&(canonical.clone(), name.clone()))
                        .map(|(ref_table, ref_column)| ForeignKeyRef {
                            table: ref_table.clone(),
                            column: ref_column.clone(),
                        });

                    ColumnSchema {
                        name: name.clone(),
                        data_type: data_type.clone(),
                        is_primary_key: None,
                        foreign_key,
                    }
                })
                .collect();

            if column_schemas.is_empty() {
                continue;
            }

            let mut constraints_by_table: BTreeMap<String, BTreeMap<String, String>> =
                BTreeMap::new();
            for ((from_table, from_column), (to_table, to_column)) in
                ctx.implied_foreign_keys.iter()
            {
                if from_table == canonical {
                    constraints_by_table
                        .entry(to_table.clone())
                        .or_default()
                        .insert(from_column.clone(), to_column.clone());
                }
            }

            let constraints: Vec<TableConstraintInfo> = constraints_by_table
                .into_iter()
                .map(|(referenced_table, column_map)| TableConstraintInfo {
                    constraint_type: ConstraintType::ForeignKey,
                    columns: column_map.keys().cloned().collect(),
                    referenced_table: Some(referenced_table),
                    referenced_columns: Some(column_map.values().cloned().collect()),
                })
                .collect();

            self.register_implied_schema_with_constraints(
                ctx,
                canonical,
                column_schemas,
                constraints,
                false,
                SOURCE_TABLE_STATEMENT_TYPE,
            );
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

        // Mark as DDL-seeded to prevent schema overwriting by later queries.
        // CTAS derives complete schema from its query projection, so it should
        // not be overwritten by partial column sets from subsequent statements.
        self.schema.mark_as_ddl_seeded(&canonical);

        // Column-level lineage from analyze_query handles the data flow edges.
        // No need to create redundant table-to-table edges here.
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

        // Mark as DDL-seeded to prevent schema overwriting by later queries.
        // Views derive complete schema from their query projection, so they should
        // not be overwritten by partial column sets from subsequent statements.
        self.schema.mark_as_ddl_seeded(&canonical);

        // Column-level lineage from analyze_query handles the data flow edges.
        // No need to create redundant table-to-table edges here.
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
