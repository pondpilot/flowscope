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

# Build CLI with serve feature (embeds web UI - requires app to be built first)
build-cli-serve: sync-cli-serve-assets
    cargo build -p flowscope-cli --features serve --release

# Build CLI with serve feature in debug mode
build-cli-serve-debug: sync-cli-serve-assets
    cargo build -p flowscope-cli --features serve

# Internal: Build app dist for embedding into CLI
_build-app-dist:
    cd app && yarn build

# Internal: Sync compiled app assets into the CLI crate
sync-cli-serve-assets: _build-app-dist
    rm -rf crates/flowscope-cli/embedded-app
    mkdir -p crates/flowscope-cli/embedded-app
    cp -R app/dist/. crates/flowscope-cli/embedded-app/

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

# Run CLI tests with serve feature
test-cli-serve: sync-cli-serve-assets
    cargo test -p flowscope-cli --features serve

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

# Build WASM module without wasm-opt (faster for development)
build-wasm-dev:
    ./scripts/build-rust.sh --no-opt

# Build TypeScript packages
build-ts:
    yarn build:ts

# Build TypeScript workspace dependencies without rebuilding WASM
_build-core-ts:
    yarn workspace @pondpilot/flowscope-core build:no-wasm

_build-react-ts: _build-core-ts
    yarn workspace @pondpilot/flowscope-react build

_build-vscode-wasm:
    wasm-pack build crates/flowscope-wasm --target nodejs --no-opt --out-dir ../../vscode/wasm-node

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
test-ts: _build-core-ts
    yarn test:ts
    npm --prefix vscode run test

# Generate app TypeScript coverage
coverage-ts:
    yarn workspace @pondpilot/flowscope-app test:coverage

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
typecheck: _build-react-ts _build-vscode-wasm
    yarn workspaces run typecheck
    npm --prefix vscode run typecheck

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
    rm -rf crates/flowscope-cli/embedded-app

# Install dependencies
install:
    yarn install
    npm ci --prefix vscode

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

# Build and run the app (skips wasm-opt for fast iteration)
run: build-wasm-dev build-ts dev

# Check everything is working (quick validation)
check: fmt-check-rust fmt-check-ts lint typecheck test-ts test-rust check-schema

# All checks (Rust + TS + schema compatibility)
check-all:
    cargo test --workspace --locked
    yarn workspace @pondpilot/flowscope-core test --silent
    just check-schema

# Deploy app to Cloudflare Pages
deploy: build-wasm build-ts
    cd app && yarn build
    wrangler pages deploy app/dist --project-name flowscope-app

# Pre-release check: verify generated code is committed
check-generated:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Building to regenerate code..."
    cargo build -p flowscope-core --quiet
    if ! git diff --quiet crates/flowscope-core/src/generated/; then
        echo "ERROR: Generated code is out of sync!"
        echo "The following files have uncommitted changes:"
        git diff --name-only crates/flowscope-core/src/generated/
        echo ""
        echo "Run 'cargo build -p flowscope-core' and commit the changes."
        exit 1
    fi
    echo "Generated code is up to date."

# Run SQLFluff parity comparison report
sqlfluff-parity SQLFLUFF_DIR="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "{{SQLFLUFF_DIR}}" ]; then
        dir="{{SQLFLUFF_DIR}}"
    elif [ -n "${SQLFLUFF_DIR:-}" ]; then
        dir="$SQLFLUFF_DIR"
    else
        echo "Usage: just sqlfluff-parity /path/to/sqlfluff"
        echo "  or:  SQLFLUFF_DIR=/path/to/sqlfluff just sqlfluff-parity"
        exit 1
    fi
    venv="$dir/.venv/bin/python"
    if [ ! -x "$venv" ]; then
        echo "SQLFluff venv not found at $venv"
        echo "Expected a SQLFluff source checkout with .venv containing PyYAML."
        exit 1
    fi
    "$venv" scripts/sqlfluff-parity-report.py "$dir"

# Compare FlowScope vs SQLFluff lint/fix behavior on a real SQL corpus
sql-corpus-parity SQL_DIR="" SQLFLUFF_BIN="" DIALECT="postgres":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "{{SQL_DIR}}" ]; then
        sql_dir="{{SQL_DIR}}"
    elif [ -n "${SQL_DIR:-}" ]; then
        sql_dir="$SQL_DIR"
    else
        echo "Usage: just sql-corpus-parity /path/to/sql-dir /path/to/sqlfluff"
        echo "  or:  SQL_DIR=/path/to/sql-dir SQLFLUFF_BIN=/path/to/sqlfluff just sql-corpus-parity"
        exit 1
    fi
    if [ -n "{{SQLFLUFF_BIN}}" ]; then
        sqlfluff_bin="{{SQLFLUFF_BIN}}"
    elif [ -n "${SQLFLUFF_BIN:-}" ]; then
        sqlfluff_bin="$SQLFLUFF_BIN"
    elif [ -n "${SQLFLUFF_DIR:-}" ] && [ -x "${SQLFLUFF_DIR}/.venv/bin/sqlfluff" ]; then
        sqlfluff_bin="${SQLFLUFF_DIR}/.venv/bin/sqlfluff"
    else
        echo "SQLFluff binary not found."
        echo "Provide it explicitly:"
        echo "  just sql-corpus-parity /path/to/sql-dir /path/to/sqlfluff"
        echo "or set SQLFLUFF_BIN=/path/to/sqlfluff"
        echo "or set SQLFLUFF_DIR with a .venv/bin/sqlfluff binary."
        exit 1
    fi
    python3 scripts/sql-corpus-parity-report.py \
        --sql-dir "$sql_dir" \
        --sqlfluff-bin "$sqlfluff_bin" \
        --dialect "{{DIALECT}}"

# LT02-only corpus parity workbench (indentation engine project)
lt02-corpus-parity SQL_DIR="" SQLFLUFF_BIN="" DIALECT="postgres" JSON_OUTPUT="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "{{SQL_DIR}}" ]; then
        sql_dir="{{SQL_DIR}}"
    elif [ -n "${SQL_DIR:-}" ]; then
        sql_dir="$SQL_DIR"
    else
        echo "Usage: just lt02-corpus-parity /path/to/sql-dir /path/to/sqlfluff"
        echo "  or:  SQL_DIR=/path/to/sql-dir SQLFLUFF_BIN=/path/to/sqlfluff just lt02-corpus-parity"
        exit 1
    fi
    if [ -n "{{SQLFLUFF_BIN}}" ]; then
        sqlfluff_bin="{{SQLFLUFF_BIN}}"
    elif [ -n "${SQLFLUFF_BIN:-}" ]; then
        sqlfluff_bin="$SQLFLUFF_BIN"
    elif [ -n "${SQLFLUFF_DIR:-}" ] && [ -x "${SQLFLUFF_DIR}/.venv/bin/sqlfluff" ]; then
        sqlfluff_bin="${SQLFLUFF_DIR}/.venv/bin/sqlfluff"
    else
        echo "SQLFluff binary not found."
        echo "Provide it explicitly:"
        echo "  just lt02-corpus-parity /path/to/sql-dir /path/to/sqlfluff"
        echo "or set SQLFLUFF_BIN=/path/to/sqlfluff"
        echo "or set SQLFLUFF_DIR with a .venv/bin/sqlfluff binary."
        exit 1
    fi
    if [ -n "{{JSON_OUTPUT}}" ]; then
        json_output="{{JSON_OUTPUT}}"
    elif [ -n "${JSON_OUTPUT:-}" ]; then
        json_output="$JSON_OUTPUT"
    else
        json_output=""
    fi
    if [ -n "$json_output" ]; then
        python3 scripts/lt02-indent-parity-workbench.py \
            --sql-dir "$sql_dir" \
            --sqlfluff-bin "$sqlfluff_bin" \
            --dialect "{{DIALECT}}" \
            --json-output "$json_output"
    else
        python3 scripts/lt02-indent-parity-workbench.py \
            --sql-dir "$sql_dir" \
            --sqlfluff-bin "$sqlfluff_bin" \
            --dialect "{{DIALECT}}"
    fi

# Pre-release validation: all checks needed before tagging a release
pre-release: check-generated check-all
    @echo "All pre-release checks passed!"
