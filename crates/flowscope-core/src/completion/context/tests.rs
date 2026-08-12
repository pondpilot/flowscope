use super::*;
use crate::generated::CanonicalType;
use crate::types::{
    ColumnSchema, CompletionClause, CompletionItemCategory, CompletionRequest, Dialect,
    SchemaMetadata, SchemaTable,
};

#[test]
fn test_completion_clause_detection() {
    let sql = "SELECT * FROM users WHERE ";
    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        // Cursor at end of string (after trailing space)
        cursor_offset: sql.len(),
        schema: None,
    };

    let context = completion_context(&request);
    assert_eq!(context.clause, CompletionClause::Where);
}

#[test]
fn test_completion_tables_and_columns() {
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
                name: "users".to_string(),
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
                        name: "user_id".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                ],
            },
        ],
    };

    let sql = "SELECT u. FROM users u JOIN orders o ON u.id = o.user_id";
    let cursor_offset = sql.find("u.").unwrap() + 2;

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(schema),
    };

    let context = completion_context(&request);
    assert_eq!(context.tables_in_scope.len(), 2);
    assert!(context
        .columns_in_scope
        .iter()
        .any(|col| col.name == "name"));
    assert!(context
        .columns_in_scope
        .iter()
        .any(|col| col.name == "user_id"));
    assert!(context
        .columns_in_scope
        .iter()
        .any(|col| col.name == "id" && col.is_ambiguous));
}

#[test]
fn test_completion_items_respects_table_qualifier() {
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
                name: "users".to_string(),
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
    };

    let sql = "SELECT u. FROM users u";
    let cursor_offset = sql.find("u.").unwrap() + 2;

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(schema),
    };

    let result = completion_items(&request);
    assert!(result.should_show);
    assert!(result
        .items
        .iter()
        .all(|item| item.category == CompletionItemCategory::Column));
    assert!(result.items.iter().any(|item| item.label == "id"));
    assert!(!result.items.iter().any(|item| item.label == "total"));
}

#[test]
fn test_completion_items_select_prefers_columns_over_tables() {
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
            columns: vec![ColumnSchema {
                name: "email".to_string(),
                data_type: Some("varchar".to_string()),
                is_primary_key: None,
                foreign_key: None,
            }],
        }],
    };

    let sql = "SELECT e";
    let cursor_offset = sql.len();

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(schema),
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
    assert!(!result
        .items
        .iter()
        .any(|item| item.category == CompletionItemCategory::SchemaTable));
}

// Unit tests for string helper functions

#[test]
fn test_extract_last_identifier_simple() {
    assert_eq!(extract_last_identifier("users"), Some("users".to_string()));
    assert_eq!(
        extract_last_identifier("foo_bar"),
        Some("foo_bar".to_string())
    );
    assert_eq!(
        extract_last_identifier("table123"),
        Some("table123".to_string())
    );
}

#[test]
fn test_extract_last_identifier_with_spaces() {
    assert_eq!(
        extract_last_identifier("SELECT users"),
        Some("users".to_string())
    );
    assert_eq!(extract_last_identifier("users "), Some("users".to_string()));
    assert_eq!(
        extract_last_identifier("  users  "),
        Some("users".to_string())
    );
}

#[test]
fn test_extract_last_identifier_quoted() {
    assert_eq!(
        extract_last_identifier("\"MyTable\""),
        Some("MyTable".to_string())
    );
    assert_eq!(
        extract_last_identifier("SELECT \"My Table\""),
        Some("My Table".to_string())
    );
    assert_eq!(
        extract_last_identifier("\"schema\".\"table\""),
        Some("table".to_string())
    );
}

#[test]
fn test_extract_last_identifier_empty() {
    assert_eq!(extract_last_identifier(""), None);
    assert_eq!(extract_last_identifier("   "), None);
    // Note: "SELECT " extracts "SELECT" because the function doesn't distinguish keywords
    assert_eq!(
        extract_last_identifier("SELECT "),
        Some("SELECT".to_string())
    );
    // Only punctuation/operators return None
    assert_eq!(extract_last_identifier("("), None);
    assert_eq!(extract_last_identifier(", "), None);
}

#[test]
fn test_extract_qualifier_with_trailing_dot() {
    assert_eq!(extract_qualifier("users.", 6), Some("users".to_string()));
    assert_eq!(extract_qualifier("SELECT u.", 9), Some("u".to_string()));
    assert_eq!(
        extract_qualifier("schema.table.", 13),
        Some("table".to_string())
    );
}

#[test]
fn test_extract_qualifier_mid_token() {
    assert_eq!(
        extract_qualifier("users.name", 10),
        Some("users".to_string())
    );
    assert_eq!(extract_qualifier("SELECT u.id", 11), Some("u".to_string()));
}

#[test]
fn test_extract_qualifier_no_qualifier() {
    assert_eq!(extract_qualifier("SELECT", 6), None);
    assert_eq!(extract_qualifier("users", 5), None);
    assert_eq!(extract_qualifier("", 0), None);
}

#[test]
fn test_extract_qualifier_cursor_at_start() {
    assert_eq!(extract_qualifier("users.name", 0), None);
}

#[test]
fn test_extract_qualifier_cursor_out_of_bounds() {
    assert_eq!(extract_qualifier("users", 100), None);
}

#[test]
fn test_extract_qualifier_utf8_boundary() {
    // Multi-byte UTF-8 character (emoji is 4 bytes)
    let sql = "SELECT 🎉.";
    // Cursor in middle of emoji (invalid boundary) should return None
    assert_eq!(extract_qualifier(sql, 8), None); // Middle of emoji
                                                 // Cursor after emoji + dot should work
    assert_eq!(extract_qualifier(sql, sql.len()), None); // 🎉 is not identifier char
}

#[test]
fn test_extract_qualifier_quoted_identifier() {
    assert_eq!(
        extract_qualifier("\"My Schema\".", 12),
        Some("My Schema".to_string())
    );
}

// Unit tests for resolve_qualifier

#[test]
fn test_resolve_qualifier_alias_match() {
    let tables = vec![CompletionTable {
        name: "users".to_string(),
        canonical: "public.users".to_string(),
        alias: Some("u".to_string()),
        matched_schema: true,
    }];
    let (registry, _) = SchemaRegistry::new(None, Dialect::Duckdb);

    let result = resolve_qualifier("u", &tables, None, &registry);
    assert!(result.is_some());
    let resolution = result.unwrap();
    assert_eq!(resolution.target, QualifierTarget::ColumnLabel);
    assert_eq!(resolution.label, Some("u".to_string()));
}

#[test]
fn test_resolve_qualifier_table_name_match() {
    // When table is in tables_in_scope (without alias), qualifier matches table name
    // Note: Schema metadata is required for table name matching (vs just alias matching)
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
            columns: vec![],
        }],
    };
    let tables = vec![CompletionTable {
        name: "users".to_string(),
        canonical: "public.users".to_string(),
        alias: None,
        matched_schema: true,
    }];
    let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);

    let result = resolve_qualifier("users", &tables, Some(&schema), &registry);
    assert!(
        result.is_some(),
        "Should match table name in tables_in_scope"
    );
    let resolution = result.unwrap();
    assert_eq!(resolution.target, QualifierTarget::ColumnLabel);
    // When no alias, label is the table name itself
    assert_eq!(resolution.label, Some("users".to_string()));
}

#[test]
fn test_resolve_qualifier_schema_only() {
    let schema = SchemaMetadata {
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        allow_implied: true,
        tables: vec![SchemaTable {
            catalog: None,
            schema: Some("myschema".to_string()),
            name: "mytable".to_string(),
            columns: vec![],
        }],
    };
    let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);

    let result = resolve_qualifier("myschema", &[], Some(&schema), &registry);
    assert!(result.is_some());
    let resolution = result.unwrap();
    assert_eq!(resolution.target, QualifierTarget::SchemaOnly);
    assert_eq!(resolution.schema, Some("myschema".to_string()));
}

#[test]
fn test_resolve_qualifier_schema_table() {
    let schema = SchemaMetadata {
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        allow_implied: true,
        tables: vec![SchemaTable {
            catalog: None,
            schema: Some("public".to_string()),
            name: "users".to_string(),
            columns: vec![ColumnSchema {
                name: "id".to_string(),
                data_type: Some("integer".to_string()),
                is_primary_key: None,
                foreign_key: None,
            }],
        }],
    };
    let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);

    // When qualifier matches a table name in schema (but not in tables_in_scope)
    let result = resolve_qualifier("users", &[], Some(&schema), &registry);
    assert!(result.is_some());
    let resolution = result.unwrap();
    assert_eq!(resolution.target, QualifierTarget::SchemaTable);
    assert_eq!(resolution.table, Some("users".to_string()));
}

#[test]
fn test_resolve_qualifier_no_match() {
    let (registry, _) = SchemaRegistry::new(None, Dialect::Duckdb);
    let result = resolve_qualifier("nonexistent", &[], None, &registry);
    assert!(result.is_none());
}

#[test]
fn test_resolve_qualifier_case_insensitive() {
    let tables = vec![CompletionTable {
        name: "Users".to_string(),
        canonical: "public.users".to_string(),
        alias: Some("U".to_string()),
        matched_schema: true,
    }];
    let (registry, _) = SchemaRegistry::new(None, Dialect::Duckdb);

    // Should match case-insensitively
    let result = resolve_qualifier("u", &tables, None, &registry);
    assert!(result.is_some());
    assert_eq!(result.unwrap().target, QualifierTarget::ColumnLabel);
}

// Test for column_name_from_label

#[test]
fn test_column_name_from_label() {
    assert_eq!(column_name_from_label("name"), "name");
    assert_eq!(column_name_from_label("users.name"), "name");
    assert_eq!(column_name_from_label("public.users.name"), "name");
}

// Tests for hybrid AST-based completion enrichment

#[test]
fn test_cte_column_completion() {
    // Test that CTE columns appear in completion
    let sql = "WITH cte AS (SELECT id, name FROM users) SELECT cte. FROM cte";
    let cursor_offset = sql.find("cte.").unwrap() + 4; // Position after "cte."

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Generic,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);
    assert!(result.should_show, "Should show completions after 'cte.'");

    // Check that CTE columns are in the completion items
    let column_names: Vec<&str> = result
        .items
        .iter()
        .filter(|item| item.category == CompletionItemCategory::Column)
        .map(|item| item.label.as_str())
        .collect();

    assert!(
        column_names.contains(&"id"),
        "Should have 'id' column from CTE. Columns found: {:?}",
        column_names
    );
    assert!(
        column_names.contains(&"name"),
        "Should have 'name' column from CTE. Columns found: {:?}",
        column_names
    );
}

#[test]
fn test_cte_with_declared_columns() {
    // Test CTE with explicit column declaration: WITH cte(a, b) AS (...)
    let sql = "WITH cte(x, y) AS (SELECT id, name FROM users) SELECT cte. FROM cte";
    let cursor_offset = sql.find("cte.").unwrap() + 4;

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Generic,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);
    assert!(result.should_show);

    let column_names: Vec<&str> = result
        .items
        .iter()
        .filter(|item| item.category == CompletionItemCategory::Column)
        .map(|item| item.label.as_str())
        .collect();

    // Should use declared names (x, y) not projected names (id, name)
    assert!(
        column_names.contains(&"x"),
        "Should have declared column 'x'. Columns found: {:?}",
        column_names
    );
    assert!(
        column_names.contains(&"y"),
        "Should have declared column 'y'. Columns found: {:?}",
        column_names
    );
}

#[test]
fn test_subquery_alias_column_completion() {
    // Test that subquery alias columns appear in completion
    // Note: The cursor must be AFTER the FROM clause for AST parsing to include the subquery
    let sql = "SELECT * FROM (SELECT a, b FROM t) AS sub WHERE sub.";
    let cursor_offset = sql.len(); // Position at the end after "sub."

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Generic,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);
    assert!(result.should_show, "Should show completions after 'sub.'");

    let column_names: Vec<&str> = result
        .items
        .iter()
        .filter(|item| item.category == CompletionItemCategory::Column)
        .map(|item| item.label.as_str())
        .collect();

    assert!(
        column_names.contains(&"a"),
        "Should have 'a' column from subquery. Columns found: {:?}",
        column_names
    );
    assert!(
        column_names.contains(&"b"),
        "Should have 'b' column from subquery. Columns found: {:?}",
        column_names
    );
}

#[test]
fn test_recursive_cte_column_completion() {
    // Test that recursive CTE base case columns appear in completion
    let sql = r#"
            WITH RECURSIVE cte AS (
                SELECT 1 AS n
                UNION ALL
                SELECT n + 1 FROM cte WHERE n < 10
            )
            SELECT cte. FROM cte
        "#;
    let cursor_offset = sql.find("cte.").unwrap() + 4;

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Generic,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);
    assert!(result.should_show);

    let column_names: Vec<&str> = result
        .items
        .iter()
        .filter(|item| item.category == CompletionItemCategory::Column)
        .map(|item| item.label.as_str())
        .collect();

    assert!(
        column_names.contains(&"n"),
        "Should have 'n' column from recursive CTE base case. Columns found: {:?}",
        column_names
    );
}

#[test]
fn test_multiple_ctes_column_completion() {
    // Test completion with multiple CTEs
    let sql = r#"
            WITH
                users_cte AS (SELECT id, name FROM users),
                orders_cte AS (SELECT order_id, user_id FROM orders)
            SELECT users_cte. FROM users_cte, orders_cte
        "#;
    let cursor_offset = sql.find("users_cte.").unwrap() + 10;

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Generic,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);
    assert!(result.should_show);

    let column_names: Vec<&str> = result
        .items
        .iter()
        .filter(|item| item.category == CompletionItemCategory::Column)
        .map(|item| item.label.as_str())
        .collect();

    // Should have columns from users_cte (the qualified table)
    assert!(
        column_names.contains(&"id"),
        "Should have 'id' column from users_cte. Columns found: {:?}",
        column_names
    );
    assert!(
        column_names.contains(&"name"),
        "Should have 'name' column from users_cte. Columns found: {:?}",
        column_names
    );
}

#[test]
fn test_type_context_inference() {
    // Direct test of type context inference
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
            columns: vec![
                ColumnSchema {
                    name: "age".to_string(),
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
    };

    let sql = "SELECT * FROM users WHERE age > ";
    let cursor_offset = sql.len();

    // Tokenize
    let tokens = tokenize_sql(sql, Dialect::Duckdb).expect("tokenization should succeed");

    // Create registry and completion context
    let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);

    // Get completion context to have tables with canonical names
    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(schema.clone()),
    };
    let ctx = completion_context(&request);

    // Test type context inference
    let type_ctx = infer_type_context(&tokens, cursor_offset, sql, &registry, &ctx.tables_in_scope);

    assert!(
        type_ctx.is_some(),
        "Should infer type context from 'age > '. Tables in scope: {:?}",
        ctx.tables_in_scope
            .iter()
            .map(|t| format!("{}(canonical:{})", t.name, t.canonical))
            .collect::<Vec<_>>()
    );

    let type_ctx = type_ctx.unwrap();
    assert_eq!(
        type_ctx.expected_type,
        CanonicalType::Integer,
        "Expected type should be Integer for 'age' column"
    );
}

#[test]
fn test_type_aware_column_completion_in_where() {
    // Test that type-compatible columns score higher in comparison contexts
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
            columns: vec![
                ColumnSchema {
                    name: "age".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                },
                ColumnSchema {
                    name: "created_at".to_string(),
                    data_type: Some("timestamp".to_string()),
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
                    name: "score".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                },
            ],
        }],
    };

    // Cursor after "age > " - should boost integer-compatible columns
    let sql = "SELECT * FROM users WHERE age > ";
    let cursor_offset = sql.len();

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(schema),
    };

    let result = completion_items(&request);
    assert!(result.should_show);

    // Find column completions
    let columns: Vec<_> = result
        .items
        .iter()
        .filter(|item| item.category == CompletionItemCategory::Column)
        .collect();

    // age and score (both integers) should score higher than name (varchar)
    let age_item = columns.iter().find(|c| c.label == "age");
    let score_item = columns.iter().find(|c| c.label == "score");
    let name_item = columns.iter().find(|c| c.label == "name");

    assert!(age_item.is_some(), "age column should be in completions");
    assert!(
        score_item.is_some(),
        "score column should be in completions"
    );
    assert!(name_item.is_some(), "name column should be in completions");

    // Integer columns should score higher than varchar in "age > " context
    let age_score = age_item.unwrap().score;
    let score_score = score_item.unwrap().score;
    let name_score = name_item.unwrap().score;

    assert!(
            age_score > name_score,
            "Integer column 'age' (score: {}) should rank higher than varchar 'name' (score: {}) in integer comparison context",
            age_score,
            name_score
        );
    assert!(
            score_score > name_score,
            "Integer column 'score' (score: {}) should rank higher than varchar 'name' (score: {}) in integer comparison context",
            score_score,
            name_score
        );
}

#[test]
fn test_type_context_with_parentheses() {
    // Test that parentheses around identifier are handled: WHERE (age) > |
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
            columns: vec![ColumnSchema {
                name: "age".to_string(),
                data_type: Some("integer".to_string()),
                is_primary_key: None,
                foreign_key: None,
            }],
        }],
    };

    let sql = "SELECT * FROM users WHERE (age) > ";
    let cursor_offset = sql.len();

    let tokens = tokenize_sql(sql, Dialect::Duckdb).expect("tokenization should succeed");
    let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);
    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(schema),
    };
    let ctx = completion_context(&request);

    let type_ctx = infer_type_context(&tokens, cursor_offset, sql, &registry, &ctx.tables_in_scope);

    assert!(
        type_ctx.is_some(),
        "Should infer type context from '(age) > '"
    );
    assert_eq!(type_ctx.unwrap().expected_type, CanonicalType::Integer);
}

#[test]
fn test_type_context_with_nested_parentheses() {
    // Test nested parens: WHERE ((age)) > |
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
            columns: vec![ColumnSchema {
                name: "age".to_string(),
                data_type: Some("integer".to_string()),
                is_primary_key: None,
                foreign_key: None,
            }],
        }],
    };

    let sql = "SELECT * FROM users WHERE ((age)) > ";
    let cursor_offset = sql.len();

    let tokens = tokenize_sql(sql, Dialect::Duckdb).expect("tokenization should succeed");
    let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);
    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(schema),
    };
    let ctx = completion_context(&request);

    let type_ctx = infer_type_context(&tokens, cursor_offset, sql, &registry, &ctx.tables_in_scope);

    assert!(
        type_ctx.is_some(),
        "Should infer type context from '((age)) > '"
    );
    assert_eq!(type_ctx.unwrap().expected_type, CanonicalType::Integer);
}

#[test]
fn test_type_context_after_and_returns_none() {
    // After AND/OR, we're in a new expression - should return None
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
            columns: vec![ColumnSchema {
                name: "age".to_string(),
                data_type: Some("integer".to_string()),
                is_primary_key: None,
                foreign_key: None,
            }],
        }],
    };

    let sql = "SELECT * FROM users WHERE age > 10 AND ";
    let cursor_offset = sql.len();

    let tokens = tokenize_sql(sql, Dialect::Duckdb).expect("tokenization should succeed");
    let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);
    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(schema),
    };
    let ctx = completion_context(&request);

    let type_ctx = infer_type_context(&tokens, cursor_offset, sql, &registry, &ctx.tables_in_scope);

    assert!(
        type_ctx.is_none(),
        "Should return None after AND (new expression context)"
    );
}

#[test]
fn test_type_context_after_or_returns_none() {
    // After OR, we're in a new expression - should return None
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
            columns: vec![ColumnSchema {
                name: "age".to_string(),
                data_type: Some("integer".to_string()),
                is_primary_key: None,
                foreign_key: None,
            }],
        }],
    };

    let sql = "SELECT * FROM users WHERE age > 10 OR ";
    let cursor_offset = sql.len();

    let tokens = tokenize_sql(sql, Dialect::Duckdb).expect("tokenization should succeed");
    let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);
    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(schema),
    };
    let ctx = completion_context(&request);

    let type_ctx = infer_type_context(&tokens, cursor_offset, sql, &registry, &ctx.tables_in_scope);

    assert!(
        type_ctx.is_none(),
        "Should return None after OR (new expression context)"
    );
}

// Lateral column alias completion tests

#[test]
fn test_lateral_alias_completion_duckdb() {
    let sql = "SELECT price * qty AS total, ";
    let cursor_offset = sql.len();

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);

    // 'total' should be available as a lateral alias
    let total_item = result
        .items
        .iter()
        .find(|i| i.label == "total" && i.detail == Some("lateral alias".to_string()));
    assert!(
        total_item.is_some(),
        "Lateral alias 'total' should be in completions for DuckDB"
    );
}

#[test]
fn test_lateral_alias_not_available_postgres() {
    let sql = "SELECT price * qty AS total, ";
    let cursor_offset = sql.len();

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Postgres,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);

    // 'total' should NOT be available as a lateral alias in PostgreSQL
    let total_item = result
        .items
        .iter()
        .find(|i| i.label == "total" && i.detail == Some("lateral alias".to_string()));
    assert!(
        total_item.is_none(),
        "Lateral alias should not appear for PostgreSQL"
    );
}

#[test]
fn test_lateral_alias_position_aware() {
    // Cursor is within the SELECT but before the alias definition ends
    let sql = "SELECT a + b AS total FROM t";
    let cursor_offset = 9; // After "SELECT a "

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);

    // 'total' should NOT be available - cursor is before alias definition
    let total_item = result
        .items
        .iter()
        .find(|i| i.label == "total" && i.detail == Some("lateral alias".to_string()));
    assert!(
        total_item.is_none(),
        "Alias defined after cursor should not appear"
    );
}

#[test]
fn test_multiple_lateral_aliases() {
    let sql = "SELECT a AS x, b AS y, ";
    let cursor_offset = sql.len();

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);

    // Both 'x' and 'y' should be available
    let x_item = result
        .items
        .iter()
        .find(|i| i.label == "x" && i.detail == Some("lateral alias".to_string()));
    let y_item = result
        .items
        .iter()
        .find(|i| i.label == "y" && i.detail == Some("lateral alias".to_string()));
    assert!(
        x_item.is_some(),
        "Lateral alias 'x' should be in completions"
    );
    assert!(
        y_item.is_some(),
        "Lateral alias 'y' should be in completions"
    );
}

#[test]
fn test_lateral_alias_quoted() {
    let sql = r#"SELECT a AS "My Total", "#;
    let cursor_offset = sql.len();

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);

    // Quoted alias should be available
    let alias_item = result
        .items
        .iter()
        .find(|i| i.label == "My Total" && i.detail == Some("lateral alias".to_string()));
    assert!(
        alias_item.is_some(),
        "Quoted lateral alias should be in completions"
    );
}

#[test]
fn test_lateral_alias_bigquery_dialect() {
    // BigQuery also supports lateral aliases
    let sql = "SELECT price AS p, p * 0.1 AS ";
    let cursor_offset = sql.len();

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Bigquery,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);

    // 'p' should be available as a lateral alias
    let p_item = result
        .items
        .iter()
        .find(|i| i.label == "p" && i.detail == Some("lateral alias".to_string()));
    assert!(
        p_item.is_some(),
        "Lateral alias 'p' should be in completions for BigQuery"
    );
}

#[test]
fn test_lateral_alias_snowflake_dialect() {
    // Snowflake also supports lateral aliases
    let sql = "SELECT amount AS amt, ";
    let cursor_offset = sql.len();

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Snowflake,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);

    // 'amt' should be available as a lateral alias
    let amt_item = result
        .items
        .iter()
        .find(|i| i.label == "amt" && i.detail == Some("lateral alias".to_string()));
    assert!(
        amt_item.is_some(),
        "Lateral alias 'amt' should be in completions for Snowflake"
    );
}

#[test]
fn test_lateral_alias_not_in_from_clause() {
    // Lateral aliases should not appear when cursor is in FROM clause
    let sql = "SELECT a AS x FROM ";
    let cursor_offset = sql.len();

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: None,
    };

    let result = completion_items(&request);

    // 'x' should NOT be available in FROM clause context
    let x_item = result
        .items
        .iter()
        .find(|i| i.label == "x" && i.detail == Some("lateral alias".to_string()));
    assert!(
        x_item.is_none(),
        "Lateral alias should not appear in FROM clause"
    );
}

#[test]
fn test_lateral_alias_not_with_qualifier() {
    // Lateral aliases should not appear when there's a table qualifier (e.g., "t.")
    let sql = "SELECT a AS x, t.";
    let cursor_offset = sql.len();

    let schema = SchemaMetadata {
        default_catalog: None,
        default_schema: None,
        search_path: None,
        case_sensitivity: None,
        allow_implied: true,
        tables: vec![SchemaTable {
            catalog: None,
            schema: None,
            name: "t".to_string(),
            columns: vec![ColumnSchema {
                name: "col1".to_string(),
                data_type: Some("integer".to_string()),
                is_primary_key: None,
                foreign_key: None,
            }],
        }],
    };

    let request = CompletionRequest {
        sql: sql.to_string(),
        dialect: Dialect::Duckdb,
        cursor_offset,
        schema: Some(schema),
    };

    let result = completion_items(&request);

    // When there's a qualifier, we should only show columns from that table
    // Lateral aliases should not appear (they don't have a table qualifier)
    let x_item = result
        .items
        .iter()
        .find(|i| i.label == "x" && i.detail == Some("lateral alias".to_string()));
    assert!(
        x_item.is_none(),
        "Lateral alias should not appear when using table qualifier"
    );
}

#[test]
fn test_should_show_for_cursor_utf8_boundary() {
    // Multi-byte UTF-8 character (emoji is 4 bytes)
    let sql = "SELECT 🎉 FROM";
    // Emoji starts at byte 7, cursor at byte 8 is mid-character
    let mid_emoji_offset = 8;

    // Should not panic, should return false for invalid boundary
    assert!(!should_show_for_cursor(sql, mid_emoji_offset, ""));
}

#[test]
fn test_should_show_for_cursor_valid_positions() {
    // Test various valid cursor positions
    let sql = "SELECT . FROM";
    assert!(should_show_for_cursor(sql, 8, "")); // After dot
    assert!(!should_show_for_cursor(sql, 0, "")); // At start (no prev char)
    assert!(should_show_for_cursor(sql, 7, "")); // After space
}

#[test]
fn test_should_show_for_cursor_out_of_bounds() {
    let sql = "SELECT";
    assert!(!should_show_for_cursor(sql, 100, "")); // Way out of bounds
    assert!(!should_show_for_cursor(sql, sql.len() + 1, "")); // Just past end
}
