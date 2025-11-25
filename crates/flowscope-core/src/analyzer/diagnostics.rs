use super::context::StatementContext;
use super::expression::ExpressionAnalyzer;
use super::Analyzer;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Expr;

impl<'a> Analyzer<'a> {
    pub(super) fn validate_column(
        &mut self,
        ctx: &StatementContext,
        table_canonical: &str,
        column: &str,
    ) {
        if let Some(schema_entry) = self.schema_tables.get(table_canonical) {
            let normalized_col = self.normalize_identifier(column);
            let column_exists = schema_entry
                .table
                .columns
                .iter()
                .any(|c| self.normalize_identifier(&c.name) == normalized_col);

            if !column_exists && !schema_entry.table.columns.is_empty() {
                self.issues.push(
                    Issue::warning(
                        issue_codes::UNKNOWN_COLUMN,
                        format!("Column '{column}' not found in table '{table_canonical}'"),
                    )
                    .with_statement(ctx.statement_index),
                );
            }
        }
    }

    pub(super) fn extract_column_refs_for_validation(
        &mut self,
        ctx: &StatementContext,
        expr: &Expr,
    ) {
        let refs = ExpressionAnalyzer::extract_column_refs(expr);
        for col_ref in refs {
            if let Some(table) = col_ref.table.as_deref() {
                let resolved = self.resolve_table_alias(ctx, Some(table));
                if let Some(table_canonical) = resolved {
                    self.validate_column(ctx, &table_canonical, &col_ref.column);
                }
            }
        }
    }
}
