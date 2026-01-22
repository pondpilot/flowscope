//! Integration tests for SQL templating functionality.

use flowscope_core::{analyze, AnalyzeRequest, Dialect, NodeType};
use std::collections::HashMap;

#[cfg(feature = "templating")]
use flowscope_core::{TemplateConfig, TemplateMode};

/// Helper to run analysis with templating.
#[cfg(feature = "templating")]
fn analyze_with_template(
    sql: &str,
    mode: TemplateMode,
    context: HashMap<String, serde_json::Value>,
) -> flowscope_core::AnalyzeResult {
    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: Some("test.sql".to_string()),
        options: None,
        schema: None,
        template_config: Some(TemplateConfig { mode, context }),
    };

    analyze(&request)
}

/// Helper to check if a table with the given name exists in the result.
/// Checks both the label and qualified_name fields.
fn has_table(result: &flowscope_core::AnalyzeResult, table_name: &str) -> bool {
    result.statements.iter().any(|stmt| {
        stmt.nodes.iter().any(|node| {
            if node.node_type != NodeType::Table {
                return false;
            }
            // Check label first
            if &*node.label == table_name {
                return true;
            }
            // Check qualified_name if present
            if let Some(ref qn) = node.qualified_name {
                return &**qn == table_name;
            }
            false
        })
    })
}

// ============================================================================
// Jinja Mode Tests
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn jinja_variable_substitution() {
    let sql = "SELECT * FROM {{ table_name }}";
    let mut context = HashMap::new();
    context.insert("table_name".to_string(), serde_json::json!("users"));

    let result = analyze_with_template(sql, TemplateMode::Jinja, context);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );
    assert!(has_table(&result, "users"), "Should detect 'users' table");
}

#[test]
#[cfg(feature = "templating")]
fn jinja_conditional_included() {
    let sql = r#"
        SELECT id, name
        {% if include_email %}, email{% endif %}
        FROM users
    "#;
    let mut context = HashMap::new();
    context.insert("include_email".to_string(), serde_json::json!(true));

    let result = analyze_with_template(sql, TemplateMode::Jinja, context);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );
    assert!(has_table(&result, "users"), "Should detect 'users' table");
}

#[test]
#[cfg(feature = "templating")]
fn jinja_conditional_excluded() {
    let sql = r#"
        SELECT id, name
        {% if include_email %}, email{% endif %}
        FROM users
    "#;
    let mut context = HashMap::new();
    context.insert("include_email".to_string(), serde_json::json!(false));

    let result = analyze_with_template(sql, TemplateMode::Jinja, context);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );
    assert!(has_table(&result, "users"), "Should detect 'users' table");
}

#[test]
#[cfg(feature = "templating")]
fn jinja_loop_expansion() {
    let sql = r#"
        SELECT
            {% for col in columns %}{{ col }}{% if not loop.last %}, {% endif %}{% endfor %}
        FROM users
    "#;
    let mut context = HashMap::new();
    context.insert(
        "columns".to_string(),
        serde_json::json!(["id", "name", "email"]),
    );

    let result = analyze_with_template(sql, TemplateMode::Jinja, context);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );
    assert!(has_table(&result, "users"), "Should detect 'users' table");
}

#[test]
#[cfg(feature = "templating")]
fn jinja_undefined_variable_error() {
    let sql = "SELECT * FROM {{ undefined_table }}";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Jinja, context);

    // Should have a TEMPLATE_ERROR issue
    assert!(
        result.issues.iter().any(|i| i.code == "TEMPLATE_ERROR"),
        "Should report template error for undefined variable: {:?}",
        result.issues
    );
}

// ============================================================================
// dbt Mode Tests
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn dbt_ref_single_arg() {
    let sql = "SELECT * FROM {{ ref('orders') }}";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );
    assert!(
        has_table(&result, "orders"),
        "Should detect 'orders' table from ref()"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_ref_two_args() {
    let sql = "SELECT * FROM {{ ref('analytics', 'users') }}";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );
    // ref('project', 'model') returns "project.model"
    assert!(
        has_table(&result, "analytics.users"),
        "Should detect 'analytics.users' table from ref(): {:?}",
        result.statements.first().map(|s| &s.nodes)
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_source_macro() {
    let sql = "SELECT * FROM {{ source('raw', 'events') }}";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );
    // source('schema', 'table') returns "schema.table"
    assert!(
        has_table(&result, "raw.events"),
        "Should detect 'raw.events' table from source(): {:?}",
        result.statements.first().map(|s| &s.nodes)
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_config_returns_empty() {
    let sql = "{{ config(materialized='table') }}SELECT * FROM users";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );
    assert!(has_table(&result, "users"), "Should detect 'users' table");
}

#[test]
#[cfg(feature = "templating")]
fn dbt_var_with_default() {
    let sql = "SELECT * FROM {{ var('schema', 'public') }}.users";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );
    // var() with default should use the default value
    assert!(
        has_table(&result, "public.users"),
        "Should detect 'public.users' table: {:?}",
        result.statements.first().map(|s| &s.nodes)
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_var_from_context() {
    let sql = "SELECT * FROM {{ var('schema', 'public') }}.users";
    let mut context = HashMap::new();
    context.insert(
        "vars".to_string(),
        serde_json::json!({ "schema": "analytics" }),
    );

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );
    // var() with context should use the context value
    assert!(
        has_table(&result, "analytics.users"),
        "Should detect 'analytics.users' table: {:?}",
        result.statements.first().map(|s| &s.nodes)
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_is_incremental_returns_false() {
    let sql = r#"
        SELECT * FROM users
        {% if is_incremental() %}
        WHERE created_at > (SELECT MAX(created_at) FROM {{ this }})
        {% endif %}
    "#;
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );
    assert!(has_table(&result, "users"), "Should detect 'users' table");
    // is_incremental() returns false, so the WHERE clause should be excluded
    // and {{ this }} should not be evaluated
}

#[test]
#[cfg(feature = "templating")]
fn dbt_complex_model() {
    let sql = r#"
        {{ config(materialized='incremental') }}

        WITH stg AS (
            SELECT * FROM {{ ref('staging_orders') }}
        )
        SELECT
            id,
            amount,
            '{{ var("version", "v1") }}' AS version
        FROM stg
        {% if is_incremental() %}
        WHERE updated_at > (SELECT MAX(updated_at) FROM {{ this }})
        {% endif %}
    "#;
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );
    assert!(
        has_table(&result, "staging_orders"),
        "Should detect 'staging_orders' from ref()"
    );
}

// ============================================================================
// Raw Mode Tests (No Templating)
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn raw_mode_passes_through() {
    let sql = "SELECT * FROM {{ not_a_template }}";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Raw, context);

    // Raw mode doesn't template, so {{ not_a_template }} is passed as-is to the parser
    // This will likely cause a parse error since it's not valid SQL
    assert!(
        result.summary.has_errors,
        "Raw mode should not template, causing parse error"
    );
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn empty_template_context() {
    let sql = "SELECT * FROM {{ ref('users') }}";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "dbt mode should work with empty context"
    );
    assert!(has_table(&result, "users"));
}

#[test]
#[cfg(feature = "templating")]
fn syntax_error_in_template() {
    let sql = "SELECT * FROM {{ unclosed";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Jinja, context);

    assert!(
        result.issues.iter().any(|i| i.code == "TEMPLATE_ERROR"),
        "Should report template syntax error"
    );
}

// ============================================================================
// Custom Macro Tests (dbt packages and project macros)
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn dbt_custom_macro_passthrough() {
    // Custom macros should be stubbed and not cause errors
    let sql = "SELECT {{ cents_to_dollars('amount') }} as amount_dollars FROM orders";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Custom macros should not cause errors: {:?}",
        result.issues
    );
    assert!(has_table(&result, "orders"), "Should detect 'orders' table");
}

#[test]
#[cfg(feature = "templating")]
fn dbt_utils_namespace_macro() {
    // dbt_utils.* macros should work via namespace passthrough
    let sql = "SELECT {{ dbt_utils.star(from=ref('users')) }} FROM {{ ref('users') }}";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "dbt_utils.* macros should not cause errors: {:?}",
        result.issues
    );
    assert!(has_table(&result, "users"), "Should detect 'users' table");
}

#[test]
#[cfg(feature = "templating")]
fn dbt_complex_with_multiple_custom_macros() {
    let sql = r#"
        {{ config(materialized='table') }}

        WITH source AS (
            SELECT
                {{ generate_surrogate_key(['order_id', 'customer_id']) }} as sk,
                {{ cents_to_dollars('amount') }} as amount_dollars
            FROM {{ ref('raw_orders') }}
        )
        SELECT * FROM source
    "#;
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Multiple custom macros should not cause errors: {:?}",
        result.issues
    );
    assert!(
        has_table(&result, "raw_orders"),
        "Should detect 'raw_orders' table"
    );
}

// ============================================================================
// Security and DoS Protection Tests
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn jinja_recursion_limit_protection() {
    // Create a deeply nested template that would trigger recursion limits
    // MiniJinja limits recursion by default; our limit of 100 should catch this
    let sql = r#"
        {% macro deep(n) %}
            {% if n > 0 %}{{ deep(n - 1) }}{% else %}done{% endif %}
        {% endmacro %}
        SELECT '{{ deep(200) }}' as result FROM users
    "#;
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Jinja, context);

    // Should fail with a template error due to recursion limit
    assert!(
        result.issues.iter().any(|i| i.code == "TEMPLATE_ERROR"),
        "Deep recursion should trigger template error: {:?}",
        result.issues
    );
}

#[test]
#[cfg(feature = "templating")]
fn jinja_context_values_with_special_chars() {
    // Test that special characters in context values work correctly
    // Note: Jinja does simple string substitution - it's the user's responsibility
    // to ensure context values produce valid SQL. This test verifies that the
    // templating system itself handles special characters without crashing.
    let sql = "SELECT * FROM {{ table_name }}";
    let mut context = HashMap::new();
    // Use a table name with underscores and numbers (valid SQL identifier)
    context.insert(
        "table_name".to_string(),
        serde_json::json!("user_data_2024"),
    );

    let result = analyze_with_template(sql, TemplateMode::Jinja, context);

    assert!(
        !result.summary.has_errors,
        "Context values should be safely included: {:?}",
        result.issues
    );
    assert!(
        has_table(&result, "user_data_2024"),
        "Should detect table with special chars"
    );
}

#[test]
#[cfg(feature = "templating")]
fn jinja_context_with_json_array() {
    // Test that JSON arrays in context are handled correctly
    let sql = r#"
        SELECT
            {% for col in columns %}{{ col }}{% if not loop.last %}, {% endif %}{% endfor %}
        FROM users
    "#;
    let mut context = HashMap::new();
    context.insert(
        "columns".to_string(),
        serde_json::json!(["id", "name", "email", "created_at"]),
    );

    let result = analyze_with_template(sql, TemplateMode::Jinja, context);

    assert!(
        !result.summary.has_errors,
        "JSON array context should work: {:?}",
        result.issues
    );
    assert!(has_table(&result, "users"), "Should detect 'users' table");
}

#[test]
#[cfg(feature = "templating")]
fn dbt_many_unknown_macros_error_message() {
    // Test that many different unknown macros produce a helpful error message
    // with the list of stubbed functions
    let mut sql = "SELECT ".to_string();
    for i in 0..55 {
        if i > 0 {
            sql.push_str(", ");
        }
        sql.push_str(&format!("{{{{ unknown_macro_{i}('arg') }}}}", i = i));
    }
    sql.push_str(" FROM users");

    let context = HashMap::new();
    let result = analyze_with_template(&sql, TemplateMode::Dbt, context);

    // Should have TEMPLATE_ERROR with details about stubbed functions
    let template_error = result.issues.iter().find(|i| i.code == "TEMPLATE_ERROR");
    assert!(
        template_error.is_some(),
        "Should have template error for too many unknown macros"
    );

    let error_msg = &template_error.unwrap().message;
    assert!(
        error_msg.contains("unknown_macro_") || error_msg.contains("Too many"),
        "Error message should mention the stubbed functions or limit: {}",
        error_msg
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_context_with_nested_json() {
    // Test that complex nested JSON in context is handled correctly
    let sql = "SELECT {{ var('config') }} as config FROM users";
    let mut context = HashMap::new();
    context.insert(
        "vars".to_string(),
        serde_json::json!({
            "config": {
                "nested": {
                    "deep": "value"
                }
            }
        }),
    );

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    // Should not crash, though the output might not be valid SQL
    // The important thing is it doesn't panic or hang
    assert!(
        result.issues.is_empty() || result.issues.iter().all(|i| i.code != "PANIC"),
        "Complex context should not cause panic"
    );
}

// ============================================================================
// RelationEmulator Integration Tests
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn dbt_ref_relation_attribute_access() {
    // Test that ref().identifier works in lineage analysis
    let sql = "SELECT * FROM {{ ref('orders').identifier }}";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Relation attribute access should work: {:?}",
        result.issues
    );
    assert!(
        has_table(&result, "orders"),
        "Should detect 'orders' from ref().identifier"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_source_relation_attribute_access() {
    // Test that source().schema works
    let sql = "SELECT '{{ source('raw', 'events').schema }}' as schema_name, * FROM {{ source('raw', 'events') }}";
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Source relation attribute should work: {:?}",
        result.issues
    );
    assert!(
        has_table(&result, "raw.events"),
        "Should detect 'raw.events' from source()"
    );
}

// ============================================================================
// this Global Integration Tests
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn dbt_this_with_model_context() {
    let sql = r#"
        SELECT * FROM {{ ref('source_table') }}
        {% if is_incremental() %}
        WHERE updated_at > (SELECT MAX(updated_at) FROM {{ this }})
        {% endif %}
    "#;

    let mut context = HashMap::new();
    context.insert("model_name".to_string(), serde_json::json!("target_model"));
    context.insert("schema".to_string(), serde_json::json!("analytics"));

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "this with model context should work: {:?}",
        result.issues
    );
    // is_incremental() returns false, so this block is skipped
    assert!(
        has_table(&result, "source_table"),
        "Should detect 'source_table'"
    );
}

// ============================================================================
// zip() Function Integration Tests
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn dbt_zip_function_generates_valid_sql() {
    // Test that zip() generates valid SQL when used to build column lists
    let sql = r#"
        SELECT
            {% for col, alias in zip(['user_id', 'email'], ['id', 'contact']) %}
            {{ col }} AS {{ alias }}{% if not loop.last %},{% endif %}
            {% endfor %}
        FROM {{ ref('users') }}
    "#;
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "zip() should produce valid SQL: {:?}",
        result.issues
    );
    assert!(has_table(&result, "users"), "Should detect 'users' table");
}

#[test]
#[cfg(feature = "templating")]
fn dbt_zip_with_context_arrays() {
    // Test zip() with arrays from template context
    let sql = r#"
        SELECT
            {% for src, tgt in zip(source_cols, target_cols) %}
            {{ src }} AS {{ tgt }}{% if not loop.last %},{% endif %}
            {% endfor %}
        FROM {{ ref('data') }}
    "#;
    let mut context = HashMap::new();
    context.insert(
        "source_cols".to_string(),
        serde_json::json!(["col_a", "col_b"]),
    );
    context.insert(
        "target_cols".to_string(),
        serde_json::json!(["alias_a", "alias_b"]),
    );

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "zip() with context arrays should work: {:?}",
        result.issues
    );
    assert!(has_table(&result, "data"), "Should detect 'data' table");
}

#[test]
#[cfg(feature = "templating")]
fn dbt_zip_strict_with_equal_lengths() {
    // Test zip_strict() works with equal-length arrays
    let sql = r#"
        SELECT
            {% for a, b in zip_strict(['x', 'y'], [1, 2]) %}
            '{{ a }}' AS col_{{ b }}{% if not loop.last %},{% endif %}
            {% endfor %}
        FROM {{ ref('items') }}
    "#;
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "zip_strict() with equal lengths should work: {:?}",
        result.issues
    );
    assert!(has_table(&result, "items"), "Should detect 'items' table");
}

// ============================================================================
// MiniJinja Built-in Feature Integration Tests
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn jinja_loop_first_in_sql() {
    // Test loop.first generates valid SQL
    let sql = r#"
        SELECT
            {% for col in columns %}
            {% if loop.first %}{{ col }}{% else %}, {{ col }}{% endif %}
            {% endfor %}
        FROM users
    "#;
    let mut context = HashMap::new();
    context.insert(
        "columns".to_string(),
        serde_json::json!(["id", "name", "email"]),
    );

    let result = analyze_with_template(sql, TemplateMode::Jinja, context);

    assert!(
        !result.summary.has_errors,
        "loop.first should produce valid SQL: {:?}",
        result.issues
    );
    assert!(has_table(&result, "users"), "Should detect 'users' table");
}

#[test]
#[cfg(feature = "templating")]
fn jinja_whitespace_control_in_sql() {
    // Test whitespace control produces clean SQL
    let sql = r#"SELECT
        {%- for col in ['a', 'b', 'c'] %}
        {{ col }}
        {%- if not loop.last %},{% endif %}
        {%- endfor %}
        FROM users"#;
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Jinja, context);

    assert!(
        !result.summary.has_errors,
        "Whitespace control should produce valid SQL: {:?}",
        result.issues
    );
    assert!(has_table(&result, "users"), "Should detect 'users' table");
}

// ============================================================================
// env_var() Integration Tests
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn dbt_env_var_in_config() {
    let sql = "SELECT * FROM {{ env_var('TARGET_SCHEMA', 'public') }}.users";
    let mut context = HashMap::new();
    context.insert(
        "env_vars".to_string(),
        serde_json::json!({ "TARGET_SCHEMA": "production" }),
    );

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "env_var should work: {:?}",
        result.issues
    );
    assert!(
        has_table(&result, "production.users"),
        "Should detect 'production.users' from env_var"
    );
}

// ============================================================================
// execute Flag Integration Tests
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn dbt_execute_flag_skips_run_query() {
    // Common pattern: only run query when execute is true
    let sql = r#"
        {% if execute %}
        {% set results = run_query("SELECT DISTINCT category FROM products") %}
        {% for row in results %}
        UNION ALL SELECT '{{ row.category }}' as category
        {% endfor %}
        {% endif %}
        SELECT * FROM products
    "#;
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "execute flag pattern should work: {:?}",
        result.issues
    );
    assert!(
        has_table(&result, "products"),
        "Should detect 'products' table"
    );
}

// ============================================================================
// Tag Preprocessing Integration Tests
// ============================================================================

#[test]
#[cfg(feature = "templating")]
fn dbt_test_block_stripped() {
    let sql = r#"
        {% test unique_orders(model) %}
        SELECT order_id FROM {{ model }} GROUP BY order_id HAVING COUNT(*) > 1
        {% endtest %}

        SELECT * FROM {{ ref('orders') }}
    "#;
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Test block should be stripped: {:?}",
        result.issues
    );
    assert!(
        has_table(&result, "orders"),
        "Should detect 'orders' from main query"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_snapshot_block_content_preserved() {
    let sql = r#"
        {% snapshot orders_snapshot %}
        {{ config(unique_key='id', strategy='timestamp', updated_at='updated_at') }}
        SELECT * FROM {{ source('raw', 'orders') }}
        {% endsnapshot %}
    "#;
    let context = HashMap::new();

    let result = analyze_with_template(sql, TemplateMode::Dbt, context);

    assert!(
        !result.summary.has_errors,
        "Snapshot content should be preserved: {:?}",
        result.issues
    );
    assert!(
        has_table(&result, "raw.orders"),
        "Should detect 'raw.orders' from snapshot content"
    );
}

// ============================================================================
// dbt Lineage Tests (Jaffle Shop based)
// ============================================================================
// These tests verify column-level lineage through dbt-templated SQL files.
// Based on the Jaffle Shop demo project structure:
//   - staging models: stg_customers, stg_orders, stg_payments
//   - intermediate: int_orders_payments
//   - marts: customers, orders, daily_revenue

#[cfg(feature = "templating")]
use flowscope_core::{EdgeType, FileSource};

/// Helper to run multi-file dbt analysis.
#[cfg(feature = "templating")]
fn analyze_dbt_files(files: Vec<FileSource>) -> flowscope_core::AnalyzeResult {
    let request = AnalyzeRequest {
        sql: String::new(),
        files: Some(files),
        dialect: Dialect::Postgres,
        source_name: None,
        options: None,
        schema: None,
        template_config: Some(TemplateConfig {
            mode: TemplateMode::Dbt,
            context: HashMap::new(),
        }),
    };

    analyze(&request)
}

/// Helper to count derivation edges in a statement.
#[cfg(feature = "templating")]
fn count_derivation_edges(result: &flowscope_core::AnalyzeResult) -> usize {
    result
        .statements
        .iter()
        .flat_map(|stmt| &stmt.edges)
        .filter(|edge| edge.edge_type == EdgeType::Derivation)
        .count()
}

#[test]
#[cfg(feature = "templating")]
fn dbt_lineage_staging_to_intermediate() {
    // Test lineage from staging models through intermediate model
    // stg_orders + stg_payments -> int_orders_payments
    let stg_orders = r#"
{{ config(materialized='view') }}
SELECT
    id AS order_id,
    user_id AS customer_id,
    order_date,
    status
FROM {{ source('jaffle_shop', 'raw_orders') }}
"#;

    let stg_payments = r#"
{{ config(materialized='view') }}
SELECT
    id AS payment_id,
    order_id,
    amount / 100.0 AS amount
FROM {{ source('stripe', 'payments') }}
"#;

    let int_orders_payments = r#"
{{ config(materialized='table') }}
WITH orders AS (
    SELECT * FROM {{ ref('stg_orders') }}
),
payments AS (
    SELECT * FROM {{ ref('stg_payments') }}
)
SELECT
    orders.order_id,
    orders.customer_id,
    orders.order_date,
    COALESCE(SUM(payments.amount), 0) AS total_amount,
    COUNT(payments.payment_id) AS payment_count
FROM orders
LEFT JOIN payments ON orders.order_id = payments.order_id
GROUP BY 1, 2, 3
"#;

    let result = analyze_dbt_files(vec![
        FileSource {
            name: "stg_orders.sql".to_string(),
            content: stg_orders.to_string(),
        },
        FileSource {
            name: "stg_payments.sql".to_string(),
            content: stg_payments.to_string(),
        },
        FileSource {
            name: "int_orders_payments.sql".to_string(),
            content: int_orders_payments.to_string(),
        },
    ]);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );

    // Verify source tables are detected
    assert!(
        has_table(&result, "jaffle_shop.raw_orders"),
        "Should detect source table 'jaffle_shop.raw_orders'"
    );
    assert!(
        has_table(&result, "stripe.payments"),
        "Should detect source table 'stripe.payments'"
    );

    // Verify ref() tables are detected
    assert!(
        has_table(&result, "stg_orders"),
        "Should detect ref'd table 'stg_orders'"
    );
    assert!(
        has_table(&result, "stg_payments"),
        "Should detect ref'd table 'stg_payments'"
    );

    // Verify cross-statement edges exist (table->CTE and column DataFlow edges)
    let cross_edges = &result.global_lineage.edges;
    assert!(!cross_edges.is_empty(), "Should have cross-statement edges");

    // Verify DataFlow edges exist (column lineage across files)
    let dataflow_edges: Vec<_> = cross_edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::DataFlow)
        .collect();
    assert!(
        !dataflow_edges.is_empty(),
        "Should have DataFlow edges for cross-file column lineage"
    );

    // Verify derivation edges are present (column lineage)
    let derivation_count = count_derivation_edges(&result);
    assert!(
        derivation_count > 0,
        "Should have derivation edges for column lineage, got {}",
        derivation_count
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_lineage_full_dag() {
    // Test full DAG: sources -> staging -> intermediate -> mart
    let stg_customers = r#"
{{ config(materialized='view') }}
SELECT
    id AS customer_id,
    first_name,
    last_name,
    first_name || ' ' || last_name AS full_name
FROM {{ source('jaffle_shop', 'raw_customers') }}
"#;

    let stg_orders = r#"
{{ config(materialized='view') }}
SELECT
    id AS order_id,
    user_id AS customer_id,
    order_date,
    status
FROM {{ source('jaffle_shop', 'raw_orders') }}
"#;

    let stg_payments = r#"
{{ config(materialized='view') }}
SELECT
    id AS payment_id,
    order_id,
    amount / 100.0 AS amount
FROM {{ source('stripe', 'payments') }}
"#;

    let int_orders_payments = r#"
{{ config(materialized='table') }}
WITH orders AS (
    SELECT * FROM {{ ref('stg_orders') }}
),
payments AS (
    SELECT * FROM {{ ref('stg_payments') }}
)
SELECT
    orders.order_id,
    orders.customer_id,
    orders.order_date,
    COALESCE(SUM(payments.amount), 0) AS total_amount
FROM orders
LEFT JOIN payments ON orders.order_id = payments.order_id
GROUP BY 1, 2, 3
"#;

    let customers_mart = r#"
{{ config(materialized='table') }}
WITH customers AS (
    SELECT * FROM {{ ref('stg_customers') }}
),
orders AS (
    SELECT * FROM {{ ref('int_orders_payments') }}
),
customer_orders AS (
    SELECT
        customer_id,
        MIN(order_date) AS first_order_date,
        MAX(order_date) AS most_recent_order_date,
        COUNT(order_id) AS number_of_orders,
        SUM(total_amount) AS lifetime_value
    FROM orders
    GROUP BY customer_id
)
SELECT
    customers.customer_id,
    customers.first_name,
    customers.last_name,
    customers.full_name,
    customer_orders.first_order_date,
    customer_orders.most_recent_order_date,
    COALESCE(customer_orders.number_of_orders, 0) AS number_of_orders,
    COALESCE(customer_orders.lifetime_value, 0) AS lifetime_value
FROM customers
LEFT JOIN customer_orders USING (customer_id)
"#;

    let result = analyze_dbt_files(vec![
        FileSource {
            name: "stg_customers.sql".to_string(),
            content: stg_customers.to_string(),
        },
        FileSource {
            name: "stg_orders.sql".to_string(),
            content: stg_orders.to_string(),
        },
        FileSource {
            name: "stg_payments.sql".to_string(),
            content: stg_payments.to_string(),
        },
        FileSource {
            name: "int_orders_payments.sql".to_string(),
            content: int_orders_payments.to_string(),
        },
        FileSource {
            name: "customers.sql".to_string(),
            content: customers_mart.to_string(),
        },
    ]);

    assert!(
        !result.summary.has_errors,
        "Full DAG analysis should succeed: {:?}",
        result.issues
    );

    // Verify all source tables
    assert!(has_table(&result, "jaffle_shop.raw_customers"));
    assert!(has_table(&result, "jaffle_shop.raw_orders"));
    assert!(has_table(&result, "stripe.payments"));

    // Verify all staging tables via ref()
    assert!(has_table(&result, "stg_customers"));
    assert!(has_table(&result, "stg_orders"));
    assert!(has_table(&result, "stg_payments"));

    // Verify intermediate table
    assert!(has_table(&result, "int_orders_payments"));

    // Verify cross-statement lineage exists
    let cross_edges = &result.global_lineage.edges;
    assert!(
        !cross_edges.is_empty(),
        "Should have cross-statement edges in full DAG"
    );

    // Verify derivation edges for column lineage
    let derivation_count = count_derivation_edges(&result);
    assert!(
        derivation_count >= 5,
        "Full DAG should have multiple derivation edges, got {}",
        derivation_count
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_lineage_column_derivation_through_ref() {
    // Test that column derivation is tracked through ref() calls
    let stg_orders = r#"
SELECT
    id AS order_id,
    user_id AS customer_id,
    total_amount
FROM {{ source('shop', 'orders') }}
"#;

    let orders_mart = r#"
SELECT
    order_id,
    customer_id,
    total_amount,
    total_amount * 0.1 AS tax_amount
FROM {{ ref('stg_orders') }}
"#;

    let result = analyze_dbt_files(vec![
        FileSource {
            name: "stg_orders.sql".to_string(),
            content: stg_orders.to_string(),
        },
        FileSource {
            name: "orders_mart.sql".to_string(),
            content: orders_mart.to_string(),
        },
    ]);

    assert!(
        !result.summary.has_errors,
        "Column derivation analysis should succeed: {:?}",
        result.issues
    );

    // Find the orders_mart statement (second file)
    let mart_stmt = result
        .statements
        .iter()
        .find(|s| s.source_name.as_deref() == Some("orders_mart.sql"))
        .expect("Should find orders_mart statement");

    // Check for output columns
    let output_columns: Vec<_> = mart_stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Column)
        .map(|n| n.label.as_ref())
        .collect();

    assert!(
        output_columns.contains(&"order_id"),
        "Should have order_id column"
    );
    assert!(
        output_columns.contains(&"tax_amount"),
        "Should have derived tax_amount column"
    );

    // Verify derivation edges exist for the derived column
    let derivations: Vec<_> = mart_stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Derivation)
        .collect();

    assert!(
        !derivations.is_empty(),
        "Should have derivation edges for tax_amount calculation"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_lineage_with_var_substitution() {
    // Test that var() substitution doesn't break lineage
    let model = r#"
SELECT
    order_id,
    customer_id,
    order_date
FROM {{ source('shop', 'orders') }}
WHERE order_date >= '{{ var("min_date", "2020-01-01") }}'
"#;

    let result = analyze_dbt_files(vec![FileSource {
        name: "filtered_orders.sql".to_string(),
        content: model.to_string(),
    }]);

    assert!(
        !result.summary.has_errors,
        "var() substitution should not break analysis: {:?}",
        result.issues
    );

    assert!(
        has_table(&result, "shop.orders"),
        "Should detect source table through var() usage"
    );

    // Verify output columns are captured
    let stmt = result.statements.first().expect("Should have statement");
    let columns: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Column)
        .collect();

    assert!(
        columns.len() >= 3,
        "Should capture output columns: order_id, customer_id, order_date"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_lineage_window_functions_through_ref() {
    // Test window function lineage through ref()
    let orders = r#"
SELECT
    order_id,
    customer_id,
    total_amount,
    order_date
FROM {{ source('shop', 'orders') }}
"#;

    let orders_ranked = r#"
SELECT
    order_id,
    customer_id,
    total_amount,
    ROW_NUMBER() OVER (PARTITION BY customer_id ORDER BY order_date) AS order_seq,
    SUM(total_amount) OVER (PARTITION BY customer_id ORDER BY order_date) AS running_total
FROM {{ ref('orders') }}
"#;

    let result = analyze_dbt_files(vec![
        FileSource {
            name: "orders.sql".to_string(),
            content: orders.to_string(),
        },
        FileSource {
            name: "orders_ranked.sql".to_string(),
            content: orders_ranked.to_string(),
        },
    ]);

    assert!(
        !result.summary.has_errors,
        "Window function analysis should succeed: {:?}",
        result.issues
    );

    // Find the orders_ranked statement
    let ranked_stmt = result
        .statements
        .iter()
        .find(|s| s.source_name.as_deref() == Some("orders_ranked.sql"))
        .expect("Should find orders_ranked statement");

    // Check for window function derived columns
    let columns: Vec<_> = ranked_stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Column)
        .map(|n| n.label.as_ref())
        .collect();

    assert!(
        columns.contains(&"order_seq"),
        "Should have ROW_NUMBER() derived column"
    );
    assert!(
        columns.contains(&"running_total"),
        "Should have SUM() window function column"
    );

    // Verify derivation edges for window functions
    // Window functions produce derivation edges when they transform columns
    let derivations: Vec<_> = ranked_stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Derivation)
        .collect();

    assert!(
        !derivations.is_empty(),
        "Should have derivation edges for window function columns, got {}",
        derivations.len()
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_lineage_aggregation_through_ref() {
    // Test aggregation lineage through ref()
    let orders = r#"
SELECT order_id, customer_id, total_amount, order_date
FROM {{ source('shop', 'orders') }}
"#;

    let daily_revenue = r#"
SELECT
    order_date,
    COUNT(DISTINCT order_id) AS total_orders,
    COUNT(DISTINCT customer_id) AS unique_customers,
    SUM(total_amount) AS revenue,
    AVG(total_amount) AS avg_order_value
FROM {{ ref('orders') }}
GROUP BY order_date
"#;

    let result = analyze_dbt_files(vec![
        FileSource {
            name: "orders.sql".to_string(),
            content: orders.to_string(),
        },
        FileSource {
            name: "daily_revenue.sql".to_string(),
            content: daily_revenue.to_string(),
        },
    ]);

    assert!(
        !result.summary.has_errors,
        "Aggregation analysis should succeed: {:?}",
        result.issues
    );

    // Find the daily_revenue statement
    let revenue_stmt = result
        .statements
        .iter()
        .find(|s| s.source_name.as_deref() == Some("daily_revenue.sql"))
        .expect("Should find daily_revenue statement");

    // Check for aggregated columns
    let columns: Vec<_> = revenue_stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Column)
        .map(|n| n.label.as_ref())
        .collect();

    assert!(
        columns.contains(&"total_orders"),
        "Should have COUNT column"
    );
    assert!(columns.contains(&"revenue"), "Should have SUM column");
    assert!(
        columns.contains(&"avg_order_value"),
        "Should have AVG column"
    );

    // Verify aggregation creates derivation edges
    let derivations: Vec<_> = revenue_stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Derivation)
        .collect();

    assert!(
        derivations.len() >= 3,
        "Should have derivation edges for aggregated columns, got {}",
        derivations.len()
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_model_cross_statement_linking() {
    // Test that dbt models are registered as produced tables for cross-statement linking.
    // When file stg_orders.sql produces a bare SELECT, the model name "stg_orders"
    // should be registered so that later files can resolve {{ ref('stg_orders') }}.
    let stg_orders = r#"
{{ config(materialized='view') }}
SELECT
    id AS order_id,
    user_id AS customer_id,
    order_date
FROM raw_orders
"#;

    let orders_summary = r#"
SELECT
    customer_id,
    COUNT(*) AS order_count
FROM {{ ref('stg_orders') }}
GROUP BY customer_id
"#;

    let result = analyze_dbt_files(vec![
        FileSource {
            name: "models/staging/stg_orders.sql".to_string(),
            content: stg_orders.to_string(),
        },
        FileSource {
            name: "models/marts/orders_summary.sql".to_string(),
            content: orders_summary.to_string(),
        },
    ]);

    // Check for UNRESOLVED_REFERENCE warnings - there should be none for stg_orders
    let unresolved_warnings: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == "UNRESOLVED_REFERENCE" && i.message.contains("stg_orders"))
        .collect();

    assert!(
        unresolved_warnings.is_empty(),
        "Should NOT have unresolved reference warnings for stg_orders (model should be registered): {:?}",
        unresolved_warnings
    );

    // Verify cross-statement edges exist
    let cross_edges: Vec<_> = result
        .global_lineage
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::CrossStatement)
        .collect();

    assert!(
        !cross_edges.is_empty(),
        "Should have cross-statement edges linking stg_orders to orders_summary"
    );

    // The first statement's output should be labeled with the model name
    let first_stmt = result
        .statements
        .first()
        .expect("Should have first statement");
    let output_node = first_stmt
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Output);

    assert!(
        output_node.is_some(),
        "First statement should have an output node"
    );

    let output = output_node.unwrap();
    assert_eq!(
        output.label.as_ref(),
        "stg_orders",
        "Output node label should be the model name"
    );
    assert_eq!(
        output.qualified_name.as_ref().map(|s| s.as_ref()),
        Some("stg_orders"),
        "Output node qualified_name should be the model name"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_model_name_extraction_from_path() {
    // Test that model names are correctly extracted from various path formats
    let model = r#"SELECT 1 AS id"#;

    // Test with nested path
    let result = analyze_dbt_files(vec![FileSource {
        name: "models/staging/stg_customers.sql".to_string(),
        content: model.to_string(),
    }]);

    let output_node = result
        .statements
        .first()
        .and_then(|s| s.nodes.iter().find(|n| n.node_type == NodeType::Output));

    assert!(output_node.is_some(), "Should have output node");
    assert_eq!(
        output_node.unwrap().label.as_ref(),
        "stg_customers",
        "Should extract 'stg_customers' from 'models/staging/stg_customers.sql'"
    );
}
