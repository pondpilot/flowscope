use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CompletionClause {
    Select,
    From,
    Where,
    Join,
    On,
    GroupBy,
    Having,
    OrderBy,
    Limit,
    Qualify,
    Window,
    Insert,
    Update,
    Delete,
    With,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CompletionTokenKind {
    Keyword,
    Identifier,
    /// Double-quoted identifier like "My Table"
    QuotedIdentifier,
    Literal,
    Operator,
    Symbol,
    /// SQL comment (line or block)
    Comment,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompletionToken {
    pub value: String,
    pub kind: CompletionTokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompletionTable {
    pub name: String,
    pub canonical: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub matched_schema: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompletionColumn {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_table: Option<String>,
    pub is_ambiguous: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompletionKeywordSet {
    pub keywords: Vec<String>,
    pub operators: Vec<String>,
    pub aggregates: Vec<String>,
    pub snippets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompletionKeywordHints {
    pub global: CompletionKeywordSet,
    pub clause: CompletionKeywordSet,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompletionContext {
    pub statement_index: usize,
    pub statement_span: Span,
    pub clause: CompletionClause,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<CompletionToken>,
    pub tables_in_scope: Vec<CompletionTable>,
    pub columns_in_scope: Vec<CompletionColumn>,
    pub keyword_hints: CompletionKeywordHints,
    /// Error message if the request could not be processed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CompletionItemKind {
    Keyword,
    Operator,
    Function,
    Snippet,
    Table,
    Column,
    SchemaTable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CompletionItemCategory {
    Keyword,
    Operator,
    Aggregate,
    Snippet,
    Table,
    Column,
    SchemaTable,
    Function,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    pub insert_text: String,
    pub kind: CompletionItemKind,
    pub category: CompletionItemCategory,
    pub score: i32,
    pub clause_specific: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItemsResult {
    pub clause: CompletionClause,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<CompletionToken>,
    pub should_show: bool,
    pub items: Vec<CompletionItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CompletionItemsResult {
    pub fn empty() -> Self {
        Self {
            clause: CompletionClause::Unknown,
            token: None,
            should_show: false,
            items: Vec::new(),
            error: None,
        }
    }

    pub fn from_error(message: impl Into<String>) -> Self {
        Self {
            clause: CompletionClause::Unknown,
            token: None,
            should_show: false,
            items: Vec::new(),
            error: Some(message.into()),
        }
    }
}

impl CompletionContext {
    pub fn empty() -> Self {
        Self {
            statement_index: 0,
            statement_span: Span::new(0, 0),
            clause: CompletionClause::Unknown,
            token: None,
            tables_in_scope: Vec::new(),
            columns_in_scope: Vec::new(),
            keyword_hints: CompletionKeywordHints::default(),
            error: None,
        }
    }

    pub fn from_error(message: impl Into<String>) -> Self {
        Self {
            statement_index: 0,
            statement_span: Span::new(0, 0),
            clause: CompletionClause::Unknown,
            token: None,
            tables_in_scope: Vec::new(),
            columns_in_scope: Vec::new(),
            keyword_hints: CompletionKeywordHints::default(),
            error: Some(message.into()),
        }
    }
}

// =============================================================================
// AST-based completion types (internal, not exposed in public API)
// =============================================================================

use std::collections::HashMap;

/// Parse strategy used to obtain AST context
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ParseStrategy {
    /// No AST available (token-only fallback)
    #[default]
    None,
    /// Full SQL parsed successfully
    FullParse,
    /// SQL truncated at cursor position
    Truncated,
    /// Only complete statements before cursor
    CompleteStatementsOnly,
    /// Minimal fixes applied to make SQL parseable
    WithFixes,
}

/// Column information extracted from AST (internal use)
#[derive(Debug, Clone, Default)]
pub(crate) struct AstColumnInfo {
    pub name: String,
    pub data_type: Option<String>,
}

/// Marker for a resolved table reference in the AST.
/// The table name is stored as the key in the AstContext.table_aliases HashMap.
#[derive(Debug, Clone, Default)]
pub(crate) struct AstTableInfo;

/// Information about a CTE definition
#[derive(Debug, Clone, Default)]
pub(crate) struct CteInfo {
    /// CTE name (alias)
    pub name: String,
    /// Explicitly declared column names: WITH cte(a, b) AS ...
    pub declared_columns: Vec<String>,
    /// Projected columns inferred from CTE body SELECT list
    pub projected_columns: Vec<AstColumnInfo>,
}

/// Information about a subquery alias (derived table)
#[derive(Debug, Clone, Default)]
pub(crate) struct SubqueryInfo {
    /// Projected columns from the subquery
    pub projected_columns: Vec<AstColumnInfo>,
}

/// AST-extracted context for completion enrichment
#[derive(Debug, Clone, Default)]
pub(crate) struct AstContext {
    /// Table aliases: alias_name → resolved AstTableInfo
    pub table_aliases: HashMap<String, AstTableInfo>,

    /// CTE definitions: cte_name → CteInfo
    pub cte_definitions: HashMap<String, CteInfo>,

    /// Subquery aliases: alias_name → SubqueryInfo
    pub subquery_aliases: HashMap<String, SubqueryInfo>,
}

impl AstContext {
    /// Check if this context has any useful information
    #[cfg(test)]
    pub fn has_enrichment(&self) -> bool {
        !self.table_aliases.is_empty()
            || !self.cte_definitions.is_empty()
            || !self.subquery_aliases.is_empty()
    }
}
