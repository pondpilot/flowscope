//! Expression analysis for SQL AST nodes.
//!
//! This module provides the `ExpressionAnalyzer` for traversing and analyzing SQL expressions.
//! It handles:
//! - Subquery detection and recursive analysis
//! - Column reference extraction for lineage tracking
//! - Aggregate function detection (SUM, COUNT, etc.)
//! - Filter predicate capture for WHERE/HAVING clauses
//! - GROUP BY expression normalization
//!
//! The analyzer works with the parent `Analyzer` to build column-level lineage graphs
//! by identifying data flow from source columns through expressions to output columns.

use super::context::{ColumnRef, StatementContext};
use super::functions;
use super::Analyzer;
use crate::types::{AggregationInfo, FilterClauseType};
use sqlparser::ast::{self, Expr, FunctionArg, FunctionArgExpr};
use std::collections::HashSet;

/// Maximum recursion depth for expression traversal to prevent stack overflow
/// on maliciously crafted or deeply nested SQL expressions.
const MAX_RECURSION_DEPTH: usize = 100;

/// Analyzes SQL expressions to extract column references, detect aggregations,
/// and capture filter predicates.
///
/// `ExpressionAnalyzer` borrows both the parent `Analyzer` and the current
/// `StatementContext` to access schema information and contribute to the
/// lineage graph being built.
///
/// # Example
///
/// ```ignore
/// let mut expr_analyzer = ExpressionAnalyzer::new(analyzer, ctx);
/// expr_analyzer.analyze(&where_clause);
/// expr_analyzer.capture_filter_predicates(&where_clause, FilterClauseType::Where);
/// ```
pub(crate) struct ExpressionAnalyzer<'a, 'b> {
    pub(crate) analyzer: &'a mut Analyzer<'b>,
    pub(crate) ctx: &'a mut StatementContext,
}

impl<'a, 'b> ExpressionAnalyzer<'a, 'b> {
    /// Creates a new expression analyzer borrowing the parent analyzer and statement context.
    pub(crate) fn new(analyzer: &'a mut Analyzer<'b>, ctx: &'a mut StatementContext) -> Self {
        Self { analyzer, ctx }
    }

    /// Analyzes an expression for subqueries and validates column references.
    ///
    /// This method:
    /// 1. Recursively traverses the expression to find and analyze subqueries
    /// 2. Validates that referenced columns exist in their respective tables
    pub(crate) fn analyze(&mut self, expr: &Expr) {
        self.visit_expression_for_subqueries(expr, 0);
        self.analyzer
            .extract_column_refs_for_validation(self.ctx, expr);
    }

    /// Recursively visits an expression to find and analyze subqueries.
    ///
    /// The `depth` parameter tracks recursion depth to prevent stack overflow
    /// on deeply nested expressions.
    fn visit_expression_for_subqueries(&mut self, expr: &Expr, depth: usize) {
        if depth > MAX_RECURSION_DEPTH {
            return;
        }
        let next_depth = depth + 1;

        match expr {
            Expr::Subquery(query) => self.analyzer.analyze_query(self.ctx, query, None),
            Expr::InSubquery { subquery, .. } => {
                self.analyzer.analyze_query(self.ctx, subquery, None)
            }
            Expr::Exists { subquery, .. } => self.analyzer.analyze_query(self.ctx, subquery, None),
            Expr::BinaryOp { left, right, .. } => {
                self.visit_expression_for_subqueries(left, next_depth);
                self.visit_expression_for_subqueries(right, next_depth);
            }
            Expr::UnaryOp { expr, .. } => self.visit_expression_for_subqueries(expr, next_depth),
            Expr::Nested(expr) => self.visit_expression_for_subqueries(expr, next_depth),
            Expr::Case {
                operand,
                conditions,
                results,
                else_result,
            } => {
                if let Some(op) = operand {
                    self.visit_expression_for_subqueries(op, next_depth);
                }
                for cond in conditions {
                    self.visit_expression_for_subqueries(cond, next_depth);
                }
                for res in results {
                    self.visit_expression_for_subqueries(res, next_depth);
                }
                if let Some(el) = else_result {
                    self.visit_expression_for_subqueries(el, next_depth);
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
                            } => self.visit_expression_for_subqueries(e, next_depth),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Extracts all column references from an expression.
    ///
    /// Returns a vector of `ColumnRef` structs representing each column
    /// referenced in the expression, including those in nested function calls,
    /// CASE expressions, and binary operations.
    ///
    /// Subquery columns are not included as they are handled separately.
    pub(crate) fn extract_column_refs(expr: &Expr) -> Vec<ColumnRef> {
        let mut refs = Vec::new();
        Self::collect_column_refs(expr, &mut refs, 0);
        refs
    }

    fn collect_column_refs(expr: &Expr, refs: &mut Vec<ColumnRef>, depth: usize) {
        if depth > MAX_RECURSION_DEPTH {
            return;
        }
        let next_depth = depth + 1;

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
                Self::collect_column_refs(left, refs, next_depth);
                Self::collect_column_refs(right, refs, next_depth);
            }
            Expr::UnaryOp { expr, .. } => {
                Self::collect_column_refs(expr, refs, next_depth);
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
                                    Self::collect_column_refs(e, refs, next_depth);
                                }
                                FunctionArg::Named {
                                    arg: FunctionArgExpr::Expr(e),
                                    ..
                                } => {
                                    Self::collect_column_refs(e, refs, next_depth);
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
                    Self::collect_column_refs(op, refs, next_depth);
                }
                for cond in conditions {
                    Self::collect_column_refs(cond, refs, next_depth);
                }
                for res in results {
                    Self::collect_column_refs(res, refs, next_depth);
                }
                if let Some(el) = else_result {
                    Self::collect_column_refs(el, refs, next_depth);
                }
            }
            Expr::Cast { expr, .. } => {
                Self::collect_column_refs(expr, refs, next_depth);
            }
            Expr::Nested(inner) => {
                Self::collect_column_refs(inner, refs, next_depth);
            }
            Expr::Subquery(_) => {
                // Subquery columns are handled separately
            }
            Expr::InList { expr, list, .. } => {
                Self::collect_column_refs(expr, refs, next_depth);
                for item in list {
                    Self::collect_column_refs(item, refs, next_depth);
                }
            }
            Expr::Between {
                expr, low, high, ..
            } => {
                Self::collect_column_refs(expr, refs, next_depth);
                Self::collect_column_refs(low, refs, next_depth);
                Self::collect_column_refs(high, refs, next_depth);
            }
            Expr::IsNull(e) | Expr::IsNotNull(e) => {
                Self::collect_column_refs(e, refs, next_depth);
            }
            Expr::IsFalse(e) | Expr::IsNotFalse(e) | Expr::IsTrue(e) | Expr::IsNotTrue(e) => {
                Self::collect_column_refs(e, refs, next_depth);
            }
            Expr::Like { expr, pattern, .. } | Expr::ILike { expr, pattern, .. } => {
                Self::collect_column_refs(expr, refs, next_depth);
                Self::collect_column_refs(pattern, refs, next_depth);
            }
            Expr::Tuple(exprs) => {
                for e in exprs {
                    Self::collect_column_refs(e, refs, next_depth);
                }
            }
            Expr::Extract { expr, .. } => {
                Self::collect_column_refs(expr, refs, next_depth);
            }
            _ => {
                // Other expressions don't contain column references or are handled elsewhere
            }
        }
    }

    /// Normalizes a GROUP BY expression to a canonical string for comparison.
    ///
    /// This allows matching GROUP BY expressions with SELECT column references,
    /// handling cases like parenthesized expressions and compound identifiers.
    pub(crate) fn normalize_group_by_expr(&self, expr: &Expr) -> String {
        self.normalize_group_by_expr_inner(expr, 0)
    }

    fn normalize_group_by_expr_inner(&self, expr: &Expr, depth: usize) -> String {
        if depth > MAX_RECURSION_DEPTH {
            return expr.to_string().to_lowercase();
        }
        match expr {
            Expr::Identifier(ident) => self.analyzer.normalize_identifier(&ident.value),
            Expr::CompoundIdentifier(parts) => {
                // Use the full qualified name
                parts
                    .iter()
                    .map(|p| self.analyzer.normalize_identifier(&p.value))
                    .collect::<Vec<_>>()
                    .join(".")
            }
            Expr::Nested(inner) => {
                // Unwrap parentheses for matching: GROUP BY (col) should match SELECT col
                self.normalize_group_by_expr_inner(inner, depth + 1)
            }
            _ => {
                // For complex expressions, use the string representation
                expr.to_string().to_lowercase()
            }
        }
    }

    /// Detects aggregation information for an expression in the context of a GROUP BY query.
    ///
    /// Returns `Some(AggregationInfo)` if:
    /// - The expression is a grouping key (is_grouping_key = true)
    /// - The expression contains an aggregate function like SUM, COUNT, etc.
    ///
    /// Returns `None` for expressions that are neither grouping keys nor aggregates
    /// (e.g., constants).
    pub(crate) fn detect_aggregation(&self, expr: &Expr) -> Option<AggregationInfo> {
        if self.ctx.has_group_by {
            // Check if this expression is a grouping key
            let expr_normalized = self.normalize_group_by_expr(expr);
            if self.ctx.is_grouping_column(&expr_normalized) {
                return Some(AggregationInfo {
                    is_grouping_key: true,
                    function: None,
                    distinct: None,
                });
            }
        }

        // Check if the expression contains an aggregate function
        if let Some(agg_call) = self.find_aggregate_function(expr, 0) {
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

    fn find_aggregate_function(
        &self,
        expr: &Expr,
        depth: usize,
    ) -> Option<functions::AggregateCall> {
        if depth > MAX_RECURSION_DEPTH {
            return None;
        }
        let next_depth = depth + 1;

        match expr {
            Expr::Function(func) => self.check_function_for_aggregate(func, next_depth),
            Expr::BinaryOp { left, right, .. } => self
                .find_aggregate_function(left, next_depth)
                .or_else(|| self.find_aggregate_function(right, next_depth)),
            Expr::UnaryOp { expr, .. } | Expr::Nested(expr) | Expr::Cast { expr, .. } => {
                self.find_aggregate_function(expr, next_depth)
            }
            Expr::Case {
                operand,
                conditions,
                results,
                else_result,
            } => self.find_aggregate_in_case(operand, conditions, results, else_result, next_depth),
            _ => None,
        }
    }

    fn check_function_for_aggregate(
        &self,
        func: &ast::Function,
        depth: usize,
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
        self.find_aggregate_in_function_args(&func.args, depth)
    }

    fn find_aggregate_in_function_args(
        &self,
        args: &ast::FunctionArguments,
        depth: usize,
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
                    if let Some(agg) = self.find_aggregate_function(e, depth) {
                        return Some(agg);
                    }
                }
            }
        }
        None
    }

    fn find_aggregate_in_case(
        &self,
        operand: &Option<Box<Expr>>,
        conditions: &[Expr],
        results: &[Expr],
        else_result: &Option<Box<Expr>>,
        depth: usize,
    ) -> Option<functions::AggregateCall> {
        // Check operand (for CASE expr WHEN ...)
        if let Some(op) = operand {
            if let Some(agg) = self.find_aggregate_function(op, depth) {
                return Some(agg);
            }
        }

        // Check WHEN conditions
        for cond in conditions {
            if let Some(agg) = self.find_aggregate_function(cond, depth) {
                return Some(agg);
            }
        }

        // Check THEN results
        for result in results {
            if let Some(agg) = self.find_aggregate_function(result, depth) {
                return Some(agg);
            }
        }

        // Check ELSE result
        if let Some(else_r) = else_result {
            if let Some(agg) = self.find_aggregate_function(else_r, depth) {
                return Some(agg);
            }
        }

        None
    }

    /// Derives a column name from an expression for output column labeling.
    ///
    /// For simple column references, returns the column name. For functions,
    /// returns the function name. For other expressions, returns a generic
    /// name like "col_0", "col_1", etc.
    pub(crate) fn derive_column_name(&self, expr: &Expr, index: usize) -> String {
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

    /// Captures filter predicates from a WHERE/HAVING expression and attaches them to table nodes.
    ///
    /// This method splits the expression by top-level AND operators to localize
    /// predicates to specific tables, so each table node only shows the filters
    /// that directly reference its columns.
    pub(crate) fn capture_filter_predicates(&mut self, expr: &Expr, clause_type: FilterClauseType) {
        // Split by AND and process each predicate separately
        let predicates = Self::split_by_and(expr);

        for predicate in predicates {
            // Extract column references from this specific predicate
            let column_refs = Self::extract_column_refs(predicate);

            // Find unique tables referenced in this predicate
            let mut affected_tables: HashSet<String> = HashSet::new();
            for col_ref in &column_refs {
                if let Some(table_canonical) = self.analyzer.resolve_column_table(
                    self.ctx,
                    col_ref.table.as_deref(),
                    &col_ref.column,
                ) {
                    affected_tables.insert(table_canonical);
                }
            }

            // If we couldn't resolve columns to specific tables (e.g., columns from
            // functions without clear table references, or ambiguous column names),
            // apply the filter to all tables in the current scope as a conservative
            // fallback. This may be imprecise for complex multi-table expressions,
            // but ensures the filter is captured rather than lost.
            if affected_tables.is_empty() && !column_refs.is_empty() {
                for table in self.ctx.tables_in_current_scope() {
                    affected_tables.insert(table);
                }
            }

            // Add this specific predicate to affected table nodes
            let filter_text = predicate.to_string();
            for table_canonical in &affected_tables {
                self.ctx
                    .add_filter_for_table(table_canonical, filter_text.clone(), clause_type);
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

    fn collect_and_predicates<'c>(expr: &'c Expr, predicates: &mut Vec<&'c Expr>) {
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
}
