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
