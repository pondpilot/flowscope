// Main exports
export {
  analyzeSql,
  analyzeSimple,
  completionItems,
  exportToDuckDbSql,
  formatSchemaError,
  splitStatements,
  validateSchemaName,
} from './analyzer';
export { initWasm, isWasmInitialized, resetWasm, getEngineVersion } from './wasm-loader';
export type { InitWasmOptions } from './wasm-loader';

// Type exports
export type {
  // Request types
  AnalyzeRequest,
  AnalysisOptions,
  CompletionClause,
  CompletionColumn,
  CompletionContext,
  CompletionItem,
  CompletionItemCategory,
  CompletionItemKind,
  CompletionItemsResult,
  CompletionKeywordHints,
  CompletionKeywordSet,
  CompletionRequest,
  CompletionTable,
  CompletionToken,
  CompletionTokenKind,
  Dialect,
  StatementSplitRequest,
  StatementSplitResult,
  SchemaMetadata,
  SchemaNamespaceHint,
  SchemaTable,
  ColumnSchema,
  ForeignKeyRef,
  FileSource,
  // Response types
  AnalyzeResult,
  StatementLineage,
  Node,
  NodeType,
  TableLikeNodeType,
  Edge,
  EdgeType,
  JoinType,
  FilterPredicate,
  FilterClauseType,
  AggregationInfo,
  GlobalLineage,
  GlobalNode,
  GlobalEdge,
  CanonicalName,
  StatementRef,
  Issue,
  Severity,
  Span,
  Summary,
  IssueCount,
  ResolvedSchemaMetadata,
  ResolvedSchemaTable,
  ResolvedColumnSchema,
  SchemaOrigin,
  ResolutionSource,
} from './types';

// Constants and utilities
export {
  IssueCodes,
  isTableLikeType,
  charOffsetToByteOffset,
  byteOffsetToCharOffset,
} from './types';
