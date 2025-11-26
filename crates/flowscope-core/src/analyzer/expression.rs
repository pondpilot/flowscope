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
use crate::generated;
use crate::types::{AggregationInfo, FilterClauseType};
use crate::Dialect;
use sqlparser::ast::{self, Expr, FunctionArg, FunctionArgExpr};
use std::collections::HashSet;
#[cfg(feature = "tracing")]
use tracing::debug;

/// Maximum recursion depth for expression traversal to prevent stack overflow
/// on maliciously crafted or deeply nested SQL expressions.
pub(super) const MAX_RECURSION_DEPTH: usize = 100;

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
        self.validate_column_refs(expr);
    }

    /// Extracts column references from an expression and validates each one.
    fn validate_column_refs(&mut self, expr: &Expr) {
        let column_refs = self.extract_column_refs_with_warning(expr);
        for col_ref in column_refs {
            if let Some(table) = col_ref.table.as_deref() {
                if let Some(canonical) = self.analyzer.resolve_table_alias(self.ctx, Some(table)) {
                    self.analyzer
                        .validate_column(self.ctx, &canonical, &col_ref.column);
                }
            }
        }
    }

    /// Recursively visits an expression to find and analyze subqueries.
    ///
    /// The `depth` parameter tracks recursion depth to prevent stack overflow
    /// on deeply nested expressions.
    fn visit_expression_for_subqueries(&mut self, expr: &Expr, depth: usize) {
        if depth > MAX_RECURSION_DEPTH {
            self.analyzer
                .emit_depth_limit_warning(self.ctx.statement_index);
            #[cfg(feature = "tracing")]
            debug!(
                depth,
                "Max recursion depth exceeded in visit_expression_for_subqueries"
            );
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
                else_result,
                ..
            } => {
                if let Some(op) = operand {
                    self.visit_expression_for_subqueries(op, next_depth);
                }
                for case_when in conditions {
                    self.visit_expression_for_subqueries(&case_when.condition, next_depth);
                    self.visit_expression_for_subqueries(&case_when.result, next_depth);
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

    /// Extracts column references and emits a depth-limit warning if needed.
    ///
    /// This is a convenience wrapper around `extract_column_refs_with_dialect`
    /// that handles emitting the depth-limit warning to the analyzer. Use this
    /// when you have access to an `ExpressionAnalyzer` instance.
    pub(crate) fn extract_column_refs_with_warning(&mut self, expr: &Expr) -> Vec<ColumnRef> {
        let dialect = self.analyzer.request.dialect;
        let (refs, depth_limited) = Self::extract_column_refs_with_dialect(expr, dialect);
        if depth_limited {
            self.analyzer
                .emit_depth_limit_warning(self.ctx.statement_index);
        }
        refs
    }

    /// Extracts all column references from an expression (dialect-aware).
    ///
    /// Returns a vector of `ColumnRef` structs representing each column
    /// referenced in the expression, including those in nested function calls,
    /// CASE expressions, and binary operations.
    ///
    /// The dialect parameter is used to determine which function arguments
    /// should be skipped (e.g., date unit keywords vary by dialect).
    ///
    /// Subquery columns are not included as they are handled separately.
    pub(crate) fn extract_column_refs_with_dialect(
        expr: &Expr,
        dialect: Dialect,
    ) -> (Vec<ColumnRef>, bool) {
        let mut refs = Vec::new();
        let depth_limited = Self::collect_column_refs(expr, &mut refs, dialect, 0);
        (refs, depth_limited)
    }

    fn collect_column_refs(
        expr: &Expr,
        refs: &mut Vec<ColumnRef>,
        dialect: Dialect,
        depth: usize,
    ) -> bool {
        if depth > MAX_RECURSION_DEPTH {
            #[cfg(feature = "tracing")]
            debug!(depth, "Max recursion depth exceeded in collect_column_refs");
            return true;
        }
        let next_depth = depth + 1;
        let mut depth_limited = false;

        match expr {
            Expr::Identifier(ident) => {
                refs.push(ColumnRef {
                    table: None,
                    column: ident.value.clone(),
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
                    });
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                depth_limited |= Self::collect_column_refs(left, refs, dialect, next_depth);
                depth_limited |= Self::collect_column_refs(right, refs, dialect, next_depth);
            }
            Expr::UnaryOp { expr, .. } => {
                depth_limited |= Self::collect_column_refs(expr, refs, dialect, next_depth);
            }
            Expr::Function(func) => {
                let func_name = func.name.to_string();
                let skip_indices = generated::skip_args_for_function(dialect, &func_name);
                match &func.args {
                    ast::FunctionArguments::List(arg_list) => {
                        for (idx, arg) in arg_list.args.iter().enumerate() {
                            // Check if this argument should be skipped (e.g., date unit keywords)
                            if skip_indices.contains(&idx) {
                                continue;
                            }
                            match arg {
                                FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => {
                                    depth_limited |=
                                        Self::collect_column_refs(e, refs, dialect, next_depth);
                                }
                                FunctionArg::Named {
                                    arg: FunctionArgExpr::Expr(e),
                                    ..
                                } => {
                                    depth_limited |=
                                        Self::collect_column_refs(e, refs, dialect, next_depth);
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
                else_result,
                ..
            } => {
                if let Some(op) = operand {
                    depth_limited |= Self::collect_column_refs(op, refs, dialect, next_depth);
                }
                for case_when in conditions {
                    depth_limited |=
                        Self::collect_column_refs(&case_when.condition, refs, dialect, next_depth);
                    depth_limited |=
                        Self::collect_column_refs(&case_when.result, refs, dialect, next_depth);
                }
                if let Some(el) = else_result {
                    depth_limited |= Self::collect_column_refs(el, refs, dialect, next_depth);
                }
            }
            Expr::Cast { expr, .. } => {
                depth_limited |= Self::collect_column_refs(expr, refs, dialect, next_depth);
            }
            Expr::Nested(inner) => {
                depth_limited |= Self::collect_column_refs(inner, refs, dialect, next_depth);
            }
            Expr::Subquery(_) => {
                // Subquery columns are handled separately
            }
            Expr::InList { expr, list, .. } => {
                depth_limited |= Self::collect_column_refs(expr, refs, dialect, next_depth);
                for item in list {
                    depth_limited |= Self::collect_column_refs(item, refs, dialect, next_depth);
                }
            }
            Expr::Between {
                expr, low, high, ..
            } => {
                depth_limited |= Self::collect_column_refs(expr, refs, dialect, next_depth);
                depth_limited |= Self::collect_column_refs(low, refs, dialect, next_depth);
                depth_limited |= Self::collect_column_refs(high, refs, dialect, next_depth);
            }
            Expr::IsNull(e) | Expr::IsNotNull(e) => {
                depth_limited |= Self::collect_column_refs(e, refs, dialect, next_depth);
            }
            Expr::IsFalse(e) | Expr::IsNotFalse(e) | Expr::IsTrue(e) | Expr::IsNotTrue(e) => {
                depth_limited |= Self::collect_column_refs(e, refs, dialect, next_depth);
            }
            Expr::Like { expr, pattern, .. } | Expr::ILike { expr, pattern, .. } => {
                depth_limited |= Self::collect_column_refs(expr, refs, dialect, next_depth);
                depth_limited |= Self::collect_column_refs(pattern, refs, dialect, next_depth);
            }
            Expr::Tuple(exprs) => {
                for e in exprs {
                    depth_limited |= Self::collect_column_refs(e, refs, dialect, next_depth);
                }
            }
            Expr::Extract { expr, .. } => {
                depth_limited |= Self::collect_column_refs(expr, refs, dialect, next_depth);
            }
            _ => {
                // Other expressions don't contain column references or are handled elsewhere
            }
        }

        depth_limited
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
            #[cfg(feature = "tracing")]
            debug!(
                depth,
                "Max recursion depth exceeded in normalize_group_by_expr_inner"
            );
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
            #[cfg(feature = "tracing")]
            debug!(
                depth,
                "Max recursion depth exceeded in find_aggregate_function"
            );
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
                else_result,
                ..
            } => self.find_aggregate_in_case(operand, conditions, else_result, next_depth),
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
        conditions: &[ast::CaseWhen],
        else_result: &Option<Box<Expr>>,
        depth: usize,
    ) -> Option<functions::AggregateCall> {
        // Check operand (for CASE expr WHEN ...)
        if let Some(op) = operand {
            if let Some(agg) = self.find_aggregate_function(op, depth) {
                return Some(agg);
            }
        }

        // Check WHEN conditions and THEN results
        for case_when in conditions {
            if let Some(agg) = self.find_aggregate_function(&case_when.condition, depth) {
                return Some(agg);
            }
            if let Some(agg) = self.find_aggregate_function(&case_when.result, depth) {
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
            let column_refs = self.extract_column_refs_with_warning(predicate);

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

    /// Extracts simple unqualified identifiers from an expression.
    ///
    /// Returns a set of identifier names that appear as bare identifiers (not qualified
    /// with table names) in the expression. Used for alias visibility checking.
    pub(crate) fn extract_simple_identifiers(expr: &Expr) -> HashSet<String> {
        let mut identifiers = HashSet::new();
        Self::collect_simple_identifiers(expr, &mut identifiers, 0);
        identifiers
    }

    fn collect_simple_identifiers(expr: &Expr, identifiers: &mut HashSet<String>, depth: usize) {
        if depth > MAX_RECURSION_DEPTH {
            #[cfg(feature = "tracing")]
            debug!(
                depth,
                "Max recursion depth exceeded in collect_simple_identifiers"
            );
            return;
        }
        let next_depth = depth + 1;

        match expr {
            // Simple identifier - the target of our collection
            Expr::Identifier(ident) => {
                identifiers.insert(ident.value.clone());
            }

            // Single expression wrappers
            Expr::UnaryOp { expr: e, .. }
            | Expr::Cast { expr: e, .. }
            | Expr::Nested(e)
            | Expr::Extract { expr: e, .. }
            | Expr::Ceil { expr: e, .. }
            | Expr::Floor { expr: e, .. }
            | Expr::IsNull(e)
            | Expr::IsNotNull(e)
            | Expr::IsFalse(e)
            | Expr::IsNotFalse(e)
            | Expr::IsTrue(e)
            | Expr::IsNotTrue(e)
            | Expr::IsUnknown(e)
            | Expr::IsNotUnknown(e)
            | Expr::JsonAccess { value: e, .. } => {
                Self::collect_simple_identifiers(e, identifiers, next_depth);
            }

            // Two expression patterns (left/right)
            Expr::BinaryOp { left, right, .. }
            | Expr::AnyOp { left, right, .. }
            | Expr::AllOp { left, right, .. } => {
                Self::collect_simple_identifiers(left, identifiers, next_depth);
                Self::collect_simple_identifiers(right, identifiers, next_depth);
            }

            // Two expression patterns (expr/pattern)
            Expr::Like { expr, pattern, .. }
            | Expr::ILike { expr, pattern, .. }
            | Expr::SimilarTo { expr, pattern, .. }
            | Expr::RLike { expr, pattern, .. } => {
                Self::collect_simple_identifiers(expr, identifiers, next_depth);
                Self::collect_simple_identifiers(pattern, identifiers, next_depth);
            }

            // Two expression patterns (other)
            Expr::Position { expr, r#in } => {
                Self::collect_simple_identifiers(expr, identifiers, next_depth);
                Self::collect_simple_identifiers(r#in, identifiers, next_depth);
            }
            Expr::AtTimeZone {
                timestamp,
                time_zone,
            } => {
                Self::collect_simple_identifiers(timestamp, identifiers, next_depth);
                Self::collect_simple_identifiers(time_zone, identifiers, next_depth);
            }
            Expr::InUnnest {
                expr, array_expr, ..
            } => {
                Self::collect_simple_identifiers(expr, identifiers, next_depth);
                Self::collect_simple_identifiers(array_expr, identifiers, next_depth);
            }
            Expr::IsDistinctFrom(e1, e2) | Expr::IsNotDistinctFrom(e1, e2) => {
                Self::collect_simple_identifiers(e1, identifiers, next_depth);
                Self::collect_simple_identifiers(e2, identifiers, next_depth);
            }

            // Three expression patterns
            Expr::Between {
                expr, low, high, ..
            } => {
                Self::collect_simple_identifiers(expr, identifiers, next_depth);
                Self::collect_simple_identifiers(low, identifiers, next_depth);
                Self::collect_simple_identifiers(high, identifiers, next_depth);
            }

            // List patterns
            Expr::Tuple(exprs) => {
                for e in exprs {
                    Self::collect_simple_identifiers(e, identifiers, next_depth);
                }
            }
            Expr::InList { expr, list, .. } => {
                Self::collect_simple_identifiers(expr, identifiers, next_depth);
                for e in list {
                    Self::collect_simple_identifiers(e, identifiers, next_depth);
                }
            }

            // Function arguments
            Expr::Function(func) => {
                if let ast::FunctionArguments::List(arg_list) = &func.args {
                    for arg in &arg_list.args {
                        match arg {
                            FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
                            | FunctionArg::Named {
                                arg: FunctionArgExpr::Expr(e),
                                ..
                            } => Self::collect_simple_identifiers(e, identifiers, next_depth),
                            _ => {}
                        }
                    }
                }
            }

            // CASE expression
            Expr::Case {
                operand,
                conditions,
                else_result,
                ..
            } => {
                if let Some(op) = operand {
                    Self::collect_simple_identifiers(op, identifiers, next_depth);
                }
                for case_when in conditions {
                    Self::collect_simple_identifiers(&case_when.condition, identifiers, next_depth);
                    Self::collect_simple_identifiers(&case_when.result, identifiers, next_depth);
                }
                if let Some(el) = else_result {
                    Self::collect_simple_identifiers(el, identifiers, next_depth);
                }
            }

            // SUBSTRING with optional parts
            Expr::Substring {
                expr,
                substring_from,
                substring_for,
                ..
            } => {
                Self::collect_simple_identifiers(expr, identifiers, next_depth);
                if let Some(from) = substring_from {
                    Self::collect_simple_identifiers(from, identifiers, next_depth);
                }
                if let Some(for_expr) = substring_for {
                    Self::collect_simple_identifiers(for_expr, identifiers, next_depth);
                }
            }

            // Skip subqueries - they have their own scope
            Expr::Subquery(_) | Expr::InSubquery { .. } | Expr::Exists { .. } => {}

            // Skip qualified names (table.column) - not simple identifiers
            Expr::CompoundIdentifier(_) => {}

            // Other expressions don't contain identifiers we care about
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_column_refs_reports_depth_limit() {
        let expr = Expr::Identifier(ast::Ident::new("col"));
        let mut refs = Vec::new();
        let hit = ExpressionAnalyzer::collect_column_refs(
            &expr,
            &mut refs,
            Dialect::Generic,
            MAX_RECURSION_DEPTH + 1,
        );
        assert!(hit, "expected depth guard to trigger");
        assert!(
            refs.is_empty(),
            "no column refs should be recorded when guard triggers"
        );
    }
}
