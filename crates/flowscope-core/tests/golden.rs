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

#[test]
fn golden_resolved_schema_with_imported_and_implied() {
    use flowscope_core::{ColumnSchema, SchemaMetadata, SchemaOrigin, SchemaTable};

    let request = AnalyzeRequest {
        sql: r#"
            CREATE TABLE orders (order_id INT, amount DECIMAL);
            CREATE VIEW high_orders AS SELECT order_id, amount FROM orders WHERE amount > 100;
            SELECT * FROM users;
        "#
        .to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: None,
        schema: Some(SchemaMetadata {
            allow_implied: true,
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            tables: vec![SchemaTable {
                catalog: None,
                schema: None,
                name: "users".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "id".to_string(),
                        data_type: Some("INTEGER".to_string()),
                    },
                    ColumnSchema {
                        name: "username".to_string(),
                        data_type: Some("TEXT".to_string()),
                    },
                ],
            }],
        }),
    };

    let result = analyze(&request);

    // Should have resolvedSchema with 3 tables
    let resolved = result.resolved_schema.expect("Should have resolved schema");
    assert_eq!(resolved.tables.len(), 3);

    // Find each table and verify origin
    let users_table = resolved
        .tables
        .iter()
        .find(|t| t.name == "users")
        .expect("Should have users table");
    assert_eq!(users_table.origin, SchemaOrigin::Imported);
    assert_eq!(users_table.columns.len(), 2);
    assert!(users_table.columns.iter().any(|c| c.name == "id"));
    assert!(users_table.columns.iter().any(|c| c.name == "username"));

    let orders_table = resolved
        .tables
        .iter()
        .find(|t| t.name == "orders")
        .expect("Should have orders table");
    assert_eq!(orders_table.origin, SchemaOrigin::Implied);
    assert_eq!(orders_table.columns.len(), 2);
    assert!(orders_table.columns.iter().any(|c| c.name == "order_id"));
    assert!(orders_table.columns.iter().any(|c| c.name == "amount"));
    assert_eq!(orders_table.source_statement_index, Some(0));

    let high_orders_view = resolved
        .tables
        .iter()
        .find(|t| t.name == "high_orders")
        .expect("Should have high_orders view");
    assert_eq!(high_orders_view.origin, SchemaOrigin::Implied);
    assert_eq!(high_orders_view.columns.len(), 2);
    assert_eq!(high_orders_view.source_statement_index, Some(1));

    assert!(!result.summary.has_errors);
}

#[test]
fn golden_imported_precedence_over_create_table() {
    use flowscope_core::{ColumnSchema, SchemaMetadata, SchemaOrigin, SchemaTable};

    let request = AnalyzeRequest {
        sql: "CREATE TABLE products (product_id INT, name TEXT, extra_col TEXT);".to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: None,
        schema: Some(SchemaMetadata {
            allow_implied: true,
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            tables: vec![SchemaTable {
                catalog: None,
                schema: None,
                name: "products".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "product_id".to_string(),
                        data_type: Some("BIGINT".to_string()),
                    },
                    ColumnSchema {
                        name: "imported_col".to_string(),
                        data_type: None,
                    },
                ],
            }],
        }),
    };

    let result = analyze(&request);

    // Should have resolvedSchema with products from imported schema
    let resolved = result.resolved_schema.expect("Should have resolved schema");
    assert_eq!(resolved.tables.len(), 1);

    let products_table = &resolved.tables[0];
    assert_eq!(products_table.name, "products");
    assert_eq!(products_table.origin, SchemaOrigin::Imported);
    assert_eq!(products_table.columns.len(), 2); // Only imported columns

    // Should NOT have extra_col from DDL
    assert!(!products_table.columns.iter().any(|c| c.name == "extra_col"));
    assert!(products_table
        .columns
        .iter()
        .any(|c| c.name == "imported_col"));

    // Should have a SCHEMA_CONFLICT warning
    let conflict_warnings: Vec<_> = result
        .issues
        .iter()
        .filter(|i| {
            i.severity == flowscope_core::Severity::Warning
                && i.code == flowscope_core::issue_codes::SCHEMA_CONFLICT
        })
        .collect();
    assert_eq!(conflict_warnings.len(), 1);
}

#[test]
fn imported_schema_preserves_qualified_names() {
    use flowscope_core::{
        analyze, issue_codes, AnalyzeRequest, CaseSensitivity, ColumnSchema, Dialect,
        SchemaMetadata, SchemaOrigin, SchemaTable,
    };

    let request = AnalyzeRequest {
        sql: "SELECT u.id, u.email FROM public.users u".to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: None,
        options: None,
        schema: Some(SchemaMetadata {
            allow_implied: true,
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: Some(CaseSensitivity::Lower),
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "id".to_string(),
                        data_type: Some("INT".to_string()),
                    },
                    ColumnSchema {
                        name: "email".to_string(),
                        data_type: Some("TEXT".to_string()),
                    },
                ],
            }],
        }),
    };

    let result = analyze(&request);

    // Ensure imported schema keys match fully qualified references (no UNKNOWN_* issues).
    assert!(
        result.issues.iter().all(|i| {
            i.code != issue_codes::UNKNOWN_TABLE && i.code != issue_codes::UNKNOWN_COLUMN
        }),
        "Unexpected schema issues: {:?}",
        result
            .issues
            .iter()
            .map(|i| (&i.code, &i.message))
            .collect::<Vec<_>>()
    );

    let resolved = result
        .resolved_schema
        .expect("Should produce resolved schema");
    let users = resolved
        .tables
        .iter()
        .find(|t| t.schema.as_deref() == Some("public") && t.name == "users")
        .expect("Should keep public.users qualified");

    assert_eq!(users.origin, SchemaOrigin::Imported);
    assert!(users.columns.iter().any(|c| c.name == "id"));
    assert!(users.columns.iter().any(|c| c.name == "email"));
}

#[test]
fn imported_schema_resolves_via_default_schema() {
    use flowscope_core::{
        analyze, issue_codes, AnalyzeRequest, ColumnSchema, Dialect, SchemaMetadata, SchemaOrigin,
        SchemaTable,
    };

    let request = AnalyzeRequest {
        sql: "SELECT id, email FROM users".to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: None,
        options: None,
        schema: Some(SchemaMetadata {
            allow_implied: true,
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "id".to_string(),
                        data_type: None,
                    },
                    ColumnSchema {
                        name: "email".to_string(),
                        data_type: None,
                    },
                ],
            }],
        }),
    };

    let result = analyze(&request);

    // With default_schema provided, unqualified references should match imported schema.
    assert!(
        result.issues.iter().all(|i| {
            i.code != issue_codes::UNKNOWN_TABLE && i.code != issue_codes::UNKNOWN_COLUMN
        }),
        "Unexpected schema issues: {:?}",
        result
            .issues
            .iter()
            .map(|i| (&i.code, &i.message))
            .collect::<Vec<_>>()
    );

    let resolved = result
        .resolved_schema
        .expect("Should produce resolved schema");
    let users = resolved
        .tables
        .iter()
        .find(|t| t.schema.as_deref() == Some("public") && t.name == "users")
        .expect("Should resolve users via default schema");

    assert_eq!(users.origin, SchemaOrigin::Imported);
    assert!(users.columns.iter().any(|c| c.name == "id"));
    assert!(users.columns.iter().any(|c| c.name == "email"));
}

#[test]
fn imported_schema_resolves_via_search_path_with_catalog() {
    use flowscope_core::{
        analyze, issue_codes, AnalyzeRequest, ColumnSchema, Dialect, SchemaMetadata,
        SchemaNamespaceHint, SchemaOrigin, SchemaTable,
    };

    let request = AnalyzeRequest {
        sql: "SELECT amount FROM reports".to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: None,
        options: None,
        schema: Some(SchemaMetadata {
            allow_implied: true,
            default_catalog: None,
            default_schema: None,
            search_path: Some(vec![SchemaNamespaceHint {
                catalog: Some("sales".to_string()),
                schema: "analytics".to_string(),
            }]),
            case_sensitivity: None,
            tables: vec![SchemaTable {
                catalog: Some("sales".to_string()),
                schema: Some("analytics".to_string()),
                name: "reports".to_string(),
                columns: vec![ColumnSchema {
                    name: "amount".to_string(),
                    data_type: None,
                }],
            }],
        }),
    };

    let result = analyze(&request);

    // Unqualified reference should be resolved via search_path (catalog + schema).
    assert!(
        result.issues.iter().all(|i| {
            i.code != issue_codes::UNKNOWN_TABLE && i.code != issue_codes::UNKNOWN_COLUMN
        }),
        "Unexpected schema issues: {:?}",
        result
            .issues
            .iter()
            .map(|i| (&i.code, &i.message))
            .collect::<Vec<_>>()
    );

    let resolved = result
        .resolved_schema
        .expect("Should produce resolved schema");
    let reports = resolved
        .tables
        .iter()
        .find(|t| {
            t.catalog.as_deref() == Some("sales")
                && t.schema.as_deref() == Some("analytics")
                && t.name == "reports"
        })
        .expect("Should resolve reports via search_path");

    assert_eq!(reports.origin, SchemaOrigin::Imported);
    assert!(reports.columns.iter().any(|c| c.name == "amount"));
}
