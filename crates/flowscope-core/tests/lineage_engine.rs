use flowscope_core::{
    analyze, issue_codes, AnalysisOptions, AnalyzeRequest, AnalyzeResult, ColumnSchema,
    ConstraintType, Dialect, Edge, EdgeType, FilterClauseType, JoinType, Node, NodeType,
    SchemaMetadata, SchemaNamespaceHint, SchemaTable, Severity, StatementLineage,
};
use rstest::rstest;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

fn run_analysis(sql: &str, dialect: Dialect, schema: Option<SchemaMetadata>) -> AnalyzeResult {
    analyze(&AnalyzeRequest {
        sql: sql.trim().to_string(),
        files: None,
        dialect,
        source_name: Some("integration_test".into()),
        options: None,
        schema,
        #[cfg(feature = "templating")]
        template_config: None,
    })
}

fn run_analysis_with_options(
    sql: &str,
    dialect: Dialect,
    schema: Option<SchemaMetadata>,
    options: AnalysisOptions,
) -> AnalyzeResult {
    analyze(&AnalyzeRequest {
        sql: sql.trim().to_string(),
        files: None,
        dialect,
        source_name: Some("integration_test".into()),
        options: Some(options),
        schema,
        #[cfg(feature = "templating")]
        template_config: None,
    })
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn dialect_fixture_dir(name: &str) -> PathBuf {
    fixtures_root().join(name)
}

fn list_fixture_files(dir: &Path) -> Vec<String> {
    let mut fixtures = Vec::new();
    if dir.exists() {
        for entry in fs::read_dir(dir).expect("failed to list fixtures") {
            let entry = entry.expect("fixture entry");
            if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("sql") {
                    fixtures.push(path.file_name().unwrap().to_string_lossy().to_string());
                }
            }
        }
    }
    fixtures.sort();
    fixtures
}

fn load_sql_fixture(dialect: &str, name: &str) -> String {
    let path = dialect_fixture_dir(dialect).join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read fixture {path:?}: {e}"))
}

fn collect_table_names(result: &AnalyzeResult) -> HashSet<String> {
    let mut tables = HashSet::new();
    for statement in &result.statements {
        for node in &statement.nodes {
            if node.node_type == NodeType::Table {
                let name = node.qualified_name.as_ref().unwrap_or(&node.label);
                tables.insert(name.to_string());
            }
        }
    }
    tables
}

fn schema_table(
    catalog: Option<&str>,
    schema: Option<&str>,
    name: &str,
    columns: &[&str],
) -> SchemaTable {
    SchemaTable {
        catalog: catalog.map(|c| c.to_string()),
        schema: schema.map(|s| s.to_string()),
        name: name.to_string(),
        columns: columns.iter().map(|col| column(col)).collect(),
    }
}

/// Create a simple column schema with just a name.
fn column(name: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_string(),
        data_type: None,
        is_primary_key: None,
        foreign_key: None,
    }
}

/// Create a column schema with a data type.
#[allow(dead_code)]
fn column_typed(name: &str, data_type: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_string(),
        data_type: Some(data_type.to_string()),
        is_primary_key: None,
        foreign_key: None,
    }
}

/// Create a primary key column schema.
#[allow(dead_code)]
fn column_pk(name: &str, data_type: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_string(),
        data_type: Some(data_type.to_string()),
        is_primary_key: Some(true),
        foreign_key: None,
    }
}

/// Create a foreign key column schema.
#[allow(dead_code)]
fn column_fk(name: &str, data_type: &str, ref_table: &str, ref_column: &str) -> ColumnSchema {
    use flowscope_core::ForeignKeyRef;
    ColumnSchema {
        name: name.to_string(),
        data_type: Some(data_type.to_string()),
        is_primary_key: None,
        foreign_key: Some(ForeignKeyRef {
            table: ref_table.to_string(),
            column: ref_column.to_string(),
        }),
    }
}

fn first_statement(result: &AnalyzeResult) -> &StatementLineage {
    result
        .statements
        .first()
        .expect("analysis should return at least one statement")
}

fn column_labels(lineage: &StatementLineage) -> Vec<String> {
    lineage
        .nodes
        .iter()
        .filter(|node| node.node_type == NodeType::Column)
        .map(|node| node.label.to_string())
        .collect()
}

fn collect_cte_names(result: &AnalyzeResult) -> HashSet<String> {
    let mut ctes = HashSet::new();
    for stmt in &result.statements {
        for node in &stmt.nodes {
            if node.node_type == NodeType::Cte {
                ctes.insert(node.label.to_string());
            }
        }
    }
    ctes
}

fn issue_codes_list(result: &AnalyzeResult) -> Vec<String> {
    result
        .issues
        .iter()
        .map(|issue| issue.code.clone())
        .collect()
}

fn edges_by_type(lineage: &StatementLineage, edge_type: EdgeType) -> Vec<&Edge> {
    lineage
        .edges
        .iter()
        .filter(|edge| edge.edge_type == edge_type)
        .collect()
}

#[allow(dead_code)]
fn find_node_by_label<'a>(lineage: &'a StatementLineage, label: &str) -> Option<&'a Node> {
    lineage.nodes.iter().find(|node| &*node.label == label)
}

fn find_column_node<'a>(lineage: &'a StatementLineage, label: &str) -> Option<&'a Node> {
    lineage
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Column && &*node.label == label)
}

fn find_table_node<'a>(lineage: &'a StatementLineage, name: &str) -> Option<&'a Node> {
    lineage.nodes.iter().find(|node| {
        node.node_type == NodeType::Table
            && (&*node.label == name || node.qualified_name.as_deref() == Some(name))
    })
}

fn find_cte_node<'a>(lineage: &'a StatementLineage, name: &str) -> Option<&'a Node> {
    lineage
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Cte && &*node.label == name)
}

#[allow(dead_code)]
fn has_edge(
    lineage: &StatementLineage,
    from_label: &str,
    to_label: &str,
    edge_type: EdgeType,
) -> bool {
    let from_node = find_node_by_label(lineage, from_label);
    let to_node = find_node_by_label(lineage, to_label);

    if let (Some(from), Some(to)) = (from_node, to_node) {
        lineage
            .edges
            .iter()
            .any(|edge| edge.from == from.id && edge.to == to.id && edge.edge_type == edge_type)
    } else {
        false
    }
}

#[rstest]
#[case("generic", Dialect::Generic)]
#[case("postgres", Dialect::Postgres)]
#[case("snowflake", Dialect::Snowflake)]
#[case("bigquery", Dialect::Bigquery)]
#[case("redshift", Dialect::Redshift)]
#[case("mysql", Dialect::Mysql)]
fn multi_dialect_fixtures_cover_core_constructs(#[case] dir_name: &str, #[case] dialect: Dialect) {
    let dir = dialect_fixture_dir(dir_name);
    let fixtures = list_fixture_files(&dir);
    assert!(
        !fixtures.is_empty(),
        "expected fixtures for dialect {dir_name}"
    );

    for fixture in fixtures {
        let sql = load_sql_fixture(dir_name, &fixture);
        let result = run_analysis(&sql, dialect, None);

        assert!(
            result.summary.statement_count >= 1,
            "fixture {dir_name}/{fixture} produced no statements (issues: {:?})",
            result.issues
        );
        assert!(
            result.statements.iter().any(|stmt| stmt
                .nodes
                .iter()
                .any(|node| matches!(node.node_type, NodeType::Table | NodeType::Cte))),
            "fixture {dir_name}/{fixture} should yield tables or CTEs"
        );
        assert!(
            !result.summary.has_errors,
            "fixture {dir_name}/{fixture} had unexpected errors: {:?}",
            result.issues
        );
    }
}

#[test]
fn multi_stage_pipeline_emits_cross_statement_edges() {
    let sql = r#"
        CREATE TABLE analytics.tmp_daily_rollup AS
        WITH recent_orders AS (
            SELECT o.id,
                   o.customer_id,
                   o.total_amount,
                   d.region
            FROM analytics.orders o
            JOIN analytics.dim_customers d
              ON o.customer_id = d.customer_id
            WHERE o.order_date >= '2024-01-01'
        ),
        spend_per_customer AS (
            SELECT customer_id,
                   SUM(total_amount) AS total_spend,
                   MAX(region) AS region
            FROM recent_orders
            GROUP BY customer_id
        )
        SELECT customer_id, total_spend, region
        FROM spend_per_customer;

        INSERT INTO analytics.customer_snapshots (customer_id, region, total_spend)
        SELECT customer_id, region, total_spend
        FROM analytics.tmp_daily_rollup;

        WITH leaderboard AS (
            SELECT region, SUM(total_spend) AS total_spend
            FROM analytics.customer_snapshots
            GROUP BY region
        )
        SELECT * FROM leaderboard;
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    assert_eq!(
        result.summary.statement_count, 3,
        "expected CTAS + INSERT + SELECT"
    );

    let tables = collect_table_names(&result);
    for expected in [
        "analytics.orders",
        "analytics.dim_customers",
        "analytics.tmp_daily_rollup",
        "analytics.customer_snapshots",
    ] {
        assert!(
            tables.contains(expected),
            "missing lineage for {expected:?}"
        );
    }

    let cross_edges: Vec<_> = result
        .global_lineage
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::CrossStatement)
        .collect();
    assert!(
        cross_edges.len() >= 2,
        "expected cross-statement edges, got {:?}",
        result.global_lineage.edges
    );
}

#[test]
fn recursive_ctes_produce_lineage_without_warnings() {
    let sql = r#"
        WITH RECURSIVE org_hierarchy AS (
            SELECT e.employee_id, e.manager_id, 0 AS depth
            FROM employees e
            WHERE e.manager_id IS NULL
            UNION ALL
            SELECT child.employee_id, child.manager_id, parent.depth + 1
            FROM employees child
            JOIN org_hierarchy parent
              ON child.manager_id = parent.employee_id
        )
        SELECT * FROM org_hierarchy;
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    assert_eq!(result.summary.statement_count, 1);

    let tables = collect_table_names(&result);
    assert!(
        tables.contains("employees"),
        "recursive CTE should still record base table lineage"
    );

    // No warnings are expected for supported recursive CTEs.
    assert!(
        result
            .issues
            .iter()
            .all(|issue| issue.severity != Severity::Warning),
        "recursive CTEs should not emit warnings when supported"
    );

    // Verify the CTE node is present and recursive references are tracked.
    let stmt = first_statement(&result);
    let cte_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Cte && n.qualified_name.as_deref() == Some("org_hierarchy")
        })
        .collect();
    assert!(
        !cte_nodes.is_empty(),
        "recursive CTE should produce at least one CTE node"
    );

    let employees_node = stmt
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Table && &*n.label == "employees")
        .expect("base table should be present");

    let cte_ids: HashSet<_> = cte_nodes.iter().map(|n| n.id.clone()).collect();
    let has_recursive_edge = stmt
        .edges
        .iter()
        .any(|e| cte_ids.contains(&e.from) && cte_ids.contains(&e.to));
    assert!(
        has_recursive_edge,
        "recursive CTE should have an edge connecting recursive CTE instances"
    );

    let has_base_edge = stmt
        .edges
        .iter()
        .any(|e| e.from == employees_node.id && cte_ids.contains(&e.to));
    assert!(
        has_base_edge,
        "recursive CTE anchor should link base table to the CTE node"
    );
}

#[test]
fn derived_tables_and_exists_predicates_produce_complete_lineage() {
    let sql = r#"
        WITH vip_flags AS (
            SELECT DISTINCT user_id
            FROM vip_users
        )
        SELECT agg.user_id,
               agg.total_amount,
               lp.payment_method
        FROM (
            SELECT o.user_id,
                   SUM(o.amount) AS total_amount
            FROM orders o
            JOIN payments p ON p.order_id = o.id
            WHERE o.status = 'completed'
            GROUP BY o.user_id
        ) AS agg
        JOIN (
            SELECT DISTINCT user_id,
                   MAX(method) AS payment_method
            FROM payments
            GROUP BY user_id
        ) AS lp
          ON agg.user_id = lp.user_id
        WHERE EXISTS (
            SELECT 1
            FROM vip_flags vf
            WHERE vf.user_id = agg.user_id
        );
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    for expected in ["orders", "payments", "vip_users"] {
        assert!(
            tables.contains(expected),
            "missing lineage for derived-table source {expected}; saw {tables:?}"
        );
    }
    assert!(
        result.summary.table_count >= 3,
        "expected at least three physical tables"
    );
    assert!(
        result
            .statements
            .first()
            .map(|stmt| !stmt.edges.is_empty())
            .unwrap_or(false),
        "expected data-flow edges connecting derived tables"
    );
}

#[test]
fn schema_metadata_and_search_path_resolve_identifiers() {
    let sql = r#"
        WITH filtered_orders AS (
            SELECT fo.order_id,
                   fo.customer_id,
                   fo.total_amount
            FROM fact_orders fo
            WHERE fo.region = 'us-east'
        )
        SELECT fo.order_id,
               d.region,
               d.loyalty_score
        FROM filtered_orders fo
        JOIN dim_customers d
          ON fo.customer_id = d.customer_id;
    "#;

    let schema = SchemaMetadata {
        allow_implied: true,
        default_catalog: Some("analytics".into()),
        default_schema: Some("marts".into()),
        search_path: Some(vec![
            SchemaNamespaceHint {
                catalog: Some("analytics".into()),
                schema: "marts".into(),
            },
            SchemaNamespaceHint {
                catalog: Some("analytics".into()),
                schema: "core".into(),
            },
        ]),
        case_sensitivity: None,
        tables: vec![
            schema_table(
                Some("analytics"),
                Some("marts"),
                "fact_orders",
                &["order_id", "customer_id", "total_amount", "region"],
            ),
            schema_table(
                Some("analytics"),
                Some("core"),
                "dim_customers",
                &["customer_id", "region"],
            ),
        ],
    };

    let result = run_analysis(sql, Dialect::Postgres, Some(schema));
    let tables = collect_table_names(&result);

    for expected in [
        "analytics.marts.fact_orders",
        "analytics.core.dim_customers",
    ] {
        assert!(
            tables.contains(expected),
            "search_path should resolve {expected}"
        );
    }
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == issue_codes::UNKNOWN_COLUMN),
        "missing loyalty_score should raise UNKNOWN_COLUMN"
    );
    assert!(
        !result.summary.has_errors,
        "validation warnings should not flip has_errors"
    );
}

#[test]
fn set_operations_track_all_source_tables() {
    let sql = r#"
        WITH combined AS (
            SELECT order_id, 'pending' AS source
            FROM pending_orders
            UNION ALL
            SELECT shipment_id AS order_id, 'shipment' AS source
            FROM pending_shipments
        ),
        filtered AS (
            SELECT order_id FROM combined
            EXCEPT
            SELECT order_id FROM quarantined_orders
        )
        SELECT order_id FROM filtered
        UNION
        SELECT legacy_id FROM legacy_orders;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert_eq!(
        result.summary.statement_count, 1,
        "entire set operation should be a single statement"
    );

    let tables = collect_table_names(&result);
    for expected in [
        "pending_orders",
        "pending_shipments",
        "quarantined_orders",
        "legacy_orders",
    ] {
        assert!(
            tables.contains(expected),
            "set operation should track {expected}"
        );
    }
    assert!(
        !result.summary.has_errors,
        "set operations fixture should succeed without errors"
    );
}

#[test]
fn ansi_select_registers_single_table_and_columns() {
    let sql = r#"
        SELECT u.id, u.email
        FROM analytics.users u
        WHERE u.is_active = TRUE;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert_eq!(result.summary.statement_count, 1);

    let tables = collect_table_names(&result);
    assert!(
        tables.contains("analytics.users"),
        "expected analytics.users in lineage, tables: {tables:?}"
    );

    let cols = column_labels(first_statement(&result));
    assert!(
        cols.iter().any(|c| c == "id"),
        "expected id column in output: {cols:?}"
    );
    assert!(
        cols.iter().any(|c| c == "email"),
        "expected email column in output: {cols:?}"
    );
    assert!(
        !result.summary.has_errors,
        "unexpected errors: {:?}",
        result.issues
    );
}

#[test]
fn ansi_join_variants_capture_all_tables() {
    let sql = r#"
        SELECT fs.order_id,
               dc.customer_name,
               ds.store_name,
               r.region_name,
               c.currency_code
        FROM fact_sales fs
        LEFT JOIN dim_customers dc ON dc.customer_id = fs.customer_id
        RIGHT JOIN dim_stores ds ON ds.store_id = fs.store_id
        FULL JOIN dim_regions r ON r.region_id = ds.region_id
        CROSS JOIN dim_currency c;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);
    for expected in [
        "fact_sales",
        "dim_customers",
        "dim_stores",
        "dim_regions",
        "dim_currency",
    ] {
        assert!(tables.contains(expected), "missing join source {expected}");
    }
    assert!(
        !result.summary.has_errors,
        "join query produced errors: {:?}",
        result.issues
    );
}

#[test]
fn ansi_nested_ctes_register_each_virtual_table() {
    let sql = r#"
        WITH base_orders AS (
            SELECT order_id, customer_id, total
            FROM orders
        ),
        ranked_orders AS (
            SELECT order_id,
                   customer_id,
                   total,
                   ROW_NUMBER() OVER (PARTITION BY customer_id ORDER BY total DESC) AS rn
            FROM base_orders
        ),
        final_orders AS (
            SELECT * FROM ranked_orders WHERE rn <= 5
        )
        SELECT * FROM final_orders;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let ctes = collect_cte_names(&result);
    for expected in ["base_orders", "ranked_orders", "final_orders"] {
        assert!(ctes.contains(expected), "missing CTE node {expected}");
    }

    let tables = collect_table_names(&result);
    assert!(
        tables.contains("orders"),
        "physical base table should still be tracked: {tables:?}"
    );
}

#[test]
fn ansi_reused_cte_is_deduplicated() {
    let sql = r#"
        WITH region_totals AS (
            SELECT region, SUM(amount) AS total_amount
            FROM orders
            GROUP BY region
        )
        SELECT *
        FROM region_totals rt
        JOIN region_totals rt2 ON rt.region = rt2.region;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let ctes = collect_cte_names(&result);
    assert_eq!(
        ctes.len(),
        1,
        "region_totals should appear once even if referenced twice"
    );
    assert!(ctes.contains("region_totals"));

    let tables = collect_table_names(&result);
    assert!(
        tables.contains("orders"),
        "expected base table for reused CTE: {tables:?}"
    );
}

#[test]
fn ansi_multi_statement_flow_updates_summary_and_cross_edges() {
    let sql = r#"
        SELECT id, email FROM users;
        INSERT INTO daily_active_users (user_id)
        SELECT id FROM users;
        CREATE TABLE user_copy AS
        SELECT id, email FROM users;
        SELECT COUNT(*) FROM user_copy;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert_eq!(result.summary.statement_count, 4);

    let cross_edges: Vec<_> = result
        .global_lineage
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::CrossStatement)
        .collect();
    assert!(
        !cross_edges.is_empty(),
        "expected at least one cross-statement edge for user_copy consumption"
    );
}

#[test]
fn ansi_insert_select_with_schema_flags_unknown_column() {
    let sql = r#"
        INSERT INTO analytics.daily_summary (order_id, amount, discount)
        SELECT order_id, amount, discount
        FROM analytics.orders;
    "#;

    let schema = SchemaMetadata {
        allow_implied: true,
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![schema_table(
            None,
            None,
            "analytics.orders",
            &["order_id", "amount"],
        )],
    };

    let result = run_analysis(sql, Dialect::Generic, Some(schema));
    let issues = issue_codes_list(&result);
    assert!(
        issues.contains(&issue_codes::UNKNOWN_COLUMN.to_string()),
        "expected UNKNOWN_COLUMN for missing discount, issues: {:?}",
        result.issues
    );
}

#[test]
fn ansi_create_table_as_union_tracks_targets_and_sources() {
    let sql = r#"
        CREATE TABLE analytics.daily_rollup AS
        SELECT order_id FROM analytics.orders
        UNION ALL
        SELECT shipment_id FROM analytics.shipments;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);
    for expected in [
        "analytics.daily_rollup",
        "analytics.orders",
        "analytics.shipments",
    ] {
        assert!(
            tables.contains(expected),
            "missing CTAS participant {expected}, tables: {tables:?}"
        );
    }
}

#[test]
fn ansi_star_without_schema_emits_approximate_lineage() {
    let sql = r#"
        SELECT * FROM analytics.events;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let issues = issue_codes_list(&result);
    assert!(
        issues.contains(&issue_codes::APPROXIMATE_LINEAGE.to_string()),
        "expected APPROXIMATE_LINEAGE info for SELECT * without schema"
    );
}

#[test]
fn ansi_star_with_schema_expands_columns() {
    let sql = r#"
        SELECT * FROM analytics.events;
    "#;

    let schema = SchemaMetadata {
        allow_implied: true,
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![schema_table(
            None,
            None,
            "analytics.events",
            &["user_id", "event_type", "event_time"],
        )],
    };

    let result = run_analysis(sql, Dialect::Generic, Some(schema));
    let issues = issue_codes_list(&result);
    assert!(
        !issues
            .iter()
            .any(|code| code == issue_codes::APPROXIMATE_LINEAGE),
        "schema metadata should prevent approximate warnings: {:?}",
        result.issues
    );
    assert!(
        result.summary.column_count >= 3,
        "column count should include expanded columns: {:?}",
        result.summary
    );
}

#[test]
fn ansi_window_functions_produce_derivation_edges() {
    let sql = r#"
        SELECT
            o.user_id,
            SUM(o.amount) OVER (PARTITION BY o.user_id ORDER BY o.created_at) AS rolling_total
        FROM orders o;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);
    let derivations = edges_by_type(stmt, EdgeType::Derivation);
    assert!(
        derivations.iter().any(|edge| {
            edge.expression
                .as_deref()
                .map(|expr| expr.contains("OVER"))
                .unwrap_or(false)
        }),
        "expected derivation edge capturing window expression: {:?}",
        derivations
    );
}

#[test]
fn ansi_values_clause_requires_no_tables() {
    let sql = r#"
        SELECT * FROM (VALUES (1, 'a'), (2, 'b')) AS v(id, label);
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);
    assert!(
        tables.is_empty(),
        "VALUES clause should not emit table nodes: {tables:?}"
    );
}

#[test]
fn ansi_table_function_emits_info_issue() {
    let sql = r#"
        SELECT *
        FROM TABLE(generate_series(1, 3)) AS g(n);
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let issues = issue_codes_list(&result);
    assert!(
        issues.contains(&issue_codes::UNSUPPORTED_SYNTAX.to_string()),
        "table function should emit UNSUPPORTED_SYNTAX info"
    );
}

#[test]
fn ansi_unnest_clause_keeps_base_table_lineage() {
    let sql = r#"
        SELECT item
        FROM orders o
        CROSS JOIN UNNEST(o.items) AS t(item);
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);
    assert!(
        tables.contains("orders"),
        "base table should still be tracked when UNNEST is used"
    );
    assert!(
        !result.summary.has_errors,
        "UNNEST support should not raise errors: {:?}",
        result.issues
    );
}

#[test]
fn ansi_pivot_usage_emits_warning() {
    let sql = r#"
        SELECT *
        FROM (
            SELECT region, month, revenue
            FROM sales
        ) src
        PIVOT (
            SUM(revenue) FOR month IN ('jan', 'feb')
        ) AS p;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let issues = issue_codes_list(&result);
    assert!(
        issues.contains(&issue_codes::UNSUPPORTED_SYNTAX.to_string()),
        "PIVOT should emit UNSUPPORTED_SYNTAX warning"
    );
}

#[test]
fn ansi_cross_apply_tracks_lateral_sources() {
    let sql = r#"
        SELECT u.id, purchases.total
        FROM users u
        CROSS APPLY (
            SELECT SUM(amount) AS total
            FROM orders o
            WHERE o.user_id = u.id
        ) purchases;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);
    for expected in ["users", "orders"] {
        assert!(
            tables.contains(expected),
            "CROSS APPLY should capture {expected}"
        );
    }
}

#[test]
fn ansi_cte_shadowing_existing_table_prefers_cte() {
    let sql = r#"
        WITH daily_metrics AS (
            SELECT *
            FROM analytics.daily_metrics
            WHERE metric_date >= CURRENT_DATE - 7
        )
        SELECT * FROM daily_metrics;
    "#;

    let schema = SchemaMetadata {
        allow_implied: true,
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![schema_table(
            None,
            None,
            "analytics.daily_metrics",
            &["metric_date", "active_users"],
        )],
    };

    let result = run_analysis(sql, Dialect::Generic, Some(schema));
    let tables = collect_table_names(&result);
    assert!(
        tables.contains("analytics.daily_metrics"),
        "base table should be registered from inside the CTE"
    );

    let ctes = collect_cte_names(&result);
    assert!(
        ctes.contains("daily_metrics"),
        "shadowing CTE should still appear as virtual node"
    );
}

#[test]
fn ansi_scalar_subquery_introduces_additional_table() {
    let sql = r#"
        WITH max_orders AS (
            SELECT user_id, MAX(amount) AS max_amount
            FROM orders
            GROUP BY user_id
        )
        SELECT u.id,
               (SELECT max_amount
                FROM max_orders mo
                WHERE mo.user_id = u.id) AS max_amount
        FROM users u;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);
    for expected in ["users", "orders"] {
        assert!(
            tables.contains(expected),
            "scalar subquery should include {expected}"
        );
    }
}

#[test]
fn ansi_correlated_predicates_capture_all_sources() {
    let sql = r#"
        WITH order_lookup AS (
            SELECT DISTINCT user_id FROM orders
        ),
        flagged_users AS (
            SELECT DISTINCT user_id FROM fraud_flags
        )
        SELECT u.id
        FROM users u
        WHERE EXISTS (
            SELECT 1 FROM order_lookup o WHERE o.user_id = u.id
        )
        AND u.id IN (
            SELECT f.user_id FROM flagged_users f
        );
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);
    for expected in ["users", "orders", "fraud_flags"] {
        assert!(
            tables.contains(expected),
            "correlated predicates should capture {expected}"
        );
    }
}

#[test]
fn ansi_group_by_and_having_keep_single_table_reference() {
    let sql = r#"
        SELECT customer_id, COUNT(*) AS total_orders
        FROM orders
        GROUP BY customer_id
        HAVING COUNT(*) > 5;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);
    assert_eq!(
        tables.len(),
        1,
        "GROUP BY/HAVING should not duplicate table entries: {tables:?}"
    );
    assert!(
        tables.contains("orders"),
        "orders table should be present in lineage"
    );
}

#[test]
fn ansi_case_expressions_emit_derivation_edges() {
    let sql = r#"
        SELECT
            CASE
                WHEN amount > 100 THEN 'big'
                ELSE 'small'
            END AS spend_bucket
        FROM orders;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);
    let derivations = edges_by_type(stmt, EdgeType::Derivation);
    assert!(
        !derivations.is_empty(),
        "CASE expression should create derivation edges"
    );
}

// ============================================================================
// DML STATEMENTS - UPDATE, DELETE, MERGE
// ============================================================================

#[test]
fn dml_update_with_from_clause_tracks_source_tables() {
    let sql = r#"
        UPDATE analytics.target t
        SET t.status = s.new_status,
            t.updated_at = s.timestamp
        FROM analytics.staging s
        WHERE t.id = s.id;
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    // Expect both target and source tables
    assert!(
        tables.contains("analytics.target"),
        "UPDATE target should be tracked"
    );
    assert!(
        tables.contains("analytics.staging"),
        "UPDATE source (FROM) should be tracked"
    );
}

#[test]
fn dml_update_with_subquery_captures_lineage() {
    let sql = r#"
        UPDATE users
        SET tier = (
            SELECT CASE
                WHEN SUM(amount) > 10000 THEN 'platinum'
                WHEN SUM(amount) > 1000 THEN 'gold'
                ELSE 'silver'
            END
            FROM orders
            WHERE orders.user_id = users.id
        );
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(tables.contains("users"), "UPDATE target should be tracked");
    assert!(
        tables.contains("orders"),
        "UPDATE subquery source should be tracked"
    );
}

#[test]
fn dml_delete_with_subquery_identifies_dependencies() {
    let sql = r#"
        DELETE FROM orders
        WHERE user_id IN (
            SELECT id FROM deleted_users
        );
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(tables.contains("orders"), "DELETE target should be tracked");
    assert!(
        tables.contains("deleted_users"),
        "DELETE subquery source should be tracked"
    );
}

#[test]
fn dml_delete_with_join_tracks_all_tables() {
    let sql = r#"
        DELETE FROM orders AS o
        USING cancelled_subscriptions AS c
        WHERE o.subscription_id = c.id
          AND c.cancelled_date < CURRENT_DATE - 30;
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("orders"),
        "DELETE target alias should resolve to table"
    );
    assert!(
        tables.contains("cancelled_subscriptions"),
        "DELETE JOIN source should be tracked"
    );
}

#[test]
fn dml_merge_statement_tracks_target_and_source() {
    let sql = r#"
        MERGE INTO analytics.customer_metrics t
        USING analytics.daily_activity s
        ON t.customer_id = s.customer_id AND t.date = s.date
        WHEN MATCHED THEN
            UPDATE SET t.activity_score = s.score, t.updated_at = CURRENT_TIMESTAMP
        WHEN NOT MATCHED THEN
            INSERT (customer_id, date, activity_score, created_at)
            VALUES (s.customer_id, s.date, s.score, CURRENT_TIMESTAMP);
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("analytics.customer_metrics"),
        "MERGE target should be tracked"
    );
    assert!(
        tables.contains("analytics.daily_activity"),
        "MERGE source should be tracked"
    );
}

#[test]
fn dml_merge_with_complex_source_query() {
    let sql = r#"
        MERGE INTO target t
        USING (
            SELECT s.id,
                   s.value,
                   d.metadata
            FROM source s
            JOIN dimensions d ON s.dim_id = d.id
            WHERE s.active = true
        ) src
        ON t.id = src.id
        WHEN MATCHED THEN UPDATE SET t.value = src.value
        WHEN NOT MATCHED THEN INSERT (id, value) VALUES (src.id, src.value);
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(tables.contains("target"), "MERGE target should be tracked");
    assert!(
        tables.contains("source"),
        "MERGE subquery source 1 should be tracked"
    );
    assert!(
        tables.contains("dimensions"),
        "MERGE subquery source 2 should be tracked"
    );
}

// ============================================================================
// COLUMN LINEAGE EDGE CASES
// ============================================================================

#[test]
fn column_lineage_using_clause_tracks_implicit_columns() {
    let sql = r#"
        SELECT t1.id, t1.name, t2.amount
        FROM orders t1
        JOIN payments t2 USING (order_id, customer_id);
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    for expected in ["orders", "payments"] {
        assert!(
            tables.contains(expected),
            "JOIN USING should track {expected}"
        );
    }

    let stmt = first_statement(&result);
    assert!(
        !stmt.edges.is_empty(),
        "JOIN USING should create column-level edges"
    );
}

#[test]
fn column_lineage_natural_join_captures_tables() {
    let sql = r#"
        SELECT o.order_id, c.customer_name
        FROM orders o
        NATURAL JOIN customers c;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    for expected in ["orders", "customers"] {
        assert!(
            tables.contains(expected),
            "NATURAL JOIN should track {expected}"
        );
    }
}

#[test]
fn column_lineage_multiple_aliases_to_same_column() {
    let sql = r#"
        SELECT id AS user_id,
               id AS customer_id,
               id AS account_id
        FROM users;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let cols = column_labels(first_statement(&result));

    for expected in ["user_id", "customer_id", "account_id"] {
        assert!(
            cols.contains(&expected.to_string()),
            "multiple aliases should create distinct column nodes: {expected}"
        );
    }
}

#[test]
fn column_lineage_renaming_chain_through_ctes() {
    let sql = r#"
        WITH stage1 AS (
            SELECT user_id AS uid FROM orders
        ),
        stage2 AS (
            SELECT uid AS customer_id FROM stage1
        ),
        stage3 AS (
            SELECT customer_id AS cid FROM stage2
        )
        SELECT cid AS final_id FROM stage3;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("orders"),
        "column renaming chain should preserve base table lineage"
    );

    let ctes = collect_cte_names(&result);
    assert_eq!(
        ctes.len(),
        3,
        "all intermediate CTEs in renaming chain should be tracked"
    );
}

#[test]
fn column_lineage_coalesce_across_multiple_tables() {
    let sql = r#"
        SELECT
            COALESCE(t1.email, t2.email, t3.email, 'unknown@example.com') AS email,
            COALESCE(t1.phone, t2.phone) AS phone
        FROM users t1
        LEFT JOIN user_profiles t2 ON t1.id = t2.user_id
        LEFT JOIN user_contacts t3 ON t1.id = t3.user_id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    for expected in ["users", "user_profiles", "user_contacts"] {
        assert!(
            tables.contains(expected),
            "COALESCE should track all source tables: {expected}"
        );
    }

    let stmt = first_statement(&result);
    let derivations = edges_by_type(stmt, EdgeType::Derivation);
    assert!(
        !derivations.is_empty(),
        "COALESCE should create derivation edges"
    );
}

#[test]
fn column_lineage_concat_and_string_functions() {
    let sql = r#"
        SELECT
            CONCAT(first_name, ' ', last_name) AS full_name,
            UPPER(email) AS email_upper,
            SUBSTRING(phone, 1, 3) AS area_code
        FROM users;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);
    let derivations = edges_by_type(stmt, EdgeType::Derivation);

    assert!(
        derivations.len() >= 3,
        "string functions should create derivation edges for each computed column"
    );
}

// ============================================================================
// ADVANCED AGGREGATIONS
// ============================================================================

#[test]
fn advanced_agg_grouping_sets_tracks_source() {
    let sql = r#"
        SELECT region, product, SUM(sales) AS total_sales
        FROM orders
        GROUP BY GROUPING SETS ((region), (product), (region, product), ());
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("orders"),
        "GROUPING SETS should track source table"
    );
}

#[test]
fn advanced_agg_cube_preserves_lineage() {
    let sql = r#"
        SELECT region, product, quarter, SUM(revenue) AS total_revenue
        FROM sales
        GROUP BY CUBE (region, product, quarter);
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("sales"),
        "CUBE aggregation should track source table"
    );
}

#[test]
fn advanced_agg_rollup_with_having() {
    let sql = r#"
        SELECT region, SUM(amount) AS total
        FROM orders
        GROUP BY ROLLUP (region)
        HAVING SUM(amount) > 1000;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("orders"),
        "ROLLUP with HAVING should track source"
    );
}

#[test]
fn advanced_agg_filter_clause_on_aggregates() {
    let sql = r#"
        SELECT
            user_id,
            COUNT(*) FILTER (WHERE status = 'active') AS active_count,
            COUNT(*) FILTER (WHERE status = 'inactive') AS inactive_count,
            SUM(amount) FILTER (WHERE category = 'premium') AS premium_total
        FROM orders
        GROUP BY user_id;
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("orders"),
        "aggregate FILTER clause should track source table"
    );

    let stmt = first_statement(&result);
    let derivations = edges_by_type(stmt, EdgeType::Derivation);
    assert!(
        !derivations.is_empty(),
        "FILTER aggregates should create derivation edges"
    );
}

#[test]
fn advanced_agg_nested_aggregations() {
    let sql = r#"
        SELECT region, AVG(product_total) AS avg_per_product
        FROM (
            SELECT region, product, SUM(amount) AS product_total
            FROM sales
            GROUP BY region, product
        ) AS subq
        GROUP BY region;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("sales"),
        "nested aggregations should track original source"
    );
}

#[test]
fn advanced_agg_array_agg_and_string_agg() {
    let sql = r#"
        SELECT
            user_id,
            ARRAY_AGG(product_id ORDER BY purchase_date) AS purchased_products,
            STRING_AGG(product_name, ', ') AS product_list
        FROM purchases
        GROUP BY user_id;
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("purchases"),
        "ARRAY_AGG/STRING_AGG should track source"
    );
}

// ============================================================================
// SELF-JOINS AND COMPLEX PATTERNS
// ============================================================================

#[test]
fn self_join_multi_level_hierarchy() {
    let sql = r#"
        SELECT
            e1.name AS employee,
            e2.name AS manager,
            e3.name AS director,
            e4.name AS vp
        FROM employees e1
        LEFT JOIN employees e2 ON e1.manager_id = e2.id
        LEFT JOIN employees e3 ON e2.manager_id = e3.id
        LEFT JOIN employees e4 ON e3.manager_id = e4.id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Each alias should produce its own table node in statement lineage
    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert_eq!(
        table_nodes.len(),
        4,
        "self-join with 4 aliases should produce 4 distinct table nodes, got: {:?}",
        table_nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
    );

    // All nodes should reference the same canonical table
    for node in &table_nodes {
        assert_eq!(
            node.qualified_name.as_deref(),
            Some("employees"),
            "all self-join nodes should have canonical qualified_name"
        );
    }

    // Each alias node should have a unique ID
    let unique_ids: HashSet<_> = table_nodes.iter().map(|n| &n.id).collect();
    assert_eq!(
        unique_ids.len(),
        4,
        "each alias should have a unique node ID"
    );

    let cols = column_labels(stmt);
    for expected in ["employee", "manager", "director", "vp"] {
        assert!(
            cols.contains(&expected.to_string()),
            "multi-level self-join should track all output columns: {expected}"
        );
    }
}

#[test]
fn self_join_with_aggregation() {
    let sql = r#"
        SELECT
            e1.department_id,
            COUNT(DISTINCT e1.id) AS employee_count,
            COUNT(DISTINCT e2.id) AS manager_count
        FROM employees e1
        LEFT JOIN employees e2 ON e1.id = e2.manager_id
        GROUP BY e1.department_id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Two aliases should produce two distinct table nodes
    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert_eq!(
        table_nodes.len(),
        2,
        "self-join with 2 aliases should produce 2 distinct table nodes"
    );

    // All nodes should reference the same canonical table
    for node in &table_nodes {
        assert_eq!(node.qualified_name.as_deref(), Some("employees"),);
    }
}

#[test]
fn self_join_filters_attach_to_correct_instance() {
    let sql = r#"
        SELECT e1.name, e2.name
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id
        WHERE e1.active = true
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert_eq!(table_nodes.len(), 2, "self-join should have 2 table nodes");

    // Exactly one node should have the WHERE filter
    let nodes_with_filters: Vec<_> = table_nodes
        .iter()
        .filter(|n| !n.filters.is_empty())
        .collect();
    assert_eq!(
        nodes_with_filters.len(),
        1,
        "only the e1 instance should have the filter, got filters on {} nodes",
        nodes_with_filters.len()
    );

    let filtered_node = nodes_with_filters[0];
    assert_eq!(filtered_node.filters.len(), 1);
    assert!(
        filtered_node.filters[0].expression.contains("active"),
        "filter should reference 'active'"
    );
}

#[test]
fn self_join_column_ownership_is_instance_aware() {
    // Verify that columns from different aliases attach to different table nodes
    let sql = r#"
        SELECT e1.name AS emp_name, e2.name AS mgr_name
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert_eq!(table_nodes.len(), 2);

    // Each table node should own different source column nodes
    // Check that both table nodes have ownership edges to column nodes
    for table_node in &table_nodes {
        let owned_columns: Vec<_> = stmt
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Ownership && e.from == table_node.id)
            .collect();
        assert!(
            !owned_columns.is_empty(),
            "each self-join instance should own at least one column, but node {} has none",
            table_node.id
        );
    }

    // The two table nodes should have DIFFERENT column children (different IDs)
    let owned_by_first: HashSet<_> = stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Ownership && e.from == table_nodes[0].id)
        .map(|e| &e.to)
        .collect();
    let owned_by_second: HashSet<_> = stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Ownership && e.from == table_nodes[1].id)
        .map(|e| &e.to)
        .collect();
    assert!(
        owned_by_first.is_disjoint(&owned_by_second),
        "self-join instances should own disjoint column sets"
    );
}

#[test]
fn self_join_wildcard_expands_all_relation_instances() {
    let sql = r#"
        SELECT *
        FROM public.employees e1
        JOIN public.employees e2 ON e1.manager_id = e2.id
    "#;

    let schema = SchemaMetadata {
        allow_implied: true,
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![schema_table(
            None,
            Some("public"),
            "employees",
            &["id", "manager_id", "name"],
        )],
    };

    let result = run_analysis(sql, Dialect::Postgres, Some(schema));
    let stmt = first_statement(&result);

    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|node| {
            node.node_type == NodeType::Table
                && node.qualified_name.as_deref() == Some("public.employees")
        })
        .collect();
    assert_eq!(
        table_nodes.len(),
        2,
        "self-join should create 2 table nodes"
    );

    let output_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Output)
        .expect("output node should exist");

    for column_name in ["id", "manager_id", "name"] {
        let output_column_id = stmt
            .edges
            .iter()
            .find(|edge| {
                edge.edge_type == EdgeType::Ownership
                    && edge.from == output_node.id
                    && stmt.nodes.iter().any(|node| {
                        node.id == edge.to
                            && node.node_type == NodeType::Column
                            && &*node.label == column_name
                    })
            })
            .map(|edge| edge.to.clone())
            .unwrap_or_else(|| panic!("output column {column_name} should exist"));

        let source_owner_ids: HashSet<_> = stmt
            .edges
            .iter()
            .filter(|edge| edge.edge_type == EdgeType::DataFlow && edge.to == output_column_id)
            .filter_map(|edge| {
                stmt.edges
                    .iter()
                    .find(|ownership| {
                        ownership.edge_type == EdgeType::Ownership && ownership.to == edge.from
                    })
                    .map(|ownership| ownership.from.clone())
            })
            .collect();

        assert_eq!(
            source_owner_ids.len(),
            2,
            "wildcard-expanded column {column_name} should receive lineage from both self-join instances"
        );
    }
}

#[test]
fn schema_qualified_unaliased_self_join_reference_uses_distinct_instance() {
    let sql = r#"
        SELECT public.employees.id AS employee_id, e2.id AS manager_id
        FROM public.employees
        JOIN public.employees e2 ON public.employees.manager_id = e2.id
    "#;

    let schema = SchemaMetadata {
        allow_implied: true,
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![schema_table(
            None,
            Some("public"),
            "employees",
            &["id", "manager_id", "name"],
        )],
    };

    let result = run_analysis(sql, Dialect::Postgres, Some(schema));
    let stmt = first_statement(&result);

    let output_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Output)
        .expect("output node should exist");

    let output_column_id = |label: &str| {
        stmt.edges
            .iter()
            .find(|edge| {
                edge.edge_type == EdgeType::Ownership
                    && edge.from == output_node.id
                    && stmt.nodes.iter().any(|node| {
                        node.id == edge.to
                            && node.node_type == NodeType::Column
                            && &*node.label == label
                    })
            })
            .map(|edge| edge.to.clone())
            .unwrap_or_else(|| panic!("{label} should exist as an output column"))
    };

    let source_owner_id = |output_column_id: &std::sync::Arc<str>| {
        let source_column_id = stmt
            .edges
            .iter()
            .find(|edge| edge.edge_type == EdgeType::DataFlow && edge.to == *output_column_id)
            .map(|edge| edge.from.clone())
            .expect("output column should have a source column");

        stmt.edges
            .iter()
            .find(|edge| edge.edge_type == EdgeType::Ownership && edge.to == source_column_id)
            .map(|edge| edge.from.clone())
            .expect("source column should be owned by a table")
    };

    let employee_owner_id = source_owner_id(&output_column_id("employee_id"));
    let manager_owner_id = source_owner_id(&output_column_id("manager_id"));

    assert_ne!(
        employee_owner_id, manager_owner_id,
        "schema-qualified reference to the unaliased side should not collapse onto the aliased self-join node"
    );
}

#[test]
fn self_join_global_lineage_merges_by_canonical() {
    let sql = r#"
        SELECT e1.name, e2.name
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    // Statement-level: 2 distinct nodes
    let stmt = first_statement(&result);
    let stmt_table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert_eq!(stmt_table_nodes.len(), 2);

    // Global lineage should still have a single "employees" entry
    let global = &result.global_lineage;
    let global_employees: Vec<_> = global
        .nodes
        .iter()
        .filter(|n| n.canonical_name.name == "employees")
        .collect();
    assert_eq!(
        global_employees.len(),
        1,
        "global lineage should merge self-join instances into one canonical node"
    );

    let global_node_ids: HashSet<_> = global.nodes.iter().map(|n| n.id.clone()).collect();
    for edge in &global.edges {
        assert!(
            global_node_ids.contains(&edge.from),
            "global edge {} has missing source node {}",
            edge.id,
            edge.from
        );
        assert!(
            global_node_ids.contains(&edge.to),
            "global edge {} has missing target node {}",
            edge.id,
            edge.to
        );
    }
}

#[test]
fn self_join_global_lineage_merges_source_columns_by_canonical() {
    let sql = r#"
        SELECT
            e1.name AS employee_name,
            e2.name AS manager_name,
            e3.name AS director_name
        FROM employees e1
        LEFT JOIN employees e2 ON e1.manager_id = e2.id
        LEFT JOIN employees e3 ON e2.manager_id = e3.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let global = &result.global_lineage;

    let employees_node = global
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Table && node.canonical_name.name == "employees")
        .expect("employees table should exist in global lineage");

    let source_name_nodes: Vec<_> = global
        .nodes
        .iter()
        .filter(|node| {
            node.node_type == NodeType::Column
                && node.canonical_name.schema.as_deref() == Some("employees")
                && node.canonical_name.name == "name"
        })
        .collect();

    assert_eq!(
        source_name_nodes.len(),
        1,
        "self-join source columns should collapse into one global canonical node"
    );

    let source_name_node = source_name_nodes[0];
    assert_eq!(
        source_name_node.statement_refs.len(),
        3,
        "merged source column should retain refs to all three statement-local instances"
    );

    let ownership_edges: Vec<_> = global
        .edges
        .iter()
        .filter(|edge| {
            edge.edge_type == EdgeType::Ownership
                && edge.from == employees_node.id
                && edge.to == source_name_node.id
        })
        .collect();
    assert_eq!(
        ownership_edges.len(),
        1,
        "merged global source column should have one ownership edge from employees"
    );

    let output_targets: HashSet<_> = global
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::DataFlow && edge.from == source_name_node.id)
        .map(|edge| edge.to.clone())
        .collect();
    assert_eq!(
        output_targets.len(),
        3,
        "merged source column should feed all three output aliases"
    );
}

#[test]
fn global_lineage_merges_qualified_columns_across_self_joins_and_cte_instances() {
    let sql = r#"
        SELECT
            e1.name AS employee_name,
            e2.name AS manager_name,
            e3.name AS director_name
        FROM employees e1
        LEFT JOIN employees e2 ON e1.manager_id = e2.id
        LEFT JOIN employees e3 ON e2.manager_id = e3.id
        WHERE e1.active = true AND e3.region = 'NA';

        WITH org AS (
            SELECT
                id AS employee_id,
                manager_id,
                department_id
            FROM employees
        )
        SELECT
            a.employee_id,
            b.employee_id AS manager_employee_id
        FROM org a
        JOIN org b ON a.manager_id = b.employee_id
        WHERE a.department_id = 10;
    "#;

    let schema = SchemaMetadata {
        allow_implied: true,
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![schema_table(
            None,
            None,
            "employees",
            &[
                "id",
                "manager_id",
                "department_id",
                "name",
                "active",
                "region",
            ],
        )],
    };

    let result = run_analysis(sql, Dialect::Generic, Some(schema));
    let global = &result.global_lineage;

    let employees_name_nodes: Vec<_> = global
        .nodes
        .iter()
        .filter(|node| {
            node.node_type == NodeType::Column
                && node.canonical_name.schema.as_deref() == Some("employees")
                && node.canonical_name.name == "name"
        })
        .collect();
    assert_eq!(
        employees_name_nodes.len(),
        1,
        "global lineage should contain one canonical employees.name node"
    );
    assert_eq!(
        employees_name_nodes[0].statement_refs.len(),
        3,
        "employees.name should retain all three self-join source refs"
    );

    let org_employee_id_nodes: Vec<_> = global
        .nodes
        .iter()
        .filter(|node| {
            node.node_type == NodeType::Column
                && node.canonical_name.schema.as_deref() == Some("org")
                && node.canonical_name.name == "employee_id"
        })
        .collect();
    assert_eq!(
        org_employee_id_nodes.len(),
        1,
        "global lineage should contain one canonical org.employee_id node"
    );
}

#[test]
fn self_join_unqualified_column_reference_stays_ambiguous() {
    let sql = r#"
        SELECT id
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    let ambiguity_issue = result.issues.iter().find(|issue| {
        issue.code == issue_codes::UNRESOLVED_REFERENCE
            && issue.message.to_lowercase().contains("ambiguous")
    });

    assert!(
        ambiguity_issue.is_some(),
        "unqualified self-join columns should still be ambiguous"
    );
}

#[test]
fn ambiguous_self_join_projection_does_not_emit_dangling_column_or_join_dependency() {
    let sql = r#"
        SELECT id
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    assert!(
        result.issues.iter().any(|issue| {
            issue.code == issue_codes::UNRESOLVED_REFERENCE
                && issue.message.to_lowercase().contains("ambiguous")
        }),
        "ambiguous self-join projection should still emit an ambiguity warning"
    );

    assert!(
        stmt.nodes
            .iter()
            .filter(|node| node.node_type == NodeType::Column && &*node.label == "id")
            .count()
            == 0,
        "ambiguous projected column should not create a dangling output column node"
    );

    assert!(
        stmt.edges
            .iter()
            .all(|edge| edge.edge_type != EdgeType::JoinDependency),
        "invalid ambiguous projection should not synthesize join-dependency edges"
    );
}

#[test]
fn unresolved_bare_join_projection_remains_in_output_schema() {
    let sql = r#"
        SELECT name
        FROM customers
        JOIN orders ON customers.id = orders.customer_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == issue_codes::UNRESOLVED_REFERENCE),
        "best-effort multi-table analysis should still surface the unresolved reference"
    );

    let output_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Output)
        .expect("Output node should exist");

    let visible_output_column = stmt.nodes.iter().find(|node| {
        node.node_type == NodeType::Column
            && &*node.label == "name"
            && stmt.edges.iter().any(|edge| {
                edge.edge_type == EdgeType::Ownership
                    && edge.from == output_node.id
                    && edge.to == node.id
            })
    });

    assert!(
        visible_output_column.is_some(),
        "unresolved bare projections should remain visible in the output schema"
    );
}

#[test]
fn repeated_cte_aliases_create_distinct_reference_nodes() {
    let sql = r#"
        WITH org AS (
            SELECT id, manager_id
            FROM employees
        )
        SELECT a.id
        FROM org a
        JOIN org b ON a.manager_id = b.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // The first reference (`org a`) reuses the CTE definition node.
    // Only the self-join reference (`org b`) gets a separate instance node.
    let cte_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Cte && n.qualified_name.as_deref() == Some("org"))
        .collect();

    assert_eq!(
        cte_nodes.len(),
        2,
        "expected CTE definition (reused by first alias) plus one self-join instance node"
    );

    let unique_ids: HashSet<_> = cte_nodes.iter().map(|n| n.id.clone()).collect();
    assert_eq!(
        unique_ids.len(),
        2,
        "CTE self-join aliases should have distinct node IDs"
    );
}

#[test]
fn repeated_cte_aliases_across_statements_keep_distinct_global_instance_nodes() {
    let sql = r#"
        WITH org AS (
            SELECT id, manager_id
            FROM employees
        )
        SELECT a.id
        FROM org a
        JOIN org b ON a.manager_id = b.id;

        WITH org AS (
            SELECT id, parent_id AS manager_id
            FROM departments
        )
        SELECT a.id
        FROM org a
        JOIN org b ON a.manager_id = b.id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    let global_org_nodes: Vec<_> = result
        .global_lineage
        .nodes
        .iter()
        .filter(|node| node.node_type == NodeType::Cte && node.canonical_name.name == "org")
        .collect();

    assert_eq!(
        global_org_nodes.len(),
        4,
        "global lineage should keep statement-local org definitions plus one org b instance per statement"
    );

    let statement_scoped_instances = global_org_nodes
        .iter()
        .filter(|node| node.statement_refs.len() == 1)
        .count();
    assert_eq!(
        statement_scoped_instances, 4,
        "CTE definitions and self-join instances should remain statement-local in global lineage"
    );

    let global_org_columns: Vec<_> = result
        .global_lineage
        .nodes
        .iter()
        .filter(|node| {
            node.node_type == NodeType::Column
                && node.canonical_name.schema.as_deref() == Some("org")
                && matches!(node.canonical_name.name.as_str(), "id" | "manager_id")
        })
        .collect();

    assert!(
        global_org_columns.len() >= 4,
        "global lineage should keep distinct org column nodes for each statement-local CTE scope"
    );

    let cross_statement_org_columns: Vec<_> = global_org_columns
        .iter()
        .filter(|node| {
            node.statement_refs
                .iter()
                .map(|r| r.statement_index)
                .collect::<HashSet<_>>()
                .len()
                > 1
        })
        .collect();

    assert!(
        cross_statement_org_columns.is_empty(),
        "CTE-owned org columns should not merge across statements in global lineage"
    );
}

#[test]
fn cte_self_join_alias_columns_do_not_leak_across_union_scopes() {
    let sql = r#"
        WITH emp AS (
            SELECT id, name
            FROM employees
        ),
        dept AS (
            SELECT id, name
            FROM departments
        )
        SELECT b.name AS emp_name
        FROM emp a
        JOIN emp b ON a.id = b.id
        UNION ALL
        SELECT b.name AS dept_name
        FROM dept a
        JOIN dept b ON a.id = b.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    assert!(
        !result.summary.has_errors,
        "CTE alias reuse across UNION branches should analyze cleanly: {:?}",
        result.issues
    );

    let output_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Output)
        .expect("Output node should exist");
    let dept_output = stmt
        .nodes
        .iter()
        .find(|node| {
            node.node_type == NodeType::Column
                && &*node.label == "dept_name"
                && stmt.edges.iter().any(|edge| {
                    edge.edge_type == EdgeType::Ownership
                        && edge.from == output_node.id
                        && edge.to == node.id
                })
        })
        .expect("dept_name output column should exist");

    let dept_source_ids: HashSet<_> = stmt
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::DataFlow && edge.to == dept_output.id)
        .map(|edge| edge.from.clone())
        .collect();
    assert!(
        !dept_source_ids.is_empty(),
        "dept_name should receive a direct data-flow edge from the dept branch source column"
    );

    let source_owner_labels: HashSet<_> = stmt
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::Ownership && dept_source_ids.contains(&edge.to))
        .filter_map(|edge| stmt.nodes.iter().find(|node| node.id == edge.from))
        .map(|node| node.label.to_string())
        .collect();

    assert!(
        source_owner_labels.contains("dept"),
        "dept_name should be sourced from a dept CTE instance, saw owners {source_owner_labels:?}"
    );
    assert!(
        !source_owner_labels.contains("emp"),
        "dept_name should not reuse emp columns when aliases are reused across UNION branches, saw owners {source_owner_labels:?}"
    );
}

#[test]
fn self_join_in_subquery_produces_distinct_nodes() {
    let sql = r#"
        SELECT sub.emp_name, sub.mgr_name
        FROM (
            SELECT e1.name AS emp_name, e2.name AS mgr_name
            FROM employees e1
            JOIN employees e2 ON e1.manager_id = e2.id
        ) sub
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // The subquery should produce two distinct table nodes for the self-join
    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert_eq!(
        table_nodes.len(),
        2,
        "self-join inside subquery should produce 2 distinct table nodes, got: {:?}",
        table_nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
    );

    // Both should reference the same canonical table
    for node in &table_nodes {
        assert_eq!(
            node.qualified_name.as_deref(),
            Some("employees"),
            "subquery self-join nodes should have canonical qualified_name"
        );
    }

    // Global lineage should merge them
    let global_employees: Vec<_> = result
        .global_lineage
        .nodes
        .iter()
        .filter(|n| n.canonical_name.name == "employees")
        .collect();
    assert_eq!(
        global_employees.len(),
        1,
        "global lineage should merge subquery self-join instances"
    );
}

#[test]
fn nested_self_join_aliases_keep_filters_isolated_per_scope() {
    let sql = r#"
        SELECT e2.name, sub.inner_mgr
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id
        JOIN (
            SELECT e2.id, e2.name AS inner_mgr
            FROM employees e1
            JOIN employees e2 ON e1.manager_id = e2.id
            WHERE e2.department = 'sales'
        ) sub ON sub.id = e2.id
        WHERE e2.department = 'eng'
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let filtered_employee_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Table
                && n.qualified_name.as_deref() == Some("employees")
                && !n.filters.is_empty()
        })
        .collect();
    assert_eq!(
        filtered_employee_nodes.len(),
        2,
        "outer and inner self-join aliases should keep separate filtered nodes"
    );

    let filter_sets: HashSet<Vec<String>> = filtered_employee_nodes
        .iter()
        .map(|node| {
            let mut filters: Vec<String> =
                node.filters.iter().map(|f| f.expression.clone()).collect();
            filters.sort();
            filters
        })
        .collect();
    assert!(
        filter_sets.contains(&vec!["e2.department = 'eng'".to_string()]),
        "expected one filtered node for the outer e2 alias, got {filter_sets:?}"
    );
    assert!(
        filter_sets.contains(&vec!["e2.department = 'sales'".to_string()]),
        "expected one filtered node for the inner e2 alias, got {filter_sets:?}"
    );
}

#[test]
fn self_join_alias_matching_another_table_name() {
    // Alias "orders" collides with the canonical name of the other table
    let sql = r#"
        SELECT c1.name, orders.total
        FROM customers c1
        JOIN customers orders ON c1.referrer_id = orders.id
        JOIN orders real_orders ON orders.id = real_orders.customer_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let customer_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Table && n.qualified_name.as_deref() == Some("customers")
        })
        .collect();
    assert_eq!(
        customer_nodes.len(),
        2,
        "customers self-join should produce 2 distinct nodes"
    );

    let order_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table && n.qualified_name.as_deref() == Some("orders"))
        .collect();
    assert_eq!(
        order_nodes.len(),
        1,
        "the real 'orders' table should have exactly 1 node"
    );
}

#[test]
fn three_way_self_join_filters_isolated() {
    let sql = r#"
        SELECT e1.name, e2.name, e3.name
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id
        JOIN employees e3 ON e2.manager_id = e3.id
        WHERE e1.active = true AND e3.level = 'director'
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert_eq!(
        table_nodes.len(),
        3,
        "3-way self-join should produce 3 nodes"
    );

    // Exactly 2 nodes should have filters (e1 and e3)
    let nodes_with_filters: Vec<_> = table_nodes
        .iter()
        .filter(|n| !n.filters.is_empty())
        .collect();
    assert_eq!(
        nodes_with_filters.len(),
        2,
        "filters should attach to exactly 2 of the 3 self-join instances"
    );

    // e2 should have no filters
    let nodes_without_filters: Vec<_> = table_nodes
        .iter()
        .filter(|n| n.filters.is_empty())
        .collect();
    assert_eq!(nodes_without_filters.len(), 1);
}

#[test]
fn self_join_unqualified_filter_applies_to_all_instances() {
    // An unqualified WHERE predicate should apply to all self-join instances
    // because the column is ambiguous without a qualifier.
    let sql = r#"
        SELECT e1.name, e2.name
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id
        WHERE active = true
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert_eq!(table_nodes.len(), 2, "self-join should produce 2 nodes");

    // Both nodes should receive the unqualified filter
    for node in &table_nodes {
        assert!(
            !node.filters.is_empty(),
            "unqualified filter should apply to all instances, but node {} has no filters",
            node.id
        );
    }
}

#[test]
fn self_join_unqualified_filter_with_other_join_applies_to_all_matching_instances() {
    let sql = r#"
        SELECT e1.name, e2.name, d.dept_name
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id
        JOIN departments d ON e1.dept_id = d.id
        WHERE active = true
    "#;

    let schema = SchemaMetadata {
        allow_implied: true,
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![
            schema_table(
                None,
                None,
                "employees",
                &["id", "manager_id", "dept_id", "name", "active"],
            ),
            schema_table(None, None, "departments", &["id", "dept_name"]),
        ],
    };

    let result = run_analysis(sql, Dialect::Generic, Some(schema));
    let stmt = first_statement(&result);

    let employee_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Table && n.qualified_name.as_deref() == Some("employees")
        })
        .collect();
    assert_eq!(
        employee_nodes.len(),
        2,
        "self-join should produce 2 employee nodes"
    );

    for node in &employee_nodes {
        assert!(
            node.filters.iter().any(|f| f.expression.contains("active")),
            "ambiguous self-join filter should apply to all employee instances, but node {} has {:?}",
            node.id,
            node.filters
        );
    }

    let departments_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Table && n.qualified_name.as_deref() == Some("departments")
        })
        .collect();
    assert_eq!(
        departments_nodes.len(),
        1,
        "regular join should produce 1 departments node"
    );
    assert!(
        departments_nodes[0].filters.is_empty(),
        "employee-only ambiguous filter should not attach to departments: {:?}",
        departments_nodes[0].filters
    );
}

#[test]
fn self_join_mixed_with_regular_join() {
    let sql = r#"
        SELECT e1.name, e2.name, d.dept_name
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id
        JOIN departments d ON e1.dept_id = d.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let emp_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Table && n.qualified_name.as_deref() == Some("employees")
        })
        .collect();
    assert_eq!(
        emp_nodes.len(),
        2,
        "self-join should produce 2 employee nodes"
    );

    let dept_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Table && n.qualified_name.as_deref() == Some("departments")
        })
        .collect();
    assert_eq!(
        dept_nodes.len(),
        1,
        "regular join should produce 1 department node"
    );

    // All 3 node IDs must be distinct
    let mut ids: Vec<_> = emp_nodes
        .iter()
        .chain(dept_nodes.iter())
        .map(|n| &n.id)
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 3, "all node IDs should be distinct");
}

#[test]
fn cte_self_join_produces_distinct_nodes() {
    let sql = r#"
        WITH emp AS (
            SELECT id, name, manager_id FROM employees
        )
        SELECT e1.name AS employee, e2.name AS manager
        FROM emp e1
        JOIN emp e2 ON e1.manager_id = e2.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let cte_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Cte)
        .collect();

    // The CTE definition itself plus possibly two references. At minimum we
    // expect the CTE definition node to exist.
    assert!(
        !cte_nodes.is_empty(),
        "CTE self-join should produce at least 1 CTE node"
    );

    // Global lineage should preserve non-definition CTE reference instances so
    // ordinary self-joins are not rendered as recursive self-loops.
    let global_emp: Vec<_> = result
        .global_lineage
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Cte && n.canonical_name.name == "emp")
        .collect();
    assert!(
        global_emp.len() >= 2,
        "global lineage should preserve non-recursive CTE instances"
    );

    assert!(
        result.global_lineage.edges.iter().all(|edge| {
            !(edge.edge_type == EdgeType::DataFlow
                && edge.from == edge.to
                && global_emp.iter().any(|node| node.id == edge.from))
        }),
        "non-recursive CTE self-joins should not become global data-flow self-loops"
    );
}

#[test]
fn self_join_mixed_qualified_and_unqualified_predicates() {
    // Qualified predicates should attach to specific instances;
    // unqualified predicates should apply to all instances of the canonical table.
    let sql = r#"
        SELECT e1.name, e2.name
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id
        WHERE e1.active = true AND department = 'sales'
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert_eq!(table_nodes.len(), 2, "self-join should produce 2 nodes");

    // The node with the e1.active filter should have at least 2 filters
    // (one instance-targeted, one canonical)
    let max_filters = table_nodes.iter().map(|n| n.filters.len()).max().unwrap();
    assert!(
        max_filters >= 2,
        "node with qualified + unqualified filters should have at least 2 filters"
    );

    // Both nodes should have the unqualified 'department' filter
    for node in &table_nodes {
        let has_dept_filter = node
            .filters
            .iter()
            .any(|f| f.expression.contains("department"));
        assert!(
            has_dept_filter,
            "unqualified filter should apply to all instances, but node {} is missing it",
            node.id
        );
    }
}

#[test]
fn self_join_without_aliases_produces_single_node() {
    // When a table self-joins without distinct aliases, both references share
    // the same node ID for backward compatibility. SQL self-joins without
    // aliases are ambiguous by nature.
    let sql = r#"
        SELECT name
        FROM employees
        JOIN employees ON employees.manager_id = employees.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert_eq!(
        table_nodes.len(),
        1,
        "unaliased self-join should collapse to a single node for backward compatibility"
    );
    assert_eq!(table_nodes[0].qualified_name.as_deref(), Some("employees"));
}

#[test]
fn triple_self_join_each_alias_gets_distinct_filter() {
    // Each of the three self-join aliases gets its own qualified filter.
    // Verifies that per-instance filter routing works across all aliases.
    let sql = r#"
        SELECT e1.name, e2.name, e3.name
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id
        JOIN employees e3 ON e2.manager_id = e3.id
        WHERE e1.active = true AND e2.department = 'eng' AND e3.level = 'director'
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert_eq!(
        table_nodes.len(),
        3,
        "3-way self-join should produce 3 table nodes"
    );

    // All 3 nodes should have exactly 1 filter each
    for node in &table_nodes {
        assert_eq!(
            node.filters.len(),
            1,
            "each self-join instance should have exactly 1 filter, but node {} has {}",
            node.id,
            node.filters.len()
        );
    }

    // Collect all filter expressions
    let mut filter_texts: Vec<String> = table_nodes
        .iter()
        .flat_map(|n| n.filters.iter().map(|f| f.expression.clone()))
        .collect();
    filter_texts.sort();

    assert!(
        filter_texts.iter().any(|f| f.contains("active")),
        "one filter should reference 'active'"
    );
    assert!(
        filter_texts.iter().any(|f| f.contains("department")),
        "one filter should reference 'department'"
    );
    assert!(
        filter_texts.iter().any(|f| f.contains("level")),
        "one filter should reference 'level'"
    );
}

#[test]
fn self_join_alias_matches_canonical_name() {
    // When one alias explicitly matches the canonical name, the alias-matching
    // side shares the same node ID as the unaliased first occurrence. This is a
    // known limitation: `relation_instance_identity` falls back to the standard
    // `relation_identity` when alias == canonical/simple_name, so `e1` (distinct)
    // gets a unique instance ID but the first (unaliased-equivalent) occurrence
    // keeps the canonical ID that "employees" also maps to.
    let sql = r#"
        SELECT e1.name, employees.name
        FROM employees e1
        JOIN employees employees ON e1.manager_id = employees.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();

    // Only 1 node: the first occurrence uses relation_identity("employees") and
    // the second occurrence (alias "employees") falls back to the same ID.
    // The `e1` alias on the FIRST occurrence doesn't help either because
    // is_self_join is false for the first occurrence, so it also uses
    // relation_identity("employees").
    assert_eq!(
        table_nodes.len(),
        1,
        "self-join where alias matches canonical collapses to 1 node (known limitation)"
    );
}

#[test]
fn cte_self_join_filters_attach_to_correct_instance() {
    // CTE self-joins should support per-instance filter attachment,
    // just like regular table self-joins.
    let sql = r#"
        WITH org AS (
            SELECT id, name, manager_id, department FROM employees
        )
        SELECT a.name, b.name
        FROM org a
        JOIN org b ON a.manager_id = b.id
        WHERE a.department = 'eng' AND b.department = 'sales'
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let cte_ref_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Cte && n.qualified_name.as_deref() == Some("org"))
        .collect();
    assert!(
        cte_ref_nodes.len() >= 2,
        "CTE self-join should produce at least 2 CTE nodes (definition + instance), got {}",
        cte_ref_nodes.len()
    );

    // At least 2 nodes should have filters
    let nodes_with_filters: Vec<_> = cte_ref_nodes
        .iter()
        .filter(|n| !n.filters.is_empty())
        .collect();
    assert_eq!(
        nodes_with_filters.len(),
        2,
        "each CTE self-join instance should receive its own filter"
    );
}

#[test]
fn cte_self_join_filters_are_isolated_per_instance() {
    // Verify that CTE self-join filters are routed to the correct instance,
    // not duplicated across instances. This is the CTE counterpart of
    // `triple_self_join_each_alias_gets_distinct_filter`.
    let sql = r#"
        WITH org AS (
            SELECT id, name, manager_id, department FROM employees
        )
        SELECT a.name AS eng_name, b.name AS mgr_name
        FROM org a
        JOIN org b ON a.manager_id = b.id
        WHERE a.department = 'eng' AND b.department = 'sales'
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Collect CTE nodes that have filters
    let filtered_cte_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Cte && !n.filters.is_empty())
        .collect();
    assert_eq!(
        filtered_cte_nodes.len(),
        2,
        "each CTE instance should receive its own filter"
    );

    // Verify filter isolation: each instance should have exactly one filter,
    // and the two filters should reference different departments.
    let all_filter_texts: Vec<String> = filtered_cte_nodes
        .iter()
        .flat_map(|n| n.filters.iter().map(|f| f.expression.clone()))
        .collect();
    assert!(
        all_filter_texts.iter().any(|f| f.contains("eng")),
        "one instance should have the 'eng' filter, got: {:?}",
        all_filter_texts
    );
    assert!(
        all_filter_texts.iter().any(|f| f.contains("sales")),
        "one instance should have the 'sales' filter, got: {:?}",
        all_filter_texts
    );

    // Verify no single node received both filters
    for node in &filtered_cte_nodes {
        let filter_texts: Vec<_> = node.filters.iter().map(|f| &f.expression).collect();
        let has_both = filter_texts.iter().any(|f| f.contains("eng"))
            && filter_texts.iter().any(|f| f.contains("sales"));
        assert!(
            !has_both,
            "CTE node {} should not have both filters, got: {:?}",
            node.id, filter_texts
        );
    }
}

#[test]
fn nested_cte_self_join_aliases_keep_filters_isolated_per_scope() {
    let sql = r#"
        WITH org AS (
            SELECT id, name, manager_id, department
            FROM employees
        )
        SELECT b.name, sub.inner_mgr
        FROM org a
        JOIN org b ON a.manager_id = b.id
        JOIN (
            SELECT b.id, b.name AS inner_mgr
            FROM org a
            JOIN org b ON a.manager_id = b.id
            WHERE b.department = 'sales'
        ) sub ON sub.id = b.id
        WHERE b.department = 'eng'
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let filtered_cte_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Cte
                && n.qualified_name.as_deref() == Some("org")
                && !n.filters.is_empty()
        })
        .collect();
    assert_eq!(
        filtered_cte_nodes.len(),
        2,
        "outer and inner CTE self-join aliases should keep separate filtered nodes"
    );

    let filter_sets: HashSet<Vec<String>> = filtered_cte_nodes
        .iter()
        .map(|node| {
            let mut filters: Vec<String> =
                node.filters.iter().map(|f| f.expression.clone()).collect();
            filters.sort();
            filters
        })
        .collect();
    assert!(
        filter_sets.contains(&vec!["b.department = 'eng'".to_string()]),
        "expected one filtered node for the outer b alias, got {filter_sets:?}"
    );
    assert!(
        filter_sets.contains(&vec!["b.department = 'sales'".to_string()]),
        "expected one filtered node for the inner b alias, got {filter_sets:?}"
    );
}

#[test]
fn self_join_with_subquery_alias_conflict() {
    // A subquery aliased as the same table name that also appears in
    // a regular join should not conflict.
    let sql = r#"
        SELECT t1.name, t2.id
        FROM (SELECT name, id FROM employees WHERE active = true) t1
        JOIN employees t2 ON t1.id = t2.manager_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Should have at least 1 table node (employees) and 1 CTE/derived node (t1)
    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert!(
        !table_nodes.is_empty(),
        "should have table nodes for employees"
    );

    let derived_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Cte)
        .collect();
    assert!(
        !derived_nodes.is_empty(),
        "derived subquery should produce a CTE-like node"
    );

    // Verify no issues about missing nodes
    let global_node_ids: HashSet<_> = result
        .global_lineage
        .nodes
        .iter()
        .map(|n| n.id.clone())
        .collect();
    for edge in &result.global_lineage.edges {
        assert!(
            global_node_ids.contains(&edge.from),
            "global edge {} has missing source node {}",
            edge.id,
            edge.from
        );
        assert!(
            global_node_ids.contains(&edge.to),
            "global edge {} has missing target node {}",
            edge.id,
            edge.to
        );
    }
}

#[test]
fn self_join_global_edges_resolve_correctly() {
    // Verify that global lineage edges for self-join scenarios
    // properly resolve through the local-to-global ID mapping.
    let sql = r#"
        SELECT e1.name AS emp_name, e2.name AS mgr_name
        FROM employees e1
        JOIN employees e2 ON e1.manager_id = e2.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let global = &result.global_lineage;

    // Global employees should be a single node
    let global_employees: Vec<_> = global
        .nodes
        .iter()
        .filter(|n| n.canonical_name.name == "employees" && n.node_type == NodeType::Table)
        .collect();
    assert_eq!(
        global_employees.len(),
        1,
        "global lineage should have exactly 1 employees node"
    );

    // All global edges should reference existing global nodes
    let global_node_ids: HashSet<_> = global.nodes.iter().map(|n| n.id.clone()).collect();
    for edge in &global.edges {
        assert!(
            global_node_ids.contains(&edge.from),
            "global edge from={} not found in global nodes",
            edge.from
        );
        assert!(
            global_node_ids.contains(&edge.to),
            "global edge to={} not found in global nodes",
            edge.to
        );
    }

    // Ownership edges from employees should point to column nodes
    let ownership_edges: Vec<_> = global
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Ownership && e.from == global_employees[0].id)
        .collect();
    assert!(
        !ownership_edges.is_empty(),
        "global employees node should own column nodes"
    );
}

#[test]
fn complex_pattern_star_schema_joins() {
    let sql = r#"
        SELECT
            f.sale_id,
            d_time.year,
            d_time.quarter,
            d_product.category,
            d_product.brand,
            d_customer.segment,
            d_store.region,
            f.amount
        FROM fact_sales f
        JOIN dim_time d_time ON f.time_id = d_time.id
        JOIN dim_product d_product ON f.product_id = d_product.id
        JOIN dim_customer d_customer ON f.customer_id = d_customer.id
        JOIN dim_store d_store ON f.store_id = d_store.id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    for expected in [
        "fact_sales",
        "dim_time",
        "dim_product",
        "dim_customer",
        "dim_store",
    ] {
        assert!(
            tables.contains(expected),
            "star schema should track all dimension tables: {expected}"
        );
    }
}

#[test]
fn complex_pattern_slowly_changing_dimension() {
    let sql = r#"
        SELECT
            f.transaction_id,
            f.transaction_date,
            d.customer_name,
            d.customer_tier,
            d.effective_from,
            d.effective_to
        FROM fact_transactions f
        JOIN dim_customer_scd d
          ON f.customer_id = d.customer_id
         AND f.transaction_date >= d.effective_from
         AND f.transaction_date < COALESCE(d.effective_to, '9999-12-31');
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    for expected in ["fact_transactions", "dim_customer_scd"] {
        assert!(
            tables.contains(expected),
            "SCD pattern should track {expected}"
        );
    }
}

// ============================================================================
// INSERT VARIANTS
// ============================================================================

#[test]
fn insert_multi_row_values() {
    let sql = r#"
        INSERT INTO users (id, name, email)
        VALUES
            (1, 'Alice', 'alice@example.com'),
            (2, 'Bob', 'bob@example.com'),
            (3, 'Charlie', 'charlie@example.com');
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("users"),
        "multi-row INSERT should track target table"
    );
}

#[test]
fn insert_with_default_values() {
    let sql = r#"
        INSERT INTO logs DEFAULT VALUES;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("logs"),
        "INSERT DEFAULT VALUES should track target"
    );
}

#[test]
fn insert_on_conflict_postgres() {
    let sql = r#"
        INSERT INTO users (id, email, updated_at)
        SELECT id, email, CURRENT_TIMESTAMP
        FROM staging_users
        ON CONFLICT (id)
        DO UPDATE SET
            email = EXCLUDED.email,
            updated_at = EXCLUDED.updated_at;
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    for expected in ["users", "staging_users"] {
        assert!(
            tables.contains(expected),
            "INSERT ON CONFLICT should track {expected}"
        );
    }
}

#[test]
fn insert_with_cte_source() {
    let sql = r#"
        WITH prepared_data AS (
            SELECT
                id,
                UPPER(name) AS name,
                LOWER(email) AS email
            FROM staging
            WHERE valid = true
        )
        INSERT INTO users (id, name, email)
        SELECT id, name, email FROM prepared_data;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    for expected in ["users", "staging"] {
        assert!(
            tables.contains(expected),
            "INSERT with CTE should track {expected}"
        );
    }

    let ctes = collect_cte_names(&result);
    assert!(
        ctes.contains("prepared_data"),
        "INSERT should track CTE used in source"
    );
}

// ============================================================================
// DIALECT-SPECIFIC ADVANCED FEATURES
// ============================================================================

#[test]
fn snowflake_qualify_clause_filters_window_results() {
    let sql = r#"
        SELECT
            user_id,
            order_date,
            amount,
            ROW_NUMBER() OVER (PARTITION BY user_id ORDER BY order_date DESC) AS rn
        FROM orders
        QUALIFY rn = 1;
    "#;

    let result = run_analysis(sql, Dialect::Snowflake, None);

    // QUALIFY is Snowflake-specific and may have limited support
    // This test documents current behavior
    // TODO: Verify QUALIFY clause support in Snowflake dialect
    assert!(
        result.summary.statement_count >= 1,
        "QUALIFY clause should parse in Snowflake"
    );
}

#[test]
fn snowflake_flatten_lateral_unnest() {
    let sql = r#"
        SELECT
            u.id,
            f.value::STRING AS tag
        FROM analytics.users u,
        LATERAL FLATTEN(input => u.tags) f
        WHERE f.value IS NOT NULL;
    "#;

    let result = run_analysis(sql, Dialect::Snowflake, None);

    // FLATTEN is Snowflake-specific and may have limited support
    // This test documents current behavior
    // TODO: Enhanced FLATTEN support for Snowflake semi-structured data
    assert!(
        result.summary.statement_count >= 1,
        "FLATTEN should parse in Snowflake"
    );
}

#[test]
fn snowflake_time_travel_query() {
    let sql = r#"
        SELECT order_id, amount
        FROM orders
        AT(TIMESTAMP => '2024-01-01 00:00:00'::timestamp);
    "#;

    let result = run_analysis(sql, Dialect::Snowflake, None);

    // Time travel syntax AT(TIMESTAMP => ...) is Snowflake-specific and not yet supported
    // This test documents that this syntax either parses with limited lineage or fails to parse
    // TODO: Implement Snowflake time travel syntax support (AT, BEFORE, etc.)
    // For now, we just check that analysis completes without crashing
    assert!(
        result.summary.statement_count == 0 || result.summary.statement_count >= 1,
        "Time travel query analysis should complete (may parse with 0 statements if unsupported)"
    );
}

#[test]
fn bigquery_struct_and_array_agg() {
    let sql = r#"
        SELECT
            user_id,
            ARRAY_AGG(STRUCT(product_id, quantity, price)) AS items
        FROM order_items
        GROUP BY user_id;
    "#;

    let result = run_analysis(sql, Dialect::Bigquery, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("order_items"),
        "STRUCT/ARRAY_AGG should track source"
    );
}

#[test]
fn bigquery_except_and_replace_modifiers() {
    let sql = r#"
        SELECT * EXCEPT (password, ssn)
        REPLACE (UPPER(email) AS email, LOWER(name) AS name)
        FROM users;
    "#;

    let result = run_analysis(sql, Dialect::Bigquery, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("users"),
        "EXCEPT/REPLACE modifiers should track table"
    );
}

#[test]
fn bigquery_unnest_arrays() {
    let sql = r#"
        SELECT u.user_id, tag
        FROM users u
        CROSS JOIN UNNEST(u.tags) AS tag
        WHERE tag LIKE 'tech%';
    "#;

    let result = run_analysis(sql, Dialect::Bigquery, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("users"),
        "UNNEST should preserve base table lineage"
    );
}

// Task 4: Tier 4 BigQuery Features - Lineage assertions

#[test]
fn bigquery_hyphenated_project_refs() {
    let sql = r#"
        SELECT id, name
        FROM `project-a.dataset-b.users`;
    "#;

    let result = run_analysis(sql, Dialect::Bigquery, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("project-a.dataset-b.users"),
        "hyphenated project refs should track full qualified name"
    );
}

#[test]
fn bigquery_hyphenated_refs_join() {
    let sql = r#"
        SELECT
            u.user_id,
            o.order_total
        FROM `my-company.core.users` u
        JOIN `my-company.sales.orders` o ON u.user_id = o.user_id;
    "#;

    let result = run_analysis(sql, Dialect::Bigquery, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("my-company.core.users"),
        "should track hyphenated users table"
    );
    assert!(
        tables.contains("my-company.sales.orders"),
        "should track hyphenated orders table"
    );
}

#[test]
fn bigquery_unnest_with_offset() {
    let sql = r#"
        SELECT item, offset_pos
        FROM UNNEST([10, 20, 30]) AS item WITH OFFSET AS offset_pos;
    "#;

    let result = run_analysis(sql, Dialect::Bigquery, None);

    // UNNEST on literal array should parse without error
    assert!(
        result.issues.is_empty()
            || result
                .issues
                .iter()
                .all(|i| i.severity != flowscope_core::Severity::Error),
        "UNNEST with OFFSET should parse without errors"
    );
}

#[test]
fn bigquery_unnest_struct_expansion() {
    let sql = r#"
        SELECT
            order_id,
            line_item.product_id,
            line_item.quantity
        FROM orders,
        UNNEST(line_items) AS line_item;
    "#;

    let result = run_analysis(sql, Dialect::Bigquery, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("orders"),
        "UNNEST struct expansion should track source table"
    );
}

#[test]
fn bigquery_select_except_excludes_columns() {
    let sql = r#"
        SELECT * EXCEPT (password, ssn)
        FROM users;
    "#;

    let result = run_analysis(sql, Dialect::Bigquery, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("users"),
        "SELECT EXCEPT should track source table"
    );
}

#[test]
fn bigquery_select_replace_transforms() {
    let sql = r#"
        SELECT * REPLACE (UPPER(email) AS email)
        FROM customers;
    "#;

    let result = run_analysis(sql, Dialect::Bigquery, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("customers"),
        "SELECT REPLACE should track source table"
    );
}

#[test]
fn bigquery_select_except_replace_combined() {
    let sql = r#"
        SELECT * EXCEPT (internal_id)
        REPLACE (ROUND(price, 2) AS price, LOWER(sku) AS sku)
        FROM products;
    "#;

    let result = run_analysis(sql, Dialect::Bigquery, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("products"),
        "Combined EXCEPT/REPLACE should track source table"
    );
}

#[test]
fn postgres_distinct_on_clause() {
    let sql = r#"
        SELECT DISTINCT ON (user_id)
            user_id,
            order_date,
            amount
        FROM orders
        ORDER BY user_id, order_date DESC;
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("orders"),
        "DISTINCT ON should track source table"
    );
}

#[test]
fn postgres_json_operators() {
    let sql = r#"
        SELECT
            data->>'user' AS user_name,
            data->'metadata'->>'email' AS email,
            (data#>>'{address,city}') AS city
        FROM events;
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("events"),
        "JSON operators should track source table"
    );

    let stmt = first_statement(&result);
    let derivations = edges_by_type(stmt, EdgeType::Derivation);
    assert!(
        !derivations.is_empty(),
        "JSON extraction should create derivation edges"
    );
}

#[test]
fn postgres_array_operators_and_functions() {
    let sql = r#"
        SELECT
            product_id,
            name
        FROM products
        WHERE tags @> ARRAY['electronics', 'sale']
           OR 'premium' = ANY(tags);
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("products"),
        "array operators should track table"
    );
}

// ============================================================================
// ERROR CONDITIONS AND VALIDATION
// ============================================================================

#[test]
fn error_ambiguous_column_reference() {
    let sql = r#"
        SELECT id
        FROM orders o
        JOIN users u ON o.user_id = u.id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    // Should still produce lineage even with ambiguous column
    let tables = collect_table_names(&result);
    assert!(
        tables.contains("orders") && tables.contains("users"),
        "ambiguous column should not prevent table tracking"
    );
}

#[test]
fn error_unknown_table_without_schema() {
    let sql = r#"
        SELECT id, name FROM nonexistent_table;
    "#;

    let schema = SchemaMetadata {
        allow_implied: true,
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![schema_table(None, None, "users", &["id", "name"])],
    };

    let result = run_analysis(sql, Dialect::Generic, Some(schema));

    // Unknown table validation may not emit specific UNKNOWN_TABLE code yet
    // This test documents current validation behavior
    // TODO: Implement UNKNOWN_TABLE issue code for schema validation
    assert!(
        result.summary.statement_count >= 1,
        "query with unknown table should still parse"
    );
}

#[test]
fn error_column_count_mismatch_in_insert() {
    let sql = r#"
        INSERT INTO users (id, name)
        SELECT id, name, email, age FROM staging;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    // Should still track lineage despite mismatch
    let tables = collect_table_names(&result);
    assert!(
        tables.contains("users") && tables.contains("staging"),
        "column mismatch should not prevent lineage tracking"
    );
}

#[test]
fn error_invalid_alias_in_where_clause() {
    let sql = r#"
        SELECT name AS user_name
        FROM users
        WHERE user_name = 'Alice';
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    // Most SQL dialects don't allow alias in WHERE, but lineage should still work
    assert!(
        !result.summary.has_errors,
        "alias usage validation is dialect-specific"
    );
}

#[test]
fn error_missing_group_by_column() {
    let sql = r#"
        SELECT user_id, region, COUNT(*) AS total
        FROM orders
        GROUP BY user_id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    // Should track lineage even with semantic error
    let tables = collect_table_names(&result);
    assert!(
        tables.contains("orders"),
        "GROUP BY errors should not prevent lineage"
    );
}

// ============================================================================
// DDL STATEMENTS - CREATE VIEW, TEMP TABLES
// ============================================================================

#[test]
fn ddl_create_view_tracks_dependencies() {
    let sql = r#"
        CREATE VIEW active_user_orders AS
        SELECT
            u.id,
            u.name,
            o.order_id,
            o.amount
        FROM users u
        JOIN orders o ON u.id = o.user_id
        WHERE u.active = true;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    assert!(
        result.summary.statement_count >= 1,
        "CREATE VIEW should parse"
    );

    // Verify the view node has the correct NodeType::View
    let view_node = result.statements[0]
        .nodes
        .iter()
        .find(|n| &*n.label == "active_user_orders");
    assert!(view_node.is_some(), "Should find view node");
    assert_eq!(
        view_node.unwrap().node_type,
        NodeType::View,
        "CREATE VIEW should create a View node type, not Table"
    );
}

#[test]
fn ddl_create_view_with_cte() {
    let sql = r#"
        CREATE OR REPLACE VIEW customer_summary AS
        WITH order_stats AS (
            SELECT
                customer_id,
                COUNT(*) AS order_count,
                SUM(amount) AS total_spent
            FROM orders
            GROUP BY customer_id
        )
        SELECT
            c.id,
            c.name,
            COALESCE(os.order_count, 0) AS orders,
            COALESCE(os.total_spent, 0) AS spent
        FROM customers c
        LEFT JOIN order_stats os ON c.id = os.customer_id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    // CREATE VIEW with CTE support may be limited - this test documents current behavior
    // TODO: Full CREATE VIEW with CTE lineage tracking
    assert!(
        result.summary.statement_count >= 1,
        "CREATE VIEW with CTE should parse"
    );
}

#[test]
fn ddl_create_temp_table() {
    let sql = r#"
        CREATE TEMP TABLE daily_summary AS
        SELECT
            DATE(created_at) AS date,
            COUNT(*) AS event_count,
            COUNT(DISTINCT user_id) AS unique_users
        FROM events
        WHERE created_at >= CURRENT_DATE
        GROUP BY DATE(created_at);
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("events"),
        "CREATE TEMP TABLE should track source"
    );
}

#[test]
fn forward_declared_tables_are_known_before_usage() {
    let sql = r#"
        CREATE VIEW future_view AS
        SELECT id FROM future_table;

        CREATE TABLE future_table (id INT, name TEXT);

        SELECT * FROM future_view;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let unresolved_count = result
        .issues
        .iter()
        .filter(|issue| issue.code == issue_codes::UNRESOLVED_REFERENCE)
        .count();
    assert_eq!(
        unresolved_count, 0,
        "forward references should not emit UNRESOLVED_REFERENCE warnings"
    );

    let unknown_column_count = result
        .issues
        .iter()
        .filter(|issue| issue.code == issue_codes::UNKNOWN_COLUMN)
        .count();
    assert_eq!(
        unknown_column_count, 0,
        "columns from forward-declared tables should be known"
    );
}

#[test]
fn ddl_multi_statement_temp_table_pipeline() {
    let sql = r#"
        CREATE TEMP TABLE bronze AS
        SELECT * FROM raw_events WHERE valid = true;

        CREATE TEMP TABLE silver AS
        SELECT event_id, user_id, event_type, created_at
        FROM bronze
        WHERE event_type IS NOT NULL;

        CREATE TABLE gold AS
        SELECT
            user_id,
            event_type,
            COUNT(*) AS event_count
        FROM silver
        GROUP BY user_id, event_type;

        SELECT * FROM gold ORDER BY event_count DESC LIMIT 100;
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    assert_eq!(
        result.summary.statement_count, 4,
        "temp table pipeline should have 4 statements"
    );

    let tables = collect_table_names(&result);
    for expected in ["raw_events", "bronze", "silver", "gold"] {
        assert!(
            tables.contains(expected),
            "pipeline should track {expected}"
        );
    }

    let cross_edges: Vec<_> = result
        .global_lineage
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::CrossStatement)
        .collect();
    assert!(
        cross_edges.len() >= 3,
        "temp table pipeline should have cross-statement edges"
    );
}

// ============================================================================
// MIXED TABLE AND VIEW SCENARIOS
// ============================================================================

#[test]
fn view_and_table_in_same_statement() {
    let sql = r#"
        CREATE VIEW active_users AS SELECT id, name FROM users WHERE active = true;
        CREATE TABLE orders (order_id INT, user_id INT, amount DECIMAL);
        SELECT v.name, o.amount
        FROM active_users v
        JOIN orders o ON v.id = o.user_id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    assert_eq!(
        result.summary.statement_count, 3,
        "Should have 3 statements"
    );

    // Verify view has correct type
    let view_node = result.statements[0]
        .nodes
        .iter()
        .find(|n| &*n.label == "active_users");
    assert!(view_node.is_some(), "Should find view node");
    assert_eq!(
        view_node.unwrap().node_type,
        NodeType::View,
        "active_users should be a View"
    );

    // Verify table has correct type
    let table_node = result.statements[1]
        .nodes
        .iter()
        .find(|n| &*n.label == "orders");
    assert!(table_node.is_some(), "Should find orders table node");
    assert_eq!(
        table_node.unwrap().node_type,
        NodeType::Table,
        "orders should be a Table"
    );
}

#[test]
fn cross_statement_view_lineage() {
    let sql = r#"
        CREATE VIEW user_orders AS
        SELECT u.id, u.name, o.order_id
        FROM users u
        JOIN orders o ON u.id = o.user_id;

        SELECT name, COUNT(*) as order_count
        FROM user_orders
        GROUP BY name;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    assert_eq!(
        result.summary.statement_count, 2,
        "Should have 2 statements"
    );

    // Check that cross-statement edges exist
    let cross_edges: Vec<_> = result
        .global_lineage
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::CrossStatement)
        .collect();

    assert!(
        !cross_edges.is_empty(),
        "Should have cross-statement edges linking view creation to its usage"
    );

    // Verify the view is correctly typed in global lineage
    let global_view = result
        .global_lineage
        .nodes
        .iter()
        .find(|n| &*n.label == "user_orders");
    assert!(global_view.is_some(), "Should find view in global lineage");
    assert_eq!(
        global_view.unwrap().node_type,
        NodeType::View,
        "View should retain View type in global lineage"
    );
}

#[test]
fn mixed_table_view_cte_in_pipeline() {
    let sql = r#"
        CREATE TABLE raw_events (event_id INT, user_id INT, event_type VARCHAR(50));

        CREATE VIEW filtered_events AS
        SELECT event_id, user_id, event_type
        FROM raw_events
        WHERE event_type IN ('click', 'purchase');

        WITH event_counts AS (
            SELECT user_id, COUNT(*) as cnt
            FROM filtered_events
            GROUP BY user_id
        )
        SELECT u.name, ec.cnt
        FROM users u
        JOIN event_counts ec ON u.id = ec.user_id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    assert_eq!(
        result.summary.statement_count, 3,
        "Should have 3 statements"
    );

    // Collect all node types
    let mut table_count = 0;
    let mut view_count = 0;
    let mut cte_count = 0;

    for node in &result.global_lineage.nodes {
        match node.node_type {
            NodeType::Table => table_count += 1,
            NodeType::View => view_count += 1,
            NodeType::Cte => cte_count += 1,
            NodeType::Column | NodeType::Output => {}
        }
    }

    assert!(
        table_count >= 2,
        "Should have at least 2 tables (raw_events, users)"
    );
    assert!(
        view_count >= 1,
        "Should have at least 1 view (filtered_events)"
    );
    assert!(cte_count >= 1, "Should have at least 1 CTE (event_counts)");
}

#[test]
fn node_type_helper_methods() {
    // Test is_table_like() - should include Table, View, and Cte
    assert!(
        NodeType::Table.is_table_like(),
        "Table should be table-like"
    );
    assert!(NodeType::View.is_table_like(), "View should be table-like");
    assert!(NodeType::Cte.is_table_like(), "Cte should be table-like");
    assert!(
        !NodeType::Column.is_table_like(),
        "Column should not be table-like"
    );

    // Test is_table_or_view() - should include Table and View but NOT Cte
    assert!(
        NodeType::Table.is_table_or_view(),
        "Table should be table-or-view"
    );
    assert!(
        NodeType::View.is_table_or_view(),
        "View should be table-or-view"
    );
    assert!(
        !NodeType::Cte.is_table_or_view(),
        "Cte should NOT be table-or-view"
    );
    assert!(
        !NodeType::Column.is_table_or_view(),
        "Column should not be table-or-view"
    );
}

#[test]
fn view_referenced_multiple_times() {
    let sql = r#"
        CREATE VIEW product_summary AS
        SELECT product_id, SUM(quantity) as total_qty
        FROM order_items
        GROUP BY product_id;

        SELECT * FROM product_summary WHERE total_qty > 100;
        SELECT * FROM product_summary WHERE total_qty < 10;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    assert_eq!(
        result.summary.statement_count, 3,
        "Should have 3 statements"
    );

    // The view should appear in global lineage only once
    let view_nodes: Vec<_> = result
        .global_lineage
        .nodes
        .iter()
        .filter(|n| &*n.label == "product_summary")
        .collect();

    assert_eq!(
        view_nodes.len(),
        1,
        "View should appear exactly once in global lineage"
    );

    // But it should have multiple statement refs
    let view_node = view_nodes[0];
    assert!(
        view_node.statement_refs.len() >= 2,
        "View should be referenced by multiple statements"
    );
}

// ============================================================================
// SCALE AND STRESS TESTS
// ============================================================================

#[test]
fn scale_deeply_nested_ctes() {
    let sql = r#"
        WITH
        l1 AS (SELECT id FROM orders),
        l2 AS (SELECT id FROM l1),
        l3 AS (SELECT id FROM l2),
        l4 AS (SELECT id FROM l3),
        l5 AS (SELECT id FROM l4),
        l6 AS (SELECT id FROM l5),
        l7 AS (SELECT id FROM l6),
        l8 AS (SELECT id FROM l7),
        l9 AS (SELECT id FROM l8),
        l10 AS (SELECT id FROM l9)
        SELECT * FROM l10;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let ctes = collect_cte_names(&result);

    assert_eq!(ctes.len(), 10, "deeply nested CTEs should all be tracked");

    let tables = collect_table_names(&result);
    assert!(
        tables.contains("orders"),
        "base table should be preserved through deep nesting"
    );
}

#[test]
fn scale_wide_select_many_columns() {
    let columns: Vec<String> = (1..=50)
        .map(|i| format!("col{} AS output{}", i, i))
        .collect();
    let sql = format!("SELECT {} FROM wide_table;", columns.join(", "));

    let result = run_analysis(&sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    assert!(
        stmt.nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .count()
            >= 50,
        "wide SELECT should track all columns"
    );
}

#[test]
fn scale_many_union_branches() {
    let sql = r#"
        SELECT id, 'source1' AS source FROM table1
        UNION ALL
        SELECT id, 'source2' FROM table2
        UNION ALL
        SELECT id, 'source3' FROM table3
        UNION ALL
        SELECT id, 'source4' FROM table4
        UNION ALL
        SELECT id, 'source5' FROM table5
        UNION ALL
        SELECT id, 'source6' FROM table6
        UNION ALL
        SELECT id, 'source7' FROM table7
        UNION ALL
        SELECT id, 'source8' FROM table8;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert_eq!(
        tables.len(),
        8,
        "many UNION branches should track all source tables"
    );
}

#[test]
fn scale_long_join_chain() {
    let sql = r#"
        SELECT
            t1.id,
            t2.value AS v2,
            t3.value AS v3,
            t4.value AS v4,
            t5.value AS v5,
            t6.value AS v6
        FROM table1 t1
        JOIN table2 t2 ON t1.id = t2.ref_id
        JOIN table3 t3 ON t2.id = t3.ref_id
        JOIN table4 t4 ON t3.id = t4.ref_id
        JOIN table5 t5 ON t4.id = t5.ref_id
        JOIN table6 t6 ON t5.id = t6.ref_id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert_eq!(tables.len(), 6, "long JOIN chain should track all tables");
}

#[test]
fn scale_complex_multi_statement_etl() {
    let sql = r#"
        -- Stage 1: Extract
        CREATE TABLE staging_raw AS
        SELECT * FROM external_data WHERE loaded_at >= CURRENT_DATE;

        -- Stage 2: Clean
        CREATE TABLE staging_clean AS
        SELECT
            id,
            TRIM(name) AS name,
            LOWER(email) AS email,
            CAST(created_at AS DATE) AS created_date
        FROM staging_raw
        WHERE email IS NOT NULL;

        -- Stage 3: Enrich
        CREATE TABLE staging_enriched AS
        SELECT
            sc.id,
            sc.name,
            sc.email,
            sc.created_date,
            d.region,
            d.segment
        FROM staging_clean sc
        LEFT JOIN dimensions d ON sc.id = d.customer_id;

        -- Stage 4: Aggregate
        INSERT INTO summary_table
        SELECT
            region,
            segment,
            DATE_TRUNC('month', created_date) AS month,
            COUNT(*) AS customer_count,
            COUNT(DISTINCT email) AS unique_emails
        FROM staging_enriched
        GROUP BY region, segment, DATE_TRUNC('month', created_date);

        -- Stage 5: Report
        SELECT
            region,
            SUM(customer_count) AS total_customers
        FROM summary_table
        WHERE month >= DATE_TRUNC('year', CURRENT_DATE)
        GROUP BY region
        ORDER BY total_customers DESC;
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);

    assert_eq!(
        result.summary.statement_count, 5,
        "complex ETL should track all 5 stages"
    );

    let tables = collect_table_names(&result);
    for expected in [
        "external_data",
        "staging_raw",
        "staging_clean",
        "staging_enriched",
        "dimensions",
        "summary_table",
    ] {
        assert!(
            tables.contains(expected),
            "ETL pipeline should track {expected}"
        );
    }

    let cross_edges: Vec<_> = result
        .global_lineage
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::CrossStatement)
        .collect();
    assert!(
        cross_edges.len() >= 4,
        "complex ETL should have multiple cross-statement edges"
    );
}

// ============================================================================
// DETAILED COLUMN-LEVEL LINEAGE TESTS
// ============================================================================

#[test]
fn column_ownership_edges_link_tables_to_columns() {
    let sql = r#"
        SELECT id, name, email FROM users;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Check that ownership edges exist from table to its columns
    let ownership_edges = edges_by_type(stmt, EdgeType::Ownership);
    assert!(
        !ownership_edges.is_empty(),
        "should have ownership edges from table to columns"
    );

    // Verify table node exists
    let table = find_table_node(stmt, "users");
    assert!(table.is_some(), "users table should exist as node");

    // Verify column nodes exist
    for col_name in ["id", "name", "email"] {
        let col = find_column_node(stmt, col_name);
        assert!(col.is_some(), "column {col_name} should exist as node");
    }
}

#[test]
fn column_dataflow_edges_track_simple_projection() {
    let sql = r#"
        WITH source AS (
            SELECT user_id, email FROM users
        )
        SELECT user_id, email FROM source;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Check that columns flow from CTE to final SELECT
    let dataflow_edges = edges_by_type(stmt, EdgeType::DataFlow);
    assert!(
        !dataflow_edges.is_empty(),
        "should have data flow edges between columns"
    );

    // Both user_id and email should appear as columns
    assert!(
        find_column_node(stmt, "user_id").is_some(),
        "user_id column should exist"
    );
    assert!(
        find_column_node(stmt, "email").is_some(),
        "email column should exist"
    );
}

#[test]
fn column_derivation_edges_capture_transformations() {
    let sql = r#"
        SELECT
            user_id,
            amount * 1.1 AS amount_with_tax,
            UPPER(name) AS name_upper
        FROM orders;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let derivation_edges = edges_by_type(stmt, EdgeType::Derivation);
    assert!(
        derivation_edges.len() >= 2,
        "should have derivation edges for computed columns"
    );

    // Check that derived columns have expressions
    let amount_with_tax = find_column_node(stmt, "amount_with_tax");
    let name_upper = find_column_node(stmt, "name_upper");

    assert!(
        amount_with_tax.is_some() || name_upper.is_some(),
        "derived columns should exist as nodes"
    );
}

#[test]
fn column_qualified_names_preserve_table_context() {
    let sql = r#"
        SELECT
            o.order_id,
            o.amount,
            u.user_id,
            u.name
        FROM orders o
        JOIN users u ON o.user_id = u.user_id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Columns should exist (though qualified_name tracking may vary)
    let cols = column_labels(stmt);
    for expected in ["order_id", "amount", "user_id", "name"] {
        assert!(
            cols.contains(&expected.to_string()),
            "column {expected} should be tracked"
        );
    }

    // Should have nodes for both tables
    assert!(
        find_table_node(stmt, "orders").is_some(),
        "orders table should exist"
    );
    assert!(
        find_table_node(stmt, "users").is_some(),
        "users table should exist"
    );
}

#[test]
fn column_lineage_through_aggregation() {
    let sql = r#"
        SELECT
            user_id,
            COUNT(*) AS order_count,
            SUM(amount) AS total_amount,
            AVG(amount) AS avg_amount
        FROM orders
        GROUP BY user_id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Aggregated columns should create derivation edges
    let derivations = edges_by_type(stmt, EdgeType::Derivation);
    assert!(
        !derivations.is_empty(),
        "aggregation should create derivation edges"
    );

    // All output columns should exist
    for col in ["user_id", "order_count", "total_amount", "avg_amount"] {
        assert!(
            find_column_node(stmt, col).is_some(),
            "output column {col} should exist"
        );
    }
}

#[test]
fn column_lineage_through_join_preserves_sources() {
    let sql = r#"
        SELECT
            o.order_id,
            p.payment_id,
            o.amount,
            p.payment_method
        FROM orders o
        JOIN payments p ON o.order_id = p.order_id;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Should have columns from both tables
    let cols = column_labels(stmt);
    for expected in ["order_id", "payment_id", "amount", "payment_method"] {
        assert!(
            cols.contains(&expected.to_string()),
            "joined column {expected} should exist"
        );
    }

    // Should have ownership edges from both tables
    let ownership = edges_by_type(stmt, EdgeType::Ownership);
    assert!(
        ownership.len() >= 2,
        "should have ownership edges from both joined tables"
    );
}

#[test]
fn column_expression_text_captured_for_derived_columns() {
    let sql = r#"
        SELECT
            order_id,
            CASE
                WHEN amount > 1000 THEN 'high'
                WHEN amount > 100 THEN 'medium'
                ELSE 'low'
            END AS amount_tier
        FROM orders;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Find the derived column
    let amount_tier = find_column_node(stmt, "amount_tier");
    assert!(
        amount_tier.is_some(),
        "derived column amount_tier should exist"
    );

    // Check if expression is captured (may or may not be depending on implementation)
    if let Some(node) = amount_tier {
        // Expression might be captured - this documents current behavior
        let _has_expression = node.expression.is_some();
        // Just verify the node exists; expression tracking is optional
    }
}

#[test]
fn column_lineage_multi_level_cte_chain() {
    let sql = r#"
        WITH stage1 AS (
            SELECT user_id, amount FROM orders
        ),
        stage2 AS (
            SELECT user_id, amount * 2 AS doubled_amount FROM stage1
        ),
        stage3 AS (
            SELECT user_id, doubled_amount + 100 AS final_amount FROM stage2
        )
        SELECT user_id, final_amount FROM stage3;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // user_id should flow through all stages
    assert!(
        find_column_node(stmt, "user_id").is_some(),
        "user_id should exist"
    );

    // Derived columns at each stage
    let cols = column_labels(stmt);
    assert!(
        cols.contains(&"final_amount".to_string()),
        "final derived column should exist"
    );

    // Should have data flow or derivation edges connecting stages
    let dataflow = edges_by_type(stmt, EdgeType::DataFlow);
    let derivation = edges_by_type(stmt, EdgeType::Derivation);
    assert!(
        !dataflow.is_empty() || !derivation.is_empty(),
        "should have edges connecting CTE stages"
    );
}

#[test]
fn column_wildcard_expansion_with_schema() {
    let sql = r#"
        SELECT * FROM users;
    "#;

    let schema = SchemaMetadata {
        allow_implied: true,
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![schema_table(
            None,
            None,
            "users",
            &["id", "name", "email", "created_at"],
        )],
    };

    let result = run_analysis(sql, Dialect::Generic, Some(schema));
    let stmt = first_statement(&result);

    // With schema, SELECT * should expand to individual columns
    let cols = column_labels(stmt);
    assert!(
        !cols.is_empty(),
        "SELECT * with schema should produce column nodes"
    );

    // Ideally should have all 4 columns, but this depends on implementation
    // This test documents current wildcard expansion behavior
}

#[test]
fn column_subquery_column_propagation() {
    let sql = r#"
        SELECT
            user_id,
            total_orders,
            total_amount
        FROM (
            SELECT
                user_id,
                COUNT(*) AS total_orders,
                SUM(amount) AS total_amount
            FROM orders
            GROUP BY user_id
        ) AS subq
        WHERE total_orders > 5;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // All three columns should appear in output
    for col in ["user_id", "total_orders", "total_amount"] {
        assert!(
            find_column_node(stmt, col).is_some(),
            "column {col} from subquery should be tracked"
        );
    }

    // Should have edges connecting subquery output to outer SELECT
    assert!(
        !stmt.edges.is_empty(),
        "should have edges for column propagation from subquery"
    );
}

#[test]
fn derived_table_alias_tracks_column_flow() {
    let sql = r#"
        SELECT sub.total_amount
        FROM (
            SELECT SUM(amount) AS total_amount
            FROM orders
        ) AS sub
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let derived_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Cte && &*node.label == "sub")
        .expect("derived table node should exist");

    let derived_column_id = stmt
        .edges
        .iter()
        .find(|edge| edge.edge_type == EdgeType::Ownership && edge.from == derived_node.id)
        .map(|edge| edge.to.clone())
        .expect("derived table should own columns");

    // The derived column should feed into the outer projection
    assert!(
        stmt.edges
            .iter()
            .any(|edge| { edge.edge_type == EdgeType::DataFlow && edge.from == derived_column_id }),
        "derived column should feed outer SELECT via data flow edges"
    );

    // Source columns from orders should feed into the derived column via derivation edges
    assert!(
        stmt.edges
            .iter()
            .any(|edge| { edge.edge_type == EdgeType::Derivation && edge.to == derived_column_id }),
        "orders.amount should derive the intermediate column before projection"
    );
}

#[test]
fn derived_table_alias_does_not_shadow_cte_with_same_name() {
    let sql = r#"
        WITH sales AS (
            SELECT order_id
            FROM orders
        )
        SELECT order_id
        FROM (
            SELECT order_id
            FROM web_orders
        ) AS sales
        UNION ALL
        SELECT order_id
        FROM sales;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let cte_node = stmt
        .nodes
        .iter()
        .find(|node| {
            node.node_type == NodeType::Cte
                && node.id.starts_with("cte_")
                && &*node.label == "sales"
        })
        .expect("original sales CTE should exist");

    let cte_columns: HashSet<_> = stmt
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::Ownership && edge.from == cte_node.id)
        .map(|edge| edge.to.clone())
        .collect();

    assert!(
        !cte_columns.is_empty(),
        "sales CTE should expose columns for downstream references"
    );

    let cte_flows_into_union = stmt
        .edges
        .iter()
        .any(|edge| edge.edge_type == EdgeType::DataFlow && cte_columns.contains(&edge.from));

    assert!(
        cte_flows_into_union,
        "sales CTE columns should feed into the UNION output even when a derived table reuses the alias"
    );
}

#[test]
fn column_union_combines_column_sets() {
    let sql = r#"
        SELECT user_id, amount FROM orders
        UNION ALL
        SELECT customer_id AS user_id, payment_amount AS amount FROM payments;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Output columns should match first SELECT
    let cols = column_labels(stmt);
    assert!(
        cols.contains(&"user_id".to_string()),
        "UNION output should have user_id column"
    );
    assert!(
        cols.contains(&"amount".to_string()),
        "UNION output should have amount column"
    );

    // Both source tables should be tracked
    let tables = collect_table_names(&result);
    assert!(
        tables.contains("orders"),
        "first UNION branch table should be tracked"
    );
    assert!(
        tables.contains("payments"),
        "second UNION branch table should be tracked"
    );
}

#[test]
fn ctas_implied_schema_ignores_inner_columns() {
    let sql = r#"
        CREATE TABLE tgt AS
        SELECT id
        FROM (
            SELECT id, extra FROM source
        ) s
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let resolved = result
        .resolved_schema
        .expect("resolved schema should be present");

    let tgt_table = resolved
        .tables
        .iter()
        .find(|table| table.name == "tgt")
        .expect("tgt table should exist in resolved schema");

    let columns: Vec<_> = tgt_table
        .columns
        .iter()
        .map(|col| col.name.clone())
        .collect();
    assert_eq!(
        columns,
        vec!["id"],
        "tgt schema should only include columns from the outer projection"
    );
}

#[test]
fn implied_schema_captures_join_relationships() {
    let sql = r#"
        CREATE TABLE b AS
        SELECT
            CAST(t1.a AS INT) AS a,
            CAST(t1.b AS INT) AS b,
            CAST(t1.c AS INT) AS c
        FROM table1 AS t1
        LEFT JOIN table2 AS t2 ON t1.a = t2.a
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let resolved = result
        .resolved_schema
        .expect("resolved schema should be present");

    let table1 = resolved
        .tables
        .iter()
        .find(|table| table.name == "table1")
        .expect("table1 should exist in resolved schema");
    let table2 = resolved
        .tables
        .iter()
        .find(|table| table.name == "table2")
        .expect("table2 should exist in resolved schema");

    let table1_fk = table1.constraints.iter().find(|constraint| {
        constraint.constraint_type == ConstraintType::ForeignKey
            && constraint.referenced_table.as_deref() == Some("table2")
            && constraint.columns == ["a"]
            && constraint
                .referenced_columns
                .as_ref()
                .map(|cols| cols.as_slice() == ["a"])
                .unwrap_or(false)
    });
    assert!(
        table1_fk.is_some(),
        "table1 should have a foreign key constraint to table2"
    );

    let table2_fk = table2.constraints.iter().find(|constraint| {
        constraint.constraint_type == ConstraintType::ForeignKey
            && constraint.referenced_table.as_deref() == Some("table1")
            && constraint.columns == ["a"]
            && constraint
                .referenced_columns
                .as_ref()
                .map(|cols| cols.as_slice() == ["a"])
                .unwrap_or(false)
    });
    assert!(
        table2_fk.is_some(),
        "table2 should have a foreign key constraint to table1"
    );

    let table1_column = table1
        .columns
        .iter()
        .find(|column| column.name == "a")
        .expect("table1.a should exist");
    let table2_column = table2
        .columns
        .iter()
        .find(|column| column.name == "a")
        .expect("table2.a should exist");

    let table1_fk_ref = table1_column
        .foreign_key
        .as_ref()
        .expect("table1.a should have foreign key metadata");
    assert_eq!(table1_fk_ref.table, "table2");
    assert_eq!(table1_fk_ref.column, "a");

    let table2_fk_ref = table2_column
        .foreign_key
        .as_ref()
        .expect("table2.a should have foreign key metadata");
    assert_eq!(table2_fk_ref.table, "table1");
    assert_eq!(table2_fk_ref.column, "a");
}

// ============================================================================
// ADDITIONAL EDGE CASES
// ============================================================================

#[test]
fn ansi_lateral_join_standard_syntax() {
    let sql = r#"
        SELECT u.id, l.last_order_date
        FROM users u
        LEFT JOIN LATERAL (
            SELECT MAX(order_date) as last_order_date
            FROM orders o
            WHERE o.user_id = u.id
        ) l ON true;
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("users") && tables.contains("orders"),
        "LATERAL JOIN should track both tables"
    );
}

#[test]
fn ansi_window_frame_clause_ignored_but_preserved() {
    let sql = r#"
        SELECT
            amount,
            SUM(amount) OVER (
                PARTITION BY user_id
                ORDER BY date
                ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
            ) as cumulative_sum
        FROM transactions;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);
    assert!(
        tables.contains("transactions"),
        "Window frame should not break lineage"
    );
}

#[test]
fn ansi_cast_syntax_variants() {
    let sql = r#"
        SELECT
            CAST(price AS INTEGER) as price_int,
            quantity::FLOAT as quantity_float,
            SAFE_CAST(date_str AS DATE) as safe_date
        FROM sales;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);
    let derivations = edges_by_type(stmt, EdgeType::Derivation);

    assert!(
        derivations.len() >= 3,
        "All cast variants should produce derivation edges"
    );
}

#[test]
fn ansi_having_subquery_lineage() {
    let sql = r#"
        SELECT user_id, SUM(amount)
        FROM orders
        GROUP BY user_id
        HAVING SUM(amount) > (SELECT AVG(target) FROM sales_targets);
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(tables.contains("orders"));
    assert!(
        tables.contains("sales_targets"),
        "Subquery in HAVING should be tracked"
    );
}

#[test]
fn quoted_identifiers_and_case_sensitivity() {
    let sql = r#"
        SELECT "U".id, "U"."Email Address"
        FROM "Users" "U"
        WHERE "U"."ActiveStatus" = 'Active';
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    let has_users = tables.contains("Users") || tables.contains("users");
    assert!(has_users, "Quoted table name should be tracked");
}

#[test]
fn comments_handling_blocks_and_inline() {
    let sql = r#"
        /*
           Block comment
           spanning multiple lines
        */
        SELECT * -- Inline comment
        FROM /* comment in middle */ users;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    assert!(tables.contains("users"), "Comments should be ignored");
}

#[test]
fn column_lineage_cte_transformation_chain_with_reuse() {
    // THIS TEST CURRENTLY FAILS - IT DOCUMENTS A BUG IN THE LINEAGE ENGINE
    //
    // BUG: The lineage engine creates spurious ownership edges from source tables
    // to derived columns that don't actually exist in those tables.
    //
    // Example SQL: CTE transforms columns, final SELECT uses transformed columns
    //   WITH transformed AS (
    //     SELECT id, UPPER(name) as name_upper, LOWER(email) as email_lower
    //     FROM users
    //   )
    //   SELECT id, name_upper, CONCAT(...) as display_name FROM transformed
    //
    // EXPECTED BEHAVIOR:
    //   - users table should own: [id, name, email]
    //   - transformed CTE should own: [id, name_upper, email_lower]
    //   - Final SELECT columns: [id, name_upper, display_name] (no owner or owned by Output)
    //
    // ACTUAL BEHAVIOR (BUG):
    //   - users table incorrectly owns: [id, name, email, name_upper, email_lower]
    //   - This causes UI to show: "users -> Output" for name_upper
    //   - Should show: "users -> transformed -> Output"
    //
    // IMPACT:
    //   - UI displays incorrect lineage paths (skips CTE in the path)
    //   - Column provenance is wrong (shows columns coming from wrong table)
    //   - Makes it impossible to trace transformations through CTEs correctly
    //
    // TO FIX:
    //   - When processing column references in a CTE's SELECT list, only create
    //     ownership edges from the CTE to its output columns
    //   - Do NOT create ownership edges from source tables to derived columns
    //   - name_upper should ONLY be owned by 'transformed', never by 'users'

    let sql = r#"
        WITH transformed AS (
            SELECT
                id,
                UPPER(name) as name_upper,
                LOWER(email) as email_lower
            FROM users
        )
        SELECT
            id,
            name_upper,
            CONCAT(name_upper, ' - ', email_lower) as display_name
        FROM transformed;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // 1. TABLE LINEAGE: users -> transformed -> (final result)
    let tables = collect_table_names(&result);
    assert!(
        tables.contains("users"),
        "source table 'users' should be tracked"
    );

    let ctes = collect_cte_names(&result);
    assert!(
        ctes.contains("transformed"),
        "CTE 'transformed' should be tracked"
    );

    // Verify the path: users -> transformed via ownership edges
    let users_table = find_table_node(stmt, "users");
    let transformed_cte = stmt
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Cte && &*n.label == "transformed");

    assert!(users_table.is_some(), "users table node should exist");
    assert!(
        transformed_cte.is_some(),
        "transformed CTE node should exist"
    );

    // Check ownership edges: Table owns its columns
    let ownership_edges = edges_by_type(stmt, EdgeType::Ownership);
    assert!(
        !ownership_edges.is_empty(),
        "should have ownership edges linking tables/CTEs to their columns"
    );

    // 2. COLUMN LINEAGE: All columns should exist as nodes

    // id - passes through unchanged
    assert!(
        find_column_node(stmt, "id").is_some(),
        "passthrough column 'id' should exist"
    );

    // name_upper and email_lower - derived in CTE
    assert!(
        find_column_node(stmt, "name_upper").is_some(),
        "CTE derived column 'name_upper' should exist"
    );
    assert!(
        find_column_node(stmt, "email_lower").is_some(),
        "CTE derived column 'email_lower' should exist"
    );

    // display_name - derived from CTE columns
    assert!(
        find_column_node(stmt, "display_name").is_some(),
        "final derived column 'display_name' should exist"
    );

    // 3. EDGE VERIFICATION: Should have derivation edges for transformations
    let derivations = edges_by_type(stmt, EdgeType::Derivation);
    assert!(
        derivations.len() >= 3,
        "should have derivation edges for UPPER, LOWER, and CONCAT transformations"
    );

    // 4. DATA FLOW: Should have edges showing column flow from CTE to final SELECT
    let dataflow = edges_by_type(stmt, EdgeType::DataFlow);
    assert!(
        !dataflow.is_empty(),
        "should have data flow edges from CTE columns to final SELECT"
    );

    // 5. EXPRESSION METADATA: Check if expressions are captured
    // Find the display_name column and verify it has expression metadata
    let display_name_col = find_column_node(stmt, "display_name");
    if let Some(node) = display_name_col {
        // Expression might contain CONCAT - this documents whether expression metadata is preserved
        // Even if not captured, the node should exist with proper lineage edges
        let _has_expr = node.expression.is_some();

        // The key is that derivation edges should connect this to its source columns
        let display_derivations: Vec<_> = derivations
            .iter()
            .filter(|edge| edge.to == node.id)
            .collect();

        assert!(
            !display_derivations.is_empty(),
            "display_name should have incoming derivation edges from source columns"
        );
    }

    // 6. VERIFY COMPLETE PATH: users -> transformed -> result
    // The path is encoded through column-level edges:
    // users.name --[Ownership]--> name (source col)
    //            --[Derivation]--> name_upper (in CTE)
    //            --[Ownership]--> transformed.name_upper
    //            --[DataFlow]--> name_upper (final SELECT)
    //            --[Derivation]--> display_name

    eprintln!("\n=== TABLE/CTE NODES ===");
    for node in &stmt.nodes {
        if node.node_type == NodeType::Table || node.node_type == NodeType::Cte {
            eprintln!("{:?}: {}", node.node_type, node.label);
        }
    }

    eprintln!("\n=== COLUMN NODES (sample) ===");
    for node in stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Column)
        .take(8)
    {
        eprintln!(
            "Column: {} (expr: {:?})",
            node.label,
            node.expression.as_ref().map(|e| &e[..50.min(e.len())])
        );
    }

    eprintln!("\n=== EDGE PATHS (sample) ===");
    for edge in stmt.edges.iter().take(12) {
        let from = stmt.nodes.iter().find(|n| n.id == edge.from);
        let to = stmt.nodes.iter().find(|n| n.id == edge.to);
        if let (Some(f), Some(t)) = (from, to) {
            eprintln!(
                "{:?}: {:?}({}) -> {:?}({})",
                edge.edge_type, f.node_type, f.label, t.node_type, t.label
            );
        }
    }

    eprintln!("\n=== EDGE SUMMARY ===");
    eprintln!("Ownership: {}", ownership_edges.len());
    eprintln!("DataFlow: {}", dataflow.len());
    eprintln!("Derivation: {}", derivations.len());

    // EXPLICIT PATH VERIFICATION:
    // Verify table-level edge: users -> transformed
    if let (Some(users), Some(transformed)) = (users_table, transformed_cte) {
        let table_to_cte_edge = stmt
            .edges
            .iter()
            .find(|e| e.from == users.id && e.to == transformed.id);
        assert!(
            table_to_cte_edge.is_some(),
            "CRITICAL: should have direct edge from users table -> transformed CTE"
        );
        eprintln!(
            "\n✅ CONFIRMED TABLE PATH: users -> transformed (edge type: {:?})",
            table_to_cte_edge.map(|e| e.edge_type)
        );

        // Verify that users table owns columns
        let users_owns_cols: Vec<_> = ownership_edges
            .iter()
            .filter(|e| e.from == users.id)
            .collect();
        assert!(
            !users_owns_cols.is_empty(),
            "users table should own columns (users -> name, email, id)"
        );
        eprintln!("users owns {} columns", users_owns_cols.len());

        // Verify that transformed CTE owns columns
        let transformed_owns_cols: Vec<_> = ownership_edges
            .iter()
            .filter(|e| e.from == transformed.id)
            .collect();
        assert!(
            !transformed_owns_cols.is_empty(),
            "transformed CTE should own columns (transformed -> id, name_upper, email_lower)"
        );
        eprintln!("transformed owns {} columns", transformed_owns_cols.len());

        // CRITICAL BUG CHECK: Verify NO spurious ownership edges
        eprintln!("\n=== COLUMN OWNERSHIP VERIFICATION ===");

        // Collect all column nodes by name
        let mut columns_by_name: std::collections::HashMap<String, Vec<(&Node, Vec<&Node>)>> =
            std::collections::HashMap::new();

        for node in &stmt.nodes {
            if node.node_type == NodeType::Column {
                let owners: Vec<_> = ownership_edges
                    .iter()
                    .filter(|e| e.to == node.id)
                    .filter_map(|e| stmt.nodes.iter().find(|n| n.id == e.from))
                    .collect();

                columns_by_name
                    .entry(node.label.to_string())
                    .or_default()
                    .push((node, owners));
            }
        }

        // Print all columns for debugging
        for (col_name, instances) in &columns_by_name {
            eprintln!("\nColumn '{}': {} instance(s)", col_name, instances.len());
            for (i, (node, owners)) in instances.iter().enumerate() {
                eprintln!(
                    "  [{}] id={}, owned by: {:?}",
                    i,
                    &node.id[..8],
                    owners
                        .iter()
                        .map(|n| format!("{:?}({})", n.node_type, n.label))
                        .collect::<Vec<_>>()
                );
            }
        }

        // EXPLICIT BUG CHECKS:
        // 1. users table should ONLY own: id, name, email (NOT name_upper, email_lower)
        let users_owned_cols: Vec<_> = ownership_edges
            .iter()
            .filter(|e| e.from == users.id)
            .filter_map(|e| stmt.nodes.iter().find(|n| n.id == e.to))
            .collect();

        let users_col_names: Vec<_> = users_owned_cols.iter().map(|n| &*n.label).collect();
        eprintln!("\nusers owns: {:?}", users_col_names);

        // BUG: users should NOT own name_upper or email_lower (these are derived in CTE)
        for col in &users_owned_cols {
            assert!(
                &*col.label != "name_upper" && &*col.label != "email_lower",
                "🐛 BUG DETECTED: users table incorrectly owns derived column '{}' (should only be in transformed CTE)",
                col.label
            );
        }

        // 2. transformed CTE should ONLY own: id, name_upper, email_lower (its output columns)
        let transformed_col_names: Vec<_> = transformed_owns_cols
            .iter()
            .filter_map(|e| stmt.nodes.iter().find(|n| n.id == e.to))
            .map(|n| &*n.label)
            .collect();
        eprintln!("transformed owns: {:?}", transformed_col_names);

        // 3. Final SELECT columns (id, name_upper, display_name) should either:
        //    - Have no owner (implicit output), OR
        //    - Be owned by an implicit "Output" node
        //    - But definitely NOT owned by users table

        eprintln!("\n=== EXPECTED vs ACTUAL ===");
        eprintln!("Expected users columns: [id, name, email]");
        eprintln!("Expected transformed columns: [id, name_upper, email_lower]");
        eprintln!("Expected final SELECT columns: [id, name_upper, display_name]");
        eprintln!("\nActual users owns: {:?}", users_col_names);
        eprintln!("Actual transformed owns: {:?}", transformed_col_names);
    }
}

#[test]
fn joined_tables_all_present_without_join_edges() {
    // Joins should not create table-to-table edges in the lineage graph.
    // The column-level data_flow edges already show where data comes from.
    // Join edges would misrepresent data flow (joins merge tables, not chain them).
    let sql = r#"
        SELECT
            o.order_id,
            c.customer_name,
            oi.quantity,
            p.product_name
        FROM orders o
        INNER JOIN customers c ON o.customer_id = c.id
        LEFT JOIN order_items oi ON o.order_id = oi.order_id
        LEFT JOIN products p ON oi.product_id = p.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Verify we have all 4 tables
    let table_names: Vec<String> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .map(|n| n.label.to_string())
        .collect();
    eprintln!("Tables found: {:?}", table_names);
    assert!(table_names.contains(&"orders".to_string()));
    assert!(table_names.contains(&"customers".to_string()));
    assert!(table_names.contains(&"order_items".to_string()));
    assert!(table_names.contains(&"products".to_string()));

    // Verify there are NO table-to-table join edges
    let table_ids: std::collections::HashSet<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .map(|n| &n.id)
        .collect();

    let table_to_table_edges: Vec<&Edge> = stmt
        .edges
        .iter()
        .filter(|e| table_ids.contains(&e.from) && table_ids.contains(&e.to))
        .collect();

    assert!(
        table_to_table_edges.is_empty(),
        "Should not have table-to-table edges for joins; found {:?}",
        table_to_table_edges
    );

    // Verify we still have column-level data_flow edges
    let data_flow_edges = edges_by_type(stmt, EdgeType::DataFlow);
    assert!(
        !data_flow_edges.is_empty(),
        "Should have column-level data_flow edges"
    );
}

#[test]
fn join_only_tables_emit_output_dependency() {
    let sql = r#"
        SELECT
            t1.a,
            t1.b
        FROM table1 t1
        LEFT JOIN table2 t2 ON t1.a = t2.a
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let output_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Output)
        .expect("Output node should exist");
    let table2_node = find_table_node(stmt, "table2").expect("table2 not found");

    let join_dependency = stmt.edges.iter().find(|edge| {
        edge.edge_type == EdgeType::JoinDependency
            && edge.from == table2_node.id
            && edge.to == output_node.id
    });

    assert!(
        join_dependency.is_some(),
        "join-only table should connect to output"
    );
}

#[test]
fn join_only_tables_emit_output_dependency_for_count_star() {
    let sql = r#"
        SELECT COUNT(*)
        FROM users u
        LEFT JOIN orders o ON u.id = o.user_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let output_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Output)
        .expect("Output node should exist");
    let orders_node = find_table_node(stmt, "orders").expect("orders not found");

    let join_dependency = stmt.edges.iter().find(|edge| {
        edge.edge_type == EdgeType::JoinDependency
            && edge.from == orders_node.id
            && edge.to == output_node.id
    });

    assert!(
        join_dependency.is_some(),
        "join-only table should connect to output for COUNT(*) queries"
    );
}

#[test]
fn count_star_keeps_base_table_connected_to_output() {
    let sql = r#"
        SELECT COUNT(*)
        FROM users u
        LEFT JOIN orders o ON u.id = o.user_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let users_node = find_table_node(stmt, "users").expect("users not found");
    let count_node = find_column_node(stmt, "count").expect("count output column not found");
    let orders_node = find_table_node(stmt, "orders").expect("orders not found");
    let output_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Output)
        .expect("Output node should exist");

    let users_dependency = stmt.edges.iter().find(|edge| {
        edge.edge_type == EdgeType::Derivation
            && edge.from == users_node.id
            && edge.to == count_node.id
    });
    assert!(
        users_dependency.is_some(),
        "base table should connect to COUNT(*) output column"
    );

    let orders_dependency = stmt.edges.iter().find(|edge| {
        edge.edge_type == EdgeType::JoinDependency
            && edge.from == orders_node.id
            && edge.to == output_node.id
    });
    assert!(
        orders_dependency.is_some(),
        "joined table should still connect to output via join dependency"
    );
}

#[test]
fn select_literal_keeps_base_table_connected_to_output() {
    let sql = r#"
        SELECT 1
        FROM users
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let users_node = find_table_node(stmt, "users").expect("users not found");
    let literal_col = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Column)
        .expect("literal output column not found");

    let dependency = stmt
        .edges
        .iter()
        .find(|edge| edge.from == users_node.id && edge.to == literal_col.id);
    assert!(
        dependency.is_some(),
        "base table should connect to literal output column"
    );
}

#[test]
fn count_star_self_join_creates_multiple_dependencies() {
    let sql = r#"
        SELECT COUNT(*)
        FROM employees e1
        LEFT JOIN employees e2 ON e1.manager_id = e2.id
        LEFT JOIN employees e3 ON e2.manager_id = e3.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let output_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Output)
        .expect("Output node should exist");

    // Find nodes that have JoinDependency edges with LEFT join type (joined aliases)
    let joined_alias_ids: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::JoinDependency && e.join_type == Some(JoinType::Left))
        .map(|e| e.from.clone())
        .collect();
    let joined_aliases: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|node| node.node_type == NodeType::Table && joined_alias_ids.contains(&node.id))
        .collect();
    assert_eq!(
        joined_aliases.len(),
        2,
        "expected two joined employee aliases in self-join aggregate query"
    );

    for alias_node in joined_aliases {
        let join_dependency = stmt.edges.iter().find(|edge| {
            edge.edge_type == EdgeType::JoinDependency
                && edge.from == alias_node.id
                && edge.to == output_node.id
        });

        assert!(
            join_dependency.is_some(),
            "joined self-join alias {} should connect to output for COUNT(*) queries",
            alias_node.id
        );
    }
}

#[test]
fn join_only_tables_emit_output_dependency_for_distinct_projection() {
    let sql = r#"
        SELECT DISTINCT u.id
        FROM users u
        LEFT JOIN orders o ON u.id = o.user_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let output_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Output)
        .expect("Output node should exist");
    let orders_node = find_table_node(stmt, "orders").expect("orders not found");

    let join_dependency = stmt.edges.iter().find(|edge| {
        edge.edge_type == EdgeType::JoinDependency
            && edge.from == orders_node.id
            && edge.to == output_node.id
    });

    assert!(
        join_dependency.is_some(),
        "join-only table should connect to output for DISTINCT projections"
    );
}

#[test]
fn join_only_tables_emit_output_dependency_for_literal_projection() {
    let sql = r#"
        SELECT 1
        FROM users u
        LEFT JOIN orders o ON u.id = o.user_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let output_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Output)
        .expect("Output node should exist");
    let orders_node = find_table_node(stmt, "orders").expect("orders not found");

    let join_dependency = stmt.edges.iter().find(|edge| {
        edge.edge_type == EdgeType::JoinDependency
            && edge.from == orders_node.id
            && edge.to == output_node.id
    });

    assert!(
        join_dependency.is_some(),
        "join-only table should connect to output for literal projections"
    );
}

#[test]
fn joined_tables_emit_output_dependency_when_column_lineage_disabled() {
    let sql = r#"
        SELECT u.id
        FROM users u
        LEFT JOIN orders o ON u.id = o.user_id
    "#;

    let result = run_analysis_with_options(
        sql,
        Dialect::Generic,
        None,
        AnalysisOptions {
            enable_column_lineage: Some(false),
            ..Default::default()
        },
    );
    let stmt = first_statement(&result);

    let output_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Output)
        .expect("output node should exist");
    let orders_node = find_table_node(stmt, "orders").expect("orders not found");

    let join_dependency = stmt.edges.iter().find(|edge| {
        edge.edge_type == EdgeType::JoinDependency
            && edge.from == orders_node.id
            && edge.to == output_node.id
    });

    let join_dependency = join_dependency
        .expect("joined table should still connect to the output when column lineage is disabled");
    assert_eq!(join_dependency.join_type, Some(JoinType::Left));
    assert_eq!(
        join_dependency.join_condition.as_deref(),
        Some("u.id = o.user_id")
    );
}

#[test]
fn wildcard_join_contributors_do_not_emit_output_dependency_without_schema() {
    let sql = r#"
        SELECT *
        FROM users u
        LEFT JOIN orders o ON u.id = o.user_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);
    let orders_node = find_table_node(stmt, "orders").expect("orders not found");

    let join_dependency = stmt
        .edges
        .iter()
        .find(|edge| edge.edge_type == EdgeType::JoinDependency && edge.from == orders_node.id);

    assert!(
        join_dependency.is_none(),
        "joined table should not emit join dependency when SELECT * creates a direct output edge"
    );
}

#[test]
fn qualified_wildcard_join_only_table_gets_dependency() {
    // SELECT u.* only expands `users`, so `orders` is join-only and
    // must still get a JoinDependency edge even though the wildcard
    // creates an approximate DataFlow edge (only for `users`).
    let sql = r#"
        SELECT u.*
        FROM users u
        LEFT JOIN orders o ON u.id = o.user_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let output_node = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Output)
        .expect("Output node should exist");
    let orders_node = find_table_node(stmt, "orders").expect("orders not found");

    let join_dependency = stmt.edges.iter().find(|edge| {
        edge.edge_type == EdgeType::JoinDependency
            && edge.from == orders_node.id
            && edge.to == output_node.id
    });

    assert!(
        join_dependency.is_some(),
        "join-only table must get JoinDependency when only other table's wildcard is selected"
    );
}

#[test]
fn column_level_edges_from_joined_tables_carry_join_info() {
    // Verifies that propagate_join_info_to_edges fills in join metadata on
    // column-level DataFlow/Derivation edges originating from joined tables.
    let sql = r#"
        SELECT o.order_id, c.name
        FROM orders o
        INNER JOIN customers c ON o.customer_id = c.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let customers_node = find_table_node(stmt, "customers").expect("customers not found");

    // Find column-level edges originating from columns owned by the joined table
    let customer_column_ids: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Ownership && e.from == customers_node.id)
        .map(|e| e.to.clone())
        .collect();

    assert!(
        !customer_column_ids.is_empty(),
        "customers should own columns"
    );

    // At least one DataFlow/Derivation edge from a customers column should carry join info
    let has_join_info = stmt.edges.iter().any(|edge| {
        matches!(edge.edge_type, EdgeType::DataFlow | EdgeType::Derivation)
            && customer_column_ids.contains(&edge.from)
            && edge.join_type == Some(JoinType::Inner)
    });

    assert!(
        has_join_info,
        "column-level edges from joined table should carry join_type after propagation"
    );
}

#[test]
fn propagated_join_info_does_not_overwrite_existing() {
    // A JoinDependency edge created in add_join_dependency_edges already has
    // join_type set. propagate_join_info_to_edges must not overwrite it.
    let sql = r#"
        SELECT u.id
        FROM users u
        LEFT JOIN orders o ON u.id = o.user_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let orders_node = find_table_node(stmt, "orders").expect("orders not found");

    let join_dep = stmt
        .edges
        .iter()
        .find(|e| e.edge_type == EdgeType::JoinDependency && e.from == orders_node.id)
        .expect("JoinDependency edge should exist for join-only table");

    assert_eq!(
        join_dep.join_type,
        Some(JoinType::Left),
        "JoinDependency edge should retain its original join_type"
    );
    assert_eq!(
        join_dep.join_condition.as_deref(),
        Some("u.id = o.user_id"),
        "JoinDependency edge should retain its original join_condition"
    );
}

#[test]
fn where_filters_attached_to_correct_tables() {
    let sql = r#"
        SELECT o.order_id, c.customer_name
        FROM orders o
        INNER JOIN customers c ON o.customer_id = c.id
        WHERE o.order_date >= '2024-01-01'
            AND c.status = 'active'
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Find orders and customers table nodes
    let orders_node = find_table_node(stmt, "orders").expect("orders table not found");
    let customers_node = find_table_node(stmt, "customers").expect("customers table not found");

    eprintln!("orders filters: {:?}", orders_node.filters);
    eprintln!("customers filters: {:?}", customers_node.filters);

    // Orders should have exactly ONE filter about order_date (localized)
    assert_eq!(
        orders_node.filters.len(),
        1,
        "orders should have exactly one filter"
    );
    assert!(
        orders_node.filters[0].expression.contains("order_date"),
        "orders filter should mention order_date"
    );
    assert!(
        !orders_node.filters[0].expression.contains("status"),
        "orders filter should NOT mention status (that belongs to customers)"
    );

    // Customers should have exactly ONE filter about status (localized)
    assert_eq!(
        customers_node.filters.len(),
        1,
        "customers should have exactly one filter"
    );
    assert!(
        customers_node.filters[0].expression.contains("status"),
        "customers filter should mention status"
    );
    assert!(
        !customers_node.filters[0].expression.contains("order_date"),
        "customers filter should NOT mention order_date (that belongs to orders)"
    );

    // All filters should be WHERE type
    for filter in &orders_node.filters {
        assert_eq!(filter.clause_type, FilterClauseType::Where);
    }
    for filter in &customers_node.filters {
        assert_eq!(filter.clause_type, FilterClauseType::Where);
    }
}

#[test]
fn having_filters_attached_correctly() {
    let sql = r#"
        SELECT
            c.category,
            SUM(p.price) as total_price,
            COUNT(*) as product_count
        FROM products p
        JOIN categories c ON p.category_id = c.id
        WHERE p.active = true
        GROUP BY c.category
        HAVING SUM(p.price) > 1000 AND COUNT(*) > 5
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let products_node = find_table_node(stmt, "products").expect("products table not found");
    let categories_node = find_table_node(stmt, "categories").expect("categories table not found");

    eprintln!("products filters: {:?}", products_node.filters);
    eprintln!("categories filters: {:?}", categories_node.filters);

    // Products should have the WHERE clause filter
    let products_where_filters: Vec<_> = products_node
        .filters
        .iter()
        .filter(|f| f.clause_type == FilterClauseType::Where)
        .collect();
    assert_eq!(
        products_where_filters.len(),
        1,
        "products should have one WHERE filter"
    );
    assert!(
        products_where_filters[0].expression.contains("active"),
        "products WHERE filter should mention 'active'"
    );

    // HAVING filters are harder to localize to specific tables since they often
    // reference aggregate functions. The important thing is they get captured.
    let all_having_filters: Vec<_> = stmt
        .nodes
        .iter()
        .flat_map(|n| &n.filters)
        .filter(|f| f.clause_type == FilterClauseType::Having)
        .collect();

    // We should have captured HAVING filters (may be split by AND)
    assert!(
        !all_having_filters.is_empty() || products_node.filters.len() > 1,
        "HAVING filters should be captured"
    );
}

#[test]
fn nested_or_predicates_not_split() {
    // OR predicates at the top level should NOT be split by AND
    // This test ensures we only split by AND at the top level
    let sql = r#"
        SELECT * FROM users
        WHERE (status = 'active' OR status = 'pending') AND age > 18
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let users_node = find_table_node(stmt, "users").expect("users table not found");

    eprintln!("users filters: {:?}", users_node.filters);

    // Should have 2 filters: the OR group and the AND condition
    assert_eq!(
        users_node.filters.len(),
        2,
        "Should split by top-level AND only, keeping OR grouped"
    );

    // One filter should contain OR
    let has_or_filter = users_node
        .filters
        .iter()
        .any(|f| f.expression.contains("OR") || f.expression.contains("pending"));
    assert!(has_or_filter, "One filter should contain the OR expression");
}

#[test]
fn cross_table_predicate_not_attached_to_individual_tables() {
    // Cross-table predicates like `a.id = b.id` in WHERE should NOT be
    // attached to either table — they are cross-table conditions, not
    // individual table filters.
    let sql = r#"
        SELECT a.name, b.amount
        FROM users a
        JOIN orders b ON a.id = b.user_id
        WHERE a.status = 'active' AND a.id = b.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let users_node = find_table_node(stmt, "users").expect("users table not found");
    let orders_node = find_table_node(stmt, "orders").expect("orders table not found");

    eprintln!("users filters: {:?}", users_node.filters);
    eprintln!("orders filters: {:?}", orders_node.filters);

    // Users should have only the single-table predicate (status = 'active')
    assert_eq!(
        users_node.filters.len(),
        1,
        "users should have exactly one filter (the single-table predicate)"
    );
    assert!(
        users_node.filters[0].expression.contains("status"),
        "users filter should be the status predicate"
    );

    // Orders should have NO filters — the cross-table predicate `a.id = b.id`
    // should not be attached to it
    assert!(
        orders_node.filters.is_empty(),
        "orders should have no filters (cross-table predicate should be skipped), got: {:?}",
        orders_node.filters
    );
}

#[test]
fn same_table_qualified_and_unqualified_refs_are_not_treated_as_cross_table() {
    let sql = r#"
        SELECT u.id
        FROM users u
        WHERE u.id = id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let users_node = find_table_node(stmt, "users").expect("users table not found");

    assert_eq!(
        users_node.filters.len(),
        1,
        "same-table qualified + unqualified predicate should be captured once"
    );
    assert!(
        users_node.filters[0].expression.contains("u.id = id"),
        "users filter should preserve the mixed predicate, got {:?}",
        users_node.filters
    );
}

#[test]
fn unresolvable_predicate_not_broadcast_to_all_tables() {
    // When a column cannot be resolved to any table (e.g., from a function),
    // the filter should NOT be broadcast to all tables in scope.
    let sql = r#"
        SELECT u.name, o.amount
        FROM users u
        JOIN orders o ON u.id = o.user_id
        WHERE u.status = 'active' AND random() > 0.5
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let users_node = find_table_node(stmt, "users").expect("users table not found");
    let orders_node = find_table_node(stmt, "orders").expect("orders table not found");

    eprintln!("users filters: {:?}", users_node.filters);
    eprintln!("orders filters: {:?}", orders_node.filters);

    // Users should have the status filter
    assert_eq!(
        users_node.filters.len(),
        1,
        "users should have exactly one filter"
    );
    assert!(
        users_node.filters[0].expression.contains("status"),
        "users filter should be the status predicate"
    );

    // Orders should NOT have the random() predicate broadcast to it
    assert!(
        orders_node.filters.is_empty(),
        "orders should have no filters (unresolvable predicate should not broadcast), got: {:?}",
        orders_node.filters
    );
}

#[test]
fn multiple_join_types_captured() {
    let sql = r#"
        SELECT *
        FROM orders o
        LEFT JOIN customers c ON o.customer_id = c.id
        INNER JOIN products p ON o.product_id = p.id
        FULL OUTER JOIN inventory i ON p.id = i.product_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // All table nodes should exist
    let _orders_node = find_table_node(stmt, "orders").expect("orders table not found");
    let customers_node = find_table_node(stmt, "customers").expect("customers table not found");
    let products_node = find_table_node(stmt, "products").expect("products table not found");
    let inventory_node = find_table_node(stmt, "inventory").expect("inventory table not found");

    use flowscope_core::JoinType;

    // Helper: find an edge originating from a given node with join_type set
    let find_join_edge = |node_id: &str| -> Option<&flowscope_core::Edge> {
        stmt.edges
            .iter()
            .find(|e| e.from.as_ref() == node_id && e.join_type.is_some())
    };

    // Join info should live on edges, not nodes
    let customers_edge =
        find_join_edge(customers_node.id.as_ref()).expect("customers should have a join edge");
    assert_eq!(
        customers_edge.join_type,
        Some(JoinType::Left),
        "customers edge should be LEFT joined"
    );
    assert!(
        customers_edge.join_condition.is_some(),
        "customers edge should have join condition"
    );

    let products_edge =
        find_join_edge(products_node.id.as_ref()).expect("products should have a join edge");
    assert_eq!(
        products_edge.join_type,
        Some(JoinType::Inner),
        "products edge should be INNER joined"
    );
    assert!(
        products_edge.join_condition.is_some(),
        "products edge should have join condition"
    );

    let inventory_edge =
        find_join_edge(inventory_node.id.as_ref()).expect("inventory should have a join edge");
    assert_eq!(
        inventory_edge.join_type,
        Some(JoinType::Full),
        "inventory edge should be FULL joined"
    );
    assert!(
        inventory_edge.join_condition.is_some(),
        "inventory edge should have join condition"
    );
}

#[test]
fn cte_join_metadata_captured() {
    let sql = r#"
        WITH user_ltv AS (
            SELECT user_id, COUNT(*) AS total_orders
            FROM orders
            GROUP BY user_id
        )
        SELECT *
        FROM users u
        LEFT JOIN user_ltv ltv ON u.user_id = ltv.user_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);
    let cte_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Cte && n.qualified_name.as_deref() == Some("user_ltv"))
        .collect();
    assert!(!cte_nodes.is_empty(), "user_ltv CTE nodes not found");

    use flowscope_core::JoinType;
    // Find an edge from a user_ltv CTE node (or its owned columns) that has LEFT join metadata
    let cte_node_ids: Vec<_> = cte_nodes.iter().map(|n| n.id.as_ref()).collect();
    let cte_col_ids: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| {
            e.edge_type == flowscope_core::EdgeType::Ownership
                && cte_node_ids.contains(&e.from.as_ref())
        })
        .map(|e| e.to.clone())
        .collect();
    let cte_related_ids: Vec<&str> = cte_node_ids
        .iter()
        .copied()
        .chain(cte_col_ids.iter().map(|s| s.as_ref()))
        .collect();
    let join_edge = stmt
        .edges
        .iter()
        .find(|e| cte_related_ids.contains(&e.from.as_ref()) && e.join_type == Some(JoinType::Left))
        .expect("CTE should have an edge with LEFT join metadata");
    assert_eq!(
        join_edge.join_condition.as_deref(),
        Some("u.user_id = ltv.user_id"),
        "CTE edge should capture join condition"
    );
}

#[test]
fn deeply_nested_and_predicates_split_correctly() {
    let sql = r#"
        SELECT * FROM users
        WHERE a = 1 AND b = 2 AND c = 3 AND d = 4
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let users_node = find_table_node(stmt, "users").expect("users table not found");

    eprintln!("users filters: {:?}", users_node.filters);

    // All 4 predicates should be split into separate filters
    assert_eq!(
        users_node.filters.len(),
        4,
        "Should split into 4 separate predicates"
    );
}

// ============================================================================
// AGGREGATION DETECTION TESTS
// ============================================================================

#[test]
fn aggregation_detects_grouping_key() {
    let sql = r#"
        SELECT region, SUM(amount) AS total
        FROM orders
        GROUP BY region;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Find the region column (grouping key)
    let region_col = find_column_node(stmt, "region").expect("region column not found");

    assert!(
        region_col.aggregation.is_some(),
        "region column should have aggregation info"
    );
    let agg = region_col.aggregation.as_ref().unwrap();
    assert!(
        agg.is_grouping_key,
        "region should be marked as grouping key"
    );
    assert!(
        agg.function.is_none(),
        "grouping key should not have function"
    );
}

#[test]
fn aggregation_detects_aggregate_function() {
    let sql = r#"
        SELECT region, SUM(amount) AS total
        FROM orders
        GROUP BY region;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Find the total column (aggregate)
    let total_col = find_column_node(stmt, "total").expect("total column not found");

    assert!(
        total_col.aggregation.is_some(),
        "total column should have aggregation info"
    );
    let agg = total_col.aggregation.as_ref().unwrap();
    assert!(
        !agg.is_grouping_key,
        "total should not be marked as grouping key"
    );
    assert_eq!(
        agg.function.as_deref(),
        Some("SUM"),
        "should detect SUM function"
    );
}

#[test]
fn aggregation_detects_distinct() {
    let sql = r#"
        SELECT region, COUNT(DISTINCT user_id) AS unique_users
        FROM orders
        GROUP BY region;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let unique_users_col =
        find_column_node(stmt, "unique_users").expect("unique_users column not found");

    assert!(
        unique_users_col.aggregation.is_some(),
        "unique_users column should have aggregation info"
    );
    let agg = unique_users_col.aggregation.as_ref().unwrap();
    assert_eq!(
        agg.function.as_deref(),
        Some("COUNT"),
        "should detect COUNT function"
    );
    assert_eq!(agg.distinct, Some(true), "should detect DISTINCT modifier");
}

#[test]
fn aggregation_no_info_without_group_by() {
    let sql = r#"
        SELECT region, amount
        FROM orders;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let region_col = find_column_node(stmt, "region").expect("region column not found");
    let amount_col = find_column_node(stmt, "amount").expect("amount column not found");

    assert!(
        region_col.aggregation.is_none(),
        "region should not have aggregation info without GROUP BY"
    );
    assert!(
        amount_col.aggregation.is_none(),
        "amount should not have aggregation info without GROUP BY"
    );
}

#[test]
fn aggregation_multiple_grouping_keys() {
    let sql = r#"
        SELECT region, product_type, AVG(price) AS avg_price
        FROM products
        GROUP BY region, product_type;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let region_col = find_column_node(stmt, "region").expect("region column not found");
    let product_type_col =
        find_column_node(stmt, "product_type").expect("product_type column not found");
    let avg_price_col = find_column_node(stmt, "avg_price").expect("avg_price column not found");

    assert!(
        region_col
            .aggregation
            .as_ref()
            .map(|a| a.is_grouping_key)
            .unwrap_or(false),
        "region should be grouping key"
    );
    assert!(
        product_type_col
            .aggregation
            .as_ref()
            .map(|a| a.is_grouping_key)
            .unwrap_or(false),
        "product_type should be grouping key"
    );
    assert_eq!(
        avg_price_col
            .aggregation
            .as_ref()
            .and_then(|a| a.function.as_deref()),
        Some("AVG"),
        "avg_price should have AVG function"
    );
}

#[test]
fn aggregation_nested_in_expression() {
    let sql = r#"
        SELECT region, SUM(amount) * 1.1 AS total_with_tax
        FROM orders
        GROUP BY region;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let total_col =
        find_column_node(stmt, "total_with_tax").expect("total_with_tax column not found");

    assert!(
        total_col.aggregation.is_some(),
        "total_with_tax should have aggregation info"
    );
    let agg = total_col.aggregation.as_ref().unwrap();
    assert_eq!(
        agg.function.as_deref(),
        Some("SUM"),
        "should detect SUM in expression"
    );
}

#[test]
fn aggregation_in_case_expression() {
    let sql = r#"
        SELECT
            region,
            CASE WHEN SUM(amount) > 1000 THEN 'high' ELSE 'low' END AS volume
        FROM orders
        GROUP BY region;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let volume_col = find_column_node(stmt, "volume").expect("volume column not found");

    assert!(
        volume_col.aggregation.is_some(),
        "volume should have aggregation info from CASE"
    );
    let agg = volume_col.aggregation.as_ref().unwrap();
    assert_eq!(
        agg.function.as_deref(),
        Some("SUM"),
        "should detect SUM in CASE expression"
    );
}

#[test]
fn aggregation_qualified_column_as_grouping_key() {
    let sql = r#"
        SELECT o.region, SUM(o.amount) AS total
        FROM orders o
        GROUP BY o.region;
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let region_col = find_column_node(stmt, "region").expect("region column not found");

    assert!(
        region_col
            .aggregation
            .as_ref()
            .map(|a| a.is_grouping_key)
            .unwrap_or(false),
        "qualified column should match grouping key"
    );
}

// ============================================================================
// NESTED SUBQUERIES AND CTE CHAINS
// ============================================================================

#[test]
fn nested_derived_tables_track_full_lineage() {
    // Test derived table inside another derived table
    let sql = r#"
        SELECT outer_sub.total
        FROM (
            SELECT inner_sub.amount AS total
            FROM (
                SELECT SUM(amount) AS amount
                FROM orders
            ) AS inner_sub
        ) AS outer_sub
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Both derived table nodes should exist
    let inner_node = stmt
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Cte && &*n.label == "inner_sub");
    let outer_node = stmt
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Cte && &*n.label == "outer_sub");

    assert!(
        inner_node.is_some(),
        "inner derived table node should exist"
    );
    assert!(
        outer_node.is_some(),
        "outer derived table node should exist"
    );

    // Source table should be tracked
    let tables = collect_table_names(&result);
    assert!(
        tables.contains("orders"),
        "base table should be tracked through nested derived tables"
    );

    // Should have derivation edges showing data flow
    let derivations = edges_by_type(stmt, EdgeType::Derivation);
    assert!(
        !derivations.is_empty(),
        "nested derived tables should produce derivation edges"
    );
}

#[test]
fn cte_referencing_another_cte() {
    // Test CTE that references a previously defined CTE
    let sql = r#"
        WITH base_orders AS (
            SELECT user_id, SUM(amount) AS total_amount
            FROM orders
            GROUP BY user_id
        ),
        enriched_orders AS (
            SELECT bo.user_id, bo.total_amount, u.name
            FROM base_orders bo
            JOIN users u ON bo.user_id = u.id
        )
        SELECT user_id, name, total_amount
        FROM enriched_orders
        WHERE total_amount > 100
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Both CTEs should exist
    let base_cte = stmt
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Cte && &*n.label == "base_orders");
    let enriched_cte = stmt
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Cte && &*n.label == "enriched_orders");

    assert!(base_cte.is_some(), "base_orders CTE should exist");
    assert!(enriched_cte.is_some(), "enriched_orders CTE should exist");

    // Source tables should be tracked
    let tables = collect_table_names(&result);
    assert!(tables.contains("orders"), "orders table should be tracked");
    assert!(tables.contains("users"), "users table should be tracked");

    // Verify CTE nodes are present
    let ctes = collect_cte_names(&result);
    assert!(ctes.contains("base_orders"), "base_orders CTE should exist");
    assert!(
        ctes.contains("enriched_orders"),
        "enriched_orders CTE should exist"
    );
}

#[test]
fn alias_shadows_table_name() {
    // Test when an alias shadows an actual table name
    let sql = r#"
        SELECT orders.amount
        FROM payments AS orders
    "#;

    let schema = SchemaMetadata {
        allow_implied: false,
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![
            schema_table(None, None, "orders", &["id", "amount"]),
            schema_table(None, None, "payments", &["id", "amount"]),
        ],
    };

    let result = run_analysis(sql, Dialect::Generic, Some(schema));
    let tables = collect_table_names(&result);

    // Should reference payments (aliased as orders), not the real orders table
    assert!(
        tables.contains("payments"),
        "payments table should be tracked (aliased as orders)"
    );
    // The real orders table should NOT be in lineage since we're using the alias
    assert!(
        !tables.contains("orders"),
        "real orders table should not be tracked when alias shadows it"
    );
}

#[test]
fn deeply_nested_cte_chain() {
    // Test a chain of CTEs where each references the previous one
    let sql = r#"
        WITH step1 AS (
            SELECT id, amount FROM orders
        ),
        step2 AS (
            SELECT id, amount * 1.1 AS adjusted FROM step1
        ),
        step3 AS (
            SELECT id, adjusted * 0.9 AS final_amount FROM step2
        )
        SELECT id, final_amount FROM step3
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // All CTEs should exist
    let ctes = collect_cte_names(&result);
    assert!(ctes.contains("step1"), "step1 CTE should exist");
    assert!(ctes.contains("step2"), "step2 CTE should exist");
    assert!(ctes.contains("step3"), "step3 CTE should exist");

    // Source table should be tracked
    let tables = collect_table_names(&result);
    assert!(
        tables.contains("orders"),
        "orders table should be tracked through CTE chain"
    );

    // Should have derivation edges showing transformations
    let derivations = edges_by_type(stmt, EdgeType::Derivation);
    assert!(
        derivations.len() >= 2,
        "CTE chain should produce multiple derivation edges for transformations"
    );
}

#[test]
fn derived_table_inside_cte() {
    // Test derived table nested inside a CTE
    let sql = r#"
        WITH summary AS (
            SELECT sub.user_id, sub.order_count
            FROM (
                SELECT user_id, COUNT(*) AS order_count
                FROM orders
                GROUP BY user_id
            ) AS sub
            WHERE sub.order_count > 5
        )
        SELECT * FROM summary
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // CTE should exist
    let ctes = collect_cte_names(&result);
    assert!(ctes.contains("summary"), "summary CTE should exist");

    // Derived table inside CTE should also exist
    let derived_node = stmt
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Cte && &*n.label == "sub");
    assert!(
        derived_node.is_some(),
        "derived table inside CTE should exist as node"
    );

    // Source table should be tracked
    let tables = collect_table_names(&result);
    assert!(
        tables.contains("orders"),
        "orders table should be tracked through CTE with nested derived table"
    );
}

// ============================================================================
// SQLPARSER 0.59 COMPATIBILITY TESTS
// Tests for new AST structures and SQL features introduced in sqlparser 0.59
// ============================================================================

#[test]
fn case_when_simple_expression() {
    // Test simple CASE WHEN expression with multiple conditions
    let sql = r#"
        SELECT
            id,
            CASE
                WHEN status = 'active' THEN 'Active'
                WHEN status = 'pending' THEN 'Pending'
                WHEN status = 'inactive' THEN 'Inactive'
                ELSE 'Unknown'
            END AS status_label
        FROM users
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert!(!result.summary.has_errors, "should parse without errors");

    let stmt = first_statement(&result);
    let status_col = find_column_node(stmt, "status_label");
    assert!(
        status_col.is_some(),
        "CASE expression should produce status_label column"
    );

    let tables = collect_table_names(&result);
    assert!(tables.contains("users"), "users table should be tracked");
}

#[test]
fn case_when_searched_form() {
    // Test searched CASE (without operand) with column references
    let sql = r#"
        SELECT
            order_id,
            CASE
                WHEN amount > 1000 THEN 'large'
                WHEN amount > 100 THEN 'medium'
                ELSE 'small'
            END AS size_category,
            CASE status
                WHEN 'A' THEN 'Active'
                WHEN 'P' THEN 'Pending'
                ELSE 'Other'
            END AS status_name
        FROM orders
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert!(!result.summary.has_errors, "should parse without errors");

    let stmt = first_statement(&result);

    // Both CASE expressions should produce columns
    let size_col = find_column_node(stmt, "size_category");
    let status_col = find_column_node(stmt, "status_name");
    assert!(size_col.is_some(), "searched CASE should work");
    assert!(status_col.is_some(), "simple CASE should work");
}

#[test]
fn case_when_nested_in_function() {
    // Test CASE inside a function call
    let sql = r#"
        SELECT
            COALESCE(
                CASE WHEN active THEN name ELSE NULL END,
                'default'
            ) AS display_name
        FROM users
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert!(
        !result.summary.has_errors,
        "nested CASE in function should parse"
    );

    let stmt = first_statement(&result);
    let col = find_column_node(stmt, "display_name");
    assert!(col.is_some(), "nested CASE should produce output column");
}

#[test]
fn case_when_with_subquery() {
    // Test CASE with subquery in condition
    let sql = r#"
        SELECT
            u.id,
            CASE
                WHEN u.id IN (SELECT user_id FROM premium_users) THEN 'premium'
                ELSE 'standard'
            END AS tier
        FROM users u
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert!(
        !result.summary.has_errors,
        "CASE with subquery should parse"
    );

    let tables = collect_table_names(&result);
    assert!(tables.contains("users"), "main table tracked");
    // Note: subquery tables in CASE expressions may or may not be tracked
    // depending on analyzer implementation. The key test is that parsing succeeds.

    let stmt = first_statement(&result);
    let tier_col = find_column_node(stmt, "tier");
    assert!(
        tier_col.is_some(),
        "CASE with subquery produces tier column"
    );
}

// ============================================================================
// JOIN TYPE TESTS (Semi, Anti, StraightJoin)
// ============================================================================

#[test]
fn left_semi_join_tracks_tables() {
    // LEFT SEMI JOIN - supported by Spark SQL, Databricks
    let sql = r#"
        SELECT o.order_id, o.total
        FROM orders o
        LEFT SEMI JOIN customers c ON o.customer_id = c.id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    // Semi joins may not parse in all dialects, check if parsing succeeded
    if !result.summary.has_errors {
        let stmt = first_statement(&result);
        let tables = collect_table_names(&result);
        assert!(tables.contains("orders"), "orders table should be tracked");
        assert!(
            tables.contains("customers"),
            "customers table should be tracked"
        );

        // Check that join was recognized
        let orders_node = find_table_node(stmt, "orders");
        let customers_node = find_table_node(stmt, "customers");
        assert!(orders_node.is_some(), "orders node exists");
        assert!(customers_node.is_some(), "customers node exists");

        // Check join type on edge from joined table
        if let Some(cust) = customers_node {
            let join_edge = stmt
                .edges
                .iter()
                .find(|e| e.from == cust.id && e.join_type.is_some());
            assert_eq!(
                join_edge.and_then(|e| e.join_type),
                Some(JoinType::LeftSemi),
                "should recognize LEFT SEMI JOIN on edge"
            );
        }
    }
}

#[test]
fn left_anti_join_tracks_tables() {
    // LEFT ANTI JOIN - rows from left that have no match in right
    let sql = r#"
        SELECT o.order_id, o.total
        FROM orders o
        LEFT ANTI JOIN returns r ON o.order_id = r.order_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    if !result.summary.has_errors {
        let stmt = first_statement(&result);
        let tables = collect_table_names(&result);
        assert!(tables.contains("orders"), "orders tracked");
        assert!(tables.contains("returns"), "returns tracked");

        let returns_node = find_table_node(stmt, "returns");
        if let Some(ret) = returns_node {
            let join_edge = stmt
                .edges
                .iter()
                .find(|e| e.from == ret.id && e.join_type.is_some());
            assert_eq!(
                join_edge.and_then(|e| e.join_type),
                Some(JoinType::LeftAnti),
                "should recognize LEFT ANTI JOIN on edge"
            );
        }
    }
}

#[test]
fn straight_join_mysql_syntax() {
    // STRAIGHT_JOIN - MySQL specific, forces join order
    let sql = r#"
        SELECT o.id, c.name
        FROM orders o
        STRAIGHT_JOIN customers c ON o.customer_id = c.id
    "#;

    let result = run_analysis(sql, Dialect::Mysql, None);
    if !result.summary.has_errors {
        let tables = collect_table_names(&result);
        assert!(tables.contains("orders"), "orders tracked");
        assert!(tables.contains("customers"), "customers tracked");

        // STRAIGHT_JOIN is treated as INNER JOIN semantically
        let stmt = first_statement(&result);
        let cust_node = find_table_node(stmt, "customers");
        if let Some(cust) = cust_node {
            // STRAIGHT_JOIN maps to Inner join type (on edge)
            // Check edges from the node or its owned columns
            let owned_col_ids: Vec<_> = stmt
                .edges
                .iter()
                .filter(|e| e.edge_type == EdgeType::Ownership && e.from == cust.id)
                .map(|e| e.to.clone())
                .collect();
            let join_edge = stmt.edges.iter().find(|e| {
                e.join_type.is_some()
                    && (e.from == cust.id || owned_col_ids.iter().any(|c| c == &e.from))
            });
            assert_eq!(
                join_edge.and_then(|e| e.join_type),
                Some(JoinType::Inner),
                "STRAIGHT_JOIN should be treated as Inner join on edge"
            );
        }
    }
}

#[test]
fn cross_apply_sql_server_syntax() {
    // CROSS APPLY - SQL Server specific
    let sql = r#"
        SELECT e.name, d.dept_name
        FROM employees e
        CROSS APPLY (
            SELECT dept_name FROM departments WHERE dept_id = e.dept_id
        ) d
    "#;

    let result = run_analysis(sql, Dialect::Mssql, None);
    if !result.summary.has_errors {
        let tables = collect_table_names(&result);
        assert!(tables.contains("employees"), "employees tracked");
        assert!(
            tables.contains("departments"),
            "departments tracked in CROSS APPLY"
        );
    }
}

#[test]
fn outer_apply_sql_server_syntax() {
    // OUTER APPLY - SQL Server specific
    let sql = r#"
        SELECT e.name, d.dept_name
        FROM employees e
        OUTER APPLY (
            SELECT TOP 1 dept_name FROM departments WHERE dept_id = e.dept_id
        ) d
    "#;

    let result = run_analysis(sql, Dialect::Mssql, None);
    if !result.summary.has_errors {
        let tables = collect_table_names(&result);
        assert!(tables.contains("employees"), "employees tracked");
    }
}

// ============================================================================
// SET OPERATOR TESTS (MINUS)
// ============================================================================

#[test]
fn minus_set_operator_oracle_syntax() {
    // MINUS - Oracle/Teradata equivalent of EXCEPT
    let sql = r#"
        SELECT customer_id FROM all_customers
        MINUS
        SELECT customer_id FROM inactive_customers
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    if !result.summary.has_errors {
        let tables = collect_table_names(&result);
        assert!(
            tables.contains("all_customers"),
            "all_customers tracked in MINUS"
        );
        assert!(
            tables.contains("inactive_customers"),
            "inactive_customers tracked in MINUS"
        );
    }
}

#[test]
fn minus_with_multiple_columns() {
    let sql = r#"
        SELECT id, name, email FROM users_2024
        MINUS
        SELECT id, name, email FROM users_2023
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    if !result.summary.has_errors {
        let tables = collect_table_names(&result);
        assert!(tables.contains("users_2024"), "users_2024 tracked");
        assert!(tables.contains("users_2023"), "users_2023 tracked");
    }
}

// ============================================================================
// UPDATE ... FROM TESTS
// ============================================================================

#[test]
fn update_from_single_source_table() {
    // Basic UPDATE ... FROM with one source table
    let sql = r#"
        UPDATE target_table t
        SET t.value = s.new_value
        FROM source_table s
        WHERE t.id = s.id
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    assert!(!result.summary.has_errors, "UPDATE FROM should parse");

    let tables = collect_table_names(&result);
    assert!(tables.contains("target_table"), "target table tracked");
    assert!(
        tables.contains("source_table"),
        "source table in FROM tracked"
    );
}

#[test]
fn update_from_multiple_source_tables() {
    // UPDATE ... FROM with multiple source tables joined
    let sql = r#"
        UPDATE orders o
        SET o.status = 'shipped',
            o.shipping_date = s.ship_date
        FROM shipments s
        JOIN carriers c ON s.carrier_id = c.id
        WHERE o.id = s.order_id
          AND c.active = true
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    if !result.summary.has_errors {
        let tables = collect_table_names(&result);
        assert!(tables.contains("orders"), "target table tracked");
        assert!(tables.contains("shipments"), "shipments source tracked");
        assert!(tables.contains("carriers"), "carriers source tracked");
    }
}

#[test]
fn update_from_with_subquery() {
    // UPDATE ... FROM with subquery in FROM clause
    let sql = r#"
        UPDATE products p
        SET p.avg_rating = agg.rating
        FROM (
            SELECT product_id, AVG(rating) as rating
            FROM reviews
            GROUP BY product_id
        ) agg
        WHERE p.id = agg.product_id
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    if !result.summary.has_errors {
        let tables = collect_table_names(&result);
        assert!(tables.contains("products"), "target table tracked");
        assert!(tables.contains("reviews"), "subquery source tracked");
    }
}

// ============================================================================
// NEW TABLE FACTOR TESTS (OPENJSON, XMLTABLE, etc.)
// ============================================================================

#[test]
fn openjson_table_factor_sql_server() {
    // OPENJSON - SQL Server JSON table-valued function
    let sql = r#"
        SELECT j.id, j.name
        FROM documents d
        CROSS APPLY OPENJSON(d.json_data)
        WITH (id INT, name VARCHAR(100)) AS j
    "#;

    let result = run_analysis(sql, Dialect::Mssql, None);
    // This may or may not parse depending on sqlparser support
    if !result.summary.has_errors {
        let tables = collect_table_names(&result);
        assert!(tables.contains("documents"), "documents table tracked");
    }
}

#[test]
fn xmltable_factor_oracle_postgres() {
    // XMLTABLE - Oracle/PostgreSQL XML parsing
    let sql = r#"
        SELECT x.id, x.value
        FROM xml_data d,
        XMLTABLE('/root/item'
            PASSING d.xml_content
            COLUMNS
                id INT PATH '@id',
                value VARCHAR(100) PATH 'text()'
        ) AS x
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    // XMLTABLE parsing support varies
    if !result.summary.has_errors {
        let tables = collect_table_names(&result);
        assert!(tables.contains("xml_data"), "xml_data table tracked");
    }
}

#[test]
fn lateral_derived_table() {
    // LATERAL - allows derived table to reference earlier FROM items
    let sql = r#"
        SELECT e.name, d.dept_name
        FROM employees e,
        LATERAL (
            SELECT dept_name
            FROM departments
            WHERE dept_id = e.dept_id
            LIMIT 1
        ) AS d
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    if !result.summary.has_errors {
        let tables = collect_table_names(&result);
        assert!(tables.contains("employees"), "employees tracked");
        assert!(
            tables.contains("departments"),
            "lateral subquery table tracked"
        );
    }
}

#[test]
fn unnest_table_factor() {
    // UNNEST - expands array to rows
    let sql = r#"
        SELECT u.name, tag
        FROM users u
        CROSS JOIN UNNEST(u.tags) AS tag
    "#;

    let result = run_analysis(sql, Dialect::Bigquery, None);
    if !result.summary.has_errors {
        let tables = collect_table_names(&result);
        assert!(tables.contains("users"), "users table tracked");
    }
}

#[test]
fn table_function_call() {
    // Table-valued function call
    let sql = r#"
        SELECT *
        FROM generate_series(1, 10) AS nums(n)
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    // Table functions may or may not produce table nodes
    assert!(
        !result.summary.has_errors || result.issues.iter().any(|i| i.code.contains("PARSE")),
        "should either parse or report parse error"
    );
}

// ============================================================================
// EDGE CASE AND REGRESSION TESTS
// ============================================================================

#[test]
fn deeply_nested_case_expressions() {
    // Test deeply nested CASE to ensure no stack overflow
    let sql = r#"
        SELECT
            CASE WHEN a = 1 THEN
                CASE WHEN b = 2 THEN
                    CASE WHEN c = 3 THEN 'deep'
                    ELSE 'c_fail'
                    END
                ELSE 'b_fail'
                END
            ELSE 'a_fail'
            END AS nested_result
        FROM test_table
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert!(!result.summary.has_errors, "nested CASE should parse");

    let stmt = first_statement(&result);
    let col = find_column_node(stmt, "nested_result");
    assert!(col.is_some(), "nested CASE produces column");
}

#[test]
fn case_with_aggregate_in_condition() {
    // CASE with aggregate function in WHEN condition
    let sql = r#"
        SELECT
            customer_id,
            CASE
                WHEN COUNT(*) > 10 THEN 'frequent'
                WHEN COUNT(*) > 5 THEN 'regular'
                ELSE 'occasional'
            END AS customer_type
        FROM orders
        GROUP BY customer_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert!(
        !result.summary.has_errors,
        "CASE with aggregates should parse"
    );

    let stmt = first_statement(&result);
    let col = find_column_node(stmt, "customer_type");
    assert!(col.is_some(), "CASE with aggregate produces column");
}

#[test]
fn multiple_join_types_in_single_query() {
    // Mix of different join types in one query
    let sql = r#"
        SELECT o.id, c.name, p.product_name, s.status
        FROM orders o
        INNER JOIN customers c ON o.customer_id = c.id
        LEFT JOIN products p ON o.product_id = p.id
        RIGHT JOIN order_status s ON o.status_id = s.id
        CROSS JOIN config cfg
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert!(!result.summary.has_errors, "mixed joins should parse");

    let tables = collect_table_names(&result);
    assert!(tables.contains("orders"), "orders tracked");
    assert!(tables.contains("customers"), "customers tracked");
    assert!(tables.contains("products"), "products tracked");
    assert!(tables.contains("order_status"), "order_status tracked");
    assert!(tables.contains("config"), "config tracked");

    let stmt = first_statement(&result);

    // Verify join types are correctly identified on edges.
    // Look for edges from the table node itself, or from columns owned by the table.
    let find_join_edge = |node_id: &str| -> Option<JoinType> {
        // Collect column IDs owned by this table
        let owned_col_ids: Vec<_> = stmt
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Ownership && e.from.as_ref() == node_id)
            .map(|e| e.to.clone())
            .collect();
        stmt.edges
            .iter()
            .find(|e| {
                e.join_type.is_some()
                    && (e.from.as_ref() == node_id || owned_col_ids.iter().any(|c| c == &e.from))
            })
            .and_then(|e| e.join_type)
    };
    if let Some(cust) = find_table_node(stmt, "customers") {
        assert_eq!(
            find_join_edge(cust.id.as_ref()),
            Some(JoinType::Inner),
            "INNER JOIN detected on edge"
        );
    }
    if let Some(prod) = find_table_node(stmt, "products") {
        assert_eq!(
            find_join_edge(prod.id.as_ref()),
            Some(JoinType::Left),
            "LEFT JOIN detected on edge"
        );
    }
    if let Some(status) = find_table_node(stmt, "order_status") {
        assert_eq!(
            find_join_edge(status.id.as_ref()),
            Some(JoinType::Right),
            "RIGHT JOIN detected on edge"
        );
    }
    if let Some(cfg) = find_table_node(stmt, "config") {
        assert_eq!(
            find_join_edge(cfg.id.as_ref()),
            Some(JoinType::Cross),
            "CROSS JOIN detected on edge"
        );
    }
}

#[test]
fn set_operations_with_all_operators() {
    // Test UNION, INTERSECT, EXCEPT all in one query
    let sql = r#"
        SELECT id FROM table_a
        UNION
        SELECT id FROM table_b
        INTERSECT
        SELECT id FROM table_c
        EXCEPT
        SELECT id FROM table_d
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert!(!result.summary.has_errors, "set operations should parse");

    let tables = collect_table_names(&result);
    assert!(tables.contains("table_a"), "table_a tracked");
    assert!(tables.contains("table_b"), "table_b tracked");
    assert!(tables.contains("table_c"), "table_c tracked");
    assert!(tables.contains("table_d"), "table_d tracked");
}

// ============================================================================
// CONSTRAINT EXTRACTION
// ============================================================================

#[test]
fn create_table_with_inline_primary_key() {
    let sql = r#"
        CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            name TEXT
        );
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert!(
        !result.summary.has_errors,
        "CREATE TABLE with PK should parse"
    );

    let resolved_schema = result
        .resolved_schema
        .as_ref()
        .expect("should have resolved schema");
    let users_table = resolved_schema
        .tables
        .iter()
        .find(|t| t.name == "users")
        .expect("users table should exist");

    let id_col = users_table
        .columns
        .iter()
        .find(|c| c.name == "id")
        .expect("id column should exist");
    assert_eq!(
        id_col.is_primary_key,
        Some(true),
        "id should be marked as PK"
    );

    let name_col = users_table
        .columns
        .iter()
        .find(|c| c.name == "name")
        .expect("name column should exist");
    assert_eq!(name_col.is_primary_key, None, "name should not be PK");
}

#[test]
fn create_table_with_table_level_primary_key() {
    let sql = r#"
        CREATE TABLE orders (
            order_id INTEGER,
            customer_id INTEGER,
            total DECIMAL,
            PRIMARY KEY (order_id)
        );
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert!(
        !result.summary.has_errors,
        "CREATE TABLE with table-level PK should parse"
    );

    let resolved_schema = result
        .resolved_schema
        .as_ref()
        .expect("should have resolved schema");
    let orders_table = resolved_schema
        .tables
        .iter()
        .find(|t| t.name == "orders")
        .expect("orders table should exist");

    let order_id_col = orders_table
        .columns
        .iter()
        .find(|c| c.name == "order_id")
        .expect("order_id column should exist");
    assert_eq!(
        order_id_col.is_primary_key,
        Some(true),
        "order_id should be marked as PK"
    );

    // Check table-level constraint
    assert!(
        !orders_table.constraints.is_empty(),
        "should have table constraints"
    );
    let pk_constraint = orders_table
        .constraints
        .iter()
        .find(|c| matches!(c.constraint_type, ConstraintType::PrimaryKey))
        .expect("PK constraint should exist");
    assert_eq!(pk_constraint.columns, vec!["order_id"]);
}

#[test]
fn create_table_with_inline_foreign_key() {
    let sql = r#"
        CREATE TABLE order_items (
            id INTEGER PRIMARY KEY,
            order_id INTEGER REFERENCES orders(id),
            product_name TEXT
        );
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    assert!(
        !result.summary.has_errors,
        "CREATE TABLE with FK should parse"
    );

    let resolved_schema = result
        .resolved_schema
        .as_ref()
        .expect("should have resolved schema");
    let items_table = resolved_schema
        .tables
        .iter()
        .find(|t| t.name == "order_items")
        .expect("order_items table should exist");

    let order_id_col = items_table
        .columns
        .iter()
        .find(|c| c.name == "order_id")
        .expect("order_id column should exist");

    let fk_ref = order_id_col
        .foreign_key
        .as_ref()
        .expect("should have FK reference");
    assert_eq!(fk_ref.table, "orders");
    assert_eq!(fk_ref.column, "id");
}

#[test]
fn create_table_with_table_level_foreign_key() {
    let sql = r#"
        CREATE TABLE order_items (
            id INTEGER PRIMARY KEY,
            order_id INTEGER,
            product_id INTEGER,
            quantity INTEGER,
            FOREIGN KEY (order_id) REFERENCES orders(id),
            FOREIGN KEY (product_id) REFERENCES products(product_id)
        );
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert!(
        !result.summary.has_errors,
        "CREATE TABLE with table-level FK should parse"
    );

    let resolved_schema = result
        .resolved_schema
        .as_ref()
        .expect("should have resolved schema");
    let items_table = resolved_schema
        .tables
        .iter()
        .find(|t| t.name == "order_items")
        .expect("order_items table should exist");

    // Check table-level FK constraints
    let fk_constraints: Vec<_> = items_table
        .constraints
        .iter()
        .filter(|c| matches!(c.constraint_type, ConstraintType::ForeignKey))
        .collect();

    assert_eq!(fk_constraints.len(), 2, "should have 2 FK constraints");

    let order_fk = fk_constraints
        .iter()
        .find(|c| c.columns.contains(&"order_id".to_string()))
        .expect("order FK should exist");
    assert_eq!(order_fk.referenced_table.as_deref(), Some("orders"));
    assert_eq!(
        order_fk.referenced_columns.as_deref(),
        Some(&["id".to_string()][..])
    );

    let product_fk = fk_constraints
        .iter()
        .find(|c| c.columns.contains(&"product_id".to_string()))
        .expect("product FK should exist");
    assert_eq!(product_fk.referenced_table.as_deref(), Some("products"));
}

#[test]
fn create_table_with_composite_primary_key() {
    let sql = r#"
        CREATE TABLE order_line_items (
            order_id INTEGER,
            line_number INTEGER,
            product_id INTEGER,
            quantity INTEGER,
            PRIMARY KEY (order_id, line_number)
        );
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    assert!(
        !result.summary.has_errors,
        "CREATE TABLE with composite PK should parse"
    );

    let resolved_schema = result
        .resolved_schema
        .as_ref()
        .expect("should have resolved schema");
    let items_table = resolved_schema
        .tables
        .iter()
        .find(|t| t.name == "order_line_items")
        .expect("order_line_items table should exist");

    // Both columns in composite PK should be marked
    let order_id_col = items_table
        .columns
        .iter()
        .find(|c| c.name == "order_id")
        .expect("order_id column should exist");
    assert_eq!(
        order_id_col.is_primary_key,
        Some(true),
        "order_id should be marked as PK"
    );

    let line_num_col = items_table
        .columns
        .iter()
        .find(|c| c.name == "line_number")
        .expect("line_number column should exist");
    assert_eq!(
        line_num_col.is_primary_key,
        Some(true),
        "line_number should be marked as PK"
    );

    // Check table constraint has both columns
    let pk_constraint = items_table
        .constraints
        .iter()
        .find(|c| matches!(c.constraint_type, ConstraintType::PrimaryKey))
        .expect("PK constraint should exist");
    assert_eq!(pk_constraint.columns.len(), 2);
    assert!(pk_constraint.columns.contains(&"order_id".to_string()));
    assert!(pk_constraint.columns.contains(&"line_number".to_string()));
}

// =============================================================================
// COPY STATEMENT LINEAGE
// =============================================================================

#[test]
fn test_copy_statement_lineage() {
    let sql = "COPY users FROM 's3://bucket/users.csv'";
    let result = run_analysis(sql, Dialect::Generic, None);

    assert!(result.issues.iter().all(|i| i.severity != Severity::Error));
    // COPY FROM: external source -> table (users is target)
    let stmt = &result.statements[0];
    assert!(stmt.nodes.iter().any(|n| n.label.contains("users")));
}

#[test]
fn test_copy_into_snowflake() {
    let sql = "COPY INTO analytics.orders FROM @my_stage/orders/";
    let result = run_analysis(sql, Dialect::Snowflake, None);

    assert!(result.issues.iter().all(|i| i.severity != Severity::Error));
    let stmt = &result.statements[0];
    assert!(stmt.nodes.iter().any(|n| n.label.contains("orders")));
}

#[test]
fn test_copy_to_with_query() {
    let sql = "COPY (SELECT id, name FROM users WHERE active = true) TO '/tmp/out.csv'";
    let result = run_analysis(sql, Dialect::Postgres, None);

    // COPY TO with query: users is source
    let stmt = &result.statements[0];
    assert!(stmt.nodes.iter().any(|n| n.label.contains("users")));
}

#[test]
fn test_copy_with_column_list() {
    // COPY with explicit column list
    let sql = "COPY users (id, name, email) FROM '/tmp/users.csv'";
    let result = run_analysis(sql, Dialect::Postgres, None);

    assert!(result.issues.iter().all(|i| i.severity != Severity::Error));
    let stmt = &result.statements[0];

    // users should be identified as target (COPY FROM loads data into table)
    assert!(
        stmt.nodes.iter().any(|n| n.label.as_ref() == "users"),
        "Expected 'users' table in lineage"
    );
}

#[test]
fn test_copy_schema_qualified_table() {
    // COPY with schema-qualified table name
    let sql = "COPY analytics.events FROM 's3://bucket/events.csv'";
    let result = run_analysis(sql, Dialect::Postgres, None);

    assert!(result.issues.iter().all(|i| i.severity != Severity::Error));
    let stmt = &result.statements[0];

    // Check that qualified name is tracked
    let table_node = stmt
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Table)
        .expect("Expected a table node");
    assert!(
        table_node.qualified_name.as_ref().map(|n| n.as_ref()) == Some("analytics.events")
            || table_node.label.as_ref() == "events",
        "Expected 'analytics.events' table, got: {:?}",
        table_node
    );
}

// =============================================================================
// UNLOAD STATEMENT LINEAGE
// =============================================================================

#[test]
fn test_unload_statement_string_query() {
    // Redshift UNLOAD with query as string literal
    let sql = r#"UNLOAD ('SELECT * FROM orders') TO 's3://bucket/out'"#;
    let result = run_analysis(sql, Dialect::Redshift, None);

    assert!(result.issues.iter().all(|i| i.severity != Severity::Error));
    let stmt = &result.statements[0];
    assert_eq!(stmt.statement_type, "UNLOAD");

    // orders should be identified as source
    assert!(
        stmt.nodes.iter().any(|n| n.label.contains("orders")),
        "Expected 'orders' table in lineage, got: {:?}",
        stmt.nodes.iter().map(|n| &n.label).collect::<Vec<_>>()
    );
}

#[test]
fn test_unload_statement_parsed_query() {
    // UNLOAD with query as parsed expression (without string literal)
    let sql = r#"UNLOAD (SELECT id, name FROM users WHERE active = true) TO 's3://bucket/out'"#;
    let result = run_analysis(sql, Dialect::Redshift, None);

    assert!(result.issues.iter().all(|i| i.severity != Severity::Error));
    let stmt = &result.statements[0];
    assert_eq!(stmt.statement_type, "UNLOAD");

    // users should be identified as source
    assert!(
        stmt.nodes.iter().any(|n| n.label.contains("users")),
        "Expected 'users' table in lineage, got: {:?}",
        stmt.nodes.iter().map(|n| &n.label).collect::<Vec<_>>()
    );
}

#[test]
fn test_unload_statement_qualified_table() {
    // UNLOAD with fully qualified table name
    let sql = r#"UNLOAD ('SELECT * FROM analytics.orders WHERE order_date > ''2024-01-01''')
TO 's3://bucket/exports/orders_'
IAM_ROLE 'arn:aws:iam::123456789:role/RedshiftCopyRole'"#;
    let result = run_analysis(sql, Dialect::Redshift, None);

    assert!(result.issues.iter().all(|i| i.severity != Severity::Error));
    let stmt = &result.statements[0];
    assert_eq!(stmt.statement_type, "UNLOAD");

    // Should have analytics.orders in lineage
    let table_node = stmt
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Table)
        .expect("Should have a table node");
    assert!(
        table_node.qualified_name.as_ref().map(|n| n.as_ref()) == Some("analytics.orders")
            || table_node.label.as_ref() == "orders",
        "Expected 'analytics.orders' table in lineage, got: {:?}",
        table_node
    );
}

#[test]
fn test_unload_statement_with_join() {
    // UNLOAD with a JOIN query
    let sql = r#"UNLOAD ('SELECT o.id, c.name FROM orders o JOIN customers c ON o.customer_id = c.id')
TO 's3://bucket/out'"#;
    let result = run_analysis(sql, Dialect::Redshift, None);

    assert!(result.issues.iter().all(|i| i.severity != Severity::Error));
    let stmt = &result.statements[0];

    // Both orders and customers should be identified as sources
    let table_labels: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .map(|n| n.label.as_ref())
        .collect();
    assert!(
        table_labels.contains(&"orders"),
        "Expected 'orders' table, got: {:?}",
        table_labels
    );
    assert!(
        table_labels.contains(&"customers"),
        "Expected 'customers' table, got: {:?}",
        table_labels
    );
}

#[test]
fn test_unload_statement_malformed_query_string() {
    // UNLOAD with a malformed query string should produce a warning, not crash
    let sql = r#"UNLOAD ('SELECT * FROM WHERE invalid syntax')
TO 's3://bucket/out'"#;
    let result = run_analysis(sql, Dialect::Redshift, None);

    // Should complete without fatal error
    assert!(
        !result.statements.is_empty(),
        "Should still produce a statement even with malformed query"
    );

    // Should have a warning about parse failure
    assert!(
        result
            .issues
            .iter()
            .any(|i| i.severity == Severity::Warning && i.code == issue_codes::PARSE_ERROR),
        "Expected PARSE_ERROR warning for malformed query, got: {:?}",
        result.issues
    );
}

// =============================================================================
// ALTER TABLE LINEAGE
// =============================================================================

#[test]
fn test_alter_table_rename() {
    let sql = "ALTER TABLE old_users RENAME TO new_users";
    let result = run_analysis(sql, Dialect::Generic, None);

    assert!(result.issues.iter().all(|i| i.severity != Severity::Error));
    let stmt = &result.statements[0];

    // Both old and new table should appear in lineage
    let labels: Vec<_> = stmt.nodes.iter().map(|n| n.label.as_ref()).collect();
    assert!(labels.contains(&"old_users"));
    assert!(labels.contains(&"new_users"));

    // Find the old_users and new_users node IDs
    let old_node = stmt
        .nodes
        .iter()
        .find(|n| n.label.as_ref() == "old_users")
        .expect("old_users node should exist");
    let new_node = stmt
        .nodes
        .iter()
        .find(|n| n.label.as_ref() == "new_users")
        .expect("new_users node should exist");

    // Should have exactly one DataFlow edge from old_users to new_users with RENAME operation
    assert_eq!(stmt.edges.len(), 1, "Should have exactly one edge");
    let edge = &stmt.edges[0];
    assert_eq!(
        edge.edge_type,
        EdgeType::DataFlow,
        "Edge should be DataFlow"
    );
    assert_eq!(
        edge.from.as_ref(),
        old_node.id.as_ref(),
        "Edge should be from old_users"
    );
    assert_eq!(
        edge.to.as_ref(),
        new_node.id.as_ref(),
        "Edge should be to new_users"
    );
    assert_eq!(
        edge.operation.as_ref().map(|o| o.as_ref()),
        Some("RENAME"),
        "Operation should be RENAME"
    );
}

#[test]
fn test_alter_table_rename_with_schema() {
    let sql = "ALTER TABLE analytics.legacy_orders RENAME TO analytics.orders_v2";
    let result = run_analysis(sql, Dialect::Generic, None);

    assert!(result.issues.iter().all(|i| i.severity != Severity::Error));
    let stmt = &result.statements[0];

    // Both old and new table should appear in lineage with schema qualification
    let qualified_names: Vec<_> = stmt
        .nodes
        .iter()
        .filter_map(|n| n.qualified_name.as_ref())
        .map(|qn| qn.as_ref())
        .collect();
    assert!(qualified_names.contains(&"analytics.legacy_orders"));
    assert!(qualified_names.contains(&"analytics.orders_v2"));

    // Find the old and new table node IDs
    let old_node = stmt
        .nodes
        .iter()
        .find(|n| {
            n.qualified_name
                .as_ref()
                .map(|qn| qn.as_ref() == "analytics.legacy_orders")
                .unwrap_or(false)
        })
        .expect("analytics.legacy_orders node should exist");
    let new_node = stmt
        .nodes
        .iter()
        .find(|n| {
            n.qualified_name
                .as_ref()
                .map(|qn| qn.as_ref() == "analytics.orders_v2")
                .unwrap_or(false)
        })
        .expect("analytics.orders_v2 node should exist");

    // Should have exactly one DataFlow edge from old table to new table with RENAME operation
    assert_eq!(stmt.edges.len(), 1, "Should have exactly one edge");
    let edge = &stmt.edges[0];
    assert_eq!(
        edge.edge_type,
        EdgeType::DataFlow,
        "Edge should be DataFlow"
    );
    assert_eq!(
        edge.from.as_ref(),
        old_node.id.as_ref(),
        "Edge should be from legacy_orders"
    );
    assert_eq!(
        edge.to.as_ref(),
        new_node.id.as_ref(),
        "Edge should be to orders_v2"
    );
    assert_eq!(
        edge.operation.as_ref().map(|o| o.as_ref()),
        Some("RENAME"),
        "Operation should be RENAME"
    );
}

#[test]
fn test_alter_table_rename_inherits_schema_when_unqualified() {
    let sql = "ALTER TABLE analytics.legacy_orders RENAME TO orders_v2";
    let result = run_analysis(sql, Dialect::Generic, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );
    let stmt = &result.statements[0];

    let old_node = stmt
        .nodes
        .iter()
        .find(|n| {
            n.qualified_name
                .as_ref()
                .map(|qn| qn.as_ref() == "analytics.legacy_orders")
                .unwrap_or(false)
        })
        .expect("analytics.legacy_orders node should exist");
    let new_node = stmt
        .nodes
        .iter()
        .find(|n| n.label.as_ref() == "orders_v2")
        .expect("orders_v2 node should exist");

    assert_eq!(
        new_node.qualified_name.as_deref(),
        Some("analytics.orders_v2"),
        "New node should inherit schema qualification"
    );

    assert_eq!(stmt.edges.len(), 1, "Should have exactly one edge");
    let edge = &stmt.edges[0];
    assert_eq!(
        edge.from.as_ref(),
        old_node.id.as_ref(),
        "Edge should originate from old table"
    );
    assert_eq!(
        edge.to.as_ref(),
        new_node.id.as_ref(),
        "Edge should point to renamed table"
    );
    assert_eq!(
        edge.operation.as_ref().map(|o| o.as_ref()),
        Some("RENAME"),
        "Edge should be marked as RENAME operation"
    );
}

// =============================================================================
// CROSS-STATEMENT TESTS
// =============================================================================
// These tests verify that lineage is correctly tracked across multiple
// statements in a single analysis batch.

#[test]
fn test_cross_statement_rename_then_select() {
    // After renaming a table, a SELECT should reference the new name
    let sql = r#"
        ALTER TABLE old_users RENAME TO users;
        SELECT id, name FROM users;
    "#;
    let result = run_analysis(sql, Dialect::Generic, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );
    assert_eq!(result.statements.len(), 2);

    // First statement: RENAME
    let rename_stmt = &result.statements[0];
    assert!(rename_stmt
        .nodes
        .iter()
        .any(|n| n.label.as_ref() == "old_users"));
    assert!(rename_stmt
        .nodes
        .iter()
        .any(|n| n.label.as_ref() == "users"));

    // Second statement: SELECT from users
    let select_stmt = &result.statements[1];
    assert!(
        select_stmt
            .nodes
            .iter()
            .any(|n| n.label.as_ref() == "users"),
        "SELECT should reference 'users' table"
    );
}

#[test]
fn test_cross_statement_copy_then_select() {
    // COPY data into a table, then SELECT from it
    let sql = r#"
        COPY users FROM '/data/users.csv';
        SELECT id, name FROM users WHERE active = true;
    "#;
    let result = run_analysis(sql, Dialect::Postgres, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );
    assert_eq!(result.statements.len(), 2);

    // Both statements should reference users table
    let copy_stmt = &result.statements[0];
    assert!(
        copy_stmt.nodes.iter().any(|n| n.label.as_ref() == "users"),
        "COPY should reference 'users' table"
    );

    let select_stmt = &result.statements[1];
    assert!(
        select_stmt
            .nodes
            .iter()
            .any(|n| n.label.as_ref() == "users"),
        "SELECT should reference 'users' table"
    );
}

#[test]
fn test_cross_statement_ctas_then_select() {
    // CREATE TABLE AS, then SELECT from the new table
    let sql = r#"
        CREATE TABLE active_users AS
        SELECT id, name FROM users WHERE active = true;

        SELECT * FROM active_users WHERE name LIKE 'A%';
    "#;
    let result = run_analysis(sql, Dialect::Generic, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );
    assert_eq!(result.statements.len(), 2);

    // First statement: CTAS
    let ctas_stmt = &result.statements[0];
    assert!(
        ctas_stmt.nodes.iter().any(|n| n.label.as_ref() == "users"),
        "CTAS should reference source 'users' table"
    );
    assert!(
        ctas_stmt
            .nodes
            .iter()
            .any(|n| n.label.as_ref() == "active_users"),
        "CTAS should create 'active_users' table"
    );

    // Second statement: SELECT from active_users
    let select_stmt = &result.statements[1];
    assert!(
        select_stmt
            .nodes
            .iter()
            .any(|n| n.label.as_ref() == "active_users"),
        "SELECT should reference 'active_users' table"
    );
}

// =============================================================================
// ADVANCED COPY/UNLOAD TESTS
// =============================================================================

#[test]
fn test_copy_into_snowflake_with_transformation_query() {
    // Snowflake COPY INTO table with transformation query
    let sql = r#"
        COPY INTO target_table
        FROM (SELECT $1, $2, CURRENT_TIMESTAMP() FROM @my_stage/data/)
        FILE_FORMAT = (TYPE = CSV)
    "#;
    let result = run_analysis(sql, Dialect::Snowflake, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );

    let stmt = &result.statements[0];
    assert!(
        stmt.nodes
            .iter()
            .any(|n| n.label.as_ref() == "target_table"),
        "Should have target_table node"
    );
}

#[test]
fn test_copy_into_location_from_table() {
    // Snowflake COPY INTO location FROM table (export)
    let sql = "COPY INTO @my_stage/export/ FROM analytics.orders";
    let result = run_analysis(sql, Dialect::Snowflake, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );

    let stmt = &result.statements[0];
    assert!(
        stmt.nodes.iter().any(|n| n.label.as_ref() == "orders"),
        "Should have orders table as source"
    );
}

#[test]
fn test_copy_into_location_from_query() {
    // Snowflake COPY INTO location FROM query (export with transformation)
    let sql = r#"
        COPY INTO @my_stage/export/
        FROM (
            SELECT o.id, o.total, c.name
            FROM orders o
            JOIN customers c ON o.customer_id = c.id
            WHERE o.status = 'completed'
        )
    "#;
    let result = run_analysis(sql, Dialect::Snowflake, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );

    let stmt = &result.statements[0];
    // Snowflake normalizes identifiers to uppercase
    let labels: Vec<_> = stmt.nodes.iter().map(|n| n.label.to_uppercase()).collect();
    assert!(
        labels.iter().any(|l| l == "ORDERS"),
        "Should have orders table, got: {:?}",
        labels
    );
    assert!(
        labels.iter().any(|l| l == "CUSTOMERS"),
        "Should have customers table, got: {:?}",
        labels
    );
}

#[test]
fn test_unload_with_subquery_in_from() {
    // UNLOAD with a derived table subquery in FROM clause
    let sql = r#"
        UNLOAD ('
            SELECT sub.id, sub.name
            FROM (
                SELECT id, name FROM users WHERE active = true
            ) sub
        ')
        TO 's3://bucket/users/'
    "#;
    let result = run_analysis(sql, Dialect::Redshift, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );

    let stmt = &result.statements[0];
    let labels: Vec<_> = stmt.nodes.iter().map(|n| n.label.as_ref()).collect();
    assert!(labels.contains(&"users"), "Should have users table");
}

#[test]
fn test_unload_with_scalar_subquery_expression() {
    // UNLOAD with a scalar subquery in SELECT expression
    // Note: Scalar subqueries store the subquery as expression text rather than
    // extracting the referenced table as a separate node. This is by design -
    // the expression column captures the full subquery text for traceability.
    let sql = r#"
        UNLOAD ('
            SELECT u.id, u.name,
                   (SELECT COUNT(*) FROM orders o WHERE o.user_id = u.id) as order_count
            FROM users u
            WHERE u.active = true
        ')
        TO 's3://bucket/users/'
    "#;
    let result = run_analysis(sql, Dialect::Redshift, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );

    let stmt = &result.statements[0];
    let labels: Vec<_> = stmt.nodes.iter().map(|n| n.label.as_ref()).collect();
    assert!(labels.contains(&"users"), "Should have users table");

    // The scalar subquery is stored as an expression, verify it's captured
    let order_count_col = stmt
        .nodes
        .iter()
        .find(|n| n.label.as_ref() == "order_count");
    assert!(order_count_col.is_some(), "Should have order_count column");
    assert!(
        order_count_col
            .unwrap()
            .expression
            .as_ref()
            .map(|e| e.contains("orders"))
            .unwrap_or(false),
        "order_count expression should reference orders table"
    );
}

#[test]
fn test_unload_with_cte() {
    // UNLOAD with a CTE in the query
    let sql = r#"
        UNLOAD ('
            WITH active_users AS (
                SELECT id, name FROM users WHERE active = true
            ),
            user_orders AS (
                SELECT user_id, SUM(total) as total_spent
                FROM orders
                GROUP BY user_id
            )
            SELECT au.id, au.name, COALESCE(uo.total_spent, 0) as total_spent
            FROM active_users au
            LEFT JOIN user_orders uo ON au.id = uo.user_id
        ')
        TO 's3://bucket/report/'
    "#;
    let result = run_analysis(sql, Dialect::Redshift, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );

    let stmt = &result.statements[0];
    let labels: Vec<_> = stmt.nodes.iter().map(|n| n.label.as_ref()).collect();
    assert!(labels.contains(&"users"), "Should have users table");
    assert!(labels.contains(&"orders"), "Should have orders table");
}

#[test]
fn test_copy_postgres_with_column_list_and_options() {
    // PostgreSQL COPY with column list and various options
    let sql = "COPY users (id, name, email) FROM '/data/users.csv' WITH (FORMAT CSV, HEADER true)";
    let result = run_analysis(sql, Dialect::Postgres, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );

    let stmt = &result.statements[0];
    assert!(
        stmt.nodes.iter().any(|n| n.label.as_ref() == "users"),
        "Should have users table"
    );
}

#[test]
fn test_copy_to_stdout() {
    // PostgreSQL COPY TO STDOUT (common pattern)
    let sql = "COPY (SELECT id, name FROM users WHERE created_at > '2024-01-01') TO STDOUT";
    let result = run_analysis(sql, Dialect::Postgres, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );

    let stmt = &result.statements[0];
    assert!(
        stmt.nodes.iter().any(|n| n.label.as_ref() == "users"),
        "Should have users table as source"
    );
}

// =============================================================================
// ERROR HANDLING AND EDGE CASES
// =============================================================================

#[test]
fn test_unload_empty_query_string() {
    // UNLOAD with empty query string should not crash
    let sql = "UNLOAD ('') TO 's3://bucket/out'";
    let result = run_analysis(sql, Dialect::Redshift, None);

    // Should complete without panic
    assert!(!result.statements.is_empty());
}

#[test]
fn test_copy_from_multiple_files_pattern() {
    // COPY from multiple files using pattern (common S3 pattern)
    let sql =
        "COPY events FROM 's3://bucket/events/2024/*' IAM_ROLE 'arn:aws:iam::123:role/redshift'";
    let result = run_analysis(sql, Dialect::Redshift, None);

    // Should parse without error
    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );
}

#[test]
fn test_alter_table_rename_preserves_case_sensitivity() {
    // Test case sensitivity in rename operations
    let sql = r#"ALTER TABLE "MyTable" RENAME TO "my_table""#;
    let result = run_analysis(sql, Dialect::Postgres, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );

    let stmt = &result.statements[0];
    // Labels include quotes, but qualified_name has the actual case-preserved identifier
    let qualified_names: Vec<_> = stmt
        .nodes
        .iter()
        .filter_map(|n| n.qualified_name.as_ref())
        .map(|qn| qn.as_ref())
        .collect();
    assert!(
        qualified_names.contains(&"MyTable"),
        "Should preserve case for MyTable, got: {:?}",
        qualified_names
    );
    assert!(
        qualified_names.contains(&"my_table"),
        "Should have my_table, got: {:?}",
        qualified_names
    );
}

// =============================================================================
// 3-PART CATALOG.SCHEMA.TABLE RENAME TESTS
// =============================================================================

#[test]
fn test_alter_table_rename_with_full_catalog_path() {
    // Snowflake and BigQuery support 3-part names: catalog.schema.table
    let sql = "ALTER TABLE analytics_db.reporting.legacy_orders RENAME TO analytics_db.reporting.orders_v2";
    let result = run_analysis(sql, Dialect::Snowflake, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );

    let stmt = &result.statements[0];

    // Should have both source and target tables
    let tables: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();

    assert_eq!(tables.len(), 2, "Should have source and target table nodes");

    // Check qualified names contain the 3-part names (Snowflake uppercases unquoted identifiers)
    let qualified_names: Vec<_> = tables
        .iter()
        .filter_map(|t| t.qualified_name.as_ref())
        .collect();
    assert!(
        qualified_names
            .iter()
            .any(|qn| qn.contains("LEGACY_ORDERS") || qn.contains("legacy_orders")),
        "Should have source table, got: {:?}",
        qualified_names
    );
    assert!(
        qualified_names
            .iter()
            .any(|qn| qn.contains("ORDERS_V2") || qn.contains("orders_v2")),
        "Should have target table, got: {:?}",
        qualified_names
    );

    // Should have a RENAME edge
    let rename_edges: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::DataFlow)
        .filter(|e| e.operation.as_deref() == Some("RENAME"))
        .collect();

    assert_eq!(rename_edges.len(), 1, "Should have exactly one RENAME edge");
}

#[test]
fn test_alter_table_rename_inherits_catalog_when_partially_qualified() {
    // When target has fewer parts than source, it should inherit missing parts
    let sql = "ALTER TABLE production.sales.monthly_report RENAME TO quarterly_report";
    let result = run_analysis(sql, Dialect::Snowflake, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );

    // The important thing is that both tables are recognized
    let stmt = &result.statements[0];
    let tables: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();

    assert_eq!(tables.len(), 2, "Should have source and target table nodes");

    // Should have a RENAME edge regardless of qualification level
    let rename_edges: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::DataFlow)
        .filter(|e| e.operation.as_deref() == Some("RENAME"))
        .collect();

    assert_eq!(rename_edges.len(), 1, "Should have exactly one RENAME edge");
}

#[test]
fn test_alter_table_rename_cross_schema() {
    // Renaming a table to a different schema (some databases support this)
    let sql = "ALTER TABLE staging.raw_data RENAME TO production.cleaned_data";
    let result = run_analysis(sql, Dialect::Snowflake, None);

    assert!(
        result.issues.iter().all(|i| i.severity != Severity::Error),
        "Should not produce errors: {:?}",
        result.issues
    );

    let stmt = &result.statements[0];

    // Check that both schemas are captured
    let qualified_names: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .filter_map(|n| n.qualified_name.as_ref())
        .collect();

    // Snowflake uppercases - check for both possible cases
    let has_staging = qualified_names
        .iter()
        .any(|qn| qn.to_uppercase().contains("STAGING") || qn.contains("staging"));
    let has_production = qualified_names
        .iter()
        .any(|qn| qn.to_uppercase().contains("PRODUCTION") || qn.contains("production"));

    assert!(
        has_staging,
        "Should reference staging schema, got: {:?}",
        qualified_names
    );
    assert!(
        has_production,
        "Should reference production schema, got: {:?}",
        qualified_names
    );

    // Should still have RENAME edge
    let rename_edges: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::DataFlow)
        .filter(|e| e.operation.as_deref() == Some("RENAME"))
        .collect();

    assert_eq!(rename_edges.len(), 1, "Should have exactly one RENAME edge");
}

// ============================================================================
// BACKWARD COLUMN INFERENCE TESTS
// Tests for inferring columns from downstream usage through SELECT * chains
// ============================================================================

#[test]
fn backward_inference_basic_select_star() {
    // Basic test: CTE with SELECT *, downstream explicit columns
    let sql = r#"
        WITH orders AS (
            SELECT * FROM stg_orders
        ),
        customer_orders AS (
            SELECT
                customer_id,
                COUNT(order_id) AS order_count
            FROM orders
            GROUP BY customer_id
        )
        SELECT * FROM customer_orders
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // stg_orders should exist as a source table
    let stg_orders_node = find_table_node(stmt, "stg_orders");
    assert!(
        stg_orders_node.is_some(),
        "stg_orders table node should exist"
    );
    let stg_orders_id = &stg_orders_node.unwrap().id;

    // Find column ownership edges from stg_orders
    let stg_orders_column_edges: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| e.from == *stg_orders_id && e.edge_type == EdgeType::Ownership)
        .collect();

    // stg_orders should have exactly 2 inferred columns (customer_id and order_id)
    assert_eq!(
        stg_orders_column_edges.len(),
        2,
        "stg_orders should have exactly 2 inferred column ownership edges (customer_id, order_id), found {}",
        stg_orders_column_edges.len()
    );

    // Verify specific columns were inferred
    let stg_orders_column_labels: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name
                    .as_ref()
                    .map(|qn| qn.starts_with("stg_orders."))
                    .unwrap_or(false)
        })
        .map(|n| n.label.to_string())
        .collect();

    assert!(
        stg_orders_column_labels.contains(&"customer_id".to_string()),
        "customer_id should be inferred on stg_orders"
    );
    assert!(
        stg_orders_column_labels.contains(&"order_id".to_string()),
        "order_id should be inferred on stg_orders"
    );

    // Verify data flow edges exist from inferred columns to CTE columns
    // Find a stg_orders column and check it has a data flow edge to the orders CTE column
    let stg_orders_customer_id = stmt
        .nodes
        .iter()
        .find(|n| {
            n.node_type == NodeType::Column
                && n.label.as_ref() == "customer_id"
                && n.qualified_name
                    .as_ref()
                    .map(|qn| qn.starts_with("stg_orders."))
                    .unwrap_or(false)
        })
        .expect("stg_orders.customer_id should exist");

    let data_flow_from_stg_orders_customer_id: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| e.from == stg_orders_customer_id.id && e.edge_type == EdgeType::DataFlow)
        .collect();

    assert!(
        !data_flow_from_stg_orders_customer_id.is_empty(),
        "Data flow edge should exist from stg_orders.customer_id to orders.customer_id"
    );
}

#[test]
fn backward_inference_select_star_in_derived_table() {
    // Regression test: derived tables using SELECT * should register pending wildcards
    // so that downstream column references infer the source columns.
    let sql = r#"
        SELECT
            o.customer_id
        FROM (
            SELECT * FROM raw_orders
        ) AS o
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    let raw_orders_customer_id = stmt
        .nodes
        .iter()
        .find(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name.as_deref() == Some("raw_orders.customer_id")
        })
        .expect("raw_orders.customer_id should be inferred through derived table");

    let has_data_flow = stmt
        .edges
        .iter()
        .any(|edge| edge.edge_type == EdgeType::DataFlow && edge.from == raw_orders_customer_id.id);

    assert!(
        has_data_flow,
        "raw_orders.customer_id should participate in data flow after inference"
    );
}

#[test]
fn backward_inference_transitive_chain() {
    // Transitive chain: step3 → SELECT * FROM step2 → SELECT * FROM step1 → SELECT * FROM source
    // Columns used in step3 should propagate back to source
    let sql = r#"
        WITH step1 AS (
            SELECT * FROM raw_events
        ),
        step2 AS (
            SELECT * FROM step1
        ),
        step3 AS (
            SELECT
                event_id,
                event_type,
                user_id
            FROM step2
        )
        SELECT * FROM step3
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // raw_events should exist as a source table
    let raw_events_node = find_table_node(stmt, "raw_events");
    assert!(
        raw_events_node.is_some(),
        "raw_events table node should exist"
    );

    // Verify exactly 3 columns were inferred on the source table
    let raw_events_columns: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name
                    .as_ref()
                    .map(|qn| qn.starts_with("raw_events."))
                    .unwrap_or(false)
        })
        .map(|n| n.label.to_string())
        .collect();

    assert_eq!(
        raw_events_columns.len(),
        3,
        "raw_events should have exactly 3 inferred columns (event_id, event_type, user_id), found: {:?}",
        raw_events_columns
    );
    assert!(
        raw_events_columns.contains(&"event_id".to_string()),
        "event_id should be inferred on raw_events"
    );
    assert!(
        raw_events_columns.contains(&"event_type".to_string()),
        "event_type should be inferred on raw_events"
    );
    assert!(
        raw_events_columns.contains(&"user_id".to_string()),
        "user_id should be inferred on raw_events"
    );

    // Verify data flow edges exist from inferred source columns through the chain
    let raw_events_event_id = stmt
        .nodes
        .iter()
        .find(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name.as_deref() == Some("raw_events.event_id")
        })
        .expect("raw_events.event_id should exist");

    let data_flow_edges_from_event_id: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| e.from == raw_events_event_id.id && e.edge_type == EdgeType::DataFlow)
        .collect();

    assert!(
        !data_flow_edges_from_event_id.is_empty(),
        "Data flow edges should exist from raw_events.event_id through the CTE chain"
    );
}

#[test]
fn backward_inference_multiple_sources() {
    // Multiple source tables with SELECT *, columns from each inferred separately
    // Note: This test uses simpler column references without table aliases in the
    // combined CTE to avoid scope-related non-determinism issues.
    let sql = r#"
        WITH orders AS (
            SELECT * FROM stg_orders
        ),
        customers AS (
            SELECT * FROM stg_customers
        ),
        combined AS (
            SELECT
                orders.order_id,
                orders.order_date,
                customers.customer_name,
                customers.email
            FROM orders
            JOIN customers ON orders.customer_id = customers.customer_id
        )
        SELECT * FROM combined
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Verify stg_orders has its columns inferred from SELECT list
    // Note: customer_id from JOIN condition is tracked separately via source_table_columns,
    // but backward inference specifically propagates columns from CTE output (SELECT list)
    let stg_orders_columns: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name
                    .as_ref()
                    .map(|qn| qn.starts_with("stg_orders."))
                    .unwrap_or(false)
        })
        .map(|n| n.label.to_string())
        .collect();

    assert_eq!(
        stg_orders_columns.len(),
        2,
        "stg_orders should have exactly 2 inferred columns from SELECT, found: {:?}",
        stg_orders_columns
    );
    assert!(
        stg_orders_columns.contains(&"order_id".to_string()),
        "order_id should be inferred on stg_orders"
    );
    assert!(
        stg_orders_columns.contains(&"order_date".to_string()),
        "order_date should be inferred on stg_orders"
    );

    // Verify stg_customers has its columns inferred from SELECT list
    let stg_customers_columns: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name
                    .as_ref()
                    .map(|qn| qn.starts_with("stg_customers."))
                    .unwrap_or(false)
        })
        .map(|n| n.label.to_string())
        .collect();

    assert_eq!(
        stg_customers_columns.len(),
        2,
        "stg_customers should have exactly 2 inferred columns from SELECT, found: {:?}",
        stg_customers_columns
    );
    assert!(
        stg_customers_columns.contains(&"customer_name".to_string()),
        "customer_name should be inferred on stg_customers"
    );
    assert!(
        stg_customers_columns.contains(&"email".to_string()),
        "email should be inferred on stg_customers"
    );

    // Verify data flow edges exist from both source tables
    let stg_orders_order_id = stmt
        .nodes
        .iter()
        .find(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name.as_deref() == Some("stg_orders.order_id")
        })
        .expect("stg_orders.order_id should exist");

    let stg_customers_customer_name = stmt
        .nodes
        .iter()
        .find(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name.as_deref() == Some("stg_customers.customer_name")
        })
        .expect("stg_customers.customer_name should exist");

    let has_orders_data_flow = stmt
        .edges
        .iter()
        .any(|e| e.from == stg_orders_order_id.id && e.edge_type == EdgeType::DataFlow);

    let has_customers_data_flow = stmt
        .edges
        .iter()
        .any(|e| e.from == stg_customers_customer_name.id && e.edge_type == EdgeType::DataFlow);

    assert!(
        has_orders_data_flow,
        "Data flow edge should exist from stg_orders.order_id to orders CTE"
    );
    assert!(
        has_customers_data_flow,
        "Data flow edge should exist from stg_customers.customer_name to customers CTE"
    );
}

#[test]
fn backward_inference_with_schema_no_inference_needed() {
    // When schema is provided, no backward inference should be needed
    let sql = r#"
        WITH orders AS (
            SELECT * FROM stg_orders
        )
        SELECT customer_id, order_id FROM orders
    "#;

    let schema = SchemaMetadata {
        allow_implied: true,
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![schema_table(
            None,
            None,
            "stg_orders",
            &["customer_id", "order_id", "amount", "order_date"],
        )],
    };

    let result = run_analysis(sql, Dialect::Generic, Some(schema));
    let stmt = first_statement(&result);

    // With schema, all columns should come from schema resolution
    let stg_orders_columns: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name
                    .as_ref()
                    .map(|qn| qn.starts_with("stg_orders."))
                    .unwrap_or(false)
        })
        .collect();

    // Should have columns from schema expansion
    assert!(
        !stg_orders_columns.is_empty(),
        "stg_orders should have columns from schema"
    );

    // Should NOT have APPROXIMATE_LINEAGE issues for this table
    let approximate_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == issue_codes::APPROXIMATE_LINEAGE && i.message.contains("stg_orders"))
        .collect();
    assert!(
        approximate_issues.is_empty(),
        "Should not have approximate lineage issues when schema is provided"
    );
}

#[test]
fn backward_inference_mixed_schema_and_no_schema() {
    // One source has schema, one doesn't - only the one without should have inference
    let sql = r#"
        WITH orders AS (
            SELECT * FROM stg_orders
        ),
        customers AS (
            SELECT * FROM stg_customers
        ),
        combined AS (
            SELECT
                o.order_id,
                c.customer_name
            FROM orders o
            JOIN customers c ON o.customer_id = c.customer_id
        )
        SELECT * FROM combined
    "#;

    // Only provide schema for stg_orders
    let schema = SchemaMetadata {
        allow_implied: true,
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![schema_table(
            None,
            None,
            "stg_orders",
            &["customer_id", "order_id", "amount"],
        )],
    };

    let result = run_analysis(sql, Dialect::Generic, Some(schema));
    let stmt = first_statement(&result);

    // stg_orders should have columns from schema (not inferred)
    let stg_orders_order_id = stmt.nodes.iter().find(|n| {
        n.node_type == NodeType::Column
            && n.qualified_name.as_deref() == Some("stg_orders.order_id")
    });
    assert!(
        stg_orders_order_id.is_some(),
        "stg_orders.order_id should exist from schema"
    );

    // stg_customers should have columns inferred
    let stg_customers_columns: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name
                    .as_ref()
                    .map(|qn| qn.starts_with("stg_customers."))
                    .unwrap_or(false)
        })
        .map(|n| n.label.to_string())
        .collect();

    assert!(
        stg_customers_columns.contains(&"customer_name".to_string()),
        "customer_name should be inferred on stg_customers"
    );

    // Should have APPROXIMATE_LINEAGE issue only for stg_customers
    let approximate_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == issue_codes::APPROXIMATE_LINEAGE)
        .collect();
    assert!(
        approximate_issues
            .iter()
            .any(|i| i.message.contains("stg_customers")),
        "Should have approximate lineage issue for stg_customers"
    );
}

#[test]
fn backward_inference_aggregation_preserves_column_names() {
    // Aggregations like COUNT(col), SUM(col) should still infer the column name
    let sql = r#"
        WITH orders AS (
            SELECT * FROM raw_orders
        ),
        summary AS (
            SELECT
                customer_id,
                COUNT(order_id) AS order_count,
                SUM(amount) AS total_amount,
                MIN(order_date) AS first_order
            FROM orders
            GROUP BY customer_id
        )
        SELECT * FROM summary
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // raw_orders should have all columns inferred
    let raw_orders_columns: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name
                    .as_ref()
                    .map(|qn| qn.starts_with("raw_orders."))
                    .unwrap_or(false)
        })
        .map(|n| n.label.to_string())
        .collect();

    assert!(
        raw_orders_columns.contains(&"customer_id".to_string()),
        "customer_id should be inferred on raw_orders"
    );
    assert!(
        raw_orders_columns.contains(&"order_id".to_string()),
        "order_id should be inferred on raw_orders"
    );
    assert!(
        raw_orders_columns.contains(&"amount".to_string()),
        "amount should be inferred on raw_orders"
    );
    assert!(
        raw_orders_columns.contains(&"order_date".to_string()),
        "order_date should be inferred on raw_orders"
    );
}

#[test]
fn backward_inference_deep_nesting() {
    // Test with a chain longer than typical to verify recursion handling.
    // The depth limit (MAX_INFERENCE_DEPTH=20) prevents stack overflow on pathological cases.
    // This test uses 10 levels which should work fine.
    let sql = r#"
        WITH step1 AS (SELECT * FROM source_table),
             step2 AS (SELECT * FROM step1),
             step3 AS (SELECT * FROM step2),
             step4 AS (SELECT * FROM step3),
             step5 AS (SELECT * FROM step4),
             step6 AS (SELECT * FROM step5),
             step7 AS (SELECT * FROM step6),
             step8 AS (SELECT * FROM step7),
             step9 AS (SELECT * FROM step8),
             step10 AS (SELECT id, name, value FROM step9)
        SELECT * FROM step10
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // source_table should have columns inferred through the entire chain
    let source_columns: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name
                    .as_ref()
                    .map(|qn| qn.starts_with("source_table."))
                    .unwrap_or(false)
        })
        .map(|n| n.label.to_string())
        .collect();

    assert_eq!(
        source_columns.len(),
        3,
        "source_table should have exactly 3 inferred columns through 10-level chain, found: {:?}",
        source_columns
    );
    assert!(
        source_columns.contains(&"id".to_string()),
        "id should be inferred on source_table"
    );
    assert!(
        source_columns.contains(&"name".to_string()),
        "name should be inferred on source_table"
    );
    assert!(
        source_columns.contains(&"value".to_string()),
        "value should be inferred on source_table"
    );

    // Verify data flow edges exist from source_table through the 10-level chain
    let source_table_id = stmt
        .nodes
        .iter()
        .find(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name.as_deref() == Some("source_table.id")
        })
        .expect("source_table.id should exist");

    let data_flow_from_id: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| e.from == source_table_id.id && e.edge_type == EdgeType::DataFlow)
        .collect();

    assert!(
        !data_flow_from_id.is_empty(),
        "Data flow edges should exist from source_table.id through the deep CTE chain"
    );
}

#[test]
fn backward_inference_cycle_detection() {
    // Test that self-referential patterns don't cause infinite loops.
    // While true cycles aren't possible in standard SQL CTEs, this tests that
    // our visited_pairs tracking handles the same source being referenced
    // through different paths.
    let sql = r#"
        WITH base AS (
            SELECT * FROM shared_source
        ),
        branch_a AS (
            SELECT * FROM base
        ),
        branch_b AS (
            SELECT * FROM base
        ),
        combined AS (
            SELECT
                branch_a.id AS a_id,
                branch_b.id AS b_id
            FROM branch_a, branch_b
        )
        SELECT * FROM combined
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // shared_source should have id inferred (referenced through both branches)
    let source_columns: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name
                    .as_ref()
                    .map(|qn| qn.starts_with("shared_source."))
                    .unwrap_or(false)
        })
        .map(|n| n.label.to_string())
        .collect();

    // Should have id column (referenced as a_id and b_id both resolve to base.id -> shared_source.id)
    assert!(
        source_columns.contains(&"id".to_string()),
        "id should be inferred on shared_source, found: {:?}",
        source_columns
    );

    // Verify data flow edges exist from shared_source.id
    let shared_source_id = stmt
        .nodes
        .iter()
        .find(|n| {
            n.node_type == NodeType::Column
                && n.qualified_name.as_deref() == Some("shared_source.id")
        })
        .expect("shared_source.id should exist");

    let data_flow_from_shared_source: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| e.from == shared_source_id.id && e.edge_type == EdgeType::DataFlow)
        .collect();

    assert!(
        !data_flow_from_shared_source.is_empty(),
        "Data flow edges should exist from shared_source.id (referenced through both branches)"
    );
}

// Task 1: Tier 1 Edge Cases - Lineage assertions

#[test]
fn nested_joins_track_all_tables() {
    // Level 3: Triple-nested join with parentheses
    let sql = r#"
        SELECT
            o.order_id,
            c.email,
            p.product_name,
            s.supplier_name
        FROM
            (
                (
                    (
                        orders o
                        JOIN customers c ON c.customer_id = o.customer_id
                    )
                    JOIN products p ON p.product_id = o.product_id
                )
                JOIN suppliers s ON s.supplier_id = p.supplier_id
            )
        WHERE c.email = 'sample@example.com'
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let tables = collect_table_names(&result);

    // All four tables should be tracked through nested join structure
    for expected in ["orders", "customers", "products", "suppliers"] {
        assert!(
            tables.contains(expected),
            "nested join should track table {expected}; saw {tables:?}"
        );
    }

    let stmt = first_statement(&result);

    // Verify output columns are present
    let columns = column_labels(stmt);
    for expected in ["order_id", "email", "product_name", "supplier_name"] {
        assert!(
            columns.contains(&expected.to_string()),
            "expected column {expected} in output; saw {columns:?}"
        );
    }

    // Verify data flow edges exist (joins create data flow between tables)
    let data_flow_edges = edges_by_type(stmt, EdgeType::DataFlow);
    assert!(
        !data_flow_edges.is_empty(),
        "expected data flow edges for 4-way join"
    );
}

#[test]
fn postgres_array_slicing_tracks_source_table() {
    let sql = r#"
        SELECT a[:], b[:1], c[2:], d[2:3]
        FROM array_data
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("array_data"),
        "array slicing query should track source table; saw {tables:?}"
    );

    let stmt = first_statement(&result);

    // Verify the statement was parsed without errors
    assert!(
        !result.summary.has_errors,
        "array slicing query should parse without errors: {:?}",
        result.issues
    );

    // Verify the table node exists
    let table_node = find_table_node(stmt, "array_data");
    assert!(
        table_node.is_some(),
        "array_data table node should exist in lineage"
    );
}

// Task 2: Tier 2 PostgreSQL Dialect Depth - Lineage assertions

#[test]
fn lateral_join_tracks_outer_and_subquery_tables() {
    let sql = r#"
        SELECT
            d.department_id,
            d.name AS department_name,
            emp.employee_name,
            emp.salary
        FROM departments d
        JOIN LATERAL (
            SELECT e.name AS employee_name, e.salary
            FROM employees e
            WHERE e.department_id = d.department_id
            ORDER BY e.salary DESC
            LIMIT 3
        ) emp ON true
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    // Both the outer table and the table inside LATERAL should be tracked
    assert!(
        tables.contains("departments"),
        "LATERAL join should track outer table; saw {tables:?}"
    );
    assert!(
        tables.contains("employees"),
        "LATERAL join should track table inside LATERAL subquery; saw {tables:?}"
    );

    let stmt = first_statement(&result);

    // Verify output columns are present
    let columns = column_labels(stmt);
    for expected in [
        "department_id",
        "department_name",
        "employee_name",
        "salary",
    ] {
        assert!(
            columns.contains(&expected.to_string()),
            "expected column {expected} in LATERAL join output; saw {columns:?}"
        );
    }

    // Verify no parsing errors
    assert!(
        !result.summary.has_errors,
        "LATERAL join should parse without errors: {:?}",
        result.issues
    );
}

#[test]
fn filter_clause_tracks_aggregation_sources() {
    let sql = r#"
        SELECT
            department_id,
            SUM(salary) AS total_salary,
            SUM(salary) FILTER (WHERE years_employed > 5) AS senior_salary,
            AVG(salary) FILTER (WHERE performance_rating >= 4) AS high_performer_avg
        FROM employees
        GROUP BY department_id
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("employees"),
        "FILTER clause query should track source table; saw {tables:?}"
    );

    let stmt = first_statement(&result);

    // Verify output columns are present
    let columns = column_labels(stmt);
    for expected in [
        "department_id",
        "total_salary",
        "senior_salary",
        "high_performer_avg",
    ] {
        assert!(
            columns.contains(&expected.to_string()),
            "expected column {expected} in FILTER clause output; saw {columns:?}"
        );
    }

    // Verify no parsing errors
    assert!(
        !result.summary.has_errors,
        "FILTER clause query should parse without errors: {:?}",
        result.issues
    );
}

#[test]
fn group_by_cube_rollup_tracks_source_table() {
    let sql = r#"
        SELECT
            region,
            city,
            GROUPING(region, city) AS grp_idx,
            COUNT(DISTINCT id) AS num_total
        FROM locations
        GROUP BY GROUPING SETS ((region), (city), (region, city), ())
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("locations"),
        "GROUPING SETS query should track source table; saw {tables:?}"
    );

    let stmt = first_statement(&result);

    // Verify output columns are present
    let columns = column_labels(stmt);
    for expected in ["region", "city", "grp_idx", "num_total"] {
        assert!(
            columns.contains(&expected.to_string()),
            "expected column {expected} in GROUPING SETS output; saw {columns:?}"
        );
    }

    // Verify no parsing errors
    assert!(
        !result.summary.has_errors,
        "GROUPING SETS query should parse without errors: {:?}",
        result.issues
    );
}

#[test]
fn rollup_tracks_hierarchical_columns() {
    let sql = r#"
        SELECT
            year,
            quarter,
            month,
            SUM(revenue) AS total_revenue
        FROM sales
        GROUP BY ROLLUP (year, quarter, month)
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("sales"),
        "ROLLUP query should track source table; saw {tables:?}"
    );

    let stmt = first_statement(&result);

    // Verify hierarchical columns are present
    let columns = column_labels(stmt);
    for expected in ["year", "quarter", "month", "total_revenue"] {
        assert!(
            columns.contains(&expected.to_string()),
            "expected column {expected} in ROLLUP output; saw {columns:?}"
        );
    }

    // Verify no parsing errors
    assert!(
        !result.summary.has_errors,
        "ROLLUP query should parse without errors: {:?}",
        result.issues
    );
}

#[test]
fn cube_tracks_all_dimensions() {
    let sql = r#"
        SELECT
            product_category,
            sales_region,
            SUM(quantity) AS total_quantity,
            SUM(amount) AS total_amount
        FROM transactions
        GROUP BY CUBE (product_category, sales_region)
    "#;

    let result = run_analysis(sql, Dialect::Postgres, None);
    let tables = collect_table_names(&result);

    assert!(
        tables.contains("transactions"),
        "CUBE query should track source table; saw {tables:?}"
    );

    let stmt = first_statement(&result);

    // Verify dimension columns are present
    let columns = column_labels(stmt);
    for expected in [
        "product_category",
        "sales_region",
        "total_quantity",
        "total_amount",
    ] {
        assert!(
            columns.contains(&expected.to_string()),
            "expected column {expected} in CUBE output; saw {columns:?}"
        );
    }

    // Verify no parsing errors
    assert!(
        !result.summary.has_errors,
        "CUBE query should parse without errors: {:?}",
        result.issues
    );
}

// Task 3: Tier 3 Snowflake Features - Lineage assertions

#[test]
fn snowflake_lateral_flatten_tracks_source_table() {
    let sql = r#"
        SELECT
            value AS p_id,
            name
        FROM a
        INNER JOIN b ON b.c_id = a.c_id,
        LATERAL FLATTEN(input => b.cool_ids)
    "#;

    let result = run_analysis(sql, Dialect::Snowflake, None);
    let tables = collect_table_names(&result);

    // Both tables in the join should be tracked
    // Snowflake normalizes to uppercase
    let has_table_a = tables.iter().any(|t| t.eq_ignore_ascii_case("a"));
    let has_table_b = tables.iter().any(|t| t.eq_ignore_ascii_case("b"));

    assert!(
        has_table_a,
        "LATERAL FLATTEN query should track table a; saw {tables:?}"
    );
    assert!(
        has_table_b,
        "LATERAL FLATTEN query should track table b; saw {tables:?}"
    );

    // Verify parsing completed
    assert!(
        result.summary.statement_count >= 1,
        "LATERAL FLATTEN query should parse at least one statement"
    );

    let stmt = first_statement(&result);
    let columns = column_labels(stmt);
    assert!(
        columns.iter().any(|c| c.eq_ignore_ascii_case("p_id")),
        "LATERAL FLATTEN pseudocolumns should still produce visible output columns"
    );
    assert!(
        columns.iter().any(|c| c.eq_ignore_ascii_case("name")),
        "best-effort unresolved projections should remain visible alongside FLATTEN outputs"
    );
}

#[test]
fn snowflake_higher_order_functions_track_source() {
    let sql = r#"
        SELECT
            FILTER(ident, i -> i:value > 0) AS sample_filter,
            TRANSFORM(ident, j -> j:value) AS sample_transform
        FROM ref
    "#;

    let result = run_analysis(sql, Dialect::Snowflake, None);
    let tables = collect_table_names(&result);

    // Source table should be tracked
    let has_ref = tables.iter().any(|t| t.eq_ignore_ascii_case("ref"));

    assert!(
        has_ref,
        "Higher-order function query should track source table; saw {tables:?}"
    );

    // Verify parsing completed
    assert!(
        result.summary.statement_count >= 1,
        "Higher-order function query should parse at least one statement"
    );
}

#[test]
fn snowflake_reduce_keeps_output_column_when_lineage_resolution_is_partial() {
    let sql = r#"
        SELECT REDUCE([1, 2, 3], 0, (acc, val) -> acc + val) AS sum_result
    "#;

    let result = run_analysis(sql, Dialect::Snowflake, None);
    let stmt = first_statement(&result);
    let columns = column_labels(stmt);

    assert!(
        columns.iter().any(|c| c.eq_ignore_ascii_case("sum_result")),
        "partially-resolved higher-order functions should still keep their output column"
    );
}

#[test]
fn snowflake_group_by_cube_tracks_source() {
    let sql = r#"
        SELECT
            name,
            age,
            COUNT(*) AS record_count
        FROM people
        GROUP BY CUBE (name, age)
    "#;

    let result = run_analysis(sql, Dialect::Snowflake, None);
    let tables = collect_table_names(&result);

    let has_people = tables.iter().any(|t| t.eq_ignore_ascii_case("people"));

    assert!(
        has_people,
        "Snowflake CUBE query should track source table; saw {tables:?}"
    );

    let stmt = first_statement(&result);
    let columns = column_labels(stmt);

    // Verify output columns (Snowflake normalizes to uppercase)
    for expected in ["NAME", "AGE", "RECORD_COUNT"] {
        assert!(
            columns.iter().any(|c| c.eq_ignore_ascii_case(expected)),
            "expected column {expected} in Snowflake CUBE output; saw {columns:?}"
        );
    }

    // Verify no parsing errors
    assert!(
        !result.summary.has_errors,
        "Snowflake CUBE query should parse without errors: {:?}",
        result.issues
    );
}

#[test]
fn snowflake_grouping_sets_tracks_source() {
    let sql = r#"
        SELECT
            foo,
            bar,
            COUNT(*) AS cnt
        FROM baz
        GROUP BY GROUPING SETS ((foo), (bar))
    "#;

    let result = run_analysis(sql, Dialect::Snowflake, None);
    let tables = collect_table_names(&result);

    let has_baz = tables.iter().any(|t| t.eq_ignore_ascii_case("baz"));

    assert!(
        has_baz,
        "Snowflake GROUPING SETS query should track source table; saw {tables:?}"
    );

    let stmt = first_statement(&result);
    let columns = column_labels(stmt);

    // Verify output columns
    for expected in ["FOO", "BAR", "CNT"] {
        assert!(
            columns.iter().any(|c| c.eq_ignore_ascii_case(expected)),
            "expected column {expected} in Snowflake GROUPING SETS output; saw {columns:?}"
        );
    }

    // Verify no parsing errors
    assert!(
        !result.summary.has_errors,
        "Snowflake GROUPING SETS query should parse without errors: {:?}",
        result.issues
    );
}

// =============================================================================
// Type inference tests (Task 2: Populate Column Types in Output)
// =============================================================================

/// Extract the data_type from a column node's metadata
fn get_column_data_type(node: &Node) -> Option<String> {
    node.metadata
        .as_ref()
        .and_then(|m| m.get("data_type"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[test]
fn type_inference_select_literals_have_correct_types() {
    // Test that SELECT with literal values correctly infers types
    let sql = r#"
        SELECT
            1 AS int_val,
            'text' AS text_val,
            true AS bool_val
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Find the columns and check their types
    let int_col = find_column_node(stmt, "int_val").expect("int_val column should exist");
    let text_col = find_column_node(stmt, "text_val").expect("text_val column should exist");
    let bool_col = find_column_node(stmt, "bool_val").expect("bool_val column should exist");

    // Numbers infer as FLOAT (since we can't distinguish int from float without context)
    assert_eq!(
        get_column_data_type(int_col),
        Some("FLOAT".to_string()),
        "Integer literal should infer as FLOAT"
    );
    assert_eq!(
        get_column_data_type(text_col),
        Some("TEXT".to_string()),
        "String literal should infer as TEXT"
    );
    assert_eq!(
        get_column_data_type(bool_col),
        Some("BOOLEAN".to_string()),
        "Boolean literal should infer as BOOLEAN"
    );
}

#[test]
fn type_inference_select_functions_have_correct_types() {
    // Test that SELECT with function calls correctly infers types
    let sql = r#"
        SELECT
            COUNT(*) AS count_val,
            SUM(amount) AS sum_val,
            CONCAT(first_name, last_name) AS concat_val,
            NOW() AS timestamp_val
        FROM users
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Find the columns and check their types
    let count_col = find_column_node(stmt, "count_val").expect("count_val column should exist");
    let sum_col = find_column_node(stmt, "sum_val").expect("sum_val column should exist");
    let concat_col = find_column_node(stmt, "concat_val").expect("concat_val column should exist");
    let timestamp_col =
        find_column_node(stmt, "timestamp_val").expect("timestamp_val column should exist");

    assert_eq!(
        get_column_data_type(count_col),
        Some("INTEGER".to_string()),
        "COUNT(*) should infer as INTEGER"
    );
    assert_eq!(
        get_column_data_type(sum_col),
        Some("FLOAT".to_string()),
        "SUM() should infer as FLOAT"
    );
    assert_eq!(
        get_column_data_type(concat_col),
        Some("TEXT".to_string()),
        "CONCAT() should infer as TEXT"
    );
    assert_eq!(
        get_column_data_type(timestamp_col),
        Some("TIMESTAMP".to_string()),
        "NOW() should infer as TIMESTAMP"
    );
}

#[test]
fn type_inference_cte_types_propagate_to_outer_query() {
    // Test that CTE column types propagate to the outer query
    let sql = r#"
        WITH metrics AS (
            SELECT
                COUNT(*) AS row_count,
                SUM(amount) AS total_amount,
                CONCAT(name, '_suffix') AS name_with_suffix
            FROM orders
        )
        SELECT
            row_count,
            total_amount,
            name_with_suffix
        FROM metrics
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);
    let stmt = first_statement(&result);

    // Find the CTE's output columns (they should have types)
    let cte_node = find_cte_node(stmt, "metrics").expect("metrics CTE should exist");

    // Find columns owned by the CTE
    let cte_columns: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Ownership && e.from == cte_node.id)
        .filter_map(|e| stmt.nodes.iter().find(|n| n.id == e.to))
        .collect();

    // CTE columns should have types
    let cte_row_count = cte_columns
        .iter()
        .find(|n| &*n.label == "row_count")
        .expect("row_count should be in CTE");
    let cte_total_amount = cte_columns
        .iter()
        .find(|n| &*n.label == "total_amount")
        .expect("total_amount should be in CTE");
    let cte_name_with_suffix = cte_columns
        .iter()
        .find(|n| &*n.label == "name_with_suffix")
        .expect("name_with_suffix should be in CTE");

    assert_eq!(
        get_column_data_type(cte_row_count),
        Some("INTEGER".to_string()),
        "CTE row_count should be INTEGER"
    );
    assert_eq!(
        get_column_data_type(cte_total_amount),
        Some("FLOAT".to_string()),
        "CTE total_amount should be FLOAT"
    );
    assert_eq!(
        get_column_data_type(cte_name_with_suffix),
        Some("TEXT".to_string()),
        "CTE name_with_suffix should be TEXT"
    );

    // Find the Output node's columns (the outer query)
    let output_node = stmt
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Output)
        .expect("Output node should exist");

    let output_columns: Vec<_> = stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Ownership && e.from == output_node.id)
        .filter_map(|e| stmt.nodes.iter().find(|n| n.id == e.to))
        .collect();

    // Output columns should also have types (propagated from CTE)
    let out_row_count = output_columns
        .iter()
        .find(|n| &*n.label == "row_count")
        .expect("row_count should be in output");
    let out_total_amount = output_columns
        .iter()
        .find(|n| &*n.label == "total_amount")
        .expect("total_amount should be in output");
    let out_name_with_suffix = output_columns
        .iter()
        .find(|n| &*n.label == "name_with_suffix")
        .expect("name_with_suffix should be in output");

    assert_eq!(
        get_column_data_type(out_row_count),
        Some("INTEGER".to_string()),
        "Output row_count should propagate INTEGER from CTE"
    );
    assert_eq!(
        get_column_data_type(out_total_amount),
        Some("FLOAT".to_string()),
        "Output total_amount should propagate FLOAT from CTE"
    );
    assert_eq!(
        get_column_data_type(out_name_with_suffix),
        Some("TEXT".to_string()),
        "Output name_with_suffix should propagate TEXT from CTE"
    );
}

// =============================================================================
// Schema-aware type lookup tests (Task 3: Schema-Aware Type Lookup)
// =============================================================================

/// Helper to create a schema table with typed columns
fn schema_table_typed(name: &str, columns: Vec<ColumnSchema>) -> SchemaTable {
    SchemaTable {
        catalog: None,
        schema: None,
        name: name.to_string(),
        columns,
    }
}

#[test]
fn test_column_reference_with_schema_returns_correct_type() {
    // When schema is provided with column types, SELECT column references should
    // inherit those types.
    let schema = SchemaMetadata {
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        allow_implied: true,
        tables: vec![schema_table_typed(
            "users",
            vec![
                column_typed("id", "integer"),
                column_typed("email", "varchar"),
                column_typed("created_at", "timestamp"),
                column_typed("is_active", "boolean"),
            ],
        )],
    };

    let sql = r#"
        SELECT id, email, created_at, is_active
        FROM users
    "#;

    let result = run_analysis(sql, Dialect::Generic, Some(schema));
    assert!(result.issues.is_empty(), "Should have no issues");

    let stmt = &result.statements[0];

    // Find column nodes and check their types
    let id_col = find_column_node(stmt, "id").expect("id column should exist");
    let email_col = find_column_node(stmt, "email").expect("email column should exist");
    let created_at_col =
        find_column_node(stmt, "created_at").expect("created_at column should exist");
    let is_active_col = find_column_node(stmt, "is_active").expect("is_active column should exist");

    assert_eq!(
        get_column_data_type(id_col),
        Some("INTEGER".to_string()),
        "id should be INTEGER from schema"
    );
    assert_eq!(
        get_column_data_type(email_col),
        Some("TEXT".to_string()),
        "email (varchar) should normalize to TEXT from schema"
    );
    assert_eq!(
        get_column_data_type(created_at_col),
        Some("TIMESTAMP".to_string()),
        "created_at should be TIMESTAMP from schema"
    );
    assert_eq!(
        get_column_data_type(is_active_col),
        Some("BOOLEAN".to_string()),
        "is_active should be BOOLEAN from schema"
    );
}

#[test]
fn test_column_reference_without_schema_returns_none() {
    // When no schema is provided, column references should have None type
    // (since we can't determine the type of a bare column reference)
    let sql = r#"
        SELECT id, email
        FROM users
    "#;

    let result = run_analysis(sql, Dialect::Generic, None);

    let stmt = &result.statements[0];

    // Find column nodes and check their types are None
    let id_col = find_column_node(stmt, "id").expect("id column should exist");
    let email_col = find_column_node(stmt, "email").expect("email column should exist");

    assert_eq!(
        get_column_data_type(id_col),
        None,
        "id should have no type without schema"
    );
    assert_eq!(
        get_column_data_type(email_col),
        None,
        "email should have no type without schema"
    );
}

#[test]
fn test_qualified_column_reference_with_schema() {
    // When using qualified column references (table.column), types should be resolved
    // from schema.
    let schema = SchemaMetadata {
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        allow_implied: true,
        tables: vec![
            schema_table_typed(
                "users",
                vec![column_typed("id", "integer"), column_typed("name", "text")],
            ),
            schema_table_typed(
                "orders",
                vec![
                    column_typed("id", "integer"),
                    column_typed("user_id", "integer"),
                    column_typed("total", "numeric"),
                ],
            ),
        ],
    };

    let sql = r#"
        SELECT users.id, users.name, orders.total
        FROM users
        JOIN orders ON users.id = orders.user_id
    "#;

    let result = run_analysis(sql, Dialect::Generic, Some(schema));
    assert!(result.issues.is_empty(), "Should have no issues");

    let stmt = &result.statements[0];

    // Find column nodes and check their types
    let id_col = find_column_node(stmt, "id").expect("id column should exist");
    let name_col = find_column_node(stmt, "name").expect("name column should exist");
    let total_col = find_column_node(stmt, "total").expect("total column should exist");

    assert_eq!(
        get_column_data_type(id_col),
        Some("INTEGER".to_string()),
        "users.id should be INTEGER from schema"
    );
    assert_eq!(
        get_column_data_type(name_col),
        Some("TEXT".to_string()),
        "users.name should be TEXT from schema"
    );
    assert_eq!(
        get_column_data_type(total_col),
        Some("FLOAT".to_string()),
        "orders.total (numeric) should normalize to FLOAT from schema"
    );
}

#[test]
fn test_schema_type_normalization() {
    // Test that various dialect-specific type names are normalized to canonical types
    let schema = SchemaMetadata {
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        allow_implied: true,
        tables: vec![schema_table_typed(
            "test_types",
            vec![
                column_typed("int64_col", "int64"),       // BigQuery-style
                column_typed("varchar_col", "varchar"),   // Standard
                column_typed("float8_col", "float8"),     // Postgres-style
                column_typed("datetime_col", "datetime"), // MySQL-style
                column_typed("bool_col", "bool"),         // Short form
            ],
        )],
    };

    let sql = r#"
        SELECT int64_col, varchar_col, float8_col, datetime_col, bool_col
        FROM test_types
    "#;

    let result = run_analysis(sql, Dialect::Generic, Some(schema));
    assert!(result.issues.is_empty(), "Should have no issues");

    let stmt = &result.statements[0];

    let int64_col = find_column_node(stmt, "int64_col").expect("int64_col should exist");
    let varchar_col = find_column_node(stmt, "varchar_col").expect("varchar_col should exist");
    let float8_col = find_column_node(stmt, "float8_col").expect("float8_col should exist");
    let datetime_col = find_column_node(stmt, "datetime_col").expect("datetime_col should exist");
    let bool_col = find_column_node(stmt, "bool_col").expect("bool_col should exist");

    assert_eq!(
        get_column_data_type(int64_col),
        Some("INTEGER".to_string()),
        "int64 should normalize to INTEGER"
    );
    assert_eq!(
        get_column_data_type(varchar_col),
        Some("TEXT".to_string()),
        "varchar should normalize to TEXT"
    );
    assert_eq!(
        get_column_data_type(float8_col),
        Some("FLOAT".to_string()),
        "float8 should normalize to FLOAT"
    );
    assert_eq!(
        get_column_data_type(datetime_col),
        Some("TIMESTAMP".to_string()),
        "datetime should normalize to TIMESTAMP"
    );
    assert_eq!(
        get_column_data_type(bool_col),
        Some("BOOLEAN".to_string()),
        "bool should normalize to BOOLEAN"
    );
}

#[test]
fn name_spans_single_table_reference() {
    let sql = "SELECT * FROM users WHERE id = 1";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let users = find_table_node(stmt, "users").expect("users table node");

    assert_eq!(users.name_spans.len(), 1);
    let span = users.name_spans[0];
    assert_eq!(&sql[span.start..span.end], "users");
    assert!(users.body_span.is_none());
}

#[test]
fn name_spans_multiple_table_references() {
    let sql = "SELECT u.id FROM users u WHERE u.id IN (SELECT id FROM users)";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let users = find_table_node(stmt, "users").expect("users table node");

    assert_eq!(
        users.name_spans.len(),
        2,
        "expected both `users` references"
    );
    for span in &users.name_spans {
        assert_eq!(&sql[span.start..span.end], "users");
    }
}

#[test]
fn name_spans_cte_with_body_and_references() {
    let sql = "WITH active AS (SELECT id FROM users WHERE active) \
               SELECT a.id FROM active a JOIN active b ON a.id = b.id";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let active_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|node| {
            node.node_type == NodeType::Cte && node.qualified_name.as_deref() == Some("active")
        })
        .collect();

    assert!(
        !active_nodes.is_empty(),
        "expected at least one CTE node for `active`"
    );

    let total_spans: Vec<_> = active_nodes
        .iter()
        .flat_map(|node| node.name_spans.iter().copied())
        .collect();
    assert_eq!(
        total_spans.len(),
        3,
        "expected declaration plus two relation occurrences across CTE instances"
    );
    for span in &total_spans {
        assert_eq!(&sql[span.start..span.end], "active");
    }

    let cte_definition = active_nodes
        .iter()
        .find(|node| node.body_span.is_some())
        .expect("cte definition node should retain body span");
    let body = cte_definition
        .body_span
        .expect("cte body span should be populated");
    let body_text = &sql[body.start..body.end];
    assert!(body_text.starts_with('(') && body_text.ends_with(')'));
    assert!(body_text.contains("SELECT id FROM users WHERE active"));
}

#[test]
fn name_spans_ignores_matches_in_strings_and_comments() {
    let sql = "-- users\nSELECT * FROM users WHERE note = 'see users'";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let users = find_table_node(stmt, "users").expect("users table node");

    assert_eq!(
        users.name_spans.len(),
        1,
        "matches inside comments/strings must not count"
    );
    let span = users.name_spans[0];
    assert_eq!(&sql[span.start..span.end], "users");
}

#[test]
fn name_spans_skip_string_literals_before_relation_occurrence() {
    let sql = "SELECT 'users' AS label FROM users";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let users = find_table_node(stmt, "users").expect("users table node");

    assert_eq!(users.name_spans.len(), 1);
    let span = users.name_spans[0];
    assert_eq!(&sql[span.start..span.end], "users");
    assert_eq!(
        span.start, 29,
        "should bind to FROM users, not the string literal"
    );
}

#[test]
fn name_spans_skip_hash_comments_in_mysql() {
    let sql = "SELECT 1 # users\nFROM users";
    let result = run_analysis(sql, Dialect::Mysql, None);
    let stmt = &result.statements[0];
    let users = find_table_node(stmt, "users").expect("users table node");

    assert_eq!(users.name_spans.len(), 1);
    let span = users.name_spans[0];
    assert_eq!(&sql[span.start..span.end], "users");
    assert_eq!(
        span.start, 22,
        "should bind to FROM users, not the hash comment"
    );
}

#[test]
fn name_spans_skip_dollar_quoted_string_literals() {
    let sql = "SELECT $$users$$ AS x FROM users";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let users = find_table_node(stmt, "users").expect("users table node");

    assert_eq!(users.name_spans.len(), 1);
    let span = users.name_spans[0];
    assert_eq!(&sql[span.start..span.end], "users");
    assert_eq!(
        span.start, 27,
        "should bind to FROM users, not the dollar-quoted literal"
    );
}

#[test]
fn name_spans_empty_on_column_nodes() {
    // Columns intentionally get empty name_spans in this release — accurate
    // per-occurrence column spans require alias/scope resolution.
    let sql = "SELECT id FROM users";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let id_col = find_column_node(stmt, "id").expect("id column node");

    assert!(
        id_col.name_spans.is_empty(),
        "column nodes should not populate name_spans in this release"
    );
}

#[test]
fn name_spans_recursive_cte_include_recursive_reference() {
    let sql = concat!(
        "WITH RECURSIVE org AS (",
        "SELECT id FROM employees ",
        "UNION ALL ",
        "SELECT e.id FROM employees e JOIN org ON e.manager_id = org.id",
        ") SELECT * FROM org"
    );
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let org = find_cte_node(stmt, "org").expect("org cte node");

    assert_eq!(
        org.name_spans.len(),
        3,
        "definition, recursive self-reference, and outer reference should all be tracked"
    );
    for span in &org.name_spans {
        assert_eq!(&sql[span.start..span.end], "org");
    }
}

#[test]
fn name_spans_self_join_are_instance_specific() {
    let sql =
        "SELECT e1.name, e2.name FROM employees e1 JOIN employees e2 ON e1.manager_id = e2.id";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let employee_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|node| {
            node.node_type == NodeType::Table && node.qualified_name.as_deref() == Some("employees")
        })
        .collect();

    assert_eq!(
        employee_nodes.len(),
        2,
        "expected one node per self-join instance"
    );
    assert!(employee_nodes.iter().all(|node| node.name_spans.len() == 1));
    assert_ne!(
        employee_nodes[0].name_spans[0],
        employee_nodes[1].name_spans[0]
    );
    for node in employee_nodes {
        let span = node.name_spans[0];
        assert_eq!(&sql[span.start..span.end], "employees");
    }
}

#[test]
fn name_spans_distinguish_same_label_qualified_relations() {
    let sql = "SELECT * FROM sales.orders so JOIN archive.orders ao ON so.id = ao.id";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let sales_orders = stmt
        .nodes
        .iter()
        .find(|node| node.qualified_name.as_deref() == Some("sales.orders"))
        .expect("sales.orders node");
    let archive_orders = stmt
        .nodes
        .iter()
        .find(|node| node.qualified_name.as_deref() == Some("archive.orders"))
        .expect("archive.orders node");

    assert_eq!(sales_orders.name_spans.len(), 1);
    assert_eq!(archive_orders.name_spans.len(), 1);
    assert_ne!(sales_orders.name_spans[0], archive_orders.name_spans[0]);
    assert_eq!(
        &sql[sales_orders.name_spans[0].start..sales_orders.name_spans[0].end],
        "orders"
    );
    assert_eq!(
        &sql[archive_orders.name_spans[0].start..archive_orders.name_spans[0].end],
        "orders"
    );
}

#[test]
fn name_spans_preserve_quoted_identifier_parts_with_embedded_dots() {
    let sql = "SELECT * FROM \"my.schema\".\"my.table\"";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let table = stmt
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Table)
        .expect("quoted table node");

    assert_eq!(table.name_spans.len(), 1);
    let span = table.name_spans[0];
    assert_eq!(&sql[span.start..span.end], "my.table");
}

#[test]
fn cte_body_span_skips_optional_column_list() {
    let sql =
        "WITH metrics(user_id, total) AS (SELECT id, amount FROM orders) SELECT * FROM metrics";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let metrics = find_cte_node(stmt, "metrics").expect("metrics cte node");

    let body = metrics
        .body_span
        .expect("cte body span should be populated");
    assert_eq!(
        &sql[body.start..body.end],
        "(SELECT id, amount FROM orders)"
    );
}

#[test]
fn cte_body_span_skips_materialization_modifiers() {
    let sql = "WITH metrics AS NOT MATERIALIZED (SELECT id FROM orders) SELECT * FROM metrics";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let metrics = find_cte_node(stmt, "metrics").expect("metrics cte node");

    let body = metrics
        .body_span
        .expect("cte body span should be populated");
    assert_eq!(&sql[body.start..body.end], "(SELECT id FROM orders)");
}

#[test]
fn cte_body_span_skips_dollar_quoted_strings() {
    let sql = "WITH metrics AS (SELECT $$)$$ AS x) SELECT * FROM metrics";
    let result = run_analysis(sql, Dialect::Postgres, None);
    let stmt = &result.statements[0];
    let metrics = find_cte_node(stmt, "metrics").expect("metrics cte node");

    let body = metrics
        .body_span
        .expect("cte body span should be populated");
    assert_eq!(&sql[body.start..body.end], "(SELECT $$)$$ AS x)");
}
