# Roadmap & Phasing

This document breaks the project into **phases** with clear deliverables and priorities.

## Phase 0 – Spike / Feasibility

**Objective:** Prove that the chosen stack (Rust + `sqlparser-rs` + WASM) works end-to-end for a simple case.

**Deliverables:**

- Minimal Rust project:
  - Parse a simple `SELECT` using `sqlparser-rs`.
  - Produce a trivial lineage summary (e.g., list tables).
- WASM build:
  - Expose a simple function.
  - Call it from a minimal HTML/JS page.
- No packaging or UI polish yet.

## Phase 1 – Table-Level Lineage MVP

**Objective:** Provide a usable core engine for **table-level lineage** across supported statements and dialects, exposed via `@pondpilot/flowscope-core`.

**Scope:**

- Statement coverage:
  - `SELECT`, `WITH`, `INSERT INTO ... SELECT`, `CREATE TABLE AS SELECT`, `UNION`.
- Lineage:
  - Table-level only (no column-level).
  - **Global cross-statement graph** emitted alongside each analysis run.
- Dialects:
  - Generic, Postgres, Snowflake, BigQuery.
- Output:
  - Per-statement table/CTE graph.
  - Basic issues and summary.
- JS/TS:
  - `@pondpilot/flowscope-core` with:
    - WASM loader.
    - `initWasm`.
    - `analyzeSql`.
- Example app:
  - Very basic UI:
    - SQL input.
    - Dialect selector.
    - JSON result display (no graph visualization yet).

## Phase 2 – Column-Level Lineage & Schema Support

**Objective:** Add **column-level lineage** capabilities and schema-awareness.

**Scope:**

- Column-level lineage:
  - For explicit columns regardless of schema.
  - For `*`/`table.*` only when schema is available.
- Schema metadata:
  - Extend canonical schema format with optional data types / PK hints.
  - Graceful behavior without schema.
- Issues:
  - Explicit warnings for approximate lineage.
- Example app:
  - Display basic column-level info (e.g., upstream columns) in a simple UI.

## Phase 3 – React Viewer & Full Demo

**Objective:** Build the React viewer (`@pondpilot/flowscope-react`) and a polished demo app.

**Scope:**

- Viewer:
  - Graph view (tables + optional columns).
  - Column lineage panel.
  - SQL highlight view.
  - Issues panel.
- Demo app:
  - Uses viewer components.
  - Demonstrates schema input, multi-statement analysis.
  - Provides raw JSON debug view.

## Phase 4 – Performance & Scalability

**Objective:** Make the engine pleasant for large workloads in real UIs and harden worker-based execution.

**Scope:**

- `@lineage/core`:
  - Optimize the existing Web Worker helper (pooling, improved cancellation, memory tuning options).
  - Add configurable payload chunking/compression and expose telemetry hooks.
- Performance:
  - Basic benchmarks established and automated.
  - Address obvious hot spots if any.
- Stability:
  - Strengthen tests around large queries and multiple statements.

## Phase 5 – Ecosystem & Integrations

**Objective:** Prepare for broader adoption and integration.

**Scope:**

- Documentation:
  - Integration guides for:
    - Internal apps.
    - Browser extension authors.
    - IDE/plugin authors.
- Sample integrations (minimal prototypes):
  - Browser extension sketch (if feasible).
  - VS Code extension sketch (optional).
- Licensing and release process:
  - Decide on license(s) for core and UI packages.
  - Set up automated releases to NPM and relevant registries.

## Later Enhancements (Post-v1 Ideas)

- Additional dialects:
  - MySQL, Redshift, Databricks SQL, etc.
- More statement types:
  - `UPDATE`, `DELETE`, `MERGE`.
- Richer schema metadata:
  - Types, keys, constraints.
- Export to standard lineage formats:
  - OpenLineage-style JSON events.
- Advanced visualizations:
  - Time-based lineage views.
  - Grouping/aggregation of nodes for very large graphs.
