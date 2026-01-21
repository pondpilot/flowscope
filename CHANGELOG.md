# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### Core Engine (flowscope-core)
- Type inference for SQL expressions with comprehensive type checking
- New `TYPE_MISMATCH` warning code for detecting incompatible type comparisons and operations
- Schema-aware column type lookup - column references now resolve types from provided schema metadata
- CTE column type propagation to outer queries

### Changed

#### Core Engine (flowscope-core)
- Unified type system: `CanonicalType` replaces internal `SqlType` enum with broader coverage (Time, Binary, Json, Array)
- `OutputColumn.data_type` now populated with inferred types for SELECT expressions

## [0.2.0] - 2026-01-18

### Highlights
- First public release of the FlowScope Rust + WASM + TypeScript stack
- Multi-dialect SQL parsing with table/column lineage, schema validation, and editor-friendly spans
- Export tooling for Mermaid, JSON, HTML, CSV bundles, and XLSX across Rust and WASM
- React components + CLI for lineage visualization and data export workflows
- CTE and derived table definitions now include spans for editor navigation
- Export downloads in React normalize byte buffers for reliable Blob creation

### Changed

#### WASM Module (flowscope-wasm)
- **Breaking**: Changed error code from `REQUEST_PARSE_ERROR` to `INVALID_REQUEST` for JSON parse/validation errors in `analyze_sql_json()`. Consumers matching on the old error code should update their error handling.

### Fixed

#### Core Engine (flowscope-core)
- Fixed potential panic in `extract_qualifier` when cursor offset lands on invalid UTF-8 boundary
- Fixed early-return bug in completion logic that incorrectly suppressed completions when schema metadata lacked column info but query context (CTEs/subqueries) had valid columns
- Fixed potential integer overflow in completion item scoring by using saturating arithmetic
- Added spans for CTE and derived table definition nodes to support editor navigation

#### Exporter (flowscope-export)
- Bundle reserved keyword list inside the crate for publish-time builds

#### React Package (@pondpilot/flowscope-react)
- Fixed ZIP/XLSX downloads by normalizing export byte buffers for Blob creation

### Improved

#### Core Engine (flowscope-core)
- Added named constants for completion scoring values to improve code maintainability
- Added `Debug` derive to `QualifierResolution` for easier debugging
- Added comprehensive unit tests for string helper functions (`extract_last_identifier`, `extract_qualifier`) and qualifier resolution logic

## [0.1.0] - 2025-11-21

### Added

#### Core Engine (flowscope-core)
- SQL parsing with sqlparser-rs for multiple dialects
- Table-level lineage extraction from SELECT, JOIN, CTE, INSERT, CTAS, UNION
- Cross-statement lineage tracking via GlobalLineage
- Schema metadata support for table validation
- UNKNOWN_TABLE warning when tables not in provided schema
- Structured issue reporting with severity levels (error, warning, info)
- Graceful degradation - partial results on parse failures

#### WASM Module (flowscope-wasm)
- WebAssembly bindings for browser usage
- JSON-in/JSON-out API via `analyze_sql_json()`
- Legacy `analyze_sql()` function for backwards compatibility
- Version info via `get_version()`

#### TypeScript Package (@pondpilot/flowscope-core)
- WASM loader with `initWasm()` function
- Type-safe `analyzeSql()` function
- Complete TypeScript type definitions
- JSDoc documentation on all public types

#### Supported Dialects
- Generic SQL
- PostgreSQL
- Snowflake
- BigQuery

#### Supported Statements
- SELECT (with aliases, subqueries)
- JOIN (INNER, LEFT, RIGHT, FULL, CROSS)
- WITH / CTE (multiple, nested references)
- INSERT INTO ... SELECT
- CREATE TABLE AS SELECT
- UNION / UNION ALL / INTERSECT / EXCEPT

#### Documentation
- User guides (quickstart, schema metadata, error handling)
- Dialect coverage matrix
- API documentation (rustdoc, TypeDoc)
- Test fixtures for all dialects

#### Testing
- 45+ Rust unit tests
- 11 TypeScript type tests
- Test fixtures per dialect (generic, postgres, snowflake, bigquery)
- CI/CD with GitHub Actions

### Known Limitations
- Column-level lineage not yet implemented (planned for v0.2.0)
- Recursive CTEs generate warning, lineage may be incomplete
- UPDATE, DELETE, MERGE statements not yet supported
