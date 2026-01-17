# Schema Handling Design

## Goals

- Accurate lineage with `*` and unqualified identifiers.
- Deterministic precedence between imported schema and implied schema.
- Clear signaling when lineage is approximate or schema conflicts occur.

## Definitions

- **Imported schema**: user-supplied schema metadata in the request.
- **Implied schema**: tables/columns inferred from DDL in the analyzed workload.
- **Resolved schema**: effective schema used for lineage (imported + implied).

## Request API

```typescript
interface SchemaMetadata {
  defaultCatalog?: string;
  defaultSchema?: string;
  searchPath?: SchemaNamespaceHint[];
  caseSensitivity?: 'dialect' | 'lower' | 'upper' | 'exact';
  tables?: SchemaTable[];
  allowImplied?: boolean;
}
```

## Resolution Rules

1. **Imported wins**: if an imported table exists, it overrides implied versions.
2. **Implied fills gaps**: if no imported table exists, use implied schema when available.
3. **Unknown tables**: emit `UNKNOWN_TABLE` and proceed with best-effort lineage.
4. **Conflicts**: when imported and implied differ, emit `SCHEMA_CONFLICT` and mark lineage as approximate where `*` expansion is affected.

## SELECT * Expansion

- With schema metadata: expand to known columns.
- With partial schema: expand known columns and mark edges `approximate`.
- With no schema: emit `APPROXIMATE_LINEAGE` and provide table-level lineage only.

## Implied Schema Capture

Implied capture is enabled when `allowImplied` is true (default). The analyzer infers schema from:

- `CREATE TABLE` (explicit column list)
- `CREATE TABLE AS SELECT`
- `CREATE VIEW`
- `CREATE OR REPLACE` variants
- `DROP TABLE` (removes implied entries)

## Output Fields

```typescript
interface AnalyzeResult {
  resolvedSchema?: ResolvedSchemaMetadata;
}

interface ResolvedSchemaTable {
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

## Node/Edge Metadata

- Nodes may include `resolutionSource` (`imported`, `implied`, `unknown`).
- Edges may include `approximate` when column expansion is partial.
