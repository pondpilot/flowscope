use duckdb::Connection;
use flowscope_core::{
    analyze, AggregationInfo, AnalyzeRequest, AnalyzeResult, Dialect, FilterClauseType,
    FilterPredicate, Node, NodeType, Span, StatementMeta, Summary,
};
use flowscope_export::{
    export_csv_bundle, export_html, export_json, export_mermaid, export_sql, export_xlsx,
    ExportNaming, MermaidView,
};
use serde_json::json;
use std::collections::HashMap;
use std::io::Read;

fn analyze_sample() -> flowscope_core::AnalyzeResult {
    analyze(&AnalyzeRequest {
        sql: "SELECT u.id, o.total FROM users u JOIN orders o ON u.id = o.user_id".to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: None,
        options: None,
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    })
}

#[test]
fn exports_mermaid_views() {
    let result = analyze_sample();
    let mermaid = export_mermaid(&result, MermaidView::Table).expect("mermaid export");
    assert!(mermaid.contains("flowchart LR"));
    assert!(mermaid.contains("users"));
    assert!(mermaid.contains("orders"));
}

#[test]
fn exports_json_pretty() {
    let result = analyze_sample();
    let json = export_json(&result, false).expect("json export");
    assert!(json.contains("\n"));
    assert!(json.contains("summary"));
}

#[test]
fn exports_html_report() {
    let result = analyze_sample();
    let naming = ExportNaming::new("Test Project");
    let html = export_html(&result, "Test Project", naming.exported_at()).expect("html export");
    assert!(html.contains("<title>Test Project - Lineage Export</title>"));
    assert!(html.contains("mermaid"));
}

#[test]
fn exports_csv_archive() {
    let result = analyze_sample();
    let bytes = export_csv_bundle(&result).expect("csv bundle");

    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).expect("zip archive");
    let mut file = archive
        .by_name("column_mappings.csv")
        .expect("column mappings file");
    let mut content = String::new();
    file.read_to_string(&mut content).expect("read csv content");
    assert!(content.contains("Source Table"));
}

#[test]
fn exports_xlsx_bytes() {
    let result = analyze_sample();
    let bytes = export_xlsx(&result).expect("xlsx export");
    assert!(!bytes.is_empty());
}

/// Multi-statement regression for `representative_join_edge_ids`.
///
/// The same logical `users JOIN orders` appears in two statements, each
/// selecting different columns. Column-level dataflow carries join metadata
/// per selected column, so without representative-edge collapsing the
/// `joins` table would contain one row per column per statement. The
/// representative logic should emit exactly one join row *per statement*
/// (dedup across column-level edges sharing the same relation pair + join
/// metadata) — so with two statements we expect exactly two join rows.
#[test]
fn sql_export_dedups_column_level_joins_and_preserves_per_statement_rows() {
    let sql = "SELECT u.id, o.total FROM users u JOIN orders o ON u.id = o.user_id;\n\
               SELECT u.id, o.total FROM users u JOIN orders o ON u.id = o.user_id;";
    let result = analyze(&AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: None,
        options: None,
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    });
    assert_eq!(result.statements.len(), 2, "expected two statements");

    let exported = export_sql(&result, None).expect("sql export");

    let join_rows: Vec<&str> = exported
        .lines()
        .filter(|line| line.contains("INSERT INTO joins"))
        .collect();
    assert_eq!(
        join_rows.len(),
        2,
        "expected one representative join row per statement, got {}:\n{}",
        join_rows.len(),
        join_rows.join("\n")
    );

    // Every representative join row must carry the shared INNER + condition
    // (the column-level dataflow edges all agree on this metadata).
    for row in &join_rows {
        assert!(
            row.contains("'INNER'") && row.contains("u.id = o.user_id"),
            "join row missing expected metadata: {row}"
        );
    }
}

#[test]
fn sql_export_preserves_statement_scoped_filters_and_aggregations() {
    let mut table_metadata = HashMap::new();
    table_metadata.insert(
        "statementFilters".to_string(),
        json!({
            "0": [{ "expression": "active = true", "clauseType": "where" }],
            "1": [],
        }),
    );

    let mut column_metadata = HashMap::new();
    column_metadata.insert(
        "statementAggregations".to_string(),
        json!({
            "0": { "isGroupingKey": false, "function": "COUNT" },
            "1": null,
        }),
    );

    let result = AnalyzeResult {
        statements: vec![
            StatementMeta {
                statement_index: 0,
                statement_type: "SELECT".to_string(),
                source_name: Some("models/filtered.sql".to_string()),
                span: Some(Span::new(0, 10)),
                join_count: 0,
                complexity_score: 1,
                resolved_sql: None,
            },
            StatementMeta {
                statement_index: 1,
                statement_type: "SELECT".to_string(),
                source_name: Some("models/plain.sql".to_string()),
                span: Some(Span::new(11, 20)),
                join_count: 0,
                complexity_score: 1,
                resolved_sql: None,
            },
        ],
        nodes: vec![
            Node {
                id: "table_users".into(),
                node_type: NodeType::Table,
                label: "users".into(),
                qualified_name: Some("public.users".into()),
                statement_ids: vec![0, 1],
                filters: vec![FilterPredicate {
                    expression: "active = true".to_string(),
                    clause_type: FilterClauseType::Where,
                }],
                metadata: Some(table_metadata),
                ..Default::default()
            },
            Node {
                id: "column_users_count".into(),
                node_type: NodeType::Column,
                label: "user_count".into(),
                qualified_name: Some("public.users.user_count".into()),
                statement_ids: vec![0, 1],
                aggregation: Some(AggregationInfo {
                    is_grouping_key: false,
                    function: Some("COUNT".to_string()),
                    distinct: None,
                }),
                metadata: Some(column_metadata),
                ..Default::default()
            },
        ],
        edges: vec![],
        issues: vec![],
        summary: Summary {
            statement_count: 2,
            table_count: 1,
            column_count: 1,
            ..Default::default()
        },
        resolved_schema: None,
    };

    let exported = export_sql(&result, None).expect("sql export");
    let conn = Connection::open_in_memory().expect("duckdb connection");
    conn.execute_batch(&exported)
        .expect("generated SQL should execute");

    let filter_rows: Vec<(i64, String)> = conn
        .prepare("SELECT statement_id, predicate FROM filters ORDER BY statement_id, predicate")
        .expect("prepare filter query")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query filters")
        .map(|row| row.expect("filter row"))
        .collect();
    assert_eq!(filter_rows, vec![(0, "active = true".to_string())]);

    let aggregation_rows: Vec<(i64, Option<String>)> = conn
        .prepare("SELECT statement_id, function FROM aggregations ORDER BY statement_id, function")
        .expect("prepare aggregation query")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query aggregations")
        .map(|row| row.expect("aggregation row"))
        .collect();
    assert_eq!(aggregation_rows, vec![(0, Some("COUNT".to_string()))]);
}
