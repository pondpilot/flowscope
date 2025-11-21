# flowscope-core

Core SQL lineage analysis engine for FlowScope.

## Overview

`flowscope-core` is a Rust library that parses SQL queries and extracts table-level lineage information. It uses `sqlparser-rs` for SQL parsing and provides a clean API for lineage analysis.

## Features

- SQL parsing using sqlparser-rs
- Table-level lineage extraction
- Support for multiple SQL dialects
- JSON serialization of results

## Usage

```rust
use flowscope_core::{parse_sql, extract_tables, LineageResult};

// Parse SQL
let sql = "SELECT * FROM users JOIN orders ON users.id = orders.user_id";
let statements = parse_sql(sql)?;

// Extract tables
let tables = extract_tables(&statements);

// Create result
let result = LineageResult::new(tables);
println!("{:?}", result); // LineageResult { tables: ["users", "orders"] }
```

## Testing

```bash
cargo test
```

## Current Limitations (Phase 0)

- Only table-level lineage (column-level coming in Phase 2)
- Basic SELECT support (CTEs, subqueries coming in Phase 1)
- No schema awareness yet (coming in Phase 1)

## License

Apache 2.0
