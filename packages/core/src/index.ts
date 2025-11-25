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
  FileSource,
  // Response types
  AnalyzeResult,
  StatementLineage,
  Node,
  NodeType,
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

// Constants
export { IssueCodes } from './types';
