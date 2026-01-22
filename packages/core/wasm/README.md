# flowscope-wasm

WebAssembly bindings for `flowscope-core` and `flowscope-export`, enabling the SQL lineage engine and export formats to run in browsers, Node.js, and other WASM-supported environments.

## Overview

This crate exposes the Rust core engine to JavaScript via `wasm-bindgen`. It handles the serialization/deserialization of requests and results, providing a bridge between the typed Rust structures and JSON.

## API

### `analyze_sql_json(request_json: string) -> string`

The primary entry point for analysis. It takes a JSON-serialized `AnalyzeRequest` and returns a JSON-serialized `AnalyzeResult`.

**Input JSON Format:**
```json
{
  "sql": "SELECT * FROM users",
  "dialect": "postgres",
  "schema": { ... }
}
```

**Output JSON Format:**
```json
{
  "statements": [ ... ],
  "issues": [ ... ],
  "summary": {
    "hasErrors": false,
    ...
  }
}
```

### `analyze_sql(sql: string) -> string`

**Legacy/Deprecated.** Simple API that takes a raw SQL string and returns a basic JSON list of tables. Use `analyze_sql_json` for full features.

### `export_to_duckdb_sql(request_json: string) -> string`

Exports an `AnalyzeResult` to DuckDB-compatible SQL (DDL + INSERT statements).

```json
{
  "result": { ... },
  "schema": "lineage"
}
```

### `export_json(request_json: string) -> string`

```json
{
  "result": { ... },
  "compact": false
}
```

### `export_mermaid(request_json: string) -> string`

```json
{
  "result": { ... },
  "view": "table"
}
```

`view` supports `table`, `script`, `column`, `hybrid`, and `all`.

### `export_html(request_json: string) -> string`

```json
{
  "result": { ... },
  "projectName": "demo",
  "exportedAt": "2026-01-18T12:30:05Z"
}
```

### `export_csv_bundle(request_json: string) -> Uint8Array`

```json
{
  "result": { ... }
}
```

Returns a ZIP archive containing `scripts.csv`, `tables.csv`, `column_mappings.csv`, `table_dependencies.csv`, `summary.csv`, `issues.csv`, and `resolved_schema.csv`.

### `export_xlsx(request_json: string) -> Uint8Array`

```json
{
  "result": { ... }
}
```

### `export_filename(request_json: string) -> string`

```json
{
  "projectName": "demo",
  "exportedAt": "2026-01-18T12:30:05Z",
  "format": { "type": "xlsx" }
}
```

The `format` payload supports:
- `json` (with `compact`)
- `mermaid` (with `view`)
- `html`
- `sql`
- `csv`
- `xlsx`
- `duckdb`
- `png`

## Building

To build the WASM artifacts for the web:

```bash
just build-wasm
```

Or directly with wasm-pack:

```bash
wasm-pack build crates/flowscope-wasm --target web --out-dir ../../packages/core/wasm
```

## Artifact Management

The compiled `.wasm` binary is committed to the repository. This enables npm consumers to use the package without requiring a Rust toolchain.

**For contributors:**
- After modifying Rust source code in `crates/`, rebuild with `just build-wasm`
- Commit the updated `.wasm` file along with your Rust changes
- The committed binary should always match the current Rust source

**Note:** The `.wasm` file may contain compiled code from template variables if you've customized the build. Review the binary size and ensure no sensitive data is inadvertently included.

## License

Apache 2.0
