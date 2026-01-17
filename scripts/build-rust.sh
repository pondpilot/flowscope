#!/bin/bash
# Build script for FlowScope Rust/WASM components
#
# This script builds the native Rust workspace and the WASM module,
# then sets up the necessary symlinks and copies for development.
#
# Directory structure:
#   packages/core/wasm/     - WASM build output (committed for npm publishing)
#   packages/core/src/wasm  - Symlink to ../wasm (for TypeScript imports)
#   app/public/wasm/        - WASM files served by the dev server
#
# Usage: Run from repository root: ./scripts/build-rust.sh

set -euo pipefail

# Ensure we're running from the repository root
cd "$(dirname "$0")/.."

echo "Building Rust workspace..."
cargo build --release --workspace

echo "Building WASM module..."
# Output to packages/core/wasm (same location as package.json build:wasm for npm publishing)
wasm-pack build crates/flowscope-wasm --target web --out-dir ../../packages/core/wasm

# Create symlink for TypeScript development imports
# The symlink allows TypeScript to import from './wasm' while the actual files
# live one directory up in packages/core/wasm (for cleaner npm package structure)
echo "Setting up development symlink..."
if [ ! -L "packages/core/src/wasm" ]; then
    # Remove any existing directory and replace with symlink
    rm -rf packages/core/src/wasm
    ln -s ../wasm packages/core/src/wasm
fi

echo "Copying WASM to app locations..."
# Copy to app/public/wasm for the Vite dev server to serve
mkdir -p app/public/wasm
cp packages/core/wasm/flowscope_wasm_bg.wasm app/public/wasm/
cp packages/core/wasm/flowscope_wasm.js app/public/wasm/
cp packages/core/wasm/flowscope_wasm.d.ts app/public/wasm/
cp packages/core/wasm/flowscope_wasm_bg.wasm.d.ts app/public/wasm/

# Copy to app's node_modules when using yarn workspace linking
# This ensures the app can resolve the WASM files from the linked package
if [ -d "app/node_modules/@pondpilot/flowscope-core/wasm" ]; then
    echo "Copying WASM to app node_modules (workspace linking)..."
    cp packages/core/wasm/flowscope_wasm_bg.wasm app/node_modules/@pondpilot/flowscope-core/wasm/
    cp packages/core/wasm/flowscope_wasm.js app/node_modules/@pondpilot/flowscope-core/wasm/
else
    echo "Skipping app node_modules copy (directory not found - expected for fresh installs)"
fi

echo "WASM build complete!"
ls -la packages/core/wasm/
