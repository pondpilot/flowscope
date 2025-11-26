use super::*;

fn make_request(sql: &str) -> AnalyzeRequest {
    AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: None,
        schema: None,
    }
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
    assert_eq!(result.statements[0].nodes.len(), 1);
    assert_eq!(&*result.statements[0].nodes[0].label, "users");
    assert_eq!(result.statements[0].nodes[0].node_type, NodeType::Table);
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
    let second_pos = sql[first_pos + "missing".len()..]
        .find("missing")
        .map(|offset| first_pos + "missing".len() + offset)
        .expect("second identifier");

    assert_eq!(spans[0].start, first_pos);
    assert_eq!(spans[1].start, second_pos);
    assert!(spans[0].start < spans[1].start);
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
fn depth_limit_warning_emitted_once_per_statement() {
    let request = make_request("SELECT 1");
    let mut analyzer = Analyzer::new(&request);

    analyzer.emit_depth_limit_warning(0);
    analyzer.emit_depth_limit_warning(0);

    assert_eq!(analyzer.issues.len(), 1, "warning should be deduplicated");
    assert_eq!(analyzer.issues[0].code, issue_codes::APPROXIMATE_LINEAGE);
}
