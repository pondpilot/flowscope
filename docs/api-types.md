# API Type Definitions

This document summarizes the public TypeScript API for `@pondpilot/flowscope-core`.

**Source of truth:**
- `packages/core/src/types.ts` (authoritative)
- `docs/api_schema.json` (generated schema snapshot)

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

```typescript
export interface AnalyzeResult {
  statements: StatementLineage[];
  globalLineage: GlobalLineage;
  issues: Issue[];
  summary: Summary;
  resolvedSchema?: ResolvedSchemaMetadata;
}
```

### StatementLineage

```typescript
export interface StatementLineage {
  statementIndex: number;
  statementType: string;
  sourceName?: string;
  nodes: Node[];
  edges: Edge[];
  span?: Span;
  joinCount: number;
  complexityScore: number;
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
  expression?: string;
  span?: Span;
  metadata?: Record<string, unknown>;
  resolutionSource?: 'imported' | 'implied' | 'unknown';
  filters?: FilterPredicate[];
  joinType?: JoinType;
  joinCondition?: string;
  aggregation?: AggregationInfo;
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
  expression?: string;
  operation?: string;
  joinType?: JoinType;
  joinCondition?: string;
  metadata?: Record<string, unknown>;
  approximate?: boolean;
}
```

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

## Notes

- All identifiers and enums are exported from `packages/core/src/index.ts`.
- Use `IssueCodes` for machine-readable error codes.
