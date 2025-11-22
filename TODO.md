# FlowScope Implementation TODO

**Version:** 0.2.0
**Last Updated:** 2025-11-21
**Status:** Phase 2 Complete, Ready for Phase 3

This document provides a detailed, phase-by-phase implementation checklist for FlowScope. Each task is designed to be actionable and testable.

---

## Phase 0: Spike / Feasibility ✅ COMPLETE

**Goal:** Prove the tech stack (Rust + sqlparser-rs + WASM) works end-to-end

### 0.1 Project Setup

- [x] **Create repository structure**
  - [x] Initialize Git repository
  - [x] Create directory structure per `docs/workspace-structure.md`
  - [x] Add `.gitignore` for Rust, Node, and build artifacts
  - [x] Set up `.github/` directory structure

- [x] **Initialize Rust workspace**
  - [x] Create root `Cargo.toml` with workspace configuration
  - [x] Create `crates/flowscope-core/` with basic structure
  - [x] Create `crates/flowscope-wasm/` with basic structure
  - [x] Add `sqlparser` dependency to flowscope-core
  - [x] Add `wasm-bindgen`, `serde`, `serde_json` to flowscope-wasm
  - [x] Verify `cargo build` succeeds

- [x] **Initialize NPM workspace**
  - [x] Create root `package.json` with workspace configuration
  - [x] Set up `packages/core/` with package.json
  - [x] Set up `packages/react/` with package.json (minimal for now)
  - [x] Set up `app/` with package.json
  - [x] Create `tsconfig.base.json`
  - [x] Verify `yarn install` succeeds

- [x] **Add essential config files**
  - [x] `.prettierrc` (Prettier config)
  - [x] `.eslintrc.js` (ESLint config)
  - [x] `LICENSE` (Apache 2.0)
  - [x] Root `README.md` (project overview)
  - [x] `CONTRIBUTING.md` (developer setup guide)
  - [x] `SECURITY.md` (security policy)

### 0.2 Minimal Rust Parser

- [x] **Implement basic SQL parsing**
  - [x] Create `parser` module in flowscope-core
  - [x] Implement `parse_sql()` function wrapping sqlparser-rs
  - [x] Support parsing simple SELECT statements
  - [x] Return Result<Vec<Statement>, ParseError>
  - [x] Write unit test for valid SELECT
  - [x] Write unit test for invalid SQL

- [x] **Implement trivial lineage extraction**
  - [x] Create `lineage` module
  - [x] Implement `extract_tables()` function
  - [x] Extract table names from FROM clauses only
  - [x] Return Vec<String> of table names
  - [x] Write unit test with SELECT from single table
  - [x] Write unit test with SELECT from multiple tables (JOIN)

- [x] **Add basic types**
  - [x] Create `types.rs` with LineageResult struct
  - [x] Implement basic serialization with serde
  - [x] Add test to verify JSON serialization works

### 0.3 WASM Bridge

- [x] **Create WASM wrapper**
  - [x] Implement `analyze_sql()` function in flowscope-wasm
  - [x] Accept SQL string input
  - [x] Call flowscope-core functions
  - [x] Return JSON string output
  - [x] Add wasm-bindgen annotations
  - [x] Handle errors without panicking

- [x] **Build WASM module**
  - [x] Install wasm-pack
  - [x] Create `wasm-pack build` command
  - [x] Verify WASM binary is generated (1.68 MB)
  - [x] Verify JS glue code is generated
  - [x] Check output size (1.68 MB < 2MB target ✅)

### 0.4 Minimal Web Demo

- [x] **Set up basic HTML page**
  - [x] Create `app/index.html`
  - [x] Add textarea for SQL input
  - [x] Add button to trigger analysis
  - [x] Add div to display results

- [x] **Load WASM in browser**
  - [x] Write JS to load WASM module
  - [x] Initialize WASM
  - [x] Handle loading errors gracefully
  - [x] Add loading indicator

- [x] **Wire up analysis**
  - [x] Get SQL from textarea on button click
  - [x] Call WASM analyze_sql function
  - [x] Parse JSON result
  - [x] Display table list in results div
  - [x] Display any errors

- [x] **Manual testing**
  - [x] Test with `SELECT * FROM users`
  - [x] Test with `SELECT * FROM users JOIN orders ON users.id = orders.user_id`
  - [x] Test with invalid SQL (should show error)
  - [ ] Verify in Chrome, Firefox, Safari (tested in Node.js)

### 0.5 Documentation

- [x] **Document spike results**
  - [x] Record WASM bundle size (1.68 MB)
  - [x] Record load time in browser (~500ms estimated)
  - [x] Note any sqlparser-rs limitations found (none)
  - [x] Update design docs if assumptions changed (no changes needed)

**Phase 0 Complete When:**
- ✅ Can parse simple SELECT and extract table names
- ✅ WASM module loads and runs in browser
- ✅ Basic demo app shows table lineage for simple queries

---

## Phase 1: Table-Level Lineage MVP ✅ COMPLETE

**Goal:** Production-ready core engine for table-level lineage

### 1.1 Core Engine - Foundation

- [x] **Enhance type system**
  - [x] Define complete `AnalyzeRequest` struct
    - [x] sql: String
    - [x] dialect: Dialect enum
    - [x] options: Option<AnalysisOptions>
    - [x] schema: Option<SchemaMetadata>
  - [x] Define complete `AnalyzeResult` struct per `api-types.md`
  - [x] Add StatementLineage struct
  - [x] Add GlobalLineage struct
  - [x] Add Issue struct with severity, code, message, span
  - [x] Add Summary struct
  - [x] Implement Serialize/Deserialize for all types
  - [x] Generate JSON Schema using schemars

- [x] **Implement dialect support**
  - [x] Create Dialect enum (Generic, Postgres, Snowflake, BigQuery)
  - [x] Map dialects to sqlparser-rs dialects
  - [x] Implement dialect selection logic
  - [x] Add fallback to Generic with warning
  - [x] Test each dialect can parse basic queries

- [x] **Build schema metadata layer**
  - [x] Define SchemaMetadata struct per spec
  - [x] Implement schema normalization (catalog.schema.table)
  - [x] Implement case-sensitivity handling per dialect
  - [ ] Implement search path resolution
  - [ ] Add schema validation
  - [x] Write tests for qualified name resolution

### 1.2 Core Engine - Lineage Computation

- [x] **Implement SELECT analysis**
  - [x] Extract source tables from FROM clause
  - [x] Handle table aliases
  - [x] Handle implicit SELECT targets (no INTO/CREATE TABLE)
  - [x] Create table nodes
  - [x] Create edges (table → statement)
  - [x] Write test: simple SELECT
  - [x] Write test: SELECT with alias

- [x] **Implement JOIN analysis**
  - [x] Detect INNER JOIN
  - [x] Detect LEFT/RIGHT/FULL JOIN
  - [x] Detect CROSS JOIN
  - [x] Extract join conditions
  - [x] Create edges for join relationships
  - [x] Write test for each join type

- [x] **Implement CTE (WITH) analysis**
  - [x] Detect CTE definitions
  - [x] Parse CTE bodies recursively
  - [x] Handle multiple CTEs
  - [x] Handle CTEs referencing other CTEs
  - [x] Create CTE nodes distinct from table nodes
  - [x] Write test: single CTE
  - [x] Write test: multiple CTEs
  - [x] Write test: nested CTE references
  - [x] Detect recursive CTEs and emit UNSUPPORTED_RECURSIVE_CTE

- [x] **Implement INSERT INTO ... SELECT**
  - [x] Parse INSERT target table
  - [x] Parse SELECT source
  - [x] Create edge: source → target
  - [x] Handle column list on INSERT
  - [x] Write test: INSERT without column list
  - [x] Write test: INSERT with column list

- [x] **Implement CREATE TABLE AS SELECT (CTAS)**
  - [x] Parse CREATE TABLE target
  - [x] Parse SELECT source
  - [x] Create table node for new table
  - [x] Create edge: source → target
  - [x] Write test: basic CTAS

- [x] **Implement UNION/UNION ALL**
  - [x] Detect set operations
  - [x] Analyze each branch separately
  - [x] Merge results
  - [x] Create edges from sources to union result
  - [x] Write test: simple UNION
  - [x] Write test: UNION ALL
  - [x] Write test: multiple UNIONs

- [x] **Implement subquery analysis**
  - [x] Detect subqueries in FROM clause
  - [x] Analyze subquery as separate statement
  - [x] Connect subquery lineage to parent
  - [x] Write test: subquery in FROM
  - [x] Write test: nested subqueries

### 1.3 Core Engine - Cross-Statement Assembly

- [x] **Implement global graph builder**
  - [x] Create GlobalLineage structure
  - [x] Deduplicate nodes across statements using canonical names
  - [x] Collect all nodes into global node list
  - [x] Collect all edges into global edge list
  - [x] Add StatementRef to each node
  - [x] Write test: two independent statements
  - [x] Write test: statement 2 reads from statement 1 output

- [x] **Implement cross-statement edge detection**
  - [x] Track tables/CTEs produced by each statement
  - [x] Detect when later statement reads earlier output
  - [x] Create cross_statement edges
  - [x] Add producer/consumer statement references
  - [x] Write test: INSERT then SELECT from same table
  - [x] Write test: CTAS then INSERT into created table

- [x] **Handle unresolved references**
  - [ ] Create placeholder nodes for missing tables
  - [ ] Emit UNRESOLVED_REFERENCE issue
  - [ ] Link placeholders to global graph
  - [x] Write test: SELECT from non-existent table

### 1.4 Core Engine - Error Handling

- [x] **Implement issue collection**
  - [x] Create IssueCollector
  - [x] Emit PARSE_ERROR for sqlparser failures
  - [ ] Emit DIALECT_FALLBACK when needed
  - [x] Emit UNSUPPORTED_SYNTAX for unhandled constructs
  - [x] Emit UNKNOWN_TABLE when table not in schema
  - [ ] Capture spans from sqlparser when available
  - [x] Associate issues with statement index

- [x] **Implement summary generation**
  - [x] Count statements analyzed
  - [x] Count unique tables discovered
  - [x] Count issues by severity
  - [x] Set has_errors flag
  - [x] Write test: verify summary correctness

- [x] **Implement graceful degradation**
  - [x] Continue analysis if one statement fails
  - [x] Provide partial lineage when possible
  - [x] Never panic/abort at WASM boundary
  - [x] Write test: multi-statement with one failure

### 1.5 WASM Layer

- [x] **Finalize WASM bridge**
  - [x] Implement complete analyze_sql function
  - [x] Serialize AnalyzeRequest from JSON
  - [x] Deserialize AnalyzeResult to JSON
  - [x] Handle JSON parse errors gracefully
  - [ ] Add error logging (to console in debug builds)
  - [x] Optimize for size (use wasm-opt)

- [x] **Test WASM boundary**
  - [ ] Write integration test calling from Node.js
  - [x] Verify round-trip JSON serialization
  - [ ] Test with large SQL (>10KB)
  - [x] Test with malformed JSON input
  - [ ] Measure performance (baseline benchmarks)

### 1.6 TypeScript Wrapper (@pondpilot/flowscope-core)

- [x] **Set up package structure**
  - [x] Create src/ directory
  - [x] Set up TypeScript configuration
  - [x] Configure build (use tsc for bundling)
  - [x] Configure test framework (Vitest)

- [x] **Implement type definitions**
  - [x] Copy/generate types from Rust (schemars → TypeScript)
  - [x] Export AnalyzeRequest interface
  - [x] Export AnalyzeResult interface
  - [x] Export all nested types (Node, Edge, Issue, etc.)
  - [x] Add TSDoc/JSDoc comments

- [x] **Implement WASM loader**
  - [x] Create wasm-loader.ts
  - [x] Implement initWasm() function
  - [x] Support custom WASM URL
  - [x] Handle fetch() errors
  - [x] Handle WebAssembly.instantiate() errors
  - [x] Make idempotent (safe to call multiple times)
  - [ ] Write test: successful init
  - [ ] Write test: init with missing WASM file

- [x] **Implement analyzeSql() function**
  - [x] Create analyzer.ts
  - [x] Implement typed analyzeSql(request: AnalyzeRequest)
  - [x] Call initWasm() lazily if needed
  - [x] Validate request object
  - [x] Call WASM analyze_sql function
  - [x] Parse JSON result
  - [x] Type-check result
  - [x] Return typed AnalyzeResult
  - [ ] Write unit test: simple query
  - [ ] Write unit test: invalid SQL
  - [ ] Write unit test: with schema metadata

- [x] **Handle errors properly**
  - [x] Reject Promise for technical errors (WASM load failure)
  - [x] Return AnalyzeResult with issues for analysis errors
  - [x] Add clear error messages
  - [ ] Write test: WASM not initialized
  - [ ] Write test: malformed request

- [x] **Create package.json**
  - [x] Set name: @pondpilot/flowscope-core
  - [x] Set version: 0.1.0
  - [x] Add exports field (main, types, worker)
  - [x] Add dependencies
  - [x] Add build scripts
  - [x] Add test scripts
  - [x] Add files field (include dist/ and wasm/)

### 1.7 Example Web Demo (Basic)

- [x] **Set up Vite project**
  - [x] Create Vite config
  - [x] Set up React 18
  - [x] Configure TypeScript
  - [x] Add @pondpilot/flowscope-core as dependency

- [x] **Build basic UI**
  - [x] Create main App component
  - [x] Add SQL textarea (basic)
  - [x] Add dialect selector (dropdown)
  - [x] Add "Analyze" button
  - [x] Add loading spinner
  - [x] Show JSON result in pre tag
  - [x] Style with CSS

- [x] **Wire up analysis**
  - [x] Import analyzeSql from @pondpilot/flowscope-core
  - [x] Call analyzeSql on button click
  - [x] Handle loading state
  - [x] Display results
  - [x] Display errors/issues
  - [x] Add sample SQL examples (dropdown)

- [x] **Manual QA**
  - [x] Test all 4 dialects
  - [x] Test with sample queries
  - [x] Test error cases
  - [ ] Verify in Chrome, Firefox, Safari (deferred - requires browser testing)
  - [x] Check console for errors

### 1.8 Testing Infrastructure

- [x] **Create Rust test fixtures**
  - [x] Set up `crates/flowscope-core/tests/fixtures/`
  - [x] Add SQL files for each dialect
    - [x] postgres/01_basic_select.sql through 05_create_table_as.sql
    - [x] snowflake/01_basic_select.sql through 05_create_table_as.sql
    - [x] bigquery/01_basic_select.sql through 05_create_table_as.sql
    - [x] generic/01_basic_select.sql through 08_multi_statement.sql
  - [x] Add schema JSON files (schemas/*.json)
  - [ ] Create golden output files (deferred)
  - [x] Write fixture loader utility (test_utils.rs)

- [x] **Write Rust unit tests**
  - [x] Test for each statement type
  - [x] Test for each dialect
  - [x] Test edge cases (empty SQL, comments only, etc.)
  - [x] Test error paths
  - [ ] Aim for >80% coverage of core logic

- [x] **Write Rust integration tests**
  - [x] Test full analyze_sql pipeline
  - [x] Test with fixtures
  - [ ] Compare against golden outputs (deferred)
  - [x] Test cross-statement lineage

- [x] **Write TypeScript unit tests**
  - [x] Test WASM loader (basic)
  - [x] Test analyzeSql function (via types)
  - [x] Test type conversions (11 tests)
  - [x] Test error handling (via types)
  - [ ] Mock WASM module where appropriate (deferred)

- [x] **Set up CI/CD**
  - [x] Create .github/workflows/ci.yml
  - [x] Add Rust build + test job
  - [x] Add WASM build job
  - [x] Add TypeScript lint + test job
  - [x] Add artifact caching
  - [x] Run on every push and PR

### 1.9 Documentation

- [x] **Create API documentation**
  - [x] Generate rustdoc for flowscope-core
  - [x] Generate rustdoc for flowscope-wasm
  - [x] Generate TypeDoc for @pondpilot/flowscope-core
  - [ ] Host docs on GitHub Pages (deferred to Phase 5)

- [x] **Write user guides**
  - [x] Quickstart: TypeScript usage (docs/guides/quickstart.md)
  - [x] Guide: Schema metadata format (docs/guides/schema-metadata.md)
  - [x] Guide: Error handling (docs/guides/error-handling.md)
  - [x] Guide: Dialect support matrix (docs/dialect-coverage.md)
  - [x] Add code examples for each

- [x] **Create dialect coverage matrix**
  - [x] Document supported SQL features per dialect
  - [x] Mark as Supported / Partial / Unsupported
  - [x] Link to test fixtures
  - [x] Add to docs/dialect-coverage.md

- [x] **Update README.md**
  - [x] Add installation instructions
  - [x] Add usage example
  - [x] Add link to docs
  - [x] Add badges (build status, version, license)

### 1.10 Release Preparation

- [x] **Code cleanup**
  - [x] Run cargo fmt on all Rust code
  - [x] Run cargo clippy and fix warnings
  - [x] Run prettier on all TypeScript code
  - [x] Run ESLint and fix warnings
  - [x] Remove debug code and console.logs

- [x] **Pre-release checklist**
  - [x] All tests passing (45 Rust + 11 TypeScript)
  - [x] No compiler warnings
  - [x] Documentation complete
  - [x] CHANGELOG.md created
  - [x] Version numbers consistent (0.1.0)
  - [x] LICENSE file present

- [ ] **Publish packages** (moved to Phase 5)
  - [ ] Publish flowscope-core to crates.io
  - [ ] Publish flowscope-wasm to crates.io
  - [ ] Build and test @pondpilot/flowscope-core locally
  - [ ] Publish @pondpilot/flowscope-core to npm
  - [ ] Create Git tag: v0.1.0
  - [ ] Create GitHub release with notes

### 1.11 Code Review Improvements ✅ COMPLETE

- [x] **Reduce code duplication**
  - [x] Add `AnalyzeResult::from_error()` helper method
  - [x] Update WASM lib to use the helper

- [x] **Improve module organization**
  - [x] Change wildcard exports (`pub use types::*`) to explicit exports
  - [x] Split `types.rs` into submodules:
    - [x] `types/mod.rs` - re-exports
    - [x] `types/request.rs` - request types
    - [x] `types/response.rs` - response types
    - [x] `types/common.rs` - shared types (Issue, Span, Summary)
    - [x] `types/legacy.rs` - backwards compatibility types

- [x] **Add documentation**
  - [x] Add module-level docs to types module
  - [x] Add rustdoc comments to all public Rust types
  - [x] Add JSDoc comments to all TypeScript types

- [x] **Evaluate error handling**
  - [x] Review thiserror adoption (not needed - error model is simple)

**Phase 1 Complete When:**
- ✅ All core lineage features work for table-level analysis
- ✅ Global cross-statement graph is generated correctly
- ✅ All 4 dialects parse and analyze successfully
- ✅ 45 Rust tests + 11 TypeScript tests passing
- ✅ Documentation complete (guides, API docs, dialect matrix)
- ✅ Demo app builds and runs
- Note: Package publishing moved to Phase 5

---

## Phase 2: Column-Level Lineage & Schema Support ✅ COMPLETE

**Goal:** Add precise column-level lineage tracking

### 2.1 Core Engine - Column Tracking

- [x] **Extend AST analysis for columns**
  - [x] Extract column references from SELECT list
  - [x] Extract column references from WHERE clause
  - [x] Extract column references from JOIN conditions
  - [x] Extract column references from GROUP BY / HAVING
  - [x] Handle column aliases
  - [x] Handle qualified column names (table.column)

- [x] **Implement column node creation**
  - [x] Create column nodes for each referenced column
  - [x] Link columns to parent tables via ownership edges
  - [x] Handle computed columns (expressions)
  - [x] Store expression text in column metadata
  - [x] Write test: SELECT with explicit columns
  - [x] Write test: SELECT with expressions

- [x] **Implement column lineage edges**
  - [x] Create data_flow edges: input column → output column
  - [x] Create derivation edges: multiple inputs → computed output
  - [x] Track expression transformations
  - [x] Write test: simple column passthrough
  - [x] Write test: computed column (SUM, CONCAT, etc.)

- [x] **Handle SELECT * expansion**
  - [x] When schema provided: expand * to explicit columns
  - [x] When schema missing: create placeholder or emit warning
  - [x] Handle table.* syntax
  - [x] Write test: SELECT * with schema
  - [x] Write test: SELECT * without schema (approximate)

- [x] **Implement JOIN column lineage**
  - [x] Track which table each output column comes from
  - [x] Handle ambiguous column names
  - [x] Create edges through join conditions
  - [x] Write test: columns from left table
  - [x] Write test: columns from right table
  - [x] Write test: computed from both sides

### 2.2 Core Engine - Schema Integration

- [x] **Enhance schema metadata**
  - [x] Add column data types (optional)
  - [ ] Add primary key hints (optional) - deferred
  - [x] Validate schema structure on input
  - [x] Emit warnings for malformed schema

- [x] **Implement schema-based validation**
  - [x] Check if referenced columns exist in schema
  - [x] Emit UNKNOWN_COLUMN issue when not found
  - [x] Continue with best-effort lineage
  - [x] Write test: valid column references
  - [x] Write test: invalid column reference

- [ ] **Improve search path resolution** - deferred
  - [ ] Use search_path for unqualified table names
  - [ ] Try each path entry in order
  - [x] Use defaultCatalog and defaultSchema as fallbacks
  - [x] Write test: qualified name resolution
  - [ ] Write test: search path resolution

### 2.3 Analysis Options

- [x] **Add enableColumnLineage option**
  - [x] Default to true
  - [x] Allow disabling for performance
  - [x] Skip column analysis when disabled
  - [x] Write test: with option enabled
  - [x] Write test: with option disabled

### 2.4 Testing

- [x] **Add column lineage test fixtures**
  - [x] Create SQL samples with explicit column references
  - [x] Create SQL samples with SELECT *
  - [x] Create SQL samples with computed columns
  - [x] Create corresponding schema JSON files
  - [ ] Create golden outputs - deferred

- [x] **Write comprehensive tests**
  - [x] Test column passthrough
  - [x] Test column expressions (math, string ops, functions)
  - [ ] Test window functions (as expressions) - deferred
  - [x] Test GROUP BY / aggregations
  - [x] Test CASE expressions

- [x] **Update integration tests**
  - [x] Verify column-level edges in output
  - [x] Verify expression metadata
  - [x] Verify schema validation

### 2.5 TypeScript & Demo Updates

- [x] **Update @pondpilot/flowscope-core**
  - [x] Add enableColumnLineage to AnalysisOptions type
  - [x] Update API documentation
  - [x] Add examples showing column lineage

- [x] **Update demo app**
  - [x] Display column nodes in JSON view
  - [x] Show expression details
  - [ ] Add checkbox to toggle column lineage - deferred
  - [x] Test with schema metadata input

### 2.6 Documentation

- [x] **Document column lineage features**
  - [x] Write guide: How column lineage works
  - [x] Document expression tracking
  - [x] Document limitations (window functions, etc.)
  - [x] Add examples

- [ ] **Update dialect coverage matrix** - deferred
  - [ ] Add column-level support status per dialect
  - [ ] Note any dialect-specific differences

### 2.7 Release

- [ ] **Version bump to 0.2.0** - deferred to publishing
  - [ ] Update Cargo.toml versions
  - [ ] Update package.json versions
  - [ ] Update CHANGELOG.md

- [ ] **Publish** - moved to Phase 5
  - [ ] Publish Rust crates
  - [ ] Publish npm package
  - [ ] Create Git tag: v0.2.0
  - [ ] Create GitHub release

**Phase 2 Complete When:**
- ✅ Column-level lineage works for explicit columns
- ✅ SELECT * expansion works with schema
- ✅ Expressions tracked and visible in output
- ✅ Tests pass with good coverage (67 Rust + 13 TypeScript)
- Note: Package publishing moved to Phase 5

---

## Phase 3: React Viewer & Full Demo (IN PROGRESS)

**Goal:** Build polished React UI components for lineage visualization

### 3.1 Package Setup (@pondpilot/flowscope-react)

- [x] **Initialize package**
  - [x] Create packages/react/ structure
  - [x] Set up package.json
  - [x] Configure TypeScript
  - [ ] Configure Tailwind CSS (using inline styles instead)
  - [x] Add React 18 and ReactFlow as dependencies
  - [x] Add @pondpilot/flowscope-core as peer dependency

- [x] **Set up build**
  - [x] Configure bundler (tsc)
  - [x] Set up CSS processing
  - [x] Generate types (.d.ts)
  - [x] Test build output

- [ ] **Set up testing**
  - [ ] Configure Jest for React
  - [ ] Add @testing-library/react
  - [ ] Create test utilities
  - [ ] Add snapshot testing capability

### 3.2 Core Components

- [x] **GraphView component**
  - [x] Set up ReactFlow
  - [x] Convert lineage nodes to ReactFlow nodes
  - [x] Convert lineage edges to ReactFlow edges
  - [x] Implement table node renderer (custom)
  - [x] Implement column node renderer (custom)
  - [x] Add zoom/pan controls
  - [ ] Add layout algorithm (Dagre or ELK)
  - [x] Handle node selection
  - [x] Emit onNodeSelect event
  - [x] Style nodes (tables, CTEs, columns distinct)
  - [ ] Write tests

- [x] **ColumnPanel component**
  - [x] Display selected column details
  - [ ] Show upstream columns (sources)
  - [ ] Show downstream columns (consumers)
  - [x] Show expression text
  - [ ] Show data flow path (A → B → C)
  - [x] Handle no selection state
  - [x] Style with inline styles
  - [ ] Write tests

- [x] **SqlView component**
  - [x] Integrate CodeMirror 6
  - [x] Display SQL with syntax highlighting
  - [x] Highlight selected node spans
  - [ ] Highlight issue spans (errors in red, warnings in yellow)
  - [ ] Handle click on highlighted spans (select node)
  - [x] Add line numbers
  - [ ] Make read-only
  - [x] Style with inline styles
  - [ ] Write tests

- [x] **IssuesPanel component**
  - [x] Display list of issues
  - [x] Group by severity (errors, warnings, info)
  - [x] Show issue count badges
  - [x] Format issue messages
  - [x] Make issue clickable → highlight in SqlView
  - [x] Style with appropriate colors (red, yellow, blue)
  - [ ] Write tests

- [x] **StatementSelector component**
  - [x] Display when multiple statements exist
  - [x] Show statement index and type
  - [x] Highlight selected statement
  - [x] Emit onStatementSelect event
  - [x] Style as tab bar
  - [ ] Write tests

### 3.3 Composite Components

- [x] **LineageExplorer component**
  - [x] Compose GraphView, SqlView, ColumnPanel, IssuesPanel
  - [x] Accept AnalyzeResult as prop
  - [x] Accept SQL string as prop
  - [x] Add StatementSelector when needed
  - [x] Wire up component interactions (selection sync)
  - [x] Add responsive layout (grid or flex)
  - [ ] Support theme prop (light/dark)
  - [x] Export as main public component
  - [ ] Write integration tests

### 3.4 Hooks & Utilities

- [x] **useLineageExplorer hook**
  - [x] Manage selected statement
  - [x] Manage selected node
  - [x] Sync selection across sub-components
  - [x] Provide helper methods (selectNode, selectStatement)
  - [ ] Write tests

- [x] **Graph layout utilities**
  - [x] Implement dagre-based layout (LR direction)
  - [ ] Implement table+column layout mode
  - [x] Position nodes to minimize edge crossings
  - [x] Add padding and spacing constants
  - [ ] Write tests

- [x] **Span highlighting utilities**
  - [x] Map span offsets to CodeMirror decorations
  - [ ] Handle overlapping spans
  - [ ] Support multiple highlight colors
  - [ ] Write tests

### 3.5 Styling & Theming

- [x] **Create theme**
  - [x] Define color palette (primary, secondary, accent)
  - [x] Define spacing scale
  - [x] Define typography
  - [x] Support dark mode

- [x] **Style all components**
  - [x] Use inline styles for library components
  - [x] Keep consistent spacing
  - [x] Ensure good contrast (accessibility)
  - [x] Add hover states
  - [ ] Add focus states (keyboard nav)

- [x] **Support customization**
  - [x] Accept className prop on all components
  - [ ] Accept theme prop on LineageExplorer
  - [ ] Document customization options

### 3.6 Package Documentation

- [ ] **Add component documentation**
  - [ ] TSDoc comments on all public components
  - [ ] Document all props
  - [ ] Add usage examples



### 3.7 Example Demo App (Enhanced)

- [x] **Update demo app to use React components**
  - [x] Replace JSON view with LineageExplorer
  - [ ] Keep option to show raw JSON (collapsible)
  - [ ] Add schema input (JSON textarea or file upload)
  - [x] Add example query library (dropdown)
  - [x] Add dialect selector
  - [x] Add "Analyze" button with loading state

- [x] **Add example queries**
  - [x] Simple SELECT
  - [x] JOIN query
  - [x] CTE query
  - [x] INSERT INTO SELECT
  - [ ] Complex dbt-style model

- [x] **Polish UI**
  - [x] Add header with logo/title
  - [x] Add footer with links (GitHub, docs)
  - [ ] Responsive layout (mobile-friendly)
  - [ ] Add error boundaries
  - [ ] Add help tooltips
  - [x] Add dark mode toggle
  - [x] Migrate to shadcn-ui components (Button, Select)

### 3.8 Testing

- [ ] **Unit tests for all components**
  - [ ] GraphView
  - [ ] ColumnPanel
  - [ ] SqlView
  - [ ] IssuesPanel
  - [ ] StatementSelector
  - [ ] LineageExplorer
  - [ ] Aim for >80% coverage

- [ ] **Integration tests**
  - [ ] Test full LineageExplorer with real AnalyzeResult
  - [ ] Test component interactions (selection sync)
  - [ ] Test error states

- [ ] **Visual regression tests (optional)**
  - [ ] Set up Percy or Chromatic
  - [ ] Capture snapshots of components
  - [ ] Run on CI


**Phase 3 Complete When:**
- ✅ All React components built and tested
- ✅ LineageExplorer works end-to-end
- ✅ Demo app is polished and deployed
- ✅ Package published to npm
- ✅ Documentation complete

---

## Phase 4: Web Worker Support & Performance

**Goal:** Optimize for large SQL workloads and production use

### 4.1 Web Worker Implementation

- [ ] **Create worker script**
  - [ ] Create packages/core/src/worker.ts
  - [ ] Import and initialize WASM in worker context
  - [ ] Listen for message events
  - [ ] Call analyzeSql on incoming requests
  - [ ] Post results back to main thread
  - [ ] Handle errors in worker

- [ ] **Build worker bundle**
  - [ ] Configure bundler to output worker.js
  - [ ] Ensure WASM can be loaded from worker
  - [ ] Test worker bundle standalone

- [ ] **Create worker helper API**
  - [ ] Create createLineageWorker() factory
  - [ ] Return object with analyzeSql method
  - [ ] Implement request/response message protocol
  - [ ] Generate unique request IDs
  - [ ] Match responses to promises
  - [ ] Implement terminate() method
  - [ ] Write tests

- [ ] **Add AbortSignal support**
  - [ ] Accept AbortSignal in analyzeSql()
  - [ ] Listen for abort events
  - [ ] Cancel in-flight requests
  - [ ] Reject promise with DOMException
  - [ ] Write tests

- [ ] **Handle large payloads**
  - [ ] Use structuredClone with transferables
  - [ ] Chunk payloads >5 MB
  - [ ] Reassemble in worker
  - [ ] Emit PAYLOAD_SIZE_WARNING when >10 MB
  - [ ] Write tests with large SQL

### 4.2 Performance Optimization

- [ ] **Profile current performance**
  - [ ] Create benchmark suite
  - [ ] Test with various query sizes (10 lines, 100 lines, 1000 lines)
  - [ ] Measure parsing time
  - [ ] Measure lineage computation time
  - [ ] Measure serialization time
  - [ ] Record baseline metrics

- [ ] **Optimize Rust code**
  - [ ] Run cargo-flamegraph to find hot spots
  - [ ] Optimize algorithms if needed
  - [ ] Use efficient data structures (HashMap, Vec)
  - [ ] Minimize cloning
  - [ ] Enable LTO in release builds
  - [ ] Re-benchmark

- [ ] **Optimize WASM binary size**
  - [ ] Use wasm-opt -Oz
  - [ ] Strip debug symbols in release
  - [ ] Check for unused dependencies
  - [ ] Measure final WASM size (<2 MB target)

- [ ] **Optimize TypeScript code**
  - [ ] Minimize JSON parsing overhead
  - [ ] Use transferable objects in workers
  - [ ] Benchmark worker vs main thread

- [ ] **Add progress reporting (optional)**
  - [ ] Emit progress events from Rust
  - [ ] Relay through worker
  - [ ] Expose progress callback in TypeScript API
  - [ ] Update demo app to show progress bar

### 4.3 Memory Management

- [ ] **Add memory configuration**
  - [ ] Add initialMemory option to initWasm()
  - [ ] Add maximumMemory option to initWasm()
  - [ ] Document defaults
  - [ ] Test with custom limits

- [ ] **Monitor memory usage**
  - [ ] Add memory telemetry in dev builds
  - [ ] Log memory stats in tests
  - [ ] Detect memory leaks in long-running tests

### 4.4 Testing

- [ ] **Performance tests**
  - [ ] Benchmark suite in CI
  - [ ] Fail if performance regresses >10%
  - [ ] Test with large fixtures (1000+ lines)

- [ ] **Worker tests**
  - [ ] Test worker initialization
  - [ ] Test concurrent requests
  - [ ] Test worker termination
  - [ ] Test abort/cancellation

- [ ] **Load tests**
  - [ ] Test with 10 concurrent analyses
  - [ ] Test with very large SQL (5 MB+)
  - [ ] Monitor memory usage

### 4.5 Documentation

- [ ] **Document worker usage**
  - [ ] Add worker example to README
  - [ ] Document when to use worker vs main thread
  - [ ] Document performance characteristics

- [ ] **Document performance tuning**
  - [ ] Memory configuration
  - [ ] Query size limits
  - [ ] Benchmarking guide


**Phase 4 Complete When:**
- ✅ Web Worker helper works reliably
- ✅ Performance meets targets (<500ms for typical queries)
- ✅ WASM binary <2 MB
- ✅ Tests and docs updated
- ✅ Published to registries

---

## Phase 5: Ecosystem & Integrations

**Goal:** Prepare for wider adoption and integration

### 5.0 Initial Package Publishing (from Phase 1)

- [ ] **Publish v0.1.0 packages**
  - [ ] Publish flowscope-core to crates.io
  - [ ] Publish flowscope-wasm to crates.io
  - [ ] Build and test @pondpilot/flowscope-core locally
  - [ ] Publish @pondpilot/flowscope-core to npm
  - [ ] Create Git tag: v0.1.0
  - [ ] Create GitHub release with notes

### 5.1 Documentation Site

- [ ] **Set up documentation site**
  - [ ] Choose framework (VitePress, Docusaurus, etc.)
  - [ ] Create site structure
  - [ ] Add navigation
  - [ ] Add search (Algolia or similar)

- [ ] **Write comprehensive guides**
  - [ ] Getting Started
  - [ ] Installation
  - [ ] Core Concepts (lineage, nodes, edges)
  - [ ] TypeScript API Reference
  - [ ] React Components Guide
  - [ ] Schema Metadata Guide
  - [ ] Error Codes Reference (link to error-codes.md)
  - [ ] Performance Tuning
  - [ ] Dialect Support Matrix

- [ ] **Add integration guides**
  - [ ] Embedding in a React app
  - [ ] Using in a vanilla JS app
  - [ ] Building a browser extension
  - [ ] Integrating with VS Code
  - [ ] Custom graph renderers

- [ ] **Deploy docs site**
  - [ ] Deploy to GitHub Pages or Vercel
  - [ ] Configure custom domain (flowscope.dev or similar)
  - [ ] Set up redirect from old URLs

### 5.2 Example Integrations

- [ ] **Browser extension example (optional)**
  - [ ] Create simple Chrome extension
  - [ ] Detect SQL in page (Snowflake, BigQuery, etc.)
  - [ ] Show lineage in side panel
  - [ ] Document how it works
  - [ ] Publish to separate repo

- [ ] **VS Code extension example (optional)**
  - [ ] Create VS Code extension
  - [ ] Add "Show Lineage" command
  - [ ] Display in webview panel
  - [ ] Document how it works
  - [ ] Publish to marketplace (optional)

### 5.4 Release Automation

- [ ] **Automate releases**
  - [ ] Set up semantic-release or similar
  - [ ] Auto-generate CHANGELOG from commits
  - [ ] Auto-bump versions
  - [ ] Auto-publish to crates.io
  - [ ] Auto-publish to npm
  - [ ] Auto-create GitHub release

- [ ] **Set up release process**
  - [ ] Document manual steps (if any)
  - [ ] Create release checklist
  - [ ] Assign release manager role


### 5.7 Release

- [ ] **Version bump to 1.0.0**
  - [ ] Finalize API (no more breaking changes)
  - [ ] Update versions
  - [ ] Update CHANGELOG.md

- [ ] **Publish stable release**
  - [ ] Publish all packages
  - [ ] Create Git tag: v1.0.0
  - [ ] Create GitHub release (mark as stable)
  - [ ] Announce widely

**Phase 5 Complete When:**
- ✅ Documentation site live and comprehensive
- ✅ Example integrations available
- ✅ Community processes established
- ✅ v1.0.0 published and announced

---

## Future Enhancements (Post-v1.0)

These are not prioritized but captured for future consideration:

### Additional Dialects
- [ ] MySQL support
- [ ] Redshift support
- [ ] Databricks SQL support
- [ ] Trino/Presto support

### Additional Statement Types
- [ ] UPDATE statements
- [ ] DELETE statements
- [ ] MERGE statements
- [ ] ALTER TABLE statements

### Advanced Features
- [ ] Recursive CTE support
- [ ] Window function lineage
- [ ] Materialized view tracking
- [ ] Time-travel/temporal lineage
- [ ] Data type inference
- [ ] Foreign key awareness

### Export Formats
- [ ] Mermaid diagram export


### UI Enhancements
- [ ] Graph diff view (compare two versions)
- [ ] Collapsible node groups
- [ ] Search/filter nodes
- [ ] Export graph as image (PNG/SVG)
- [ ] Dark mode


---

## Task Metadata

**Legend:**
- [ ] Not started
- [x] Complete
- [~] In progress
- [!] Blocked

**Priority Levels:**
- P0: Critical path, must complete before next phase
- P1: Important, should complete in current phase
- P2: Nice to have, can defer

**Estimation Guidelines:**
- Tasks sized to be completable in 1-4 hours
- If larger, break down further
- Include testing and documentation time

---

Last Updated: 2025-11-21 (Phase 3 In Progress)
