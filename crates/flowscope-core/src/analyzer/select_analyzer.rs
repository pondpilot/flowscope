use super::context::StatementContext;
use super::expression::ExpressionAnalyzer;
use super::helpers::{infer_expr_type, is_simple_column_ref};
use super::query::OutputColumnParams;
use super::Analyzer;
use crate::types::FilterClauseType;
use sqlparser::ast::{self, Select, SelectItem};
use std::collections::HashSet;

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

                    // Check if this expression matches a SELECT alias (validation only)
                    // This check happens before projection analysis, so output_columns
                    // would typically be empty. For proper alias checking, we'd need
                    // to analyze projection first, but that changes semantics.
                    // For now, we only check if alias_in_group_by is forbidden and
                    // the expression matches an existing output column name.
                    let matched_alias = self
                        .ctx
                        .output_columns
                        .iter()
                        .find(|c| c.name == expr_str)
                        .map(|c| c.name.clone());

                    if let Some(alias_name) = matched_alias {
                        if !dialect.alias_in_group_by() {
                            use crate::types::{issue_codes, Issue};
                            let statement_index = self.ctx.statement_index;
                            self.analyzer.issues.push(
                                Issue::warning(
                                    issue_codes::UNSUPPORTED_SYNTAX,
                                    format!(
                                        "Dialect '{dialect:?}' does not support referencing aliases in GROUP BY (alias '{alias_name}' used). This may fail at runtime."
                                    ),
                                )
                                .with_statement(statement_index),
                            );
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
        let dialect = self.analyzer.request.dialect;
        for (idx, item) in projection.iter().enumerate() {
            match item {
                SelectItem::UnnamedExpr(expr) => {
                    let (sources, name, aggregation) = {
                        let ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
                        (
                            ExpressionAnalyzer::extract_column_refs_with_dialect(expr, dialect),
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
                            target_node: self.target_node.clone(),
                            approximate: false,
                            aggregation,
                        },
                    );
                }
                SelectItem::ExprWithAlias { expr, alias } => {
                    let (sources, aggregation) = {
                        let ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
                        (
                            ExpressionAnalyzer::extract_column_refs_with_dialect(expr, dialect),
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

    fn analyze_selection(&mut self, selection: &Option<sqlparser::ast::Expr>) {
        if let Some(ref where_clause) = selection {
            let mut ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
            ea.analyze(where_clause);
            ea.capture_filter_predicates(where_clause, FilterClauseType::Where);
        }
    }

    fn analyze_having(&mut self, having: &Option<sqlparser::ast::Expr>) {
        if let Some(ref having) = having {
            let mut ea = ExpressionAnalyzer::new(self.analyzer, self.ctx);
            ea.analyze(having);
            ea.capture_filter_predicates(having, FilterClauseType::Having);
        }
    }
}
