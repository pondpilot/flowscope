use crate::error::ParseError;
use crate::types::*;
use serde_json::json;
use sqlparser::ast::{
    self, Assignment, Expr, FromTable, FunctionArg, FunctionArgExpr, ObjectName, Query, SelectItem,
    SetExpr, Statement, TableFactor, TableWithJoins,
};
use std::collections::{HashMap, HashSet};
#[cfg(feature = "tracing")]
use tracing::info_span;

mod context;
pub mod helpers;
mod input;
mod resolution;
mod global;
mod diagnostics;

use context::{ColumnRef, OutputColumn, StatementContext};
use helpers::{
    classify_query_type, extract_simple_name, generate_column_node_id, generate_edge_id,
    generate_node_id, is_simple_column_ref,
};
use input::{collect_statements, StatementInput};

/// Main entry point for SQL analysis
pub fn analyze(request: &AnalyzeRequest) -> AnalyzeResult {
    #[cfg(feature = "tracing")]
    let _span = info_span!("analyze_request", statement_count = %request.sql.matches(';').count() + 1).entered();
    let mut analyzer = Analyzer::new(request);
    analyzer.analyze()
}

/// Internal analyzer state
pub(super) struct Analyzer<'a> {
    pub(super) request: &'a AnalyzeRequest,
    pub(super) issues: Vec<Issue>,
    pub(super) statement_lineages: Vec<StatementLineage>,
    /// Track which tables are produced by which statement (for cross-statement linking)
    pub(super) produced_tables: HashMap<String, usize>,
    /// Track which tables are consumed by which statements
    pub(super) consumed_tables: HashMap<String, Vec<usize>>,
    /// All discovered tables across statements (for global lineage)
    pub(super) all_tables: HashSet<String>,
    /// All discovered CTEs
    pub(super) all_ctes: HashSet<String>,
    /// Known tables from schema metadata (for validation)
    pub(super) known_tables: HashSet<String>,
    /// Schema lookup: table canonical name -> table schema info
    pub(super) schema_tables: HashMap<String, SchemaTable>,
    /// Whether column lineage is enabled
    pub(super) column_lineage_enabled: bool,
    /// Default catalog for unqualified identifiers
    pub(super) default_catalog: Option<String>,
    /// Default schema for unqualified identifiers
    pub(super) default_schema: Option<String>,
    /// Ordered search path entries
    pub(super) search_path: Vec<SearchPathEntry>,
}

#[derive(Debug, Clone)]
pub(super) struct SearchPathEntry {
    catalog: Option<String>,
    schema: String,
}

#[derive(Debug, Clone)]
struct TableResolution {
    canonical: String,
    matched_schema: bool,
}

impl<'a> Analyzer<'a> {
    fn new(request: &'a AnalyzeRequest) -> Self {
        // Check if column lineage is enabled (default: true)
        let column_lineage_enabled = request
            .options
            .as_ref()
            .and_then(|o| o.enable_column_lineage)
            .unwrap_or(true);

        let mut analyzer = Self {
            request,
            issues: Vec::new(),
            statement_lineages: Vec::new(),
            produced_tables: HashMap::new(),
            consumed_tables: HashMap::new(),
            all_tables: HashSet::new(),
            all_ctes: HashSet::new(),
            known_tables: HashSet::new(),
            schema_tables: HashMap::new(),
            column_lineage_enabled,
            default_catalog: None,
            default_schema: None,
            search_path: Vec::new(),
        };

        analyzer.initialize_schema_metadata();

        analyzer
    }

    fn analyze(&mut self) -> AnalyzeResult {
        let (all_statements, mut preflight_issues) = collect_statements(self.request);
        self.issues.append(&mut preflight_issues);

        if all_statements.is_empty() {
            return self.build_result();
        }

        // Analyze all statements
        for (index, StatementInput { statement, source_name }) in all_statements.into_iter().enumerate() {
            #[cfg(feature = "tracing")]
            let _stmt_span = info_span!(
                "analyze_statement",
                index,
                source = source_name.as_deref().unwrap_or("inline"),
                stmt_type = ?statement
            )
            .entered();
            match self.analyze_statement(index, &statement, source_name) {
                Ok(lineage) => {
                    self.statement_lineages.push(lineage);
                }
                Err(e) => {
                    self.issues.push(
                        Issue::error(issue_codes::PARSE_ERROR, e.to_string()).with_statement(index),
                    );
                }
            }
        }

        self.build_result()
    }

    fn analyze_statement(
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
                    self.analyze_create_table_as(&mut ctx, &create.name, query);
                    "CREATE_TABLE_AS".to_string()
                } else {
                    self.analyze_create_table(&mut ctx, &create.name);
                    "CREATE_TABLE".to_string()
                }
            }
            Statement::CreateView { name, query, .. } => {
                self.analyze_create_view(&mut ctx, name, query);
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
                on: _,
                clauses: _,
            } => {
                self.analyze_merge(&mut ctx, *into, table, source);
                "MERGE".to_string()
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

        Ok(StatementLineage {
            statement_index: index,
            statement_type,
            source_name,
            nodes: ctx.nodes,
            edges: ctx.edges,
            span: None,
        })
    }

    fn analyze_query(
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
        for cte in ctes {
            let cte_name = cte.alias.name.to_string();

            // Check for recursive CTE
            if query.with.as_ref().map(|w| w.recursive).unwrap_or(false) {
                self.issues.push(
                    Issue::warning(
                        issue_codes::UNSUPPORTED_RECURSIVE_CTE,
                        format!("Recursive CTE '{cte_name}' detected - lineage may be incomplete"),
                    )
                    .with_statement(ctx.statement_index),
                );
            }

            // Create CTE node
            let cte_id = ctx.add_node(Node {
                id: generate_node_id("cte", &cte_name),
                node_type: NodeType::Cte,
                label: cte_name.clone(),
                qualified_name: Some(cte_name.clone()),
                expression: None,
                span: None,
                metadata: None,
            });

            // Register CTE for resolution
            ctx.cte_definitions.insert(cte_name.clone(), cte_id.clone());
            self.all_ctes.insert(cte_name.clone());

            // Analyze CTE body
            self.analyze_query_body(ctx, &cte.query.body, Some(&cte_id));

            // Capture CTE columns for lineage linking
            let columns = std::mem::take(&mut ctx.output_columns);
            ctx.cte_columns.insert(cte_name, columns);
        }

        // Analyze main query body
        self.analyze_query_body(ctx, &query.body, target_node);
    }

    fn analyze_query_body(
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
            SetExpr::Values(_) => {
                // VALUES clause doesn't have table sources
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
        // Analyze FROM clause first to register tables and aliases
        for table_with_joins in &select.from {
            self.analyze_table_with_joins(ctx, table_with_joins, target_node);
        }

        // Analyze columns if column lineage is enabled
        if self.column_lineage_enabled {
            self.analyze_select_columns(ctx, select, target_node);
        }
    }

    fn analyze_select_columns(
        &mut self,
        ctx: &mut StatementContext,
        select: &ast::Select,
        target_node: Option<&str>,
    ) {
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
                    self.add_output_column(ctx, &name, sources, expr_text, target_node);
                }
                SelectItem::ExprWithAlias { expr, alias } => {
                    let sources = self.extract_column_refs(expr);
                    let name = alias.value.clone();
                    let expr_text = if is_simple_column_ref(expr) {
                        None
                    } else {
                        Some(expr.to_string())
                    };
                    self.add_output_column(ctx, &name, sources, expr_text, target_node);
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

        // Also extract column refs from WHERE, GROUP BY, HAVING for completeness
        if let Some(ref where_clause) = select.selection {
            self.analyze_expression(ctx, where_clause);
        }

        // Handle GROUP BY
        match &select.group_by {
            ast::GroupByExpr::Expressions(exprs, _) => {
                for group_by in exprs {
                    self.analyze_expression(ctx, group_by);
                }
            }
            ast::GroupByExpr::All(_) => {}
        }

        if let Some(ref having) = select.having {
            self.analyze_expression(ctx, having);
        }
    }

    fn extract_column_refs(&self, expr: &Expr) -> Vec<ColumnRef> {
        let mut refs = Vec::new();
        self.collect_column_refs(expr, &mut refs);
        refs
    }

    fn collect_column_refs(&self, expr: &Expr, refs: &mut Vec<ColumnRef>) {
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
                self.collect_column_refs(left, refs);
                self.collect_column_refs(right, refs);
            }
            Expr::UnaryOp { expr, .. } => {
                self.collect_column_refs(expr, refs);
            }
            Expr::Function(func) => match &func.args {
                ast::FunctionArguments::List(arg_list) => {
                    for arg in &arg_list.args {
                        match arg {
                            FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => {
                                self.collect_column_refs(e, refs);
                            }
                            FunctionArg::Named { arg, .. } => {
                                if let FunctionArgExpr::Expr(e) = arg {
                                    self.collect_column_refs(e, refs);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                ast::FunctionArguments::Subquery(_) => {}
                ast::FunctionArguments::None => {}
            },
            Expr::Case {
                operand,
                conditions,
                results,
                else_result,
            } => {
                if let Some(op) = operand {
                    self.collect_column_refs(op, refs);
                }
                for cond in conditions {
                    self.collect_column_refs(cond, refs);
                }
                for res in results {
                    self.collect_column_refs(res, refs);
                }
                if let Some(el) = else_result {
                    self.collect_column_refs(el, refs);
                }
            }
            Expr::Cast { expr, .. } => {
                self.collect_column_refs(expr, refs);
            }
            Expr::Nested(inner) => {
                self.collect_column_refs(inner, refs);
            }
            Expr::Subquery(_) => {
                // Subquery columns are handled separately
            }
            Expr::InList { expr, list, .. } => {
                self.collect_column_refs(expr, refs);
                for item in list {
                    self.collect_column_refs(item, refs);
                }
            }
            Expr::Between {
                expr, low, high, ..
            } => {
                self.collect_column_refs(expr, refs);
                self.collect_column_refs(low, refs);
                self.collect_column_refs(high, refs);
            }
            Expr::IsNull(e) | Expr::IsNotNull(e) => {
                self.collect_column_refs(e, refs);
            }
            Expr::IsFalse(e) | Expr::IsNotFalse(e) | Expr::IsTrue(e) | Expr::IsNotTrue(e) => {
                self.collect_column_refs(e, refs);
            }
            Expr::Like { expr, pattern, .. } | Expr::ILike { expr, pattern, .. } => {
                self.collect_column_refs(expr, refs);
                self.collect_column_refs(pattern, refs);
            }
            Expr::Tuple(exprs) => {
                for e in exprs {
                    self.collect_column_refs(e, refs);
                }
            }
            Expr::Extract { expr, .. } => {
                self.collect_column_refs(expr, refs);
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
                .unwrap_or_else(|| format!("col_{}", index)),
            Expr::Function(func) => func.name.to_string().to_lowercase(),
            _ => format!("col_{}", index),
        }
    }

    fn add_output_column(
        &mut self,
        ctx: &mut StatementContext,
        name: &str,
        sources: Vec<ColumnRef>,
        expression: Option<String>,
        target_node: Option<&str>,
    ) {
        let normalized_name = self.normalize_identifier(name);
        let node_id = generate_column_node_id(target_node, &normalized_name);

        // Create column node
        let col_node = Node {
            id: node_id.clone(),
            node_type: NodeType::Column,
            label: normalized_name.clone(),
            qualified_name: None, // Will be set if we have target table
            expression: expression.clone(),
            span: None,
            metadata: None,
        };
        ctx.add_node(col_node);

        // Create ownership edge if we have a target (table/CTE being written to)
        if let Some(target) = target_node {
            let edge_id = generate_edge_id(target, &node_id);
            if !ctx.edge_ids.contains(&edge_id) {
                ctx.add_edge(Edge {
                    id: edge_id,
                    from: target.to_string(),
                    to: node_id.clone(),
                    edge_type: EdgeType::Ownership,
                    expression: None,
                    operation: None,
                    metadata: None,
                });
            }
        }

        // Create data flow edges from source columns
        for source in &sources {
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

                // Fallback to generating a new ID (standard table column)
                let source_col_id = source_col_id.unwrap_or_else(|| {
                    generate_column_node_id(
                        Some(&generate_node_id("table", table_canonical)),
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
                };
                ctx.add_node(source_col_node);

                // Create ownership edge from table to source column
                let table_node_id = generate_node_id("table", table_canonical);
                let ownership_edge_id = generate_edge_id(&table_node_id, &source_col_id);
                if !ctx.edge_ids.contains(&ownership_edge_id) {
                    ctx.add_edge(Edge {
                        id: ownership_edge_id,
                        from: table_node_id,
                        to: source_col_id.clone(),
                        edge_type: EdgeType::Ownership,
                        expression: None,
                        operation: None,
                        metadata: None,
                    });
                }

                // Create data flow edge from source to output
                let edge_type = if expression.is_some() {
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
                        expression: expression.clone(),
                        operation: None,
                        metadata: None,
                    });
                }
            }
        }

        // Record output column
        ctx.output_columns.push(OutputColumn {
            name: normalized_name,
            sources,
            expression,
            node_id,
        });
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
            let columns_to_add: Option<Vec<(String, String, String)>> = self
                .schema_tables
                .get(&table_canonical)
                .map(|schema_table| {
                    schema_table
                        .columns
                        .iter()
                        .map(|col| {
                            (
                                col.name.clone(),
                                table_canonical.clone(),
                                table_canonical.clone(),
                            )
                        })
                        .collect()
                });

            if let Some(columns) = columns_to_add {
                // Expand from schema
                for (col_name, table, resolved_table) in columns {
                    let sources = vec![ColumnRef {
                        table: Some(table),
                        column: col_name.clone(),
                        resolved_table: Some(resolved_table),
                    }];
                    self.add_output_column(ctx, &col_name, sources, None, target_node);
                }
            } else {
                // No schema available - emit approximate lineage warning
                self.issues.push(
                    Issue::info(
                        issue_codes::APPROXIMATE_LINEAGE,
                        format!(
                            "SELECT * from '{}' - column list unknown without schema metadata",
                            table_canonical
                        ),
                    )
                    .with_statement(ctx.statement_index),
                );
            }
        }
    }

    fn resolve_table_alias(
        &self,
        ctx: &StatementContext,
        qualifier: Option<&str>,
    ) -> Option<String> {
        match qualifier {
            Some(q) => {
                // Check if it's an alias
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

    /// Resolve which table a column belongs to.
    /// If qualifier is provided, resolve via alias. Otherwise, try to infer from tables in scope.
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
        let tables_in_scope: Vec<_> = ctx.table_node_ids.keys().cloned().collect();

        // If only one table in scope, assume column belongs to it
        if tables_in_scope.len() == 1 {
            return Some(tables_in_scope[0].clone());
        }

        let normalized_col = self.normalize_identifier(column);

        // Collect candidates using CTE output columns and schema metadata
        let mut candidate_tables: Vec<String> = Vec::new();
        for table_canonical in &tables_in_scope {
            if let Some(cte_cols) = ctx.cte_columns.get(table_canonical) {
                if cte_cols.iter().any(|c| c.name == normalized_col) {
                    candidate_tables.push(table_canonical.clone());
                    continue;
                }
            }

            if let Some(schema_table) = self.schema_tables.get(table_canonical) {
                if schema_table
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
                // Ambiguous because we have multiple tables in scope but no way to disambiguate.
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
                // Column exists in multiple tables — require explicit qualifier.
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

    fn analyze_table_with_joins(
        &mut self,
        ctx: &mut StatementContext,
        table_with_joins: &TableWithJoins,
        target_node: Option<&str>,
    ) {
        // Analyze main relation
        self.analyze_table_factor(ctx, &table_with_joins.relation, target_node);

        // Analyze joins
        for join in &table_with_joins.joins {
            let join_type = match &join.join_operator {
                ast::JoinOperator::Inner(_) => "INNER_JOIN",
                ast::JoinOperator::LeftOuter(_) => "LEFT_JOIN",
                ast::JoinOperator::RightOuter(_) => "RIGHT_JOIN",
                ast::JoinOperator::FullOuter(_) => "FULL_JOIN",
                ast::JoinOperator::CrossJoin => "CROSS_JOIN",
                ast::JoinOperator::LeftSemi(_) => "LEFT_SEMI_JOIN",
                ast::JoinOperator::RightSemi(_) => "RIGHT_SEMI_JOIN",
                ast::JoinOperator::LeftAnti(_) => "LEFT_ANTI_JOIN",
                ast::JoinOperator::RightAnti(_) => "RIGHT_ANTI_JOIN",
                ast::JoinOperator::CrossApply => "CROSS_APPLY",
                ast::JoinOperator::OuterApply => "OUTER_APPLY",
                ast::JoinOperator::AsOf { .. } => "AS_OF_JOIN",
            };
            ctx.last_operation = Some(join_type.to_string());
            self.analyze_table_factor(ctx, &join.relation, target_node);
        }
    }

    fn analyze_table_factor(
        &mut self,
        ctx: &mut StatementContext,
        table_factor: &TableFactor,
        target_node: Option<&str>,
    ) {
        match table_factor {
            TableFactor::Table { name, alias, .. } => {
                let table_name = name.to_string();

                let canonical = self.add_source_table(ctx, &table_name, target_node);

                // Register alias if present
                if let (Some(a), Some(canonical_name)) = (alias, canonical) {
                    ctx.table_aliases.insert(a.name.to_string(), canonical_name);
                }
            }
            TableFactor::Derived {
                subquery, alias, ..
            } => {
                // Subquery - analyze recursively
                self.analyze_query(ctx, subquery, target_node);

                if let Some(a) = alias {
                    // Register subquery alias
                    ctx.subquery_aliases.insert(a.name.to_string());
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

    fn add_source_table(
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
                ctx.table_node_ids
                    .insert(table_name.to_string(), id.clone());
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
                            format!(
                                "Table '{}' could not be resolved using provided schema metadata or search path",
                                canonical
                            ),
                        )
                        .with_statement(ctx.statement_index),
                    );
                }

                ctx.add_node(Node {
                    id: id.clone(),
                    node_type: NodeType::Table,
                    label: extract_simple_name(&canonical),
                    qualified_name: Some(canonical.clone()),
                    expression: None,
                    span: None,
                    metadata,
                });
            }

            self.all_tables.insert(canonical.clone());
            self.consumed_tables
                .entry(canonical.clone())
                .or_default()
                .push(ctx.statement_index);

            // Track table node ID for column ownership
            ctx.table_node_ids.insert(canonical, id.clone());

            Some(id)
        };

        // Create edge to target if specified
        if let (Some(target), Some(source_id)) = (target_node, node_id.clone()) {
            let edge_id = generate_edge_id(&source_id, target);
            if !ctx.edge_ids.contains(&edge_id) {
                ctx.add_edge(Edge {
                    id: edge_id,
                    from: source_id,
                    to: target.to_string(),
                    edge_type: EdgeType::DataFlow,
                    expression: None,
                    operation: ctx.last_operation.clone(),
                    metadata: None,
                });
            }
        }

        canonical_for_alias
    }

    fn analyze_insert(&mut self, ctx: &mut StatementContext, insert: &ast::Insert) {
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
        });

        self.all_tables.insert(canonical.clone());
        self.produced_tables.insert(canonical, ctx.statement_index);

        // Analyze source - check the body of the insert
        if let Some(ref source_body) = insert.source {
            self.analyze_query_body(ctx, &source_body.body, Some(&target_id));
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

    fn analyze_create_table_as(
        &mut self,
        ctx: &mut StatementContext,
        table_name: &ObjectName,
        query: &Query,
    ) {
        let target_name = table_name.to_string();
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
        });

        self.all_tables.insert(canonical.clone());
        self.produced_tables.insert(canonical, ctx.statement_index);

        // Analyze source query
        self.analyze_query(ctx, query, Some(&target_id));

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
                    metadata: None,
                });
            }
        }
    }

    fn analyze_create_table(&mut self, ctx: &mut StatementContext, name: &ObjectName) {
        let target_name = name.to_string();
        let canonical = self.normalize_table_name(&target_name);

        // Create target table node
        ctx.add_node(Node {
            id: generate_node_id("table", &canonical),
            node_type: NodeType::Table,
            label: extract_simple_name(&target_name),
            qualified_name: Some(canonical.clone()),
            expression: None,
            span: None,
            metadata: None,
        });

        self.all_tables.insert(canonical.clone());
        self.produced_tables
            .insert(canonical, ctx.statement_index);
    }

    fn analyze_create_view(&mut self, ctx: &mut StatementContext, name: &ObjectName, query: &Query) {
        let target_name = name.to_string();
        let canonical = self.normalize_table_name(&target_name);

        // Create target view/table node
        let target_id = ctx.add_node(Node {
            id: generate_node_id("table", &canonical),
            node_type: NodeType::Table, // Represent views as tables for now
            label: extract_simple_name(&target_name),
            qualified_name: Some(canonical.clone()),
            expression: None,
            span: None,
            metadata: None,
        });

        self.all_tables.insert(canonical.clone());
        self.produced_tables
            .insert(canonical, ctx.statement_index);

        // Analyze source query
        self.analyze_query(ctx, query, Some(&target_id));

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
                    metadata: None,
                });
            }
        }
    }

    fn analyze_expression(&mut self, ctx: &mut StatementContext, expr: &Expr) {
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

    fn analyze_update(
        &mut self,
        ctx: &mut StatementContext,
        table: &TableWithJoins,
        assignments: &[Assignment],
        from: &Option<TableWithJoins>,
        selection: &Option<Expr>,
    ) {
        // 1. Analyze the target table
        self.analyze_table_with_joins(ctx, table, None);

        // Register this table as being "produced" (modified)
        if let TableFactor::Table { name, .. } = &table.relation {
            let canonical = self.normalize_table_name(&name.to_string());
            self.produced_tables.insert(canonical, ctx.statement_index);
        }

        // 2. Analyze FROM clause (Postgres style)
        if let Some(from_table) = from {
            self.analyze_table_with_joins(ctx, from_table, None);
        }

        // 3. Analyze assignments (SET clause)
        for assignment in assignments {
            self.analyze_expression(ctx, &assignment.value);
        }

        // 4. Analyze selection (WHERE clause)
        if let Some(expr) = selection {
            self.analyze_expression(ctx, expr);
        }
    }

    fn analyze_delete(
        &mut self,
        ctx: &mut StatementContext,
        tables: &[ObjectName],
        from: &FromTable,
        using: &Option<Vec<TableWithJoins>>,
        selection: &Option<Expr>,
    ) {
        // 1. Analyze sources (FROM + USING)
        match from {
            FromTable::WithFromKeyword(ts) | FromTable::WithoutKeyword(ts) => {
                for t in ts {
                    self.analyze_table_with_joins(ctx, t, None);
                }
            }
        }
        if let Some(us) = using {
            for t in us {
                self.analyze_table_with_joins(ctx, t, None);
            }
        }

        // 2. Identify and register targets
        if !tables.is_empty() {
            // MySQL multi-table delete syntax
            for obj in tables {
                let name = obj.to_string();
                // Try to resolve against aliases found in FROM
                let canonical = if let Some(c) = ctx.table_aliases.get(&name) {
                    c.clone()
                } else {
                    self.canonicalize_table_reference(&name).canonical
                };
                self.produced_tables.insert(canonical, ctx.statement_index);
            }
        } else {
            // Standard SQL: first table in FROM is target
            let ts = match from {
                FromTable::WithFromKeyword(ts) | FromTable::WithoutKeyword(ts) => ts,
            };
            if let Some(first) = ts.first() {
                if let TableFactor::Table { name, alias, .. } = &first.relation {
                    let table_name = name.to_string();
                    let lookup = alias
                        .as_ref()
                        .map(|a| a.name.to_string())
                        .unwrap_or(table_name.clone());

                    let canonical = if let Some(c) = ctx.table_aliases.get(&lookup) {
                        c.clone()
                    } else {
                        self.canonicalize_table_reference(&table_name).canonical
                    };
                    self.produced_tables.insert(canonical, ctx.statement_index);
                }
            }
        }

        // 3. Analyze selection
        if let Some(expr) = selection {
            self.analyze_expression(ctx, expr);
        }
    }

    fn analyze_merge(
        &mut self,
        ctx: &mut StatementContext,
        _into: bool,
        table: &TableFactor,
        source: &TableFactor,
    ) {
        // 1. Analyze Target Table
        self.analyze_table_factor(ctx, table, None);

        if let TableFactor::Table { name, .. } = table {
            let canonical = self.normalize_table_name(&name.to_string());
            self.produced_tables.insert(canonical, ctx.statement_index);
        }

        // 2. Analyze Source Table
        self.analyze_table_factor(ctx, source, None);
    }

}


#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(sql: &str) -> AnalyzeRequest {
        AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        }
    }

    #[test]
    fn test_simple_select() {
        let request = make_request("SELECT * FROM users");
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 1);
        assert_eq!(result.statements[0].statement_type, "SELECT");
        assert_eq!(result.statements[0].nodes.len(), 1);
        assert_eq!(result.statements[0].nodes[0].label, "users");
        assert_eq!(result.statements[0].nodes[0].node_type, NodeType::Table);
        assert!(!result.summary.has_errors);
    }

    #[test]
    fn test_select_with_join() {
        let request = make_request("SELECT * FROM users u JOIN orders o ON u.id = o.user_id");
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 1);
        assert_eq!(result.statements[0].nodes.len(), 2);

        let labels: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .map(|n| n.label.as_str())
            .collect();
        assert!(labels.contains(&"users"));
        assert!(labels.contains(&"orders"));
    }

    #[test]
    fn test_cte_analysis() {
        let request = make_request(
            r#"
            WITH active_users AS (
                SELECT * FROM users WHERE active = true
            )
            SELECT * FROM active_users
        "#,
        );
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 1);
        assert_eq!(result.statements[0].statement_type, "WITH");

        let cte_nodes: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Cte)
            .collect();
        assert_eq!(cte_nodes.len(), 1);
        assert_eq!(cte_nodes[0].label, "active_users");
    }

    #[test]
    fn test_insert_select() {
        let request = make_request("INSERT INTO archive SELECT * FROM users WHERE deleted = true");
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 1);
        assert_eq!(result.statements[0].statement_type, "INSERT");
        assert!(result.statements[0].nodes.len() >= 2);

        // Should have edge from users to archive
        assert!(!result.statements[0].edges.is_empty());
    }

    #[test]
    fn test_create_table_as() {
        let request = make_request("CREATE TABLE users_backup AS SELECT * FROM users");
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 1);
        assert_eq!(result.statements[0].statement_type, "CREATE_TABLE_AS");
        assert!(result.statements[0].nodes.len() >= 2);
        assert!(!result.statements[0].edges.is_empty());
    }

    #[test]
    fn test_union_query() {
        let request = make_request("SELECT id FROM users UNION ALL SELECT id FROM admins");
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 1);
        assert_eq!(result.statements[0].statement_type, "UNION");
        // Count only table nodes (columns are also added now)
        let table_count = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Table)
            .count();
        assert_eq!(table_count, 2);
    }

    #[test]
    fn test_subquery() {
        let request = make_request("SELECT * FROM (SELECT id, name FROM users) AS subq");
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 1);
        // Should find the users table in the subquery
        let has_users = result.statements[0]
            .nodes
            .iter()
            .any(|n| n.label == "users");
        assert!(has_users);
    }

    #[test]
    fn test_multiple_statements() {
        let request = make_request(
            r#"
            CREATE TABLE temp AS SELECT * FROM users;
            INSERT INTO archive SELECT * FROM temp;
        "#,
        );
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 2);
        assert_eq!(result.summary.statement_count, 2);
    }

    #[test]
    fn test_cross_statement_lineage() {
        let request = make_request(
            r#"
            CREATE TABLE temp AS SELECT * FROM users;
            SELECT * FROM temp;
        "#,
        );
        let result = analyze(&request);

        // Should detect cross-statement dependency
        let cross_edges: Vec<_> = result
            .global_lineage
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::CrossStatement)
            .collect();
        assert!(!cross_edges.is_empty());
    }

    #[test]
    fn test_invalid_sql() {
        let request = make_request("SELECT * FROM");
        let result = analyze(&request);

        assert!(result.summary.has_errors);
        assert!(result
            .issues
            .iter()
            .any(|i| i.code == issue_codes::PARSE_ERROR));
    }

    #[test]
    fn test_ambiguous_unqualified_column_emits_issue() {
        let request = make_request(
            r#"
            SELECT id
            FROM users u
            JOIN orders o ON u.id = o.user_id
        "#,
        );
        let result = analyze(&request);

        assert!(
            result
                .issues
                .iter()
                .any(|i| i.code == issue_codes::UNRESOLVED_REFERENCE),
            "expected ambiguous column to produce UNRESOLVED_REFERENCE warning"
        );
    }

    #[test]
    fn test_dialect_case_sensitivity() {
        // Postgres normalizes to lowercase
        let pg_request = AnalyzeRequest {
            sql: "SELECT * FROM Users".to_string(),
            files: None,
            dialect: Dialect::Postgres,
            source_name: None,
            options: None,
            schema: None,
        };
        let pg_result = analyze(&pg_request);
        let pg_name = &pg_result.statements[0].nodes[0].qualified_name;
        assert_eq!(pg_name.as_deref(), Some("users"));

        // Snowflake normalizes to uppercase
        let sf_request = AnalyzeRequest {
            sql: "SELECT * FROM Users".to_string(),
            files: None,
            dialect: Dialect::Snowflake,
            source_name: None,
            options: None,
            schema: None,
        };
        let sf_result = analyze(&sf_request);
        let sf_name = &sf_result.statements[0].nodes[0].qualified_name;
        assert_eq!(sf_name.as_deref(), Some("USERS"));
    }

    #[test]
    fn test_global_lineage_deduplication() {
        let request = make_request(
            r#"
            SELECT * FROM users;
            SELECT * FROM users JOIN orders ON users.id = orders.user_id;
        "#,
        );
        let result = analyze(&request);

        // users table should appear once in global lineage (deduplicated)
        let users_nodes: Vec<_> = result
            .global_lineage
            .nodes
            .iter()
            .filter(|n| n.label == "users")
            .collect();
        assert_eq!(users_nodes.len(), 1);

        // But should have references to both statements
        assert_eq!(users_nodes[0].statement_refs.len(), 2);
    }

    #[test]
    fn test_summary_counts() {
        let request = make_request(
            r#"
            SELECT * FROM users;
            SELECT * FROM orders;
        "#,
        );
        let result = analyze(&request);

        assert_eq!(result.summary.statement_count, 2);
        assert_eq!(result.summary.table_count, 2);
        assert_eq!(result.summary.issue_count.errors, 0);
        assert!(!result.summary.has_errors);
    }

    #[test]
    fn test_unknown_table_with_schema() {
        let request = AnalyzeRequest {
            sql: "SELECT * FROM unknown_table".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                default_catalog: None,
                default_schema: None,
                search_path: None,
                case_sensitivity: None,
                tables: vec![crate::types::SchemaTable {
                    catalog: None,
                    schema: None,
                    name: "users".to_string(),
                    columns: vec![],
                }],
            }),
        };
        let result = analyze(&request);

        // Should emit UNRESOLVED_REFERENCE warning
        assert!(result
            .issues
            .iter()
            .any(|i| i.code == issue_codes::UNRESOLVED_REFERENCE));
    }

    #[test]
    fn test_known_table_no_warning() {
        let request = AnalyzeRequest {
            sql: "SELECT * FROM users".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                default_catalog: None,
                default_schema: None,
                search_path: None,
                case_sensitivity: None,
                tables: vec![crate::types::SchemaTable {
                    catalog: None,
                    schema: None,
                    name: "users".to_string(),
                    columns: vec![],
                }],
            }),
        };
        let result = analyze(&request);

        // Should NOT emit UNRESOLVED_REFERENCE warning
        assert!(!result
            .issues
            .iter()
            .any(|i| i.code == issue_codes::UNRESOLVED_REFERENCE));
    }

    #[test]
    fn test_invalid_request_without_sql_or_files() {
        let request = AnalyzeRequest {
            sql: "".to_string(),
            files: Some(vec![]),
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        };

        let result = analyze(&request);

        assert!(result.summary.has_errors);
        assert_eq!(result.statements.len(), 0);
        assert!(result
            .issues
            .iter()
            .any(|i| i.code == issue_codes::INVALID_REQUEST));
    }

    #[test]
    fn test_files_can_partially_succeed_when_one_fails() {
        let request = AnalyzeRequest {
            sql: "".to_string(),
            files: Some(vec![
                crate::types::FileSource {
                    name: "good.sql".to_string(),
                    content: "SELECT * FROM users;".to_string(),
                },
                crate::types::FileSource {
                    name: "bad.sql".to_string(),
                    content: "SELECT FROM".to_string(),
                },
            ]),
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        };

        let result = analyze(&request);

        // One statement analyzed, one issue captured
        assert_eq!(result.statements.len(), 1);
        assert!(result
            .issues
            .iter()
            .any(|i| i.code == issue_codes::PARSE_ERROR && i.message.contains("bad.sql")));
        assert!(result.summary.has_errors);
    }

    #[test]
    fn test_search_path_resolves_unqualified_table() {
        let request = AnalyzeRequest {
            sql: "SELECT * FROM orders".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                default_catalog: None,
                default_schema: Some("analytics".to_string()),
                search_path: Some(vec![crate::types::SchemaNamespaceHint {
                    catalog: None,
                    schema: "analytics".to_string(),
                }]),
                case_sensitivity: None,
                tables: vec![crate::types::SchemaTable {
                    catalog: None,
                    schema: Some("analytics".to_string()),
                    name: "orders".to_string(),
                    columns: vec![],
                }],
            }),
        };
        let result = analyze(&request);

        assert!(!result
            .issues
            .iter()
            .any(|i| i.code == issue_codes::UNRESOLVED_REFERENCE));

        let table_node = result.statements[0]
            .nodes
            .iter()
            .find(|n| n.node_type == NodeType::Table)
            .expect("table node");
        assert_eq!(
            table_node.qualified_name.as_deref(),
            Some("analytics.orders")
        );
    }

    #[test]
    fn test_default_schema_used_without_search_path() {
        let request = AnalyzeRequest {
            sql: "SELECT * FROM orders".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                default_catalog: None,
                default_schema: Some("analytics".to_string()),
                search_path: None,
                case_sensitivity: None,
                tables: vec![crate::types::SchemaTable {
                    catalog: None,
                    schema: Some("analytics".to_string()),
                    name: "orders".to_string(),
                    columns: vec![],
                }],
            }),
        };
        let result = analyze(&request);

        assert!(!result
            .issues
            .iter()
            .any(|i| i.code == issue_codes::UNRESOLVED_REFERENCE));
    }

    #[test]
    fn test_placeholder_node_marked_for_unresolved_table() {
        let request = AnalyzeRequest {
            sql: "SELECT * FROM missing_table".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                default_catalog: None,
                default_schema: Some("analytics".to_string()),
                search_path: None,
                case_sensitivity: None,
                tables: vec![crate::types::SchemaTable {
                    catalog: None,
                    schema: Some("analytics".to_string()),
                    name: "orders".to_string(),
                    columns: vec![],
                }],
            }),
        };
        let result = analyze(&request);

        assert!(result
            .issues
            .iter()
            .any(|i| i.code == issue_codes::UNRESOLVED_REFERENCE));

        let placeholder = result.statements[0]
            .nodes
            .iter()
            .find(|n| n.label == "missing_table")
            .expect("missing table node");
        let placeholder_flag = placeholder
            .metadata
            .as_ref()
            .and_then(|m| m.get("placeholder"))
            .and_then(|v| v.as_bool());
        assert_eq!(placeholder_flag, Some(true));
    }

    #[test]
    fn test_insert_with_column_list() {
        let request =
            make_request("INSERT INTO users (id, name, email) SELECT id, name, email FROM staging");
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 1);
        assert_eq!(result.statements[0].statement_type, "INSERT");
        // Should have both tables
        let labels: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .map(|n| n.label.as_str())
            .collect();
        assert!(labels.contains(&"users"));
        assert!(labels.contains(&"staging"));
    }

    #[test]
    fn test_multiple_unions() {
        let request = make_request(
            "SELECT id FROM users UNION SELECT id FROM admins UNION SELECT id FROM guests",
        );
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 1);
        // Count only table nodes (columns are also added now)
        let table_count = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Table)
            .count();
        assert_eq!(table_count, 3);

        let labels: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Table)
            .map(|n| n.label.as_str())
            .collect();
        assert!(labels.contains(&"users"));
        assert!(labels.contains(&"admins"));
        assert!(labels.contains(&"guests"));
    }

    #[test]
    fn test_nested_subqueries() {
        let request = make_request(
            r#"
            SELECT * FROM (
                SELECT * FROM (
                    SELECT id, name FROM users
                ) AS inner_sq
            ) AS outer_sq
        "#,
        );
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 1);
        // Should find users table through nested subqueries
        let has_users = result.statements[0]
            .nodes
            .iter()
            .any(|n| n.label == "users");
        assert!(has_users);
    }

    #[test]
    fn test_empty_sql() {
        let request = make_request("");
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 0);
        assert_eq!(result.summary.statement_count, 0);
    }

    #[test]
    fn test_comment_only_sql() {
        let request = make_request("-- This is just a comment");
        let result = analyze(&request);

        // Comments only should result in empty statements
        assert_eq!(result.statements.len(), 0);
    }

    #[test]
    fn test_recursive_cte_warning() {
        let request = make_request(
            r#"
            WITH RECURSIVE cte AS (
                SELECT 1 AS n
                UNION ALL
                SELECT n + 1 FROM cte WHERE n < 10
            )
            SELECT * FROM cte
        "#,
        );
        let result = analyze(&request);

        // Should emit recursive CTE warning
        assert!(result
            .issues
            .iter()
            .any(|i| i.code == issue_codes::UNSUPPORTED_RECURSIVE_CTE));
    }

    #[test]
    fn test_partial_failure_continues() {
        let request = make_request(
            r#"
            SELECT * FROM users;
            SELECT * FROM;
            SELECT * FROM orders;
        "#,
        );
        let result = analyze(&request);

        // Should have parsed 2 statements successfully (first and third)
        // The middle invalid one causes parse error for the whole batch in sqlparser
        // So we expect parse error
        assert!(result.summary.has_errors);
    }

    // =====================================================
    // Column Lineage Tests (Phase 2)
    // =====================================================

    #[test]
    fn test_column_lineage_simple() {
        let request = make_request("SELECT id, name FROM users");
        let result = analyze(&request);

        // Should have table node and column nodes
        let table_nodes: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Table)
            .collect();
        let column_nodes: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        assert_eq!(table_nodes.len(), 1);
        assert!(column_nodes.len() >= 2); // at least id and name
    }

    #[test]
    fn test_column_lineage_with_alias() {
        let request = make_request("SELECT id AS user_id, name AS user_name FROM users");
        let result = analyze(&request);

        let column_nodes: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        // Should have columns with aliased names
        let labels: Vec<_> = column_nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(labels.contains(&"user_id") || labels.contains(&"user_name"));
    }

    #[test]
    fn test_column_lineage_with_expression() {
        let request =
            make_request("SELECT CONCAT(first_name, ' ', last_name) AS full_name FROM users");
        let result = analyze(&request);

        let column_nodes: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        // Should have output column with expression
        let full_name_col = column_nodes.iter().find(|n| n.label == "full_name");
        assert!(full_name_col.is_some());
        // The expression should be recorded
        if let Some(col) = full_name_col {
            assert!(col.expression.is_some());
        }
    }

    #[test]
    fn test_column_lineage_derivation_edge() {
        let request = make_request("SELECT u.id + 1 AS incremented_id FROM users u");
        let result = analyze(&request);

        // Should have derivation edge for computed column
        let derivation_edges: Vec<_> = result.statements[0]
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Derivation)
            .collect();

        assert!(!derivation_edges.is_empty());
    }

    #[test]
    fn test_column_lineage_data_flow_edge() {
        let request = make_request("SELECT u.id FROM users u");
        let result = analyze(&request);

        // Should have data flow edge for direct column passthrough
        let data_flow_edges: Vec<_> = result.statements[0]
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::DataFlow)
            .collect();

        assert!(!data_flow_edges.is_empty());
    }

    #[test]
    fn test_column_lineage_ownership_edge() {
        let request = make_request("SELECT u.id FROM users u");
        let result = analyze(&request);

        // Should have ownership edges from tables to their columns
        let ownership_edges: Vec<_> = result.statements[0]
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Ownership)
            .collect();

        assert!(!ownership_edges.is_empty());
    }

    #[test]
    fn test_column_lineage_join() {
        let request =
            make_request("SELECT u.id, o.order_id FROM users u JOIN orders o ON u.id = o.user_id");
        let result = analyze(&request);

        let column_nodes: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        // Should have columns from both tables
        assert!(column_nodes.len() >= 2);
    }

    #[test]
    fn test_column_lineage_disabled() {
        let request = AnalyzeRequest {
            sql: "SELECT id, name FROM users".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: Some(AnalysisOptions {
                enable_column_lineage: Some(false),
                graph_detail_level: None,
            }),
            schema: None,
        };
        let result = analyze(&request);

        // Should have no column nodes when disabled
        let column_nodes: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        assert_eq!(column_nodes.len(), 0);
    }

    #[test]
    fn test_column_lineage_with_schema() {
        let request = AnalyzeRequest {
            sql: "SELECT id, name FROM users".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                default_catalog: None,
                default_schema: None,
                search_path: None,
                case_sensitivity: None,
                tables: vec![crate::types::SchemaTable {
                    catalog: None,
                    schema: None,
                    name: "users".to_string(),
                    columns: vec![
                        crate::types::ColumnSchema {
                            name: "id".to_string(),
                            data_type: Some("integer".to_string()),
                        },
                        crate::types::ColumnSchema {
                            name: "name".to_string(),
                            data_type: Some("varchar".to_string()),
                        },
                    ],
                }],
            }),
        };
        let result = analyze(&request);

        // Should have columns
        let column_nodes: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        assert!(!column_nodes.is_empty());
        // No unknown column warnings since columns exist
        assert!(!result
            .issues
            .iter()
            .any(|i| i.code == issue_codes::UNKNOWN_COLUMN));
    }

    #[test]
    fn test_column_lineage_unknown_column() {
        let request = AnalyzeRequest {
            sql: "SELECT u.nonexistent FROM users u".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                default_catalog: None,
                default_schema: None,
                search_path: None,
                case_sensitivity: None,
                tables: vec![crate::types::SchemaTable {
                    catalog: None,
                    schema: None,
                    name: "users".to_string(),
                    columns: vec![crate::types::ColumnSchema {
                        name: "id".to_string(),
                        data_type: None,
                    }],
                }],
            }),
        };
        let result = analyze(&request);

        // Should emit unknown column warning
        assert!(result
            .issues
            .iter()
            .any(|i| i.code == issue_codes::UNKNOWN_COLUMN));
    }

    #[test]
    fn test_column_lineage_select_star_with_schema() {
        let request = AnalyzeRequest {
            sql: "SELECT * FROM users".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                default_catalog: None,
                default_schema: None,
                search_path: None,
                case_sensitivity: None,
                tables: vec![crate::types::SchemaTable {
                    catalog: None,
                    schema: None,
                    name: "users".to_string(),
                    columns: vec![
                        crate::types::ColumnSchema {
                            name: "id".to_string(),
                            data_type: None,
                        },
                        crate::types::ColumnSchema {
                            name: "name".to_string(),
                            data_type: None,
                        },
                        crate::types::ColumnSchema {
                            name: "email".to_string(),
                            data_type: None,
                        },
                    ],
                }],
            }),
        };
        let result = analyze(&request);

        // Should expand * to individual columns
        let column_nodes: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        // Should have 3 columns from expansion (id, name, email) plus 3 source columns
        assert!(column_nodes.len() >= 3);
    }

    #[test]
    fn test_column_lineage_select_star_without_schema() {
        let request = make_request("SELECT * FROM users");
        let result = analyze(&request);

        // Should emit approximate lineage warning
        assert!(result
            .issues
            .iter()
            .any(|i| i.code == issue_codes::APPROXIMATE_LINEAGE));
    }

    #[test]
    fn test_column_count_in_summary() {
        let request = make_request("SELECT id, name FROM users");
        let result = analyze(&request);

        // Summary should include column count
        assert!(result.summary.column_count > 0);
    }

    #[test]
    fn test_column_lineage_aggregate_function() {
        let request = make_request("SELECT COUNT(*), SUM(amount) AS total FROM orders");
        let result = analyze(&request);

        let column_nodes: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        // Should have columns for aggregates
        let labels: Vec<_> = column_nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(labels.contains(&"count") || labels.contains(&"total"));
    }

    #[test]
    fn test_column_lineage_case_expression() {
        let request = make_request(
            "SELECT CASE WHEN status = 'active' THEN 1 ELSE 0 END AS is_active FROM users",
        );
        let result = analyze(&request);

        let column_nodes: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        let is_active_col = column_nodes.iter().find(|n| n.label == "is_active");
        assert!(is_active_col.is_some());
        // Should have expression recorded
        if let Some(col) = is_active_col {
            assert!(col.expression.is_some());
        }
    }
}
