use crate::error::ParseError;
use crate::parser::parse_sql_with_dialect;
use crate::types::*;
use serde_json::json;
use sqlparser::ast::{
    self, Expr, FunctionArg, FunctionArgExpr, ObjectName, Query, SelectItem, SetExpr, Statement,
    TableFactor, TableWithJoins,
};
use std::collections::{HashMap, HashSet};

/// Main entry point for SQL analysis
pub fn analyze(request: &AnalyzeRequest) -> AnalyzeResult {
    let mut analyzer = Analyzer::new(request);
    analyzer.analyze()
}

/// Internal analyzer state
struct Analyzer<'a> {
    request: &'a AnalyzeRequest,
    issues: Vec<Issue>,
    statement_lineages: Vec<StatementLineage>,
    /// Track which tables are produced by which statement (for cross-statement linking)
    produced_tables: HashMap<String, usize>,
    /// Track which tables are consumed by which statements
    consumed_tables: HashMap<String, Vec<usize>>,
    /// All discovered tables across statements (for global lineage)
    all_tables: HashSet<String>,
    /// All discovered CTEs
    all_ctes: HashSet<String>,
    /// Known tables from schema metadata (for validation)
    known_tables: HashSet<String>,
    /// Schema lookup: table canonical name -> table schema info
    schema_tables: HashMap<String, SchemaTable>,
    /// Whether column lineage is enabled
    column_lineage_enabled: bool,
    /// Default catalog for unqualified identifiers
    default_catalog: Option<String>,
    /// Default schema for unqualified identifiers
    default_schema: Option<String>,
    /// Ordered search path entries
    search_path: Vec<SearchPathEntry>,
}

#[derive(Debug, Clone)]
struct SearchPathEntry {
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

    fn initialize_schema_metadata(&mut self) {
        if let Some(schema) = self.request.schema.as_ref() {
            self.default_catalog = schema
                .default_catalog
                .as_ref()
                .map(|c| self.normalize_identifier(c));
            self.default_schema = schema
                .default_schema
                .as_ref()
                .map(|s| self.normalize_identifier(s));
            if let Some(search_path) = schema.search_path.as_ref() {
                self.search_path = search_path
                    .iter()
                    .map(|hint| SearchPathEntry {
                        catalog: hint.catalog.as_ref().map(|c| self.normalize_identifier(c)),
                        schema: self.normalize_identifier(&hint.schema),
                    })
                    .collect();
            } else if let Some(default_schema) = &self.default_schema {
                self.search_path = vec![SearchPathEntry {
                    catalog: self.default_catalog.clone(),
                    schema: default_schema.clone(),
                }];
            }

            for table in &schema.tables {
                let canonical = self.schema_table_key(table);
                self.known_tables.insert(canonical.clone());
                self.schema_tables.insert(canonical, table.clone());
            }
        }
    }

    fn schema_table_key(&self, table: &SchemaTable) -> String {
        let mut parts = Vec::new();
        if let Some(catalog) = &table.catalog {
            parts.push(catalog.clone());
        }
        if let Some(schema) = &table.schema {
            parts.push(schema.clone());
        }
        parts.push(table.name.clone());
        self.normalize_table_name(&parts.join("."))
    }

    fn canonicalize_table_reference(&self, name: &str) -> TableResolution {
        let parts = split_qualified_identifiers(name);
        if parts.is_empty() {
            return TableResolution {
                canonical: String::new(),
                matched_schema: false,
            };
        }

        let normalized: Vec<String> = parts
            .into_iter()
            .map(|part| self.normalize_identifier(&part))
            .collect();

        match normalized.len() {
            len if len >= 3 => {
                let canonical = normalized.join(".");
                let matched = self.known_tables.contains(&canonical);
                TableResolution {
                    canonical,
                    matched_schema: matched,
                }
            }
            2 => {
                let canonical = normalized.join(".");
                if self.known_tables.contains(&canonical) {
                    return TableResolution {
                        canonical,
                        matched_schema: true,
                    };
                }
                if let Some(default_catalog) = &self.default_catalog {
                    let with_catalog = format!("{default_catalog}.{canonical}");
                    if self.known_tables.contains(&with_catalog) {
                        return TableResolution {
                            canonical: with_catalog,
                            matched_schema: true,
                        };
                    }
                }
                TableResolution {
                    canonical,
                    matched_schema: false,
                }
            }
            _ => {
                let table_only = normalized[0].clone();

                if self.known_tables.contains(&table_only) {
                    return TableResolution {
                        canonical: table_only,
                        matched_schema: true,
                    };
                }

                if let Some(candidate) = self.resolve_via_search_path(&table_only) {
                    return TableResolution {
                        canonical: candidate,
                        matched_schema: true,
                    };
                }

                if let Some(schema) = &self.default_schema {
                    let canonical = if let Some(catalog) = &self.default_catalog {
                        format!("{catalog}.{schema}.{table_only}")
                    } else {
                        format!("{schema}.{table_only}")
                    };
                    let matched = self.known_tables.contains(&canonical);
                    return TableResolution {
                        canonical,
                        matched_schema: matched,
                    };
                }

                TableResolution {
                    canonical: table_only.clone(),
                    matched_schema: self.known_tables.contains(&table_only),
                }
            }
        }
    }

    fn resolve_via_search_path(&self, table: &str) -> Option<String> {
        for entry in &self.search_path {
            let canonical = match (&entry.catalog, &entry.schema) {
                (Some(catalog), schema) => format!("{catalog}.{schema}.{table}"),
                (None, schema) => format!("{schema}.{table}"),
            };

            if self.known_tables.contains(&canonical) {
                return Some(canonical);
            }
        }
        None
    }

    fn analyze(&mut self) -> AnalyzeResult {
        // Parse SQL
        let statements = match parse_sql_with_dialect(&self.request.sql, self.request.dialect) {
            Ok(stmts) => stmts,
            Err(e) => {
                self.issues
                    .push(Issue::error(issue_codes::PARSE_ERROR, e.to_string()));
                return self.build_result();
            }
        };

        // Analyze each statement
        for (index, statement) in statements.iter().enumerate() {
            match self.analyze_statement(index, statement) {
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
                    "CREATE_TABLE".to_string()
                }
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
            source_name: self.request.source_name.clone(),
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
            self.all_ctes.insert(cte_name);

            // Analyze CTE body
            self.analyze_query_body(ctx, &cte.query.body, Some(&cte_id));
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
            self.extract_column_refs_for_validation(ctx, where_clause);
        }

        // Handle GROUP BY
        match &select.group_by {
            ast::GroupByExpr::Expressions(exprs, _) => {
                for group_by in exprs {
                    self.extract_column_refs_for_validation(ctx, group_by);
                }
            }
            ast::GroupByExpr::All(_) => {}
        }

        if let Some(ref having) = select.having {
            self.extract_column_refs_for_validation(ctx, having);
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
                let source_col_id = generate_column_node_id(
                    Some(&generate_node_id("table", table_canonical)),
                    &self.normalize_identifier(&source.column),
                );

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
        &self,
        ctx: &StatementContext,
        qualifier: Option<&str>,
        column: &str,
    ) -> Option<String> {
        // If qualifier provided, use standard resolution
        if let Some(q) = qualifier {
            return self.resolve_table_alias(ctx, Some(q));
        }

        // No qualifier - try to find which table owns this column
        let tables_in_scope: Vec<_> = ctx.table_node_ids.keys().collect();

        // If only one table in scope, assume column belongs to it
        if tables_in_scope.len() == 1 {
            return Some(tables_in_scope[0].clone());
        }

        // Multiple tables - check schema to find which one has this column
        let normalized_col = self.normalize_identifier(column);
        for table_canonical in &tables_in_scope {
            if let Some(schema_table) = self.schema_tables.get(*table_canonical) {
                if schema_table
                    .columns
                    .iter()
                    .any(|c| self.normalize_identifier(&c.name) == normalized_col)
                {
                    return Some((*table_canonical).clone());
                }
            }
        }

        // Can't determine - pick first table (best effort)
        tables_in_scope.first().map(|s| (*s).clone())
    }

    fn validate_column(&mut self, ctx: &StatementContext, table_canonical: &str, column: &str) {
        if let Some(schema_table) = self.schema_tables.get(table_canonical) {
            let normalized_col = self.normalize_identifier(column);
            let column_exists = schema_table
                .columns
                .iter()
                .any(|c| self.normalize_identifier(&c.name) == normalized_col);

            if !column_exists && !schema_table.columns.is_empty() {
                self.issues.push(
                    Issue::warning(
                        issue_codes::UNKNOWN_COLUMN,
                        format!(
                            "Column '{}' not found in table '{}'",
                            column, table_canonical
                        ),
                    )
                    .with_statement(ctx.statement_index),
                );
            }
        }
    }

    fn extract_column_refs_for_validation(&mut self, ctx: &StatementContext, expr: &Expr) {
        let refs = self.extract_column_refs(expr);
        for col_ref in refs {
            if let Some(table) = col_ref.table.as_deref() {
                let resolved = self.resolve_table_alias(ctx, Some(table));
                if let Some(table_canonical) = resolved {
                    self.validate_column(ctx, &table_canonical, &col_ref.column);
                }
            }
        }
    }

    fn normalize_identifier(&self, name: &str) -> String {
        let case_sensitivity = self
            .request
            .schema
            .as_ref()
            .and_then(|s| s.case_sensitivity)
            .unwrap_or(CaseSensitivity::Dialect);

        let effective_case = match case_sensitivity {
            CaseSensitivity::Dialect => self.request.dialect.default_case_sensitivity(),
            other => other,
        };

        if is_quoted_identifier(name) {
            name.to_string()
        } else {
            match effective_case {
                CaseSensitivity::Lower | CaseSensitivity::Dialect => name.to_lowercase(),
                CaseSensitivity::Upper => name.to_uppercase(),
                CaseSensitivity::Exact => name.to_string(),
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

    fn normalize_table_name(&self, name: &str) -> String {
        let case_sensitivity = self
            .request
            .schema
            .as_ref()
            .and_then(|s| s.case_sensitivity)
            .unwrap_or(CaseSensitivity::Dialect);

        let effective_case = match case_sensitivity {
            CaseSensitivity::Dialect => self.request.dialect.default_case_sensitivity(),
            other => other,
        };

        let parts = split_qualified_identifiers(name);
        if parts.is_empty() {
            return String::new();
        }

        let normalized: Vec<String> = parts
            .into_iter()
            .map(|part| {
                if is_quoted_identifier(&part) {
                    part
                } else {
                    match effective_case {
                        CaseSensitivity::Lower | CaseSensitivity::Dialect => part.to_lowercase(),
                        CaseSensitivity::Upper => part.to_uppercase(),
                        CaseSensitivity::Exact => part,
                    }
                }
            })
            .collect();

        normalized.join(".")
    }

    fn build_result(&self) -> AnalyzeResult {
        let global_lineage = self.build_global_lineage();
        let summary = self.build_summary(&global_lineage);

        AnalyzeResult {
            statements: self.statement_lineages.clone(),
            global_lineage,
            issues: self.issues.clone(),
            summary,
        }
    }

    fn build_global_lineage(&self) -> GlobalLineage {
        let mut global_nodes: HashMap<String, GlobalNode> = HashMap::new();
        let mut global_edges: Vec<GlobalEdge> = Vec::new();

        // Collect all nodes from all statements
        for lineage in &self.statement_lineages {
            for node in &lineage.nodes {
                let canonical = node.qualified_name.clone().unwrap_or(node.label.clone());
                let canonical_name = parse_canonical_name(&canonical);

                global_nodes
                    .entry(node.id.clone())
                    .and_modify(|existing| {
                        existing.statement_refs.push(StatementRef {
                            statement_index: lineage.statement_index,
                            node_id: Some(node.id.clone()),
                        });
                    })
                    .or_insert_with(|| GlobalNode {
                        id: node.id.clone(),
                        node_type: node.node_type,
                        label: node.label.clone(),
                        canonical_name,
                        statement_refs: vec![StatementRef {
                            statement_index: lineage.statement_index,
                            node_id: Some(node.id.clone()),
                        }],
                        metadata: None,
                    });
            }

            // Collect edges
            for edge in &lineage.edges {
                global_edges.push(GlobalEdge {
                    id: edge.id.clone(),
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    edge_type: edge.edge_type,
                    producer_statement: Some(StatementRef {
                        statement_index: lineage.statement_index,
                        node_id: None,
                    }),
                    consumer_statement: None,
                    metadata: None,
                });
            }
        }

        // Detect cross-statement edges
        for (table_name, consumers) in &self.consumed_tables {
            if let Some(&producer_idx) = self.produced_tables.get(table_name) {
                for &consumer_idx in consumers {
                    if consumer_idx > producer_idx {
                        // This is a cross-statement dependency
                        let edge_id = format!("cross_{producer_idx}_{consumer_idx}");
                        global_edges.push(GlobalEdge {
                            id: edge_id,
                            from: generate_node_id("table", table_name),
                            to: generate_node_id("table", table_name),
                            edge_type: EdgeType::CrossStatement,
                            producer_statement: Some(StatementRef {
                                statement_index: producer_idx,
                                node_id: None,
                            }),
                            consumer_statement: Some(StatementRef {
                                statement_index: consumer_idx,
                                node_id: None,
                            }),
                            metadata: None,
                        });
                    }
                }
            }
        }

        GlobalLineage {
            nodes: global_nodes.into_values().collect(),
            edges: global_edges,
        }
    }

    fn build_summary(&self, global_lineage: &GlobalLineage) -> Summary {
        let error_count = self
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count();
        let warning_count = self
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count();
        let info_count = self
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Info)
            .count();

        let table_count = global_lineage
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Table || n.node_type == NodeType::Cte)
            .count();

        let column_count = global_lineage
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .count();

        Summary {
            statement_count: self.statement_lineages.len(),
            table_count,
            column_count,
            issue_count: IssueCount {
                errors: error_count,
                warnings: warning_count,
                infos: info_count,
            },
            has_errors: error_count > 0,
        }
    }
}

/// Context for analyzing a single statement
struct StatementContext {
    statement_index: usize,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    node_ids: HashSet<String>,
    edge_ids: HashSet<String>,
    /// CTE name -> node ID
    cte_definitions: HashMap<String, String>,
    /// Alias -> canonical table name
    table_aliases: HashMap<String, String>,
    /// Subquery aliases (for reference tracking)
    subquery_aliases: HashSet<String>,
    /// Last join/operation type for edge labeling
    last_operation: Option<String>,
    /// Table canonical name -> node ID (for column ownership)
    table_node_ids: HashMap<String, String>,
    /// Output columns for this statement (for column lineage)
    output_columns: Vec<OutputColumn>,
    /// CTE columns: CTE name -> list of output columns
    cte_columns: HashMap<String, Vec<OutputColumn>>,
}

/// Represents an output column in the SELECT list
#[derive(Debug, Clone)]
struct OutputColumn {
    /// Alias or derived name for the column
    name: String,
    /// Source columns that contribute to this output
    sources: Vec<ColumnRef>,
    /// Expression text for computed columns
    expression: Option<String>,
    /// Node ID for this column
    node_id: String,
}

/// A reference to a source column
#[derive(Debug, Clone)]
struct ColumnRef {
    /// Table name or alias
    table: Option<String>,
    /// Column name
    column: String,
    /// Resolved table canonical name (if known)
    resolved_table: Option<String>,
}

impl StatementContext {
    fn new(statement_index: usize) -> Self {
        Self {
            statement_index,
            nodes: Vec::new(),
            edges: Vec::new(),
            node_ids: HashSet::new(),
            edge_ids: HashSet::new(),
            cte_definitions: HashMap::new(),
            table_aliases: HashMap::new(),
            subquery_aliases: HashSet::new(),
            last_operation: None,
            table_node_ids: HashMap::new(),
            output_columns: Vec::new(),
            cte_columns: HashMap::new(),
        }
    }

    fn add_node(&mut self, node: Node) -> String {
        let id = node.id.clone();
        if self.node_ids.insert(id.clone()) {
            self.nodes.push(node);
        }
        id
    }

    fn add_edge(&mut self, edge: Edge) {
        let id = edge.id.clone();
        if self.edge_ids.insert(id) {
            self.edges.push(edge);
        }
    }
}

/// Generate a deterministic node ID based on type and name
fn generate_node_id(node_type: &str, name: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    node_type.hash(&mut hasher);
    name.hash(&mut hasher);
    let hash = hasher.finish();

    format!("{node_type}_{hash:016x}")
}

/// Generate a deterministic edge ID
fn generate_edge_id(from: &str, to: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    from.hash(&mut hasher);
    to.hash(&mut hasher);
    let hash = hasher.finish();

    format!("edge_{hash:016x}")
}

/// Generate a deterministic column node ID
fn generate_column_node_id(parent_id: Option<&str>, column_name: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    "column".hash(&mut hasher);
    if let Some(parent) = parent_id {
        parent.hash(&mut hasher);
    }
    column_name.hash(&mut hasher);
    let hash = hasher.finish();

    format!("column_{hash:016x}")
}

/// Check if an expression is a simple column reference (no transformation)
fn is_simple_column_ref(expr: &Expr) -> bool {
    matches!(expr, Expr::Identifier(_) | Expr::CompoundIdentifier(_))
}

/// Extract simple name from potentially qualified name
fn extract_simple_name(name: &str) -> String {
    let mut parts = split_qualified_identifiers(name);
    parts.pop().unwrap_or_else(|| name.to_string())
}

/// Parse a qualified name string into CanonicalName
fn parse_canonical_name(name: &str) -> CanonicalName {
    let parts = split_qualified_identifiers(name);
    match parts.len() {
        0 => CanonicalName::table(None, None, String::new()),
        1 => CanonicalName::table(None, None, parts[0].clone()),
        2 => CanonicalName::table(None, Some(parts[0].clone()), parts[1].clone()),
        3 => CanonicalName::table(
            Some(parts[0].clone()),
            Some(parts[1].clone()),
            parts[2].clone(),
        ),
        _ => CanonicalName::table(None, None, name.to_string()),
    }
}

fn split_qualified_identifiers(name: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = name.chars().peekable();
    let mut active_quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        if let Some(q) = active_quote {
            current.push(ch);
            if ch == q {
                if matches!(q, '"' | '\'' | '`') {
                    if let Some(next) = chars.peek() {
                        if *next == q {
                            current.push(chars.next().unwrap());
                            continue;
                        }
                    }
                }
                active_quote = None;
            } else if q == ']' && ch == ']' {
                active_quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' | '`' => {
                active_quote = Some(ch);
                current.push(ch);
            }
            '[' => {
                active_quote = Some(']');
                current.push(ch);
            }
            '.' => {
                if !current.is_empty() {
                    parts.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        parts.push(current.trim().to_string());
    }

    if parts.is_empty() && !name.is_empty() {
        vec![name.trim().to_string()]
    } else {
        parts
    }
}

fn is_quoted_identifier(part: &str) -> bool {
    let trimmed = part.trim();
    if trimmed.len() < 2 {
        return false;
    }
    let first = trimmed.chars().next().unwrap();
    let last = trimmed.chars().last().unwrap();
    matches!(
        (first, last),
        ('"', '"') | ('`', '`') | ('[', ']') | ('\'', '\'')
    )
}

/// Classify the type of a query
fn classify_query_type(query: &Query) -> String {
    if query.with.is_some() {
        "WITH".to_string()
    } else {
        match &*query.body {
            SetExpr::Select(_) => "SELECT".to_string(),
            SetExpr::SetOperation { op, .. } => match op {
                ast::SetOperator::Union => "UNION".to_string(),
                ast::SetOperator::Intersect => "INTERSECT".to_string(),
                ast::SetOperator::Except => "EXCEPT".to_string(),
            },
            SetExpr::Values(_) => "VALUES".to_string(),
            _ => "SELECT".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(sql: &str) -> AnalyzeRequest {
        AnalyzeRequest {
            sql: sql.to_string(),
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
    fn test_dialect_case_sensitivity() {
        // Postgres normalizes to lowercase
        let pg_request = AnalyzeRequest {
            sql: "SELECT * FROM Users".to_string(),
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
    fn test_search_path_resolves_unqualified_table() {
        let request = AnalyzeRequest {
            sql: "SELECT * FROM orders".to_string(),
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
