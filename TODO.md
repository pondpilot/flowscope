# FlowScope Implementation TODO

**Version:** 0.1.0
**Last Updated:** 2025-11-20
**Status:** Ready for Phase 0

This document provides a detailed, phase-by-phase implementation checklist for FlowScope. Each task is designed to be actionable and testable.

---

## Phase 0: Spike / Feasibility ✅ READY TO START

**Goal:** Prove the tech stack (Rust + sqlparser-rs + WASM) works end-to-end

### 0.1 Project Setup

- [ ] **Create repository structure**
  - [ ] Initialize Git repository
  - [ ] Create directory structure per `docs/workspace-structure.md`
  - [ ] Add `.gitignore` for Rust, Node, and build artifacts
  - [ ] Set up `.github/` directory structure

- [ ] **Initialize Rust workspace**
  - [ ] Create root `Cargo.toml` with workspace configuration
  - [ ] Create `crates/flowscope-core/` with basic structure
  - [ ] Create `crates/flowscope-wasm/` with basic structure
  - [ ] Add `sqlparser` dependency to flowscope-core
  - [ ] Add `wasm-bindgen`, `serde`, `serde_json` to flowscope-wasm
  - [ ] Verify `cargo build` succeeds

- [ ] **Initialize NPM workspace**
  - [ ] Create root `package.json` with workspace configuration
  - [ ] Set up `packages/core/` with package.json
  - [ ] Set up `packages/react/` with package.json (minimal for now)
  - [ ] Set up `examples/web-demo/` with package.json
  - [ ] Create `tsconfig.base.json`
  - [ ] Verify `yarn install` succeeds

- [ ] **Add essential config files**
  - [ ] `.prettierrc` (Prettier config)
  - [ ] `.eslintrc.js` (ESLint config)
  - [ ] `LICENSE` (Apache 2.0)
  - [ ] Root `README.md` (project overview)
  - [ ] `CONTRIBUTING.md` (developer setup guide)
  - [ ] `SECURITY.md` (security policy)

### 0.2 Minimal Rust Parser

- [ ] **Implement basic SQL parsing**
  - [ ] Create `parser` module in flowscope-core
  - [ ] Implement `parse_sql()` function wrapping sqlparser-rs
  - [ ] Support parsing simple SELECT statements
  - [ ] Return Result<Vec<Statement>, ParseError>
  - [ ] Write unit test for valid SELECT
  - [ ] Write unit test for invalid SQL

- [ ] **Implement trivial lineage extraction**
  - [ ] Create `lineage` module
  - [ ] Implement `extract_tables()` function
  - [ ] Extract table names from FROM clauses only
  - [ ] Return Vec<String> of table names
  - [ ] Write unit test with SELECT from single table
  - [ ] Write unit test with SELECT from multiple tables (JOIN)

- [ ] **Add basic types**
  - [ ] Create `types.rs` with LineageResult struct
  - [ ] Implement basic serialization with serde
  - [ ] Add test to verify JSON serialization works

### 0.3 WASM Bridge

- [ ] **Create WASM wrapper**
  - [ ] Implement `analyze_sql()` function in flowscope-wasm
  - [ ] Accept JSON string input
  - [ ] Call flowscope-core functions
  - [ ] Return JSON string output
  - [ ] Add wasm-bindgen annotations
  - [ ] Handle errors without panicking

- [ ] **Build WASM module**
  - [ ] Install wasm-pack
  - [ ] Create `wasm-pack build` command
  - [ ] Verify WASM binary is generated
  - [ ] Verify JS glue code is generated
  - [ ] Check output size (should be reasonable, <2MB)

### 0.4 Minimal Web Demo

- [ ] **Set up basic HTML page**
  - [ ] Create `examples/web-demo/index.html`
  - [ ] Add textarea for SQL input
  - [ ] Add button to trigger analysis
  - [ ] Add div to display results

- [ ] **Load WASM in browser**
  - [ ] Write JS to load WASM module
  - [ ] Initialize WASM
  - [ ] Handle loading errors gracefully
  - [ ] Add loading indicator

- [ ] **Wire up analysis**
  - [ ] Get SQL from textarea on button click
  - [ ] Call WASM analyze_sql function
  - [ ] Parse JSON result
  - [ ] Display table list in results div
  - [ ] Display any errors

- [ ] **Manual testing**
  - [ ] Test with `SELECT * FROM users`
  - [ ] Test with `SELECT * FROM users JOIN orders ON users.id = orders.user_id`
  - [ ] Test with invalid SQL (should show error)
  - [ ] Verify in Chrome, Firefox, Safari

### 0.5 Documentation

- [ ] **Document spike results**
  - [ ] Record WASM bundle size
  - [ ] Record load time in browser
  - [ ] Note any sqlparser-rs limitations found
  - [ ] Update design docs if assumptions changed

**Phase 0 Complete When:**
- ✅ Can parse simple SELECT and extract table names
- ✅ WASM module loads and runs in browser
- ✅ Basic demo app shows table lineage for simple queries

---

## Phase 1: Table-Level Lineage MVP

**Goal:** Production-ready core engine for table-level lineage

### 1.1 Core Engine - Foundation

- [ ] **Enhance type system**
  - [ ] Define complete `AnalyzeRequest` struct
    - [ ] sql: String
    - [ ] dialect: Dialect enum
    - [ ] options: Option<AnalysisOptions>
    - [ ] schema: Option<SchemaMetadata>
  - [ ] Define complete `AnalyzeResult` struct per `api-types.md`
  - [ ] Add StatementLineage struct
  - [ ] Add GlobalLineage struct
  - [ ] Add Issue struct with severity, code, message, span
  - [ ] Add Summary struct
  - [ ] Implement Serialize/Deserialize for all types
  - [ ] Generate JSON Schema using schemars

- [ ] **Implement dialect support**
  - [ ] Create Dialect enum (Generic, Postgres, Snowflake, BigQuery)
  - [ ] Map dialects to sqlparser-rs dialects
  - [ ] Implement dialect selection logic
  - [ ] Add fallback to Generic with warning
  - [ ] Test each dialect can parse basic queries

- [ ] **Build schema metadata layer**
  - [ ] Define SchemaMetadata struct per spec
  - [ ] Implement schema normalization (catalog.schema.table)
  - [ ] Implement case-sensitivity handling per dialect
  - [ ] Implement search path resolution
  - [ ] Add schema validation
  - [ ] Write tests for qualified name resolution

### 1.2 Core Engine - Lineage Computation

- [ ] **Implement SELECT analysis**
  - [ ] Extract source tables from FROM clause
  - [ ] Handle table aliases
  - [ ] Handle implicit SELECT targets (no INTO/CREATE TABLE)
  - [ ] Create table nodes
  - [ ] Create edges (table → statement)
  - [ ] Write test: simple SELECT
  - [ ] Write test: SELECT with alias

- [ ] **Implement JOIN analysis**
  - [ ] Detect INNER JOIN
  - [ ] Detect LEFT/RIGHT/FULL JOIN
  - [ ] Detect CROSS JOIN
  - [ ] Extract join conditions
  - [ ] Create edges for join relationships
  - [ ] Write test for each join type

- [ ] **Implement CTE (WITH) analysis**
  - [ ] Detect CTE definitions
  - [ ] Parse CTE bodies recursively
  - [ ] Handle multiple CTEs
  - [ ] Handle CTEs referencing other CTEs
  - [ ] Create CTE nodes distinct from table nodes
  - [ ] Write test: single CTE
  - [ ] Write test: multiple CTEs
  - [ ] Write test: nested CTE references
  - [ ] Detect recursive CTEs and emit UNSUPPORTED_RECURSIVE_CTE

- [ ] **Implement INSERT INTO ... SELECT**
  - [ ] Parse INSERT target table
  - [ ] Parse SELECT source
  - [ ] Create edge: source → target
  - [ ] Handle column list on INSERT
  - [ ] Write test: INSERT without column list
  - [ ] Write test: INSERT with column list

- [ ] **Implement CREATE TABLE AS SELECT (CTAS)**
  - [ ] Parse CREATE TABLE target
  - [ ] Parse SELECT source
  - [ ] Create table node for new table
  - [ ] Create edge: source → target
  - [ ] Write test: basic CTAS

- [ ] **Implement UNION/UNION ALL**
  - [ ] Detect set operations
  - [ ] Analyze each branch separately
  - [ ] Merge results
  - [ ] Create edges from sources to union result
  - [ ] Write test: simple UNION
  - [ ] Write test: UNION ALL
  - [ ] Write test: multiple UNIONs

- [ ] **Implement subquery analysis**
  - [ ] Detect subqueries in FROM clause
  - [ ] Analyze subquery as separate statement
  - [ ] Connect subquery lineage to parent
  - [ ] Write test: subquery in FROM
  - [ ] Write test: nested subqueries

### 1.3 Core Engine - Cross-Statement Assembly

- [ ] **Implement global graph builder**
  - [ ] Create GlobalLineage structure
  - [ ] Deduplicate nodes across statements using canonical names
  - [ ] Collect all nodes into global node list
  - [ ] Collect all edges into global edge list
  - [ ] Add StatementRef to each node
  - [ ] Write test: two independent statements
  - [ ] Write test: statement 2 reads from statement 1 output

- [ ] **Implement cross-statement edge detection**
  - [ ] Track tables/CTEs produced by each statement
  - [ ] Detect when later statement reads earlier output
  - [ ] Create cross_statement edges
  - [ ] Add producer/consumer statement references
  - [ ] Write test: INSERT then SELECT from same table
  - [ ] Write test: CTAS then INSERT into created table

- [ ] **Handle unresolved references**
  - [ ] Create placeholder nodes for missing tables
  - [ ] Emit UNRESOLVED_REFERENCE issue
  - [ ] Link placeholders to global graph
  - [ ] Write test: SELECT from non-existent table

### 1.4 Core Engine - Error Handling

- [ ] **Implement issue collection**
  - [ ] Create IssueCollector
  - [ ] Emit PARSE_ERROR for sqlparser failures
  - [ ] Emit DIALECT_FALLBACK when needed
  - [ ] Emit UNSUPPORTED_SYNTAX for unhandled constructs
  - [ ] Emit UNKNOWN_TABLE when table not in schema
  - [ ] Capture spans from sqlparser when available
  - [ ] Associate issues with statement index

- [ ] **Implement summary generation**
  - [ ] Count statements analyzed
  - [ ] Count unique tables discovered
  - [ ] Count issues by severity
  - [ ] Set has_errors flag
  - [ ] Write test: verify summary correctness

- [ ] **Implement graceful degradation**
  - [ ] Continue analysis if one statement fails
  - [ ] Provide partial lineage when possible
  - [ ] Never panic/abort at WASM boundary
  - [ ] Write test: multi-statement with one failure

### 1.5 WASM Layer

- [ ] **Finalize WASM bridge**
  - [ ] Implement complete analyze_sql function
  - [ ] Serialize AnalyzeRequest from JSON
  - [ ] Deserialize AnalyzeResult to JSON
  - [ ] Handle JSON parse errors gracefully
  - [ ] Add error logging (to console in debug builds)
  - [ ] Optimize for size (use wasm-opt)

- [ ] **Test WASM boundary**
  - [ ] Write integration test calling from Node.js
  - [ ] Verify round-trip JSON serialization
  - [ ] Test with large SQL (>10KB)
  - [ ] Test with malformed JSON input
  - [ ] Measure performance (baseline benchmarks)

### 1.6 TypeScript Wrapper (@pondpilot/flowscope-core)

- [ ] **Set up package structure**
  - [ ] Create src/ directory
  - [ ] Set up TypeScript configuration
  - [ ] Configure build (use Bun for bundling)
  - [ ] Configure test framework (Jest)

- [ ] **Implement type definitions**
  - [ ] Copy/generate types from Rust (schemars → TypeScript)
  - [ ] Export AnalyzeRequest interface
  - [ ] Export AnalyzeResult interface
  - [ ] Export all nested types (Node, Edge, Issue, etc.)
  - [ ] Add TSDoc comments

- [ ] **Implement WASM loader**
  - [ ] Create wasm-loader.ts
  - [ ] Implement initWasm() function
  - [ ] Support custom WASM URL
  - [ ] Handle fetch() errors
  - [ ] Handle WebAssembly.instantiate() errors
  - [ ] Make idempotent (safe to call multiple times)
  - [ ] Write test: successful init
  - [ ] Write test: init with missing WASM file

- [ ] **Implement analyzeSql() function**
  - [ ] Create analyzer.ts
  - [ ] Implement typed analyzeSql(request: AnalyzeRequest)
  - [ ] Call initWasm() lazily if needed
  - [ ] Validate request object
  - [ ] Call WASM analyze_sql function
  - [ ] Parse JSON result
  - [ ] Type-check result
  - [ ] Return typed AnalyzeResult
  - [ ] Write unit test: simple query
  - [ ] Write unit test: invalid SQL
  - [ ] Write unit test: with schema metadata

- [ ] **Handle errors properly**
  - [ ] Reject Promise for technical errors (WASM load failure)
  - [ ] Return AnalyzeResult with issues for analysis errors
  - [ ] Add clear error messages
  - [ ] Write test: WASM not initialized
  - [ ] Write test: malformed request

- [ ] **Create package.json**
  - [ ] Set name: @pondpilot/flowscope-core
  - [ ] Set version: 0.1.0
  - [ ] Add exports field (main, types, worker)
  - [ ] Add dependencies
  - [ ] Add build scripts
  - [ ] Add test scripts
  - [ ] Add files field (include dist/ and wasm/)

### 1.7 Example Web Demo (Basic)

- [ ] **Set up Vite project**
  - [ ] Create Vite config
  - [ ] Set up React 18
  - [ ] Configure TypeScript
  - [ ] Add @pondpilot/flowscope-core as dependency

- [ ] **Build basic UI**
  - [ ] Create main App component
  - [ ] Add SQL textarea (CodeMirror 6)
  - [ ] Add dialect selector (dropdown)
  - [ ] Add "Analyze" button
  - [ ] Add loading spinner
  - [ ] Show JSON result in pre tag
  - [ ] Style with Tailwind CSS

- [ ] **Wire up analysis**
  - [ ] Import analyzeSql from @pondpilot/flowscope-core
  - [ ] Call analyzeSql on button click
  - [ ] Handle loading state
  - [ ] Display results
  - [ ] Display errors/issues
  - [ ] Add sample SQL examples (dropdown)

- [ ] **Manual QA**
  - [ ] Test all 4 dialects
  - [ ] Test with sample queries
  - [ ] Test error cases
  - [ ] Verify in Chrome, Firefox, Safari
  - [ ] Check console for errors

### 1.8 Testing Infrastructure

- [ ] **Create Rust test fixtures**
  - [ ] Set up `crates/flowscope-core/tests/fixtures/`
  - [ ] Add SQL files for each dialect
    - [ ] postgres/01_basic_select.sql
    - [ ] postgres/02_join.sql
    - [ ] postgres/03_cte.sql
    - [ ] (repeat for snowflake, bigquery, generic)
  - [ ] Add schema JSON files
  - [ ] Create golden output files
  - [ ] Write fixture loader utility

- [ ] **Write Rust unit tests**
  - [ ] Test for each statement type
  - [ ] Test for each dialect
  - [ ] Test edge cases (empty SQL, comments only, etc.)
  - [ ] Test error paths
  - [ ] Aim for >80% coverage of core logic

- [ ] **Write Rust integration tests**
  - [ ] Test full analyze_sql pipeline
  - [ ] Test with fixtures
  - [ ] Compare against golden outputs
  - [ ] Test cross-statement lineage

- [ ] **Write TypeScript unit tests**
  - [ ] Test WASM loader
  - [ ] Test analyzeSql function
  - [ ] Test type conversions
  - [ ] Test error handling
  - [ ] Mock WASM module where appropriate

- [ ] **Set up CI/CD**
  - [ ] Create .github/workflows/ci.yml
  - [ ] Add Rust build + test job
  - [ ] Add WASM build job
  - [ ] Add TypeScript lint + test job
  - [ ] Add artifact caching
  - [ ] Run on every push and PR

### 1.9 Documentation

- [ ] **Create API documentation**
  - [ ] Generate rustdoc for flowscope-core
  - [ ] Generate rustdoc for flowscope-wasm
  - [ ] Generate TypeDoc for @pondpilot/flowscope-core
  - [ ] Host docs on GitHub Pages (gh-pages branch)

- [ ] **Write user guides**
  - [ ] Quickstart: TypeScript usage
  - [ ] Guide: Schema metadata format
  - [ ] Guide: Error handling
  - [ ] Guide: Dialect support matrix
  - [ ] Add code examples for each

- [ ] **Create dialect coverage matrix**
  - [ ] Document supported SQL features per dialect
  - [ ] Mark as Supported / Partial / Unsupported
  - [ ] Link to test fixtures
  - [ ] Add to docs/dialect-coverage.md

- [ ] **Update README.md**
  - [ ] Add installation instructions
  - [ ] Add usage example
  - [ ] Add link to docs
  - [ ] Add badges (build status, version, license)

### 1.10 Release Preparation

- [ ] **Code cleanup**
  - [ ] Run cargo fmt on all Rust code
  - [ ] Run cargo clippy and fix warnings
  - [ ] Run prettier on all TypeScript code
  - [ ] Run ESLint and fix warnings
  - [ ] Remove debug code and console.logs

- [ ] **Pre-release checklist**
  - [ ] All tests passing
  - [ ] No compiler warnings
  - [ ] Documentation complete
  - [ ] CHANGELOG.md created
  - [ ] Version numbers consistent (0.1.0)
  - [ ] LICENSE file present

- [ ] **Publish packages**
  - [ ] Publish flowscope-core to crates.io
  - [ ] Publish flowscope-wasm to crates.io
  - [ ] Build and test @pondpilot/flowscope-core locally
  - [ ] Publish @pondpilot/flowscope-core to npm
  - [ ] Create Git tag: v0.1.0
  - [ ] Create GitHub release with notes

**Phase 1 Complete When:**
- ✅ All core lineage features work for table-level analysis
- ✅ Global cross-statement graph is generated correctly
- ✅ All 4 dialects parse and analyze successfully
- ✅ Tests pass with >80% coverage
- ✅ Packages published to crates.io and npm
- ✅ Demo app works in all major browsers

---

## Phase 2: Column-Level Lineage & Schema Support

**Goal:** Add precise column-level lineage tracking

### 2.1 Core Engine - Column Tracking

- [ ] **Extend AST analysis for columns**
  - [ ] Extract column references from SELECT list
  - [ ] Extract column references from WHERE clause
  - [ ] Extract column references from JOIN conditions
  - [ ] Extract column references from GROUP BY / HAVING
  - [ ] Handle column aliases
  - [ ] Handle qualified column names (table.column)

- [ ] **Implement column node creation**
  - [ ] Create column nodes for each referenced column
  - [ ] Link columns to parent tables via ownership edges
  - [ ] Handle computed columns (expressions)
  - [ ] Store expression text in column metadata
  - [ ] Write test: SELECT with explicit columns
  - [ ] Write test: SELECT with expressions

- [ ] **Implement column lineage edges**
  - [ ] Create data_flow edges: input column → output column
  - [ ] Create derivation edges: multiple inputs → computed output
  - [ ] Track expression transformations
  - [ ] Write test: simple column passthrough
  - [ ] Write test: computed column (SUM, CONCAT, etc.)

- [ ] **Handle SELECT * expansion**
  - [ ] When schema provided: expand * to explicit columns
  - [ ] When schema missing: create placeholder or emit warning
  - [ ] Handle table.* syntax
  - [ ] Write test: SELECT * with schema
  - [ ] Write test: SELECT * without schema (approximate)

- [ ] **Implement JOIN column lineage**
  - [ ] Track which table each output column comes from
  - [ ] Handle ambiguous column names
  - [ ] Create edges through join conditions
  - [ ] Write test: columns from left table
  - [ ] Write test: columns from right table
  - [ ] Write test: computed from both sides

### 2.2 Core Engine - Schema Integration

- [ ] **Enhance schema metadata**
  - [ ] Add column data types (optional)
  - [ ] Add primary key hints (optional)
  - [ ] Validate schema structure on input
  - [ ] Emit warnings for malformed schema

- [ ] **Implement schema-based validation**
  - [ ] Check if referenced columns exist in schema
  - [ ] Emit UNKNOWN_COLUMN issue when not found
  - [ ] Continue with best-effort lineage
  - [ ] Write test: valid column references
  - [ ] Write test: invalid column reference

- [ ] **Improve search path resolution**
  - [ ] Use search_path for unqualified table names
  - [ ] Try each path entry in order
  - [ ] Use defaultCatalog and defaultSchema as fallbacks
  - [ ] Write test: qualified name resolution
  - [ ] Write test: search path resolution

### 2.3 Analysis Options

- [ ] **Add enableColumnLineage option**
  - [ ] Default to true
  - [ ] Allow disabling for performance
  - [ ] Skip column analysis when disabled
  - [ ] Write test: with option enabled
  - [ ] Write test: with option disabled

### 2.4 Testing

- [ ] **Add column lineage test fixtures**
  - [ ] Create SQL samples with explicit column references
  - [ ] Create SQL samples with SELECT *
  - [ ] Create SQL samples with computed columns
  - [ ] Create corresponding schema JSON files
  - [ ] Create golden outputs

- [ ] **Write comprehensive tests**
  - [ ] Test column passthrough
  - [ ] Test column expressions (math, string ops, functions)
  - [ ] Test window functions (as expressions)
  - [ ] Test GROUP BY / aggregations
  - [ ] Test CASE expressions

- [ ] **Update integration tests**
  - [ ] Verify column-level edges in output
  - [ ] Verify expression metadata
  - [ ] Verify schema validation

### 2.5 TypeScript & Demo Updates

- [ ] **Update @pondpilot/flowscope-core**
  - [ ] Add enableColumnLineage to AnalysisOptions type
  - [ ] Update API documentation
  - [ ] Add examples showing column lineage

- [ ] **Update demo app**
  - [ ] Display column nodes in JSON view
  - [ ] Show expression details
  - [ ] Add checkbox to toggle column lineage
  - [ ] Test with schema metadata input

### 2.6 Documentation

- [ ] **Document column lineage features**
  - [ ] Write guide: How column lineage works
  - [ ] Document expression tracking
  - [ ] Document limitations (window functions, etc.)
  - [ ] Add examples

- [ ] **Update dialect coverage matrix**
  - [ ] Add column-level support status per dialect
  - [ ] Note any dialect-specific differences

### 2.7 Release

- [ ] **Version bump to 0.2.0**
  - [ ] Update Cargo.toml versions
  - [ ] Update package.json versions
  - [ ] Update CHANGELOG.md

- [ ] **Publish**
  - [ ] Publish Rust crates
  - [ ] Publish npm package
  - [ ] Create Git tag: v0.2.0
  - [ ] Create GitHub release

**Phase 2 Complete When:**
- ✅ Column-level lineage works for explicit columns
- ✅ SELECT * expansion works with schema
- ✅ Expressions tracked and visible in output
- ✅ Tests pass with good coverage
- ✅ Published to registries

---

## Phase 3: React Viewer & Full Demo

**Goal:** Build polished React UI components for lineage visualization

### 3.1 Package Setup (@pondpilot/flowscope-react)

- [ ] **Initialize package**
  - [ ] Create packages/react/ structure
  - [ ] Set up package.json
  - [ ] Configure TypeScript
  - [ ] Configure Tailwind CSS
  - [ ] Add React 18 and ReactFlow as dependencies
  - [ ] Add @pondpilot/flowscope-core as peer dependency

- [ ] **Set up build**
  - [ ] Configure bundler (Bun or Rollup)
  - [ ] Set up CSS processing
  - [ ] Generate types (.d.ts)
  - [ ] Test build output

- [ ] **Set up testing**
  - [ ] Configure Jest for React
  - [ ] Add @testing-library/react
  - [ ] Create test utilities
  - [ ] Add snapshot testing capability

### 3.2 Core Components

- [ ] **GraphView component**
  - [ ] Set up ReactFlow
  - [ ] Convert lineage nodes to ReactFlow nodes
  - [ ] Convert lineage edges to ReactFlow edges
  - [ ] Implement table node renderer (custom)
  - [ ] Implement column node renderer (custom)
  - [ ] Add zoom/pan controls
  - [ ] Add layout algorithm (Dagre or ELK)
  - [ ] Handle node selection
  - [ ] Emit onNodeSelect event
  - [ ] Style nodes (tables, CTEs, columns distinct)
  - [ ] Write tests

- [ ] **ColumnPanel component**
  - [ ] Display selected column details
  - [ ] Show upstream columns (sources)
  - [ ] Show downstream columns (consumers)
  - [ ] Show expression text
  - [ ] Show data flow path (A → B → C)
  - [ ] Handle no selection state
  - [ ] Style with Tailwind
  - [ ] Write tests

- [ ] **SqlView component**
  - [ ] Integrate CodeMirror 6
  - [ ] Display SQL with syntax highlighting
  - [ ] Highlight selected node spans
  - [ ] Highlight issue spans (errors in red, warnings in yellow)
  - [ ] Handle click on highlighted spans (select node)
  - [ ] Add line numbers
  - [ ] Make read-only
  - [ ] Style with Tailwind
  - [ ] Write tests

- [ ] **IssuesPanel component**
  - [ ] Display list of issues
  - [ ] Group by severity (errors, warnings, info)
  - [ ] Show issue count badges
  - [ ] Format issue messages
  - [ ] Make issue clickable → highlight in SqlView
  - [ ] Style with appropriate colors (red, yellow, blue)
  - [ ] Write tests

- [ ] **StatementSelector component**
  - [ ] Display when multiple statements exist
  - [ ] Show statement index and type
  - [ ] Highlight selected statement
  - [ ] Emit onStatementSelect event
  - [ ] Style as tab bar or dropdown
  - [ ] Write tests

### 3.3 Composite Components

- [ ] **LineageExplorer component**
  - [ ] Compose GraphView, SqlView, ColumnPanel, IssuesPanel
  - [ ] Accept AnalyzeResult as prop
  - [ ] Accept SQL string as prop
  - [ ] Add StatementSelector when needed
  - [ ] Wire up component interactions (selection sync)
  - [ ] Add responsive layout (grid or flex)
  - [ ] Support theme prop (light/dark)
  - [ ] Export as main public component
  - [ ] Write integration tests

### 3.4 Hooks & Utilities

- [ ] **useLineageExplorer hook**
  - [ ] Manage selected statement
  - [ ] Manage selected node
  - [ ] Sync selection across sub-components
  - [ ] Provide helper methods (selectNode, selectStatement)
  - [ ] Write tests

- [ ] **Graph layout utilities**
  - [ ] Implement table-only layout mode
  - [ ] Implement table+column layout mode
  - [ ] Position nodes to minimize edge crossings
  - [ ] Add padding and spacing constants
  - [ ] Write tests

- [ ] **Span highlighting utilities**
  - [ ] Map span offsets to CodeMirror decorations
  - [ ] Handle overlapping spans
  - [ ] Support multiple highlight colors
  - [ ] Write tests

### 3.5 Styling & Theming

- [ ] **Create Tailwind theme**
  - [ ] Define color palette (primary, secondary, accent)
  - [ ] Define spacing scale
  - [ ] Define typography
  - [ ] Support dark mode (optional for v1)

- [ ] **Style all components**
  - [ ] Use Tailwind utility classes
  - [ ] Keep consistent spacing
  - [ ] Ensure good contrast (accessibility)
  - [ ] Add hover states
  - [ ] Add focus states (keyboard nav)

- [ ] **Support customization**
  - [ ] Accept className prop on all components
  - [ ] Accept theme prop on LineageExplorer
  - [ ] Document customization options

### 3.6 Package Documentation

- [ ] **Add component documentation**
  - [ ] TSDoc comments on all public components
  - [ ] Document all props
  - [ ] Add usage examples

- [ ] **Create README**
  - [ ] Installation instructions
  - [ ] Quick start example
  - [ ] Component API reference
  - [ ] Styling guide
  - [ ] Link to Storybook (if added)

- [ ] **Set up Storybook (optional)**
  - [ ] Install Storybook
  - [ ] Create stories for each component
  - [ ] Add example states (loading, error, success)
  - [ ] Deploy Storybook to GitHub Pages

### 3.7 Example Demo App (Enhanced)

- [ ] **Update demo app to use React components**
  - [ ] Replace JSON view with LineageExplorer
  - [ ] Keep option to show raw JSON (collapsible)
  - [ ] Add schema input (JSON textarea or file upload)
  - [ ] Add example query library (dropdown)
  - [ ] Add dialect selector
  - [ ] Add "Analyze" button with loading state

- [ ] **Add example queries**
  - [ ] Simple SELECT
  - [ ] JOIN query
  - [ ] CTE query
  - [ ] INSERT INTO SELECT
  - [ ] Complex dbt-style model

- [ ] **Polish UI**
  - [ ] Add header with logo/title
  - [ ] Add footer with links (GitHub, docs)
  - [ ] Responsive layout (mobile-friendly)
  - [ ] Add error boundaries
  - [ ] Add help tooltips

- [ ] **Deploy demo app**
  - [ ] Set up Vercel or Netlify deployment
  - [ ] Configure custom domain (optional)
  - [ ] Add analytics (optional)
  - [ ] Test in production

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

### 3.9 Release

- [ ] **Version bump to 0.3.0**
  - [ ] Update package.json
  - [ ] Update CHANGELOG.md

- [ ] **Publish**
  - [ ] Build package
  - [ ] Test package locally (npm pack, install in test project)
  - [ ] Publish @pondpilot/flowscope-react to npm
  - [ ] Create Git tag: v0.3.0
  - [ ] Create GitHub release

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

### 4.6 Release

- [ ] **Version bump to 0.4.0**
  - [ ] Update versions
  - [ ] Update CHANGELOG.md

- [ ] **Publish**
  - [ ] Publish updated packages
  - [ ] Create Git tag: v0.4.0
  - [ ] Create GitHub release

**Phase 4 Complete When:**
- ✅ Web Worker helper works reliably
- ✅ Performance meets targets (<500ms for typical queries)
- ✅ WASM binary <2 MB
- ✅ Tests and docs updated
- ✅ Published to registries

---

## Phase 5: Ecosystem & Integrations

**Goal:** Prepare for wider adoption and integration

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

### 5.3 Community & Governance

- [ ] **Set up issue templates**
  - [ ] Bug report template
  - [ ] Feature request template
  - [ ] Dialect support request template
  - [ ] Question template

- [ ] **Create PR template**
  - [ ] Checklist for contributors
  - [ ] Require tests
  - [ ] Require docs update

- [ ] **Set up discussions**
  - [ ] Enable GitHub Discussions
  - [ ] Create categories (Q&A, Ideas, etc.)

- [ ] **Create contributing guide**
  - [ ] How to build the project
  - [ ] How to run tests
  - [ ] Code style guidelines
  - [ ] PR process
  - [ ] Release process

- [ ] **Create roadmap**
  - [ ] Document planned features
  - [ ] Solicit community input
  - [ ] Prioritize based on feedback

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

### 5.5 Analytics & Monitoring (Optional)

- [ ] **Add telemetry (opt-in)**
  - [ ] Track usage metrics (anonymized)
  - [ ] Track errors
  - [ ] Track performance
  - [ ] Document privacy policy

- [ ] **Set up monitoring**
  - [ ] Monitor demo app uptime
  - [ ] Monitor docs site uptime
  - [ ] Set up alerts

### 5.6 Marketing & Adoption

- [ ] **Create announcement materials**
  - [ ] Blog post announcement
  - [ ] Tweet thread
  - [ ] Show HN post
  - [ ] Reddit posts (r/programming, r/datascience)

- [ ] **Create showcase**
  - [ ] Add "Powered by FlowScope" badge
  - [ ] Collect adopter logos
  - [ ] Create case studies (if available)

- [ ] **Engage community**
  - [ ] Respond to issues promptly
  - [ ] Review PRs
  - [ ] Answer questions on Discussions
  - [ ] Update docs based on feedback

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
- [ ] OpenLineage JSON events
- [ ] GraphML export
- [ ] DOT format export
- [ ] Mermaid diagram export

### Performance
- [ ] Incremental analysis (cache previous results)
- [ ] Streaming analysis for very large SQL
- [ ] Parallel statement analysis

### UI Enhancements
- [ ] Graph diff view (compare two versions)
- [ ] Collapsible node groups
- [ ] Search/filter nodes
- [ ] Export graph as image (PNG/SVG)
- [ ] Dark mode

### Integrations
- [ ] dbt integration (parse dbt models directly)
- [ ] Airflow integration
- [ ] Dagster integration
- [ ] SQLMesh integration

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

Last Updated: 2025-11-20
