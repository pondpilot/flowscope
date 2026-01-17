use flowscope_core::{
    completion_context, completion_items, ColumnSchema, CompletionClause, CompletionItemCategory,
    CompletionRequest, Dialect, SchemaMetadata, SchemaTable,
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
    assert!(context
        .tables_in_scope
        .iter()
        .any(|t| t.alias == Some("u".to_string())));
    assert!(context
        .tables_in_scope
        .iter()
        .any(|t| t.alias == Some("o".to_string())));
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
    let id_columns: Vec<_> = context
        .columns_in_scope
        .iter()
        .filter(|c| c.name == "id")
        .collect();
    assert_eq!(id_columns.len(), 2);
    assert!(id_columns.iter().all(|c| c.is_ambiguous));
}

#[test]
fn schema_qualified_table_canonical() {
    let request = request_at_cursor("SELECT * FROM public.users|", Some(sample_schema()));
    let context = completion_context(&request);
    let table = context
        .tables_in_scope
        .iter()
        .find(|t| t.name == "public.users");
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
    let table = context
        .tables_in_scope
        .iter()
        .find(|t| t.name.contains("customers"));
    assert!(table.is_some());
}

#[test]
fn schema_catalog_default_resolution() {
    let request = request_at_cursor("SELECT * FROM customers|", Some(schema_with_catalog()));
    let context = completion_context(&request);
    let table = context
        .tables_in_scope
        .iter()
        .find(|t| t.name == "customers");
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
    assert!(result
        .items
        .iter()
        .all(|item| item.category == CompletionItemCategory::Column));
    assert!(result.items.iter().any(|item| item.label == "email"));
}

#[test]
fn qualifier_alias_case_insensitive() {
    let request = request_at_cursor("SELECT U.| FROM users U", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
    assert!(result
        .items
        .iter()
        .all(|item| item.category == CompletionItemCategory::Column));
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
    assert!(result
        .items
        .iter()
        .all(|item| item.category == CompletionItemCategory::Column));
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
    assert!(result
        .items
        .iter()
        .all(|item| item.category == CompletionItemCategory::SchemaTable));
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
    assert!(result
        .items
        .iter()
        .all(|item| item.category == CompletionItemCategory::Column));
}

#[test]
fn qualifier_subquery_alias() {
    let request = request_at_cursor(
        "SELECT sq.| FROM (SELECT id FROM users) sq",
        Some(sample_schema()),
    );
    let context = completion_context(&request);
    // Subquery alias should be recognized
    assert!(context
        .tables_in_scope
        .iter()
        .any(|t| t.alias == Some("sq".to_string())));
}

#[test]
fn no_qualifier_shows_all_columns() {
    let request = request_at_cursor(
        "SELECT | FROM users u JOIN orders o ON u.id = o.id",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
    // Should show columns from both tables
    let column_items: Vec<_> = result
        .items
        .iter()
        .filter(|item| item.category == CompletionItemCategory::Column)
        .collect();
    assert!(column_items.len() >= 4); // id, email from users + id, total from orders
}

#[test]
fn ambiguous_column_shows_prefixed() {
    let request = request_at_cursor(
        "SELECT id| FROM users JOIN orders ON users.id = orders.id",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    // Ambiguous "id" should suggest prefixed versions
    let id_items: Vec<_> = result
        .items
        .iter()
        .filter(|item| item.label.contains("id"))
        .collect();
    assert!(!id_items.is_empty());
}

// Robustness: cursor inside quoted identifier or string
#[test]
fn cursor_inside_string_literal() {
    let request = request_at_cursor("SELECT 'hello|world' FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    // Should not show completions inside string literal
    assert!(!result.should_show);
}

// =============================================================================
// Scoring and Ranking Tests
// =============================================================================

#[test]
fn score_select_columns_before_tables() {
    let request = request_at_cursor("SELECT | FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);

    // Find first column and first table (if any)
    let first_column = result
        .items
        .iter()
        .position(|item| item.category == CompletionItemCategory::Column);
    let first_table = result.items.iter().position(|item| {
        item.category == CompletionItemCategory::Table
            || item.category == CompletionItemCategory::SchemaTable
    });

    // In SELECT clause, columns should come before tables (if tables present)
    if let (Some(col_pos), Some(tbl_pos)) = (first_column, first_table) {
        assert!(
            col_pos < tbl_pos,
            "columns should rank before tables in SELECT"
        );
    }
}

#[test]
fn score_from_tables_before_columns() {
    let request = request_at_cursor("SELECT * FROM |", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);

    let first_table = result.items.iter().position(|item| {
        item.category == CompletionItemCategory::Table
            || item.category == CompletionItemCategory::SchemaTable
    });
    let first_column = result
        .items
        .iter()
        .position(|item| item.category == CompletionItemCategory::Column);

    // In FROM clause, tables should come before columns
    if let (Some(tbl_pos), Some(col_pos)) = (first_table, first_column) {
        assert!(
            tbl_pos < col_pos,
            "tables should rank before columns in FROM"
        );
    }
}

#[test]
fn score_where_columns_available() {
    let request = request_at_cursor("SELECT * FROM users WHERE |", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);

    // WHERE clause should have columns ranked high
    let column_items: Vec<_> = result
        .items
        .iter()
        .filter(|item| item.category == CompletionItemCategory::Column)
        .collect();
    assert!(!column_items.is_empty());
}

#[test]
fn score_order_by_columns_first() {
    let request = request_at_cursor("SELECT * FROM users ORDER BY |", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);

    // First items should be columns
    if let Some(first) = result.items.first() {
        assert_eq!(first.category, CompletionItemCategory::Column);
    }
}

#[test]
fn score_exact_match_ranks_highest() {
    let request = request_at_cursor("SELECT email|", Some(schema_prefix_matching()));
    let result = completion_items(&request);
    assert!(result.should_show);

    // "email" should rank above "email_verified"
    let email_pos = result.items.iter().position(|item| item.label == "email");
    let email_verified_pos = result
        .items
        .iter()
        .position(|item| item.label == "email_verified");

    if let (Some(exact), Some(prefix)) = (email_pos, email_verified_pos) {
        assert!(
            exact < prefix,
            "exact match 'email' should rank above 'email_verified'"
        );
    }
}

#[test]
fn score_prefix_match_above_contains() {
    let request = request_at_cursor("SELECT user|", Some(schema_prefix_matching()));
    let result = completion_items(&request);
    assert!(result.should_show);

    // "user_id" (prefix match) should rank above "power_user" (contains)
    let user_id_pos = result.items.iter().position(|item| item.label == "user_id");
    let power_user_pos = result
        .items
        .iter()
        .position(|item| item.label == "power_user");

    if let (Some(prefix), Some(contains)) = (user_id_pos, power_user_pos) {
        assert!(
            prefix < contains,
            "'user_id' should rank above 'power_user'"
        );
    }
}

#[test]
fn score_from_keyword_boost_on_f() {
    let schema = schema_prefix_matching();
    let request = request_at_cursor("SELECT f|", Some(schema));
    let result = completion_items(&request);
    assert!(result.should_show);

    // FROM keyword should be boosted when typing "f" in SELECT
    let from_item = result.items.iter().find(|item| item.label == "FROM");
    assert!(from_item.is_some(), "FROM keyword should be present");

    // FROM should be near the top (within first 10 items)
    let from_pos = result.items.iter().position(|item| item.label == "FROM");
    if let Some(pos) = from_pos {
        assert!(
            pos < 10,
            "FROM should be ranked high when typing 'f' in SELECT"
        );
    }
}

#[test]
fn score_alphabetical_tiebreak() {
    let request = request_at_cursor("SELECT | FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);

    // Find items with the same score and verify alphabetical ordering
    let items = &result.items;
    for window in items.windows(2) {
        if window[0].score == window[1].score {
            assert!(
                window[0].label.to_lowercase() <= window[1].label.to_lowercase(),
                "same-score items should be alphabetically ordered: {} vs {}",
                window[0].label,
                window[1].label
            );
        }
    }
}

// =============================================================================
// Edge Case Tests - Completion Suppression in Special Contexts
// =============================================================================

#[test]
fn cursor_inside_line_comment() {
    let request = request_at_cursor(
        "SELECT -- comment |here\n* FROM users",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    // Should not show completions inside line comment
    assert!(
        !result.should_show,
        "should suppress completions inside line comment"
    );
}

#[test]
fn cursor_inside_block_comment() {
    let request = request_at_cursor(
        "SELECT /* comment |here */ * FROM users",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    // Should not show completions inside block comment
    assert!(
        !result.should_show,
        "should suppress completions inside block comment"
    );
}

#[test]
fn cursor_inside_number_literal() {
    let request = request_at_cursor("SELECT 123|45 FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    // Should not show completions inside number literal
    assert!(
        !result.should_show,
        "should suppress completions inside number literal"
    );
}

#[test]
fn cursor_inside_double_quoted_identifier() {
    let request = request_at_cursor("SELECT * FROM \"My |Table\"", Some(sample_schema()));
    let result = completion_items(&request);
    // Should not show completions inside quoted identifier (it's a literal string)
    assert!(
        !result.should_show,
        "should suppress completions inside double-quoted identifier"
    );
}

#[test]
fn cursor_at_string_open_quote() {
    let request = request_at_cursor("SELECT '|hello' FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    // Cursor right after opening quote - inside the string
    assert!(
        !result.should_show,
        "should suppress completions at string start"
    );
}

#[test]
fn cursor_at_string_close_quote() {
    let request = request_at_cursor("SELECT 'hello|' FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    // Cursor right before closing quote - still inside the string
    assert!(
        !result.should_show,
        "should suppress completions at string end"
    );
}

// =============================================================================
// Error Recovery Tests - Malformed SQL Handling
// =============================================================================

#[test]
fn error_recovery_missing_from_keyword() {
    // SQL missing FROM keyword - should still provide completions
    let request = request_at_cursor("SELECT | users", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn error_recovery_unbalanced_parens_open() {
    let request = request_at_cursor("SELECT COUNT(| FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn error_recovery_unbalanced_parens_close() {
    let request = request_at_cursor("SELECT id) | FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn error_recovery_trailing_comma() {
    let request = request_at_cursor("SELECT id, | FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
    // Should suggest columns after trailing comma
    let has_columns = result
        .items
        .iter()
        .any(|item| item.category == CompletionItemCategory::Column);
    assert!(has_columns);
}

#[test]
fn error_recovery_incomplete_join() {
    let request = request_at_cursor("SELECT * FROM users JOIN |", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn error_recovery_double_keyword() {
    // Accidental double keyword - should still work
    let request = request_at_cursor("SELECT SELECT | FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
}

// =============================================================================
// DML Statement Tests - INSERT, UPDATE, DELETE
// =============================================================================

#[test]
fn dml_insert_columns() {
    let request = request_at_cursor("INSERT INTO users (|) VALUES (1)", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn dml_insert_into_table() {
    let request = request_at_cursor("INSERT INTO |", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn dml_update_set_column() {
    let request = request_at_cursor("UPDATE users SET | = 'value'", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn dml_update_where_column() {
    let request = request_at_cursor(
        "UPDATE users SET email = 'x' WHERE |",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
    let has_columns = result
        .items
        .iter()
        .any(|item| item.category == CompletionItemCategory::Column);
    assert!(has_columns);
}

#[test]
fn dml_delete_where_column() {
    let request = request_at_cursor("DELETE FROM users WHERE |", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn dml_delete_from_table() {
    let request = request_at_cursor("DELETE FROM |", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
}

// =============================================================================
// Window Function Tests - OVER, PARTITION BY, ORDER BY
// =============================================================================

#[test]
fn window_over_partition_by() {
    let request = request_at_cursor(
        "SELECT id, ROW_NUMBER() OVER (PARTITION BY |) FROM users",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn window_over_order_by() {
    let request = request_at_cursor(
        "SELECT id, ROW_NUMBER() OVER (ORDER BY |) FROM users",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn window_over_partition_and_order() {
    let request = request_at_cursor(
        "SELECT id, ROW_NUMBER() OVER (PARTITION BY id ORDER BY |) FROM users",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn window_aggregate_over() {
    let request = request_at_cursor("SELECT SUM(id) OVER (|) FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);
}

// =============================================================================
// Self-Join Tests - Same Table Aliased Multiple Times
// =============================================================================

#[test]
fn self_join_both_aliases_available() {
    let request = request_at_cursor(
        "SELECT | FROM users u1 JOIN users u2 ON u1.id = u2.id",
        Some(sample_schema()),
    );
    let context = completion_context(&request);
    // Should have both aliases
    assert!(context
        .tables_in_scope
        .iter()
        .any(|t| t.alias == Some("u1".to_string())));
    assert!(context
        .tables_in_scope
        .iter()
        .any(|t| t.alias == Some("u2".to_string())));
}

#[test]
fn self_join_alias_filters_correctly() {
    let request = request_at_cursor(
        "SELECT u1.| FROM users u1 JOIN users u2 ON u1.id = u2.id",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
    // Should show columns for u1
    assert!(result.items.iter().any(|item| item.label == "id"));
    assert!(result.items.iter().any(|item| item.label == "email"));
}

#[test]
fn self_join_columns_from_both() {
    let request = request_at_cursor(
        "SELECT | FROM users u1 JOIN users u2 ON u1.id = u2.id",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
    // Columns should be marked ambiguous since same table twice
    let id_items: Vec<_> = result
        .items
        .iter()
        .filter(|item| item.label.contains("id"))
        .collect();
    assert!(!id_items.is_empty());
}

// =============================================================================
// Nested Subquery Tests - Multiple Levels Deep
// =============================================================================

#[test]
fn nested_subquery_two_levels() {
    let request = request_at_cursor(
        "SELECT * FROM (SELECT * FROM (SELECT id FROM users) inner_sq) outer_sq WHERE |",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn nested_subquery_in_where() {
    let request = request_at_cursor(
        "SELECT * FROM users WHERE id IN (SELECT | FROM orders)",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn nested_subquery_exists() {
    let request = request_at_cursor(
        "SELECT * FROM users u WHERE EXISTS (SELECT 1 FROM orders o WHERE o.| = u.id)",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn nested_subquery_scalar() {
    let request = request_at_cursor(
        "SELECT (SELECT | FROM users LIMIT 1) as sub FROM orders",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn nested_subquery_correlated() {
    let request = request_at_cursor(
        "SELECT * FROM users u WHERE u.id = (SELECT MAX(|) FROM orders o WHERE o.id = u.id)",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
}

// =============================================================================
// CASE Expression Tests - WHEN/THEN/ELSE Completion
// =============================================================================

#[test]
fn case_when_column() {
    let request = request_at_cursor(
        "SELECT CASE WHEN | THEN 1 END FROM users",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
    let has_columns = result
        .items
        .iter()
        .any(|item| item.category == CompletionItemCategory::Column);
    assert!(has_columns);
}

#[test]
fn case_then_expression() {
    let request = request_at_cursor(
        "SELECT CASE WHEN id > 1 THEN | END FROM users",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn case_else_expression() {
    let request = request_at_cursor(
        "SELECT CASE WHEN id > 1 THEN 'a' ELSE | END FROM users",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn case_simple_when() {
    let request = request_at_cursor(
        "SELECT CASE id WHEN | THEN 'match' END FROM users",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
}

#[test]
fn case_nested() {
    let request = request_at_cursor(
        "SELECT CASE WHEN id > 1 THEN CASE WHEN | THEN 'a' END END FROM users",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert!(result.should_show);
}
