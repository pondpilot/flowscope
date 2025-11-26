//! Visitor pattern for AST traversal and lineage analysis.
//!
//! This module provides a visitor-based approach to traversing SQL AST nodes
//! and building lineage graphs. It separates traversal logic (the `Visitor` trait)
//! from analysis logic (the `LineageVisitor` implementation).

use super::context::StatementContext;
use super::expression::ExpressionAnalyzer;
use super::helpers::generate_node_id;
use super::select_analyzer::SelectAnalyzer;
use super::Analyzer;
use crate::types::{issue_codes, Issue, Node, NodeType};
use sqlparser::ast::{
    self, Cte, Expr, Ident, Join, Query, Select, SetExpr, SetOperator, Statement, TableAlias,
    TableFactor, TableWithJoins, Values,
};
use std::sync::Arc;

/// A visitor trait for traversing the SQL AST.
///
/// This trait defines default behavior for visiting nodes (traversing children).
/// Implementors can override specific methods to add custom logic.
pub trait Visitor {
    fn visit_statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Query(query) => self.visit_query(query),
            Statement::Insert(insert) => {
                if let Some(source) = &insert.source {
                    self.visit_query(source);
                }
            }
            Statement::CreateTable(create) => {
                if let Some(query) = &create.query {
                    self.visit_query(query);
                }
            }
            Statement::CreateView { query, .. } => self.visit_query(query),
            _ => {}
        }
    }

    fn visit_query(&mut self, query: &Query) {
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.visit_cte(cte);
            }
        }
        self.visit_set_expr(&query.body);
    }

    fn visit_cte(&mut self, cte: &Cte) {
        self.visit_query(&cte.query);
    }

    fn visit_set_expr(&mut self, set_expr: &SetExpr) {
        match set_expr {
            SetExpr::Select(select) => self.visit_select(select),
            SetExpr::Query(query) => self.visit_query(query),
            SetExpr::SetOperation { left, right, .. } => {
                self.visit_set_expr(left);
                self.visit_set_expr(right);
            }
            SetExpr::Values(values) => self.visit_values(values),
            SetExpr::Insert(stmt) => self.visit_statement(stmt),
            _ => {}
        }
    }

    fn visit_select(&mut self, select: &Select) {
        for from in &select.from {
            self.visit_table_with_joins(from);
        }
    }

    fn visit_table_with_joins(&mut self, table: &TableWithJoins) {
        self.visit_table_factor(&table.relation);
        for join in &table.joins {
            self.visit_join(join);
        }
    }

    fn visit_table_factor(&mut self, table: &TableFactor) {
        match table {
            TableFactor::Derived { subquery, .. } => self.visit_query(subquery),
            TableFactor::NestedJoin {
                table_with_joins, ..
            } => self.visit_table_with_joins(table_with_joins),
            _ => {}
        }
    }

    fn visit_join(&mut self, join: &Join) {
        self.visit_table_factor(&join.relation);
    }

    fn visit_values(&mut self, values: &Values) {
        for row in &values.rows {
            for expr in row {
                self.visit_expr(expr);
            }
        }
    }

    fn visit_expr(&mut self, _expr: &Expr) {}
}

/// Visitor implementation that builds the lineage graph.
pub(crate) struct LineageVisitor<'a, 'b> {
    pub(crate) analyzer: &'a mut Analyzer<'b>,
    pub(crate) ctx: &'a mut StatementContext,
    pub(crate) target_node: Option<String>,
}

impl<'a, 'b> LineageVisitor<'a, 'b> {
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

    #[inline]
    pub fn target_from_arc(arc: Option<&Arc<str>>) -> Option<String> {
        arc.map(|s| s.to_string())
    }

    pub fn set_target_node(&mut self, target: Option<String>) {
        self.target_node = target;
    }

    pub fn set_last_operation(&mut self, op: Option<String>) {
        self.ctx.last_operation = op;
    }

    pub fn add_source_table(&mut self, table_name: &str) -> Option<String> {
        self.analyzer
            .add_source_table(self.ctx, table_name, self.target_node.as_deref())
    }

    pub fn analyze_dml_target(
        &mut self,
        table_name: &str,
        alias: Option<&TableAlias>,
    ) -> Option<(String, Arc<str>)> {
        let canonical_res = self.analyzer.add_source_table(self.ctx, table_name, None);
        let canonical = canonical_res
            .clone()
            .unwrap_or_else(|| self.analyzer.normalize_table_name(table_name));

        if let (Some(a), Some(canonical_name)) = (alias, canonical_res) {
            self.ctx
                .table_aliases
                .insert(a.name.to_string(), canonical_name);
        }

        let node_id = self
            .ctx
            .table_node_ids
            .get(&canonical)
            .cloned()
            .unwrap_or_else(|| self.analyzer.relation_node_id(&canonical));

        self.analyzer
            .tracker
            .record_produced(&canonical, self.ctx.statement_index);
        self.analyzer
            .add_table_columns_from_schema(self.ctx, &canonical, &node_id);

        Some((canonical, node_id))
    }

    pub fn analyze_dml_target_factor(&mut self, table: &TableFactor) -> Option<Arc<str>> {
        if let TableFactor::Table { name, alias, .. } = table {
            let table_name = name.to_string();
            self.analyze_dml_target(&table_name, alias.as_ref())
                .map(|(_, node_id)| node_id)
        } else {
            self.visit_table_factor(table);
            None
        }
    }

    pub fn analyze_dml_target_from_table_with_joins(
        &mut self,
        table: &TableWithJoins,
    ) -> Option<Arc<str>> {
        if let TableFactor::Table { name, alias, .. } = &table.relation {
            let table_name = name.to_string();
            self.analyze_dml_target(&table_name, alias.as_ref())
                .map(|(_, node_id)| node_id)
        } else {
            self.visit_table_with_joins(table);
            None
        }
    }

    pub fn register_aliases_in_table_with_joins(&mut self, table_with_joins: &TableWithJoins) {
        self.register_aliases_in_table_factor(&table_with_joins.relation);
        for join in &table_with_joins.joins {
            self.register_aliases_in_table_factor(&join.relation);
        }
    }

    fn register_aliases_in_table_factor(&mut self, table_factor: &TableFactor) {
        match table_factor {
            TableFactor::Table {
                name,
                alias: Some(a),
                ..
            } => {
                let canonical = self
                    .analyzer
                    .canonicalize_table_reference(&name.to_string())
                    .canonical;
                self.ctx.table_aliases.insert(a.name.to_string(), canonical);
            }
            TableFactor::Derived { alias: Some(a), .. } => {
                self.ctx.subquery_aliases.insert(a.name.to_string());
            }
            TableFactor::NestedJoin {
                table_with_joins, ..
            } => {
                self.register_aliases_in_table_with_joins(table_with_joins);
            }
            _ => {}
        }
    }

    pub fn resolve_table_alias(&self, alias: Option<&str>) -> Option<String> {
        self.analyzer.resolve_table_alias(self.ctx, alias)
    }

    pub(super) fn canonicalize_table_reference(&self, name: &str) -> super::TableResolution {
        self.analyzer.canonicalize_table_reference(name)
    }

    /// Extracts table identifiers from an expression (best-effort for unsupported constructs).
    ///
    /// Used for PIVOT, UNPIVOT, and table functions where full semantic analysis is not
    /// implemented. This may produce false positives (column references mistaken for tables)
    /// or false negatives (table references in unhandled expression types).
    fn extract_identifiers_from_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Identifier(ident) => {
                self.try_add_identifier_as_table(&[ident.clone()]);
            }
            Expr::CompoundIdentifier(idents) => {
                self.try_add_identifier_as_table(idents);
            }
            Expr::Function(func) => {
                if let ast::FunctionArguments::List(arg_list) = &func.args {
                    for arg in &arg_list.args {
                        if let ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(e)) = arg {
                            self.extract_identifiers_from_expr(e);
                        }
                    }
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.extract_identifiers_from_expr(left);
                self.extract_identifiers_from_expr(right);
            }
            Expr::UnaryOp { expr, .. } => {
                self.extract_identifiers_from_expr(expr);
            }
            Expr::Nested(e) => {
                self.extract_identifiers_from_expr(e);
            }
            Expr::InList { expr, list, .. } => {
                self.extract_identifiers_from_expr(expr);
                for e in list {
                    self.extract_identifiers_from_expr(e);
                }
            }
            Expr::Case {
                operand,
                conditions,
                results,
                else_result,
            } => {
                if let Some(op) = operand {
                    self.extract_identifiers_from_expr(op);
                }
                for cond in conditions {
                    self.extract_identifiers_from_expr(cond);
                }
                for result in results {
                    self.extract_identifiers_from_expr(result);
                }
                if let Some(else_r) = else_result {
                    self.extract_identifiers_from_expr(else_r);
                }
            }
            _ => {}
        }
    }

    fn try_add_identifier_as_table(&mut self, idents: &[Ident]) {
        if idents.is_empty() {
            return;
        }

        let name = idents
            .iter()
            .map(|i| i.value.as_str())
            .collect::<Vec<_>>()
            .join(".");

        let resolution = self.analyzer.canonicalize_table_reference(&name);
        if resolution.matched_schema {
            self.add_source_table(&name);
        }
    }
}

impl<'a, 'b> Visitor for LineageVisitor<'a, 'b> {
    fn visit_query(&mut self, query: &Query) {
        if let Some(with) = &query.with {
            let mut cte_ids: Vec<(String, Arc<str>)> = Vec::new();
            for cte in &with.cte_tables {
                let cte_name = cte.alias.name.to_string();
                let cte_id = self.ctx.add_node(Node {
                    id: generate_node_id("cte", &cte_name),
                    node_type: NodeType::Cte,
                    label: cte_name.clone().into(),
                    qualified_name: Some(cte_name.clone().into()),
                    expression: None,
                    span: None,
                    metadata: None,
                    resolution_source: None,
                    filters: Vec::new(),
                    join_type: None,
                    join_condition: None,
                    aggregation: None,
                });

                self.ctx
                    .cte_definitions
                    .insert(cte_name.clone(), cte_id.clone());
                self.analyzer.tracker.record_cte(&cte_name);
                cte_ids.push((cte_name, cte_id));
            }

            for (cte, (_, cte_id)) in with.cte_tables.iter().zip(cte_ids.iter()) {
                let mut cte_visitor =
                    LineageVisitor::new(self.analyzer, self.ctx, Some(cte_id.to_string()));
                cte_visitor.visit_query(&cte.query);
                let columns = std::mem::take(&mut self.ctx.output_columns);
                self.ctx
                    .cte_columns
                    .insert(cte.alias.name.to_string(), columns);
            }
        }
        self.visit_set_expr(&query.body);
    }

    fn visit_set_expr(&mut self, set_expr: &SetExpr) {
        match set_expr {
            SetExpr::Select(select) => self.visit_select(select),
            SetExpr::Query(query) => self.visit_query(query),
            SetExpr::SetOperation {
                op, left, right, ..
            } => {
                let op_name = match op {
                    SetOperator::Union => "UNION",
                    SetOperator::Intersect => "INTERSECT",
                    SetOperator::Except => "EXCEPT",
                };
                self.visit_set_expr(left);
                self.visit_set_expr(right);
                if self.target_node.is_some() {
                    self.ctx.last_operation = Some(op_name.to_string());
                }
            }
            SetExpr::Values(values) => self.visit_values(values),
            SetExpr::Insert(insert_stmt) => {
                let Statement::Insert(insert) = insert_stmt else {
                    return;
                };
                let target_name = insert.table_name.to_string();
                self.add_source_table(&target_name);
            }
            SetExpr::Table(tbl) => {
                let name = tbl
                    .table_name
                    .as_ref()
                    .map(|n| n.to_string())
                    .unwrap_or_default();
                if !name.is_empty() {
                    self.add_source_table(&name);
                }
            }
            _ => {}
        }
    }

    fn visit_select(&mut self, select: &Select) {
        self.ctx.push_scope();
        for table_with_joins in &select.from {
            self.visit_table_with_joins(table_with_joins);
        }
        if self.analyzer.column_lineage_enabled {
            let mut select_analyzer =
                SelectAnalyzer::new(self.analyzer, self.ctx, self.target_node.clone());
            select_analyzer.analyze(select);
        }
        self.ctx.pop_scope();
    }

    fn visit_table_with_joins(&mut self, table_with_joins: &TableWithJoins) {
        self.visit_table_factor(&table_with_joins.relation);
        for join in &table_with_joins.joins {
            let (join_type, join_condition) = Analyzer::convert_join_operator(&join.join_operator);
            self.ctx.current_join_info.join_type = join_type;
            self.ctx.current_join_info.join_condition = join_condition;
            self.ctx.last_operation = Analyzer::join_type_to_operation(join_type);
            self.visit_table_factor(&join.relation);
            self.ctx.current_join_info.join_type = None;
            self.ctx.current_join_info.join_condition = None;
        }
    }

    fn visit_table_factor(&mut self, table_factor: &TableFactor) {
        match table_factor {
            TableFactor::Table { name, alias, .. } => {
                let table_name = name.to_string();
                let canonical = self.add_source_table(&table_name);
                if let (Some(a), Some(canonical_name)) = (alias, canonical) {
                    self.ctx
                        .register_alias_in_scope(a.name.to_string(), canonical_name);
                }
            }
            TableFactor::Derived {
                subquery, alias, ..
            } => {
                self.visit_query(subquery);
                if let Some(a) = alias {
                    self.ctx
                        .register_subquery_alias_in_scope(a.name.to_string());
                }
            }
            TableFactor::NestedJoin {
                table_with_joins, ..
            } => {
                self.visit_table_with_joins(table_with_joins);
            }
            TableFactor::TableFunction { expr, alias, .. } => {
                self.extract_identifiers_from_expr(expr);
                if let Some(a) = alias {
                    self.ctx
                        .register_subquery_alias_in_scope(a.name.to_string());
                }
                self.analyzer.issues.push(
                    Issue::info(
                        issue_codes::UNSUPPORTED_SYNTAX,
                        "Table function lineage extracted with best-effort identifier matching",
                    )
                    .with_statement(self.ctx.statement_index),
                );
            }
            TableFactor::Pivot {
                table,
                aggregate_functions,
                value_column,
                value_source,
                alias,
                ..
            } => {
                self.visit_table_factor(table);
                for func in aggregate_functions {
                    self.extract_identifiers_from_expr(&func.expr);
                }
                for ident in value_column {
                    self.try_add_identifier_as_table(&[ident.clone()]);
                }
                match value_source {
                    ast::PivotValueSource::List(values) => {
                        for value in values {
                            self.extract_identifiers_from_expr(&value.expr);
                        }
                    }
                    ast::PivotValueSource::Any(_) => {}
                    ast::PivotValueSource::Subquery(q) => {
                        self.visit_query(q);
                    }
                }
                if let Some(a) = alias {
                    self.ctx
                        .register_subquery_alias_in_scope(a.name.to_string());
                }
                self.analyzer.issues.push(
                    Issue::warning(
                        issue_codes::UNSUPPORTED_SYNTAX,
                        "PIVOT lineage extracted with best-effort identifier matching",
                    )
                    .with_statement(self.ctx.statement_index),
                );
            }
            TableFactor::Unpivot {
                table,
                columns,
                alias,
                ..
            } => {
                self.visit_table_factor(table);
                for col in columns {
                    self.try_add_identifier_as_table(&[col.clone()]);
                }
                if let Some(a) = alias {
                    self.ctx
                        .register_subquery_alias_in_scope(a.name.to_string());
                }
                self.analyzer.issues.push(
                    Issue::warning(
                        issue_codes::UNSUPPORTED_SYNTAX,
                        "UNPIVOT lineage extracted with best-effort identifier matching",
                    )
                    .with_statement(self.ctx.statement_index),
                );
            }
            _ => {}
        }
    }

    fn visit_values(&mut self, values: &Values) {
        let mut expr_analyzer = ExpressionAnalyzer::new(self.analyzer, self.ctx);
        for row in &values.rows {
            for expr in row {
                expr_analyzer.analyze(expr);
            }
        }
    }
}
