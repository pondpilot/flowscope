use super::context::StatementContext;
use super::Analyzer;

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
}
