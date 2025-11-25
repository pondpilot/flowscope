use crate::types::*;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
#[cfg(feature = "tracing")]
use tracing::{info, info_span};

mod complexity;
mod context;
mod ddl;
mod diagnostics;
mod expression;
mod functions;
mod global;
pub mod helpers;
mod input;
mod query;
mod resolution;
mod select;
mod statements;

use input::{collect_statements, StatementInput};

/// Main entry point for SQL analysis
pub fn analyze(request: &AnalyzeRequest) -> AnalyzeResult {
    #[cfg(feature = "tracing")]
    let _span =
        info_span!("analyze_request", statement_count = %request.sql.matches(';').count() + 1)
            .entered();
    let mut analyzer = Analyzer::new(request);
    analyzer.analyze()
}

/// Internal analyzer state
pub(super) struct Analyzer<'a> {
    pub(super) request: &'a AnalyzeRequest,
    pub(super) issues: Vec<Issue>,
    pub(super) statement_lineages: Vec<StatementLineage>,
    /// Track which tables are produced by which statement (for cross-statement linking)
    pub(super) produced_tables: HashMap<String, usize>,
    /// Track which tables are consumed by which statements
    pub(super) consumed_tables: HashMap<String, Vec<usize>>,
    /// All discovered tables across statements (for global lineage)
    pub(super) all_tables: HashSet<String>,
    /// All discovered CTEs
    pub(super) all_ctes: HashSet<String>,
    /// Known tables from schema metadata (for validation)
    pub(super) known_tables: HashSet<String>,
    /// Tables from imported (user-provided) schema that should not be overwritten
    pub(super) imported_tables: HashSet<String>,
    /// Schema lookup: table canonical name -> table schema entry with metadata
    pub(super) schema_tables: HashMap<String, SchemaTableEntry>,
    /// Whether column lineage is enabled
    pub(super) column_lineage_enabled: bool,
    /// Default catalog for unqualified identifiers
    pub(super) default_catalog: Option<String>,
    /// Default schema for unqualified identifiers
    pub(super) default_schema: Option<String>,
    /// Ordered search path entries
    pub(super) search_path: Vec<SearchPathEntry>,
}

#[derive(Debug, Clone)]
pub(super) struct SearchPathEntry {
    catalog: Option<String>,
    schema: String,
}

#[derive(Debug, Clone)]
struct TableResolution {
    canonical: String,
    matched_schema: bool,
}

/// Schema table entry with origin metadata for tracking imported vs implied schema
#[derive(Debug, Clone)]
pub(super) struct SchemaTableEntry {
    pub(super) table: SchemaTable,
    pub(super) origin: SchemaOrigin,
    pub(super) source_statement_idx: Option<usize>,
    pub(super) updated_at: DateTime<Utc>,
    pub(super) temporary: bool,
}

impl<'a> Analyzer<'a> {
    fn new(request: &'a AnalyzeRequest) -> Self {
        // Check if column lineage is enabled (default: true)
        let column_lineage_enabled = request
            .options
            .as_ref()
            .and_then(|o| o.enable_column_lineage)
            .unwrap_or(true);

        let mut analyzer = Self {
            request,
            issues: Vec::new(),
            statement_lineages: Vec::new(),
            produced_tables: HashMap::new(),
            consumed_tables: HashMap::new(),
            all_tables: HashSet::new(),
            all_ctes: HashSet::new(),
            known_tables: HashSet::new(),
            imported_tables: HashSet::new(),
            schema_tables: HashMap::new(),
            column_lineage_enabled,
            default_catalog: None,
            default_schema: None,
            search_path: Vec::new(),
        };

        analyzer.initialize_schema_metadata();

        analyzer
    }

    /// Check if implied schema capture is allowed (default: true)
    fn allow_implied(&self) -> bool {
        self.request
            .schema
            .as_ref()
            .map(|s| s.allow_implied)
            .unwrap_or(true)
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
