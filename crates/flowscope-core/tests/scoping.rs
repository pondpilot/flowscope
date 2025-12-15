use flowscope_core::analyze;
use flowscope_core::types::{
    issue_codes, AnalysisOptions, AnalyzeRequest, Dialect, SchemaMetadata, Severity,
};

// ============================================================================
// ALIAS VISIBILITY RULES TESTS
// ============================================================================
// These tests verify that the analyzer emits warnings when SELECT aliases
// are used in clauses where the dialect doesn't support them.
// See: specs/dialect-semantics/scoping_rules.toml
// ============================================================================

/// Helper to count warnings matching a predicate
fn count_warnings<F>(result: &flowscope_core::types::AnalyzeResult, predicate: F) -> usize
where
    F: Fn(&flowscope_core::types::Issue) -> bool,
{
    result
        .issues
        .iter()
        .filter(|issue| issue.severity == Severity::Warning && predicate(issue))
        .count()
}

// --- GROUP BY alias tests ---
// NOTE: GROUP BY alias checking is a known limitation of the current implementation.
// The check happens before projection analysis, so output_columns are not yet
// populated. These tests document the current behavior and verify no crashes occur.
// When GROUP BY alias checking is implemented, these tests should be updated to
// verify actual warnings.

#[test]
fn test_alias_in_group_by_mysql_no_crash() {
    // MySQL allows alias references in GROUP BY.
    // This test verifies the query processes without crashing.
    let sql = "SELECT x + y AS sum FROM t GROUP BY sum";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Mysql,
        source_name: Some("test_group_by_mysql".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
        tag_hints: None,
    };

    let result = analyze(&request);

    // Verify the query processes successfully
    assert_eq!(result.statements.len(), 1);
    // No GROUP BY warnings expected for MySQL (it allows aliases in GROUP BY)
    let alias_warnings = count_warnings(&result, |issue| {
        issue.code == issue_codes::UNSUPPORTED_SYNTAX && issue.message.contains("GROUP BY")
    });
    assert_eq!(
        alias_warnings, 0,
        "MySQL should not warn about alias in GROUP BY: {:?}",
        result.issues
    );
}

#[test]
fn test_alias_in_group_by_postgres_no_crash() {
    // PostgreSQL does NOT allow alias references in GROUP BY.
    // This test verifies the query processes without crashing.
    // NOTE: Warnings for alias in GROUP BY are not yet implemented because
    // GROUP BY is analyzed before the projection, so aliases aren't known.
    let sql = "SELECT x + y AS sum FROM t GROUP BY sum";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: Some("test_group_by_postgres".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
        tag_hints: None,
    };

    let result = analyze(&request);

    // Verify the query processes successfully
    assert_eq!(result.statements.len(), 1);
    // NOTE: Currently no warning is emitted because GROUP BY is analyzed before
    // projection. When this limitation is addressed, this test should assert
    // that exactly 1 warning is emitted for the alias 'sum' in GROUP BY.
}

// --- HAVING alias tests ---

#[test]
fn test_alias_in_having_mysql_allowed() {
    // MySQL allows alias references in HAVING
    let sql = "SELECT COUNT(*) AS cnt FROM t GROUP BY x HAVING cnt > 5";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Mysql,
        source_name: Some("test_having_mysql".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
        tag_hints: None,
    };

    let result = analyze(&request);

    let alias_warnings = count_warnings(&result, |issue| {
        issue.code == issue_codes::UNSUPPORTED_SYNTAX && issue.message.contains("HAVING")
    });

    assert_eq!(
        alias_warnings, 0,
        "MySQL should not warn about alias in HAVING: {:?}",
        result.issues
    );
}

#[test]
fn test_alias_in_having_postgres_warned() {
    // PostgreSQL does NOT allow alias references in HAVING
    let sql = "SELECT COUNT(*) AS cnt FROM t GROUP BY x HAVING cnt > 5";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: Some("test_having_postgres".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
        tag_hints: None,
    };

    let result = analyze(&request);

    let alias_warnings = count_warnings(&result, |issue| {
        issue.code == issue_codes::UNSUPPORTED_SYNTAX && issue.message.contains("HAVING")
    });

    assert_eq!(
        alias_warnings, 1,
        "PostgreSQL should warn about alias 'cnt' in HAVING: {:?}",
        result.issues
    );

    // Verify the warning message contains the alias name
    let warning = result
        .issues
        .iter()
        .find(|i| i.code == issue_codes::UNSUPPORTED_SYNTAX && i.message.contains("HAVING"))
        .expect("Should have a HAVING warning");
    assert!(
        warning.message.contains("cnt"),
        "Warning should mention the alias 'cnt': {}",
        warning.message
    );
}

#[test]
fn test_alias_in_having_snowflake_warned() {
    // Snowflake does NOT allow alias references in HAVING
    let sql = "SELECT SUM(amount) AS total FROM orders GROUP BY customer_id HAVING total > 1000";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Snowflake,
        source_name: Some("test_having_snowflake".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
        tag_hints: None,
    };

    let result = analyze(&request);

    let alias_warnings = count_warnings(&result, |issue| {
        issue.code == issue_codes::UNSUPPORTED_SYNTAX && issue.message.contains("HAVING")
    });

    assert_eq!(
        alias_warnings, 1,
        "Snowflake should warn about alias 'total' in HAVING: {:?}",
        result.issues
    );
}

// --- ORDER BY alias tests ---

#[test]
fn test_alias_in_order_by_all_dialects_allowed() {
    // Almost all dialects allow alias references in ORDER BY
    // Let's test with Postgres which allows ORDER BY aliases
    let sql = "SELECT x + y AS sum FROM t ORDER BY sum";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: Some("test_order_by_postgres".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
        tag_hints: None,
    };

    let result = analyze(&request);

    let alias_warnings = count_warnings(&result, |issue| {
        issue.code == issue_codes::UNSUPPORTED_SYNTAX && issue.message.contains("ORDER BY")
    });

    assert_eq!(
        alias_warnings, 0,
        "Postgres should not warn about alias in ORDER BY: {:?}",
        result.issues
    );
}

// --- Lateral column alias tests ---

#[test]
fn test_lateral_column_alias_bigquery_allowed() {
    // BigQuery supports lateral column aliases
    let sql = "SELECT x + y AS sum, sum * 2 AS double_sum FROM t";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Bigquery,
        source_name: Some("test_lateral_bigquery".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
        tag_hints: None,
    };

    let result = analyze(&request);

    let alias_warnings = count_warnings(&result, |issue| {
        issue.code == issue_codes::UNSUPPORTED_SYNTAX
            && issue.message.contains("lateral column alias")
    });

    assert_eq!(
        alias_warnings, 0,
        "BigQuery should not warn about lateral column alias: {:?}",
        result.issues
    );
}

#[test]
fn test_lateral_column_alias_postgres_warned() {
    // PostgreSQL does NOT support lateral column aliases
    let sql = "SELECT x + y AS sum, sum * 2 AS double_sum FROM t";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: Some("test_lateral_postgres".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
        tag_hints: None,
    };

    let result = analyze(&request);

    let alias_warnings = count_warnings(&result, |issue| {
        issue.code == issue_codes::UNSUPPORTED_SYNTAX
            && issue.message.contains("lateral column alias")
    });

    assert_eq!(
        alias_warnings, 1,
        "PostgreSQL should warn about lateral column alias 'sum': {:?}",
        result.issues
    );

    // Verify the warning message contains the alias name
    let warning = result
        .issues
        .iter()
        .find(|i| {
            i.code == issue_codes::UNSUPPORTED_SYNTAX && i.message.contains("lateral column alias")
        })
        .expect("Should have a lateral column alias warning");
    assert!(
        warning.message.contains("sum"),
        "Warning should mention the alias 'sum': {}",
        warning.message
    );
}

#[test]
fn test_lateral_column_alias_mysql_warned() {
    // MySQL does NOT support lateral column aliases (unlike GROUP BY/HAVING)
    let sql = "SELECT price * quantity AS total, total * tax_rate AS tax FROM orders";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Mysql,
        source_name: Some("test_lateral_mysql".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
        tag_hints: None,
    };

    let result = analyze(&request);

    let alias_warnings = count_warnings(&result, |issue| {
        issue.code == issue_codes::UNSUPPORTED_SYNTAX
            && issue.message.contains("lateral column alias")
    });

    assert_eq!(
        alias_warnings, 1,
        "MySQL should warn about lateral column alias 'total': {:?}",
        result.issues
    );
}

#[test]
fn test_lateral_column_alias_snowflake_allowed() {
    // Snowflake supports lateral column aliases
    let sql = "SELECT a + b AS sum, sum / 2 AS half FROM t";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Snowflake,
        source_name: Some("test_lateral_snowflake".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
        tag_hints: None,
    };

    let result = analyze(&request);

    let alias_warnings = count_warnings(&result, |issue| {
        issue.code == issue_codes::UNSUPPORTED_SYNTAX
            && issue.message.contains("lateral column alias")
    });

    assert_eq!(
        alias_warnings, 0,
        "Snowflake should not warn about lateral column alias: {:?}",
        result.issues
    );
}

#[test]
fn test_no_lateral_warning_for_first_item() {
    // The first SELECT item can't have a lateral alias (nothing defined yet)
    let sql = "SELECT x AS a FROM t";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: Some("test_no_lateral_first".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
        tag_hints: None,
    };

    let result = analyze(&request);

    let alias_warnings = count_warnings(&result, |issue| {
        issue.code == issue_codes::UNSUPPORTED_SYNTAX
            && issue.message.contains("lateral column alias")
    });

    assert_eq!(
        alias_warnings, 0,
        "Should not warn for first SELECT item: {:?}",
        result.issues
    );
}

// --- Multiple alias violations in same query ---

#[test]
fn test_multiple_lateral_violations() {
    // Multiple lateral alias violations should produce multiple warnings
    let sql = "SELECT a AS x, x AS y, y AS z FROM t";

    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: Some("test_multiple_lateral".to_string()),
        options: Some(AnalysisOptions {
            enable_column_lineage: Some(true),
            ..Default::default()
        }),
        schema: None,
        tag_hints: None,
    };

    let result = analyze(&request);

    let alias_warnings = count_warnings(&result, |issue| {
        issue.code == issue_codes::UNSUPPORTED_SYNTAX
            && issue.message.contains("lateral column alias")
    });

    // 'x' is used in second item (warning), 'y' is used in third item (warning)
    assert_eq!(
        alias_warnings, 2,
        "Should have warnings for both 'x' and 'y': {:?}",
        result.issues
    );
}

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
        tag_hints: None,
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
        tag_hints: None,
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
        tag_hints: None,
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
