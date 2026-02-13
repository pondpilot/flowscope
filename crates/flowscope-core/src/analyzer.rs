use crate::linter::document::{LintDocument, LintStatement};
use crate::linter::Linter;
use crate::types::*;
use sqlparser::ast::Statement;
use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;
#[cfg(feature = "tracing")]
use tracing::info_span;

/// Maximum SQL input size (10MB) to prevent memory exhaustion.
/// This matches the TypeScript validation limit.
const MAX_SQL_LENGTH: usize = 10 * 1024 * 1024;

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
mod select_analyzer;
mod statements;
mod transform;
pub mod visitor;

use cross_statement::CrossStatementTracker;
use helpers::{build_column_schemas_with_constraints, find_identifier_span};
use input::{collect_statements, StatementInput};
use schema_registry::SchemaRegistry;

// Re-export for use in other analyzer modules
pub(crate) use schema_registry::TableResolution;

/// Main entry point for SQL analysis
#[must_use]
pub fn analyze(request: &AnalyzeRequest) -> AnalyzeResult {
    #[cfg(feature = "tracing")]
    let _span =
        info_span!("analyze_request", statement_count = %request.sql.matches(';').count() + 1)
            .entered();
    let mut analyzer = Analyzer::new(request);
    analyzer.analyze()
}

/// Split SQL into statement spans.
#[must_use]
pub fn split_statements(request: &StatementSplitRequest) -> StatementSplitResult {
    // Validate input size to prevent memory exhaustion
    if request.sql.len() > MAX_SQL_LENGTH {
        return StatementSplitResult::from_error(format!(
            "SQL exceeds maximum length of {} bytes ({} bytes provided)",
            MAX_SQL_LENGTH,
            request.sql.len()
        ));
    }

    StatementSplitResult {
        statements: input::split_statement_spans_with_dialect(&request.sql, request.dialect),
        error: None,
    }
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
    /// Source slice for the currently analyzed statement (for span lookups).
    current_statement_source: Option<StatementSourceSlice<'a>>,
    /// Statements that already emitted a recursion-depth warning.
    depth_limit_statements: HashSet<usize>,
    /// SQL linter (None if linting is disabled).
    linter: Option<Linter>,
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

        // Initialize linter only when explicitly requested via options.lint
        let linter = request
            .options
            .as_ref()
            .and_then(|o| o.lint.clone())
            .filter(|c| c.enabled)
            .map(Linter::new);

        Self {
            request,
            issues: init_issues,
            statement_lineages: Vec::new(),
            schema,
            tracker: CrossStatementTracker::new(),
            column_lineage_enabled,
            current_statement_source: None,
            depth_limit_statements: HashSet::new(),
            linter,
        }
    }

    /// Finds the span of an identifier in the SQL text.
    ///
    /// This is used to attach source locations to issues for better error reporting.
    pub(crate) fn find_span(&self, identifier: &str) -> Option<Span> {
        if let Some(source) = &self.current_statement_source {
            let statement_sql = &source.sql[source.range.clone()];
            return find_identifier_span(statement_sql, identifier, 0).map(|span| {
                Span::new(
                    source.range.start + span.start,
                    source.range.start + span.end,
                )
            });
        }

        find_identifier_span(&self.request.sql, identifier, 0)
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

    /// Emits a warning when expression traversal exceeds the recursion guard.
    pub(crate) fn emit_depth_limit_warning(&mut self, statement_index: usize) {
        if self.depth_limit_statements.insert(statement_index) {
            self.issues.push(
                Issue::warning(
                    issue_codes::APPROXIMATE_LINEAGE,
                    format!(
                        "Expression recursion depth exceeded (>{}). Lineage may be incomplete.",
                        expression::MAX_RECURSION_DEPTH
                    ),
                )
                .with_statement(statement_index),
            );
        }
    }

    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self), fields(dialect = ?self.request.dialect, stmt_count)))]
    fn analyze(&mut self) -> AnalyzeResult {
        let (all_statements, mut preflight_issues) = collect_statements(self.request);
        self.issues.append(&mut preflight_issues);

        #[cfg(feature = "tracing")]
        tracing::Span::current().record("stmt_count", all_statements.len());

        self.precollect_ddl(&all_statements);

        if all_statements.is_empty() {
            self.run_lint_documents_without_statements();
            return self.build_result();
        }

        self.run_lint_documents(&all_statements);

        // Analyze all statements
        for (
            index,
            StatementInput {
                statement,
                source_name,
                source_sql,
                source_range,
                templating_applied,
                ..
            },
        ) in all_statements.into_iter().enumerate()
        {
            #[cfg(feature = "tracing")]
            let _stmt_span = info_span!(
                "analyze_statement",
                index,
                source = source_name.as_deref().map_or("inline", String::as_str),
                stmt_type = ?statement
            )
            .entered();

            // Extract resolved SQL when templating was applied
            let resolved_sql = if templating_applied {
                Some(source_sql[source_range.clone()].to_string())
            } else {
                None
            };
            self.current_statement_source = Some(StatementSourceSlice {
                sql: source_sql,
                range: source_range.clone(),
            });

            let source_name_owned = source_name.as_deref().map(String::from);
            let result = self.analyze_statement(
                index,
                &statement,
                source_name_owned,
                source_range,
                resolved_sql,
            );
            self.current_statement_source = None;

            match result {
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

struct StatementSourceSlice<'a> {
    sql: Cow<'a, str>,
    range: Range<usize>,
}

impl<'a> Analyzer<'a> {
    fn run_lint_documents(&mut self, statements: &[StatementInput<'a>]) {
        let Some(linter) = self.linter.as_ref() else {
            return;
        };

        let mut start = 0usize;
        while start < statements.len() {
            let source_name_key = statements[start]
                .source_name
                .as_deref()
                .map(|name| name.as_str());
            let source_sql_key = statements[start].source_sql.as_ref();
            let source_untemplated_sql_key = statements[start].source_sql_untemplated.as_deref();

            let mut end = start + 1;
            while end < statements.len()
                && statements[end]
                    .source_name
                    .as_deref()
                    .map(|name| name.as_str())
                    == source_name_key
                && statements[end].source_sql.as_ref() == source_sql_key
                && statements[end].source_sql_untemplated.as_deref() == source_untemplated_sql_key
            {
                end += 1;
            }

            let mut lint_statements = Vec::with_capacity(end - start);
            let mut source_statement_ranges = Vec::with_capacity(end - start);
            for (offset, statement_input) in statements[start..end].iter().enumerate() {
                lint_statements.push(LintStatement {
                    statement: &statement_input.statement,
                    statement_index: offset,
                    statement_range: statement_input.source_range.clone(),
                });
                source_statement_ranges.push(statement_input.source_range_untemplated.clone());
            }

            let parser_fallback_used = statements[start..end]
                .iter()
                .any(|statement_input| statement_input.parser_fallback_used);
            let document = LintDocument::new_with_parser_fallback_and_source(
                source_sql_key,
                source_untemplated_sql_key,
                self.request.dialect,
                lint_statements,
                parser_fallback_used,
                Some(source_statement_ranges),
            );
            self.issues.extend(linter.check_document(&document));

            start = end;
        }
    }

    fn run_lint_documents_without_statements(&mut self) {
        let Some(linter) = self.linter.as_ref() else {
            return;
        };

        if let Some(files) = &self.request.files {
            if files.is_empty() {
                return;
            }
            for file in files {
                let document = LintDocument::new(&file.content, self.request.dialect, Vec::new());
                self.issues.extend(linter.check_document(&document));
            }
            return;
        }

        if !self.request.sql.is_empty() {
            let document = LintDocument::new(&self.request.sql, self.request.dialect, Vec::new());
            self.issues.extend(linter.check_document(&document));
        }
    }

    /// Pre-registers CREATE TABLE/VIEW targets so earlier statements can resolve them.
    fn precollect_ddl(&mut self, statements: &[StatementInput]) {
        for (index, stmt_input) in statements.iter().enumerate() {
            match &stmt_input.statement {
                Statement::CreateTable(create) => {
                    self.precollect_create_table(create, index);
                }
                Statement::CreateView { name, .. } => {
                    self.precollect_create_view(name);
                }
                _ => {}
            }
        }
    }

    /// Handles CREATE TABLE statements during DDL pre-collection.
    fn precollect_create_table(
        &mut self,
        create: &sqlparser::ast::CreateTable,
        statement_index: usize,
    ) {
        let canonical = self.normalize_table_name(&create.name.to_string());

        if create.query.is_none() {
            let (column_schemas, table_constraints) =
                build_column_schemas_with_constraints(&create.columns, &create.constraints);

            self.schema.seed_implied_schema_with_constraints(
                &canonical,
                column_schemas,
                table_constraints,
                create.temporary,
                statement_index,
            );
        } else {
            // This is a CTAS (CREATE TABLE ... AS SELECT ...).
            // We mark the table as known to prevent UNRESOLVED_REFERENCE
            // errors, but we don't have column schema yet.
            self.schema.mark_table_known(&canonical);
        }
    }

    /// Handles CREATE VIEW statements during DDL pre-collection.
    fn precollect_create_view(&mut self, name: &sqlparser::ast::ObjectName) {
        let canonical = self.normalize_table_name(&name.to_string());
        self.schema.mark_table_known(&canonical);
    }
}

#[cfg(test)]
mod tests;
