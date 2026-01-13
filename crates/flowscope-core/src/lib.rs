pub mod analyzer;
pub mod error;
pub mod extractors;
pub mod generated;
pub mod parser;
pub mod types;

// Re-export main types and functions
pub use analyzer::analyze;
pub use error::ParseError;
pub use extractors::extract_tables;
pub use parser::{parse_sql, parse_sql_with_dialect};

// Re-export types explicitly
pub use types::{
    // Issue codes
    issue_codes,
    // Request types
    AggregationInfo,
    AnalysisOptions,
    AnalyzeRequest,
    // Response types
    AnalyzeResult,
    CanonicalName,
    CaseSensitivity,
    ColumnSchema,
    ConstraintType,
    Dialect,
    Edge,
    EdgeType,
    FileSource,
    FilterClauseType,
    FilterPredicate,
    ForeignKeyRef,
    GlobalEdge,
    GlobalLineage,
    GlobalNode,
    Issue,
    IssueCount,
    JoinType,
    // Legacy
    LineageResult,
    Node,
    NodeType,
    ResolutionSource,
    ResolvedColumnSchema,
    ResolvedSchemaMetadata,
    ResolvedSchemaTable,
    SchemaMetadata,
    SchemaNamespaceHint,
    SchemaOrigin,
    SchemaTable,
    Severity,
    Span,
    StatementLineage,
    StatementRef,
    Summary,
    TableConstraintInfo,
};

// Test utilities and helper exposure (must be at end of file)
#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
pub mod analyzer_helpers {
    pub use crate::analyzer::helpers;
}
