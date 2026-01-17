use flowscope_core::{
    completion_context, completion_items, ColumnSchema, CompletionClause,
    CompletionItemCategory, CompletionRequest, Dialect, SchemaMetadata, SchemaTable,
};

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
                ],
            },
            SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "orders".to_string(),
                columns: vec![ColumnSchema {
                    name: "total".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                }],
            },
        ],
    }
}

/// Creates a CompletionRequest with cursor at the "|" marker position.
/// Example: "SELECT | FROM users" places cursor after SELECT.
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

/// Schema with catalog: sales.public.customers
fn schema_with_catalog() -> SchemaMetadata {
    SchemaMetadata {
        default_catalog: Some("sales".to_string()),
        default_schema: Some("public".to_string()),
        search_path: None,
        case_sensitivity: None,
        allow_implied: true,
        tables: vec![SchemaTable {
            catalog: Some("sales".to_string()),
            schema: Some("public".to_string()),
            name: "customers".to_string(),
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
            ],
        }],
    }
}

/// Multi-schema: public.users, analytics.events
fn schema_multi_schema() -> SchemaMetadata {
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
                ],
            },
            SchemaTable {
                catalog: None,
                schema: Some("analytics".to_string()),
                name: "events".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "id".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                    ColumnSchema {
                        name: "event_type".to_string(),
                        data_type: Some("varchar".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                ],
            },
        ],
    }
}

/// Schema for prefix matching tests: users table with user_id, power_user, email, email_verified
fn schema_prefix_matching() -> SchemaMetadata {
    SchemaMetadata {
        default_catalog: None,
        default_schema: Some("public".to_string()),
        search_path: None,
        case_sensitivity: None,
        allow_implied: true,
        tables: vec![SchemaTable {
            catalog: None,
            schema: Some("public".to_string()),
            name: "users".to_string(),
            columns: vec![
                ColumnSchema {
                    name: "user_id".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                },
                ColumnSchema {
                    name: "power_user".to_string(),
                    data_type: Some("boolean".to_string()),
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
                    name: "email_verified".to_string(),
                    data_type: Some("boolean".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                },
            ],
        }],
    }
}

#[test]
fn completion_items_filters_by_alias_case_insensitive() {
    let sql = "SELECT U. FROM users U";
    let cursor_offset = sql.find("U.").unwrap() + 2;
    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(sample_schema()),
    };

    let result = completion_items(&request);
    assert!(result.should_show);
    assert!(result
        .items
        .iter()
        .all(|item| item.category == CompletionItemCategory::Column));
    assert!(result.items.iter().any(|item| item.label == "email"));
    assert!(!result.items.iter().any(|item| item.label == "total"));
}

#[test]
fn completion_items_select_returns_columns_when_schema_available() {
    let sql = "SELECT e";
    let cursor_offset = sql.len();
    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(sample_schema()),
    };

    let result = completion_items(&request);
    assert!(result.should_show);
    assert!(result
        .items
        .iter()
        .any(|item| item.category == CompletionItemCategory::Column));
    assert!(!result
        .items
        .iter()
        .any(|item| item.category == CompletionItemCategory::Table));
}

#[test]
fn completion_items_resolves_table_name_qualifier() {
    let sql = "SELECT users. FROM users";
    let cursor_offset = sql.find("users.").unwrap() + 6;
    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(sample_schema()),
    };

    let result = completion_items(&request);
    assert!(result.should_show);
    assert!(result
        .items
        .iter()
        .all(|item| item.category == CompletionItemCategory::Column));
    assert!(result.items.iter().any(|item| item.label == "email"));
    assert!(!result.items.iter().any(|item| item.label == "total"));
}

#[test]
fn completion_items_resolves_schema_qualifier() {
    let sql = "SELECT public.";
    let cursor_offset = sql.len();
    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(sample_schema()),
    };

    let result = completion_items(&request);
    assert!(result.should_show);
    assert!(result
        .items
        .iter()
        .all(|item| item.category == CompletionItemCategory::SchemaTable));
    assert!(result.items.iter().any(|item| item.label == "public.users"));
}

#[test]
fn completion_items_prefers_exact_column_match() {
    let sql = "SELECT node_id";
    let cursor_offset = sql.len();
    let schema = SchemaMetadata {
        default_catalog: None,
        default_schema: Some("public".to_string()),
        search_path: None,
        case_sensitivity: None,
        allow_implied: true,
        tables: vec![
            SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "nodes".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "node_id".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                    ColumnSchema {
                        name: "node_details".to_string(),
                        data_type: Some("varchar".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                ],
            },
            SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "details".to_string(),
                columns: vec![ColumnSchema {
                    name: "id".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                }],
            },
        ],
    };

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(schema),
    };

    let result = completion_items(&request);
    assert!(result.should_show);
    let first = result.items.first().expect("first completion item");
    assert_eq!(first.label, "node_id");
}

#[test]
fn completion_items_select_returns_nothing_without_columns() {
    let schema = SchemaMetadata {
        default_catalog: None,
        default_schema: Some("public".to_string()),
        search_path: None,
        case_sensitivity: None,
        allow_implied: true,
        tables: vec![SchemaTable {
            catalog: None,
            schema: Some("public".to_string()),
            name: "users".to_string(),
            columns: Vec::new(),
        }],
    };

    let sql = "SELECT u";
    let cursor_offset = sql.len();
    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(schema),
    };

    let result = completion_items(&request);
    assert!(!result.should_show);
    assert!(result.items.is_empty());
}

// =============================================================================
// Clause Detection Tests
// =============================================================================

#[test]
fn clause_detection_select() {
    let request = request_at_cursor("SELECT | FROM users", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Select);
}

#[test]
fn clause_detection_select_after_column() {
    let request = request_at_cursor("SELECT id, | FROM users", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Select);
}

#[test]
fn clause_detection_from() {
    let request = request_at_cursor("SELECT * FROM |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::From);
}

#[test]
fn clause_detection_from_after_table() {
    let request = request_at_cursor("SELECT * FROM users, |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::From);
}

#[test]
fn clause_detection_where() {
    let request = request_at_cursor("SELECT * FROM users WHERE |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Where);
}

#[test]
fn clause_detection_where_mid_expression() {
    let request = request_at_cursor("SELECT * FROM t WHERE id = |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Where);
}

#[test]
fn clause_detection_join() {
    let request = request_at_cursor("SELECT * FROM a JOIN |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Join);
}

#[test]
fn clause_detection_on() {
    let request = request_at_cursor("SELECT * FROM a JOIN b ON |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::On);
}

#[test]
fn clause_detection_group_by() {
    let request = request_at_cursor("SELECT * FROM t GROUP BY |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::GroupBy);
}

#[test]
fn clause_detection_having() {
    let request = request_at_cursor("SELECT * FROM t GROUP BY x HAVING |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Having);
}

#[test]
fn clause_detection_order_by() {
    let request = request_at_cursor("SELECT * FROM t ORDER BY |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::OrderBy);
}

#[test]
fn clause_detection_limit() {
    let request = request_at_cursor("SELECT * FROM t LIMIT |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Limit);
}
