# WASM & JS/TS Layer Spec

This document defines how the **Rust core engine** is exposed to **JavaScript/TypeScript** as a WebAssembly module, and how the **NPM package** (`@pondpilot/flowscope-core`) behaves.

## 1. WASM Module (`flowscope-wasm`)

### 1.1 Responsibilities

- Wrap `flowscope-core` and:
  - Accept a serialized request object (JSON).
  - Invoke the core analysis.
  - Return a serialized result object (JSON).
- Expose minimal, stable functions suitable for binding with `wasm-bindgen`.

### 1.2 Public WASM Functions (Conceptual)

A single main function is sufficient for MVP:

- `analyze_sql(request_json: string) -> string`
  - `request_json`:
    - JSON string representing an `AnalyzeRequest` (see `core-engine-spec.md`).
  - Return:
    - JSON string representing an `AnalyzeResult`.
  - Error cases:
    - If JSON parsing fails for the request:
      - Return a JSON string representing an `AnalyzeResult` with an appropriate error issue.
      - Do **not** throw or abort at the WASM boundary where possible.

The actual Rust signature will differ (e.g., using `JsValue`), but this is the conceptual contract.

## 2. JS/TS Wrapper Package (`@pondpilot/flowscope-core`)

### 2.1 Responsibilities

- Manage **loading of the WASM binary**.
- Provide a strongly typed **TypeScript API** to callers.
- Hide JSON string conversions and low-level WASM details.
- Provide an optional **Web Worker helper** for off-main-thread execution.

### 2.2 Package Contents

- TypeScript source code for:
  - WASM loader.
  - Typed request/response interfaces.
  - Public `analyzeSql` function.
- Bundled WASM binary and glue code.
- Type definitions (`.d.ts`) for all exports.

### 2.3 Initialization

The package should expose an initialization function that:

- Ensures the WASM binary is loaded and ready.
- Is safe to call multiple times (idempotent).

Conceptual behavior:

- `init(options?)`:\n  - Options may include:
    - Custom URL or loader for the WASM file.
  - Resolves when WASM is ready.
- `analyzeSql(...)` must:
  - Either require `init` to be called explicitly,
  - Or call `init` lazily on first use (and internally await readiness).

Implementation detail (explicit vs lazy) can be chosen for developer ergonomics, but the spec requires:

- `analyzeSql` must either:
  - Fail with a clear error if init was never invoked, or
  - Perform a robust lazy initialization internally.

### 2.4 Public API (Conceptual)

Key functions:

1. **Initialization**
   - `initWasm(options?) -> Promise<void>`

2. **Analysis**
   - `analyzeSql(request) -> Promise<result>`

The **request** shape mirrors the core engine's conceptual `AnalyzeRequest`:

- `sql: string` (fully rendered, no templating)
- `dialect: 'generic' | 'postgres' | 'snowflake' | 'bigquery'`
- `options?: { enableColumnLineage?: boolean }`
- `schema?: SchemaMetadata` where `SchemaMetadata` is the structured form defined in `api-types.md` (legacy map inputs are still accepted and rewritten)

The **result** shape mirrors `AnalyzeResult`:

- `statements: [...]`
- `globalLineage: { nodes: [...], edges: [...] }`
- `issues: [...]`
- `summary: {...}`

(Exact interface details are defined in TypeScript within this package.)

### 2.5 Error Handling

Errors can occur at several levels:

- WASM loading:
  - Network failures, incompatible environment, etc.
- Request serialization:
  - Invalid request object, fields of wrong type.
- Engine-level:
  - Parse errors, unsupported syntax, etc.

The wrapper should:

- Distinguish between:
  - **Technical errors** (e.g. WASM couldn't load): reject the Promise.
  - **Analysis-level errors** (e.g. SQL parse failure): succeed with an `AnalyzeResult` that contains error issues.

Consumers should be able to:

- Catch technical failures using Promise rejection.
- Inspect `result.summary.has_errors` and `result.issues` for analysis failures.

## 3. Web Worker Helper

### 3.1 Rationale

Parsing and lineage computation for large queries can be CPU-intensive. To prevent UI jank in browser contexts, the package should provide a helper abstraction compatible with a **Web Worker**.

### 3.2 Concept

- Provide a helper factory:
  - `createLineageWorker(config)`, which:
    - Spawns a dedicated worker.
    - Loads the same `@lineage/core` analysis logic inside the worker context.
    - Exposes a `worker.analyzeSql(request)` function that:
      - Posts a message to the worker.
      - Resolves when the worker returns a result.

- The worker script:
  - Contains bootstrap code to:
    - Initialize WASM in the worker.
    - Listen for inbound messages containing requests.
    - Run `analyzeSql` and post back results.

### 3.3 Message Protocol

Worker messages should be simple JSON-friendly objects:

- Request:
  - Unique request ID.
  - `AnalyzeRequest` payload.

- Response:
  - The same request ID.
  - Either:
    - `AnalyzeResult`, or
    - Error descriptor for technical failures.

Implementations can use a small internal dispatcher to match responses to Promises.

## 4. Large Payloads & Cancellation

### 4.1 Payload Transport

- Use `WebAssembly.instantiateStreaming` (where available) plus `Response.arrayBuffer()` fallback to avoid double-buffering large modules.
- When posting requests/results to a worker:
  - Prefer `structuredClone` with `Transferable` `ArrayBuffer`s so SQL text and result JSON move without copies.
  - For very large SQL bodies (>5 MB), chunk the payload into transferable blocks and reconstruct inside the worker.
  - Document an upper bound (currently 10 MB) but keep the protocol extensible for streaming improvements.

### 4.2 Cancellation

- All public APIs accept an optional `AbortSignal`.
- Worker helper listens for `abort` and:
  - Marks the in-flight request cancelled.
  - Drops the Promise with a DOMException.
  - Sends a cancellation hint to the WASM module (no-op in MVP, hook for future cooperative cancellation).

### 4.3 Progress & Partial Results

- WASM exports an optional `analyze_sql_progress` hook that periodically posts lightweight `ProgressEvent`s (percent statements processed + byte offsets) back to the worker host.
- The worker relays these events so host apps can show loading indicators and optionally short-circuit long analyses.

### 4.4 Memory Growth

- Stick with wasm-bindgen defaults initially, but surface `initialMemory` / `maximumMemory` overrides via `initWasm` options so heavy hosts (e.g., VS Code) can bump limits.
- Monitor memory usage via simple telemetry counters included in development builds.

### 4.5 Tracing (optional)

- When built with the `tracing` feature, the WASM module exposes `enable_tracing` (surfaced in TS via `initWasm({ enableTracing: true })`).
- Default is off; enabling forwards tracing spans to the browser console (via `tracing-wasm`).

## 5. Environment Assumptions

### 4.1 Browser

- `fetch` or equivalent is available for loading the WASM binary.
- `WebAssembly` APIs are available.
- Optional `Worker` support for concurrency.

### 4.2 Node.js / Deno (future consideration)

The architecture should not **hard-code** browser-only globals (e.g., `window`). Instead:

- Environment-specific logic should be:
  - Minimal and isolated.
- For MVP, browser support is required; Node/Deno can be added later by:
  - Supplying alternative WASM loading strategies (e.g., `fs` in Node).

## 6. Versioning & API Stability

- The JSON request/response format at the WASM boundary should be **backward compatible** across minor versions:
  - Additive changes only (new fields, new node/edge types) with sensible defaults.
- The TypeScript interfaces in `@lineage/core` should follow semantic versioning:
  - **Minor versions**: backward-compatible additions.
  - **Major versions**: breaking changes to public interfaces.
- Version handshake:
  - `lineage-wasm` exposes a `get_version()` export (semantic version string) that `initWasm` calls before running any analyses.
  - `@lineage/core` publishes the engine versions it is compatible with and enforces that the loaded WASM's **major** version matches; mismatches throw a descriptive init error, while minor/patch differences raise a warning issue in the first `AnalyzeResult`.
