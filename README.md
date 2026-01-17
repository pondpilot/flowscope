# FlowScope

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.82+-orange.svg)](https://www.rust-lang.org)
[![TypeScript](https://img.shields.io/badge/typescript-5.0+-blue.svg)](https://www.typescriptlang.org)
[![WebAssembly](https://img.shields.io/badge/wasm-ready-purple.svg)](https://webassembly.org)

FlowScope is a privacy-first SQL lineage engine that runs entirely in your browser. Built with Rust and WebAssembly, it analyzes SQL queries to produce detailed lineage graphs showing how tables, CTEs, and columns flow through your data transformations.

The engine is designed for embedding into web applications, browser extensions, and development tools where you need instant lineage analysis without sending queries to a server.

## What's Inside

This is a monorepo containing the complete FlowScope stack:

**Core Engine** (`crates/`)
- `flowscope-core` — Rust-based SQL parser and lineage analyzer built on sqlparser-rs
- `flowscope-wasm` — WebAssembly bindings exposing the core engine to JavaScript
- `flowscope-cli` — Command-line interface for analyzing SQL files and generating diagrams

**NPM Packages** (`packages/`)
- `@pondpilot/flowscope-core` — TypeScript API with WASM loader and type-safe interfaces
- `@pondpilot/flowscope-react` — React components for interactive lineage visualization

**Demo Application** (`app/`)
- Full-featured web application showcasing FlowScope capabilities with multi-file project support

## Why FlowScope

**Privacy by Design**
Your SQL queries and schema metadata never leave the browser. The entire analysis happens client-side using WebAssembly, making it suitable for sensitive data workflows and compliance-heavy environments.

**Drop-in Integration**
The core package is a simple TypeScript import. No backend infrastructure, no API keys, no telemetry. Initialize the WASM module once and start analyzing queries immediately.

**Multi-Dialect Support**
FlowScope understands SQL variations across PostgreSQL, Snowflake, BigQuery, and generic ANSI SQL, handling the nuances of each dialect's syntax and semantics.

**Table and Column Lineage**
Track dependencies at both table and column levels. See how data flows through complex CTEs, joins, subqueries, window functions, and set operations. When you provide schema metadata, FlowScope expands wildcards and validates column references.

**Developer-Friendly Output**
Results come as structured JSON with typed interfaces, making it straightforward to build custom visualizations or integrate with existing tools. Issues and warnings include source spans for precise error highlighting.

## Quick Start

Install the core engine:

```bash
npm install @pondpilot/flowscope-core
```

Analyze your first query:

```typescript
import { initWasm, analyzeSql } from '@pondpilot/flowscope-core';

await initWasm();

const result = await analyzeSql({
  sql: `
    WITH active_users AS (
      SELECT user_id, email, signup_date
      FROM analytics.users
      WHERE last_login >= CURRENT_DATE - 30
    )
    SELECT u.email, COUNT(o.order_id) as order_count
    FROM active_users u
    LEFT JOIN analytics.orders o ON u.user_id = o.user_id
    GROUP BY u.email;
  `,
  dialect: 'postgres'
});

console.log('Tables referenced:', result.statements[0].nodes);
console.log('Data flows:', result.statements[0].edges);
console.log('Analysis issues:', result.issues);
```

The result includes a lineage graph with nodes representing tables, CTEs, and columns, plus edges showing ownership, data flow, and derivation relationships.

## Adding Visualization

For interactive graph visualization, add the React components:

```bash
npm install @pondpilot/flowscope-react
```

Then render the lineage:

```tsx
import { LineageExplorer } from '@pondpilot/flowscope-react';
import '@pondpilot/flowscope-react/styles.css';

function App() {
  const [result, setResult] = useState(null);

  // ... analyze SQL and store result ...

  return (
    <div style={{ height: '600px' }}>
      <LineageExplorer
        result={result}
        sql={yourSqlQuery}
        theme="light"
      />
    </div>
  );
}
```

The `LineageExplorer` component provides a complete UI with an interactive graph view, SQL editor with syntax highlighting, issue diagnostics, and search capabilities.

## Architecture

FlowScope uses a layered architecture that separates parsing logic from presentation:

```
┌─────────────────────────────────────┐
│   Your Application                  │
│   (Web App / Extension / IDE)       │
└──────────────┬──────────────────────┘
               │
               ↓
┌─────────────────────────────────────┐
│   @pondpilot/flowscope-core         │
│   TypeScript API + WASM Loader      │
└──────────────┬──────────────────────┘
               │ JSON Request
               ↓
┌─────────────────────────────────────┐
│   WebAssembly Module                │
│   (Rust: flowscope-core + wasm)     │
└──────────────┬──────────────────────┘
               │ JSON Response
               ↓
┌─────────────────────────────────────┐
│   @pondpilot/flowscope-react        │
│   (Optional) Visualization Layer    │
└─────────────────────────────────────┘
```

The core engine parses SQL into an abstract syntax tree using sqlparser-rs, then walks the AST to construct a lineage graph. The WASM boundary provides a clean serialization layer, while the TypeScript wrapper handles initialization and provides ergonomic types. React components are completely optional and focus solely on rendering.

## Schema Metadata

FlowScope works without schema information, but providing metadata unlocks additional capabilities:

```typescript
const result = await analyzeSql({
  sql: 'SELECT * FROM users JOIN orders ON users.id = orders.user_id',
  dialect: 'postgres',
  schema: {
    defaultSchema: 'public',
    tables: [
      {
        name: 'users',
        columns: [
          { name: 'id' },
          { name: 'email' },
          { name: 'created_at' }
        ]
      },
      {
        name: 'orders',
        columns: [
          { name: 'order_id' },
          { name: 'user_id' },
          { name: 'total' }
        ]
      }
    ]
  }
});
```

With schema metadata, FlowScope will expand `SELECT *` to explicit columns, validate column references, and provide more precise column-level lineage. Without it, the engine still produces useful table-level lineage and flags wildcards with approximate lineage warnings.

## Supported SQL

FlowScope focuses on analytics-oriented SQL with broad coverage of common patterns:

**Statement Types**
SELECT queries with joins, aliases, and subqueries | WITH clauses and CTEs (including recursive with warnings) | INSERT INTO ... SELECT | CREATE TABLE ... AS SELECT | UNION, UNION ALL, EXCEPT, and INTERSECT

**SQL Constructs**
All join types (INNER, LEFT, RIGHT, FULL, CROSS) | Window functions and OVER clauses | CASE expressions and scalar subqueries | Derived tables and table functions | GROUP BY and HAVING clauses | Set operations with multiple sources

**Dialect Coverage**
Generic ANSI SQL | PostgreSQL with specific extensions | Snowflake syntax variants | BigQuery Standard SQL

The engine reports unsupported syntax as structured issues while still producing partial lineage where possible. Check the test suite in `crates/flowscope-core/tests/` for detailed coverage examples.

## Development Setup

FlowScope uses a monorepo structure with both Rust and TypeScript workspaces. You'll need Rust (1.82+), Node.js (18+), and Yarn.

Clone and install dependencies:

```bash
git clone https://github.com/melonamin/flowscope.git
cd flowscope
yarn install
```

Build everything:

```bash
just build
# or individually:
just build-rust    # Rust workspace
just build-wasm    # WASM module
just build-ts      # TypeScript packages
```

Run tests:

```bash
just test          # All tests
just test-rust     # Rust unit + integration
just test-ts       # TypeScript/Vitest
just test-lineage  # Lineage engine only
```

Start the demo app:

```bash
just dev
# Opens http://localhost:5173
```

The project uses [Just](https://github.com/casey/just) as a task runner. Run `just` without arguments to see all available commands.

## Project Structure

```
flowscope/
├── crates/
│   ├── flowscope-core/      # Core SQL analysis engine (Rust)
│   │   ├── src/
│   │   │   ├── analyzer.rs          # Main analysis orchestration
│   │   │   ├── analyzer/            # Modular analysis components
│   │   │   │   ├── context.rs       # Per-statement state and scope management
│   │   │   │   ├── schema_registry.rs # Schema metadata and name resolution
│   │   │   │   ├── visitor.rs       # AST visitor for lineage extraction
│   │   │   │   ├── query.rs         # Query analysis (SELECT, subqueries)
│   │   │   │   ├── expression.rs    # Expression and column lineage
│   │   │   │   ├── statements.rs    # Statement-level analysis
│   │   │   │   ├── ddl.rs           # DDL statement handling
│   │   │   │   ├── diagnostics.rs   # Issue reporting
│   │   │   │   └── helpers/         # Utility functions
│   │   │   ├── parser/              # SQL dialect handling
│   │   │   └── types/               # Request/response types
│   │   └── tests/                   # Comprehensive test suite
│   ├── flowscope-wasm/      # WebAssembly bindings
│   └── flowscope-cli/       # Command-line interface
│
├── packages/
│   ├── core/                # @pondpilot/flowscope-core
│   │   ├── src/
│   │   │   ├── analyzer.ts          # Public API
│   │   │   ├── wasm-loader.ts       # WASM initialization
│   │   │   └── types.ts             # TypeScript definitions
│   │   └── tests/                   # API + integration tests
│   │
│   └── react/               # @pondpilot/flowscope-react
│       ├── src/
│       │   ├── components/          # React components
│       │   │   ├── LineageExplorer.tsx
│       │   │   ├── GraphView.tsx
│       │   │   ├── SqlView.tsx
│       │   │   └── ui/              # Reusable primitives
│       │   ├── store.ts             # Zustand state management
│       │   └── utils/               # Layout + graph builders
│       └── tests/
│
├── app/                     # Demo web application
│   └── src/
│       ├── components/              # App-level components
│       ├── hooks/                   # React hooks
│       └── lib/                     # Project state management
│
└── docs/                    # Architecture and guides
```

## Testing

The Rust core has extensive integration tests covering SQL feature combinations across all supported dialects. Tests are organized by SQL construct and use fixture files for readability:

```bash
# Run full lineage test suite
just test-lineage

# Run specific test pattern
just test-lineage-filter "cte"

# Generate coverage report
just coverage
```

TypeScript packages include unit tests for the API layer and WASM loading. React component tests use Vitest with jsdom.

## Diagnostics and Issues

FlowScope attaches structured issues to analysis results rather than failing silently. Each issue includes a severity level (info, warning, error), a machine-readable code, and an optional source span:

```typescript
result.issues.forEach(issue => {
  console.log(`${issue.severity}: ${issue.message}`);
  if (issue.span) {
    console.log(`  at characters ${issue.span.start}-${issue.span.end}`);
  }
});
```

Common issue codes include `PARSE_ERROR` for syntax problems, `UNSUPPORTED_SYNTAX` for constructs the engine doesn't handle yet, and `APPROXIMATE_LINEAGE` when wildcards are used without schema metadata. UI components can use spans to highlight problematic SQL fragments.

## Contributing

Contributions are welcome for new dialect support, additional statement types, improved lineage semantics, and viewer enhancements. The project follows standard Rust and TypeScript conventions:

**Before submitting:**
- Run `just check` to validate formatting, linting, and tests
- Add test coverage for new SQL constructs in `crates/flowscope-core/tests/`
- Update type definitions if modifying the request/response structure
- Ensure WASM builds successfully with `just build-wasm`

The architecture documentation in `docs/` provides context for understanding how the pieces fit together.

## Use Cases

**SQL Editor Integration**
Add a lineage viewer alongside your query editor. Users can see table dependencies and column flows before running expensive queries.

**Browser Extensions**
Overlay lineage visualization on cloud data warehouse UIs (Snowflake, BigQuery, Databricks). Extract query text from the page, analyze it locally, and show results in a popup.

**IDE Plugins**
Build VS Code or JetBrains extensions that provide lineage-on-hover for SQL files, especially useful for dbt projects with complex model dependencies.

**Impact Analysis Tools**
Show downstream dependencies when proposing schema changes. FlowScope can batch-analyze multiple queries to build a comprehensive dependency graph.

**Data Governance Platforms**
Integrate into internal data catalogs to automatically document table relationships from query logs, all processed client-side for security.

## Roadmap

The core lineage engine and visualization components are production-ready. Current development focuses on:

- Additional dialect support (MySQL, Redshift, Databricks SQL)
- Extended statement coverage (UPDATE, DELETE, MERGE)
- OpenLineage format export for ecosystem integration
- Web Worker helper for processing large query files
- Example integrations (browser extension, VS Code plugin)

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

Copyright 2025 PondPilot Team

## Acknowledgments

FlowScope builds on [sqlparser-rs](https://github.com/sqlparser-rs/sqlparser-rs), an excellent SQL parsing library with multi-dialect support. The visualization layer uses [@xyflow/react](https://reactflow.dev/) for graph rendering and [CodeMirror](https://codemirror.net/) for SQL editing.

---

Part of the [PondPilot](https://github.com/pondpilot/pondpilot) project.
