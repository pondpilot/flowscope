# Workspace Structure

This document defines the **monorepo layout**, **build organization**, and **package relationships** for the FlowScope project.

## Directory Layout

```
flowscope/                          # Repository root
├── .github/                        # GitHub configuration
│   ├── workflows/                  # CI/CD workflows (GitHub Actions)
│   │   ├── ci.yml                  # Main CI pipeline
│   │   ├── release.yml             # Release automation
│   │   └── test.yml                # Test matrix
│   └── ISSUE_TEMPLATE/             # Issue templates
│       ├── bug_report.md
│       ├── feature_request.md
│       └── dialect_support.md
│
├── crates/                         # Rust workspace
│   ├── flowscope-core/             # Core lineage engine (Rust library)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── parser/             # SQL parsing layer
│   │   │   ├── lineage/            # Lineage computation
│   │   │   ├── schema/             # Schema metadata handling
│   │   │   ├── types.rs            # Core data structures
│   │   │   └── error.rs            # Error types
│   │   ├── tests/
│   │   │   ├── fixtures/           # Test data (shared with JS/TS tests)
│   │   │   │   ├── sql/
│   │   │   │   │   ├── postgres/
│   │   │   │   │   ├── snowflake/
│   │   │   │   │   ├── bigquery/
│   │   │   │   │   └── generic/
│   │   │   │   ├── schemas/        # JSON schema samples
│   │   │   │   └── golden/         # Golden output snapshots
│   │   │   ├── unit/
│   │   │   └── integration/
│   │   ├── Cargo.toml
│   │   └── README.md
│   │
│   └── flowscope-wasm/             # WASM bindings
│       ├── src/
│       │   ├── lib.rs              # WASM exports
│       │   └── bridge.rs           # JSON serialization boundary
│       ├── Cargo.toml
│       └── README.md
│
├── packages/                       # NPM workspace
│   ├── core/                       # @pondpilot/flowscope-core
│   │   ├── src/
│   │   │   ├── index.ts            # Main entry point
│   │   │   ├── wasm-loader.ts      # WASM initialization
│   │   │   ├── analyzer.ts         # analyzeSql implementation
│   │   │   ├── worker.ts           # Web Worker helper
│   │   │   ├── types/              # TypeScript type definitions
│   │   │   │   ├── index.ts
│   │   │   │   ├── request.ts
│   │   │   │   ├── response.ts
│   │   │   │   └── generated/      # Auto-generated from Rust
│   │   │   └── utils/
│   │   ├── dist/                   # Build output (not in git)
│   │   │   ├── index.js
│   │   │   ├── index.d.ts
│   │   │   ├── worker.js
│   │   │   └── wasm/
│   │   │       ├── flowscope_bg.wasm
│   │   │       └── flowscope.js
│   │   ├── tests/
│   │   │   ├── unit/
│   │   │   └── integration/
│   │   ├── package.json
│   │   ├── tsconfig.json
│   │   └── README.md
│   │
│   └── react/                      # @pondpilot/flowscope-react
│       ├── src/
│       │   ├── index.ts            # Main exports
│       │   ├── components/
│       │   │   ├── LineageExplorer/
│       │   │   ├── GraphView/
│       │   │   ├── ColumnPanel/
│       │   │   ├── SqlView/
│       │   │   └── IssuesPanel/
│       │   ├── hooks/              # React hooks
│       │   ├── styles/             # Tailwind styles
│       │   └── utils/
│       ├── dist/                   # Build output (not in git)
│       ├── tests/
│       │   └── unit/
│       ├── package.json
│       ├── tsconfig.json
│       ├── tailwind.config.js
│       └── README.md
│
├── examples/                       # Example applications
│   └── web-demo/                   # Browser-based demo app
│       ├── src/
│       │   ├── App.tsx
│       │   ├── main.tsx
│       │   ├── components/
│       │   └── styles/
│       ├── public/
│       ├── tests/
│       │   └── e2e/                # Playwright E2E tests
│       ├── index.html
│       ├── package.json
│       ├── vite.config.ts
│       └── README.md
│
├── docs/                           # Documentation (this spec)
│   ├── README.md
│   ├── architecture-overview.md
│   ├── core-engine-spec.md
│   ├── api-types.md
│   ├── wasm-and-js-layer.md
│   ├── product-scope.md
│   ├── ui-and-integrations.md
│   ├── testing-and-quality.md
│   ├── implementation-decisions.md
│   ├── roadmap-and-phasing.md
│   ├── design-gaps-and-questions.md
│   ├── error-codes.md              # Error code catalog
│   └── workspace-structure.md      # This document
│
├── scripts/                        # Build and utility scripts
│   ├── build-all.sh                # Build entire workspace
│   ├── test-all.sh                 # Run all tests
│   ├── codegen.sh                  # Generate types from Rust
│   └── release.sh                  # Release orchestration
│
├── .gitignore
├── .prettierrc
├── .eslintrc.js
├── Cargo.toml                      # Rust workspace manifest
├── package.json                    # NPM workspace root
├── tsconfig.base.json              # Shared TypeScript config
├── LICENSE                         # Apache 2.0
├── README.md                       # Project overview
├── CONTRIBUTING.md                 # Contributor guide
├── SECURITY.md                     # Security policy
└── TODO.md                         # Implementation roadmap
```

## Workspace Configuration

### Rust Workspace (`Cargo.toml`)

```toml
[workspace]
members = [
    "crates/flowscope-core",
    "crates/flowscope-wasm",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
authors = ["PondPilot Team"]
edition = "2021"
license = "Apache-2.0"
repository = "https://github.com/pondpilot/flowscope"

[workspace.dependencies]
sqlparser = "0.50"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
schemars = { version = "0.8", features = ["preserve_order"] }
```

### NPM Workspace (`package.json`)

```json
{
  "name": "flowscope",
  "private": true,
  "workspaces": [
    "packages/*",
    "examples/*"
  ],
  "scripts": {
    "build": "yarn build:rust && yarn build:ts",
    "build:rust": "./scripts/build-rust.sh",
    "build:ts": "yarn workspaces run build",
    "test": "yarn test:rust && yarn test:ts",
    "test:rust": "cargo test --workspace",
    "test:ts": "yarn workspaces run test",
    "test:integration": "yarn workspace web-demo test:integration",
    "lint": "yarn workspaces run lint",
    "typecheck": "yarn workspaces run typecheck",
    "codegen": "./scripts/codegen.sh",
    "clean": "yarn workspaces run clean && cargo clean"
  },
  "devDependencies": {
    "@types/node": "^18.0.0",
    "typescript": "^5.0.0",
    "prettier": "^3.0.0",
    "eslint": "^8.0.0"
  },
  "packageManager": "yarn@1.22.19",
  "engines": {
    "node": ">=18.0.0",
    "yarn": ">=1.22.0"
  }
}
```

### TypeScript Base Config (`tsconfig.base.json`)

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "lib": ["ES2020", "DOM"],
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true
  }
}
```

## Build Pipeline

### Phase 1: Build Rust Core

```bash
cd crates/flowscope-core
cargo build --release
cargo test
```

### Phase 2: Build WASM

```bash
cd crates/flowscope-wasm
wasm-pack build --target web --out-dir ../../packages/core/src/wasm
```

### Phase 3: Build TypeScript Packages

```bash
# Build @pondpilot/flowscope-core
cd packages/core
yarn build

# Build @pondpilot/flowscope-react
cd ../react
yarn build
```

### Phase 4: Build Example App

```bash
cd examples/web-demo
yarn build
```

## Package Dependencies

```
flowscope-core (Rust crate)
    ↓
flowscope-wasm (Rust crate)
    ↓ (compiles to)
@pondpilot/flowscope-core (NPM)
    ↓ (consumed by)
@pondpilot/flowscope-react (NPM)
    ↓ (consumed by)
web-demo (example app)
```

## Published Artifacts

### NPM Packages

**@pondpilot/flowscope-core**
- Entry: `dist/index.js`
- Types: `dist/index.d.ts`
- Includes: WASM binary in `dist/wasm/`
- Size target: < 2 MB (including WASM)

**@pondpilot/flowscope-react**
- Entry: `dist/index.js`
- Types: `dist/index.d.ts`
- Peer deps: React 18+, @pondpilot/flowscope-core
- Size target: < 500 KB

### Rust Crates

**flowscope-core**
- Published to crates.io
- Used for native integrations

**flowscope-wasm**
- Not published (internal build artifact)

## Development Workflow

### Initial Setup

```bash
# Clone repository
git clone https://github.com/pondpilot/flowscope.git
cd flowscope

# Install dependencies
yarn install

# Build everything
yarn build

# Run tests
yarn test
```

### Development Loop

```bash
# Watch mode for TypeScript changes
cd packages/core
yarn dev

# In another terminal, rebuild WASM when Rust changes
cd crates/flowscope-wasm
cargo watch -x 'build --target wasm32-unknown-unknown'

# Run demo app with hot reload
cd examples/web-demo
yarn dev
```

### Running Tests

```bash
# All tests
yarn test

# Rust unit tests only
cargo test --workspace

# TypeScript unit tests only
yarn test:unit

# Integration tests (Playwright)
yarn test:integration

# E2E tests for demo app
cd examples/web-demo
yarn test:e2e
```

## Version Management

All packages follow **synchronized semantic versioning**:

- Major versions: Breaking API changes
- Minor versions: New features (backward compatible)
- Patch versions: Bug fixes

Version bumps coordinated across:
- Both Rust crates
- Both NPM packages
- Demo app (uses published packages)

## CI/CD Integration

GitHub Actions workflows orchestrate:

1. **Lint & Format Check**
   - Rust: `cargo fmt --check`, `cargo clippy`
   - TypeScript: `yarn lint`, `yarn prettier --check`

2. **Build**
   - Rust crates (debug + release)
   - WASM module
   - TypeScript packages
   - Demo app

3. **Test**
   - Rust unit tests
   - Rust integration tests
   - TypeScript unit tests (Jest)
   - TypeScript integration tests (Playwright)
   - E2E tests on demo app

4. **Publish** (on release)
   - Rust crates to crates.io
   - NPM packages to npm registry
   - Demo app to Vercel/Netlify

---

Last Updated: 2025-11-20
