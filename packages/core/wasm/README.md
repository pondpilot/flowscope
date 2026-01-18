# flowscope-wasm

WebAssembly bindings for `flowscope-core`, enabling the SQL lineage engine to run in browsers, Node.js, and other WASM-supported environments.

## Overview

This crate exposes the Rust core engine to JavaScript via `wasm-bindgen`. It handles the serialization/deserialization of requests and results, providing a bridge between the typed Rust structures and JSON.

## API

### `analyze_sql_json(request_json: string) -> string`

The primary entry point for analysis. It takes a JSON-serialized `AnalyzeRequest` and returns a JSON-serialized `AnalyzeResult`.

**Input JSON Format:**
```json
{
  "sql": "SELECT * FROM users",
  "dialect": "postgres", // or "snowflake", "bigquery", "generic"
  "schema": { ... }      // Optional schema metadata
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

## Building

To build the WASM artifacts for the web:

```bash
wasm-pack build --target web --out-dir ../../app/public/wasm
```

## License

Apache 2.0