# Schema Handling Design

## Goals
- Accurate lineage even with `*` or unqualified identifiers.
- Clear separation + precedence between imported schema (user) and implied schema (workload), with a deterministic hybrid view.
- Transparent, auditable schema assumptions surfaced to callers/UI, including approximate signals.
- Serve both roles of schema: (1) UI exploration (imported, implied, hybrid), (2) parser/lineage expansion for columns.
- Get 90% right with minimal complexity; include the most common DDL (`CREATE TABLE ... (cols...)`, CTAS, CREATE VIEW) in the core and defer advanced mutations/merging.

## Definitions
- **Imported schema**: User-supplied catalog/schema/table/columns (optional types).
- **Implied schema**: Tables/columns inferred from DDL in the analyzed workload (e.g., `CREATE TABLE`, `CREATE VIEW`, `CTAS`, `CREATE OR REPLACE`).
- **Hybrid schema**: Union of imported and implied; imported wins conflicts at table level (if imported defines a table, ignore implied for that table, but log mismatches).
- **Approximate lineage**: Lineage produced when columns could not be fully expanded (e.g., `SELECT *` without schema metadata); flagged for UI/callers.

## Analyzer Behavior

### 1. Seed registry with imported schema from request
- Normalize table/column names via dialect case rules + search path.
- Store each imported table with metadata: `origin: imported`, `updatedAt` timestamp.
- If `allowImplied` is false (default: true), skip implied capture entirely (imported-only mode).

### 2. While analyzing statements (implied capture)
**Phase 1 (Core - 90% value):**
- Capture implied tables/columns from `CREATE TABLE ... (col list...)` by parsing explicit column definitions.
- Capture implied tables/columns from `CREATE VIEW` and `CTAS` by analyzing the SELECT output columns.
- Capture implied tables from `CREATE OR REPLACE VIEW/TABLE` (replaces previous implied entry for that table).
- Detect temporary tables (`CREATE TEMP/TEMPORARY TABLE`) and mark with `temporary: true`.
- Handle `DROP TABLE` by removing the implied entry (prevents stale implied schema within the workload).
- For each implied entry, store: `origin: implied`, `sourceStatementIndex`, `updatedAt` timestamp.

**Phase 2+ (Deferred - complexity):**
- `ALTER TABLE ADD/DROP COLUMN`: Mutate implied schema (skip for Phase 1).
- `ALTER TABLE RENAME COLUMN`: Track alias (skip for Phase 1).

### 3. Resolution for lineage (table-level precedence)
**Hybrid Registry Lookup:**
- If imported schema defines table `T`, use imported columns exclusively (ignore any implied schema for `T`).
- If no imported schema for table `T`, use implied columns if available.
- This ensures user-provided schema is authoritative while allowing workload discovery to fill gaps.
- **Conflict signaling**: When implied schema for table `T` differs from imported (extra/missing/typed columns), still use imported, but emit a schema-mismatch issue and mark `SELECT *` expansions as `approximate=true` for that table.

**SELECT * Expansion:**
- **Full expansion**: All columns known → expand and create lineage for each column.
- **Partial expansion**: Some columns known → expand known columns, emit edges with `approximate: true`, log INFO issue noting incomplete column list.
- **No expansion**: No columns known → emit `APPROXIMATE_LINEAGE` warning issue, create table-level lineage only.

**Resolution Tracking:**
- Track which source resolved each table (`imported`, `implied`, or `unknown`).
- Keep existing `UNKNOWN_TABLE`/`UNKNOWN_COLUMN` issues; add `resolutionSource` tag for context.

### 4. Output
Attach `resolvedSchema` to `AnalyzeResult`:
- List all tables used during analysis (both imported and implied).
- Annotate each with `origin`, `sourceStatementIndex?`, `updatedAt`, `temporary?`.
- Include column-level `origin` tracking (marks which columns came from which source).

Attach lineage-level metadata:
- `approximate: boolean` on edges (true when expansion was partial/uncertain).
- `resolutionSource: "imported" | "implied" | "unknown"` on nodes (tracks lookup result).

## UI/Caller Expectations
- **Imported/Implied/Hybrid views**: UI can filter `resolvedSchema` by origin to show separate imported/implied/combined views.
- **Source statement links**: Implied tables show reference to `sourceStatementIndex` so user can jump to the `CREATE` statement.
- **Temporary table indicators**: Flag temporary tables as session-scoped (not persisted).
- **Approximate lineage prompts**: When `approximate: true` or `APPROXIMATE_LINEAGE` appears, suggest adding columns to imported schema or ensuring DDL appears earlier in workload.
- **Schema export**: User can export `resolvedSchema` (including implied tables) as JSON, then reimport as `schema.tables` for next analysis.

## Data Model

### Internal (Analyzer)
```rust
struct SchemaTableEntry {
    table: SchemaTable,              // Canonical table with columns
    origin: SchemaOrigin,            // Imported | Implied
    source_statement_idx: Option<usize>, // For implied: which statement created it
    updated_at: SystemTime,          // Timestamp
    temporary: bool,                 // True for temp tables
}

enum SchemaOrigin {
    Imported,
    Implied,
}
```

### Request API
```typescript
interface SchemaMetadata {
    defaultCatalog?: string;
    defaultSchema?: string;
    searchPath?: SchemaNamespaceHint[];
    caseSensitivity?: CaseSensitivity;
    tables: SchemaTable[];

    // NEW: Global toggle for implied schema (default: true)
    allowImplied?: boolean;
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
```

### Response API
```typescript
interface AnalyzeResult {
    statements: StatementLineage[];
    globalLineage: GlobalLineage;
    issues: Issue[];
    summary: Summary;

    // NEW: Effective schema used during analysis
    resolvedSchema?: ResolvedSchemaMetadata;
}

interface ResolvedSchemaMetadata {
    tables: ResolvedSchemaTable[];
}

interface ResolvedSchemaTable {
    catalog?: string;
    schema?: string;
    name: string;
    columns: ResolvedColumnSchema[];

    // Metadata
    origin: "imported" | "implied";
    sourceStatementIndex?: number;  // For implied tables
    updatedAt: string;              // ISO 8601 timestamp
    temporary?: boolean;            // True for temp tables
}

interface ResolvedColumnSchema {
    name: string;
    dataType?: string;
    origin?: "imported" | "implied"; // Column-level origin tracking
}

// Enhanced edge metadata
interface Edge {
    // ... existing fields (id, source, target, edgeType)

    approximate?: boolean;          // True if expansion was partial/uncertain
}

// Enhanced node metadata
interface Node {
    // ... existing fields (id, nodeType, label, etc.)

    resolutionSource?: "imported" | "implied" | "unknown"; // How table was resolved
}
```

## Precedence Rules (Table-Level)

1. **Imported wins conflicts**: If imported schema defines table `users`, use imported columns exclusively, even if workload has `CREATE TABLE users (...)`.
2. **Implied fills gaps**: If imported schema does NOT define table `orders`, use implied schema from workload if available.
3. **No column-level merging** (Phase 1): Precedence is all-or-nothing at table level. Future phases could merge columns.

### Examples

**Example 1 - Imported wins:**
```sql
-- Imported schema: users(id, name)
-- Workload: CREATE TABLE users (id INT, name VARCHAR, email VARCHAR)
-- Result: Hybrid uses imported columns [id, name], ignores implied [email]; emits mismatch issue
SELECT * FROM users;  -- Expands to: id, name; approximate=true due to mismatch
```

**Example 2 - Implied fills gap:**
```sql
-- Imported schema: (empty)
-- Workload: CREATE VIEW active_users AS SELECT id, name FROM users
-- Result: Hybrid includes implied active_users(id, name)
SELECT * FROM active_users;  -- Expands to: id, name
```

**Example 3 - Progressive schema building:**
```sql
-- Imported schema: users(id)
-- Workload:
CREATE TABLE orders AS SELECT user_id, amount FROM raw_orders;
INSERT INTO report SELECT * FROM orders;  -- Expands to: user_id, amount (from implied)
```

**Example 4 - allowImplied=false:**
```sql
-- Request: allowImplied=false
-- Workload: CREATE TABLE new_table AS SELECT * FROM users
SELECT * FROM new_table;  -- Results in UNKNOWN_TABLE or APPROXIMATE_LINEAGE warning
```

## Testing Plan

### Unit Tests
- **Implied capture from CTAS/CREATE VIEW**: Verify columns extracted from SELECT output.
- **CREATE TABLE (explicit column list)**: Verify explicit column parsing populates implied schema.
- **CREATE OR REPLACE**: Later DDL replaces earlier implied schema for same table.
- **Temporary tables**: Detected and flagged with `temporary: true`.
- **Imported precedence**: Imported table ignores implied table with same name.
- **Conflict signaling**: When imported vs implied differ for same table, imported is used and a mismatch issue + approximate flag on `*` is emitted.
- **Implied fills gaps**: Implied table used when no imported schema exists.
- **allowImplied=false**: No implied schema captured or used.
- **SELECT * expansion**:
  - Full: All columns known → expand fully, approximate=false.
  - Partial: Some columns known → expand known, approximate=true, INFO issue.
  - None: No columns known → APPROXIMATE_LINEAGE warning, table-level lineage only.

### Integration/Golden Tests
- Assert `resolvedSchema` contents, origins, and timestamps.
- Assert lineage edges have correct `approximate` flags.
- Assert nodes have correct `resolutionSource` tags.
- Test cross-statement dependencies (table created in stmt 1, used in stmt 2).

### Schema Guard (TypeScript)
- Update schema guard to validate `resolvedSchema` structure.
- Ensure `origin`, `sourceStatementIndex`, `updatedAt`, `temporary`, `approximate`, `resolutionSource` fields.

## Rollout Steps

### Phase 1: Core Implementation (Target: 90% value, 2-3 weeks)
1. **Data model updates**:
   - Add `SchemaTableEntry` wrapper with origin/metadata.
   - Add `ResolvedSchemaMetadata` to response types.
   - Add `allowImplied` to request.
   - Add `approximate` to edges, `resolutionSource` to nodes.

2. **Analyzer changes**:
   - Wrap imported schema in `SchemaTableEntry` with `origin: Imported`.
   - Implement CREATE TABLE (explicit columns) parsing.
   - Implement CTAS/CREATE VIEW column extraction (analyze SELECT output).
   - Implement CREATE OR REPLACE handling (replace implied entry).
   - Detect temporary tables (parse `CREATE TEMP TABLE`).
   - Implement table-level precedence (imported beats implied) with conflict signaling/approximate on mismatches.
   - Implement DROP TABLE removal for implied entries.
   - Enhance `*` expansion: full/partial/none with approximate flags.

3. **Output**:
   - Populate `resolvedSchema` in `AnalyzeResult`.
   - Emit `approximate` flags on edges.
   - Emit `resolutionSource` tags on nodes.

4. **Testing**:
   - Add unit tests for all core scenarios.
   - Add golden tests with `resolvedSchema` assertions.
   - Update TypeScript schema guard.

### Phase 2: Enhanced Capture (Deferred, 1-2 weeks)
- `ALTER TABLE ADD COLUMN` (mutate implied schema).
- `ALTER TABLE DROP COLUMN` (mutate implied schema).
- `ALTER TABLE RENAME COLUMN` (track aliases).

### Phase 3: Advanced Merging (Future consideration)
- Column-level merging (combine imported + implied columns while retaining origin metadata).

### Phase 4: UI Integration (Post-core)
- Render imported/implied/hybrid views in UI.
- Source statement links for implied tables.
- Approximate lineage prompts and suggestions.
- Schema export/import workflow.

## Open Questions & Future Considerations

1. **Type inference**: Should we infer column types from SQL expressions (e.g., `SELECT id + 1` → `INTEGER`)? Defer to Phase 3+.
2. **CTE schema**: Should CTEs be captured as implied schema? Currently CTEs are statement-local; could add `origin: ImpliedCte` with statement scope.
3. **Column-level merging**: If imported has `users(id, name)` and implied has `users(id, name, email)`, should hybrid be `[id, name]` or `[id, name, email]`? Phase 1 uses table-level precedence (imported wins); Phase 3+ could merge.
4. **Schema versioning**: If same table is created multiple times, should all versions be tracked? Phase 1 uses last-write-wins; Phase 3+ could add versioning.
5. **Cross-dialect normalization**: How to handle dialect differences in temp table syntax, CREATE OR REPLACE support, etc.? Handle incrementally as issues arise.
