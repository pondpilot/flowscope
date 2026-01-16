# Publishing @pondpilot/flowscope-core

This document covers the npm publishing flow for the FlowScope core package.

## Prerequisites

- `wasm-pack` is available on your PATH for local builds.
- GitHub has `NPM_TOKEN` configured for publishing.

## Versioning

- Update `packages/core/package.json` with the new semver version.
- If you want Rust + npm parity, update the Rust workspace version first, then rebuild wasm.

## Local build

```bash
yarn workspace @pondpilot/flowscope-core build
```

Build output:

- `packages/core/dist/` for TypeScript output.
- `packages/core/wasm/` for the prebuilt WASM assets.

## Local pack check

```bash
cd packages/core
npm pack --dry-run --workspaces=false
```

Confirm the tarball list includes `wasm/flowscope_wasm_bg.wasm` and `wasm/flowscope_wasm.js`.

## Tag-based publish

The GitHub Actions workflow publishes on tag pushes.

```bash
git tag v0.1.0
git push origin v0.1.0
```

The workflow builds wasm + TypeScript and runs `npm publish --workspaces=false` from `packages/core`.

## Workflow dry-run

Use the manual workflow trigger with `dry_run=true` to validate packaging without publishing. The job runs `npm pack --dry-run --workspaces=false` after the build.

## WASM usage

```typescript
import { initWasm, analyzeSql } from '@pondpilot/flowscope-core';

await initWasm({ wasmUrl: '/wasm/flowscope_wasm_bg.wasm' });

const result = await analyzeSql({
  sql: 'SELECT * FROM users',
  dialect: 'duckdb'
});
```
