use flowscope_core::{
    completion_items, ColumnSchema, CompletionRequest, Dialect, SchemaMetadata, SchemaTable,
};
use insta::assert_json_snapshot;

fn request_at_cursor(sql: &str, schema: Option<SchemaMetadata>) -> CompletionRequest {
    let cursor_offset = sql.find('|').expect("sql must contain cursor marker '|'");
    let clean_sql = sql.replace('|', "");
    CompletionRequest {
        sql: clean_sql,
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema,
    }
}

#[allow(dead_code)]
fn request_at_cursor_with_dialect(
    sql: &str,
    schema: Option<SchemaMetadata>,
    dialect: Dialect,
) -> CompletionRequest {
    let cursor_offset = sql.find('|').expect("sql must contain cursor marker '|'");
    let clean_sql = sql.replace('|', "");
    CompletionRequest {
        sql: clean_sql,
        dialect,
        cursor_offset,
        schema,
    }
}

fn sample_schema() -> SchemaMetadata {
    SchemaMetadata {
        default_catalog: None,
        default_schema: Some("public".to_string()),
        search_path: None,
        case_sensitivity: None,
        allow_implied: true,
        tables: vec![
            SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "id".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                    ColumnSchema {
                        name: "email".to_string(),
                        data_type: Some("varchar".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                    ColumnSchema {
                        name: "name".to_string(),
                        data_type: Some("varchar".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                ],
            },
            SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "orders".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "id".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                    ColumnSchema {
                        name: "total".to_string(),
                        data_type: Some("decimal".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                    ColumnSchema {
                        name: "user_id".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                ],
            },
            SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "products".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "id".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                    ColumnSchema {
                        name: "name".to_string(),
                        data_type: Some("varchar".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                    ColumnSchema {
                        name: "price".to_string(),
                        data_type: Some("decimal".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                ],
            },
        ],
    }
}

#[test]
fn snap_select_with_schema() {
    let request = request_at_cursor("SELECT | FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}

#[test]
fn snap_from_clause_tables() {
    let request = request_at_cursor("SELECT * FROM |", Some(sample_schema()));
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}

#[test]
fn snap_join_on_condition() {
    let request = request_at_cursor(
        "SELECT * FROM users u JOIN orders o ON |",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}

#[test]
fn snap_qualified_alias() {
    let request = request_at_cursor(
        "SELECT u.| FROM users u JOIN orders o ON u.id = o.user_id",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}

#[test]
fn snap_qualified_schema() {
    let request = request_at_cursor("SELECT * FROM public.|", Some(sample_schema()));
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}
