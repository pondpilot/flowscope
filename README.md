# FlowScope

[![CI](https://github.com/pondpilot/flowscope/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/pondpilot/flowscope/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-available-brightgreen.svg)](docs/README.md)
[![Coverage](https://img.shields.io/badge/coverage-n%2Fa-lightgrey.svg)](https://github.com/pondpilot/flowscope/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.82+-orange.svg)](https://www.rust-lang.org)
[![TypeScript](https://img.shields.io/badge/typescript-5.0+-blue.svg)](https://www.typescriptlang.org)
[![WebAssembly](https://img.shields.io/badge/wasm-ready-purple.svg)](https://webassembly.org)
[![Crates.io](https://img.shields.io/crates/v/flowscope-core.svg)](https://crates.io/crates/flowscope-core)
[![Crates.io](https://img.shields.io/crates/v/flowscope-export.svg)](https://crates.io/crates/flowscope-export)
[![Crates.io](https://img.shields.io/crates/v/flowscope-cli.svg)](https://crates.io/crates/flowscope-cli)
[![npm](https://img.shields.io/npm/v/@pondpilot/flowscope-core.svg)](https://www.npmjs.com/package/@pondpilot/flowscope-core)

FlowScope includes a full web application at [flowscope.pondpilot.io](https://flowscope.pondpilot.io) for interactive, multi-file SQL lineage analysis.

Under the hood, it is a privacy-first SQL lineage engine that runs entirely in the browser. Built with Rust and WebAssembly, it analyzes SQL queries to produce lineage graphs that describe how tables, CTEs, and columns flow through transformations.

The engine is designed for embedding into web apps, browser extensions, and developer tools that need instant lineage analysis without sending SQL to a server.

## Components

- `app/` — the hosted web application at [flowscope.pondpilot.io](https://flowscope.pondpilot.io)
- `crates/` — Rust engine, WASM bindings, and CLI
- `packages/` — TypeScript API and React visualization components

## Key Features

- Client-side analysis with zero data egress
- Multi-dialect coverage (PostgreSQL, Snowflake, BigQuery, ANSI SQL)
- Table and column lineage with schema-aware wildcard expansion
- Structured diagnostics with spans for precise highlighting
- Completion API for SQL authoring workflows
- TypeScript API and optional React visualization components

## Quick Start

Install the core package:

```bash
npm install @pondpilot/flowscope-core
```

Analyze a query:

```typescript
import { initWasm, analyzeSql } from '@pondpilot/flowscope-core';

await initWasm();

const result = await analyzeSql({
  sql: 'SELECT * FROM analytics.orders',
  dialect: 'postgres',
});

console.log(result.statements[0]);
```

## Completion API

Use the completion API to provide SQL authoring hints at a cursor position. See [docs/guides/schema-metadata.md](docs/guides/schema-metadata.md) for schema setup details.

```typescript
import {
  charOffsetToByteOffset,
  completionItems,
  initWasm,
} from '@pondpilot/flowscope-core';

await initWasm();

const sql = 'SELECT * FROM analytics.';
const cursorOffset = charOffsetToByteOffset(sql, sql.length);

const result = await completionItems({
  sql,
  dialect: 'postgres',
  cursorOffset,
  schema: {
    defaultSchema: 'analytics',
    tables: [{ name: 'orders', columns: [{ name: 'order_id' }, { name: 'total' }] }],
  },
});

console.log(result.items.slice(0, 5));
```

## Visualization

For interactive lineage graphs, add the React package and render the `LineageExplorer` component. See [docs/guides/quickstart.md](docs/guides/quickstart.md) for a full walkthrough.

```bash
npm install @pondpilot/flowscope-react
```

## Documentation

- [docs/README.md](docs/README.md) — documentation map and reference index
- [docs/guides/quickstart.md](docs/guides/quickstart.md) — TypeScript quickstart guide
- [docs/guides/schema-metadata.md](docs/guides/schema-metadata.md) — schema metadata setup
- [docs/dialect-coverage.md](docs/dialect-coverage.md) — dialect and statement coverage
- [crates/flowscope-cli/README.md](crates/flowscope-cli/README.md) — CLI usage and examples
- [docs/workspace-structure.md](docs/workspace-structure.md) — monorepo layout and build entry points

## Development

FlowScope uses `just` for common tasks. Run `just build`, `just test`, or `just dev`, and see [docs/workspace-structure.md](docs/workspace-structure.md) for the full command list.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, testing expectations, and contribution guidelines.

## Usage

PondPilot uses the `@pondpilot/flowscope-core` package for SQL parsing and autocomplete in the hosted app at [app.pondpilot.io](https://app.pondpilot.io). Learn more at [github.com/pondpilot/pondpilot](https://github.com/pondpilot/pondpilot).

## License

The core engine and packages are released under Apache-2.0. See [LICENSE](LICENSE) for details. The `app/` directory uses the O'Saasy License; see [app/LICENSE](app/LICENSE).

---

Part of the [PondPilot](https://github.com/pondpilot/pondpilot) project.
