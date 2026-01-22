# FlowScope Justfile
# Common commands for building, testing, and development

set shell := ["bash", "-c"]

# Default recipe - show available commands
default:
    @just --list

# Build all components
build: build-wasm build-ts

# Build Rust workspace
build-rust:
    cargo build --workspace

# Build Rust workspace in release mode
build-rust-release:
    cargo build --release --workspace

# Build CLI binary
build-cli:
    cargo build -p flowscope-cli --release

# Install CLI locally
install-cli:
    cargo install --path crates/flowscope-cli --force

# Run CLI with arguments
cli *ARGS:
    cargo run -p flowscope-cli -- {{ARGS}}

# Run CLI with arguments in release mode
cli-release *ARGS:
    cargo run -p flowscope-cli --release -- {{ARGS}}

# Run CLI tests
test-cli:
    cargo test -p flowscope-cli

# Run CLI integration tests (SQLite + PostgreSQL + MySQL)
test-integration: test-integration-sqlite test-integration-postgres test-integration-mysql

# Run SQLite integration tests (no external dependencies)
test-integration-sqlite:
    cargo test -p flowscope-cli --features integration-tests --test integration sqlite -- --test-threads=1

# Run PostgreSQL integration tests (starts Docker container)
test-integration-postgres: _postgres-start
    #!/usr/bin/env bash
    set -euo pipefail

    # Wait for PostgreSQL to be ready
    echo "Waiting for PostgreSQL to be ready..."
    for i in {1..30}; do
        if docker exec flowscope-test-postgres pg_isready -U flowscope > /dev/null 2>&1; then
            echo "PostgreSQL is ready"
            break
        fi
        if [ $i -eq 30 ]; then
            echo "PostgreSQL failed to start"
            just _postgres-stop
            exit 1
        fi
        sleep 1
    done

    # Create test tables
    docker exec flowscope-test-postgres psql -U flowscope -d flowscope -c "
        DROP TABLE IF EXISTS order_items CASCADE;
        DROP TABLE IF EXISTS orders CASCADE;
        DROP TABLE IF EXISTS users CASCADE;

        CREATE TABLE users (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT UNIQUE
        );

        CREATE TABLE orders (
            id SERIAL PRIMARY KEY,
            user_id INTEGER NOT NULL REFERENCES users(id),
            total NUMERIC(10,2) NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE order_items (
            id SERIAL PRIMARY KEY,
            order_id INTEGER NOT NULL REFERENCES orders(id),
            product_name TEXT NOT NULL,
            quantity INTEGER NOT NULL,
            price NUMERIC(10,2) NOT NULL
        );
    "

    # Run tests
    cargo test -p flowscope-cli --features integration-tests --test integration postgres -- --test-threads=1

    # Stop PostgreSQL
    just _postgres-stop

# Start PostgreSQL container for integration tests
_postgres-start:
    #!/usr/bin/env bash
    set -euo pipefail

    # Stop existing container if running
    docker rm -f flowscope-test-postgres 2>/dev/null || true

    # Start PostgreSQL on port 5433 to avoid conflicts
    docker run -d \
        --name flowscope-test-postgres \
        -e POSTGRES_USER=flowscope \
        -e POSTGRES_PASSWORD=flowscope \
        -e POSTGRES_DB=flowscope \
        -p 5433:5432 \
        postgres:16-alpine

    echo "PostgreSQL container started on port 5433"

# Stop PostgreSQL container
_postgres-stop:
    docker rm -f flowscope-test-postgres 2>/dev/null || true

# Run MySQL integration tests (starts Docker container)
test-integration-mysql: _mysql-start
    #!/usr/bin/env bash
    set -euo pipefail

    # Wait for MySQL to be ready (check with actual user connection, not just ping)
    echo "Waiting for MySQL to be ready..."
    for i in {1..60}; do
        if docker exec flowscope-test-mysql mysql -uflowscope -pflowscope -e "SELECT 1" > /dev/null 2>&1; then
            echo "MySQL is ready"
            break
        fi
        if [ $i -eq 60 ]; then
            echo "MySQL failed to start"
            just _mysql-stop
            exit 1
        fi
        sleep 1
    done

    # Create test tables
    docker exec flowscope-test-mysql mysql -uflowscope -pflowscope flowscope -e "
        DROP TABLE IF EXISTS order_items;
        DROP TABLE IF EXISTS orders;
        DROP TABLE IF EXISTS users;

        CREATE TABLE users (
            id INT AUTO_INCREMENT PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            email VARCHAR(255) UNIQUE
        );

        CREATE TABLE orders (
            id INT AUTO_INCREMENT PRIMARY KEY,
            user_id INT NOT NULL,
            total DECIMAL(10,2) NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id)
        );

        CREATE TABLE order_items (
            id INT AUTO_INCREMENT PRIMARY KEY,
            order_id INT NOT NULL,
            product_name VARCHAR(255) NOT NULL,
            quantity INT NOT NULL,
            price DECIMAL(10,2) NOT NULL,
            FOREIGN KEY (order_id) REFERENCES orders(id)
        );
    "

    # Run tests
    cargo test -p flowscope-cli --features integration-tests --test integration mysql -- --test-threads=1

    # Stop MySQL
    just _mysql-stop

# Start MySQL container for integration tests
_mysql-start:
    #!/usr/bin/env bash
    set -euo pipefail

    # Stop existing container if running
    docker rm -f flowscope-test-mysql 2>/dev/null || true

    # Start MySQL on port 3307 to avoid conflicts
    docker run -d \
        --name flowscope-test-mysql \
        -e MYSQL_ROOT_PASSWORD=root \
        -e MYSQL_USER=flowscope \
        -e MYSQL_PASSWORD=flowscope \
        -e MYSQL_DATABASE=flowscope \
        -p 3307:3306 \
        mysql:8.0

    echo "MySQL container started on port 3307"

# Stop MySQL container
_mysql-stop:
    docker rm -f flowscope-test-mysql 2>/dev/null || true

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

# Generate HTML coverage report (requires cargo-llvm-cov)
coverage:
    cargo llvm-cov --workspace --html --output-dir coverage/

# Generate LCOV coverage file for CI/Codecov integration
coverage-lcov:
    cargo llvm-cov --workspace --lcov --output-path lcov.info

# Print coverage summary to stdout
coverage-summary:
    cargo llvm-cov --workspace --summary-only

# Run development server
dev:
    cd app && yarn dev

# Run linters
lint: lint-rust lint-ts

check-schema:
    cargo test -p flowscope-core --test schema_guard --locked
    cd packages/core && yarn test schema-compat.test.ts --silent

# Regenerate the API schema snapshot from Rust definitions
update-schema:
    node ./scripts/update_api_schema.cjs

# Run Rust clippy
lint-rust:
    cargo clippy --workspace -- -D warnings

# Run TypeScript linters
lint-ts:
    yarn workspaces run lint

# Fix TypeScript lint issues
lint-fix:
    yarn workspaces run lint:fix

# Run TypeScript type checking
typecheck:
    yarn workspaces run typecheck

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
    yarn prettier:write

# Check TypeScript formatting
fmt-check-ts:
    yarn prettier

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

# Install Rust tools (wasm-pack, cargo-watch, cargo-llvm-cov)
install-rust-tools:
    @command -v wasm-pack >/dev/null 2>&1 || cargo install wasm-pack --version "^0.13"
    @command -v cargo-watch >/dev/null 2>&1 || cargo install cargo-watch --version "^8"
    @command -v cargo-llvm-cov >/dev/null 2>&1 || cargo install cargo-llvm-cov

# Install pre-commit hooks (requires prek: https://github.com/j178/prek)
install-hooks:
    prek install

# Full CI workflow - lint, typecheck, test
ci: lint typecheck test

# Full development setup - install deps, hooks, and build
setup: install install-rust-tools install-hooks build

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
check: fmt-check-rust fmt-check-ts lint typecheck test-rust check-schema

# All checks (Rust + TS + schema compatibility)
check-all:
    cargo test --workspace --locked
    yarn workspace @pondpilot/flowscope-core test --silent
    just check-schema

# Deploy app to Cloudflare Pages
deploy: build-wasm build-ts
    cd app && yarn build
    wrangler pages deploy app/dist --project-name flowscope-app
