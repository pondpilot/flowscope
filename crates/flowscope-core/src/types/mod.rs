//! Types for SQL lineage analysis API.
//!
//! This module defines the request and response types for the FlowScope analysis API.
//! The API accepts SQL queries and returns detailed lineage information including
//! tables, columns, and their relationships.

mod common;
mod completion;
mod legacy;
mod request;
mod response;
pub mod serde_utils;

// Re-export all public types
pub use common::{
    issue_codes, CaseSensitivity, Issue, IssueAutofix, IssueAutofixApplicability, IssueCount,
    IssuePatchEdit, LintConfidence, LintEngine, LintFallbackSource, Severity, Span, Summary,
};
pub use completion::{
    CompletionClause, CompletionColumn, CompletionContext, CompletionItem, CompletionItemCategory,
    CompletionItemKind, CompletionItemsResult, CompletionKeywordHints, CompletionKeywordSet,
    CompletionTable, CompletionToken, CompletionTokenKind,
};

// Re-export internal AST types for crate-internal use only
pub(crate) use completion::{
    AstColumnInfo, AstContext, AstTableInfo, CteInfo, ParseStrategy, SubqueryInfo,
};
pub use legacy::LineageResult;
pub use request::{
    AnalysisOptions, AnalyzeRequest, ColumnSchema, CompletionRequest, Dialect, FileSource,
    ForeignKeyRef, SchemaMetadata, SchemaNamespaceHint, SchemaTable, StatementSplitRequest,
};
#[cfg(feature = "templating")]
pub use request::{TemplateConfig, TemplateError, TemplateMode};
pub use response::{
    AggregationInfo, AnalyzeResult, CanonicalName, ConstraintType, Edge, EdgeType,
    FilterClauseType, FilterPredicate, JoinType, Node, NodeType, ResolutionSource,
    ResolvedColumnSchema, ResolvedSchemaMetadata, ResolvedSchemaTable, SchemaOrigin, StatementMeta,
    StatementSplitResult, TableConstraintInfo, STATEMENT_AGGREGATIONS_METADATA_KEY,
    STATEMENT_FILTERS_METADATA_KEY,
};
// Crate-internal intermediate used during analysis; not exposed to API consumers.
pub(crate) use response::StatementLineage;
