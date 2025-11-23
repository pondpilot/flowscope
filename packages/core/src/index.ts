// Main exports
export { analyzeSql, analyzeSimple } from './analyzer';
export { initWasm, isWasmInitialized, resetWasm } from './wasm-loader';
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
} from './types';

// Constants
export { IssueCodes } from './types';
