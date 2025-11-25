use crate::error::ParseError;
use crate::types::*;
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlparser::ast::{
    self, Assignment, ColumnDef, Expr, FromTable, FunctionArg, FunctionArgExpr, MergeAction,
    MergeClause, MergeInsertKind, ObjectName, Query, SelectItem, SetExpr, Statement, TableFactor,
    TableWithJoins,
};
use std::collections::{HashMap, HashSet};
#[cfg(feature = "tracing")]
use tracing::{info, info_span};

mod context;
mod diagnostics;
mod functions;
mod global;
pub mod helpers;
mod input;
mod resolution;

use context::{ColumnRef, OutputColumn, StatementContext};
use helpers::{
    classify_query_type, extract_simple_name, generate_column_node_id, generate_edge_id,
    generate_node_id, is_simple_column_ref, split_qualified_identifiers,
};
use input::{collect_statements, StatementInput};

/// Main entry point for SQL analysis
pub fn analyze(request: &AnalyzeRequest) -> AnalyzeResult {
    #[cfg(feature = "tracing")]
    let _span =
        info_span!("analyze_request", statement_count = %request.sql.matches(';').count() + 1)
            .entered();
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
    /// Tables from imported (user-provided) schema that should not be overwritten
    pub(super) imported_tables: HashSet<String>,
    /// Schema lookup: table canonical name -> table schema entry with metadata
    pub(super) schema_tables: HashMap<String, SchemaTableEntry>,
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

/// Schema table entry with origin metadata for tracking imported vs implied schema
#[derive(Debug, Clone)]
pub(super) struct SchemaTableEntry {
    pub(super) table: SchemaTable,
    pub(super) origin: SchemaOrigin,
    pub(super) source_statement_idx: Option<usize>,
    pub(super) updated_at: DateTime<Utc>,
    pub(super) temporary: bool,
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
            imported_tables: HashSet::new(),
            schema_tables: HashMap::new(),
            column_lineage_enabled,
            default_catalog: None,
            default_schema: None,
            search_path: Vec::new(),
        };

        analyzer.initialize_schema_metadata();

        analyzer
    }

    /// Check if implied schema capture is allowed (default: true)
    fn allow_implied(&self) -> bool {
        self.request
            .schema
            .as_ref()
            .map(|s| s.allow_implied)
            .unwrap_or(true)
    }

    /// Convert an AST JoinOperator to JoinType enum, also extracting the join condition.
    fn convert_join_operator(op: &ast::JoinOperator) -> (Option<JoinType>, Option<String>) {
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
    fn join_type_to_operation(join_type: Option<JoinType>) -> Option<String> {
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

    fn analyze(&mut self) -> AnalyzeResult {
        let (all_statements, mut preflight_issues) = collect_statements(self.request);
        self.issues.append(&mut preflight_issues);

        if all_statements.is_empty() {
            return self.build_result();
        }

        // Analyze all statements
        for (
            index,
            StatementInput {
                statement,
                source_name,
            },
        ) in all_statements.into_iter().enumerate()
        {
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

        Ok(StatementLineage {
            statement_index: index,
            statement_type,
            source_name,
            nodes: ctx.nodes,
            edges: ctx.edges,
            span: None,
        })
    }

    fn add_table_columns_from_schema(
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
                    self.add_output_column_with_aggregation(
                        ctx,
                        &name,
                        sources,
                        expr_text,
                        target_node,
                        false,
                        aggregation,
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
                    self.add_output_column_with_aggregation(
                        ctx,
                        &name,
                        sources,
                        expr_text,
                        target_node,
                        false,
                        aggregation,
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

    /// Normalize a GROUP BY expression for comparison with SELECT expressions.
    ///
    /// # Limitations
    ///
    /// This uses string-based normalization which has inherent limitations:
    /// - Semantically equivalent expressions with different syntax may not match
    ///   (e.g., `a + b` vs `b + a` for commutative operations)
    /// - Complex expressions fall back to `expr.to_string().to_lowercase()` which
    ///   may have formatting differences between the GROUP BY and SELECT clauses
    ///
    /// For most real-world queries with simple column references, this works correctly.
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

    /// Detect aggregation in an expression and return AggregationInfo if applicable
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

    /// Find the first aggregate function in an expression.
    ///
    /// Recursively traverses the expression tree looking for aggregate function calls
    /// (SUM, COUNT, AVG, etc.). Returns the first aggregate found.
    ///
    /// # Note on Window Functions
    ///
    /// This method does not distinguish between aggregate functions used in a
    /// GROUP BY context vs. window function context (e.g., `SUM(x) OVER (...)`).
    /// Window functions are parsed as `Expr::Function` with an `over` clause,
    /// but this detection treats them the same as regular aggregates.
    /// For lineage purposes, both represent a many-to-one data transformation.
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

    /// Check if a function is an aggregate and return its info, or search its arguments.
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

    /// Search function arguments for nested aggregate functions.
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

    /// Search CASE expression components for aggregate functions.
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

    fn extract_column_refs(&self, expr: &Expr) -> Vec<ColumnRef> {
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

    fn add_output_column(
        &mut self,
        ctx: &mut StatementContext,
        name: &str,
        sources: Vec<ColumnRef>,
        expression: Option<String>,
        target_node: Option<&str>,
        approximate: bool,
    ) {
        self.add_output_column_with_aggregation(
            ctx,
            name,
            sources,
            expression,
            target_node,
            approximate,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add_output_column_with_aggregation(
        &mut self,
        ctx: &mut StatementContext,
        name: &str,
        sources: Vec<ColumnRef>,
        expression: Option<String>,
        target_node: Option<&str>,
        approximate: bool,
        aggregation: Option<AggregationInfo>,
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
            resolution_source: None,
            filters: Vec::new(),
            join_type: None,
            join_condition: None,
            aggregation,
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
                    join_type: None,
                    join_condition: None,
                    metadata: None,
                    approximate: None,
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
                        join_type: None,
                        join_condition: None,
                        metadata: None,
                        approximate: if approximate { Some(true) } else { None },
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
                .map(|schema_entry| {
                    schema_entry
                        .table
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
                // Expand from schema - NOT approximate
                for (col_name, table, resolved_table) in columns {
                    let sources = vec![ColumnRef {
                        table: Some(table),
                        column: col_name.clone(),
                        resolved_table: Some(resolved_table),
                    }];
                    self.add_output_column(ctx, &col_name, sources, None, target_node, false);
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
    /// Uses stack-based scoping to only consider tables in the current SELECT's FROM clause.
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

    fn analyze_table_with_joins(
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
    fn capture_filter_predicates(
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
    fn apply_pending_filters(&self, ctx: &mut StatementContext) {
        // Collect pending filters to avoid borrow issues
        let pending: Vec<(String, Vec<FilterPredicate>)> = ctx.pending_filters.drain().collect();

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

    /// Pre-register aliases in a table factor without creating nodes.
    fn register_aliases_in_table_factor(
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

    /// Pre-register aliases in a joined table tree.
    fn register_aliases_in_table_with_joins(
        &self,
        ctx: &mut StatementContext,
        table_with_joins: &TableWithJoins,
    ) {
        self.register_aliases_in_table_factor(ctx, &table_with_joins.relation);
        for join in &table_with_joins.joins {
            self.register_aliases_in_table_factor(ctx, &join.relation);
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
                    label: extract_simple_name(&canonical),
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

    /// Helper to register implied schema from CREATE TABLE/VIEW/CTAS statements.
    /// Handles conflict detection with imported schema and only registers if allow_implied is true.
    fn register_implied_schema(
        &mut self,
        ctx: &StatementContext,
        canonical: &str,
        columns: Vec<ColumnSchema>,
        is_temporary: bool,
        statement_type: &str, // "TABLE", "VIEW", or "CREATE TABLE AS"
    ) {
        if !self.allow_implied() {
            return;
        }

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

        // Only store if we have columns
        if columns.is_empty() {
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
        self.known_tables.insert(canonical.to_string());
    }

    fn analyze_create_table_as(
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
                data_type: None, // Type inference not implemented yet
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

    fn analyze_create_table(
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

        // Create column nodes immediately from schema (either imported or from CREATE TABLE)

        if self.schema_tables.contains_key(&canonical) {
            self.add_table_columns_from_schema(ctx, &canonical, &node_id);
        }

        self.all_tables.insert(canonical.clone());

        self.produced_tables.insert(canonical, ctx.statement_index);
    }

    fn analyze_create_view(
        &mut self,
        ctx: &mut StatementContext,
        name: &ObjectName,
        query: &Query,
        is_temporary: bool,
    ) {
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
            resolution_source: None,
            filters: Vec::new(),
            join_type: None,
            join_condition: None,
            aggregation: None,
        });

        self.all_tables.insert(canonical.clone());
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
                data_type: None, // Type inference not implemented yet
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

    fn analyze_drop(
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
        let mut target_node_id = None;

        if let TableFactor::Table { name, alias, .. } = &table.relation {
            let table_name = name.to_string();
            let canonical_res = self.add_source_table(ctx, &table_name, None);
            let canonical = canonical_res
                .clone()
                .unwrap_or_else(|| self.normalize_table_name(&table_name));

            // Register alias if present
            if let (Some(a), Some(canonical_name)) = (alias, canonical_res) {
                ctx.table_aliases.insert(a.name.to_string(), canonical_name);
            }

            // We need the Node ID
            let node_id = generate_node_id("table", &canonical);

            target_node_id = Some(node_id.clone());

            #[cfg(feature = "tracing")]
            info!(target: "analyzer", "UPDATE target identified: {} (ID: {})", canonical, node_id);

            self.produced_tables
                .insert(canonical.clone(), ctx.statement_index);

            // Expand columns from schema if available
            self.add_table_columns_from_schema(ctx, &canonical, &node_id);
        } else {
            self.analyze_table_with_joins(ctx, table, None);
        }

        // 2. Analyze FROM clause (Postgres style)
        if let Some(from_table) = from {
            self.analyze_table_with_joins(ctx, from_table, target_node_id.as_deref());
        }

        // 3. Analyze assignments (SET clause)
        for assignment in assignments {
            self.analyze_expression(ctx, &assignment.value);
        }

        // 4. Analyze selection (WHERE clause)
        if let Some(expr) = selection {
            self.analyze_expression(ctx, expr);
        }

        // Also analyze the joins in the target table structure itself
        for join in &table.joins {
            let join_type = "JOIN";
            ctx.last_operation = Some(join_type.to_string());
            self.analyze_table_factor(ctx, &join.relation, target_node_id.as_deref());
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
        let mut target_ids = Vec::new();

        // Pre-register aliases from sources so multi-table deletes can resolve targets.
        match from {
            FromTable::WithFromKeyword(ts) | FromTable::WithoutKeyword(ts) => {
                for t in ts {
                    self.register_aliases_in_table_with_joins(ctx, t);
                }
            }
        }
        if let Some(us) = using {
            for t in us {
                self.register_aliases_in_table_with_joins(ctx, t);
            }
        }

        // 1. Identify targets
        if !tables.is_empty() {
            // Multi-table delete
            for obj in tables {
                let name = obj.to_string();
                let target_canonical = self
                    .resolve_table_alias(ctx, Some(&name))
                    .unwrap_or_else(|| self.canonicalize_table_reference(&name).canonical);
                // We add them as nodes (they are being affected)
                self.add_source_table(ctx, &target_canonical, None);

                let node_id = generate_node_id("table", &target_canonical);
                target_ids.push(node_id.clone());

                #[cfg(feature = "tracing")]
                info!(target: "analyzer", "DELETE target identified: {} (ID: {})", target_canonical, node_id);

                self.produced_tables
                    .insert(target_canonical.clone(), ctx.statement_index);

                // Expand columns from schema if available
                self.add_table_columns_from_schema(ctx, &target_canonical, &node_id);
            }
        } else {
            // Standard SQL: first table in FROM is target
            let ts = match from {
                FromTable::WithFromKeyword(ts) | FromTable::WithoutKeyword(ts) => ts,
            };
            if let Some(first) = ts.first() {
                if let TableFactor::Table { name, alias, .. } = &first.relation {
                    let name_str = name.to_string();
                    let canonical_res = self.add_source_table(ctx, &name_str, None);

                    // Use the canonical name returned by add_source_table
                    let canonical = canonical_res
                        .clone()
                        .unwrap_or_else(|| self.normalize_table_name(&name_str));

                    // Register alias if present
                    if let (Some(a), Some(canonical_name)) = (alias, canonical_res) {
                        ctx.table_aliases.insert(a.name.to_string(), canonical_name);
                    }

                    let node_id = generate_node_id("table", &canonical);
                    target_ids.push(node_id.clone());

                    #[cfg(feature = "tracing")]
                    info!(target: "analyzer", "DELETE target identified: {} (ID: {})", canonical, node_id);

                    self.produced_tables
                        .insert(canonical.clone(), ctx.statement_index);

                    // Expand columns from schema if available
                    self.add_table_columns_from_schema(ctx, &canonical, &node_id);
                }
            }
        }

        // 2. Analyze sources (FROM + USING)

        let mut analyze_sources = |ctx: &mut StatementContext, source_tables: &[TableWithJoins]| {
            for t in source_tables {
                if target_ids.is_empty() {
                    self.analyze_table_with_joins(ctx, t, None);
                } else {
                    for target_id in &target_ids {
                        self.analyze_table_with_joins(ctx, t, Some(target_id));
                    }
                }
            }
        };

        match from {
            FromTable::WithFromKeyword(ts) | FromTable::WithoutKeyword(ts) => {
                analyze_sources(ctx, ts);
            }
        }
        if let Some(us) = using {
            analyze_sources(ctx, us);
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
        on: &Expr,
        clauses: &[MergeClause],
    ) {
        // 1. Analyze Target Table
        let mut target_id = None;
        if let TableFactor::Table { name, alias, .. } = table {
            let table_name = name.to_string();
            let canonical_res = self.add_source_table(ctx, &table_name, None);

            // Use the canonical name returned by add_source_table
            let canonical = canonical_res
                .clone()
                .unwrap_or_else(|| self.normalize_table_name(&table_name));

            // Register alias if present
            if let (Some(a), Some(canonical_name)) = (alias, canonical_res) {
                ctx.table_aliases.insert(a.name.to_string(), canonical_name);
            }

            let node_id = generate_node_id("table", &canonical);
            target_id = Some(node_id.clone());

            #[cfg(feature = "tracing")]
            info!(target: "analyzer", "MERGE target identified: {} (ID: {})", canonical, node_id);

            self.produced_tables
                .insert(canonical.clone(), ctx.statement_index);

            // Expand columns from schema if available
            self.add_table_columns_from_schema(ctx, &canonical, &node_id);
        } else {
            self.analyze_table_factor(ctx, table, None);
        }

        // 2. Analyze Source Table (USING clause)
        self.analyze_table_factor(ctx, source, target_id.as_deref());

        // 3. Analyze ON predicate
        self.analyze_expression(ctx, on);

        // 4. Analyze MERGE clauses
        for clause in clauses {
            match &clause.action {
                MergeAction::Update { assignments } => {
                    // Analyze assignments in UPDATE clause
                    for assignment in assignments {
                        self.analyze_expression(ctx, &assignment.value);
                    }
                }
                MergeAction::Insert(insert_expr) => {
                    // Analyze INSERT clause
                    // MergeInsertExpr contains columns and kind fields
                    match &insert_expr.kind {
                        MergeInsertKind::Values(values) => {
                            // VALUES clause with rows
                            for row in &values.rows {
                                for value in row {
                                    self.analyze_expression(ctx, value);
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
                self.analyze_expression(ctx, predicate);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Severity;

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
    fn test_multi_table_delete_resolves_alias_targets() {
        use sqlparser::dialect::MySqlDialect;
        use sqlparser::parser::Parser;

        let sql = r#"
            DELETE t
            FROM orders AS t
            INNER JOIN order_items AS oi ON oi.order_id = t.id
            WHERE oi.cancelled = true;
        "#;

        let stmt = Parser::parse_sql(&MySqlDialect {}, sql)
            .expect("parse should succeed")
            .into_iter()
            .next()
            .expect("one statement parsed");

        let request = make_request(sql);
        let mut analyzer = Analyzer::new(&request);
        let lineage = analyzer
            .analyze_statement(0, &stmt, None)
            .expect("analysis should succeed");

        let tables: HashSet<_> = lineage
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Table)
            .map(|n| n.qualified_name.clone().unwrap_or_else(|| n.label.clone()))
            .collect();

        assert!(
            tables.contains("orders"),
            "DELETE target alias should resolve to base table"
        );
        assert!(
            tables.contains("order_items"),
            "DELETE join source should be tracked"
        );
        assert!(
            !tables.contains("t"),
            "DELETE should not produce table nodes for bare aliases"
        );
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
                allow_implied: true,
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
                allow_implied: true,
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
                allow_implied: true,
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
                allow_implied: true,
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
                allow_implied: true,
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
    fn test_recursive_cte_supported_without_warning() {
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

        // Should be supported without warnings
        assert!(result
            .issues
            .iter()
            .all(|i| i.severity != Severity::Warning));
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
                allow_implied: true,
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
                allow_implied: true,
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
                allow_implied: true,
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

        // All edges should NOT be marked as approximate (we have schema)
        let approximate_edges: Vec<_> = result.statements[0]
            .edges
            .iter()
            .filter(|e| e.approximate == Some(true))
            .collect();
        assert_eq!(
            approximate_edges.len(),
            0,
            "Should not have approximate edges when schema is available"
        );
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

    #[test]
    fn test_create_or_replace_table_updates_implied_schema() {
        // Test that CREATE OR REPLACE TABLE updates the implied schema for subsequent statements
        let request = make_request(
            r#"
            CREATE TABLE t (id INT);
            CREATE OR REPLACE TABLE t (id INT, name VARCHAR);
            SELECT * FROM t;
        "#,
        );
        let result = analyze(&request);

        assert_eq!(result.statements.len(), 3);

        // The SELECT * should expand to both id and name columns (from second CREATE)
        let select_stmt = &result.statements[2];
        let column_nodes: Vec<_> = select_stmt
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        // Should have both columns from the second CREATE TABLE
        let column_labels: Vec<_> = column_nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(
            column_labels.contains(&"id"),
            "Expected 'id' column, found: {:?}",
            column_labels
        );
        assert!(
            column_labels.contains(&"name"),
            "Expected 'name' column, found: {:?}",
            column_labels
        );
    }

    #[test]
    fn test_imported_schema_not_overwritten_by_create_table() {
        // Test that imported (user-provided) schemas take precedence and are not overwritten
        use crate::types::{ColumnSchema, SchemaMetadata, SchemaTable};

        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![SchemaTable {
                catalog: None,
                schema: None,
                name: "t".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "id".to_string(),
                        data_type: Some("INT".to_string()),
                    },
                    ColumnSchema {
                        name: "imported_col".to_string(),
                        data_type: Some("VARCHAR".to_string()),
                    },
                ],
            }],
        };

        let request = AnalyzeRequest {
            sql: r#"
                CREATE TABLE t (id INT, different_col VARCHAR);
                SELECT * FROM t;
            "#
            .to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            schema: Some(schema),
            options: None,
        };

        let result = analyze(&request);

        // The SELECT * should expand to imported schema columns, not the CREATE TABLE columns
        let select_stmt = &result.statements[1];
        let column_nodes: Vec<_> = select_stmt
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        let column_labels: Vec<_> = column_nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(
            column_labels.contains(&"id"),
            "Expected 'id' column from imported schema, found: {:?}",
            column_labels
        );
        assert!(
            column_labels.contains(&"imported_col"),
            "Expected 'imported_col' from imported schema, found: {:?}",
            column_labels
        );
        assert!(
            !column_labels.contains(&"different_col"),
            "Should not have 'different_col' from CREATE TABLE (imported schema takes precedence), found: {:?}",
            column_labels
        );
    }

    // =====================================================
    // Schema Handling Tests (Phase 1-10)
    // =====================================================

    #[test]
    fn test_create_view_captures_implied_schema() {
        let request = make_request(
            r#"
            CREATE VIEW active_users AS 
            SELECT id, name, email FROM users WHERE active = true;
            
            SELECT * FROM active_users;
        "#,
        );
        let result = analyze(&request);

        // Check that resolvedSchema contains the view
        assert!(result.resolved_schema.is_some());
        let schema = result.resolved_schema.unwrap();

        let view = schema.tables.iter().find(|t| t.name == "active_users");
        assert!(
            view.is_some(),
            "active_users view should be in resolved schema"
        );

        let view = view.unwrap();
        assert_eq!(view.origin, crate::types::SchemaOrigin::Implied);
        assert_eq!(view.source_statement_index, Some(0));
        assert_eq!(view.columns.len(), 3);

        let col_names: Vec<_> = view.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(col_names.contains(&"id"));
        assert!(col_names.contains(&"name"));
        assert!(col_names.contains(&"email"));
    }

    #[test]
    fn test_create_table_explicit_columns_captured() {
        let request = make_request(
            r#"
            CREATE TABLE products (
                id INTEGER,
                name VARCHAR(255),
                price DECIMAL(10,2)
            );
        "#,
        );
        let result = analyze(&request);

        assert!(result.resolved_schema.is_some());
        let schema = result.resolved_schema.unwrap();

        let table = schema.tables.iter().find(|t| t.name == "products");
        assert!(table.is_some());

        let table = table.unwrap();
        assert_eq!(table.origin, crate::types::SchemaOrigin::Implied);
        assert_eq!(table.columns.len(), 3);
        assert!(table
            .columns
            .iter()
            .any(|c| c.name == "id" && c.data_type.is_some()));
    }

    #[test]
    fn test_temporary_table_flagged() {
        let request = make_request(
            r#"
            CREATE TEMPORARY TABLE temp_data (
                id INTEGER,
                value TEXT
            );
        "#,
        );
        let result = analyze(&request);

        assert!(result.resolved_schema.is_some());
        let schema = result.resolved_schema.unwrap();

        let table = schema.tables.iter().find(|t| t.name == "temp_data");
        assert!(table.is_some());

        let table = table.unwrap();
        assert_eq!(table.temporary, Some(true));
    }

    #[test]
    fn test_drop_table_removes_implied_schema() {
        let request = make_request(
            r#"
            CREATE TABLE temp (id INTEGER);
            DROP TABLE temp;
        "#,
        );
        let result = analyze(&request);

        if let Some(schema) = result.resolved_schema {
            assert!(
                !schema.tables.iter().any(|t| t.name == "temp"),
                "Dropped table should not be in resolved schema"
            );
        }
    }

    #[test]
    fn test_imported_schema_preserved_in_resolved() {
        let request = AnalyzeRequest {
            sql: "SELECT * FROM users".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                allow_implied: true,
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
                            data_type: Some("INTEGER".to_string()),
                        },
                        crate::types::ColumnSchema {
                            name: "name".to_string(),
                            data_type: Some("VARCHAR".to_string()),
                        },
                    ],
                }],
            }),
        };
        let result = analyze(&request);

        assert!(result.resolved_schema.is_some());
        let schema = result.resolved_schema.unwrap();

        let table = schema.tables.iter().find(|t| t.name == "users");
        assert!(table.is_some());

        let table = table.unwrap();
        assert_eq!(table.origin, crate::types::SchemaOrigin::Imported);
        assert_eq!(table.source_statement_index, None);
        assert_eq!(table.columns.len(), 2);
    }

    #[test]
    fn test_mixed_imported_and_implied_schema() {
        let request = AnalyzeRequest {
            sql: r#"
                CREATE VIEW active_users AS 
                SELECT id, name FROM users WHERE active = true;
            "#
            .to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                allow_implied: true,
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
                        data_type: Some("INTEGER".to_string()),
                    }],
                }],
            }),
        };
        let result = analyze(&request);

        assert!(result.resolved_schema.is_some());
        let schema = result.resolved_schema.unwrap();

        // Should have both imported (users) and implied (active_users)
        assert!(schema
            .tables
            .iter()
            .any(|t| t.name == "users" && t.origin == crate::types::SchemaOrigin::Imported));
        assert!(schema
            .tables
            .iter()
            .any(|t| t.name == "active_users" && t.origin == crate::types::SchemaOrigin::Implied));
    }

    #[test]
    fn test_create_or_replace_updates_schema() {
        let request = make_request(
            r#"
            CREATE TABLE data (old_col INTEGER);
            CREATE TABLE data (new_col TEXT);
        "#,
        );
        let result = analyze(&request);

        assert!(result.resolved_schema.is_some());
        let schema = result.resolved_schema.unwrap();

        let table = schema.tables.iter().find(|t| t.name == "data");
        assert!(table.is_some());

        let table = table.unwrap();
        // Should have the latest schema (new_col)
        assert!(table.columns.iter().any(|c| c.name == "new_col"));
    }

    // Phase 8 - Test for table-level precedence and conflict detection
    #[test]
    fn test_imported_precedence_with_conflict_detection() {
        let request = AnalyzeRequest {
            sql: r#"
                CREATE TABLE users (id INT, name VARCHAR, extra_col TEXT);
                SELECT * FROM users;
            "#
            .to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                allow_implied: true,
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
                            data_type: Some("INTEGER".to_string()),
                        },
                        crate::types::ColumnSchema {
                            name: "imported_col".to_string(),
                            data_type: None,
                        },
                    ],
                }],
            }),
        };
        let result = analyze(&request);

        // Imported schema should win - resolvedSchema should only show imported columns
        let schema = result.resolved_schema.expect("Should have resolved schema");
        let users_table = schema
            .tables
            .iter()
            .find(|t| t.name == "users")
            .expect("users table should exist");

        assert_eq!(users_table.origin, crate::types::SchemaOrigin::Imported);
        assert_eq!(users_table.columns.len(), 2); // Only imported columns
        assert!(users_table.columns.iter().any(|c| c.name == "imported_col"));
        assert!(!users_table.columns.iter().any(|c| c.name == "extra_col"));

        // Should have a SCHEMA_CONFLICT warning
        let conflict_warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Warning && i.code == issue_codes::SCHEMA_CONFLICT)
            .collect();
        assert_eq!(
            conflict_warnings.len(),
            1,
            "Should have exactly one SCHEMA_CONFLICT warning"
        );
        assert!(conflict_warnings[0]
            .message
            .contains("conflicts with imported schema"));
    }

    // Test that allow_implied=false disables implied schema capture
    #[test]
    fn test_allow_implied_false_disables_schema_capture() {
        let request = AnalyzeRequest {
            sql: r#"
                CREATE TABLE orders (order_id INT, amount DECIMAL);
                CREATE VIEW high_orders AS SELECT order_id, amount FROM orders WHERE amount > 100;
                DROP TABLE orders;
            "#
            .to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                allow_implied: false, // Disable implied schema
                default_catalog: None,
                default_schema: None,
                search_path: None,
                case_sensitivity: None,
                tables: vec![],
            }),
        };
        let result = analyze(&request);

        // Should NOT have any resolvedSchema entries (allow_implied=false)
        assert!(
            result.resolved_schema.is_none() || result.resolved_schema.unwrap().tables.is_empty(),
            "Should not capture implied schema when allow_implied=false"
        );

        // Should NOT have any conflict warnings (implied schema not captured)
        let conflict_warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.code == issue_codes::SCHEMA_CONFLICT)
            .collect();
        assert_eq!(
            conflict_warnings.len(),
            0,
            "Should not emit conflict warnings when allow_implied=false"
        );
    }

    // Test that allow_implied=false with imported schema still preserves imported
    #[test]
    fn test_allow_implied_false_preserves_imported_schema() {
        let request = AnalyzeRequest {
            sql: r#"
                CREATE TABLE users (id INT, name TEXT, extra_col TEXT);
                SELECT * FROM users;
            "#
            .to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: Some(crate::types::SchemaMetadata {
                allow_implied: false, // Disable implied schema
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
                            data_type: Some("INTEGER".to_string()),
                        },
                        crate::types::ColumnSchema {
                            name: "username".to_string(),
                            data_type: Some("TEXT".to_string()),
                        },
                    ],
                }],
            }),
        };
        let result = analyze(&request);

        // Should ONLY have imported schema (users), not implied schema
        let resolved = result.resolved_schema.expect("Should have resolved schema");
        assert_eq!(resolved.tables.len(), 1);

        let users_table = &resolved.tables[0];
        assert_eq!(users_table.name, "users");
        assert_eq!(users_table.origin, crate::types::SchemaOrigin::Imported);
        assert_eq!(users_table.columns.len(), 2);

        // Should NOT have conflict warnings (allow_implied=false means no conflict check)
        let conflict_warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.code == issue_codes::SCHEMA_CONFLICT)
            .collect();
        assert_eq!(
            conflict_warnings.len(),
            0,
            "Should not emit conflict warnings when allow_implied=false"
        );
    }

    // =====================================================
    // Scope-based Resolution Tests
    // =====================================================

    #[test]
    fn test_multi_cte_scope_isolation() {
        // Test that columns are resolved within their immediate CTE scope,
        // not leaking across CTE boundaries
        let request = make_request(
            r#"
            WITH user_metrics AS (
                SELECT
                    u.user_id,
                    COUNT(DISTINCT o.order_id) as total_orders,
                    SUM(o.total_amount) as lifetime_value
                FROM users u
                LEFT JOIN orders o ON u.user_id = o.user_id
                GROUP BY u.user_id
            ),
            user_segments AS (
                SELECT
                    user_id,
                    total_orders,
                    lifetime_value,
                    CASE
                        WHEN lifetime_value > 1000 THEN 'VIP'
                        ELSE 'Regular'
                    END as customer_segment
                FROM user_metrics
            ),
            segment_summary AS (
                SELECT
                    customer_segment,
                    COUNT(DISTINCT user_id) as user_count,
                    SUM(lifetime_value) as total_revenue
                FROM user_segments
                GROUP BY customer_segment
            )
            SELECT
                customer_segment,
                user_count,
                total_revenue
            FROM segment_summary
            "#,
        );
        let result = analyze(&request);

        // Should not have ambiguous column warnings because each CTE scope
        // is isolated - columns should resolve to the single table in scope
        let ambiguous_warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| {
                i.code == issue_codes::UNRESOLVED_REFERENCE
                    && i.message.contains("exists in multiple tables")
            })
            .collect();

        assert!(
            ambiguous_warnings.is_empty(),
            "Should not have ambiguous column warnings in properly scoped CTEs. Found: {:?}",
            ambiguous_warnings
        );
    }

    #[test]
    fn test_datediff_day_not_treated_as_column() {
        // Test that 'day' in DATEDIFF(day, ...) is not treated as a column reference
        let request = make_request(
            r#"
            SELECT
                user_id,
                DATEDIFF(day, created_at, CURRENT_DATE) as days_since_creation
            FROM users
            "#,
        );
        let result = analyze(&request);

        // Should NOT have a warning about 'day' being ambiguous or unresolved
        let day_warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.message.contains("'day'"))
            .collect();

        assert!(
            day_warnings.is_empty(),
            "Should not treat 'day' unit keyword as a column reference. Found warnings: {:?}",
            day_warnings
        );

        // Should have column lineage for the actual columns
        let column_nodes: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        let column_labels: Vec<_> = column_nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(
            column_labels.contains(&"user_id") || column_labels.contains(&"days_since_creation")
        );
    }

    #[test]
    fn test_dateadd_unit_not_treated_as_column() {
        // Test various date functions with unit keywords
        let request = make_request(
            r#"
            SELECT
                DATEADD(month, 1, created_at) as next_month,
                DATEPART(year, created_at) as year_value,
                TIMESTAMPDIFF(hour, start_time, end_time) as hours_diff
            FROM events
            "#,
        );
        let result = analyze(&request);

        // Should NOT have warnings about 'month', 'year', or 'hour' being ambiguous
        let unit_warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| {
                i.message.contains("'month'")
                    || i.message.contains("'year'")
                    || i.message.contains("'hour'")
            })
            .collect();

        assert!(
            unit_warnings.is_empty(),
            "Should not treat date/time unit keywords as column references. Found warnings: {:?}",
            unit_warnings
        );
    }

    #[test]
    fn test_cte_with_join_scope_isolation() {
        // Test that when a CTE joins multiple tables, subsequent CTEs that reference
        // only the CTE don't see the internal tables
        let request = make_request(
            r#"
            WITH enriched_orders AS (
                SELECT
                    o.order_id,
                    o.user_id,
                    o.total_amount,
                    u.email,
                    p.category
                FROM orders o
                JOIN users u ON o.user_id = u.user_id
                JOIN products p ON o.product_id = p.product_id
            ),
            order_summary AS (
                SELECT
                    user_id,
                    email,
                    SUM(total_amount) as total_spent,
                    COUNT(DISTINCT category) as categories_purchased
                FROM enriched_orders
                GROUP BY user_id, email
            )
            SELECT * FROM order_summary
            "#,
        );
        let result = analyze(&request);

        // The order_summary CTE references enriched_orders, not the underlying
        // orders/users/products tables. Columns like user_id, email should resolve
        // to enriched_orders, not be ambiguous across multiple tables.
        let ambiguous_warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| {
                i.code == issue_codes::UNRESOLVED_REFERENCE
                    && i.message.contains("exists in multiple tables")
            })
            .collect();

        assert!(
            ambiguous_warnings.is_empty(),
            "Columns in CTE referencing another CTE should resolve to that CTE, \
            not see internal tables. Found: {:?}",
            ambiguous_warnings
        );
    }

    #[test]
    fn test_column_named_day_tracked_in_aggregate() {
        // Regression test: columns named like date units (day, month, year, etc.)
        // should still be tracked when used in non-date functions like MAX, SUM, etc.
        let request = make_request(
            r#"
            SELECT MAX(day) as max_day, SUM(hours) as total_hours
            FROM metrics
            "#,
        );
        let result = analyze(&request);

        // Should NOT have any warnings - these are legitimate columns
        let column_warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.message.contains("'day'") || i.message.contains("'hours'"))
            .collect();

        assert!(
            column_warnings.is_empty(),
            "Columns named 'day' or 'hours' should be tracked in aggregate functions. Found warnings: {:?}",
            column_warnings
        );

        // Should have column lineage for both 'day' and 'hours'
        let column_labels: Vec<_> = result.statements[0]
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .map(|n| n.label.as_str())
            .collect();

        assert!(
            column_labels.contains(&"day"),
            "Should track 'day' column in MAX(day). Found columns: {:?}",
            column_labels
        );
        assert!(
            column_labels.contains(&"hours"),
            "Should track 'hours' column in SUM(hours). Found columns: {:?}",
            column_labels
        );
    }
}
