use super::context::StatementContext;
use super::expression::ExpressionAnalyzer;
use super::Analyzer;
use sqlparser::ast::Expr;

impl<'a> Analyzer<'a> {
    /// Validates that a column exists in a table's schema.
    ///
    /// Adds a warning issue if the column is not found.
    ///
    /// # Parameters
    ///
    /// - `ctx`: The statement context
    /// - `canonical`: The canonical table name
    /// - `column`: The column name to validate
    pub(super) fn validate_column(
        &mut self,
        ctx: &StatementContext,
        canonical: &str,
        column: &str,
    ) {
        if let Some(issue) = self
            .schema
            .validate_column(canonical, column, ctx.statement_index)
        {
            self.issues.push(issue);
        }
    }

    /// Extracts column references from an expression and validates each one.
    pub(super) fn extract_column_refs_for_validation(
        &mut self,
        ctx: &StatementContext,
        expr: &Expr,
    ) {
        let refs = ExpressionAnalyzer::extract_column_refs(expr);
        for col_ref in refs {
            if let Some(table) = col_ref.table.as_deref() {
                if let Some(canonical) = self.resolve_table_alias(ctx, Some(table)) {
                    self.validate_column(ctx, &canonical, &col_ref.column);
                }
            }
        }
    }
}
