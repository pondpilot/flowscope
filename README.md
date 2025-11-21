# FlowScope – In-Browser SQL Lineage Engine

[![CI](https://github.com/pondpilot/flowscope/actions/workflows/ci.yml/badge.svg)](https://github.com/pondpilot/flowscope/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

FlowScope is a **client-side SQL lineage engine** that runs entirely in the browser using **Rust + WebAssembly**, with a **TypeScript API** and optional **React viewer components**.

It parses SQL, understands your CTEs, joins, inserts, and unions, and returns a **lineage graph** (tables + columns + edges) that you can render or analyze in your own tools.

> **Privacy-first.** Your SQL and schema metadata never leave the page.  
> **Embeddable.** Drop it into web apps, browser extensions, and IDE plugins.  
> **Multi-dialect.** Built on `sqlparser-rs` with common analytics dialects supported.

FlowScope is a subproject of **PondPilot**, focused specifically on SQL parsing and lineage.

---

## Features

- **Fully client-side**  
  Core engine is compiled to WebAssembly and runs in the browser (or other JS runtimes). No server, no telemetry.

- **Multi-dialect SQL parsing**  
  Built on top of `sqlparser-rs`, with support for common analytics dialects (e.g. Snowflake, BigQuery, Postgres, Generic).

- **Table & column-level lineage**  
  Understand how tables, CTEs, and columns feed each other across `SELECT`, `WITH`, `INSERT ... SELECT`, `CREATE TABLE AS SELECT`, and basic set operations.

- **Schema-aware (when you want it)**  
  Provide a simple JSON schema to expand `*` and validate column references, or run schema-less for quick explorations.

- **Embeddable engine + React viewer**  
  Core is exposed as an NPM package (e.g. `@pondpilot/flowscope-core`), with optional React components (e.g. `@pondpilot/flowscope-react`) that render graphs, column details, and SQL highlights.

- **Issues & diagnostics**  
  FlowScope reports parse errors, unsupported constructs, and approximate lineage (“we saw a `*` without schema”) as structured issues.

---

## Packages

FlowScope is a small stack of components:

- **Rust crates**
  - `flowscope-core` – core SQL lineage engine (Rust).
  - `flowscope-wasm` – WebAssembly bindings around `flowscope-core`.

- **NPM packages**
  - `@pondpilot/flowscope-core` – WASM loader + TypeScript API (analysis only).
  - `@pondpilot/flowscope-react` – React components for lineage visualization.

- **Examples**
  - `app` – Single-page React app showing FlowScope end-to-end.

You can use just the engine (`@pondpilot/flowscope-core`) or add the viewer on top.

---

## High-Level Architecture

```text
[Your App / Extension / Plugin]
      |
      |  (SQL string + options + optional schema)
      v
[@pondpilot/flowscope-core]     (TypeScript)
      |
      |  (JSON request via WebAssembly boundary)
      v
[FlowScope WASM Module]         (Rust: flowscope-core + flowscope-wasm)
      |
      |  (JSON result: lineage graph + issues + summary)
      v
[@pondpilot/flowscope-core]     (typed AnalyzeResult)
      |
      v
[Your UI or @pondpilot/flowscope-react components]
```

- The **core engine** is written in Rust and uses `sqlparser-rs` to parse SQL for multiple dialects.
- The engine walks the AST to build a **lineage graph**:
  - Nodes: tables, CTEs, columns (and optionally operations/statements).
  - Edges: data flow, derivation, ownership.
- The **WASM wrapper** exposes a small JSON-in / JSON-out interface.
- The **TypeScript wrapper** (`@pondpilot/flowscope-core`) handles:
  - WASM loading and initialization.
  - Type-safe request/response models.
  - Optional Web Worker integration.
- The **React viewer** (`@pondpilot/flowscope-react`) renders graphs and SQL highlights but does not compute lineage itself.

---

## Use Cases

- Add a **“Lineage” button** next to the “Run” button in your SQL editor.
- Build a **browser extension** that overlays lineage on Snowflake / BigQuery / dbt Cloud UIs.
- Show **impact analysis** in an internal data platform: “If I change this column, what breaks?”
- Inspect **dbt models or complex analytics queries** in a standalone web app.

---

## Installation

> Package names here use the `@pondpilot` scope as an example. Adjust the org scope to match your actual publishing setup.

### Core engine (required)

```bash
npm install @pondpilot/flowscope-core
# or
yarn add @pondpilot/flowscope-core
# or
pnpm add @pondpilot/flowscope-core
```

### React viewer (optional)

```bash
npm install @pondpilot/flowscope-react
# or
yarn add @pondpilot/flowscope-react
# or
pnpm add @pondpilot/flowscope-react
```

---

## Quickstart (TypeScript)

### 1. Analyze a query

This example shows how to analyze a simple query and inspect the result.

```ts
import {
  initWasm,
  analyzeSql,
  type AnalyzeRequest,
  type AnalyzeResult,
} from "@pondpilot/flowscope-core";

async function runExample(): Promise<void> {
  // 1. Initialize the WASM engine once at app bootstrap.
  //    If you skip this, analyzeSql can perform lazy init instead –
  //    but explicit init makes errors easier to diagnose.
  await initWasm();

  // 2. Prepare a request.
  const request: AnalyzeRequest = {
    sql: `
      WITH recent_orders AS (
        SELECT order_id, customer_id, amount
        FROM analytics.orders
        WHERE order_date >= CURRENT_DATE - 30
      )
      INSERT INTO analytics.order_summary (customer_id, total_amount)
      SELECT customer_id, SUM(amount) AS total_amount
      FROM recent_orders
      GROUP BY customer_id;
    `,
    dialect: "postgres",  // "generic" | "postgres" | "snowflake" | "bigquery"
    // Optional schema metadata – this allows FlowScope to validate table references.
    schema: {
      defaultSchema: "analytics",
      tables: [
        { name: "orders", columns: [{ name: "order_id" }, { name: "customer_id" }, { name: "amount" }, { name: "order_date" }] },
        { name: "order_summary", columns: [{ name: "customer_id" }, { name: "total_amount" }] },
      ],
    },
  };

  // 3. Analyze the SQL.
  const result: AnalyzeResult = await analyzeSql(request);

  // 4. Inspect the result.
  console.log("Summary:", result.summary);
  console.log("Issues:", result.issues);
  console.log("Statements:", result.statements.length);
  console.log("Tables found:", result.statements[0]?.nodes.map(n => n.label));
}

// Call runExample() from your app bootstrap or a test harness.
void runExample();
```

Key points:

- `sql` is a free-form multi-statement string.
- `dialect` controls which SQL dialect is used by the parser.
- `schema` is optional but helps with table validation and future column-level lineage.

---

## Using the React Viewer

Once you have an `AnalyzeResult`, you can render it using the React components from `@pondpilot/flowscope-react`.

```tsx
import React, { useState } from "react";
import {
  initWasm,
  analyzeSql,
  type AnalyzeResult,
} from "@pondpilot/flowscope-core";
import {
  LineageExplorer,
  type LineageExplorerProps,
} from "@pondpilot/flowscope-react";

const SAMPLE_SQL = `
SELECT
  u.id,
  u.email,
  o.total_amount
FROM public.users u
JOIN public.orders o
  ON u.id = o.user_id;
`;

export function FlowScopeDemo(): JSX.Element {
  const [sql, setSql] = useState<string>(SAMPLE_SQL);
  const [result, setResult] = useState<AnalyzeResult | null>(null);
  const [isAnalyzing, setIsAnalyzing] = useState<boolean>(false);

  async function handleAnalyze(): Promise<void> {
    setIsAnalyzing(true);
    try {
      await initWasm();
      const analyzeResult = await analyzeSql({
        sql,
        options: { dialect: "postgres", enableColumnLineage: true },
      });
      setResult(analyzeResult);
    } finally {
      setIsAnalyzing(false);
    }
  }

  const explorerProps: Partial<LineageExplorerProps> = result
    ? {
        result,
        sql,
        selectedStatementIndex: 0,
      }
    : {};

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
      <textarea
        rows={10}
        value={sql}
        onChange={(e) => setSql(e.target.value)}
        style={{ width: "100%", fontFamily: "monospace" }}
      />
      <button onClick={handleAnalyze} disabled={isAnalyzing}>
        {isAnalyzing ? "Analyzing..." : "Analyze SQL"}
      </button>

      {result && (
        <div style={{ height: "500px", border: "1px solid #e5e7eb" }}>
          <LineageExplorer
            result={explorerProps.result!}
            sql={explorerProps.sql!}
            selectedStatementIndex={explorerProps.selectedStatementIndex}
          />
        </div>
      )}
    </div>
  );
}
```

The `LineageExplorer` composite component typically includes:

- A statement selector (if multiple statements).
- A graph view (tables/CTEs/columns + arrows).
- A SQL panel with highlights.
- A column details panel.
- An issues list.

---

## Schema Format

FlowScope accepts a simple **schema metadata** object to improve column-level lineage:

```ts
const schema = {
  tables: {
    "analytics.orders": ["order_id", "customer_id", "amount", "order_date"],
    "analytics.order_summary": ["customer_id", "total_amount"],
  },
};
```

Guidelines:

- Keys are strings representing fully qualified table names.
- Values are arrays of column names (`string[]`).
- FlowScope treats keys as opaque identifiers; pick a consistent convention (e.g. `db.schema.table` or `schema.table`) and stick with it.
- With schema provided:
  - `SELECT * FROM analytics.orders` will expand to explicit columns.
  - Unknown columns will be flagged as issues.
- Without schema:
  - FlowScope still produces lineage for explicitly named columns.
  - `*` and `table.*` will be treated as approximate, with warnings.

---

## Dialects & Statement Coverage (MVP)

Initial support focuses on **analytics-oriented SQL**:

- **Dialects**
  - `generic`
  - `postgres`
  - `snowflake`
  - `bigquery`

- **Statements**
  - `SELECT` (joins, aliases, subqueries, basic window functions as expressions).
  - `WITH` / CTEs.
  - `INSERT INTO ... SELECT`.
  - `CREATE TABLE ... AS SELECT`.
  - `UNION` / `UNION ALL` and basic set operations.

FlowScope aims to provide:

- **Table-level lineage** for all of the above.
- **Column-level lineage** for explicit columns always, and for `*` when schema metadata is available.

Unsupported syntax should be reported as issues, with partial lineage where possible rather than hard failure.

---

## Issues & Diagnostics

FlowScope attaches structured **issues** to analysis results:

- Severity:
  - `info`
  - `warning`
  - `error`
- Example issue codes:
  - `PARSE_ERROR`
  - `UNSUPPORTED_SYNTAX`
  - `APPROXIMATE_LINEAGE`
- Each issue can include:
  - A human-readable message.
  - Optional `span` (start/end offsets) to highlight the relevant part of the SQL.

UI components can use these to:

- Show an issues panel.
- Highlight problematic tokens.
- Explain when lineage is approximate.

---

## Performance & Web Workers

For small/moderate queries you can call `analyzeSql` directly.

For larger scripts (multi-hundred-line dbt models, for example), use the **Web Worker helper** from `@pondpilot/flowscope-core` to avoid blocking the UI thread. The helper:

- Spawns a worker.
- Initializes the WASM engine inside it.
- Posts `AnalyzeRequest` messages and returns `AnalyzeResult` via Promises.

Details and examples live in the `docs` directory and the worker helper module.

---

## Roadmap (High-Level)

- ✅ Rust core + WASM wrapper + TypeScript API.
- ✅ Table- and column-level lineage for core analytics constructs.
- ✅ React viewer components + web demo.
- ⏳ Additional dialects (MySQL, Redshift, Databricks SQL, …).
- ⏳ More statement types (`UPDATE`, `DELETE`, `MERGE`).
- ⏳ Export to OpenLineage-style formats.
- ⏳ Example integrations (browser extension, VS Code extension).

---

## Contributing

FlowScope is designed as a **library-first** project:

- Contributions are welcome for:
  - New dialects.
  - Additional statement coverage.
  - Improved lineage semantics.
  - Viewer UX and layouts.
- Tests should cover:
  - Core lineage behavior for new features.
  - Regression cases for dialect-specific syntax.
  - TS/React integration where applicable.

See `docs/` for architecture, engine, and branding specs that govern how new features should be added.

---

## License

License information will be specified at the repo level. The intention is:

- Core engine: a permissive open-source license (e.g. MIT/Apache-2.0).
- UI components: same or compatible license.

Check the root `LICENSE` file in this repository for the authoritative terms.
