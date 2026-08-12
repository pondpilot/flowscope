#!/bin/bash
# Build script for FlowScope Rust/WASM components
#
# This script builds the native Rust workspace and the browser WASM module,
# then sets up the TypeScript source symlink used by package and app builds.
#
# Directory structure:
#   packages/core/wasm/     - Canonical browser WASM output (also published to npm)
#   packages/core/src/wasm  - Symlink to ../wasm (for package/Vite imports)
#
# Usage: Run from repository root: ./scripts/build-rust.sh

set -euo pipefail

NO_OPT=""

for arg in "$@"; do
    case "$arg" in
        --no-opt)
            NO_OPT="--no-opt"
            ;;
    esac
done

# Ensure we're running from the repository root
cd "$(dirname "$0")/.."

echo "Building Rust workspace..."
cargo build --release --workspace

echo "Building WASM module..."
# Output to packages/core/wasm (same location as package.json build:wasm for npm publishing)
wasm-pack build crates/flowscope-wasm --release --target web --out-dir ../../packages/core/wasm $NO_OPT

# Restore .gitignore to allow WASM artifacts to be committed
# (wasm-pack generates a .gitignore that ignores everything)
echo "# Keep wasm artifacts available for publishing" > packages/core/wasm/.gitignore

# Remove the legacy app/public mirror, including ignored binaries left behind
# by older versions of this script. Vite now emits the package-owned WASM.
node app/scripts/remove-legacy-wasm.mjs

# Create symlink for TypeScript development imports
# The symlink allows TypeScript to import from './wasm' while the actual files
# live one directory up in packages/core/wasm (for cleaner npm package structure)
echo "Setting up development symlink..."
if [ ! -L "packages/core/src/wasm" ]; then
    # Remove any existing directory and replace with symlink
    rm -rf packages/core/src/wasm
    ln -s ../wasm packages/core/src/wasm
fi

echo "WASM build complete!"
ls -la packages/core/wasm/
