//! Visitor pattern for AST traversal.

use sqlparser::ast::{
    Cte, Expr, Join, Query, Select, SetExpr, Statement, TableFactor, TableWithJoins, Values,
};

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
            // Add other statements as needed
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
        if let Some(selection) = &select.selection {
            self.visit_expr(selection);
        }
        // Visit projection expressions if needed
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
        // Visit join condition
    }

    fn visit_values(&mut self, values: &Values) {
        for row in &values.rows {
            for expr in row {
                self.visit_expr(expr);
            }
        }
    }

    fn visit_expr(&mut self, _expr: &Expr) {
        // Default: do nothing or traverse sub-expressions
    }
}
