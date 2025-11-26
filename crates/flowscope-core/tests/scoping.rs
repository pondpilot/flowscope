use flowscope_core::analyze;
use flowscope_core::types::{
    issue_codes, AnalysisOptions, AnalyzeRequest, Dialect, SchemaMetadata, Severity,
};

#[test]
fn test_alias_shadowing_in_subquery() {
    let sql = "
        SELECT a.id 
        FROM t1 AS a
        WHERE EXISTS (
            SELECT 1 FROM t2 AS a WHERE a.id = 10 -- Inner 'a' is t2
        )
        AND a.id = 20 -- Outer 'a' should be t1
    ";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: Some("test_scoping".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
    };

    let result = analyze(&request);

    // Check that we have nodes for t1 and t2
    let t1_nodes: Vec<_> = result.statements[0]
        .nodes
        .iter()
        .filter(|n| &*n.label == "t1")
        .collect();
    let t2_nodes: Vec<_> = result.statements[0]
        .nodes
        .iter()
        .filter(|n| &*n.label == "t2")
        .collect();

    assert!(!t1_nodes.is_empty(), "t1 should be present");
    assert!(!t2_nodes.is_empty(), "t2 should be present");

    // Check for issues (ambiguity or unresolved references)
    assert!(
        result.issues.is_empty(),
        "Should be no analysis issues: {:?}",
        result.issues
    );

    // We can also verify that the output column 'id' comes from t1
    // The query outputs `a.id`. 'a' is t1. So it should come from t1.
    // Let's check output columns of the statement.
    // The result.statements[0] is StatementLineage.
    // We can check edges.

    let edges = &result.statements[0].edges;
    let t1_id = &t1_nodes[0].id;

    // Find ownership edge from t1 to a column
    let t1_cols: Vec<_> = edges
        .iter()
        .filter(|e| e.from == *t1_id && e.edge_type == flowscope_core::types::EdgeType::Ownership)
        .map(|e| &e.to)
        .collect();

    assert!(!t1_cols.is_empty(), "t1 should have columns");

    // There should be a data flow edge from one of t1's columns to the output column
    let flows_from_t1 = edges.iter().any(|e| {
        // Edge from a column of t1
        t1_cols.contains(&&e.from) && e.edge_type == flowscope_core::types::EdgeType::DataFlow
    });

    assert!(flows_from_t1, "Output should flow from t1");

    // It should NOT flow from t2 (except maybe via filter dependency? but pure data flow for SELECT list comes from t1)
    let t2_id = &t2_nodes[0].id;
    let t2_cols: Vec<_> = edges
        .iter()
        .filter(|e| e.from == *t2_id && e.edge_type == flowscope_core::types::EdgeType::Ownership)
        .map(|e| &e.to)
        .collect();

    let flows_from_t2_data = edges.iter().any(|e| {
        t2_cols.contains(&&e.from) && e.edge_type == flowscope_core::types::EdgeType::DataFlow
    });

    // The subquery is in WHERE EXISTS, so it contributes to filtering, not data flow in projection.
    // So there should be no DataFlow edge from t2 to the output column.

    assert!(
        !flows_from_t2_data,
        "Output should NOT flow from t2 (it is only in WHERE clause)"
    );
}

#[test]
fn new_tables_are_known_when_implied_schema_disabled() {
    let sql = "
        CREATE TABLE foo (id INT);
        SELECT * FROM foo;
    ";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: Some("test_implied_disabled".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: Some(SchemaMetadata {
            default_schema: Some("public".to_string()),
            allow_implied: false,
            ..Default::default()
        }),
    };

    let result = analyze(&request);

    assert_eq!(result.statements.len(), 2, "Expected CREATE + SELECT");
    assert!(
        result
            .issues
            .iter()
            .all(|issue| issue.code != issue_codes::UNRESOLVED_REFERENCE),
        "Should not warn about unresolved tables: {:?}",
        result.issues
    );

    let select_tables: Vec<_> = result.statements[1]
        .nodes
        .iter()
        .filter(|n| n.node_type == flowscope_core::types::NodeType::Table)
        .collect();

    assert_eq!(select_tables.len(), 1, "SELECT should reference foo once");
    assert_eq!(&*select_tables[0].label, "foo");
    assert!(
        select_tables[0]
            .metadata
            .as_ref()
            .map_or(true, |m| !m.contains_key("placeholder")),
        "Table node should not be marked as placeholder"
    );
}

#[test]
fn missing_table_warned_when_other_tables_known() {
    // When we have some knowledge (from DDL), we should warn about unknown tables.
    let sql = "
        CREATE TABLE foo AS SELECT 1 as id;
        SELECT * FROM missing_table;
    ";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: Some("test_missing_warned".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: Some(SchemaMetadata {
            default_schema: Some("public".to_string()),
            allow_implied: false,
            ..Default::default()
        }),
    };

    let result = analyze(&request);

    assert_eq!(result.statements.len(), 2, "Expected CREATE + SELECT");

    // Should have an UNRESOLVED_REFERENCE warning for missing_table
    let unresolved_warnings: Vec<_> = result
        .issues
        .iter()
        .filter(|issue| {
            issue.code == issue_codes::UNRESOLVED_REFERENCE && issue.severity == Severity::Warning
        })
        .collect();

    assert_eq!(
        unresolved_warnings.len(),
        1,
        "Should have exactly one unresolved reference warning: {:?}",
        result.issues
    );
    assert!(
        unresolved_warnings[0].message.contains("missing_table")
            || unresolved_warnings[0]
                .message
                .contains("public.missing_table"),
        "Warning should mention missing_table: {:?}",
        unresolved_warnings[0]
    );
}
