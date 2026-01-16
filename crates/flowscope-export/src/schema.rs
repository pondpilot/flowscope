//! Database schema definitions (DDL).

/// SQL to create all tables.
///
/// The `prefix` parameter is used to namespace all tables (e.g., "myschema." or "").
pub fn tables_ddl(prefix: &str) -> String {
    format!(
        r#"
-- Metadata about the export
CREATE TABLE {prefix}_meta (
    key TEXT PRIMARY KEY,
    value TEXT
);

-- SQL statements analyzed
CREATE TABLE {prefix}statements (
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
-- Composite key allows same logical node to appear in multiple statements
CREATE TABLE {prefix}nodes (
    id TEXT NOT NULL,
    statement_id INTEGER NOT NULL REFERENCES {prefix}statements(id),
    node_type TEXT NOT NULL,
    label TEXT NOT NULL,
    qualified_name TEXT,
    expression TEXT,
    span_start INTEGER,
    span_end INTEGER,
    resolution_source TEXT,
    PRIMARY KEY (id, statement_id)
);

-- Graph edges (data flow relationships)
CREATE TABLE {prefix}edges (
    id INTEGER PRIMARY KEY,
    statement_id INTEGER NOT NULL REFERENCES {prefix}statements(id),
    edge_type TEXT NOT NULL,
    from_node_id TEXT NOT NULL,
    to_node_id TEXT NOT NULL,
    expression TEXT,
    operation TEXT,
    is_approximate BOOLEAN DEFAULT FALSE,
    FOREIGN KEY (from_node_id, statement_id) REFERENCES {prefix}nodes(id, statement_id),
    FOREIGN KEY (to_node_id, statement_id) REFERENCES {prefix}nodes(id, statement_id)
);

-- Join metadata (linked to nodes with join info)
CREATE TABLE {prefix}joins (
    id INTEGER PRIMARY KEY,
    node_id TEXT NOT NULL,
    statement_id INTEGER NOT NULL,
    join_type TEXT NOT NULL,
    join_condition TEXT,
    FOREIGN KEY (node_id, statement_id) REFERENCES {prefix}nodes(id, statement_id)
);

-- Filter predicates on nodes
CREATE TABLE {prefix}filters (
    id INTEGER PRIMARY KEY,
    node_id TEXT NOT NULL,
    statement_id INTEGER NOT NULL,
    predicate TEXT NOT NULL,
    filter_type TEXT,
    FOREIGN KEY (node_id, statement_id) REFERENCES {prefix}nodes(id, statement_id)
);

-- Aggregation info on column nodes
CREATE TABLE {prefix}aggregations (
    node_id TEXT NOT NULL,
    statement_id INTEGER NOT NULL,
    is_grouping_key BOOLEAN NOT NULL,
    function TEXT,
    is_distinct BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (node_id, statement_id),
    FOREIGN KEY (node_id, statement_id) REFERENCES {prefix}nodes(id, statement_id)
);

-- Analysis issues
CREATE TABLE {prefix}issues (
    id INTEGER PRIMARY KEY,
    statement_id INTEGER REFERENCES {prefix}statements(id),
    severity TEXT NOT NULL,
    code TEXT NOT NULL,
    message TEXT NOT NULL,
    span_start INTEGER,
    span_end INTEGER
);

-- Schema tables (imported + inferred)
CREATE TABLE {prefix}schema_tables (
    id INTEGER PRIMARY KEY,
    catalog TEXT,
    schema_name TEXT,
    name TEXT NOT NULL,
    resolution_source TEXT,
    UNIQUE(catalog, schema_name, name)
);

-- Schema columns
CREATE TABLE {prefix}schema_columns (
    id INTEGER PRIMARY KEY,
    table_id INTEGER NOT NULL REFERENCES {prefix}schema_tables(id),
    name TEXT NOT NULL,
    data_type TEXT,
    is_nullable BOOLEAN,
    is_primary_key BOOLEAN DEFAULT FALSE
);

-- Global nodes (cross-statement)
CREATE TABLE {prefix}global_nodes (
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
CREATE TABLE {prefix}global_edges (
    id TEXT PRIMARY KEY,
    from_node_id TEXT NOT NULL,
    to_node_id TEXT NOT NULL,
    edge_type TEXT NOT NULL
);

-- Statement references for global nodes
CREATE TABLE {prefix}global_node_statement_refs (
    id INTEGER PRIMARY KEY,
    global_node_id TEXT NOT NULL REFERENCES {prefix}global_nodes(id),
    statement_index INTEGER NOT NULL,
    local_node_id TEXT
);
"#,
        prefix = prefix
    )
}

/// SQL to create all views.
///
/// The `prefix` parameter is used to namespace all views and table references
/// (e.g., "myschema." or "").
pub fn views_ddl(prefix: &str) -> String {
    format!(
        r#"
-- ============================================================================
-- LINEAGE VIEWS
-- ============================================================================

-- Direct column-to-column lineage
CREATE VIEW {prefix}column_lineage AS
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
FROM {prefix}edges e
JOIN {prefix}nodes fn ON e.from_node_id = fn.id AND e.statement_id = fn.statement_id
JOIN {prefix}nodes tn ON e.to_node_id = tn.id AND e.statement_id = tn.statement_id
JOIN {prefix}statements s ON e.statement_id = s.id
WHERE fn.node_type = 'column'
  AND tn.node_type = 'column'
  AND e.edge_type IN ('data_flow', 'derivation');

-- Table-level dependencies
CREATE VIEW {prefix}table_dependencies AS
SELECT DISTINCT
    s.source_name,
    fn.qualified_name AS source_table,
    tn.qualified_name AS target_table,
    e.edge_type
FROM {prefix}edges e
JOIN {prefix}nodes fn ON e.from_node_id = fn.id AND e.statement_id = fn.statement_id
JOIN {prefix}nodes tn ON e.to_node_id = tn.id AND e.statement_id = tn.statement_id
JOIN {prefix}statements s ON e.statement_id = s.id
WHERE fn.node_type IN ('table', 'view', 'cte')
  AND tn.node_type IN ('table', 'view', 'cte');

-- Recursive: all upstream columns
CREATE VIEW {prefix}column_ancestors AS
WITH RECURSIVE ancestors AS (
    SELECT
        to_node_id AS column_id,
        from_node_id AS ancestor_id,
        1 AS depth,
        expression AS transformation
    FROM {prefix}edges
    WHERE edge_type IN ('data_flow', 'derivation')

    UNION ALL

    SELECT
        a.column_id,
        e.from_node_id AS ancestor_id,
        a.depth + 1,
        e.expression
    FROM ancestors a
    JOIN {prefix}edges e ON a.ancestor_id = e.to_node_id
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
JOIN {prefix}nodes n1 ON a.column_id = n1.id
JOIN {prefix}nodes n2 ON a.ancestor_id = n2.id
WHERE n1.node_type = 'column'
  AND n2.node_type = 'column';

-- Recursive: all downstream columns
CREATE VIEW {prefix}column_descendants AS
WITH RECURSIVE descendants AS (
    SELECT
        from_node_id AS column_id,
        to_node_id AS descendant_id,
        1 AS depth,
        expression AS transformation
    FROM {prefix}edges
    WHERE edge_type IN ('data_flow', 'derivation')

    UNION ALL

    SELECT
        d.column_id,
        e.to_node_id AS descendant_id,
        d.depth + 1,
        e.expression
    FROM descendants d
    JOIN {prefix}edges e ON d.descendant_id = e.from_node_id
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
JOIN {prefix}nodes n1 ON d.column_id = n1.id
JOIN {prefix}nodes n2 ON d.descendant_id = n2.id
WHERE n1.node_type = 'column'
  AND n2.node_type = 'column';

-- ============================================================================
-- GRAPH VIEWS
-- ============================================================================

-- Denormalized node details
CREATE VIEW {prefix}node_details AS
SELECT
    n.id,
    n.statement_id,
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
FROM {prefix}nodes n
LEFT JOIN {prefix}statements s ON n.statement_id = s.id
LEFT JOIN {prefix}aggregations a ON n.id = a.node_id AND n.statement_id = a.statement_id;

-- Denormalized edge details
CREATE VIEW {prefix}edge_details AS
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
FROM {prefix}edges e
JOIN {prefix}nodes fn ON e.from_node_id = fn.id AND e.statement_id = fn.statement_id
JOIN {prefix}nodes tn ON e.to_node_id = tn.id AND e.statement_id = tn.statement_id
LEFT JOIN {prefix}joins j ON fn.id = j.node_id AND fn.statement_id = j.statement_id
LEFT JOIN {prefix}statements s ON e.statement_id = s.id;

-- All joins with context
CREATE VIEW {prefix}join_graph AS
SELECT
    s.source_name,
    s.statement_index,
    j.join_type,
    j.join_condition,
    n.qualified_name AS table_name,
    n.label AS table_label
FROM {prefix}joins j
JOIN {prefix}nodes n ON j.node_id = n.id AND j.statement_id = n.statement_id
LEFT JOIN {prefix}statements s ON n.statement_id = s.id;

-- Filters applied to nodes
CREATE VIEW {prefix}node_filters AS
SELECT
    n.qualified_name AS table_name,
    n.label AS node_label,
    n.node_type,
    f.predicate,
    f.filter_type,
    s.source_name,
    s.statement_index
FROM {prefix}filters f
JOIN {prefix}nodes n ON f.node_id = n.id AND f.statement_id = n.statement_id
LEFT JOIN {prefix}statements s ON n.statement_id = s.id;

-- ============================================================================
-- METRICS VIEWS
-- ============================================================================

-- Complexity breakdown by statement
CREATE VIEW {prefix}complexity_by_statement AS
SELECT
    s.source_name,
    s.statement_index,
    s.statement_type,
    s.complexity_score,
    s.join_count,
    COUNT(DISTINCT CASE WHEN n.node_type = 'table' THEN n.id END) AS table_count,
    COUNT(DISTINCT CASE WHEN n.node_type = 'column' THEN n.id END) AS column_count,
    COUNT(DISTINCT e.id) AS edge_count
FROM {prefix}statements s
LEFT JOIN {prefix}nodes n ON n.statement_id = s.id
LEFT JOIN {prefix}edges e ON e.statement_id = s.id
GROUP BY s.id, s.source_name, s.statement_index, s.statement_type,
         s.complexity_score, s.join_count;

-- Issue summary with context
CREATE VIEW {prefix}issues_summary AS
SELECT
    i.severity,
    i.code,
    i.message,
    s.source_name,
    s.statement_index,
    s.statement_type,
    i.span_start,
    i.span_end
FROM {prefix}issues i
LEFT JOIN {prefix}statements s ON i.statement_id = s.id;

-- Table usage statistics
CREATE VIEW {prefix}table_usage AS
SELECT
    n.qualified_name AS table_name,
    n.node_type,
    n.resolution_source,
    COUNT(DISTINCT n.statement_id) AS statement_count,
    COUNT(DISTINCT e_in.id) AS incoming_edges,
    COUNT(DISTINCT e_out.id) AS outgoing_edges
FROM {prefix}nodes n
LEFT JOIN {prefix}edges e_in ON n.id = e_in.to_node_id
LEFT JOIN {prefix}edges e_out ON n.id = e_out.from_node_id
WHERE n.node_type IN ('table', 'view', 'cte')
GROUP BY n.qualified_name, n.node_type, n.resolution_source;

-- Most connected columns
CREATE VIEW {prefix}column_connectivity AS
SELECT
    n.qualified_name AS table_name,
    n.label AS column_name,
    COUNT(DISTINCT e_in.id) AS upstream_count,
    COUNT(DISTINCT e_out.id) AS downstream_count,
    COUNT(DISTINCT e_in.id) + COUNT(DISTINCT e_out.id) AS total_connections
FROM {prefix}nodes n
LEFT JOIN {prefix}edges e_in ON n.id = e_in.to_node_id
LEFT JOIN {prefix}edges e_out ON n.id = e_out.from_node_id
WHERE n.node_type = 'column'
GROUP BY n.id, n.qualified_name, n.label
HAVING COUNT(DISTINCT e_in.id) + COUNT(DISTINCT e_out.id) > 0
ORDER BY total_connections DESC;

-- Statements with issues
CREATE VIEW {prefix}statements_with_issues AS
SELECT
    s.source_name,
    s.statement_index,
    s.statement_type,
    s.complexity_score,
    COUNT(CASE WHEN i.severity = 'error' THEN 1 END) AS error_count,
    COUNT(CASE WHEN i.severity = 'warning' THEN 1 END) AS warning_count,
    COUNT(CASE WHEN i.severity = 'info' THEN 1 END) AS info_count
FROM {prefix}statements s
JOIN {prefix}issues i ON i.statement_id = s.id
GROUP BY s.id, s.source_name, s.statement_index, s.statement_type, s.complexity_score;

-- ============================================================================
-- COMPLIANCE VIEWS
-- ============================================================================

-- Full data flow paths
CREATE VIEW {prefix}data_flow_paths AS
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
FROM {prefix}edges e
JOIN {prefix}nodes fn ON e.from_node_id = fn.id AND e.statement_id = fn.statement_id
JOIN {prefix}nodes tn ON e.to_node_id = tn.id AND e.statement_id = tn.statement_id
JOIN {prefix}statements s ON e.statement_id = s.id
WHERE e.edge_type IN ('data_flow', 'derivation');

-- Impact analysis: columns by source table
CREATE VIEW {prefix}columns_by_source_table AS
SELECT DISTINCT
    column_table AS source_table,
    column_name AS source_column,
    descendant_table AS affected_table,
    descendant_column AS affected_column,
    depth AS distance
FROM {prefix}column_descendants;

-- Transformation audit
CREATE VIEW {prefix}transformation_audit AS
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
FROM {prefix}edges e
JOIN {prefix}nodes fn ON e.from_node_id = fn.id AND e.statement_id = fn.statement_id
JOIN {prefix}nodes tn ON e.to_node_id = tn.id AND e.statement_id = tn.statement_id
JOIN {prefix}statements s ON e.statement_id = s.id
LEFT JOIN {prefix}aggregations a ON tn.id = a.node_id AND tn.statement_id = a.statement_id
WHERE e.expression IS NOT NULL
   OR a.function IS NOT NULL;

-- Cross-statement dependencies
CREATE VIEW {prefix}cross_statement_flow AS
SELECT
    s1.source_name AS from_source,
    s1.statement_index AS from_statement,
    s2.source_name AS to_source,
    s2.statement_index AS to_statement,
    fn.qualified_name AS shared_object,
    e.edge_type
FROM {prefix}edges e
JOIN {prefix}nodes fn ON e.from_node_id = fn.id AND e.statement_id = fn.statement_id
JOIN {prefix}nodes tn ON e.to_node_id = tn.id AND e.statement_id = tn.statement_id
JOIN {prefix}statements s1 ON fn.statement_id = s1.id
JOIN {prefix}statements s2 ON tn.statement_id = s2.id
WHERE s1.id != s2.id;

-- Schema coverage
CREATE VIEW {prefix}schema_coverage AS
SELECT
    st.catalog,
    st.schema_name,
    st.name AS table_name,
    st.resolution_source,
    CASE WHEN COUNT(n.id) > 0 THEN TRUE ELSE FALSE END AS is_referenced,
    COUNT(DISTINCT n.statement_id) AS reference_count
FROM {prefix}schema_tables st
LEFT JOIN {prefix}nodes n ON n.qualified_name LIKE '%' || st.name || '%'
    AND n.node_type IN ('table', 'view')
GROUP BY st.id, st.catalog, st.schema_name, st.name, st.resolution_source;
"#,
        prefix = prefix
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tables_ddl_no_prefix() {
        let ddl = tables_ddl("");
        assert!(ddl.contains("CREATE TABLE _meta"));
        assert!(ddl.contains("CREATE TABLE statements"));
        assert!(ddl.contains("CREATE TABLE nodes"));
        assert!(ddl.contains("CREATE TABLE edges"));
        assert!(ddl.contains("CREATE TABLE issues"));
        assert!(ddl.contains("REFERENCES statements(id)"));
        assert!(ddl.contains("REFERENCES nodes(id, statement_id)"));
    }

    #[test]
    fn test_tables_ddl_with_prefix() {
        let ddl = tables_ddl("lineage.");

        // All tables should be prefixed
        assert!(ddl.contains("CREATE TABLE lineage._meta"));
        assert!(ddl.contains("CREATE TABLE lineage.statements"));
        assert!(ddl.contains("CREATE TABLE lineage.nodes"));
        assert!(ddl.contains("CREATE TABLE lineage.edges"));
        assert!(ddl.contains("CREATE TABLE lineage.joins"));
        assert!(ddl.contains("CREATE TABLE lineage.filters"));
        assert!(ddl.contains("CREATE TABLE lineage.aggregations"));
        assert!(ddl.contains("CREATE TABLE lineage.issues"));
        assert!(ddl.contains("CREATE TABLE lineage.schema_tables"));
        assert!(ddl.contains("CREATE TABLE lineage.schema_columns"));
        assert!(ddl.contains("CREATE TABLE lineage.global_nodes"));
        assert!(ddl.contains("CREATE TABLE lineage.global_edges"));
        assert!(ddl.contains("CREATE TABLE lineage.global_node_statement_refs"));

        // Foreign key references should be prefixed
        assert!(ddl.contains("REFERENCES lineage.statements(id)"));
        assert!(ddl.contains("REFERENCES lineage.nodes(id, statement_id)"));
        assert!(ddl.contains("REFERENCES lineage.schema_tables(id)"));
        assert!(ddl.contains("REFERENCES lineage.global_nodes(id)"));

        // No unprefixed table names in CREATE statements
        assert!(!ddl.contains("CREATE TABLE _meta "));
        assert!(!ddl.contains("CREATE TABLE statements "));
        assert!(!ddl.contains("CREATE TABLE nodes "));
    }

    #[test]
    fn test_views_ddl_no_prefix() {
        let ddl = views_ddl("");
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
        // Table references
        assert!(ddl.contains("FROM edges"));
        assert!(ddl.contains("JOIN nodes"));
        assert!(ddl.contains("JOIN statements"));
    }

    #[test]
    fn test_views_ddl_with_prefix() {
        let ddl = views_ddl("lineage.");

        // All views should be prefixed
        assert!(ddl.contains("CREATE VIEW lineage.column_lineage"));
        assert!(ddl.contains("CREATE VIEW lineage.table_dependencies"));
        assert!(ddl.contains("CREATE VIEW lineage.column_ancestors"));
        assert!(ddl.contains("CREATE VIEW lineage.column_descendants"));
        assert!(ddl.contains("CREATE VIEW lineage.node_details"));
        assert!(ddl.contains("CREATE VIEW lineage.edge_details"));
        assert!(ddl.contains("CREATE VIEW lineage.join_graph"));
        assert!(ddl.contains("CREATE VIEW lineage.node_filters"));
        assert!(ddl.contains("CREATE VIEW lineage.complexity_by_statement"));
        assert!(ddl.contains("CREATE VIEW lineage.issues_summary"));
        assert!(ddl.contains("CREATE VIEW lineage.table_usage"));
        assert!(ddl.contains("CREATE VIEW lineage.column_connectivity"));
        assert!(ddl.contains("CREATE VIEW lineage.statements_with_issues"));
        assert!(ddl.contains("CREATE VIEW lineage.data_flow_paths"));
        assert!(ddl.contains("CREATE VIEW lineage.columns_by_source_table"));
        assert!(ddl.contains("CREATE VIEW lineage.transformation_audit"));
        assert!(ddl.contains("CREATE VIEW lineage.cross_statement_flow"));
        assert!(ddl.contains("CREATE VIEW lineage.schema_coverage"));

        // Table references should be prefixed
        assert!(ddl.contains("FROM lineage.edges"));
        assert!(ddl.contains("JOIN lineage.nodes"));
        assert!(ddl.contains("JOIN lineage.statements"));
        assert!(ddl.contains("FROM lineage.schema_tables"));
    }

    #[test]
    fn test_no_double_prefix() {
        let tables = tables_ddl("lineage.");
        let views = views_ddl("lineage.");
        assert!(!tables.contains("lineage.lineage."));
        assert!(!views.contains("lineage.lineage."));
    }
}
