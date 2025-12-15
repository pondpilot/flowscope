use crate::types::*;
use sqlparser::ast::Statement;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
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
mod select_analyzer;
mod statements;
pub mod visitor;

use context::StatementContext;
use cross_statement::CrossStatementTracker;
use helpers::{build_column_schemas_with_constraints, find_identifier_span};
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
    /// Column-level tag hints provided externally (table.column -> tags)
    tag_hints: HashMap<String, Vec<ColumnTag>>,
    /// Table-level default tags (table -> tags) applied to all columns
    table_tag_defaults: HashMap<String, Vec<ColumnTag>>,
    /// Source slice for the currently analyzed statement (for span lookups).
    current_statement_source: Option<StatementSourceSlice<'a>>,
    /// Statements that already emitted a recursion-depth warning.
    depth_limit_statements: HashSet<usize>,
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

        let mut analyzer = Self {
            request,
            issues: init_issues,
            statement_lineages: Vec::new(),
            schema,
            tracker: CrossStatementTracker::new(),
            column_lineage_enabled,
            tag_hints: HashMap::new(),
            table_tag_defaults: HashMap::new(),
            current_statement_source: None,
            depth_limit_statements: HashSet::new(),
        };

        analyzer.initialize_tag_hints();
        analyzer
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
            return self.build_result();
        }

        // Analyze all statements
        for (
            index,
            StatementInput {
                statement,
                source_name,
                source_sql,
                source_range,
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
            self.current_statement_source = Some(StatementSourceSlice {
                sql: source_sql,
                range: source_range,
            });

            let source_name_owned = source_name.as_deref().map(String::from);
            let result = self.analyze_statement(index, &statement, source_name_owned);
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
    sql: &'a str,
    range: Range<usize>,
}

impl<'a> Analyzer<'a> {
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

    fn initialize_tag_hints(&mut self) {
        if let Some(hints) = &self.request.tag_hints {
            for hint in hints {
                if hint.tags.is_empty() {
                    continue;
                }
                let canonical_table = self.normalize_table_name(&hint.table);
                if hint.column.trim() == "*" {
                    self.table_tag_defaults
                        .entry(canonical_table)
                        .or_default()
                        .extend(hint.tags.clone());
                    continue;
                }
                let normalized_column = self.normalize_identifier(&hint.column);
                self.tag_hints
                    .entry(Self::tag_key(&canonical_table, &normalized_column))
                    .or_default()
                    .extend(hint.tags.clone());
            }
        }
    }

    fn collect_base_tags_for_column(&self, canonical: &str, column_name: &str) -> Vec<ColumnTag> {
        let normalized_column = self.normalize_identifier(column_name);
        let mut tags: Vec<ColumnTag> = Vec::new();

        if let Some(entry) = self.schema.get(canonical) {
            if let Some(schema_column) = entry
                .table
                .columns
                .iter()
                .find(|col| self.normalize_identifier(&col.name) == normalized_column)
            {
                if let Some(classifications) = &schema_column.classifications {
                    tags.extend(classifications.clone());
                }
            }
        }

        if let Some(defaults) = self.table_tag_defaults.get(canonical) {
            tags.extend(defaults.clone());
        }

        if let Some(hints) = self
            .tag_hints
            .get(&Self::tag_key(canonical, &normalized_column))
        {
            tags.extend(hints.clone());
        }

        Self::dedupe_tags(&mut tags);
        tags
    }

    fn inherit_tags(&self, tags: &[ColumnTag], source_id: &Arc<str>) -> Vec<ColumnTag> {
        tags.iter()
            .map(|tag| {
                let mut inherited = tag.clone();
                inherited.inherited = Some(true);
                inherited.from_column_id = Some(source_id.to_string());
                inherited
            })
            .collect()
    }

    fn dedupe_tags(tags: &mut Vec<ColumnTag>) {
        let mut seen: HashSet<(String, String, bool)> = HashSet::new();
        tags.retain(|tag| {
            let key = (
                tag.name.to_lowercase(),
                tag.from_column_id.clone().unwrap_or_default(),
                tag.inherited.unwrap_or(false),
            );
            if seen.contains(&key) {
                false
            } else {
                seen.insert(key);
                true
            }
        });
    }

    fn find_canonical_by_node<'b>(
        &self,
        ctx: &'b StatementContext,
        node_id: &str,
    ) -> Option<&'b String> {
        ctx.table_node_ids.iter().find_map(|(canonical, id)| {
            if id.as_ref() == node_id {
                Some(canonical)
            } else {
                None
            }
        })
    }

    fn tag_key(table: &str, column: &str) -> String {
        format!("{table}.{column}")
    }
}

#[cfg(test)]
mod tests;
