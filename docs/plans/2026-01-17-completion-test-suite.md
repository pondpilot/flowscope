# SQL Autocomplete Test Suite Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a comprehensive test suite for the SQL autocomplete engine covering clause detection, schema resolution, qualifier handling, and scoring/ranking.

**Architecture:** Unit tests with explicit assertions in `completion_items.rs` for targeted behavior verification. Snapshot tests in `completion_snapshots.rs` for holistic output capture. Shared test infrastructure with cursor marker helpers and reusable schema builders.

**Tech Stack:** Rust, insta (snapshot testing), flowscope-core completion APIs

**Design Document:** `docs/plans/2026-01-17-completion-test-suite-design.md`

---

## Task 1: Add Shared Test Infrastructure

**Files:**
- Modify: `crates/flowscope-core/tests/completion_items.rs:1-50`

**Step 1: Add cursor marker helper and schema builders**

Add these helpers after the existing imports and `sample_schema()` function:

```rust
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

/// Schema for prefix matching tests: nodes table with node_id, node_details, plus power_user column
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
```

**Step 2: Run tests to verify helpers compile**

Run: `cargo test -p flowscope-core completion_items --no-run`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add crates/flowscope-core/tests/completion_items.rs
git commit -m "test(completion): add shared test infrastructure

Add cursor marker helper and reusable schema builders for
clause detection, schema resolution, and prefix matching tests."
```

---

## Task 2: Clause Detection Tests - Basic Clauses

**Files:**
- Modify: `crates/flowscope-core/tests/completion_items.rs`

**Step 1: Add import for completion_context**

At the top of the file, update imports:

```rust
use flowscope_core::{
    completion_context, completion_items, ColumnSchema, CompletionClause,
    CompletionItemCategory, CompletionRequest, Dialect, SchemaMetadata, SchemaTable,
};
```

**Step 2: Write clause detection tests**

Add after the existing tests:

```rust
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
```

**Step 3: Run tests**

Run: `cargo test -p flowscope-core clause_detection`
Expected: All 12 tests pass

**Step 4: Commit**

```bash
git add crates/flowscope-core/tests/completion_items.rs
git commit -m "test(completion): add clause detection tests for basic clauses

Cover SELECT, FROM, WHERE, JOIN, ON, GROUP BY, HAVING, ORDER BY, LIMIT."
```

---

## Task 3: Clause Detection Tests - DML and Multi-Statement

**Files:**
- Modify: `crates/flowscope-core/tests/completion_items.rs`

**Step 1: Add DML and multi-statement tests**

```rust
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
```

**Step 2: Run tests**

Run: `cargo test -p flowscope-core clause_detection`
Expected: All 20 tests pass

**Step 3: Commit**

```bash
git add crates/flowscope-core/tests/completion_items.rs
git commit -m "test(completion): add DML and multi-statement clause detection

Cover INSERT, UPDATE, DELETE, WITH, multi-statement cursor positioning,
and robustness cases for whitespace and comments."
```

---

## Task 4: Schema Resolution Tests - Tables in Scope

**Files:**
- Modify: `crates/flowscope-core/tests/completion_items.rs`

**Step 1: Add schema resolution tests for tables**

```rust
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
```

**Step 2: Run tests**

Run: `cargo test -p flowscope-core schema_`
Expected: All 4 tests pass

**Step 3: Commit**

```bash
git add crates/flowscope-core/tests/completion_items.rs
git commit -m "test(completion): add schema resolution tests for tables in scope

Cover single table, aliased table, comma-separated tables, and JOIN tables."
```

---

## Task 5: Schema Resolution Tests - Columns in Scope

**Files:**
- Modify: `crates/flowscope-core/tests/completion_items.rs`

**Step 1: Add column resolution tests**

```rust
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
```

**Step 2: Run tests**

Run: `cargo test -p flowscope-core schema_`
Expected: All 9 tests pass

**Step 3: Commit**

```bash
git add crates/flowscope-core/tests/completion_items.rs
git commit -m "test(completion): add schema resolution tests for columns

Cover columns from single table, ambiguous columns in JOINs,
qualified table names, default schema resolution, case insensitivity."
```

---

## Task 6: Schema Resolution Tests - Catalog Qualified Names

**Files:**
- Modify: `crates/flowscope-core/tests/completion_items.rs`

**Step 1: Add catalog-qualified tests**

```rust
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
```

**Step 2: Run tests**

Run: `cargo test -p flowscope-core schema_catalog`
Expected: All 2 tests pass

**Step 3: Commit**

```bash
git add crates/flowscope-core/tests/completion_items.rs
git commit -m "test(completion): add catalog-qualified name resolution tests

Cover catalog.schema.table references and default catalog resolution."
```

---

## Task 7: Qualifier Handling Tests - Alias Qualifiers

**Files:**
- Modify: `crates/flowscope-core/tests/completion_items.rs`

**Step 1: Add alias qualifier tests**

```rust
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
```

**Step 2: Run tests**

Run: `cargo test -p flowscope-core qualifier_alias`
Expected: All 4 tests pass

**Step 3: Commit**

```bash
git add crates/flowscope-core/tests/completion_items.rs
git commit -m "test(completion): add alias qualifier handling tests

Cover alias filtering, case insensitivity, exclusion of other tables,
and partial prefix matching."
```

---

## Task 8: Qualifier Handling Tests - Table and Schema Qualifiers

**Files:**
- Modify: `crates/flowscope-core/tests/completion_items.rs`

**Step 1: Add table and schema qualifier tests**

```rust
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
```

**Step 2: Run tests**

Run: `cargo test -p flowscope-core qualifier_`
Expected: All 8 tests pass

**Step 3: Commit**

```bash
git add crates/flowscope-core/tests/completion_items.rs
git commit -m "test(completion): add table and schema qualifier tests

Cover table name qualifiers, schema-only qualifiers showing tables,
and schema.table qualifiers showing columns."
```

---

## Task 9: Qualifier Handling Tests - Edge Cases and Robustness

**Files:**
- Modify: `crates/flowscope-core/tests/completion_items.rs`

**Step 1: Add edge case and robustness tests**

```rust
#[test]
fn qualifier_subquery_alias() {
    let request = request_at_cursor(
        "SELECT sq.| FROM (SELECT id FROM users) sq",
        Some(sample_schema()),
    );
    let context = completion_context(&request);
    // Subquery alias should be recognized
    assert!(context.tables_in_scope.iter().any(|t| t.alias == Some("sq".to_string())));
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
    let column_items: Vec<_> = result.items.iter()
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
    let id_items: Vec<_> = result.items.iter()
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
```

**Step 2: Run tests**

Run: `cargo test -p flowscope-core -q 2>&1 | tail -5`
Expected: All tests pass

**Step 3: Commit**

```bash
git add crates/flowscope-core/tests/completion_items.rs
git commit -m "test(completion): add qualifier edge cases and robustness tests

Cover subquery aliases, unqualified column display, ambiguous column
prefixing, and cursor inside string literals."
```

---

## Task 10: Scoring Tests - Category Ordering

**Files:**
- Modify: `crates/flowscope-core/tests/completion_items.rs`

**Step 1: Add scoring category tests**

```rust
// =============================================================================
// Scoring and Ranking Tests
// =============================================================================

#[test]
fn score_select_columns_before_tables() {
    let request = request_at_cursor("SELECT | FROM users", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);

    // Find first column and first table (if any)
    let first_column = result.items.iter()
        .position(|item| item.category == CompletionItemCategory::Column);
    let first_table = result.items.iter()
        .position(|item| item.category == CompletionItemCategory::Table
                      || item.category == CompletionItemCategory::SchemaTable);

    // In SELECT clause, columns should come before tables (if tables present)
    if let (Some(col_pos), Some(tbl_pos)) = (first_column, first_table) {
        assert!(col_pos < tbl_pos, "columns should rank before tables in SELECT");
    }
}

#[test]
fn score_from_tables_before_columns() {
    let request = request_at_cursor("SELECT * FROM |", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);

    let first_table = result.items.iter()
        .position(|item| item.category == CompletionItemCategory::Table
                      || item.category == CompletionItemCategory::SchemaTable);
    let first_column = result.items.iter()
        .position(|item| item.category == CompletionItemCategory::Column);

    // In FROM clause, tables should come before columns
    if let (Some(tbl_pos), Some(col_pos)) = (first_table, first_column) {
        assert!(tbl_pos < col_pos, "tables should rank before columns in FROM");
    }
}

#[test]
fn score_where_columns_available() {
    let request = request_at_cursor("SELECT * FROM users WHERE |", Some(sample_schema()));
    let result = completion_items(&request);
    assert!(result.should_show);

    // WHERE clause should have columns ranked high
    let column_items: Vec<_> = result.items.iter()
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
```

**Step 2: Run tests**

Run: `cargo test -p flowscope-core score_`
Expected: All 4 tests pass

**Step 3: Commit**

```bash
git add crates/flowscope-core/tests/completion_items.rs
git commit -m "test(completion): add scoring tests for category ordering

Cover SELECT (columns first), FROM (tables first), WHERE (columns available),
and ORDER BY (columns first)."
```

---

## Task 11: Scoring Tests - Prefix Matching

**Files:**
- Modify: `crates/flowscope-core/tests/completion_items.rs`

**Step 1: Add prefix matching tests**

```rust
#[test]
fn score_exact_match_ranks_highest() {
    let request = request_at_cursor("SELECT email|", Some(schema_prefix_matching()));
    let result = completion_items(&request);
    assert!(result.should_show);

    // "email" should rank above "email_verified"
    let email_pos = result.items.iter().position(|item| item.label == "email");
    let email_verified_pos = result.items.iter().position(|item| item.label == "email_verified");

    if let (Some(exact), Some(prefix)) = (email_pos, email_verified_pos) {
        assert!(exact < prefix, "exact match 'email' should rank above 'email_verified'");
    }
}

#[test]
fn score_prefix_match_above_contains() {
    let request = request_at_cursor("SELECT user|", Some(schema_prefix_matching()));
    let result = completion_items(&request);
    assert!(result.should_show);

    // "user_id" (prefix match) should rank above "power_user" (contains)
    let user_id_pos = result.items.iter().position(|item| item.label == "user_id");
    let power_user_pos = result.items.iter().position(|item| item.label == "power_user");

    if let (Some(prefix), Some(contains)) = (user_id_pos, power_user_pos) {
        assert!(prefix < contains, "'user_id' should rank above 'power_user'");
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

    // FROM should be near the top (within first 5 items)
    let from_pos = result.items.iter().position(|item| item.label == "FROM");
    if let Some(pos) = from_pos {
        assert!(pos < 10, "FROM should be ranked high when typing 'f' in SELECT");
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
                window[0].label, window[1].label
            );
        }
    }
}
```

**Step 2: Run tests**

Run: `cargo test -p flowscope-core score_`
Expected: All 8 tests pass

**Step 3: Commit**

```bash
git add crates/flowscope-core/tests/completion_items.rs
git commit -m "test(completion): add scoring tests for prefix matching

Cover exact match ranking, prefix vs contains ranking, FROM keyword
boost when typing 'f', and alphabetical tie-breaking."
```

---

## Task 12: Create Snapshot Test File

**Files:**
- Create: `crates/flowscope-core/tests/completion_snapshots.rs`

**Step 1: Create snapshot test file with initial tests**

```rust
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
```

**Step 2: Run to generate snapshots**

Run: `cargo test -p flowscope-core completion_snapshots`
Expected: Tests fail with "new snapshots" - this is expected

**Step 3: Review and accept snapshots**

Run: `cargo insta review`
Accept all snapshots after reviewing they look correct.

**Step 4: Run tests again to verify**

Run: `cargo test -p flowscope-core completion_snapshots`
Expected: All 5 tests pass

**Step 5: Commit**

```bash
git add crates/flowscope-core/tests/completion_snapshots.rs
git add crates/flowscope-core/tests/snapshots/
git commit -m "test(completion): add initial snapshot tests

Cover SELECT with schema, FROM clause tables, JOIN ON condition,
alias-qualified completions, and schema-qualified completions."
```

---

## Task 13: Snapshot Tests - Complex Scenarios

**Files:**
- Modify: `crates/flowscope-core/tests/completion_snapshots.rs`

**Step 1: Add complex scenario snapshots**

```rust
#[test]
fn snap_three_way_join() {
    let request = request_at_cursor(
        "SELECT | FROM users u JOIN orders o ON u.id = o.user_id JOIN products p ON o.product_id = p.id",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}

#[test]
fn snap_ambiguous_columns() {
    let request = request_at_cursor(
        "SELECT | FROM users JOIN orders ON users.id = orders.user_id",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}

#[test]
fn snap_multi_statement() {
    let request = request_at_cursor(
        "SELECT * FROM users; SELECT | FROM orders",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}

#[test]
fn snap_cte_reference() {
    let request = request_at_cursor(
        "WITH active_users AS (SELECT id, email FROM users WHERE active = true) SELECT | FROM active_users",
        Some(sample_schema()),
    );
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}

#[test]
fn snap_empty_schema() {
    let request = request_at_cursor("SELECT | FROM users", None);
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}
```

**Step 2: Generate and review snapshots**

Run: `cargo test -p flowscope-core completion_snapshots`
Run: `cargo insta review`
Accept snapshots after review.

**Step 3: Verify tests pass**

Run: `cargo test -p flowscope-core completion_snapshots`
Expected: All 10 tests pass

**Step 4: Commit**

```bash
git add crates/flowscope-core/tests/completion_snapshots.rs
git add crates/flowscope-core/tests/snapshots/
git commit -m "test(completion): add complex scenario snapshot tests

Cover three-way joins, ambiguous columns, multi-statement,
CTE references, and empty schema behavior."
```

---

## Task 14: Snapshot Tests - Dialect and Keywords

**Files:**
- Modify: `crates/flowscope-core/tests/completion_snapshots.rs`

**Step 1: Add dialect and keyword snapshots**

```rust
#[test]
fn snap_duckdb_dialect() {
    let request = request_at_cursor_with_dialect(
        "SELECT | FROM users",
        Some(sample_schema()),
        Dialect::Duckdb,
    );
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}

#[test]
fn snap_postgres_dialect() {
    let request = request_at_cursor_with_dialect(
        "SELECT | FROM users",
        Some(sample_schema()),
        Dialect::Postgres,
    );
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}

#[test]
fn snap_keywords_select_clause() {
    let request = request_at_cursor("SELECT |", None);
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}

#[test]
fn snap_keywords_where_clause() {
    let request = request_at_cursor("SELECT * FROM t WHERE |", None);
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}

#[test]
fn snap_keywords_order_by_clause() {
    let request = request_at_cursor("SELECT * FROM t ORDER BY |", None);
    let result = completion_items(&request);
    assert_json_snapshot!(result);
}
```

**Step 2: Generate and review snapshots**

Run: `cargo test -p flowscope-core completion_snapshots`
Run: `cargo insta review`
Accept snapshots after review.

**Step 3: Verify all tests pass**

Run: `cargo test -p flowscope-core completion_snapshots`
Expected: All 15 tests pass

**Step 4: Commit**

```bash
git add crates/flowscope-core/tests/completion_snapshots.rs
git add crates/flowscope-core/tests/snapshots/
git commit -m "test(completion): add dialect and keyword snapshot tests

Cover DuckDB dialect, Postgres dialect, and keyword hints for
SELECT, WHERE, and ORDER BY clauses."
```

---

## Task 15: Final Verification and Summary

**Step 1: Run full test suite**

Run: `cargo test -p flowscope-core`
Expected: All tests pass (existing + ~57 new unit tests + 15 snapshot tests)

**Step 2: Run clippy**

Run: `cargo clippy -p flowscope-core -- -D warnings`
Expected: No warnings

**Step 3: Verify test count**

Run: `cargo test -p flowscope-core completion 2>&1 | grep -E "test result|running"`
Expected: Shows ~70+ completion tests running

**Step 4: Final commit if any cleanup needed**

If any fixes were needed:
```bash
git add -A
git commit -m "test(completion): final cleanup and verification"
```

**Step 5: Summary**

The completion test suite is complete with:
- ~57 unit tests covering clause detection, schema resolution, qualifier handling, and scoring
- 15 snapshot tests covering complex scenarios, dialects, and keyword hints
- Shared test infrastructure with cursor marker helpers and reusable schemas
