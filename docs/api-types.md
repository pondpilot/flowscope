# API Type Definitions

This document summarizes the public TypeScript API for `@pondpilot/flowscope-core`.

**Source of truth:**
- `packages/core/src/types.ts` (authoritative)
- `docs/api_schema.json` (generated schema snapshot)

## Encoding

All analysis functions accept an optional `encoding` field to control how text offsets are represented:

```typescript
export type Encoding = 'utf8' | 'utf16';
```

- `'utf8'` (default): All span offsets are UTF-8 byte offsets
- `'utf16'`: All span offsets are UTF-16 code units (for JavaScript/browser compatibility)

## Requests

### AnalyzeRequest

```typescript
export interface AnalyzeRequest {
  sql: string;
  files?: FileSource[];
  dialect: Dialect;
  sourceName?: string;
  options?: AnalysisOptions;
  schema?: SchemaMetadata;
}

export interface AnalysisOptions {
  enableColumnLineage?: boolean;
  graphDetailLevel?: 'script' | 'table' | 'column';
  hideCtes?: boolean;
}
```

### SchemaMetadata

```typescript
export interface SchemaMetadata {
  defaultCatalog?: string;
  defaultSchema?: string;
  searchPath?: SchemaNamespaceHint[];
  caseSensitivity?: 'dialect' | 'lower' | 'upper' | 'exact';
  tables?: SchemaTable[];
  allowImplied?: boolean;
}

export interface SchemaTable {
  catalog?: string;
  schema?: string;
  name: string;
  columns?: ColumnSchema[];
}

export interface ColumnSchema {
  name: string;
  dataType?: string;
  isPrimaryKey?: boolean;
  foreignKey?: ForeignKeyRef;
}
```

## Responses

### AnalyzeResult

The lineage graph is flat: a single top-level `nodes` / `edges` pair spans
every statement. Each `Node` / `Edge` carries `statementIds` listing every
statement it participates in. Per-statement metadata (type, span,
complexity) lives in `statements: StatementMeta[]`.

```typescript
export interface AnalyzeResult {
  statements: StatementMeta[];
  nodes: Node[];
  edges: Edge[];
  issues: Issue[];
  summary: Summary;
  resolvedSchema?: ResolvedSchemaMetadata;
}
```

### StatementMeta

```typescript
export interface StatementMeta {
  statementIndex: number;
  statementType: string;
  sourceName?: string;
  span?: Span;
  joinCount: number;
  complexityScore: number;
  resolvedSql?: string;
}
```

### Node & Edge

```typescript
export type NodeType = 'table' | 'view' | 'cte' | 'output' | 'column';

export interface Node {
  id: string;
  type: NodeType;
  label: string;
  qualifiedName?: string;
  canonicalName?: CanonicalName;
  statementIds: number[];
  expression?: string;
  span?: Span;
  nameSpans?: Span[];
  bodySpan?: Span;
  metadata?: Record<string, unknown>;
  resolutionSource?: 'imported' | 'implied' | 'unknown';
  filters?: FilterPredicate[];
  aggregation?: AggregationInfo;
}

export interface CanonicalName {
  catalog?: string;
  schema?: string;
  name: string;
  column?: string;
}

export type EdgeType =
  | 'ownership'
  | 'data_flow'
  | 'derivation'
  | 'join_dependency'
  | 'cross_statement';

export interface Edge {
  id: string;
  from: string;
  to: string;
  type: EdgeType;
  statementIds: number[];
  expression?: string;
  operation?: string;
  joinType?: JoinType;
  joinCondition?: string;
  metadata?: Record<string, unknown>;
  approximate?: boolean;
}
```

`cross_statement` edges carry `statementIds: [producer, consumer]` in
that order. Other edge kinds list every statement that produced an
edge with the same `(from, to, edgeType)` triple.

### Issues & Summary

```typescript
export interface Issue {
  severity: 'error' | 'warning' | 'info';
  code: string;
  message: string;
  span?: Span;
  statementIndex?: number;
}

export interface Summary {
  statementCount: number;
  tableCount: number;
  columnCount: number;
  joinCount: number;
  complexityScore: number;
  issueCount: IssueCount;
  hasErrors: boolean;
}
```

### Resolved Schema

```typescript
export interface ResolvedSchemaMetadata {
  tables: ResolvedSchemaTable[];
}

export interface ResolvedSchemaTable {
  catalog?: string;
  schema?: string;
  name: string;
  columns: ResolvedColumnSchema[];
  origin: 'imported' | 'implied';
  sourceStatementIndex?: number;
  updatedAt: string;
  temporary?: boolean;
  constraints?: TableConstraintInfo[];
}
```

## Exports

```typescript
export type MermaidView = 'all' | 'script' | 'table' | 'column' | 'hybrid';

export type ExportFormat =
  | 'json'
  | 'mermaid'
  | 'html'
  | 'sql'
  | 'csv'
  | 'xlsx'
  | 'duckdb'
  | 'png';
```

Core helpers (via `packages/core/src/index.ts`):

- `exportJson(result, { compact })`
- `exportMermaid(result, view)`
- `exportHtml(result, { projectName, exportedAt })`
- `exportCsvArchive(result)`
- `exportXlsx(result)`
- `exportFilename({ projectName, exportedAt, format, view, compact })`

## Notes

- All identifiers and enums are exported from `packages/core/src/index.ts`.
- Use `IssueCodes` for machine-readable error codes.
