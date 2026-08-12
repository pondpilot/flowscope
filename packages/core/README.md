# @pondpilot/flowscope-core

The TypeScript client library for FlowScope.

## Overview

This package provides the high-level API for interacting with the FlowScope WebAssembly engine. It handles loading the WASM module and provides typed interfaces for analysis requests and results.

## Installation

```bash
npm install @pondpilot/flowscope-core
```

## Usage

```typescript
import { initWasm, analyzeSql } from '@pondpilot/flowscope-core';

await initWasm();

const result = await analyzeSql({
  sql: 'SELECT * FROM users',
  dialect: 'duckdb',
});
```

Bundlers such as Vite resolve the package-owned WASM URL automatically. Pass
`wasmUrl` only when a host deliberately serves the binary from a custom
location; application builds should not copy the package WASM into a public
asset directory as well.

### Lint Diagnostics

Enable linting via the `options.lint` field. Lint issues appear in `result.issues` with codes prefixed by `LINT_`:

```typescript
const result = await analyzeSql({
  sql: 'SELECT * FROM users',
  dialect: 'postgres',
  options: { lint: { enabled: true } },
});

const lintIssues = result.issues.filter((i) => i.code.startsWith('LINT_'));
```

See the root [README](../../README.md) for more details.
