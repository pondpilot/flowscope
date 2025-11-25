//! Types for SQL lineage analysis API.
//!
//! This module defines the request and response types for the FlowScope analysis API.
//! The API accepts SQL queries and returns detailed lineage information including
//! tables, columns, and their relationships.

mod common;
mod legacy;
mod request;
mod response;

// Re-export all public types
pub use common::{issue_codes, CaseSensitivity, Issue, IssueCount, Severity, Span, Summary};
pub use legacy::LineageResult;
pub use request::{
    AnalysisOptions, AnalyzeRequest, ColumnSchema, Dialect, FileSource, SchemaMetadata,
    SchemaNamespaceHint, SchemaTable,
};
pub use response::{
    AnalyzeResult, CanonicalName, Edge, EdgeType, FilterClauseType, FilterPredicate, GlobalEdge,
    GlobalLineage, GlobalNode, JoinType, Node, NodeType, ResolutionSource, ResolvedColumnSchema,
    ResolvedSchemaMetadata, ResolvedSchemaTable, SchemaOrigin, StatementLineage, StatementRef,
};
