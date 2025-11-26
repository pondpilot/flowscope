pub mod analyzer;
pub mod error;
pub mod generated;
pub mod lineage;
pub mod parser;
#[cfg(test)]
pub mod test_utils;
pub mod types;

// Internal helper exposure for tests only
#[cfg(test)]
pub mod analyzer_helpers {
    pub use crate::analyzer::helpers;
}

// Re-export main types and functions
pub use analyzer::analyze;
pub use error::ParseError;
pub use lineage::extract_tables;
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
    Dialect,
    Edge,
    EdgeType,
    FileSource,
    FilterClauseType,
    FilterPredicate,
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
};
