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
pub use common::{issue_codes, CaseSensitivity, Issue, IssueCount, Severity, Span, Summary};
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
pub use response::{
    AggregationInfo, AnalyzeResult, CanonicalName, ConstraintType, Edge, EdgeType,
    FilterClauseType, FilterPredicate, GlobalEdge, GlobalLineage, GlobalNode, JoinType, Node,
    NodeType, ResolutionSource, ResolvedColumnSchema, ResolvedSchemaMetadata, ResolvedSchemaTable,
    SchemaOrigin, StatementLineage, StatementRef, StatementSplitResult, TableConstraintInfo,
};
