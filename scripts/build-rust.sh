#!/bin/bash
set -euo pipefail

echo "Building Rust workspace..."
cargo build --release --workspace

echo "Building WASM module..."
wasm-pack build crates/flowscope-wasm --target web --out-dir ../../packages/core/src/wasm

echo "Copying WASM to app locations..."
# Copy to app/public/wasm for dev server
mkdir -p app/public/wasm
cp packages/core/src/wasm/flowscope_wasm_bg.wasm app/public/wasm/
cp packages/core/src/wasm/flowscope_wasm.js app/public/wasm/
cp packages/core/src/wasm/flowscope_wasm.d.ts app/public/wasm/
cp packages/core/src/wasm/flowscope_wasm_bg.wasm.d.ts app/public/wasm/

# Copy to app's node_modules (for yarn workspace link)
if [ -d "app/node_modules/@pondpilot/flowscope-core/wasm" ]; then
    cp packages/core/src/wasm/flowscope_wasm_bg.wasm app/node_modules/@pondpilot/flowscope-core/wasm/
    cp packages/core/src/wasm/flowscope_wasm.js app/node_modules/@pondpilot/flowscope-core/wasm/
fi

echo "WASM build complete!"
ls -la packages/core/src/wasm/
