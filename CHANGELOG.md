# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### CLI (flowscope-cli)
- **Serve mode**: Run FlowScope as a local HTTP server with embedded web UI
  - `--serve` flag to start HTTP server
  - `--port <PORT>` to specify server port (default: 3000)
  - `--watch <DIR>` for directories to watch for SQL files (repeatable)
  - `--open` to auto-open browser on startup
  - REST API endpoints for analysis, completion, file listing, and export
  - File watcher with 100ms debounce for automatic reload on changes
  - Assets embedded at compile time via rust-embed for single-binary deployment

#### Web App (app/)
- Backend adapter pattern for REST/WASM detection
- Read-only mode for files loaded from backend
- Schema display from database introspection in serve mode

## [0.3.0] - 2026-01-22

### Added

#### Core Engine (flowscope-core)
- **Jinja/dbt templating support**: MiniJinja-based preprocessing for dbt projects
  - Built-in dbt macros: `ref()`, `source()`, `config()`, `var()`, `is_incremental()`
  - RelationEmulator for dbt Relation object attribute access (`.schema`, `.identifier`)
  - `this` global variable and `execute` flag for templates
  - Custom macro passthrough stubs for graceful handling
- **COPY statement lineage**: Track source/target tables in COPY/COPY INTO (PostgreSQL, Snowflake)
- **ALTER TABLE RENAME lineage**: Track table renames as dataflow edges
- **UNLOAD statement lineage**: Track source tables from Redshift UNLOAD statements
- **Lateral column alias support**: Resolve aliases in same SELECT list (BigQuery, Snowflake, DuckDB, etc.)
- **Backward column inference**: Infer columns for SELECT * without schema from downstream usage
- Type inference for SQL expressions with comprehensive type checking
- New `TYPE_MISMATCH` warning code for detecting incompatible type comparisons and operations
- Schema-aware column type lookup - column references now resolve types from provided schema metadata
- CTE column type propagation to outer queries
- Dialect-aware type compatibility rules (e.g., Boolean/Integer comparison allowed in MySQL but not PostgreSQL)
- NULL comparison anti-pattern detection (`= NULL` warns to use `IS NULL` instead)

#### Completion API
- Smart function completions with signature metadata (params, return types, categories)
- Context-aware scoring: boost aggregates in GROUP BY, window functions in OVER clauses
- Lateral column alias extraction with proper scope isolation
- Type-aware column scoring in comparison contexts
- Dialect-aware keyword parsing using sqlparser tokenizer
- CASE expression type inference from THEN/ELSE branches

#### CLI (flowscope-cli)
- `--template jinja|dbt` flag for templated SQL preprocessing
- `--template-var KEY=VALUE` for template variable injection
- `--metadata-url` for live database schema introspection (PostgreSQL, MySQL, SQLite)
- `--metadata-schema` for schema filtering during introspection

#### React Package (@pondpilot/flowscope-react)
- Web workers for graph/matrix/layout computations (improved UI responsiveness)
- LayoutProgressIndicator component for visual layout feedback
- Debug flags (GRAPH_DEBUG, LAYOUT_DEBUG) for performance diagnostics

#### Web App (app/)
- Template mode selector for dbt/Jinja SQL preprocessing in the toolbar
- Issue-to-editor navigation: click issues to jump to source location
- Issues tab filtering: filter by severity, error code, and file
- Stats popover: complexity dots trigger dropdown with table/column/join counts
- Clear analysis cache option in project menu
- Bundled "dbt Jaffle Shop" demo project showcasing ref/source/config/var

### Changed

#### Core Engine (flowscope-core)
- Unified type system: `CanonicalType` replaces internal `SqlType` enum with broader coverage (Time, Binary, Json, Array)
- `OutputColumn.data_type` now populated with inferred types for SELECT expressions
- Type checking now accepts dialect parameter for dialect-specific rules

### Fixed

#### Core Engine (flowscope-core)
- dbt model cross-statement linking with proper case normalization for Snowflake
- DDL-seeded schema preservation (schemas no longer overwritten by later queries)
- UTF-8 safety in `should_show_for_cursor` when cursor offset is mid-character

### Known Limitations

#### Type Inference
- **Schema-unaware type checking**: TYPE_MISMATCH warnings only detect mismatches between literals, CASTs, and known function return types. Column type mismatches (e.g., `WHERE users.id = users.email` where `id` is INTEGER and `email` is TEXT) are not detected because expression type inference does not yet resolve column types from schema metadata.
- **No expression spans**: TYPE_MISMATCH warnings include the statement index but not precise source spans for editor integration. This is because sqlparser's expression AST nodes don't include span information by default.

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
