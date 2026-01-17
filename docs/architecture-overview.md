# Architecture Overview

## Component Summary

FlowScope is a Rust + TypeScript monorepo with a WASM boundary and optional UI layers.

1. **Core Engine (Rust)**
   - `flowscope-core`: SQL parsing and lineage analysis.
   - `flowscope-wasm`: WASM bindings exposing JSON APIs.
   - `flowscope-export`: Export helpers (DuckDB/SQL text) used by CLI and JS tooling.
   - `flowscope-cli`: CLI wrapper around the core engine.

2. **JS/TS Runtime Layer**
   - `@pondpilot/flowscope-core`: TypeScript API + WASM loader.

3. **UI & Integrations**
   - `@pondpilot/flowscope-react`: React visualization components.
   - `app/`: Demo Vite app.
   - `vscode/`: VS Code extension + webview UI.

## Data Flow

```text
[Host App] --(SQL + schema + options)--> [@pondpilot/flowscope-core]
    --(JSON)--> [flowscope-wasm]
        --(Rust analysis)--> [flowscope-core]
        --(JSON result)--> [@pondpilot/flowscope-core]
            --(typed result)--> [Host App / @pondpilot/flowscope-react]
```

## Responsibilities

### Core Engine (`flowscope-core`)
- Parses SQL with `sqlparser-rs`.
- Produces statement-level lineage graphs and a global graph.
- Emits structured issues for unsupported syntax and partial lineage.
- Uses dialect semantics generated from `crates/flowscope-core/specs/dialect-semantics/`.

### WASM Boundary (`flowscope-wasm`)
- Bridges JSON request/response payloads.
- Avoids exposing Rust internals to JS consumers.

### TypeScript Wrapper (`@pondpilot/flowscope-core`)
- Initializes WASM modules.
- Provides `analyzeSql`, `splitStatements`, and completion APIs.
- Exposes strongly typed results and issue codes.

### UI Layer (`@pondpilot/flowscope-react`)
- Renders lineage graphs and diagnostics.
- Consumes typed results without re-running analysis.

## Related Docs

- API shape: `api-types.md`
- Engine behavior: `core-engine-spec.md`
- Workspace layout: `workspace-structure.md`
