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
                columns: vec![
                    ColumnSchema {
                        name: "id".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                    ColumnSchema {
                        name: "total".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                ],
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

#[test]
fn clause_detection_insert() {
    let request = request_at_cursor("INSERT |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Insert);
}

#[test]
fn clause_detection_update() {
    let request = request_at_cursor("UPDATE |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Update);
}

#[test]
fn clause_detection_delete() {
    let request = request_at_cursor("DELETE |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Delete);
}

#[test]
fn clause_detection_with() {
    let request = request_at_cursor("WITH cte AS (|", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::With);
}

#[test]
fn clause_detection_multi_statement_first() {
    let request = request_at_cursor("SELECT |; SELECT * FROM t", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Select);
    assert_eq!(context.statement_index, 0);
}

#[test]
fn clause_detection_multi_statement_second() {
    let request = request_at_cursor("SELECT 1; SELECT * FROM |", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::From);
    assert_eq!(context.statement_index, 1);
}

// Robustness: whitespace and comments
#[test]
fn clause_detection_cursor_in_whitespace() {
    let request = request_at_cursor("SELECT  |  FROM users", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Select);
}

#[test]
fn clause_detection_after_comment() {
    let request = request_at_cursor("SELECT /* comment */ | FROM users", None);
    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Select);
}

// =============================================================================
// Schema Resolution Tests
// =============================================================================

#[test]
fn schema_table_in_from() {
    let request = request_at_cursor("SELECT * FROM users|", Some(sample_schema()));
    let context = completion_context(&request);
    assert!(context.tables_in_scope.iter().any(|t| t.name == "users"));
}

#[test]
fn schema_table_with_alias() {
    let request = request_at_cursor("SELECT * FROM users u|", Some(sample_schema()));
    let context = completion_context(&request);
    let table = context.tables_in_scope.iter().find(|t| t.name == "users");
    assert!(table.is_some());
    assert_eq!(table.unwrap().alias, Some("u".to_string()));
}

#[test]
fn schema_multiple_tables() {
    let request = request_at_cursor("SELECT * FROM users, orders|", Some(sample_schema()));
    let context = completion_context(&request);
    assert!(context.tables_in_scope.iter().any(|t| t.name == "users"));
    assert!(context.tables_in_scope.iter().any(|t| t.name == "orders"));
}

#[test]
fn schema_join_tables() {
    let request = request_at_cursor(
        "SELECT * FROM users u JOIN orders o ON |",
        Some(sample_schema()),
    );
    let context = completion_context(&request);
    assert_eq!(context.tables_in_scope.len(), 2);
    assert!(context.tables_in_scope.iter().any(|t| t.alias == Some("u".to_string())));
    assert!(context.tables_in_scope.iter().any(|t| t.alias == Some("o".to_string())));
}

#[test]
fn schema_columns_from_table() {
    let request = request_at_cursor("SELECT | FROM users", Some(sample_schema()));
    let context = completion_context(&request);
    assert!(context.columns_in_scope.iter().any(|c| c.name == "id"));
    assert!(context.columns_in_scope.iter().any(|c| c.name == "email"));
}

#[test]
fn schema_columns_joined_marks_ambiguous() {
    let request = request_at_cursor(
        "SELECT | FROM users u JOIN orders o ON u.id = o.id",
        Some(sample_schema()),
    );
    let context = completion_context(&request);
    // Both tables have "id" column - should be marked ambiguous
    let id_columns: Vec<_> = context.columns_in_scope.iter().filter(|c| c.name == "id").collect();
    assert_eq!(id_columns.len(), 2);
    assert!(id_columns.iter().all(|c| c.is_ambiguous));
}

#[test]
fn schema_qualified_table_canonical() {
    let request = request_at_cursor("SELECT * FROM public.users|", Some(sample_schema()));
    let context = completion_context(&request);
    let table = context.tables_in_scope.iter().find(|t| t.name == "public.users");
    assert!(table.is_some());
    assert!(table.unwrap().canonical.contains("users"));
}

#[test]
fn schema_default_resolution() {
    let request = request_at_cursor("SELECT * FROM users|", Some(sample_schema()));
    let context = completion_context(&request);
    let table = context.tables_in_scope.iter().find(|t| t.name == "users");
    assert!(table.is_some());
    assert!(table.unwrap().matched_schema);
}

#[test]
fn schema_case_insensitive_match() {
    let request = request_at_cursor("SELECT * FROM USERS|", Some(sample_schema()));
    let context = completion_context(&request);
    let table = context.tables_in_scope.iter().find(|t| t.name == "USERS");
    assert!(table.is_some());
    assert!(table.unwrap().matched_schema);
}

// =============================================================================
// Schema Resolution Tests - Catalog Qualified Names
// =============================================================================

#[test]
fn schema_catalog_qualified() {
    let request = request_at_cursor(
        "SELECT * FROM sales.public.customers|",
        Some(schema_with_catalog()),
    );
    let context = completion_context(&request);
    let table = context.tables_in_scope.iter().find(|t| t.name.contains("customers"));
    assert!(table.is_some());
}

#[test]
fn schema_catalog_default_resolution() {
    let request = request_at_cursor("SELECT * FROM customers|", Some(schema_with_catalog()));
    let context = completion_context(&request);
    let table = context.tables_in_scope.iter().find(|t| t.name == "customers");
    assert!(table.is_some());
    assert!(table.unwrap().matched_schema);
}

// =============================================================================
// Qualifier Handling Tests
// =============================================================================

#[test]
fn qualifier_alias_filters_columns() {
    let request = request_at_cursor("SELECT u.| FROM users u", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
    assert!(result.items.iter().all(|item| item.category == CompletionItemCategory::Column));
    assert!(result.items.iter().any(|item| item.label == "email"));
}

#[test]
fn qualifier_alias_case_insensitive() {
    let request = request_at_cursor("SELECT U.| FROM users U", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
    assert!(result.items.iter().all(|item| item.category == CompletionItemCategory::Column));
    assert!(result.items.iter().any(|item| item.label == "email"));
}

#[test]
fn qualifier_alias_excludes_other_tables() {
    let request = request_at_cursor(
        "SELECT u.| FROM users u JOIN orders o ON u.id = o.id",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
    // Should only show users columns, not orders columns
    assert!(result.items.iter().any(|item| item.label == "email"));
    assert!(!result.items.iter().any(|item| item.label == "total"));
}

#[test]
fn qualifier_alias_partial_prefix() {
    let request = request_at_cursor("SELECT u.em| FROM users u", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
    // "email" should match, filtered by prefix
    let email_item = result.items.iter().find(|item| item.label == "email");
    assert!(email_item.is_some());
}

#[test]
fn qualifier_table_name_filters_columns() {
    let request = request_at_cursor("SELECT users.| FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
    assert!(result.items.iter().all(|item| item.category == CompletionItemCategory::Column));
    assert!(result.items.iter().any(|item| item.label == "email"));
}

#[test]
fn qualifier_table_name_excludes_other_tables() {
    let request = request_at_cursor(
        "SELECT users.| FROM users JOIN orders ON users.id = orders.id",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
    assert!(result.items.iter().any(|item| item.label == "email"));
    assert!(!result.items.iter().any(|item| item.label == "total"));
}

#[test]
fn qualifier_schema_only_shows_tables() {
    let request = request_at_cursor("SELECT * FROM public.|", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
    assert!(result.items.iter().all(|item| item.category == CompletionItemCategory::SchemaTable));
    assert!(result.items.iter().any(|item| item.label.contains("users")));
}

#[test]
fn qualifier_schema_table_shows_columns() {
    let request = request_at_cursor(
        "SELECT public.users.| FROM public.users",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
    assert!(result.items.iter().all(|item| item.category == CompletionItemCategory::Column));
}
