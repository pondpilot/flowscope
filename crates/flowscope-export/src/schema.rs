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
