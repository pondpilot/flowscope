# Testing & Quality Strategy

This document outlines how to validate **correctness, robustness, and performance** of the lineage engine and associated layers.

## 1. Core Testing Goals

- **Correctness**:
  - The lineage graph correctly reflects the data flow implied by SQL for supported constructs.
- **Robustness**:
  - The engine handles malformed and edge-case SQL gracefully.
  - Partial lineage is provided whenever possible.
- **Stability**:
  - Changes to the core engine do not silently break existing behavior.
- **Performance**:
  - Parsing and lineage are fast enough for interactive use on realistic queries.

## 2. Test Categories

### 2.1 Unit Tests (Rust)

- For `flowscope-core`:
  - Per-language-feature tests:
    - Simple `SELECT` with aliases.
    - Joins (`INNER`, `LEFT`, `RIGHT`, `FULL`, `CROSS`).
    - CTEs.
    - `INSERT INTO ... SELECT`.
    - `CREATE TABLE AS SELECT`.
    - `UNION` / `UNION ALL`.
  - Column lineage:
    - Expressions combining multiple columns.
    - Non-trivial aliases.
    - Window functions treated as expressions.
  - Schema-aware behavior:
    - With schema: `*` expansion correctness.
    - Without schema: proper warnings and approximate lineage.

### 2.2 Integration Tests (Rust + WASM boundary)

- Call the WASM-level function with JSON requests and validate JSON results:
  - Ensure:
    - Dialect selection works.
    - Request/response JSON is stable.
    - Issues are reported correctly.
    - `globalLineage` contains deduplicated nodes/edges that line up with per-statement graphs.

These can be run using a WASM test harness or via Node-based tests consuming the built WASM.

### 2.3 TS/JS Tests

- For `@pondpilot/flowscope-core`:
  - WASM loading:
    - Successful initialization.
    - Failure behavior when WASM cannot be loaded.
  - `analyzeSql`:
    - Propagation of engine-level results.
    - Proper handling of Promise rejections for technical errors.
- For worker helper:
  - Multiple concurrent analyze calls.
  - Worker lifecycle (creating/destroying).

### 2.4 UI Tests

- Snapshot/visual tests for `@pondpilot/flowscope-react`:
  - Graph rendering for simple scenarios.
  - Column lineage panel when selecting a node.
  - SQL highlighting alignment with spans.

Automated visual regression can be added later (e.g., with Storybook + visual diff tools).

### 2.5 End-to-End Tests (Web Demo)

- Use a headless browser to:
  - Load the demo app.
  - Paste sample SQL.
  - Select dialect.
  - Click "Analyze".
  - Assert:
    - No console errors.
    - A graph is rendered.
    - Expected number of tables/columns is present.

## 3. Test Data & Suites

### 3.1 Curated SQL Samples

Create a set of sample SQL files covering:

- Basic queries for each dialect (Generic/Postgres/Snowflake/BigQuery).
- Complex scripts:
  - Multiple CTEs.
  - Nested subqueries.
  - Chained `INSERT`/`CTAS`.
  - Unions and joins combined.

Each sample should have an expected:

- List of discovered tables.
- For some cases, expected lineage relationships (a small golden set).

**Location**: `crates/flowscope-core/tests/fixtures/`

### 3.2 Schema Metadata Samples

- Simple schemas:
  - Few tables, few columns.
- More realistic warehouse schemas:
  - Fact and dimension tables.

These schemas are used to verify:

- `*` expansion.
  - Column validation.

### 3.3 Dialect Coverage Matrix

- Maintain a machine-readable matrix (CSV/JSON) enumerating syntax features per dialect with status (`supported`, `partial`, `unsupported`).
- Link each row to one or more SQL fixtures plus expected outputs so regressions are easy to triage.
- Gate releases:
  - Every row marked `supported` must have at least one regression test.
  - Downgrading from `supported` → `partial/unsupported` requires a tracking issue and PM/tech lead sign-off.
- Surface matrix excerpts in docs so adopters know exactly what "dialect support" means today.

## 4. Regression & Compatibility

- Maintain a directory of **golden result snapshots**:
  - Input: SQL + schema + options.
  - Output: serialized lineage graphs + issues.
- API schema snapshot enforced by `schema_guard` test (`docs/api_schema.json` kept in sync).
- Property tests for stability (e.g., random simple joins).
- Fuzz target (`cargo fuzz run analyze`) to guard against parser/analyzer panics.
- Use regression tests to:
  - Diff new run outputs against golden snapshots.
  - Flag unexpected behavior changes.

When deliberate behavioral changes are introduced:

- Golden files should be updated with accompanying notes.

## 5. Performance Benchmarks

Define benchmark scripts that:

- Run the engine on:
  - A large single query (e.g., multi-hundred-line dbt model).
  - A multi-statement script.
- Measure:
  - Time for parsing.
  - Time for lineage computation.
- Targets:
  - Reasonable thresholds for interactive use (e.g., under a few hundred ms for typical queries on a modern laptop/browser).

These benchmarks do not need to be perfect scientific measures but should catch regressions.

## 6. QA Checklist for Releases

Before each release:

1. All test suites pass:
   - Rust unit/integration tests.
   - JS/TS tests.
   - UI snapshot tests.
   - E2E demo tests.
2. Benchmark suite shows no major regression vs previous release.
3. Basic manual sanity checks:
   - Run the demo app on all supported dialects for a small set of sample queries.
   - Confirm that:
     - Graphs look sensible.
     - Issues are surfaced correctly.
     - No major UI glitches in the latest supported browsers.
