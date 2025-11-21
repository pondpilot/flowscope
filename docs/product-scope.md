# Product Scope

## 1. Problem & Vision

### 1.1 Problem

Data teams and tool builders need to understand **where data comes from and where it flows to** in complex SQL. Existing lineage offerings:

- Are often **server-based metadata platforms** (Atlas, OpenMetadata, etc.) that require substantial infra.
- Or **closed, SaaS/on-prem products** (e.g., SQLFlow) where analysis is done on backend services.
- Or **language-bound libraries** (e.g., Python-based lineage libraries) that don't drop cleanly into browser-based tools.

There is currently no widely-used **lightweight, fully client-side, embeddable** lineage engine that can run entirely in the browser and integrate seamlessly into existing frontends and editors.

### 1.2 Vision

Build a **client-side SQL lineage engine** that:

- Runs entirely in the browser as WebAssembly (no backend required).
- Accepts SQL (and optional schema metadata) as input.
- Produces **table-level and column-level lineage graphs** plus analysis issues.
- Is distributed as:
  - A Rust crate (for future native uses),
  - A WebAssembly module,
  - One or more **NPM packages** for JS/TS consumers.
- Includes optional React components for visualizing lineage, so it can be dropped into:
  - Internal tools,
  - Browser extensions,
  - IDE/web editors,
  - Vendor UIs.

## 2. Target Users & Use Cases

### 2.1 Primary Personas

1. **Analytics/Data Engineers**
   - Paste a query or the **compiled SQL** emitted by their dbt model file and see **table and column lineage**.
   - Use a browser-based app or editor plugin (VS Code, dbt Cloud, Snowflake UI overlays).

2. **Tool Builders (SaaS or internal tools)**
   - Embed lineage visualization in their own SQL editors, notebooks, BI tools, etc.
   - Want an **NPM package + React components** they can plug in.

3. **Consultants / SIs**
   - Need portable tooling to analyze client SQL **without sending data off-prem** and without standing up heavy infrastructure.

### 2.2 Example Use Cases

- Understanding the downstream impact of changing a column in a warehouse table.
- Visualizing how dbt models and views depend on each other.
- Inspecting a large analytics query in a browser extension layered over a cloud data warehouse UI.
- Providing inline lineage in a web-based SQL IDE.

## 3. In-Scope Functionality (Initial Versions)

### 3.1 Functional Scope

- **SQL input**
  - Multi-statement SQL string.
  - Input must already be **fully rendered SQL**. dbt/Dagster templating, macros, variables, or Jinja-like constructs are resolved by the host application before invoking the engine.
  - Optional dialect declaration per request.
  - Optional schema metadata (simple JSON structure).

- **SQL parsing & lineage**
  - Support for common analytics dialects:
    - Generic,
    - Postgres,
    - Snowflake,
    - BigQuery.
  - Statement types:
    - `SELECT` (with joins, aliases, subqueries).
    - `WITH` / CTEs.
    - `INSERT INTO ... SELECT`.
    - `CREATE TABLE ... AS SELECT`.
    - `UNION / UNION ALL` and basic set operations.
  - Lineage levels:
    - **Table-level lineage** (which tables/CTEs feed which targets).
    - **Column-level lineage**:
      - Map output columns to upstream input columns.
      - Track expressions (e.g. `price * quantity`).
      - Best-effort for `*` without schema; precise mapping when schema is provided.

- **Output**
  - A **lineage graph** representation:
    - Nodes (tables, CTEs, columns, optional operations/statements).
    - Directed edges representing data flow and derivation.
  - A **global dependency graph** that deduplicates nodes across statements so downstream consumers can answer cross-statement questions (e.g., "what uses this temp table later in the script?").
  - A flat list of **issues** (errors, warnings, infos) for:
    - Parse failures.
    - Unsupported syntax/dialects.
    - Ambiguous or approximate lineage (e.g. unresolved `*`).
  - A basic **summary**:
    - Statement counts, table counts, column counts, issue counts, flags.

- **UI / Viewer**
  - React-based components for:
    - Graph visualization.
    - Column-level detail panel.
    - SQL highlighting.
  - Example Web SPA that demonstrates:
    - Input SQL textarea.
    - Dialect dropdown.
    - Optional schema JSON upload.
    - "Analyze" action that visualizes lineage.

### 3.3 Schema Metadata Contract

- **Canonical structure**
  - `SchemaMetadata` now carries explicit `catalog`, `schema`, and `name` components per table rather than relying on a single opaque string key.
  - Optional `defaultCatalog`, `defaultSchema`, and `searchPath` hints let the engine resolve unqualified identifiers consistently with warehouse behavior.
- **Case sensitivity**
  - Callers can set `caseSensitivity` to `dialect` (default), `lower`, `upper`, or `exact`.
  - When set to `dialect`, the engine applies the per-dialect normalization rules documented in `implementation-decisions.md`.
- **Table entries**
  - Each table entry specifies `{ catalog?, schema?, name, columns: ColumnSchema[] }`.
  - Column names are stored exactly as provided and normalized in the engine according to the same rules as SQL identifiers.
- **Backwards compatibility**
  - The wrapper still accepts the old `Record<string, TableSchema>` form, but immediately rewrites it into the structured representation so we maintain a single canonical form internally.

### 3.2 Non-Functional Scope

- **Pure client-side operation**
  - No HTTP calls or remote execution in the core engine.
  - Suitable for embedding in privacy-sensitive contexts.

- **Performance**
  - Handle realistic analytics workloads:
    - Single queries up to a few thousand lines.
    - Simple multi-file projects in the example app (explicit large-project support can come later).
    - Cross-statement aggregation must remain interactive (global graph assembly targets sub-100 ms for typical multi-statement scripts).
  - Parsing and lineage computation executed off the main UI thread in host apps (via Web Worker helper).

- **Portability**
  - Browser environments (primary target).
  - Node.js / Deno type environments are nice-to-have but not required in v1 (architecture shouldn't block it).

## 4. Out-of-Scope (for MVP)

- Automatic live connection to warehouses or catalogs (no direct DB connections).
- Non-SQL lineage (e.g., Python dataframes, Spark pipelines, etc.).
- Preprocessing or rendering of templated SQL (dbt macros, Jinja, Dagster asset definitions, etc.). The engine accepts only raw SQL text provided by the caller.
- Full metadata catalog with governance features.
- Complex access control / tenant isolation (beyond host app's concern).
- Database query execution or validation against real schemas (beyond simple schema JSON).

## 5. Key Technology Choices

- **Language for core engine:** Rust.
- **SQL parser:** `sqlparser-rs` (DataFusion SQL parser) compiled to WASM.
- **Intermediary representation:** Custom **lineage graph model** (nodes, edges, issues).
- **Binary format:** WebAssembly module with JSON-in/JSON-out boundary.
- **Frontend stack:** TypeScript + React for the viewer and demo app.
- **Graph rendering:** Use a JS graph library (e.g., ElkJS/Cytoscape/Dagre). Exact choice is implementation detail; spec only requires a pluggable visualization approach.
- **Packaging:**
  - `flowscope-core` (Rust crate).
  - `flowscope-wasm` (Rust crate providing WASM exports).
  - `@pondpilot/flowscope-core` (NPM package: WASM loader + TS wrapper).
  - `@pondpilot/flowscope-react` (NPM package: React components).
  - `/examples/web-demo` (SPA).
