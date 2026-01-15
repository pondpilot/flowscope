# DuckDB Export Design

Export analysis results to a queryable DuckDB database, enabling ad-hoc lineage queries, impact analysis, and compliance audits.

## Overview

**Goal:** Export the full `AnalyzeResult` to a DuckDB database file that users can query with standard SQL.

**Target users:**
- Data engineers auditing lineage
- Compliance/governance teams tracing PII flows
- Developers debugging complex SQL

**Architecture:** New `flowscope-export` Rust crate with two export paths:
- **Native (CLI):** Creates binary DuckDB database file
- **WASM (browser):** Generates SQL statements (DDL + INSERT) for duckdb-wasm

## Architecture

```
flowscope-core (types + analysis)
       ↑
flowscope-export (database generation)
       ↑                       ↑
flowscope-cli          flowscope-wasm
(native binary)        (SQL text output)
```

The native CLI path creates a binary DuckDB file via the `bundled` feature.
The WASM path generates SQL text that can be executed by duckdb-wasm in the browser.

**Note:** DuckDB's native code cannot compile to WASM, so the browser uses SQL text generation.

## Crate Structure

```
crates/flowscope-export/
├── Cargo.toml
└── src/
    ├── lib.rs              # Public API (Format enum, export functions)
    ├── error.rs            # Error types
    ├── schema.rs           # DDL generation (tables + views)
    ├── writer.rs           # Backend trait (for future SQLite)
    ├── duckdb_backend.rs   # Native DuckDB export (feature-gated)
    └── sql_backend.rs      # SQL text generation (WASM-compatible)
```

## Public API

```rust
use flowscope_core::AnalyzeResult;

pub enum Format {
    DuckDB,   // Native binary (requires duckdb feature)
    Sql,      // SQL text (WASM-compatible)
}

// Generic export (bytes for DuckDB/Sql)
pub fn export(result: &AnalyzeResult, format: Format) -> Result<Vec<u8>, ExportError>;

// Native DuckDB binary export (CLI only)
#[cfg(feature = "duckdb")]
pub fn export_duckdb(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError>;

// SQL text export (WASM-compatible)
pub fn export_sql(result: &AnalyzeResult) -> Result<String, ExportError>;
```

## Database Schema

### Core Tables

```sql
-- Metadata about the export
CREATE TABLE _meta (
    key TEXT PRIMARY KEY,
    value TEXT
);

-- SQL statements analyzed
CREATE TABLE statements (
    id INTEGER PRIMARY KEY,
    statement_index INTEGER NOT NULL,
    statement_type TEXT NOT NULL,
    source_name TEXT,
    sql_text TEXT,
    span_start INTEGER,
    span_end INTEGER,
    join_count INTEGER NOT NULL DEFAULT 0,
    complexity_score INTEGER
);

-- Graph nodes (tables, CTEs, columns, outputs)
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    statement_id INTEGER REFERENCES statements(id),
    node_type TEXT NOT NULL,
    label TEXT NOT NULL,
    qualified_name TEXT,
    expression TEXT,
    span_start INTEGER,
    span_end INTEGER,
    resolution_source TEXT
);

-- Graph edges (data flow relationships)
CREATE TABLE edges (
    id INTEGER PRIMARY KEY,
    statement_id INTEGER REFERENCES statements(id),
    edge_type TEXT NOT NULL,
    from_node_id TEXT NOT NULL REFERENCES nodes(id),
    to_node_id TEXT NOT NULL REFERENCES nodes(id),
    expression TEXT,
    operation TEXT,
    is_approximate BOOLEAN DEFAULT FALSE
);

-- Join metadata
CREATE TABLE joins (
    edge_id INTEGER PRIMARY KEY REFERENCES edges(id),
    join_type TEXT NOT NULL,
    join_condition TEXT
);

-- Filter predicates on nodes
CREATE TABLE filters (
    id INTEGER PRIMARY KEY,
    node_id TEXT NOT NULL REFERENCES nodes(id),
    predicate TEXT NOT NULL,
    filter_type TEXT
);

-- Aggregation info on column nodes
CREATE TABLE aggregations (
    node_id TEXT PRIMARY KEY REFERENCES nodes(id),
    is_grouping_key BOOLEAN NOT NULL,
    function TEXT,
    is_distinct BOOLEAN DEFAULT FALSE
);

-- Analysis issues
CREATE TABLE issues (
    id INTEGER PRIMARY KEY,
    statement_id INTEGER REFERENCES statements(id),
    severity TEXT NOT NULL,
    code TEXT NOT NULL,
    message TEXT NOT NULL,
    span_start INTEGER,
    span_end INTEGER
);

-- Schema information
CREATE TABLE schema_tables (
    id INTEGER PRIMARY KEY,
    catalog TEXT,
    schema TEXT,
    name TEXT NOT NULL,
    resolution_source TEXT,
    UNIQUE(catalog, schema, name)
);

CREATE TABLE schema_columns (
    id INTEGER PRIMARY KEY,
    table_id INTEGER NOT NULL REFERENCES schema_tables(id),
    name TEXT NOT NULL,
    data_type TEXT,
    is_nullable BOOLEAN,
    is_primary_key BOOLEAN DEFAULT FALSE
);
```

### Lineage Views

```sql
-- Direct column-to-column lineage
CREATE VIEW column_lineage AS
SELECT
    e.id AS edge_id,
    s.source_name,
    s.statement_index,
    fn.qualified_name AS source_table,
    fn.label AS source_column,
    tn.qualified_name AS target_table,
    tn.label AS target_column,
    e.expression AS transformation,
    e.operation,
    e.is_approximate
FROM edges e
JOIN nodes fn ON e.from_node_id = fn.id
JOIN nodes tn ON e.to_node_id = tn.id
JOIN statements s ON e.statement_id = s.id
WHERE fn.node_type = 'COLUMN'
  AND tn.node_type = 'COLUMN'
  AND e.edge_type IN ('DATA_FLOW', 'DERIVATION');

-- Table-level dependencies
CREATE VIEW table_dependencies AS
SELECT DISTINCT
    s.source_name,
    fn.qualified_name AS source_table,
    tn.qualified_name AS target_table,
    e.edge_type
FROM edges e
JOIN nodes fn ON e.from_node_id = fn.id
JOIN nodes tn ON e.to_node_id = tn.id
JOIN statements s ON e.statement_id = s.id
WHERE fn.node_type IN ('TABLE', 'VIEW', 'CTE')
  AND tn.node_type IN ('TABLE', 'VIEW', 'CTE');

-- Recursive: all upstream columns
CREATE VIEW column_ancestors AS
WITH RECURSIVE ancestors AS (
    SELECT
        to_node_id AS column_id,
        from_node_id AS ancestor_id,
        1 AS depth,
        expression AS transformation
    FROM edges
    WHERE edge_type IN ('DATA_FLOW', 'DERIVATION')

    UNION ALL

    SELECT
        a.column_id,
        e.from_node_id AS ancestor_id,
        a.depth + 1,
        e.expression
    FROM ancestors a
    JOIN edges e ON a.ancestor_id = e.to_node_id
    WHERE e.edge_type IN ('DATA_FLOW', 'DERIVATION')
      AND a.depth < 50
)
SELECT
    n1.qualified_name AS column_table,
    n1.label AS column_name,
    n2.qualified_name AS ancestor_table,
    n2.label AS ancestor_column,
    a.depth,
    a.transformation
FROM ancestors a
JOIN nodes n1 ON a.column_id = n1.id
JOIN nodes n2 ON a.ancestor_id = n2.id
WHERE n1.node_type = 'COLUMN'
  AND n2.node_type = 'COLUMN';

-- Recursive: all downstream columns
CREATE VIEW column_descendants AS
WITH RECURSIVE descendants AS (
    SELECT
        from_node_id AS column_id,
        to_node_id AS descendant_id,
        1 AS depth,
        expression AS transformation
    FROM edges
    WHERE edge_type IN ('DATA_FLOW', 'DERIVATION')

    UNION ALL

    SELECT
        d.column_id,
        e.to_node_id AS descendant_id,
        d.depth + 1,
        e.expression
    FROM descendants d
    JOIN edges e ON d.descendant_id = e.from_node_id
    WHERE e.edge_type IN ('DATA_FLOW', 'DERIVATION')
      AND d.depth < 50
)
SELECT
    n1.qualified_name AS column_table,
    n1.label AS column_name,
    n2.qualified_name AS descendant_table,
    n2.label AS descendant_column,
    d.depth,
    d.transformation
FROM descendants d
JOIN nodes n1 ON d.column_id = n1.id
JOIN nodes n2 ON d.descendant_id = n2.id
WHERE n1.node_type = 'COLUMN'
  AND n2.node_type = 'COLUMN';
```

### Graph Views

```sql
-- Denormalized node details
CREATE VIEW node_details AS
SELECT
    n.id,
    n.node_type,
    n.label,
    n.qualified_name,
    n.expression,
    n.resolution_source,
    s.statement_index,
    s.source_name,
    s.statement_type,
    a.is_grouping_key,
    a.function AS aggregation_function,
    a.is_distinct AS aggregation_distinct
FROM nodes n
LEFT JOIN statements s ON n.statement_id = s.id
LEFT JOIN aggregations a ON n.id = a.node_id;

-- Denormalized edge details
CREATE VIEW edge_details AS
SELECT
    e.id,
    e.edge_type,
    e.operation,
    e.expression,
    e.is_approximate,
    fn.node_type AS from_type,
    fn.label AS from_label,
    fn.qualified_name AS from_qualified_name,
    tn.node_type AS to_type,
    tn.label AS to_label,
    tn.qualified_name AS to_qualified_name,
    j.join_type,
    j.join_condition,
    s.statement_index,
    s.source_name
FROM edges e
JOIN nodes fn ON e.from_node_id = fn.id
JOIN nodes tn ON e.to_node_id = tn.id
LEFT JOIN joins j ON e.id = j.edge_id
LEFT JOIN statements s ON e.statement_id = s.id;

-- All joins with context
CREATE VIEW join_graph AS
SELECT
    s.source_name,
    s.statement_index,
    j.join_type,
    j.join_condition,
    fn.qualified_name AS left_table,
    tn.qualified_name AS right_table,
    e.is_approximate
FROM joins j
JOIN edges e ON j.edge_id = e.id
JOIN nodes fn ON e.from_node_id = fn.id
JOIN nodes tn ON e.to_node_id = tn.id
JOIN statements s ON e.statement_id = s.id;

-- Filters applied to nodes
CREATE VIEW node_filters AS
SELECT
    n.qualified_name AS table_name,
    n.label AS node_label,
    n.node_type,
    f.predicate,
    f.filter_type,
    s.source_name,
    s.statement_index
FROM filters f
JOIN nodes n ON f.node_id = n.id
LEFT JOIN statements s ON n.statement_id = s.id;
```

### Quality & Metrics Views

```sql
-- Complexity breakdown by statement
CREATE VIEW complexity_by_statement AS
SELECT
    s.source_name,
    s.statement_index,
    s.statement_type,
    s.complexity_score,
    s.join_count,
    COUNT(DISTINCT n.id) FILTER (WHERE n.node_type = 'TABLE') AS table_count,
    COUNT(DISTINCT n.id) FILTER (WHERE n.node_type = 'COLUMN') AS column_count,
    COUNT(DISTINCT e.id) AS edge_count
FROM statements s
LEFT JOIN nodes n ON n.statement_id = s.id
LEFT JOIN edges e ON e.statement_id = s.id
GROUP BY s.id, s.source_name, s.statement_index, s.statement_type,
         s.complexity_score, s.join_count;

-- Issue summary with context
CREATE VIEW issues_summary AS
SELECT
    i.severity,
    i.code,
    i.message,
    s.source_name,
    s.statement_index,
    s.statement_type,
    i.span_start,
    i.span_end
FROM issues i
LEFT JOIN statements s ON i.statement_id = s.id;

-- Table usage statistics
CREATE VIEW table_usage AS
SELECT
    n.qualified_name AS table_name,
    n.node_type,
    n.resolution_source,
    COUNT(DISTINCT n.statement_id) AS statement_count,
    COUNT(DISTINCT e_in.id) AS incoming_edges,
    COUNT(DISTINCT e_out.id) AS outgoing_edges
FROM nodes n
LEFT JOIN edges e_in ON n.id = e_in.to_node_id
LEFT JOIN edges e_out ON n.id = e_out.from_node_id
WHERE n.node_type IN ('TABLE', 'VIEW', 'CTE')
GROUP BY n.qualified_name, n.node_type, n.resolution_source;

-- Most connected columns
CREATE VIEW column_connectivity AS
SELECT
    n.qualified_name AS table_name,
    n.label AS column_name,
    COUNT(DISTINCT e_in.id) AS upstream_count,
    COUNT(DISTINCT e_out.id) AS downstream_count,
    COUNT(DISTINCT e_in.id) + COUNT(DISTINCT e_out.id) AS total_connections
FROM nodes n
LEFT JOIN edges e_in ON n.id = e_in.to_node_id
LEFT JOIN edges e_out ON n.id = e_out.from_node_id
WHERE n.node_type = 'COLUMN'
GROUP BY n.id, n.qualified_name, n.label
HAVING total_connections > 0
ORDER BY total_connections DESC;

-- Statements with issues
CREATE VIEW statements_with_issues AS
SELECT
    s.source_name,
    s.statement_index,
    s.statement_type,
    s.complexity_score,
    COUNT(*) FILTER (WHERE i.severity = 'ERROR') AS error_count,
    COUNT(*) FILTER (WHERE i.severity = 'WARNING') AS warning_count,
    COUNT(*) FILTER (WHERE i.severity = 'INFO') AS info_count
FROM statements s
JOIN issues i ON i.statement_id = s.id
GROUP BY s.id, s.source_name, s.statement_index, s.statement_type, s.complexity_score;
```

### Compliance & Audit Views

```sql
-- Full data flow paths
CREATE VIEW data_flow_paths AS
SELECT
    s.source_name,
    s.statement_index,
    fn.qualified_name AS source_table,
    fn.label AS source_column,
    e.expression AS transformation,
    e.operation,
    tn.qualified_name AS target_table,
    tn.label AS target_column,
    CASE WHEN e.is_approximate THEN 'APPROXIMATE' ELSE 'EXACT' END AS lineage_confidence
FROM edges e
JOIN nodes fn ON e.from_node_id = fn.id
JOIN nodes tn ON e.to_node_id = tn.id
JOIN statements s ON e.statement_id = s.id
WHERE e.edge_type IN ('DATA_FLOW', 'DERIVATION');

-- Impact analysis: columns by source table
CREATE VIEW columns_by_source_table AS
SELECT DISTINCT
    ancestor_table AS source_table,
    ancestor_column AS source_column,
    column_table AS affected_table,
    column_name AS affected_column,
    depth AS distance
FROM column_descendants;

-- Transformation audit
CREATE VIEW transformation_audit AS
SELECT
    s.source_name,
    s.statement_index,
    fn.qualified_name AS input_table,
    fn.label AS input_column,
    e.expression AS transformation_expression,
    e.operation AS transformation_type,
    tn.qualified_name AS output_table,
    tn.label AS output_column,
    a.function AS aggregation_applied
FROM edges e
JOIN nodes fn ON e.from_node_id = fn.id
JOIN nodes tn ON e.to_node_id = tn.id
JOIN statements s ON e.statement_id = s.id
LEFT JOIN aggregations a ON tn.id = a.node_id
WHERE e.expression IS NOT NULL
   OR a.function IS NOT NULL;

-- Cross-statement dependencies
CREATE VIEW cross_statement_flow AS
SELECT
    s1.source_name AS from_source,
    s1.statement_index AS from_statement,
    s2.source_name AS to_source,
    s2.statement_index AS to_statement,
    fn.qualified_name AS shared_object,
    e.edge_type
FROM edges e
JOIN nodes fn ON e.from_node_id = fn.id
JOIN nodes tn ON e.to_node_id = tn.id
JOIN statements s1 ON fn.statement_id = s1.id
JOIN statements s2 ON tn.statement_id = s2.id
WHERE s1.id != s2.id;

-- Schema coverage
CREATE VIEW schema_coverage AS
SELECT
    st.catalog,
    st.schema,
    st.name AS table_name,
    st.resolution_source,
    CASE WHEN n.id IS NOT NULL THEN TRUE ELSE FALSE END AS is_referenced,
    COUNT(DISTINCT n.statement_id) AS reference_count
FROM schema_tables st
LEFT JOIN nodes n ON n.qualified_name LIKE '%' || st.name || '%'
    AND n.node_type IN ('TABLE', 'VIEW')
GROUP BY st.id, st.catalog, st.schema, st.name, st.resolution_source,
         CASE WHEN n.id IS NOT NULL THEN TRUE ELSE FALSE END;
```

## Integration

### CLI

```bash
# Export to binary DuckDB file
flowscope analyze query.sql --format duckdb -o lineage.duckdb

# Query the exported database
duckdb lineage.duckdb -c "SELECT * FROM column_lineage"
```

### WASM

```rust
// Returns SQL text (DDL + INSERT statements)
#[wasm_bindgen]
pub fn export_to_duckdb_sql(result_json: &str) -> Result<String, JsError>;

// Combined analyze + export
#[wasm_bindgen]
pub fn analyze_and_export_sql(request_json: &str) -> Result<String, JsError>;
```

### Frontend (TypeScript)

```typescript
import { exportToDuckDbSql } from '@pondpilot/flowscope-core';

// Export via analysis worker
const sql = await exportToDuckDbSql(result);

// Download as .sql file
const blob = new Blob([sql], { type: 'text/sql' });
// ... trigger download

// Or execute in duckdb-wasm
const db = await initDuckDB();
await db.query(sql);
const lineage = await db.query('SELECT * FROM column_lineage');
```

## Example Queries

```sql
-- Where does customer.email ultimately come from?
SELECT * FROM column_ancestors
WHERE column_table LIKE '%customer%' AND column_name = 'email'
ORDER BY depth DESC;

-- What depends on users.id?
SELECT * FROM column_descendants
WHERE column_table LIKE '%users%' AND column_name = 'id';

-- Show all LEFT JOINs
SELECT * FROM join_graph WHERE join_type = 'LEFT';

-- Which statements are most complex?
SELECT * FROM complexity_by_statement ORDER BY complexity_score DESC LIMIT 10;

-- Show all errors
SELECT * FROM issues_summary WHERE severity = 'ERROR';
```

## Future Work

- SQLite backend (same schema, different writer)
- Incremental export (append to existing database)
- Parquet export (using same schema)
