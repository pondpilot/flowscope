pub mod analyzer;
pub mod error;
pub mod lineage;
pub mod parser;
#[cfg(test)]
pub mod test_utils;
pub mod types;

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
    GlobalEdge,
    GlobalLineage,
    GlobalNode,
    Issue,
    IssueCount,
    // Legacy
    LineageResult,
    Node,
    NodeType,
    SchemaMetadata,
    SchemaNamespaceHint,
    SchemaTable,
    Severity,
    Span,
    StatementLineage,
    StatementRef,
    Summary,
};
