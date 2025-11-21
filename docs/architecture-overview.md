# Architecture Overview

## 1. Component Diagram (Conceptual)

Logical components:

1. **Core Engine (Rust)**
   - `flowscope-core`:
     - Responsible for parsing SQL, computing per-statement lineage, and assembling a cross-statement dependency graph.
   - `flowscope-wasm`:
     - Wraps `flowscope-core` and exposes WebAssembly-compatible functions.

2. **JS/TS Runtime Layer**
   - `@pondpilot/flowscope-core`:
     - Loads the WASM module.
     - Exposes a stable, typed analyze function.
     - Optionally manages a Web Worker to keep heavy work off the UI thread.

3. **UI & Integrations**
   - `@pondpilot/flowscope-react`:
     - React components for presenting lineage graph and details.
   - Host applications:
     - Example web SPA for testing and demos.
     - Future integrations (browser extension, editor plugins, etc.).

High-level data flow:

```text
[Host App] --(fully rendered SQL + schema hints)--> [@pondpilot/flowscope-core (TS)] --(JSON)--> [WASM Module (Rust)]
[WASM Module]
  |- per-statement analysis
  |- cross-statement assembler
  v
(JSON result with global graph)
        --(typed result)--> [@pondpilot/flowscope-core] --(global + per-statement views)--> [Host App + @pondpilot/flowscope-react UI]
```

## 2. Core Responsibilities

### 2.1 Core Engine (Rust)

* Translate (SQL string + dialect + schema metadata) → (lineage graph + issues + summary).
* Canonicalize schema metadata using dialect-aware catalog/schema rules so table references can be matched consistently.
* Hide internal AST complexity from JS/TS consumers.
* Maintain deterministic behavior for the same inputs.
* Provide a stable schema for serialized results (JSON-friendly).
* Produce both per-statement lineage and a deduplicated global dependency graph linking statements together.

### 2.2 WASM Wrapper

* Expose a small, stable function surface that:

  * Accepts a serialized request (JSON).
  * Returns a serialized result (JSON).
* Abstract away Rust types so frontends don't have to understand Rust memory management or FFI details.

### 2.3 JS/TS Wrapper

* Manage:

  * WASM module loading and initialization.
  * Conversion between JSON and strongly typed TS objects.
  * Error handling and type checking.
* Provide:

  * A high-level `analyze`-style function to host apps.
  * Optional Web Worker support for concurrency.

### 2.4 UI Layer

* Consume `AnalyzeResult` (typed) and render:

  * Graphs.
  * Column lineages.
  * SQL with highlights.
* Stay strictly presentation-oriented; no lineage logic here.

## 3. Deployment & Packaging Model

### 3.1 Repository Layout (Suggested)

A monorepo-style layout (can be adjusted as needed):

```text
/ (root)
  /crates
    /flowscope-core      # Rust: core logic
    /flowscope-wasm      # Rust: wasm bindings
  /packages
    /core                # NPM: @pondpilot/flowscope-core (TS wrapper + wasm artifacts)
    /react               # NPM: @pondpilot/flowscope-react
  /examples
    /web-demo            # React-based demo app
  /docs                  # This spec
```

Each sub-project should be buildable independently, but CI can orchestrate cross-project builds.

### 3.2 Build Flow (Conceptual)

1. Build Rust core:

   * `flowscope-core` → Rust library.

2. Build WASM:

   * `flowscope-wasm` → WASM binary + JS glue.

3. Package WASM + TS:

   * `@pondpilot/flowscope-core`:

     * Bundles the WASM binary and loader logic.
     * Exposes typed APIs.

4. Build UI:

   * `@pondpilot/flowscope-react` and `web-demo` consume `@pondpilot/flowscope-core`.

### 3.3 Runtime Environments

* **Browser (primary)**

  * Load WASM with `fetch`/`import`.
  * Use Web Worker for heavy analysis.

* **Node.js/Deno (nice-to-have)**

  * `@pondpilot/flowscope-core` should not assume `window`.
  * Node-specific initialization can be introduced later if needed.

## 4. Data Model Overview

### 4.1 Inputs

* SQL text (UTF-8 string) that has already had any templating/macros rendered by the host.
* Dialect identifier (string/enum).
* Optional schema metadata using the canonical `SchemaMetadata` structure:

  * `defaultCatalog`, `defaultSchema`, and `searchPath` hints to emulate database resolution.
  * Explicit table objects with `{ catalog?, schema?, name, columns[] }`.
  * Case-sensitivity directives so identifiers line up with dialect rules.

### 4.2 Outputs

* **Lineage graph per statement**:

  * Nodes: tables, CTEs, columns, etc.
  * Edges: data flow, derivation, ownership.

* **Global lineage graph**:

  * Deduplicated nodes/edges across the entire script.
  * Cross-statement edges explicitly linking statement outputs to downstream consumers.
  * Statement reference metadata so UIs can hop between global and local context quickly.

* **Issues**:

  * List of problems, warnings, and notes for UI to display.

* **Summary**:

  * Basic counts and flags.

Actual shape is defined in `core-engine-spec.md` and mirrored in `wasm-and-js-layer.md`.

## 5. Design Constraints & Tradeoffs

* Use **sqlparser-rs** rather than implementing a new parser:

  * Pros:

    * Mature, multi-dialect, used in production by other engines.
    * Rust-native, easy to compile to WASM.
  * Cons:

    * Syntax coverage is "good but not perfect" across all warehouse dialects.
    * No built-in semantics or type system (we implement lineage semantics ourselves).

* Pure client-side:

  * Strong privacy and convenience.
  * No central server or shared cache; all caching is left to host apps (e.g., storing schema in IndexedDB).

* JSON boundary:

  * Simple, portable, debuggable.
  * Slight overhead vs binary, but acceptable for intended workloads.
