# DuckDB Export Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Create `flowscope-export` crate that exports `AnalyzeResult` to a queryable DuckDB database file.

**Architecture:** New Rust crate with pluggable backend trait. DuckDB first, SQLite later. Exposed via WASM for frontend, used natively by CLI.

**Tech Stack:** Rust, DuckDB (duckdb crate with bundled feature), wasm-bindgen, thiserror

---

## Task 1: Create flowscope-export crate skeleton

**Files:**
- Create: `crates/flowscope-export/Cargo.toml`
- Create: `crates/flowscope-export/src/lib.rs`
- Modify: `Cargo.toml` (workspace)

**Step 1: Create Cargo.toml**

Create file `crates/flowscope-export/Cargo.toml`:

```toml
[package]
name = "flowscope-export"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
description = "Database export for FlowScope analysis results"

[features]
default = ["duckdb"]
duckdb = ["dep:duckdb"]

[dependencies]
flowscope-core = { path = "../flowscope-core" }
thiserror = "2.0"
duckdb = { version = "1.0", features = ["bundled"], optional = true }

[dev-dependencies]
tempfile = "3"
```

**Step 2: Create lib.rs with public API skeleton**

Create file `crates/flowscope-export/src/lib.rs`:

```rust
//! Database export for FlowScope analysis results.
//!
//! Exports `AnalyzeResult` to queryable database formats (DuckDB, SQLite).

mod error;
mod schema;
mod writer;

#[cfg(feature = "duckdb")]
mod duckdb_backend;

pub use error::ExportError;

use flowscope_core::AnalyzeResult;

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// DuckDB database file
    DuckDB,
}

/// Export analysis result to a database file.
///
/// Returns raw bytes of the database file.
pub fn export(result: &AnalyzeResult, format: Format) -> Result<Vec<u8>, ExportError> {
    match format {
        #[cfg(feature = "duckdb")]
        Format::DuckDB => duckdb_backend::export(result),
        #[cfg(not(feature = "duckdb"))]
        Format::DuckDB => Err(ExportError::UnsupportedFormat("DuckDB feature not enabled")),
    }
}

/// Export analysis result to DuckDB format.
#[cfg(feature = "duckdb")]
pub fn export_duckdb(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    duckdb_backend::export(result)
}
```

**Step 3: Create error.rs**

Create file `crates/flowscope-export/src/error.rs`:

```rust
//! Error types for the export crate.

use thiserror::Error;

/// Errors that can occur during database export.
#[derive(Debug, Error)]
pub enum ExportError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(&'static str),
}

#[cfg(feature = "duckdb")]
impl From<duckdb::Error> for ExportError {
    fn from(e: duckdb::Error) -> Self {
        ExportError::Database(e.to_string())
    }
}
```

**Step 4: Create placeholder modules**

Create file `crates/flowscope-export/src/schema.rs`:

```rust
//! Database schema definitions (DDL).

/// Table DDL statements.
pub const TABLES_DDL: &str = "";

/// View DDL statements.
pub const VIEWS_DDL: &str = "";
```

Create file `crates/flowscope-export/src/writer.rs`:

```rust
//! Data writing utilities.

use flowscope_core::AnalyzeResult;

/// Write analysis result data to database.
pub fn write_data<W: DatabaseWriter>(
    _writer: &mut W,
    _result: &AnalyzeResult,
) -> Result<(), crate::ExportError> {
    Ok(())
}

/// Trait for database backends.
pub trait DatabaseWriter {
    fn execute(&mut self, sql: &str) -> Result<(), crate::ExportError>;
}
```

Create file `crates/flowscope-export/src/duckdb_backend.rs`:

```rust
//! DuckDB backend implementation.

use crate::ExportError;
use flowscope_core::AnalyzeResult;

pub fn export(_result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    Err(ExportError::UnsupportedFormat("Not yet implemented"))
}
```

**Step 5: Add to workspace**

In `Cargo.toml` (workspace root), add to members:

```toml
members = [
    "crates/flowscope-core",
    "crates/flowscope-wasm",
    "crates/flowscope-cli",
    "crates/flowscope-export",
]
```

**Step 6: Verify it compiles**

Run: `cargo check -p flowscope-export`
Expected: Compiles with no errors

**Step 7: Commit**

```bash
git add crates/flowscope-export Cargo.toml
git commit -m "$(cat <<'EOF'
feat(export): add flowscope-export crate skeleton

New crate for database export functionality. Currently DuckDB only,
with pluggable backend architecture for future SQLite support.
EOF
)"
```

---

## Task 2: Implement core tables DDL

**Files:**
- Modify: `crates/flowscope-export/src/schema.rs`

**Step 1: Write test for DDL syntax**

Create file `crates/flowscope-export/src/schema.rs` with tests:

```rust
//! Database schema definitions (DDL).

/// SQL to create all tables.
pub fn tables_ddl() -> &'static str {
    r#"
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
    from_node_id TEXT NOT NULL,
    to_node_id TEXT NOT NULL,
    expression TEXT,
    operation TEXT,
    is_approximate BOOLEAN DEFAULT FALSE
);

-- Join metadata (linked to nodes with join info)
CREATE TABLE joins (
    id INTEGER PRIMARY KEY,
    node_id TEXT NOT NULL REFERENCES nodes(id),
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

-- Schema tables (imported + inferred)
CREATE TABLE schema_tables (
    id INTEGER PRIMARY KEY,
    catalog TEXT,
    schema_name TEXT,
    name TEXT NOT NULL,
    resolution_source TEXT,
    UNIQUE(catalog, schema_name, name)
);

-- Schema columns
CREATE TABLE schema_columns (
    id INTEGER PRIMARY KEY,
    table_id INTEGER NOT NULL REFERENCES schema_tables(id),
    name TEXT NOT NULL,
    data_type TEXT,
    is_nullable BOOLEAN,
    is_primary_key BOOLEAN DEFAULT FALSE
);

-- Global nodes (cross-statement)
CREATE TABLE global_nodes (
    id TEXT PRIMARY KEY,
    node_type TEXT NOT NULL,
    label TEXT NOT NULL,
    canonical_catalog TEXT,
    canonical_schema TEXT,
    canonical_name TEXT NOT NULL,
    canonical_column TEXT,
    resolution_source TEXT
);

-- Global edges (cross-statement)
CREATE TABLE global_edges (
    id TEXT PRIMARY KEY,
    from_node_id TEXT NOT NULL,
    to_node_id TEXT NOT NULL,
    edge_type TEXT NOT NULL
);

-- Statement references for global nodes
CREATE TABLE global_node_statement_refs (
    id INTEGER PRIMARY KEY,
    global_node_id TEXT NOT NULL REFERENCES global_nodes(id),
    statement_index INTEGER NOT NULL,
    local_node_id TEXT
);
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tables_ddl_is_valid_sql() {
        // Just verify it's non-empty and contains expected tables
        let ddl = tables_ddl();
        assert!(ddl.contains("CREATE TABLE _meta"));
        assert!(ddl.contains("CREATE TABLE statements"));
        assert!(ddl.contains("CREATE TABLE nodes"));
        assert!(ddl.contains("CREATE TABLE edges"));
        assert!(ddl.contains("CREATE TABLE issues"));
    }
}
```

**Step 2: Run test**

Run: `cargo test -p flowscope-export test_tables_ddl_is_valid_sql`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/flowscope-export/src/schema.rs
git commit -m "feat(export): add core tables DDL schema"
```

---

## Task 3: Implement views DDL

**Files:**
- Modify: `crates/flowscope-export/src/schema.rs`

**Step 1: Add views DDL function**

Add to `crates/flowscope-export/src/schema.rs`:

```rust
/// SQL to create all views.
pub fn views_ddl() -> &'static str {
    r#"
-- ============================================================================
-- LINEAGE VIEWS
-- ============================================================================

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
WHERE fn.node_type = 'column'
  AND tn.node_type = 'column'
  AND e.edge_type IN ('data_flow', 'derivation');

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
WHERE fn.node_type IN ('table', 'view', 'cte')
  AND tn.node_type IN ('table', 'view', 'cte');

-- Recursive: all upstream columns
CREATE VIEW column_ancestors AS
WITH RECURSIVE ancestors AS (
    SELECT
        to_node_id AS column_id,
        from_node_id AS ancestor_id,
        1 AS depth,
        expression AS transformation
    FROM edges
    WHERE edge_type IN ('data_flow', 'derivation')

    UNION ALL

    SELECT
        a.column_id,
        e.from_node_id AS ancestor_id,
        a.depth + 1,
        e.expression
    FROM ancestors a
    JOIN edges e ON a.ancestor_id = e.to_node_id
    WHERE e.edge_type IN ('data_flow', 'derivation')
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
WHERE n1.node_type = 'column'
  AND n2.node_type = 'column';

-- Recursive: all downstream columns
CREATE VIEW column_descendants AS
WITH RECURSIVE descendants AS (
    SELECT
        from_node_id AS column_id,
        to_node_id AS descendant_id,
        1 AS depth,
        expression AS transformation
    FROM edges
    WHERE edge_type IN ('data_flow', 'derivation')

    UNION ALL

    SELECT
        d.column_id,
        e.to_node_id AS descendant_id,
        d.depth + 1,
        e.expression
    FROM descendants d
    JOIN edges e ON d.descendant_id = e.from_node_id
    WHERE e.edge_type IN ('data_flow', 'derivation')
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
WHERE n1.node_type = 'column'
  AND n2.node_type = 'column';

-- ============================================================================
-- GRAPH VIEWS
-- ============================================================================

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
LEFT JOIN joins j ON fn.id = j.node_id
LEFT JOIN statements s ON e.statement_id = s.id;

-- All joins with context
CREATE VIEW join_graph AS
SELECT
    s.source_name,
    s.statement_index,
    j.join_type,
    j.join_condition,
    n.qualified_name AS table_name,
    n.label AS table_label
FROM joins j
JOIN nodes n ON j.node_id = n.id
LEFT JOIN statements s ON n.statement_id = s.id;

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

-- ============================================================================
-- METRICS VIEWS
-- ============================================================================

-- Complexity breakdown by statement
CREATE VIEW complexity_by_statement AS
SELECT
    s.source_name,
    s.statement_index,
    s.statement_type,
    s.complexity_score,
    s.join_count,
    COUNT(DISTINCT CASE WHEN n.node_type = 'table' THEN n.id END) AS table_count,
    COUNT(DISTINCT CASE WHEN n.node_type = 'column' THEN n.id END) AS column_count,
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
WHERE n.node_type IN ('table', 'view', 'cte')
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
WHERE n.node_type = 'column'
GROUP BY n.id, n.qualified_name, n.label
HAVING COUNT(DISTINCT e_in.id) + COUNT(DISTINCT e_out.id) > 0
ORDER BY total_connections DESC;

-- Statements with issues
CREATE VIEW statements_with_issues AS
SELECT
    s.source_name,
    s.statement_index,
    s.statement_type,
    s.complexity_score,
    COUNT(CASE WHEN i.severity = 'error' THEN 1 END) AS error_count,
    COUNT(CASE WHEN i.severity = 'warning' THEN 1 END) AS warning_count,
    COUNT(CASE WHEN i.severity = 'info' THEN 1 END) AS info_count
FROM statements s
JOIN issues i ON i.statement_id = s.id
GROUP BY s.id, s.source_name, s.statement_index, s.statement_type, s.complexity_score;

-- ============================================================================
-- COMPLIANCE VIEWS
-- ============================================================================

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
WHERE e.edge_type IN ('data_flow', 'derivation');

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
    st.schema_name,
    st.name AS table_name,
    st.resolution_source,
    CASE WHEN COUNT(n.id) > 0 THEN TRUE ELSE FALSE END AS is_referenced,
    COUNT(DISTINCT n.statement_id) AS reference_count
FROM schema_tables st
LEFT JOIN nodes n ON n.qualified_name LIKE '%' || st.name || '%'
    AND n.node_type IN ('table', 'view')
GROUP BY st.id, st.catalog, st.schema_name, st.name, st.resolution_source;
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tables_ddl_is_valid_sql() {
        let ddl = tables_ddl();
        assert!(ddl.contains("CREATE TABLE _meta"));
        assert!(ddl.contains("CREATE TABLE statements"));
        assert!(ddl.contains("CREATE TABLE nodes"));
        assert!(ddl.contains("CREATE TABLE edges"));
        assert!(ddl.contains("CREATE TABLE issues"));
    }

    #[test]
    fn test_views_ddl_contains_all_views() {
        let ddl = views_ddl();
        // Lineage views
        assert!(ddl.contains("CREATE VIEW column_lineage"));
        assert!(ddl.contains("CREATE VIEW table_dependencies"));
        assert!(ddl.contains("CREATE VIEW column_ancestors"));
        assert!(ddl.contains("CREATE VIEW column_descendants"));
        // Graph views
        assert!(ddl.contains("CREATE VIEW node_details"));
        assert!(ddl.contains("CREATE VIEW edge_details"));
        assert!(ddl.contains("CREATE VIEW join_graph"));
        assert!(ddl.contains("CREATE VIEW node_filters"));
        // Metrics views
        assert!(ddl.contains("CREATE VIEW complexity_by_statement"));
        assert!(ddl.contains("CREATE VIEW issues_summary"));
        assert!(ddl.contains("CREATE VIEW table_usage"));
        assert!(ddl.contains("CREATE VIEW column_connectivity"));
        assert!(ddl.contains("CREATE VIEW statements_with_issues"));
        // Compliance views
        assert!(ddl.contains("CREATE VIEW data_flow_paths"));
        assert!(ddl.contains("CREATE VIEW columns_by_source_table"));
        assert!(ddl.contains("CREATE VIEW transformation_audit"));
        assert!(ddl.contains("CREATE VIEW cross_statement_flow"));
        assert!(ddl.contains("CREATE VIEW schema_coverage"));
    }
}
```

**Step 2: Run tests**

Run: `cargo test -p flowscope-export`
Expected: All tests pass

**Step 3: Commit**

```bash
git add crates/flowscope-export/src/schema.rs
git commit -m "feat(export): add views DDL for lineage, graph, metrics, compliance"
```

---

## Task 4: Implement DuckDB backend - database creation and schema

**Files:**
- Modify: `crates/flowscope-export/src/duckdb_backend.rs`

**Step 1: Write test for basic export**

Update `crates/flowscope-export/src/duckdb_backend.rs`:

```rust
//! DuckDB backend implementation.

use crate::schema::{tables_ddl, views_ddl};
use crate::ExportError;
use duckdb::{Connection, params};
use flowscope_core::AnalyzeResult;
use std::fs;
use tempfile::NamedTempFile;

/// Export analysis result to DuckDB database bytes.
pub fn export(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    // Create temp file for database
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path();

    // Create database and connection
    let conn = Connection::open(db_path)?;

    // Create schema
    create_schema(&conn)?;

    // Write data
    write_data(&conn, result)?;

    // Read file bytes
    let bytes = fs::read(db_path)?;

    Ok(bytes)
}

fn create_schema(conn: &Connection) -> Result<(), ExportError> {
    // Execute table DDL
    conn.execute_batch(tables_ddl())?;

    // Execute view DDL
    conn.execute_batch(views_ddl())?;

    Ok(())
}

fn write_data(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    write_meta(conn)?;
    write_statements(conn, result)?;
    write_nodes(conn, result)?;
    write_edges(conn, result)?;
    write_issues(conn, result)?;
    write_schema_tables(conn, result)?;
    write_global_lineage(conn, result)?;
    Ok(())
}

fn write_meta(conn: &Connection) -> Result<(), ExportError> {
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES (?, ?)",
        params!["version", env!("CARGO_PKG_VERSION")],
    )?;
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES (?, ?)",
        params!["exported_at", chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

fn write_statements(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    let mut stmt = conn.prepare(
        "INSERT INTO statements (id, statement_index, statement_type, source_name, span_start, span_end, join_count, complexity_score)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )?;

    for (idx, s) in result.statements.iter().enumerate() {
        let (span_start, span_end) = s.span.map(|sp| (Some(sp.start as i64), Some(sp.end as i64))).unwrap_or((None, None));
        stmt.execute(params![
            idx as i64,
            s.statement_index as i64,
            &s.statement_type,
            &s.source_name,
            span_start,
            span_end,
            s.join_count as i64,
            s.complexity_score as i64,
        ])?;
    }
    Ok(())
}

fn write_nodes(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    let mut node_stmt = conn.prepare(
        "INSERT INTO nodes (id, statement_id, node_type, label, qualified_name, expression, span_start, span_end, resolution_source)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )?;

    let mut join_stmt = conn.prepare(
        "INSERT INTO joins (node_id, join_type, join_condition) VALUES (?, ?, ?)"
    )?;

    let mut filter_stmt = conn.prepare(
        "INSERT INTO filters (node_id, predicate, filter_type) VALUES (?, ?, ?)"
    )?;

    let mut agg_stmt = conn.prepare(
        "INSERT INTO aggregations (node_id, is_grouping_key, function, is_distinct) VALUES (?, ?, ?, ?)"
    )?;

    for (stmt_idx, statement) in result.statements.iter().enumerate() {
        for node in &statement.nodes {
            let (span_start, span_end) = node.span.map(|sp| (Some(sp.start as i64), Some(sp.end as i64))).unwrap_or((None, None));
            let node_type = format!("{:?}", node.node_type).to_lowercase();
            let resolution = node.resolution_source.map(|r| format!("{:?}", r).to_lowercase());

            node_stmt.execute(params![
                node.id.as_ref(),
                stmt_idx as i64,
                node_type,
                node.label.as_ref(),
                node.qualified_name.as_ref().map(|s| s.as_ref()),
                node.expression.as_ref().map(|s| s.as_ref()),
                span_start,
                span_end,
                resolution,
            ])?;

            // Write join info if present
            if let Some(join_type) = &node.join_type {
                let jt = format!("{:?}", join_type).to_uppercase();
                join_stmt.execute(params![
                    node.id.as_ref(),
                    jt,
                    node.join_condition.as_ref().map(|s| s.as_ref()),
                ])?;
            }

            // Write filters
            for filter in &node.filters {
                let ft = format!("{:?}", filter.clause_type).to_lowercase();
                filter_stmt.execute(params![
                    node.id.as_ref(),
                    &filter.expression,
                    ft,
                ])?;
            }

            // Write aggregation info
            if let Some(agg) = &node.aggregation {
                agg_stmt.execute(params![
                    node.id.as_ref(),
                    agg.is_grouping_key,
                    &agg.function,
                    agg.distinct,
                ])?;
            }
        }
    }
    Ok(())
}

fn write_edges(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    let mut stmt = conn.prepare(
        "INSERT INTO edges (statement_id, edge_type, from_node_id, to_node_id, expression, operation, is_approximate)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )?;

    for (stmt_idx, statement) in result.statements.iter().enumerate() {
        for edge in &statement.edges {
            let edge_type = format!("{:?}", edge.edge_type).to_lowercase();
            stmt.execute(params![
                stmt_idx as i64,
                edge_type,
                edge.from.as_ref(),
                edge.to.as_ref(),
                edge.expression.as_ref().map(|s| s.as_ref()),
                edge.operation.as_ref().map(|s| s.as_ref()),
                edge.approximate.unwrap_or(false),
            ])?;
        }
    }
    Ok(())
}

fn write_issues(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    let mut stmt = conn.prepare(
        "INSERT INTO issues (statement_id, severity, code, message, span_start, span_end)
         VALUES (?, ?, ?, ?, ?, ?)"
    )?;

    for issue in &result.issues {
        let severity = format!("{:?}", issue.severity).to_lowercase();
        let (span_start, span_end) = issue.span.map(|sp| (Some(sp.start as i64), Some(sp.end as i64))).unwrap_or((None, None));
        stmt.execute(params![
            issue.statement_index.map(|i| i as i64),
            severity,
            &issue.code,
            &issue.message,
            span_start,
            span_end,
        ])?;
    }
    Ok(())
}

fn write_schema_tables(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    let Some(schema) = &result.resolved_schema else {
        return Ok(());
    };

    let mut table_stmt = conn.prepare(
        "INSERT INTO schema_tables (id, catalog, schema_name, name, resolution_source)
         VALUES (?, ?, ?, ?, ?)"
    )?;

    let mut col_stmt = conn.prepare(
        "INSERT INTO schema_columns (table_id, name, data_type, is_nullable, is_primary_key)
         VALUES (?, ?, ?, ?, ?)"
    )?;

    for (table_id, table) in schema.tables.iter().enumerate() {
        let origin = format!("{:?}", table.origin).to_lowercase();
        table_stmt.execute(params![
            table_id as i64,
            &table.catalog,
            &table.schema,
            &table.name,
            origin,
        ])?;

        for col in &table.columns {
            col_stmt.execute(params![
                table_id as i64,
                &col.name,
                &col.data_type,
                None::<bool>, // is_nullable not in current schema
                col.is_primary_key,
            ])?;
        }
    }
    Ok(())
}

fn write_global_lineage(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    let mut node_stmt = conn.prepare(
        "INSERT INTO global_nodes (id, node_type, label, canonical_catalog, canonical_schema, canonical_name, canonical_column, resolution_source)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )?;

    let mut ref_stmt = conn.prepare(
        "INSERT INTO global_node_statement_refs (global_node_id, statement_index, local_node_id)
         VALUES (?, ?, ?)"
    )?;

    let mut edge_stmt = conn.prepare(
        "INSERT INTO global_edges (id, from_node_id, to_node_id, edge_type)
         VALUES (?, ?, ?, ?)"
    )?;

    for node in &result.global_lineage.nodes {
        let node_type = format!("{:?}", node.node_type).to_lowercase();
        let resolution = node.resolution_source.map(|r| format!("{:?}", r).to_lowercase());

        node_stmt.execute(params![
            node.id.as_ref(),
            node_type,
            node.label.as_ref(),
            &node.canonical_name.catalog,
            &node.canonical_name.schema,
            &node.canonical_name.name,
            &node.canonical_name.column,
            resolution,
        ])?;

        for stmt_ref in &node.statement_refs {
            ref_stmt.execute(params![
                node.id.as_ref(),
                stmt_ref.statement_index as i64,
                stmt_ref.node_id.as_ref().map(|s| s.as_ref()),
            ])?;
        }
    }

    for edge in &result.global_lineage.edges {
        let edge_type = format!("{:?}", edge.edge_type).to_lowercase();
        edge_stmt.execute(params![
            edge.id.as_ref(),
            edge.from.as_ref(),
            edge.to.as_ref(),
            edge_type,
        ])?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flowscope_core::{analyze, AnalyzeRequest, Dialect};

    #[test]
    fn test_export_empty_result() {
        let result = AnalyzeResult::default();
        let bytes = export(&result).expect("Export should succeed");
        assert!(!bytes.is_empty(), "Database file should not be empty");
    }

    #[test]
    fn test_export_simple_query() {
        let request = AnalyzeRequest {
            sql: "SELECT id, name FROM users WHERE active = true".to_string(),
            dialect: Dialect::Generic,
            ..Default::default()
        };
        let result = analyze(&request);
        let bytes = export(&result).expect("Export should succeed");
        assert!(!bytes.is_empty());

        // Verify we can open the database and query it
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), &bytes).unwrap();
        let conn = Connection::open(temp_file.path()).unwrap();

        // Check statements table
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM statements", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);

        // Check nodes exist
        let node_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0)).unwrap();
        assert!(node_count > 0);
    }

    #[test]
    fn test_export_with_joins() {
        let request = AnalyzeRequest {
            sql: "SELECT u.name, o.total FROM users u LEFT JOIN orders o ON u.id = o.user_id".to_string(),
            dialect: Dialect::Generic,
            ..Default::default()
        };
        let result = analyze(&request);
        let bytes = export(&result).expect("Export should succeed");

        let temp_file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), &bytes).unwrap();
        let conn = Connection::open(temp_file.path()).unwrap();

        // Check joins table has data
        let join_count: i64 = conn.query_row("SELECT COUNT(*) FROM joins", [], |r| r.get(0)).unwrap();
        assert!(join_count > 0);
    }
}
```

**Step 2: Add chrono dependency**

Update `crates/flowscope-export/Cargo.toml`:

```toml
[dependencies]
flowscope-core = { path = "../flowscope-core" }
thiserror = "2.0"
duckdb = { version = "1.0", features = ["bundled"], optional = true }
chrono = "0.4"
tempfile = "3"
```

And move tempfile from dev-dependencies since we need it in the main code.

**Step 3: Run tests**

Run: `cargo test -p flowscope-export`
Expected: All tests pass

**Step 4: Commit**

```bash
git add crates/flowscope-export/
git commit -m "feat(export): implement DuckDB backend with full data export"
```

---

## Task 5: Add CLI integration

**Files:**
- Modify: `crates/flowscope-cli/Cargo.toml`
- Modify: `crates/flowscope-cli/src/cli.rs`
- Modify: `crates/flowscope-cli/src/main.rs`

**Step 1: Add dependency**

Update `crates/flowscope-cli/Cargo.toml` dependencies:

```toml
[dependencies]
flowscope-core = { path = "../flowscope-core" }
flowscope-export = { path = "../flowscope-export" }
# ... rest unchanged
```

**Step 2: Add DuckDB to OutputFormat**

Update `crates/flowscope-cli/src/cli.rs`:

```rust
/// Output format options
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable table format
    Table,
    /// JSON output
    Json,
    /// Mermaid diagram
    Mermaid,
    /// DuckDB database file
    Duckdb,
}
```

**Step 3: Handle DuckDB output in main.rs**

Update `crates/flowscope-cli/src/main.rs`:

Add import:
```rust
use flowscope_export::export_duckdb;
```

Update the format match in `run()`:
```rust
    // Format output
    let output_str = match args.format {
        OutputFormat::Json => format_json(&result, args.compact),
        OutputFormat::Table => format_table(&result, args.quiet, !args.quiet),
        OutputFormat::Mermaid => {
            let view = match args.view {
                ViewMode::Script => MermaidViewMode::Script,
                ViewMode::Table => MermaidViewMode::Table,
                ViewMode::Column => MermaidViewMode::Column,
                ViewMode::Hybrid => MermaidViewMode::Hybrid,
            };
            format_mermaid(&result, view)
        }
        OutputFormat::Duckdb => {
            // DuckDB outputs bytes, not string
            let bytes = export_duckdb(&result)
                .context("Failed to export to DuckDB")?;
            return write_binary_output(&args.output, &bytes, result.summary.has_errors);
        }
    };
```

Add helper function:
```rust
fn write_binary_output(path: &Option<std::path::PathBuf>, content: &[u8], has_errors: bool) -> Result<bool> {
    if let Some(path) = path {
        fs::write(path, content)
            .with_context(|| format!("Failed to write to {}", path.display()))?;
    } else {
        // Binary to stdout - just write raw bytes
        io::stdout()
            .write_all(content)
            .context("Failed to write to stdout")?;
    }
    Ok(has_errors)
}
```

**Step 4: Test CLI**

Run: `echo "SELECT * FROM users" | cargo run -p flowscope-cli -- -f duckdb -o /tmp/test.duckdb`
Expected: Creates /tmp/test.duckdb file

Run: `duckdb /tmp/test.duckdb "SELECT * FROM statements"`
Expected: Shows statement data

**Step 5: Commit**

```bash
git add crates/flowscope-cli/
git commit -m "feat(cli): add DuckDB export format (--format duckdb)"
```

---

## Task 6: Add WASM integration

**Files:**
- Modify: `crates/flowscope-wasm/Cargo.toml`
- Modify: `crates/flowscope-wasm/src/lib.rs`

**Step 1: Add dependency**

Update `crates/flowscope-wasm/Cargo.toml`:

```toml
[dependencies]
flowscope-core = { path = "../flowscope-core", default-features = false }
flowscope-export = { path = "../flowscope-export", default-features = false, features = ["duckdb"] }
# ... rest unchanged
```

**Step 2: Add WASM export function**

Add to `crates/flowscope-wasm/src/lib.rs`:

```rust
use flowscope_export::export_duckdb;

/// Export analysis result to DuckDB database bytes.
///
/// Takes a JSON-serialized AnalyzeResult (from analyze_sql_json output).
/// Returns raw bytes of the DuckDB database file.
#[wasm_bindgen]
pub fn export_to_duckdb(result_json: &str) -> Result<Vec<u8>, JsValue> {
    let result: AnalyzeResult = serde_json::from_str(result_json)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse result: {e}")))?;

    export_duckdb(&result)
        .map_err(|e| JsValue::from_str(&format!("Export failed: {e}")))
}
```

**Step 3: Build WASM**

Run: `wasm-pack build crates/flowscope-wasm --target web`
Expected: Builds successfully

Note: DuckDB WASM compilation may need special handling. If it fails, we may need to investigate duckdb-wasm alternatives for browser.

**Step 4: Commit**

```bash
git add crates/flowscope-wasm/
git commit -m "feat(wasm): add export_to_duckdb function for browser export"
```

---

## Task 7: Add frontend export option

**Files:**
- Modify: `app/src/components/ExportDialog.tsx`

**Step 1: Add DuckDB export handler**

Add import at top of `ExportDialog.tsx`:
```typescript
import { export_to_duckdb } from 'flowscope-wasm';
```

Add export function:
```typescript
async function exportToDuckDB(
  result: AnalyzeResult,
  projectName: string
): Promise<void> {
  try {
    const resultJson = JSON.stringify(result);
    const bytes = export_to_duckdb(resultJson);
    const blob = new Blob([bytes], { type: 'application/octet-stream' });
    const filename = generateFilename(projectName, 'duckdb');

    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);

    toast.success('DuckDB export complete', {
      description: `Saved as ${filename}`,
    });
  } catch (error) {
    toast.error('DuckDB export failed', {
      description: error instanceof Error ? error.message : 'Unknown error',
    });
  }
}
```

**Step 2: Add menu item**

In the export dropdown menu, add:
```tsx
<DropdownMenuSeparator />
<DropdownMenuLabel>Database</DropdownMenuLabel>
<DropdownMenuItem
  onClick={() => result && exportToDuckDB(result, projectName)}
  disabled={!result}
>
  <Database className="mr-2 h-4 w-4" />
  DuckDB (.duckdb)
</DropdownMenuItem>
```

Add Database icon import:
```typescript
import { Database } from 'lucide-react';
```

**Step 3: Test in browser**

Run: `cd app && yarn dev`
Expected: Export dialog shows DuckDB option, clicking it downloads .duckdb file

**Step 4: Commit**

```bash
git add app/src/components/ExportDialog.tsx
git commit -m "feat(app): add DuckDB export to export dialog"
```

---

## Task 8: Write integration test

**Files:**
- Create: `crates/flowscope-export/tests/integration.rs`

**Step 1: Create integration test**

Create `crates/flowscope-export/tests/integration.rs`:

```rust
//! Integration tests for DuckDB export.

use duckdb::Connection;
use flowscope_core::{analyze, AnalyzeRequest, Dialect};
use flowscope_export::export_duckdb;
use tempfile::NamedTempFile;

fn export_and_open(sql: &str) -> Connection {
    let request = AnalyzeRequest {
        sql: sql.to_string(),
        dialect: Dialect::Generic,
        ..Default::default()
    };
    let result = analyze(&request);
    let bytes = export_duckdb(&result).expect("Export should succeed");

    let temp_file = NamedTempFile::new().unwrap();
    std::fs::write(temp_file.path(), &bytes).unwrap();
    Connection::open(temp_file.path()).unwrap()
}

#[test]
fn test_column_lineage_view() {
    let conn = export_and_open(
        "SELECT u.name AS user_name, u.email FROM users u"
    );

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM column_lineage", [], |r| r.get(0))
        .unwrap();
    assert!(count > 0, "column_lineage should have rows");
}

#[test]
fn test_table_dependencies_view() {
    let conn = export_and_open(
        "INSERT INTO archive SELECT * FROM users WHERE created_at < '2024-01-01'"
    );

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM table_dependencies", [], |r| r.get(0))
        .unwrap();
    assert!(count > 0, "table_dependencies should have rows");
}

#[test]
fn test_complexity_view() {
    let conn = export_and_open(
        "SELECT a.*, b.*, c.* FROM a JOIN b ON a.id = b.a_id JOIN c ON b.id = c.b_id WHERE a.active"
    );

    let (complexity, join_count): (i64, i64) = conn
        .query_row(
            "SELECT complexity_score, join_count FROM complexity_by_statement LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?))
        )
        .unwrap();

    assert!(complexity > 0, "Should have complexity score");
    assert_eq!(join_count, 2, "Should have 2 joins");
}

#[test]
fn test_issues_view() {
    let conn = export_and_open("SELECT * FROM");  // Invalid SQL

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM issues_summary WHERE severity = 'error'",
            [],
            |r| r.get(0)
        )
        .unwrap();
    assert!(count > 0, "Should have error issues");
}

#[test]
fn test_meta_table() {
    let conn = export_and_open("SELECT 1");

    let version: String = conn
        .query_row("SELECT value FROM _meta WHERE key = 'version'", [], |r| r.get(0))
        .unwrap();
    assert!(!version.is_empty(), "Version should be set");

    let exported_at: String = conn
        .query_row("SELECT value FROM _meta WHERE key = 'exported_at'", [], |r| r.get(0))
        .unwrap();
    assert!(exported_at.contains("T"), "Should be ISO timestamp");
}

#[test]
fn test_recursive_ancestors_view() {
    let conn = export_and_open(
        "WITH cte AS (SELECT id FROM source) SELECT id AS final_id FROM cte"
    );

    // Just verify the view works without error
    let _: i64 = conn
        .query_row("SELECT COUNT(*) FROM column_ancestors", [], |r| r.get(0))
        .unwrap();
}
```

**Step 2: Run integration tests**

Run: `cargo test -p flowscope-export --test integration`
Expected: All tests pass

**Step 3: Commit**

```bash
git add crates/flowscope-export/tests/
git commit -m "test(export): add integration tests for DuckDB export views"
```

---

## Task 9: Update documentation

**Files:**
- Modify: `crates/flowscope-export/README.md` (create)
- Modify: `crates/flowscope-cli/README.md`

**Step 1: Create export crate README**

Create `crates/flowscope-export/README.md`:

```markdown
# flowscope-export

Database export for FlowScope analysis results.

## Overview

Exports `AnalyzeResult` to queryable database formats, enabling ad-hoc SQL queries against lineage data.

## Supported Formats

- **DuckDB** - Analytical database optimized for complex queries

## Usage

### Rust

```rust
use flowscope_core::{analyze, AnalyzeRequest};
use flowscope_export::export_duckdb;

let request = AnalyzeRequest {
    sql: "SELECT * FROM users".to_string(),
    ..Default::default()
};
let result = analyze(&request);
let db_bytes = export_duckdb(&result)?;
std::fs::write("lineage.duckdb", db_bytes)?;
```

### CLI

```bash
flowscope analyze query.sql --format duckdb -o lineage.duckdb
```

### Query Examples

```sql
-- Find all columns derived from users.email
SELECT * FROM column_descendants
WHERE column_table LIKE '%users%' AND column_name = 'email';

-- Show most complex statements
SELECT * FROM complexity_by_statement
ORDER BY complexity_score DESC LIMIT 10;

-- List all errors
SELECT * FROM issues_summary WHERE severity = 'error';
```

## Database Schema

### Core Tables

- `_meta` - Export metadata (version, timestamp)
- `statements` - Analyzed SQL statements
- `nodes` - Graph nodes (tables, columns, CTEs)
- `edges` - Graph edges (data flow relationships)
- `joins` - Join metadata
- `filters` - WHERE/HAVING predicates
- `aggregations` - GROUP BY information
- `issues` - Analysis errors and warnings
- `schema_tables` / `schema_columns` - Schema information

### Views

**Lineage:**
- `column_lineage` - Direct column-to-column mappings
- `table_dependencies` - Table-level data flow
- `column_ancestors` - Recursive upstream lineage
- `column_descendants` - Recursive downstream lineage

**Graph:**
- `node_details` - Denormalized node information
- `edge_details` - Denormalized edge information
- `join_graph` - All joins with conditions
- `node_filters` - Filters by node

**Metrics:**
- `complexity_by_statement` - Complexity breakdown
- `issues_summary` - Issues with context
- `table_usage` - Table reference counts
- `column_connectivity` - Most connected columns
- `statements_with_issues` - Statements grouped by issue count

**Compliance:**
- `data_flow_paths` - Full lineage paths
- `columns_by_source_table` - Impact analysis
- `transformation_audit` - Expression transformations
- `cross_statement_flow` - Cross-statement dependencies
- `schema_coverage` - Schema usage analysis
```

**Step 2: Update CLI README**

Add to `crates/flowscope-cli/README.md` output formats section:

```markdown
### DuckDB Export

Export to a queryable DuckDB database:

```bash
flowscope analyze *.sql --format duckdb -o lineage.duckdb
```

Query with DuckDB CLI:
```bash
duckdb lineage.duckdb "SELECT * FROM column_lineage LIMIT 10"
```
```

**Step 3: Commit**

```bash
git add crates/flowscope-export/README.md crates/flowscope-cli/README.md
git commit -m "docs: add documentation for DuckDB export"
```

---

## Summary

After completing all tasks:

1. **New crate:** `flowscope-export` with DuckDB backend
2. **CLI:** `--format duckdb` option
3. **WASM:** `export_to_duckdb()` function
4. **Frontend:** DuckDB option in export dialog
5. **Schema:** 11 core tables + 16 queryable views
6. **Tests:** Unit tests + integration tests
7. **Docs:** README with usage examples

Total: ~9 tasks, each with 3-7 steps
