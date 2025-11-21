# Implementation Decisions

This document captures key technical decisions made during the design phase to guide implementation.

## Build & Tooling

### Rust/WASM Build Pipeline
- **Tool**: wasm-pack (standard)
- **Rationale**: Standard tool with good defaults and npm integration, well-maintained by the Rust WASM working group

### JavaScript Bundler
- **Tool**: Vite
- **Rationale**: Widely adopted dev server/bundler that outputs both ESM and CJS bundles, integrates cleanly with TypeScript, and plays well with Yarn workspaces

### Monorepo Management
- **Tool**: Yarn workspaces only
- **Rationale**: Simple, native workspace support without extra tooling overhead. Keeps the build pipeline straightforward.

### Package Manager
- **Tool**: Yarn (with workspaces)
- **Note**: Yarn remains the single package manager for all workspaces to avoid duplicate lockfiles

### Runtime Architecture
- **Execution model**: All analyses initiated from browser contexts run inside a dedicated Web Worker; the main thread never calls the WASM module directly.
- **Helper contract**: `@lineage/core` exposes a worker factory used by default by the demo app and recommended for host products; direct, same-thread analysis is reserved for Node/Deno adapters in the future.
- **Benefits**: Guarantees responsive UIs from day one, simplifies cancellation semantics (AbortSignal relayed to the worker), and keeps the transport protocol (chunked ArrayBuffers + progress events) consistent across hosts.

### Version Compatibility Handshake
- **Engine Version Source**: `lineage-wasm` exposes a `get_version()` export that returns the semver of the compiled Rust engine.
- **JS Enforcement**: `@lineage/core` compares that value against its own declared compatible range during `initWasm`; mismatched major versions cause initialization failure, while minor/patch mismatches emit a warning issue.
- **Testing Hook**: Golden regression fixtures record the engine+wrapper version tuple so CI detects accidental drift.

## Frontend Stack

### React Version
- **Version**: React 18+
- **Rationale**: Concurrent features, automatic batching, and modern hooks support

### Graph Visualization
- **Library**: ReactFlow
- **Rationale**: React-native, interactive, good performance, built-in controls for zoom/pan, extensive documentation

### SQL Code Editor (Demo App)
- **Library**: CodeMirror 6
- **Rationale**: Modern, lighter weight than Monaco (~500KB vs ~3MB), good SQL mode available, extensible

### State Management
- **Library**: Zustand (from CLAUDE.md requirements)
- **Use**: For demo app and potentially @lineage/react for component-level state

### Styling
- **Library**: Tailwind CSS (from CLAUDE.md requirements)
- **Use**: All React components and demo app

## Core Engine Behavior

### Node/Edge ID Generation
- **Strategy**: Content-based hash
- **Details**: Hash table/column qualified names and expressions to ensure stability across runs with the same SQL
- **Benefits**: Deterministic, allows diffing between analyses, cache-friendly

### Multi-Statement Error Handling
- **Strategy**: Continue on error
- **Behavior**:
  - Analyze all statements even if some fail
  - Collect errors for each failed statement as issues
  - Return all partial results plus error issues
  - Set `has_errors` flag in summary

### Cross-Statement Lineage Graph
- **Strategy**: Always build a global graph alongside per-statement graphs
- **Implementation**:
  - Canonicalize `(catalog, schema, name, column?)` identifiers for every produced table/column
  - Deduplicate nodes using content-hash IDs derived from canonical identifiers
  - Emit `cross_statement` edges when later statements read earlier results, with producer/consumer statement references
  - Attach `statement_refs` to each global node so UIs can hop between views
- **Result**: Host apps can answer impact-analysis questions ("who uses this temp table later?") without recalculating lineage themselves

### Case Sensitivity in Schema Metadata
- **Strategy**: Dialect-aware
- **Implementation**:
  - Postgres: lowercase normalization
  - Snowflake: uppercase normalization
  - BigQuery: case-sensitive as-is
  - Generic: case-insensitive (lowercase)
- **Note**: Engine normalizes both schema keys and SQL identifiers according to dialect rules

### Schema Metadata Canonicalization
- **Strategy**: Structured representation with defaults
- **Details**:
  - `SchemaMetadata` carries `default_catalog`, `default_schema`, optional `search_path`, and explicit `SchemaTable` entries
  - Legacy `Record<string, TableSchema>` inputs are accepted by the JS wrapper and rewritten before hitting WASM
  - Normalization occurs once inside Rust so subsequent passes always see canonical identifiers

### Analysis Timeouts
- **Decision**: No built-in timeouts
- **Rationale**:
  - Host apps can kill workers if needed
  - Simpler WASM boundary
  - Avoids complexity of cross-platform timeout mechanisms
- **Host App Responsibility**: Applications should implement their own timeout logic if needed, particularly in worker scenarios

## Data Format & Boundaries

### Large Payload Handling
- **Transport**: Use `structuredClone` with Transferables for SQL text and result buffers when communicating with workers to avoid copies
- **Chunking**: Inputs >5 MB are chunked into transferable `ArrayBuffer`s that are reassembled inside the worker before calling WASM
- **Documented Limits**: Publish a soft limit of 10 MB per request/result; emit a warning issue when the payload exceeds this threshold

### Span Representation
- **Format**: Byte offsets (start, end) in the original SQL string
- **Rationale**: Simple, unambiguous, easy to convert to line/col if needed by host apps
- **Optional**: Spans may be null/absent when `sqlparser-rs` doesn't provide location info

### Expression Serialization
- **Format**: Plain text string of the expression
- **Details**:
  - Store the original SQL expression text in full (no length limits)
  - For column-level lineage, also store list of input column IDs
  - No AST serialization (keeps JSON boundary simple)
- **Performance**: Engine should be fast enough to handle large expressions without artificial limits

### WASM Memory Management
- **Strategy**: Rely on wasm-bindgen defaults
- **Note**: For MVP, no explicit memory limits. Provide optional overrides (`initialMemory`, `maximumMemory`) via `initWasm` options for hosts that need tuning (Phase 4)

### Worker Lifecycle
- **Strategy**: Lazy creation, manual destruction
- **Details**:
  - Create worker on first use
  - Reuse for multiple analyses
  - Provide explicit `terminate()` method
  - No worker pooling in MVP (can add later if needed)

### Request Cancellation & Progress
- **Cancellation**: All public APIs accept an optional `AbortSignal`; workers listen for `abort` events, reject the Promise, and send a cancellation flag to WASM (future hook for cooperative aborts)
- **Progress Events**: Worker relays lightweight progress updates (statements processed, bytes parsed) emitted by the Rust engine so host apps can show activity indicators
- **Partial Results**: If cancellation occurs mid-run, the worker returns the partial `AnalyzeResult` accumulated so far plus a `CANCELLED` issue

## Testing

### Test Framework (JS/TS)
- **Unit Tests**: Jest (from CLAUDE.md requirements)
- **Integration Tests**: Playwright (from CLAUDE.md requirements)
- **Coverage**: Must have unit, integration, AND e2e tests (per CLAUDE.md policy)

### Test Fixtures Location
- **Location**: In `crates/flowscope-core/tests/fixtures/`
- **Structure**:
  ```
  crates/flowscope-core/tests/fixtures/
    sql/
      postgres/
      snowflake/
      bigquery/
      generic/
    schemas/
    golden/
  ```
- **Sharing**: JS/TS tests can read these files directly from the Rust crate location

### Golden Snapshot Format
- **Format**: JSON files with full `AnalyzeResult` objects
- **Comparison**: Structural equality with special handling for:
  - Issue counts (may vary slightly)
  - Span positions (if sqlparser-rs behavior changes)
- **Versioning**: Include engine version in golden file names

### Performance Baselines
- **Target**: < 500ms for typical dbt model (100-200 lines)
- **Large Query**: < 2s for 1000+ line query
- **Environment**: Measured in browser (not just Rust benchmarks)

### Dialect Coverage Matrix
- **Matrix**: Maintain `docs/dialect-coverage.md` (Phase 1 deliverable) enumerating syntax features per dialect with status: Supported, Partial, Unsupported
- **Regression Suites**:
  - Each dialect folder in `tests/fixtures/sql/<dialect>` contains representative queries plus golden outputs
  - CI runs dialect-specific suites; regressions block release
- **Gap Handling**:
  - Unsupported constructs must emit `UNSUPPORTED_SYNTAX` with dialect tag
  - Add tracking issue templates referencing the matrix row before downgrading severity

## Browser Compatibility

### Target Browsers
- **Support**: Modern evergreen browsers (Chrome, Edge, Firefox, Safari) last 2 versions
- **Requirements**:
  - WebAssembly support (all modern browsers have this)
  - ES2020+ features (optional chaining, nullish coalescing, etc.)
  - Web Workers (for off-main-thread analysis)

### Polyfills
- **Decision**: No polyfills for MVP
- **Rationale**: Modern evergreen target means native support for all features

## Security & Privacy

### XSS Protection in SQL Display
- **Strategy**: Use React's built-in escaping for text content
- **SQL Highlighting**:
  - For CodeMirror: Uses its own safe rendering
  - For span highlights: Apply styles via CSS classes, not inline HTML

### Schema Data Validation
- **WASM Layer**: Basic JSON schema validation
- **JS Layer**: TypeScript type checking + runtime validation with clear error messages
- **Invalid Schema**: Emit warning issues, continue with best-effort analysis

## Project Metadata

### License
- **License**: Apache 2.0
- **Rationale**: Permissive with explicit patent grant and contributor protections
- **Application**:
  - All Rust crates: Apache 2.0
  - All NPM packages: Apache 2.0
  - Example app: Apache 2.0

### Versioning Strategy
- **Approach**: Semantic versioning (semver) across all packages
- **Coordination**: Major versions synchronized across Rust and JS packages
- **Breaking Changes**: Only in major versions, with migration guides

## Documentation Strategy

### API Documentation
- **Rust**: Standard rustdoc comments
- **TypeScript**: TSDoc comments (JSDoc format)
- **Generation**: Automated from source comments

### User Documentation
- **Location**: `/docs` folder (this spec)
- **Format**: Markdown
- **Publishing**: GitHub Pages or similar (Phase 5)

### Integration Examples
- **Location**: `/examples` folder
- **Coverage**:
  - web-demo (included in repo)
  - Browser extension (Phase 5, separate repo likely)
  - VS Code extension (Phase 5, separate repo likely)

## CI/CD

### CI Platform
- **Platform**: GitHub Actions (assumed, not specified in requirements)
- **Pipelines**:
  - Rust: cargo test, cargo clippy, cargo fmt check
  - WASM: wasm-pack build and test
  - JS/TS: yarn test (unit), yarn test:integration (Playwright), yarn lint, yarn typecheck
  - E2E: Full web-demo test in headless browser

### Release Automation
- **Rust Crates**: cargo publish (manual for MVP, automate in Phase 5)
- **NPM Packages**: npm publish (manual for MVP, automate in Phase 5)
- **Versioning**: Manual version bumps for MVP, consider conventional commits later

## Type System & API Contracts

### Rust-TypeScript Type Synchronization
- **Strategy**: Use `serde` + `schemars` to generate JSON Schema from Rust types
- **Process**:
  1. Define Rust types with `serde` derive macros
  2. Generate JSON Schema using `schemars`
  3. Generate TypeScript types from JSON Schema
  4. Integration tests verify both sides compile to same schema
- **Benefits**: Single source of truth (Rust types), automatic synchronization, compile-time safety

### API Request Structure
```typescript
interface AnalyzeRequest {
  sql: string;
  dialect: 'generic' | 'postgres' | 'snowflake' | 'bigquery'; // Required with default 'generic'
  options?: {
    enableColumnLineage?: boolean; // Default: true in Phase 2, not relevant in Phase 1
  };
  schema?: SchemaMetadata;
}
```
- **Dialect**: Required field with default value of `'generic'` at the TS wrapper level
- **Version Info**: Not included in request/response (keep it simple)
- **Minimal Options**: Only essential flags, no premature configurability

### CTE Handling
- **Phase 1**: Non-recursive CTEs only
- **Recursive CTEs**: Emit warning issue with code `UNSUPPORTED_RECURSIVE_CTE`
- **Scoping**: CTEs scoped to their statement, no cross-statement references
- **Future**: Full recursive CTE support in post-MVP enhancement

### Dialect-Specific Feature Support
- **Strategy**: Work with what `sqlparser-rs` offers out of the box
- **Philosophy**: Don't overengineer dialect-specific handling in MVP
- **Approach**:
  - If `sqlparser-rs` parses it, attempt lineage analysis
  - If we can't analyze a construct, emit warning issue
  - Document known limitations per dialect
- **Examples**:
  - Snowflake `FLATTEN`: Best effort if parsed, otherwise warning
  - BigQuery `UNNEST`: Best effort if parsed, otherwise warning
  - Postgres `LATERAL`: Best effort if parsed, otherwise warning

## Remaining Open Questions

These items are intentionally left flexible or can be decided during implementation:

1. **Exact TypeScript config**: tsconfig.json settings (will follow standard strict config)
2. **WASM bundle size optimization**: Measure in Phase 4, optimize if needed
3. **Column type inference**: Future enhancement, MVP doesn't infer types
4. **Caching helpers in @lineage/core**: Not in MVP, host apps handle caching
5. **Error message localization**: English-only for MVP
6. **Accessibility (a11y) details**: Phase 3 will implement basic ARIA labels, full a11y audit post-MVP
7. **SQL prettification**: Not in scope, use user's original SQL formatting

## Decision Review

This document should be updated when:
- Major technical decisions are made during implementation
- Performance testing reveals need for strategy changes
- User feedback suggests different approaches

Last Updated: 2025-11-20
