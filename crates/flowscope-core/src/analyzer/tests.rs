use super::*;
use crate::test_utils::{load_schema_fixture, load_sql_fixture};
use crate::{
    types::{AnalysisOptions, LintConfidence, LintFallbackSource},
    LintConfig,
};
use std::collections::{BTreeSet, HashMap, HashSet};

fn make_request(sql: &str) -> AnalyzeRequest {
    AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: None,
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    }
}

fn make_request_with_options(
    sql: &str,
    hide_ctes: bool,
    enable_column_lineage: bool,
) -> AnalyzeRequest {
    let mut request = make_request(sql);
    request.options = Some(AnalysisOptions {
        enable_column_lineage: Some(enable_column_lineage),
        hide_ctes: Some(hide_ctes),
        ..Default::default()
    });
    request
}

fn schema_with_known_table() -> SchemaMetadata {
    SchemaMetadata {
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        tables: vec![SchemaTable {
            catalog: None,
            schema: None,
            name: "existing".to_string(),
            columns: Vec::new(),
        }],
        allow_implied: true,
    }
}

#[test]
fn test_simple_select() {
    let request = make_request("SELECT * FROM users");
    let result = analyze(&request);

    assert_eq!(result.statements.len(), 1);
    assert_eq!(result.statements[0].statement_type, "SELECT");
    // Expect 2 nodes: table + output
    assert_eq!(result.statements[0].nodes.len(), 2);
    let table_node = result.statements[0]
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Table)
        .expect("should have a table node");
    assert_eq!(&*table_node.label, "users");
    assert!(result.statements[0]
        .nodes
        .iter()
        .any(|n| n.node_type == NodeType::Output));
    assert!(!result.summary.has_errors);
}

#[test]
fn test_select_with_join() {
    let request = make_request("SELECT * FROM users u JOIN orders o ON u.id = o.user_id");
    let result = analyze(&request);

    assert_eq!(result.statements.len(), 1);
    // Expect 2 tables
    let tables: Vec<_> = result.statements[0]
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Table)
        .collect();
    assert_eq!(tables.len(), 2);
}

#[test]
fn test_ddl_create_table() {
    let request = make_request("CREATE TABLE test (id INT)");
    let result = analyze(&request);
    assert_eq!(result.statements[0].statement_type, "CREATE_TABLE");
}

#[test]
fn test_dml_insert() {
    let request = make_request("INSERT INTO test VALUES (1)");
    let result = analyze(&request);
    assert_eq!(result.statements[0].statement_type, "INSERT");
}

#[test]
fn ctas_edges_only_from_relations() {
    let request = make_request("CREATE TABLE tgt AS SELECT id FROM src");
    let result = analyze(&request);
    let statement = &result.statements[0];
    let target = statement
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Table && &*node.label == "tgt")
        .expect("target node");

    for edge in &statement.edges {
        if edge.edge_type == EdgeType::DataFlow && edge.to == target.id {
            let source = statement
                .nodes
                .iter()
                .find(|node| node.id == edge.from)
                .expect("source node");
            assert!(
                source.node_type.is_table_like(),
                "non relational node {:?} linked to CTAS target",
                source.node_type
            );
        }
    }
}

#[test]
fn spans_anchor_to_current_statement() {
    let sql = "SELECT 1 FROM missing;\nSELECT 1 FROM missing;";
    let mut request = make_request(sql);
    request.schema = Some(schema_with_known_table());
    let result = analyze(&request);

    let spans: Vec<Span> = result
        .issues
        .iter()
        .filter(|issue| issue.code == issue_codes::UNRESOLVED_REFERENCE)
        .filter_map(|issue| issue.span)
        .collect();

    assert_eq!(spans.len(), 2, "expected two unresolved reference spans");

    let first_pos = sql.find("missing").expect("first identifier");
    let second_pos = sql[first_pos + 1..]
        .find("missing")
        .map(|pos| pos + first_pos + 1)
        .expect("second identifier");

    assert!(spans.iter().any(|span| span.start == first_pos));
    assert!(spans.iter().any(|span| span.start == second_pos));
}

#[test]
fn cte_nodes_have_spans() {
    let sql = "WITH my_cte AS (SELECT 1) SELECT * FROM my_cte";
    let request = make_request(sql);
    let result = analyze(&request);

    let cte_node = result.statements[0]
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Cte && node.label.as_ref() == "my_cte")
        .expect("cte node");

    let span = cte_node.span.expect("cte span");
    assert_eq!(span, Span::new(5, 11));
    assert_eq!(&sql[span.start..span.end], "my_cte");
}

#[test]
fn multiple_cte_nodes_have_distinct_spans() {
    let sql = "WITH cte1 AS (SELECT 1), cte2 AS (SELECT 2) SELECT * FROM cte1, cte2";
    let request = make_request(sql);
    let result = analyze(&request);

    let mut cte_spans: HashMap<&str, Span> = HashMap::new();
    for node in &result.statements[0].nodes {
        if node.node_type == NodeType::Cte {
            cte_spans.insert(node.label.as_ref(), node.span.expect("cte span"));
        }
    }

    assert_eq!(cte_spans.get("cte1"), Some(&Span::new(5, 9)));
    assert_eq!(cte_spans.get("cte2"), Some(&Span::new(25, 29)));
}

#[test]
fn nested_cte_nodes_have_spans() {
    let sql = "WITH outer_cte AS (WITH inner_cte AS (SELECT 1) SELECT * FROM inner_cte) SELECT * FROM outer_cte";
    let request = make_request(sql);
    let result = analyze(&request);

    let statement = &result.statements[0];
    let outer_node = statement
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Cte && node.label.as_ref() == "outer_cte")
        .expect("outer cte node");
    let inner_node = statement
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Cte && node.label.as_ref() == "inner_cte")
        .expect("inner cte node");

    let outer_span = outer_node.span.expect("outer span");
    let inner_span = inner_node.span.expect("inner span");

    let outer_start = sql.find("outer_cte").expect("outer start");
    let inner_start = sql.find("inner_cte").expect("inner start");

    assert_eq!(outer_span.start, outer_start);
    assert_eq!(outer_span.end, outer_start + "outer_cte".len());
    assert_eq!(inner_span.start, inner_start);
    assert_eq!(inner_span.end, inner_start + "inner_cte".len());
}

#[test]
fn derived_table_nodes_have_spans() {
    let sql = "SELECT * FROM (SELECT 1) AS derived";
    let request = make_request(sql);
    let result = analyze(&request);

    let derived_node = result.statements[0]
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Cte && node.label.as_ref() == "derived")
        .expect("derived node");

    let span = derived_node.span.expect("derived span");
    assert_eq!(&sql[span.start..span.end], "derived");
}

#[test]
fn combined_cte_and_derived_table_spans() {
    // Test that the shared span cursor correctly handles both CTEs and derived tables
    // in the same query, maintaining proper lexical ordering
    let sql = "WITH my_cte AS (SELECT 1 AS x) SELECT * FROM my_cte JOIN (SELECT 2 AS y) AS derived ON my_cte.x = derived.y";
    let request = make_request(sql);
    let result = analyze(&request);

    let cte_node = result.statements[0]
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Cte && node.label.as_ref() == "my_cte")
        .expect("cte node");

    let derived_node = result.statements[0]
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::Cte && node.label.as_ref() == "derived")
        .expect("derived node");

    let cte_span = cte_node.span.expect("cte span");
    let derived_span = derived_node.span.expect("derived span");

    // Verify the spans are at the correct positions and don't overlap
    assert_eq!(&sql[cte_span.start..cte_span.end], "my_cte");
    assert_eq!(&sql[derived_span.start..derived_span.end], "derived");
    assert!(
        cte_span.end < derived_span.start,
        "CTE span should come before derived span"
    );
}

#[test]
fn file_statements_produce_spans() {
    let mut request = make_request("");
    let file_sql = "SELECT * FROM missing_table";
    request.schema = Some(schema_with_known_table());
    request.files = Some(vec![FileSource {
        name: "file.sql".to_string(),
        content: file_sql.to_string(),
    }]);

    let result = analyze(&request);
    let issue = result
        .issues
        .iter()
        .find(|issue| issue.code == issue_codes::UNRESOLVED_REFERENCE)
        .expect("missing table issue");

    let span = issue
        .span
        .expect("span should be present for file statement");
    assert_eq!(&file_sql[span.start..span.end], "missing_table");
}

#[test]
fn lint_document_rules_apply_to_each_file_in_multi_file_request() {
    let mut request = make_request("");
    request.files = Some(vec![
        FileSource {
            name: "first.sql".to_string(),
            content: "SELECT 1;;".to_string(),
        },
        FileSource {
            name: "second.sql".to_string(),
            content: "SELECT 2;;".to_string(),
        },
    ]);
    request.options = Some(AnalysisOptions {
        lint: Some(LintConfig::default()),
        ..Default::default()
    });

    let result = analyze(&request);
    let st012_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|issue| issue.code == issue_codes::LINT_ST_012)
        .collect();

    assert_eq!(st012_issues.len(), 2, "expected one ST_012 issue per file");
    assert!(
        st012_issues
            .iter()
            .all(|issue| issue.statement_index == Some(0)),
        "document-level lint rules should run with per-document statement indices"
    );
}

#[test]
fn parser_fallback_metadata_is_attached_to_lint_issues() {
    let mut request =
        make_request("SELECT usage_metadata ? 'pipeline_id' FROM ledger.usage_line_item;;");
    request.options = Some(AnalysisOptions {
        lint: Some(LintConfig::default()),
        ..Default::default()
    });

    let result = analyze(&request);
    let st012_issue = result
        .issues
        .iter()
        .find(|issue| issue.code == issue_codes::LINT_ST_012)
        .expect("expected ST_012 lint issue");

    assert_eq!(
        st012_issue.lint_confidence,
        Some(LintConfidence::Medium),
        "parser fallback should downgrade lint confidence"
    );
    assert_eq!(
        st012_issue.lint_fallback_source,
        Some(LintFallbackSource::ParserFallback),
        "lint issue should report parser fallback provenance"
    );
}

#[test]
fn depth_limit_warning_emitted_once_per_statement() {
    let request = make_request("SELECT 1");
    let mut analyzer = Analyzer::new(&request);

    analyzer.emit_depth_limit_warning(0);
    analyzer.emit_depth_limit_warning(0);

    assert_eq!(analyzer.issues.len(), 1, "warning should be deduplicated");
    assert_eq!(analyzer.issues[0].code, issue_codes::APPROXIMATE_LINEAGE);
}

#[test]
fn hide_ctes_option_filters_statement_and_global_lineage() {
    let sql = "WITH temp AS (SELECT * FROM source_table) SELECT * FROM temp";

    let without_hiding = analyze(&make_request_with_options(sql, false, false));
    let with_hiding = analyze(&make_request_with_options(sql, true, false));

    let stmt_without = &without_hiding.statements[0];
    assert!(
        stmt_without
            .nodes
            .iter()
            .any(|n| n.node_type == NodeType::Cte),
        "expected CTE nodes when hide_ctes is disabled"
    );
    assert!(
        without_hiding
            .global_lineage
            .nodes
            .iter()
            .any(|n| n.node_type == NodeType::Cte),
        "global lineage should include CTE nodes when not hidden"
    );

    let stmt_with = &with_hiding.statements[0];
    assert!(
        stmt_with.nodes.iter().all(|n| n.node_type != NodeType::Cte),
        "CTE nodes should be filtered when hide_ctes is enabled"
    );
    assert!(
        with_hiding
            .global_lineage
            .nodes
            .iter()
            .all(|n| n.node_type != NodeType::Cte),
        "global lineage should exclude CTE nodes when hidden"
    );

    assert!(
        stmt_with.nodes.len() < stmt_without.nodes.len(),
        "hiding CTEs should reduce statement node count"
    );
    assert!(
        with_hiding.summary.table_count < without_hiding.summary.table_count,
        "summary table_count should decrease when CTE nodes are hidden"
    );
    assert!(
        with_hiding.summary.complexity_score < without_hiding.summary.complexity_score,
        "complexity score should decrease when CTE nodes are hidden"
    );
}

#[test]
fn hide_ctes_customer_360_preserves_relationships() {
    let sql = load_sql_fixture("generic", "09_customer_360.sql");
    let schema = load_schema_fixture("customer_360_schema.json");

    let mut request = make_request_with_options(&sql, true, true);
    request.schema = Some(schema);

    let result = analyze(&request);
    let statement = &result.statements[0];

    assert!(statement
        .nodes
        .iter()
        .all(|node| node.node_type != NodeType::Cte));

    let node_types: HashMap<&str, NodeType> = statement
        .nodes
        .iter()
        .map(|node| (node.id.as_ref(), node.node_type))
        .collect();
    let node_labels: HashMap<&str, &str> = statement
        .nodes
        .iter()
        .map(|node| (node.id.as_ref(), node.label.as_ref()))
        .collect();
    let node_by_id: HashMap<&str, &Node> = statement
        .nodes
        .iter()
        .map(|node| (node.id.as_ref(), node))
        .collect();
    let node_ids: HashSet<&str> = node_types.keys().copied().collect();

    for edge in &statement.edges {
        assert!(node_ids.contains(edge.from.as_ref()));
        assert!(node_ids.contains(edge.to.as_ref()));
    }

    let forbidden_labels: HashSet<&str> = ["user_ltv", "user_engagement"].into_iter().collect();
    assert!(statement
        .nodes
        .iter()
        .all(|node| !forbidden_labels.contains(node.label.as_ref())));

    let table_pairs: HashSet<String> = statement
        .edges
        .iter()
        .filter(|edge| {
            matches!(
                node_types.get(edge.from.as_ref()),
                Some(NodeType::Table) | Some(NodeType::View)
            ) && matches!(
                node_types.get(edge.to.as_ref()),
                Some(NodeType::Table) | Some(NodeType::View)
            )
        })
        .filter_map(|edge| {
            let from_label = node_labels.get(edge.from.as_ref())?;
            let to_label = node_labels.get(edge.to.as_ref())?;
            Some(format!("{from_label}->{to_label}"))
        })
        .collect();

    let expected_table_pairs: HashSet<String> = [
        "orders->customer_360",
        "users->customer_360",
        "session_summary->customer_360",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    assert_eq!(table_pairs, expected_table_pairs);

    let view_id = statement
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::View && node.label.as_ref() == "customer_360")
        .expect("customer_360 view node")
        .id
        .to_string();

    let base_column_id = |qualified_name: &str| -> String {
        statement
            .nodes
            .iter()
            .find(|node| {
                node.node_type == NodeType::Column
                    && node
                        .qualified_name
                        .as_ref()
                        .is_some_and(|name| name.as_ref() == qualified_name)
            })
            .unwrap_or_else(|| panic!("missing column node {qualified_name}"))
            .id
            .to_string()
    };

    let view_column_id = |column_name: &str| -> String {
        statement
            .edges
            .iter()
            .find(|edge| {
                edge.edge_type == EdgeType::Ownership
                    && edge.from.as_ref() == view_id.as_str()
                    && node_by_id
                        .get(edge.to.as_ref())
                        .is_some_and(|node| node.label.as_ref() == column_name)
            })
            .unwrap_or_else(|| panic!("missing view column node {column_name}"))
            .to
            .to_string()
    };

    let view_column_labels: HashSet<&str> = statement
        .edges
        .iter()
        .filter(|edge| {
            edge.edge_type == EdgeType::Ownership && edge.from.as_ref() == view_id.as_str()
        })
        .filter_map(|edge| {
            node_by_id
                .get(edge.to.as_ref())
                .map(|node| node.label.as_ref())
        })
        .collect();

    let expected_view_columns: HashSet<&str> = [
        "user_id",
        "email",
        "signup_source",
        "total_orders",
        "lifetime_value",
        "last_order_date",
        "total_sessions",
        "last_seen",
        "customer_segment",
    ]
    .into_iter()
    .collect();

    assert_eq!(view_column_labels, expected_view_columns);

    let edges: HashSet<(String, String)> = statement
        .edges
        .iter()
        .map(|edge| (edge.from.to_string(), edge.to.to_string()))
        .collect();

    let base_prefixes = ["users.", "orders.", "session_summary."];
    let edge_type_name = |edge_type: EdgeType| -> &'static str {
        match edge_type {
            EdgeType::Ownership => "ownership",
            EdgeType::DataFlow => "data_flow",
            EdgeType::Derivation => "derivation",
            EdgeType::JoinDependency => "join_dependency",
            EdgeType::CrossStatement => "cross_statement",
        }
    };

    let base_columns: HashMap<&str, String> = [
        ("users.user_id", base_column_id("users.user_id")),
        ("users.email", base_column_id("users.email")),
        ("users.signup_source", base_column_id("users.signup_source")),
        ("orders.order_id", base_column_id("orders.order_id")),
        ("orders.total_amount", base_column_id("orders.total_amount")),
        ("orders.created_at", base_column_id("orders.created_at")),
        (
            "session_summary.session_id",
            base_column_id("session_summary.session_id"),
        ),
        (
            "session_summary.session_end",
            base_column_id("session_summary.session_end"),
        ),
    ]
    .into_iter()
    .collect();

    let view_columns: HashMap<&str, String> = [
        ("user_id", view_column_id("user_id")),
        ("email", view_column_id("email")),
        ("signup_source", view_column_id("signup_source")),
        ("total_orders", view_column_id("total_orders")),
        ("lifetime_value", view_column_id("lifetime_value")),
        ("last_order_date", view_column_id("last_order_date")),
        ("total_sessions", view_column_id("total_sessions")),
        ("last_seen", view_column_id("last_seen")),
        ("customer_segment", view_column_id("customer_segment")),
    ]
    .into_iter()
    .collect();

    let expected_sources: HashMap<&str, BTreeSet<(String, &'static str)>> = [
        (
            "user_id",
            BTreeSet::from([("users.user_id".to_string(), "data_flow")]),
        ),
        (
            "email",
            BTreeSet::from([("users.email".to_string(), "data_flow")]),
        ),
        (
            "signup_source",
            BTreeSet::from([("users.signup_source".to_string(), "data_flow")]),
        ),
        (
            "total_orders",
            BTreeSet::from([("orders.order_id".to_string(), "derivation")]),
        ),
        (
            "lifetime_value",
            BTreeSet::from([("orders.total_amount".to_string(), "derivation")]),
        ),
        (
            "last_order_date",
            BTreeSet::from([("orders.created_at".to_string(), "derivation")]),
        ),
        (
            "total_sessions",
            BTreeSet::from([("session_summary.session_id".to_string(), "derivation")]),
        ),
        (
            "last_seen",
            BTreeSet::from([("session_summary.session_end".to_string(), "derivation")]),
        ),
        (
            "customer_segment",
            BTreeSet::from([("orders.total_amount".to_string(), "derivation")]),
        ),
    ]
    .into_iter()
    .collect();

    for view_column in &expected_view_columns {
        let view_column_id = view_columns
            .get(view_column)
            .expect("view column id")
            .clone();
        let incoming_edges: Vec<_> = statement
            .edges
            .iter()
            .filter(|edge| {
                edge.to.as_ref() == view_column_id.as_str()
                    && matches!(edge.edge_type, EdgeType::DataFlow | EdgeType::Derivation)
            })
            .collect();

        assert!(
            !incoming_edges.is_empty(),
            "expected incoming edge for view column {view_column}"
        );

        let mut actual_sources = BTreeSet::new();
        for edge in incoming_edges {
            assert!(edge.approximate.is_none());
            let source_node = node_by_id
                .get(edge.from.as_ref())
                .expect("source node exists");
            assert_eq!(
                source_node.node_type,
                NodeType::Column,
                "expected column source for view column {view_column}"
            );
            let qualified = source_node
                .qualified_name
                .as_ref()
                .expect("source column qualified name");
            assert!(
                base_prefixes
                    .iter()
                    .any(|prefix| qualified.as_ref().starts_with(prefix)),
                "unexpected source column {qualified} for view column {view_column}"
            );
            actual_sources.insert((qualified.to_string(), edge_type_name(edge.edge_type)));
        }

        let expected = expected_sources
            .get(view_column)
            .unwrap_or_else(|| panic!("missing expected sources for {view_column}"));
        assert_eq!(&actual_sources, expected);
    }

    let expected_column_edges = [
        ("users.user_id", "user_id"),
        ("users.email", "email"),
        ("users.signup_source", "signup_source"),
        ("orders.order_id", "total_orders"),
        ("orders.total_amount", "lifetime_value"),
        ("orders.created_at", "last_order_date"),
        ("session_summary.session_id", "total_sessions"),
        ("session_summary.session_end", "last_seen"),
        ("orders.total_amount", "customer_segment"),
    ];

    for (base_col, view_col) in expected_column_edges {
        assert!(
            edges.contains(&(
                base_columns[base_col].clone(),
                view_columns[view_col].clone()
            )),
            "expected edge from {base_col} to {view_col}"
        );
    }
}

#[test]
fn test_source_tables_in_resolved_schema() {
    // This test verifies that source tables from SELECT queries appear in resolved_schema
    let sql = "SELECT cast(t1.a as int) as a, cast(t1.b as int) as b, cast(t2.c as int) as c
               FROM table1 AS t1 LEFT JOIN table2 AS t2 ON t1.a = t2.a";
    let request = make_request(sql);
    let result = analyze(&request);

    assert!(!result.summary.has_errors);
    let resolved_schema = result.resolved_schema.expect("should have resolved_schema");

    // Collect table names from resolved schema
    let table_names: HashSet<_> = resolved_schema
        .tables
        .iter()
        .map(|t| t.name.as_str())
        .collect();

    assert!(
        table_names.contains("table1"),
        "table1 should be in resolved schema, got: {:?}",
        table_names
    );
    assert!(
        table_names.contains("table2"),
        "table2 should be in resolved schema, got: {:?}",
        table_names
    );

    // Verify table1 has columns a, b
    let table1 = resolved_schema
        .tables
        .iter()
        .find(|t| t.name == "table1")
        .expect("table1");
    let t1_cols: HashSet<_> = table1.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(t1_cols.contains("a"), "table1 should have column 'a'");
    assert!(t1_cols.contains("b"), "table1 should have column 'b'");

    // Verify table2 has columns a (from join condition), c (from projection)
    let table2 = resolved_schema
        .tables
        .iter()
        .find(|t| t.name == "table2")
        .expect("table2");
    let t2_cols: HashSet<_> = table2.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(
        t2_cols.contains("a"),
        "table2 should have column 'a' from join condition, got: {:?}",
        t2_cols
    );
    assert!(
        t2_cols.contains("c"),
        "table2 should have column 'c' from projection, got: {:?}",
        t2_cols
    );

    // Verify FK relationships inferred from JOIN condition (t1.a = t2.a)
    let t1_col_a = table1
        .columns
        .iter()
        .find(|c| c.name == "a")
        .expect("table1.a");
    let t2_col_a = table2
        .columns
        .iter()
        .find(|c| c.name == "a")
        .expect("table2.a");

    // table1.a should reference table2.a
    assert!(
        t1_col_a.foreign_key.is_some(),
        "table1.a should have a foreign key reference"
    );
    let t1_fk = t1_col_a.foreign_key.as_ref().unwrap();
    assert_eq!(t1_fk.table, "table2");
    assert_eq!(t1_fk.column, "a");

    // table2.a should reference table1.a (bidirectional)
    assert!(
        t2_col_a.foreign_key.is_some(),
        "table2.a should have a foreign key reference"
    );
    let t2_fk = t2_col_a.foreign_key.as_ref().unwrap();
    assert_eq!(t2_fk.table, "table1");
    assert_eq!(t2_fk.column, "a");
}

// Type mismatch warning tests

#[test]
fn test_type_mismatch_integer_vs_text_warning() {
    // Literal integer compared to literal string should warn
    let sql = "SELECT 1 FROM users WHERE 1 = 'text'";
    let request = make_request(sql);
    let result = analyze(&request);

    let type_mismatch_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == issue_codes::TYPE_MISMATCH)
        .collect();

    assert_eq!(
        type_mismatch_issues.len(),
        1,
        "expected one type mismatch warning, got {:?}",
        type_mismatch_issues
    );
    assert!(type_mismatch_issues[0].message.contains("TEXT"));
    assert_eq!(type_mismatch_issues[0].severity, Severity::Warning);
}

#[test]
fn test_type_mismatch_same_types_no_warning() {
    // Same types should not warn
    let sql = "SELECT 1 FROM users WHERE 'a' = 'b'";
    let request = make_request(sql);
    let result = analyze(&request);

    let type_mismatch_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == issue_codes::TYPE_MISMATCH)
        .collect();

    assert!(
        type_mismatch_issues.is_empty(),
        "expected no type mismatch warnings for same types, got {:?}",
        type_mismatch_issues
    );
}

#[test]
fn test_type_mismatch_numeric_types_compatible() {
    // Integer and Float should be compatible (no warning)
    let sql = "SELECT 1 FROM users WHERE 1 = 2.5";
    let request = make_request(sql);
    let result = analyze(&request);

    let type_mismatch_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == issue_codes::TYPE_MISMATCH)
        .collect();

    assert!(
        type_mismatch_issues.is_empty(),
        "expected no type mismatch warnings for numeric types, got {:?}",
        type_mismatch_issues
    );
}

#[test]
fn test_type_mismatch_arithmetic_date_plus_bool_warning() {
    // Date + Boolean in WHERE clause should warn (incompatible arithmetic)
    // Neither Date nor Boolean can implicitly cast to Float (numeric)
    // Note: Type checking only happens in WHERE/HAVING clauses, not SELECT
    let sql = "SELECT 1 FROM users WHERE CAST('2024-01-01' AS DATE) + true";
    let request = make_request(sql);
    let result = analyze(&request);

    let type_mismatch_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == issue_codes::TYPE_MISMATCH)
        .collect();

    assert_eq!(
        type_mismatch_issues.len(),
        1,
        "expected one type mismatch warning for DATE + BOOLEAN, got {:?}",
        type_mismatch_issues
    );
    assert!(type_mismatch_issues[0].message.contains("DATE"));
    assert!(type_mismatch_issues[0].message.contains("BOOLEAN"));
}

#[test]
fn test_type_mismatch_string_concatenation_allowed() {
    // String + String should be allowed (concatenation)
    let sql = "SELECT 'a' + 'b' FROM users";
    let request = make_request(sql);
    let result = analyze(&request);

    let type_mismatch_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == issue_codes::TYPE_MISMATCH)
        .collect();

    assert!(
        type_mismatch_issues.is_empty(),
        "expected no type mismatch warnings for string concatenation, got {:?}",
        type_mismatch_issues
    );
}

#[test]
fn test_type_mismatch_nested_expression() {
    // Nested expression with type mismatch should warn once
    let sql = "SELECT 1 FROM users WHERE (1 = 'text') AND (2 = 3)";
    let request = make_request(sql);
    let result = analyze(&request);

    let type_mismatch_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == issue_codes::TYPE_MISMATCH)
        .collect();

    assert_eq!(
        type_mismatch_issues.len(),
        1,
        "expected one type mismatch warning for nested expression, got {:?}",
        type_mismatch_issues
    );
    assert!(type_mismatch_issues[0].message.contains("TEXT"));
}

#[test]
fn test_type_mismatch_multiple_issues() {
    // Multiple type mismatches should produce multiple warnings
    let sql = "SELECT 1 FROM users WHERE 1 = 'a' AND 2 = 'b'";
    let request = make_request(sql);
    let result = analyze(&request);

    let type_mismatch_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == issue_codes::TYPE_MISMATCH)
        .collect();

    assert_eq!(
        type_mismatch_issues.len(),
        2,
        "expected two type mismatch warnings, got {:?}",
        type_mismatch_issues
    );
}

#[test]
fn test_type_mismatch_has_statement_index() {
    // Type mismatch warning should include statement index
    let sql = "SELECT 1; SELECT 1 FROM users WHERE 1 = 'text'";
    let request = make_request(sql);
    let result = analyze(&request);

    let type_mismatch_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == issue_codes::TYPE_MISMATCH)
        .collect();

    assert_eq!(type_mismatch_issues.len(), 1);
    assert_eq!(
        type_mismatch_issues[0].statement_index,
        Some(1),
        "type mismatch warning should reference the second statement"
    );
}
