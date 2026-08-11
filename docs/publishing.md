# Publishing @pondpilot/flowscope-core

This document covers the npm publishing flow for the FlowScope core package.

## Prerequisites

- `wasm-pack` available on PATH.
- `NPM_TOKEN` configured in GitHub Actions.

## Versioning

- Update `packages/core/package.json` with the new version.
- If you want Rust + npm parity, update workspace versions first, then rebuild WASM.

## Local Build

```bash
just build-wasm
just build-ts
```

Or only the core package:

```bash
yarn workspace @pondpilot/flowscope-core build
```

## Local Pack Check

```bash
cd packages/core
npm pack --dry-run --workspaces=false
```

Confirm the tarball includes:
- `dist/` (TypeScript output)
- `wasm/` (WASM assets)

## Tag-based Publish

Publishing is triggered by tag pushes in CI:

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

The workflow builds WASM + TypeScript and runs `npm publish --workspaces=false` from `packages/core`.
