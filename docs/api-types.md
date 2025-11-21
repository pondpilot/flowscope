# API Type Definitions

This document defines the exact TypeScript interfaces that will be generated from Rust types via the `serde` + `schemars` pipeline.

## Request Types

### AnalyzeRequest

```typescript
interface AnalyzeRequest {
  /** The SQL code to analyze (UTF-8 string, multi-statement supported) */
  sql: string;

  /** SQL dialect - required field with default 'generic' at TS wrapper level */
  dialect: 'generic' | 'postgres' | 'snowflake' | 'bigquery';

  /** Optional analysis options */
  options?: AnalysisOptions;

  /** Optional schema metadata for accurate column resolution */
  schema?: SchemaMetadata;
}

interface AnalysisOptions {
  /** Enable column-level lineage (Phase 2+, default true when implemented) */
  enableColumnLineage?: boolean;
}

interface SchemaMetadata {
  /** Default catalog applied to unqualified identifiers */
  defaultCatalog?: string;

  /** Default schema applied to unqualified identifiers */
  defaultSchema?: string;

  /** Ordered list mirroring database search_path behavior */
  searchPath?: SchemaNamespaceHint[];

  /** Override for identifier normalization (default 'dialect') */
  caseSensitivity?: 'dialect' | 'lower' | 'upper' | 'exact';

  /** Canonical table representations */
  tables: SchemaTable[];
}

interface SchemaNamespaceHint {
  catalog?: string;
  schema: string;
}

interface SchemaTable {
  catalog?: string;
  schema?: string;
  name: string;
  columns: ColumnSchema[];
}

interface ColumnSchema {
  name: string;
  dataType?: string;
}

/**
 * Backwards compatibility:
 * the TS wrapper still accepts the legacy Record<string, { columns: string[] }>
 * form and rewrites it to the structured representation above before the
 * request crosses the WASM boundary.
 */
```

## Response Types

### AnalyzeResult

```typescript
interface AnalyzeResult {
  /** Per-statement lineage analysis results */
  statements: StatementLineage[];

  /** Global lineage graph spanning all statements */
  globalLineage: GlobalLineage;

  /** All issues encountered during analysis */
  issues: Issue[];

  /** Summary statistics */
  summary: Summary;
}

interface GlobalLineage {
  nodes: GlobalNode[];
  edges: GlobalEdge[];
}

interface GlobalNode {
  /** Stable ID derived from canonical identifier */
  id: string;
  type: Node['type'];
  label: string;
  canonicalName: CanonicalName;
  statementRefs: StatementRef[];
  metadata?: Record<string, unknown>;
}

interface CanonicalName {
  catalog?: string;
  schema?: string;
  name: string;
  column?: string;
}

interface StatementRef {
  /** Statement index in the original request */
  statementIndex: number;
  /** ID of the local node inside that statement graph (if available) */
  nodeId?: string;
}

interface GlobalEdge {
  id: string;
  from: string;
  to: string;
  type: Edge['type'] | 'cross_statement';
  producerStatement?: StatementRef;
  consumerStatement?: StatementRef;
  metadata?: Record<string, unknown>;
}
```

### StatementLineage

```typescript
interface StatementLineage {
  /** Zero-based index of the statement in the input SQL */
  statementIndex: number;

  /**
   * Type of SQL statement as a string enum:
   * 'SELECT' | 'INSERT' | 'CREATE_TABLE_AS' | 'WITH' | 'UNION' | 'UNKNOWN'
   */
  statementType: string;

  /** All nodes in the lineage graph for this statement */
  nodes: Node[];

  /** All edges connecting nodes in the lineage graph */
  edges: Edge[];

  /** Optional span of the entire statement in source SQL */
  span?: Span;
}
```

### Node

```typescript
interface Node {
  /**
   * Stable content-based hash ID
   * Format: hash of (type, qualified name, expression)
   */
  id: string;

  /** Node type */
  type: 'table' | 'cte' | 'column';

  /** Human-readable label (short name) */
  label: string;

  /** Fully qualified name when available (e.g., 'db.schema.table' or 'table.column') */
  qualifiedName?: string;

  /** SQL expression text for computed columns */
  expression?: string;

  /** Source location in original SQL */
  span?: Span;

  /** Extensible metadata for future use */
  metadata?: Record<string, unknown>;
}
```

### Edge

```typescript
interface Edge {
  /** Stable content-based hash ID */
  id: string;

  /** Source node ID */
  from: string;

  /** Target node ID */
  to: string;

  /**
   * Edge type:
   * - 'ownership': table/CTE owns columns
   * - 'data_flow': data flows from one column to another
   * - 'derivation': output derived from inputs (with transformation)
   */
  type: 'ownership' | 'data_flow' | 'derivation';

  /** Optional: SQL expression if this edge represents a transformation */
  expression?: string;

  /** Optional: operation label ('JOIN', 'UNION', 'AGGREGATE', etc.) */
  operation?: string;

  /** Extensible metadata for future use */
  metadata?: Record<string, unknown>;
}
```

### Issue

```typescript
interface Issue {
  /** Severity level */
  severity: 'error' | 'warning' | 'info';

  /**
   * Machine-readable issue code, examples:
   * - PARSE_ERROR
   * - UNSUPPORTED_SYNTAX
   * - UNSUPPORTED_RECURSIVE_CTE
   * - APPROXIMATE_LINEAGE
   * - UNKNOWN_COLUMN
   * - UNKNOWN_TABLE
   * - DIALECT_FALLBACK
   */
  code: string;

  /** Human-readable error message */
  message: string;

  /** Optional: location in source SQL where issue occurred */
  span?: Span;

  /** Optional: which statement index this issue relates to */
  statementIndex?: number;
}
```

### Span

```typescript
interface Span {
  /** Byte offset from start of SQL string (inclusive) */
  start: number;

  /** Byte offset from start of SQL string (exclusive) */
  end: number;
}
```

### Summary

```typescript
interface Summary {
  /** Total number of statements analyzed */
  statementCount: number;

  /** Total unique tables/CTEs discovered across all statements */
  tableCount: number;

  /** Total columns in output (Phase 2+) */
  columnCount: number;

  /** Issue counts by severity */
  issueCount: {
    errors: number;
    warnings: number;
    infos: number;
  };

  /** Quick check: true if any errors were encountered */
  hasErrors: boolean;
}
```

## Rust Type Mapping Notes

When implementing in Rust, these types map as follows:

```rust
// Approximate Rust structure (actual implementation may vary)

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct AnalyzeRequest {
    pub sql: String,
    pub dialect: Dialect,
    #[serde(default)]
    pub options: Option<AnalysisOptions>,
    #[serde(default)]
    pub schema: Option<SchemaMetadata>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Dialect {
    Generic,
    Postgres,
    Snowflake,
    Bigquery,
}

// ... similar for other types
```

## Version Compatibility

- **Backward Compatibility**: Minor version changes may add optional fields
- **Breaking Changes**: Only in major versions
- **Omitted Fields**: Deserializers should tolerate missing optional fields
- **Unknown Fields**: Deserializers should ignore unknown fields (future-proofing)

## Examples

### Simple SELECT Request

```json
{
  "sql": "SELECT id, name FROM users WHERE active = true",
  "dialect": "postgres"
}
```

### With Schema Metadata

```json
{
  "sql": "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
  "dialect": "postgres",
  "schema": {
    "defaultSchema": "public",
    "searchPath": [{ "schema": "public" }],
    "tables": [
      {
        "schema": "public",
        "name": "users",
        "columns": [
          { "name": "id" },
          { "name": "name" },
          { "name": "email" },
          { "name": "active" }
        ]
      },
      {
        "schema": "public",
        "name": "orders",
        "columns": [
          { "name": "id" },
          { "name": "user_id" },
          { "name": "total" },
          { "name": "created_at" }
        ]
      }
    ]
  }
}
```

### Example Result (Simplified)

```json
{
  "statements": [
    {
      "statementIndex": 0,
      "statementType": "SELECT",
      "nodes": [
        {
          "id": "tbl_abc123",
          "type": "table",
          "label": "users",
          "qualifiedName": "users"
        },
        {
          "id": "col_def456",
          "type": "column",
          "label": "id",
          "qualifiedName": "users.id"
        }
      ],
      "edges": [
        {
          "id": "edge_xyz789",
          "from": "tbl_abc123",
          "to": "col_def456",
          "type": "ownership"
        }
      ]
    }
  ],
  "globalLineage": {
    "nodes": [
      {
        "id": "tbl_abc123",
        "type": "table",
        "label": "users",
        "canonicalName": { "name": "users" },
        "statementRefs": [{ "statementIndex": 0, "nodeId": "tbl_abc123" }]
      }
    ],
    "edges": []
  },
  "issues": [],
  "summary": {
    "statementCount": 1,
    "tableCount": 1,
    "columnCount": 1,
    "issueCount": {
      "errors": 0,
      "warnings": 0,
      "infos": 0
    },
    "hasErrors": false
  }
}
```

## Design Decisions Summary

1. **statementType**: Free-form string (not enum) for forward compatibility with new statement types
2. **No engineVersion**: Keeps response minimal, version tracking is deployment concern
3. **Edge spans**: No - edges derive from node relationships, nodes have spans
4. **Metadata fields**: Generic `Record<string, unknown>` for extensibility without breaking changes
5. **Issue codes**: Standardized strings in SCREAMING_SNAKE_CASE, documented in code
6. **Spans always optional**: `sqlparser-rs` doesn't always provide location info

Last Updated: 2025-11-20
