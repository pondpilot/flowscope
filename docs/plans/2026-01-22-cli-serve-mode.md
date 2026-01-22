# Plan: CLI --serve Mode with Embedded Web UI

Add a `--serve` flag to `flowscope-cli` that starts a local HTTP server serving the embedded web UI with a REST API backend.

## Validation Commands

- `cargo build -p flowscope-cli --features serve` — Build CLI with serve feature
- `cargo test -p flowscope-cli --features serve` — Run CLI tests
- `cd app && yarn build` — Build app for embedding
- `cd app && yarn test:unit` — Run frontend unit tests
- `./target/debug/flowscope --serve --watch ./examples` — Manual smoke test

## Summary

**What:** Single-binary deployment where `flowscope --serve --watch ./sql/` starts a web server with embedded UI.

**Key decisions:**
- Asset embedding via `rust-embed` (single executable, no external files)
- REST/JSON API (same format as WASM for easy switching)
- CLI watches directories (source of truth for files)
- DB integration via existing `--metadata-url` flag

## API Design

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check with version |
| `/api/analyze` | POST | Main lineage analysis |
| `/api/completion` | POST | Code completion items |
| `/api/split` | POST | Split SQL statements |
| `/api/files` | GET | List watched files with content |
| `/api/schema` | GET | Schema metadata (from DB or DDL) |
| `/api/export/:format` | POST | Export to json/mermaid/html/csv/xlsx |
| `/api/config` | GET | Server config (dialect, watch dirs) |
| `/*` | GET | Static assets (fallback to index.html) |

---

### Task 1: Add serve feature and dependencies to CLI

- [x] Add `serve` feature flag to `crates/flowscope-cli/Cargo.toml`
- [x] Add dependencies: `axum`, `tower-http`, `rust-embed`, `notify`, `tokio`
- [x] Gate dependencies with `optional = true` under serve feature

**Files:** `crates/flowscope-cli/Cargo.toml`

---

### Task 2: Add CLI arguments for serve mode

- [x] Add `--serve` flag to start server
- [x] Add `--port` with default 3000
- [x] Add `--watch` for directories to watch (repeatable)
- [x] Add `--open` to auto-open browser
- [x] Gate all serve args with `#[cfg(feature = "serve")]`

**Files:** `crates/flowscope-cli/src/cli.rs`

---

### Task 3: Create server module structure

- [x] Create `crates/flowscope-cli/src/server/mod.rs` — exports and main server loop
- [x] Create `crates/flowscope-cli/src/server/assets.rs` — rust-embed static files
- [x] Create `crates/flowscope-cli/src/server/state.rs` — shared AppState
- [x] Create `crates/flowscope-cli/src/server/api.rs` — REST handlers
- [x] Create `crates/flowscope-cli/src/server/watcher.rs` — file system watcher
- [x] Add `mod server` (gated) to `main.rs`

**Files:** `crates/flowscope-cli/src/server/*.rs`, `crates/flowscope-cli/src/main.rs`

---

### Task 4: Implement asset embedding

- [x] Define `WebAssets` struct with `#[derive(RustEmbed)]`
- [x] Configure folder path to `../../app/dist/`
- [x] Add include patterns for html, js, css, wasm, svg, png, ico
- [x] Implement `static_files` handler serving embedded assets
- [x] Handle SPA fallback to index.html for non-asset routes

**Files:** `crates/flowscope-cli/src/server/assets.rs`

---

### Task 5: Implement AppState and config

- [ ] Define `AppState` with `RwLock<Vec<FileSource>>` for files
- [ ] Add `RwLock<Option<SchemaMetadata>>` for schema
- [ ] Add `ServerConfig` struct (dialect, watch_dirs, metadata_url, port)
- [ ] Implement `AppState::new()` that loads initial files and schema

**Files:** `crates/flowscope-cli/src/server/state.rs`

---

### Task 6: Implement API handlers

- [ ] `GET /api/health` — return status and version
- [ ] `POST /api/analyze` — call `flowscope_core::analyze()`, return JSON
- [ ] `POST /api/completion` — call completion endpoint
- [ ] `POST /api/split` — call statement splitting
- [ ] `GET /api/files` — return watched files with content
- [ ] `GET /api/schema` — return schema metadata
- [ ] `POST /api/export/:format` — call export functions
- [ ] `GET /api/config` — return server configuration

**Files:** `crates/flowscope-cli/src/server/api.rs`

---

### Task 7: Implement file watcher

- [ ] Use `notify` crate to watch directories
- [ ] Filter for `.sql` files
- [ ] Debounce changes (100ms)
- [ ] Update `AppState::files` on changes
- [ ] Log file changes to console

**Files:** `crates/flowscope-cli/src/server/watcher.rs`

---

### Task 8: Wire up server in main.rs

- [ ] Check `args.serve` flag in main
- [ ] Build `ServerConfig` from CLI args
- [ ] Create tokio runtime and call `server::run_server()`
- [ ] Print server URL on startup
- [ ] Optionally open browser with `--open` flag

**Files:** `crates/flowscope-cli/src/main.rs`

---

### Task 9: Frontend backend detection

- [ ] Create `app/src/lib/backend-adapter.ts` with adapter interface
- [ ] Implement `RestBackendAdapter` using fetch
- [ ] Implement `WasmBackendAdapter` wrapping existing worker
- [ ] Add `createBackendAdapter()` factory with health check fallback
- [ ] Update `useAnalysis.ts` to use adapter pattern

**Files:** `app/src/lib/backend-adapter.ts`, `app/src/hooks/useAnalysis.ts`

---

### Task 10: Frontend file loading from backend

- [ ] Add `useBackendFiles()` hook to fetch `/api/files`
- [ ] Integrate with ProjectProvider when in backend mode
- [ ] Files from backend are read-only in UI
- [ ] Schema comes from `/api/schema` instead of local DDL editor

**Files:** `app/src/hooks/useBackendFiles.ts`, `app/src/lib/project-store.tsx`

---

### Task 11: Build pipeline integration

- [ ] Add `build-cli-serve` target to justfile
- [ ] Ensure app builds before CLI with serve feature
- [ ] Add CI job for building/testing serve feature
- [ ] Document build order in AGENTS.md

**Files:** `justfile`, `.github/workflows/ci.yml`, `AGENTS.md`

---

### Task 12: Write tests

- [ ] Unit tests for API handlers with mock state
- [ ] Integration test: server startup and health endpoint
- [ ] Integration test: analyze endpoint with sample SQL
- [ ] Integration test: file watcher triggers update

**Files:** `crates/flowscope-cli/tests/serve_*.rs`

---

### Task 13: Documentation and README

- [ ] Add --serve usage examples to CLI README
- [ ] Document serve mode in main README
- [ ] Add troubleshooting section (port conflicts, permissions)

**Files:** `crates/flowscope-cli/README.md`, `README.md`
