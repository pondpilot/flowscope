# flowscope-core

Core SQL lineage analysis engine for FlowScope.

## Overview

`flowscope-core` is a Rust library that performs static analysis on SQL queries to extract table and column-level lineage information. It serves as the foundation for the FlowScope ecosystem, powering the WebAssembly bindings and JavaScript packages.

## Features

- **Multi-Dialect Parsing:** Built on `sqlparser-rs`, supporting PostgreSQL, Snowflake, BigQuery, and Generic ANSI SQL.
- **Deep Lineage Extraction:**
  - Table-level dependencies (SELECT, INSERT, UPDATE, MERGE, etc.)
  - Column-level data flow (including transformations)
- **Complex SQL Support:** Handles CTEs (Common Table Expressions), Subqueries, Joins, Unions, and Window Functions.
- **Schema Awareness:** Can utilize provided schema metadata to validate column references and resolve wildcards (`SELECT *`).
- **Diagnostics:** Returns structured issues (errors, warnings) with source spans for precise highlighting.

## Usage

```rust
use flowscope_core::{analyze, AnalyzeRequest, Dialect};

fn main() {
    let request = AnalyzeRequest {
        sql: "SELECT u.name, o.id FROM users u JOIN orders o ON u.id = o.user_id".to_string(),
        dialect: Dialect::Postgres,
        schema: None, // Optional schema metadata
        file_path: None,
    };

    let result = analyze(&request);

    // Access table lineage
    for statement in result.statements {
        println!("Tables: {:?}", statement.nodes);
        println!("Edges: {:?}", statement.edges);
    }
}
```

## Testing

```bash
cargo test
```

## License

Apache 2.0