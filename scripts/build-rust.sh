#!/bin/bash
set -e

echo "Building Rust workspace..."
cargo build --release --workspace

echo "Building WASM module..."
wasm-pack build crates/flowscope-wasm --target web --out-dir ../../packages/core/wasm

echo "WASM build complete!"
ls -la packages/core/wasm/
