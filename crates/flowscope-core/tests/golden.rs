use flowscope_core::{analyze, AnalyzeRequest, Dialect, FileSource};
use serde_json::json;

fn summarize_tables(result: &flowscope_core::AnalyzeResult) -> serde_json::Value {
    let statements = result
        .statements
        .iter()
        .map(|stmt| {
            let mut tables: Vec<_> = stmt
                .nodes
                .iter()
                .filter(|n| n.node_type == flowscope_core::NodeType::Table)
                .map(|n| n.label.clone())
                .collect();
            tables.sort();
            tables.dedup();

            json!({
                "statementType": stmt.statement_type,
                "source": stmt.source_name,
                "tables": tables,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "statements": statements,
        "issues": result.issues.iter().map(|i| json!({
            "code": i.code,
            "severity": format!("{:?}", i.severity),
        })).collect::<Vec<_>>(),
        "summary": {
            "statementCount": result.summary.statement_count,
            "tableCount": result.summary.table_count,
            "columnCount": result.summary.column_count,
            "hasErrors": result.summary.has_errors,
        }
    })
}

fn collect_columns(result: &flowscope_core::AnalyzeResult) -> Vec<String> {
    let mut cols: Vec<_> = result
        .statements
        .iter()
        .flat_map(|stmt| {
            stmt.nodes
                .iter()
                .filter(|n| n.node_type == flowscope_core::NodeType::Column)
                .map(|n| n.label.clone())
        })
        .collect();
    cols.sort();
    cols.dedup();
    cols
}

#[test]
fn golden_inline_select_tables_only() {
    let request = AnalyzeRequest {
        sql: "SELECT u.id, o.total_amount FROM users u JOIN orders o ON u.id = o.user_id"
            .to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: None,
        schema: None,
    };

    let result = analyze(&request);
    let summary = summarize_tables(&result);

    let expected = json!({
        "statements": [
            {
                "statementType": "SELECT",
                "source": null,
                "tables": ["orders", "users"],
            }
        ],
        "issues": [],
        "summary": {
            "statementCount": 1,
            "tableCount": 2,
            "columnCount": result.summary.column_count,
            "hasErrors": false,
        }
    });

    assert_eq!(summary, expected);
}

#[test]
fn golden_multi_file_keeps_sources() {
    let request = AnalyzeRequest {
        sql: "".to_string(),
        files: Some(vec![
            FileSource {
                name: "alpha.sql".to_string(),
                content: "SELECT * FROM alpha_table;".to_string(),
            },
            FileSource {
                name: "beta.sql".to_string(),
                content: "SELECT * FROM beta_table;".to_string(),
            },
        ]),
        dialect: Dialect::Generic,
        source_name: None,
        options: None,
        schema: None,
    };

    let result = analyze(&request);
    let summary = summarize_tables(&result);

    let mut statements = summary["statements"]
        .as_array()
        .cloned()
        .expect("statements array");
    statements.sort_by(|a, b| {
        a["source"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["source"].as_str().unwrap_or_default())
    });

    let expected = vec![
        json!({
            "statementType": "SELECT",
            "source": "alpha.sql",
            "tables": ["alpha_table"],
        }),
        json!({
            "statementType": "SELECT",
            "source": "beta.sql",
            "tables": ["beta_table"],
        }),
    ];

    assert_eq!(statements, expected);
    assert!(!summary["summary"]["hasErrors"].as_bool().unwrap());
}

#[test]
fn golden_column_lineage_union_captures_outputs() {
    let request = AnalyzeRequest {
        sql: "SELECT id, name FROM users UNION ALL SELECT id, name FROM admins".to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: None,
        schema: None,
    };

    let result = analyze(&request);
    let columns = collect_columns(&result);
    let tables = summarize_tables(&result);

    assert_eq!(columns, vec!["id", "name"]);
    assert_eq!(
        tables["statements"][0]["tables"],
        json!(["admins", "users"])
    );
    assert!(!result.summary.has_errors);
}

#[test]
fn golden_window_functions_emit_columns() {
    let request = AnalyzeRequest {
        sql: "SELECT id, ROW_NUMBER() OVER (PARTITION BY dept) AS rn FROM employees".to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: None,
        schema: None,
    };

    let result = analyze(&request);
    let columns = collect_columns(&result);

    assert_eq!(columns, vec!["id", "rn"]);
    assert!(!result.summary.has_errors);
}

#[test]
fn golden_ctas_captures_target_columns() {
    let request = AnalyzeRequest {
        sql: "CREATE TABLE tgt AS SELECT id, UPPER(name) AS upper_name FROM users".to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: None,
        schema: None,
    };

    let result = analyze(&request);
    let columns = collect_columns(&result);
    let tables = summarize_tables(&result);

    assert!(columns.contains(&"id".to_string()));
    assert!(columns.contains(&"upper_name".to_string()));
    assert_eq!(tables["statements"][0]["tables"], json!(["tgt", "users"]));
    assert!(!result.summary.has_errors);
}
