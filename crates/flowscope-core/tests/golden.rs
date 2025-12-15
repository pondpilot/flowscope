use flowscope_core::{analyze, AnalyzeRequest, Dialect, FileSource};
use insta::assert_json_snapshot;

mod common;
use common::prepare_for_snapshot;

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
        tag_hints: None,
    };

    let result = analyze(&request);
    let cleaned = prepare_for_snapshot(result);
    assert_json_snapshot!(cleaned);
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
        tag_hints: None,
    };

    let result = analyze(&request);
    let cleaned = prepare_for_snapshot(result);
    assert_json_snapshot!(cleaned);
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
        tag_hints: None,
    };

    let result = analyze(&request);
    let cleaned = prepare_for_snapshot(result);
    assert_json_snapshot!(cleaned);
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
        tag_hints: None,
    };

    let result = analyze(&request);
    let cleaned = prepare_for_snapshot(result);
    assert_json_snapshot!(cleaned);
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
        tag_hints: None,
    };

    let result = analyze(&request);
    let cleaned = prepare_for_snapshot(result);
    assert_json_snapshot!(cleaned);
}

#[test]
fn golden_resolved_schema_with_imported_and_implied() {
    use flowscope_core::{ColumnSchema, SchemaMetadata, SchemaTable};

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
                        is_primary_key: None,
                        foreign_key: None,
                        classifications: None,
                    },
                    ColumnSchema {
                        name: "username".to_string(),
                        data_type: Some("TEXT".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                        classifications: None,
                    },
                ],
            }],
        }),
        tag_hints: None,
    };

    let result = analyze(&request);
    let cleaned = prepare_for_snapshot(result);
    assert_json_snapshot!(cleaned);
}

#[test]
fn golden_imported_precedence_over_create_table() {
    use flowscope_core::{ColumnSchema, SchemaMetadata, SchemaTable};

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
                        is_primary_key: None,
                        foreign_key: None,
                        classifications: None,
                    },
                    ColumnSchema {
                        name: "imported_col".to_string(),
                        data_type: None,
                        is_primary_key: None,
                        foreign_key: None,
                        classifications: None,
                    },
                ],
            }],
        }),
        tag_hints: None,
    };

    let result = analyze(&request);
    let cleaned = prepare_for_snapshot(result);
    assert_json_snapshot!(cleaned);
}

#[test]
fn imported_schema_preserves_qualified_names() {
    use flowscope_core::{
        AnalyzeRequest, CaseSensitivity, ColumnSchema, Dialect, SchemaMetadata, SchemaTable,
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
                        is_primary_key: None,
                        foreign_key: None,
                        classifications: None,
                    },
                    ColumnSchema {
                        name: "email".to_string(),
                        data_type: Some("TEXT".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                        classifications: None,
                    },
                ],
            }],
        }),
        tag_hints: None,
    };

    let result = analyze(&request);
    let cleaned = prepare_for_snapshot(result);
    assert_json_snapshot!(cleaned);
}

#[test]
fn imported_schema_resolves_via_default_schema() {
    use flowscope_core::{AnalyzeRequest, ColumnSchema, Dialect, SchemaMetadata, SchemaTable};

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
                        is_primary_key: None,
                        foreign_key: None,
                        classifications: None,
                    },
                    ColumnSchema {
                        name: "email".to_string(),
                        data_type: None,
                        is_primary_key: None,
                        foreign_key: None,
                        classifications: None,
                    },
                ],
            }],
        }),
        tag_hints: None,
    };

    let result = analyze(&request);
    let cleaned = prepare_for_snapshot(result);
    assert_json_snapshot!(cleaned);
}

#[test]
fn imported_schema_resolves_via_search_path_with_catalog() {
    use flowscope_core::{
        AnalyzeRequest, ColumnSchema, Dialect, SchemaMetadata, SchemaNamespaceHint, SchemaTable,
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
                    is_primary_key: None,
                    foreign_key: None,
                    classifications: None,
                }],
            }],
        }),
        tag_hints: None,
    };

    let result = analyze(&request);
    let cleaned = prepare_for_snapshot(result);
    assert_json_snapshot!(cleaned);
}
