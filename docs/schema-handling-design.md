# Schema Handling Design

## Goals
- Accurate lineage when `*` or unqualified identifiers are present.
- Clear separation and precedence between user-provided schema and schema implied by the workload.
- Transparent, auditable schema assumptions surfaced to callers/UI.

## Definitions
- **Imported schema**: user-supplied catalog/schema/table/columns (optional types).
- **Implied schema**: tables/columns inferred from DDL in the analyzed workload (e.g., `CREATE TABLE/VIEW/CTAS`).
- **Hybrid schema**: union of imported and implied; imported wins on conflicts unless explicitly overridden.

## Analyzer Behavior
1. **Seed registry with imported schema from the request**  
   - Normalize via dialect case rules + search path (current behavior).  
   - Track origin metadata (`origin: imported`, `updatedAt`).

2. **While analyzing statements**  
   - For `CREATE TABLE/VIEW/CTAS`, capture output columns and insert into registry as implied (`origin: implied`, `sourceStatement`).  
   - Use hybrid registry for resolution: imported > implied. Record which source resolved a table/column.  
   - `*` expansion: expand only if the resolved table has columns in the registry (either origin); otherwise emit `APPROXIMATE_LINEAGE`.  
   - Issues: keep `UNKNOWN_TABLE`/`UNKNOWN_COLUMN`, but add context tags (resolved via imported/implied/unknown). Consider specific codes later (`UNKNOWN_TABLE_IMPORTED`, etc.).

3. **Output**  
   - Add optional `resolvedSchema` to `AnalyzeResult`, containing tables/columns with origin (`imported|implied`), `sourceStatement` for implied, and timestamps.  
   - Leave `schema` request field as-is; add `schema.allowImplied?: boolean` (default true) and `schema.overrides?` if we want an explicit imported block vs implied acceptance.

## UI/Caller Expectations
- Show Imported, Implied, and Hybrid views; allow toggling implied items on/off.
- When approximate lineage or missing columns occur, prompt to add columns to imported schema and re-run.
- For DDL, display implied schema with source statement reference.

## Data Model
- Canonical key: `catalog.schema.table` (column list under each table).
- Column fields: name, optional type.
- Metadata: `origin`, `sourceStatement` (implied), `updatedAt`.

## API Changes (incremental)
- Request: add `schema.allowImplied?: boolean` (default true).
- Response: add `resolvedSchema?: { tables: [...], origin, sourceStatement? }`.
- Backward compatible: if not provided, behavior matches current (imported-only).

## Testing Plan
- Unit tests for:
  - Implied schema capture from `CREATE TABLE/VIEW/CTAS`.
  - Imported vs implied precedence (imported wins).
  - `*` expansion using implied columns vs approximate when absent.
- Golden tests that assert `resolvedSchema` contents and origins.
- TS schema guard updated to cover `resolvedSchema`/new fields.

## Rollout Steps
- Extend Rust types with `resolvedSchema`, origin metadata, and `allowImplied`.
- Implement registry, precedence, and implied capture in analyzer.
- Surface `resolvedSchema` in `AnalyzeResult`; add golden tests.
- Update TS types + schema guard.
- Add UI hooks later (separate change) to render/import/implied/hybrid views.
