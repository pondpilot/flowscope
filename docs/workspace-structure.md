# Workspace Structure

This document captures the current FlowScope monorepo layout and the build organization used today.

## Directory Layout

```text
flowscope/
├── .github/
│   └── workflows/
│       ├── ci.yml
│       └── publish-core.yml
├── app/                          # Demo Vite app
├── crates/                       # Rust workspace
│   ├── flowscope-core/           # Core lineage engine
│   ├── flowscope-wasm/           # WASM bindings
│   ├── flowscope-cli/            # CLI
│   └── flowscope-export/         # Export helpers
├── packages/                     # NPM workspace
│   ├── core/                     # @pondpilot/flowscope-core
│   └── react/                    # @pondpilot/flowscope-react
├── vscode/                       # VS Code extension
│   └── webview-ui/               # Webview frontend
├── docs/                         # Documentation
├── scripts/                      # Build + tooling scripts
│   ├── build-rust.sh
│   ├── update_api_schema.cjs
│   ├── generate_test_coverage.sh
│   └── check_schema_sync.sh
├── justfile                      # Task runner entry point
├── Cargo.toml                    # Rust workspace manifest
├── package.json                  # Yarn workspace root
├── tsconfig.base.json            # Shared TS config
├── README.md                     # Project overview
└── CONTRIBUTING.md               # Contributor guide
```

## Build Entry Points

The project uses `just` as the task runner. Key targets:

- `just build` (build WASM + TS packages)
- `just build-rust` (Rust workspace)
- `just build-wasm` (WASM via `scripts/build-rust.sh`)
- `just build-ts` (TypeScript packages)
- `just dev` (demo app dev server)
- `just test` (Rust + TS tests)

See `justfile` for the full command list.

## Package Relationships

```text
flowscope-core (Rust)
    ↓
flowscope-wasm (Rust)
    ↓ (WASM artifacts)
@pondpilot/flowscope-core (TS)
    ↓
@pondpilot/flowscope-react (TS)
    ↓
app/ and vscode/webview-ui
```

## Notes

- The demo app and VS Code webview currently define no tests.
- API schema snapshots live in `docs/api_schema.json` and are validated by `just check-schema`.
