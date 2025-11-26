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
