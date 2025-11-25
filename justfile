# FlowScope Justfile
# Common commands for building, testing, and development

# Default recipe - show available commands
default:
    @just --list

# Build all components
build: build-rust build-ts

# Build Rust workspace
build-rust:
    cargo build --workspace

# Build Rust workspace in release mode
build-rust-release:
    cargo build --release --workspace

# Build WASM module and TypeScript packages
build-wasm:
    ./scripts/build-rust.sh

# Build TypeScript packages
build-ts:
    yarn build:ts

# Run all tests
test: test-rust test-ts

# Run all Rust tests
test-rust:
    cargo test --workspace

# Run lineage engine tests specifically
test-lineage:
    cargo test -p flowscope-core --test lineage_engine

# Run lineage engine tests with output
test-lineage-verbose:
    cargo test -p flowscope-core --test lineage_engine -- --nocapture

# Run specific lineage engine test by name
test-lineage-filter PATTERN:
    cargo test -p flowscope-core --test lineage_engine {{PATTERN}}

# Run flowscope-core unit tests
test-core:
    cargo test -p flowscope-core

# Run TypeScript tests
test-ts:
    yarn test:ts

# Generate test coverage report
coverage:
    ./scripts/generate_test_coverage.sh

# Run development server
dev:
    cd app && yarn dev

# Run linters
lint: lint-rust lint-ts

check-schema:
    cargo test -p flowscope-core --test schema_guard --locked
    cd packages/core && yarn test schema-compat.test.ts --silent

# Run Rust clippy
lint-rust:
    cargo clippy --workspace -- -D warnings

# Run TypeScript linters
lint-ts:
    yarn lint

# Fix TypeScript lint issues
lint-fix:
    yarn workspaces run lint:fix

# Run TypeScript type checking
typecheck:
    yarn typecheck

# Format code
fmt: fmt-rust fmt-ts

# Format Rust code
fmt-rust:
    cargo fmt --all

# Check Rust formatting
fmt-check-rust:
    cargo fmt --all -- --check

# Format TypeScript code
fmt-ts:
    yarn workspaces run prettier:write || yarn prettier --write "**/*.{ts,tsx,js,jsx,json,md}"

# Clean build artifacts
clean:
    cargo clean
    yarn workspaces run clean || true
    rm -rf node_modules
    rm -rf packages/*/node_modules
    rm -rf app/node_modules

# Install dependencies
install:
    yarn install

# Full CI workflow - lint, typecheck, test
ci: lint typecheck test

# Full development setup - install deps and build
setup: install build

# Watch and rebuild on changes (Rust)
watch:
    cargo watch -x "build --workspace"

# Watch and run tests on changes
watch-test:
    cargo watch -x "test --workspace"

# Watch and run lineage tests on changes
watch-lineage:
    cargo watch -x "test -p flowscope-core --test lineage_engine"

# Run Rust tests in release mode (faster execution)
test-rust-release:
    cargo test --workspace --release

# Build and run the app
run: build dev

# Check everything is working (quick validation)
check: fmt-check-rust lint typecheck test-rust check-schema

# All checks (Rust + TS + schema compatibility)
check-all:
    cargo test --workspace --locked
    yarn workspace @pondpilot/flowscope-core test --silent
    just check-schema

# Deploy app to Cloudflare Pages
deploy: build-wasm build-ts
    cd app && yarn build
    wrangler pages deploy app/dist --project-name flowscope-app
