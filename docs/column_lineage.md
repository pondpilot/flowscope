# Column Lineage Semantics

This document summarizes how FlowScope represents column-level lineage in statement graphs.

## Edge Types

- **Ownership**: table/CTE/view/output owns its columns.
- **Data flow**: direct projection from a source column.
- **Derivation**: computed column derived from one or more inputs.
- **Join dependency**: output depends on all joined sources.
- **Cross-statement**: global graph links producers to downstream consumers.

## Column Rules

- **Direct projection** (`SELECT col AS alias`) → `data_flow` edge.
- **Computed expressions** (`SELECT a + b AS total`) → `derivation` edge and `expression` metadata.
- **Aggregations**:
  - Grouping columns flow directly (`data_flow`).
  - Aggregated outputs are `derivation` with `aggregation` metadata.
- **Set operations** map branch outputs to combined outputs via `derivation`.
- **Write targets** (INSERT/CTAS/VIEW) create `ownership` edges on target columns plus flow/derivation edges from sources.

## Approximate Lineage

- When schema is missing and `SELECT *` is used, column lineage can be marked `approximate`.
- Approximate edges indicate uncertainty rather than failure.

## Node/Edge Metadata

- Nodes may include `joinType`, `joinCondition`, `filters`, and `resolutionSource`.
- Edges may include `operation`, `joinType`, `joinCondition`, and `approximate`.

## Testing Guidance

Prefer stable summaries of column lineage (labels, types, and counts) over hash IDs, which are content-derived and subject to change.
