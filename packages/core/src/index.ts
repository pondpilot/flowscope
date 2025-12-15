// Main exports
export { analyzeSql, analyzeSimple } from './analyzer';
export { initWasm, isWasmInitialized, resetWasm, getEngineVersion } from './wasm-loader';
export type { InitWasmOptions } from './wasm-loader';

// Type exports
export type {
  // Request types
  AnalyzeRequest,
  AnalysisOptions,
  Dialect,
  SchemaMetadata,
  SchemaNamespaceHint,
  SchemaTable,
  ColumnSchema,
  ForeignKeyRef,
  ColumnTag,
  TagSource,
  TagHint,
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
  TagCount,
  TagFlowSummary,
} from './types';

// Constants and utilities
export { IssueCodes, isTableLikeType } from './types';
