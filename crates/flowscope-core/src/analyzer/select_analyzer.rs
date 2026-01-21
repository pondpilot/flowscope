use super::context::{ColumnRef, StatementContext};
use super::expression::ExpressionAnalyzer;
use super::helpers::{
    alias_visibility_warning, infer_expr_type, is_simple_column_ref, lateral_alias_warning,
};
use super::query::OutputColumnParams;
use super::Analyzer;
use crate::types::FilterClauseType;
use sqlparser::ast::{self, Select, SelectItem};
use std::collections::{HashMap, HashSet};

/// Analyzes SELECT statements to extract column lineage.
pub(crate) struct SelectAnalyzer<'a, 'b> {
    analyzer: &'a mut Analyzer<'b>,
    ctx: &'a mut StatementContext,
    target_node: Option<String>,
}

impl<'a, 'b> SelectAnalyzer<'a, 'b> {
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

    /// Analyze a SELECT statement's projection, group by, and filter clauses.
    ///
    /// This method populates:
    /// - Output columns in the context
    /// - Filter predicates
    /// - Aggregation info
    pub(crate) fn analyze(&mut self, select: &Select) {
        self.ctx.clear_grouping();

        self.analyze_group_by(&select.group_by);
        self.analyze_projection(&select.projection);
        self.analyze_selection(&select.selection);
        self.analyze_having(&select.having);
    }

    /// Analyzes GROUP BY expressions to track grouping columns.
    ///
    /// # Limitations
    ///
    /// TODO: GROUP BY alias visibility checking is incomplete because GROUP BY is
    /// analyzed before the SELECT projection. This means `output_columns` is typically
    /// empty when we try to detect alias references. A multi-pass analysis approach
    /// would be needed to properly detect aliases used in GROUP BY, which would require:
    /// 1. First pass: collect SELECT aliases
    /// 2. Second pass: analyze GROUP BY with alias knowledge
    /// 3. Third pass: analyze projection with grouping context
    ///
    /// For now, this check only catches edge cases where output_columns were populated
    /// from a previous statement or context.
    fn analyze_group_by(&mut self, group_by: &ast::GroupByExpr) {
        let dialect = self.analyzer.request.dialect;
        match group_by {
            ast::GroupByExpr::Expressions(exprs, _) => {
                let mut processed_grouping_exprs = HashSet::new();
                for group_by_expr in exprs {
                    // Normalize expression first without creating expr_analyzer yet
                    let expr_str = {
                        let ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
                        ea.normalize_group_by_expr(group_by_expr)
                    };

                    // Alias visibility check (limited - see function doc comment for details)
                    let matched_alias = self
                        .ctx
                        .output_columns
                        .iter()
                        .find(|c| c.name == expr_str)
                        .map(|c| c.name.clone());

                    if let Some(alias_name) = matched_alias {
                        if !dialect.alias_in_group_by() {
                            self.emit_alias_warning("GROUP BY", &alias_name);
                        }
                    }

                    if !processed_grouping_exprs.insert(expr_str.clone()) {
                        continue;
                    }

                    // Now create expr_analyzer for the actual analysis
                    let mut expr_analyzer = ExpressionAnalyzer::new(self.analyzer, self.ctx);
                    expr_analyzer.ctx.add_grouping_column(expr_str);
                    expr_analyzer.analyze(group_by_expr);
                }
            }
            ast::GroupByExpr::All(_) => {
                self.ctx.has_group_by = true;
            }
        }
    }

    fn analyze_projection(&mut self, projection: &[SelectItem]) {
        // Track aliases defined in this SELECT list for lateral column alias checking.
        // For dialects that support lateral aliases, we also track the sources so we can
        // resolve references to them in subsequent SELECT items.
        let mut defined_aliases: HashSet<String> = HashSet::new();
        // Maps normalized alias name -> sources (column refs that compose the alias)
        let mut lateral_alias_sources: HashMap<String, Vec<ColumnRef>> = HashMap::new();

        let dialect = self.analyzer.request.dialect;
        let supports_lateral = dialect.lateral_column_alias();

        for (idx, item) in projection.iter().enumerate() {
            match item {
                SelectItem::UnnamedExpr(expr) => {
                    // Check for lateral column alias usage
                    self.check_lateral_column_alias(expr, &defined_aliases);

                    let (mut sources, name, aggregation) = {
                        let mut ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
                        let column_refs = ea.extract_column_refs_with_warning(expr);
                        (
                            column_refs,
                            ea.derive_column_name(expr, idx),
                            ea.detect_aggregation(expr),
                        )
                    };

                    // Resolve lateral alias references if dialect supports them
                    if supports_lateral {
                        sources = self.resolve_lateral_alias_sources(
                            expr,
                            sources,
                            &lateral_alias_sources,
                        );
                    }

                    let expr_text = if is_simple_column_ref(expr) {
                        None
                    } else {
                        Some(expr.to_string())
                    };
                    let data_type = infer_expr_type(expr).map(|t| t.to_string());

                    self.record_source_columns_with_type(&sources, &data_type);

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
                    // Check for lateral column alias usage
                    self.check_lateral_column_alias(expr, &defined_aliases);

                    let (mut sources, aggregation) = {
                        let mut ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
                        let column_refs = ea.extract_column_refs_with_warning(expr);
                        (column_refs, ea.detect_aggregation(expr))
                    };

                    // Resolve lateral alias references if dialect supports them
                    if supports_lateral {
                        sources = self.resolve_lateral_alias_sources(
                            expr,
                            sources,
                            &lateral_alias_sources,
                        );
                    }

                    let name = alias.value.clone();
                    let expr_text = if is_simple_column_ref(expr) {
                        None
                    } else {
                        Some(expr.to_string())
                    };
                    let data_type = infer_expr_type(expr).map(|t| t.to_string());

                    self.record_source_columns_with_type(&sources, &data_type);

                    // Record this alias for subsequent lateral column alias checking
                    let normalized_alias = self.analyzer.normalize_identifier(&name);
                    defined_aliases.insert(normalized_alias.clone());

                    // Track sources for lateral alias resolution in subsequent items
                    if supports_lateral {
                        lateral_alias_sources.insert(normalized_alias, sources.clone());
                    }

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
    }

    /// Resolves lateral column alias references in the sources.
    ///
    /// For dialects that support lateral column aliases (BigQuery, Snowflake, etc.),
    /// when an unqualified identifier in the expression matches a previously-defined
    /// alias, we replace that identifier's source with the sources of the alias.
    ///
    /// Example: `SELECT a + 1 AS b, b + 1 AS c FROM t`
    /// When processing `c`, the identifier `b` matches the lateral alias, so we
    /// resolve `c`'s sources to include `t.a` (via `b`) instead of treating `b`
    /// as an unresolved column reference.
    fn resolve_lateral_alias_sources(
        &self,
        expr: &sqlparser::ast::Expr,
        mut sources: Vec<ColumnRef>,
        lateral_alias_sources: &HashMap<String, Vec<ColumnRef>>,
    ) -> Vec<ColumnRef> {
        if lateral_alias_sources.is_empty() {
            return sources;
        }

        // Find unqualified identifiers in the expression that match lateral aliases
        let identifiers = ExpressionAnalyzer::extract_simple_identifiers(expr);
        let mut additional_sources = Vec::new();

        for ident in &identifiers {
            let normalized_ident = self.analyzer.normalize_identifier(ident);
            if let Some(alias_sources) = lateral_alias_sources.get(&normalized_ident) {
                // This identifier is a lateral alias reference. Add the alias's sources
                // to our sources list, and remove any ColumnRef that just has the alias
                // name without a table (since it's not a real table column).
                additional_sources.extend(alias_sources.clone());

                // Remove the unresolved reference to the alias itself
                sources.retain(|s| {
                    !(s.table.is_none()
                        && self.analyzer.normalize_identifier(&s.column) == normalized_ident)
                });
            }
        }

        sources.extend(additional_sources);
        sources
    }

    /// Emits a warning for unsupported alias usage in a clause.
    fn emit_alias_warning(&mut self, clause_name: &str, alias_name: &str) {
        let dialect = self.analyzer.request.dialect;
        let statement_index = self.ctx.statement_index;
        self.analyzer.issues.push(alias_visibility_warning(
            dialect,
            clause_name,
            alias_name,
            statement_index,
        ));
    }

    /// Checks if an expression uses a lateral column alias (an alias defined earlier
    /// in the same SELECT list) and emits a warning if the dialect doesn't support it.
    fn check_lateral_column_alias(
        &mut self,
        expr: &sqlparser::ast::Expr,
        defined_aliases: &HashSet<String>,
    ) {
        let dialect = self.analyzer.request.dialect;

        if !dialect.lateral_column_alias() && !defined_aliases.is_empty() {
            let identifiers = ExpressionAnalyzer::extract_simple_identifiers(expr);
            for ident in &identifiers {
                let normalized_ident = self.analyzer.normalize_identifier(ident);
                if defined_aliases.contains(&normalized_ident) {
                    let statement_index = self.ctx.statement_index;
                    self.analyzer.issues.push(lateral_alias_warning(
                        dialect,
                        ident,
                        statement_index,
                    ));
                }
            }
        }
    }

    fn analyze_selection(&mut self, selection: &Option<sqlparser::ast::Expr>) {
        if let Some(ref where_clause) = selection {
            let mut ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
            ea.analyze(where_clause);
            ea.capture_filter_predicates(where_clause, FilterClauseType::Where);
        }
    }

    fn analyze_having(&mut self, having: &Option<sqlparser::ast::Expr>) {
        if let Some(ref having_expr) = having {
            let dialect = self.analyzer.request.dialect;

            // Check for alias usage in HAVING clause
            if !dialect.alias_in_having() {
                self.check_alias_in_clause(having_expr, "HAVING");
            }

            let mut ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
            ea.analyze(having_expr);
            ea.capture_filter_predicates(having_expr, FilterClauseType::Having);
        }
    }

    /// Checks if an expression references any output column aliases and emits a warning.
    ///
    /// Used by HAVING and can be extended to other clauses that need alias checking.
    fn check_alias_in_clause(&mut self, expr: &sqlparser::ast::Expr, clause_name: &str) {
        let identifiers = ExpressionAnalyzer::extract_simple_identifiers(expr);
        for ident in &identifiers {
            let normalized_ident = self.analyzer.normalize_identifier(ident);
            if let Some(alias_name) = self
                .ctx
                .output_columns
                .iter()
                .find(|c| self.analyzer.normalize_identifier(&c.name) == normalized_ident)
                .map(|c| c.name.clone())
            {
                self.emit_alias_warning(clause_name, &alias_name);
            }
        }
    }

    /// Record source columns with inferred data types for implied schema tracking.
    fn record_source_columns_with_type(
        &mut self,
        sources: &[ColumnRef],
        data_type: &Option<String>,
    ) {
        let Some(ref dt) = data_type else { return };

        for col_ref in sources {
            let Some(table) = col_ref.table.as_deref() else {
                continue;
            };
            let Some(canonical) = self.analyzer.resolve_table_alias(self.ctx, Some(table)) else {
                continue;
            };
            self.ctx
                .record_source_column(&canonical, &col_ref.column, Some(dt.clone()));
        }
    }
}
