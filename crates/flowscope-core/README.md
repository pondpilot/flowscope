# flowscope-core

Core SQL lineage analysis engine for FlowScope.

## Overview

`flowscope-core` is a Rust library that performs static analysis on SQL queries to extract table and column-level lineage information. It serves as the foundation for the FlowScope ecosystem, powering the WebAssembly bindings and JavaScript packages.

## Features

- **Multi-Dialect Parsing:** Built on `sqlparser-rs`, supporting PostgreSQL, Snowflake, BigQuery, DuckDB, Redshift, MySQL, SQLite, Databricks, ClickHouse, and Generic ANSI SQL.
- **Deep Lineage Extraction:**
  - Table-level dependencies (SELECT, INSERT, UPDATE, MERGE, COPY, UNLOAD, etc.)
  - Column-level data flow (including transformations)
  - Cross-statement lineage tracking (CREATE TABLE AS, INSERT INTO ... SELECT)
- **dbt/Jinja Templating:** Preprocess SQL with Jinja or dbt-style templates before analysis, with built-in stubs for `ref()`, `source()`, `config()`, `var()`, and `is_incremental()`.
- **Complex SQL Support:** Handles CTEs (Common Table Expressions), Subqueries, Joins, Unions, Window Functions, and lateral column aliases.
- **Schema Awareness:** Utilize provided schema metadata to validate column references and resolve wildcards (`SELECT *`).
- **Type Inference:** Infer expression types with dialect-aware type compatibility checking.
- **Diagnostics:** Returns structured issues (errors, warnings) with source spans for precise highlighting.

## Structure

```
src/
├── analyzer.rs              # Main analysis orchestration
├── analyzer/
│   ├── context.rs           # Per-statement state and scope management
│   ├── schema_registry.rs   # Schema metadata and name resolution
│   ├── visitor.rs           # AST visitor for lineage extraction
│   ├── query.rs             # Query analysis (SELECT, subqueries)
│   ├── expression.rs        # Expression and column lineage
│   ├── select_analyzer.rs   # SELECT clause analysis
│   ├── statements.rs        # Statement-level analysis
│   ├── ddl.rs               # DDL statement handling (CREATE, ALTER)
│   ├── cross_statement.rs   # Cross-statement lineage tracking
│   ├── diagnostics.rs       # Issue reporting
│   ├── input.rs             # Input merging and deduplication
│   └── helpers/             # Utility functions
├── parser/                  # SQL dialect handling
├── types/                   # Request/response types
└── lineage/                 # Lineage graph construction
```

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