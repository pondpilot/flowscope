use crate::types::*;
use std::sync::Arc;
#[cfg(feature = "tracing")]
use tracing::info_span;

mod complexity;
mod context;
pub(crate) mod cross_statement;
mod ddl;
mod diagnostics;
mod expression;
mod functions;
mod global;
pub mod helpers;
mod input;
mod query;
pub(crate) mod schema_registry;
mod statements;
pub mod visitor;

use cross_statement::CrossStatementTracker;
use input::{collect_statements, StatementInput};
use schema_registry::SchemaRegistry;

// Re-export for use in other analyzer modules
pub(crate) use schema_registry::TableResolution;

/// Main entry point for SQL analysis
pub fn analyze(request: &AnalyzeRequest) -> AnalyzeResult {
    #[cfg(feature = "tracing")]
    let _span =
        info_span!("analyze_request", statement_count = %request.sql.matches(';').count() + 1)
            .entered();
    let mut analyzer = Analyzer::new(request);
    analyzer.analyze()
}

/// Internal analyzer state.
///
/// The analyzer is organized into focused components:
/// - `schema`: Manages schema metadata, resolution, and normalization
/// - `tracker`: Tracks cross-statement dependencies and lineage
/// - `issues`: Collects warnings and errors during analysis
/// - `statement_lineages`: Stores per-statement analysis results
pub(crate) struct Analyzer<'a> {
    pub(crate) request: &'a AnalyzeRequest,
    pub(crate) issues: Vec<Issue>,
    pub(crate) statement_lineages: Vec<StatementLineage>,
    /// Schema registry for table/column resolution.
    pub(crate) schema: SchemaRegistry,
    /// Cross-statement dependency tracker.
    pub(crate) tracker: CrossStatementTracker,
    /// Whether column lineage is enabled.
    pub(crate) column_lineage_enabled: bool,
}

impl<'a> Analyzer<'a> {
    fn new(request: &'a AnalyzeRequest) -> Self {
        // Check if column lineage is enabled (default: true)
        let column_lineage_enabled = request
            .options
            .as_ref()
            .and_then(|o| o.enable_column_lineage)
            .unwrap_or(true);

        let (schema, init_issues) = SchemaRegistry::new(request.schema.as_ref(), request.dialect);

        Self {
            request,
            issues: init_issues,
            statement_lineages: Vec::new(),
            schema,
            tracker: CrossStatementTracker::new(),
            column_lineage_enabled,
        }
    }

    /// Returns the correct node ID and type for a relation (view vs table).
    pub(crate) fn relation_identity(&self, canonical: &str) -> (Arc<str>, NodeType) {
        self.tracker.relation_identity(canonical)
    }

    /// Returns the node ID for a relation.
    pub(crate) fn relation_node_id(&self, canonical: &str) -> Arc<str> {
        self.tracker.relation_node_id(canonical)
    }

    /// Check if implied schema capture is allowed (default: true).
    pub(crate) fn allow_implied(&self) -> bool {
        self.schema.allow_implied()
    }

    /// Canonicalizes a table reference using schema resolution.
    pub(crate) fn canonicalize_table_reference(&self, name: &str) -> TableResolution {
        self.schema.canonicalize_table_reference(name)
    }

    /// Normalizes an identifier according to dialect case sensitivity.
    pub(crate) fn normalize_identifier(&self, name: &str) -> String {
        self.schema.normalize_identifier(name)
    }

    /// Normalizes a qualified table name.
    pub(crate) fn normalize_table_name(&self, name: &str) -> String {
        self.schema.normalize_table_name(name)
    }

    fn analyze(&mut self) -> AnalyzeResult {
        let (all_statements, mut preflight_issues) = collect_statements(self.request);
        self.issues.append(&mut preflight_issues);

        if all_statements.is_empty() {
            return self.build_result();
        }

        // Analyze all statements
        for (
            index,
            StatementInput {
                statement,
                source_name,
            },
        ) in all_statements.into_iter().enumerate()
        {
            #[cfg(feature = "tracing")]
            let _stmt_span = info_span!(
                "analyze_statement",
                index,
                source = source_name.as_deref().unwrap_or("inline"),
                stmt_type = ?statement
            )
            .entered();
            match self.analyze_statement(index, &statement, source_name) {
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
}

#[cfg(test)]
mod tests;
