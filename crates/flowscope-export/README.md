# flowscope-export

Database export helpers for FlowScope analysis results.

## Overview

`flowscope-export` provides utilities to export `flowscope-core` lineage results to database and report formats. The default backend is DuckDB, which can be disabled by turning off the `duckdb` feature.

## Features

- `duckdb` (default): Export lineage to a DuckDB database file.
- SQL export: Generate DDL + INSERT statements for DuckDB (WASM-friendly).
- Mermaid export: Script/table/column/hybrid diagrams.
- CSV archive: ZIP bundle of structured CSV exports.
- XLSX export: Excel workbook with summary and lineage sheets.
- HTML export: Self-contained report with Mermaid diagrams.
- JSON export: Pretty or compact `AnalyzeResult`.

## Usage

Add it to your project alongside `flowscope-core`:

```toml
[dependencies]
flowscope-core = "0.1.0"
flowscope-export = "0.1.0"
```

## License

Apache 2.0
