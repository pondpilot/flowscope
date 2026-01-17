# Core Engine Spec (Rust)

This document describes the behavior of the Rust lineage engine (`flowscope-core`). It focuses on runtime behavior rather than API surface.

## Responsibilities

The core engine must:

1. Accept:
   - Fully rendered SQL (no templating/macros).
   - Dialect selection.
   - Optional schema metadata and analysis options.
2. Parse SQL via `sqlparser-rs`.
3. For each statement, compute:
   - Statement lineage (nodes + edges).
   - Statement summary stats (join count, complexity score).
4. Build a global lineage graph across statements.
5. Emit issues for parsing problems, unsupported syntax, or approximate lineage.

## Supported Statement Types

FlowScope analyzes the following statements when parsed successfully by `sqlparser-rs`:

- `SELECT` / `WITH` / set operations
- `INSERT INTO ... SELECT`
- `CREATE TABLE` (explicit columns)
- `CREATE TABLE AS SELECT`
- `CREATE VIEW`
- `UPDATE`
- `DELETE`
- `MERGE`
- `DROP` (used to update implied schema)

If a statement parses but is not supported, the engine emits `UNSUPPORTED_SYNTAX` and returns a minimal lineage placeholder.

## Schema Metadata Behavior

- Schema metadata is optional.
- When provided, the engine validates table/column references and expands `SELECT *`.
- When absent, it performs best-effort lineage and emits `APPROXIMATE_LINEAGE` warnings as needed.
- Implied schema can be captured from DDL when `allowImplied` is enabled.

## Lineage Graph Output

Each statement yields:

- **Nodes**: `table`, `view`, `cte`, `output`, `column`.
- **Edges**: `ownership`, `data_flow`, `derivation`, `join_dependency`.
- **Metadata**: join conditions, aggregation info, filter predicates, approximate flags.

The global graph (`GlobalLineage`) deduplicates table/column identifiers across statements and adds `cross_statement` edges.

## Issues & Summary

- Issues include severity, code, message, span, and statement index.
- `Summary` includes counts (statements, tables, columns, joins), a complexity score, and per-severity issue counts.

## Performance Expectations

- The engine favors deterministic behavior and stable output for identical inputs.
- Large statements may incur higher latency, but analysis should remain linear relative to total nodes/edges.
