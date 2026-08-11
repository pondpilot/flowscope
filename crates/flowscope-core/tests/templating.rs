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

/// Helper to check if a persistent relation with the given name exists in the result.
/// Checks both the label and qualified_name fields.
fn has_table(result: &flowscope_core::AnalyzeResult, table_name: &str) -> bool {
    result.nodes.iter().any(|node| {
        if !matches!(node.node_type, NodeType::Table | NodeType::View) {
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
        &result.nodes
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
        &result.nodes
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
        &result.nodes
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
        &result.nodes
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
use flowscope_core::{
    AnalysisOptions, ColumnSchema, EdgeType, FileSource, SchemaMetadata, SchemaTable,
};

/// Helper to run multi-file dbt analysis.
#[cfg(feature = "templating")]
fn analyze_dbt_files(files: Vec<FileSource>) -> flowscope_core::AnalyzeResult {
    analyze_dbt_files_with_dialect(files, Dialect::Postgres)
}

#[cfg(feature = "templating")]
fn analyze_dbt_files_with_dialect(
    files: Vec<FileSource>,
    dialect: Dialect,
) -> flowscope_core::AnalyzeResult {
    let request = AnalyzeRequest {
        sql: String::new(),
        files: Some(files),
        dialect,
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
        .edges
        .iter()
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
    let cross_edges = &result.edges;
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
    let cross_edges = &result.edges;
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
    let output_columns: Vec<_> = result
        .nodes_in_statement(mart_stmt.statement_index)
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
    let derivations: Vec<_> = result
        .edges_in_statement(mart_stmt.statement_index)
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
    let columns: Vec<_> = result
        .nodes_in_statement(stmt.statement_index)
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
    let columns: Vec<_> = result
        .nodes_in_statement(ranked_stmt.statement_index)
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
    let derivations: Vec<_> = result
        .edges_in_statement(ranked_stmt.statement_index)
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
    let columns: Vec<_> = result
        .nodes_in_statement(revenue_stmt.statement_index)
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
    let derivations: Vec<_> = result
        .edges_in_statement(revenue_stmt.statement_index)
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
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::CrossStatement)
        .collect();

    assert!(
        !cross_edges.is_empty(),
        "Should have cross-statement edges linking stg_orders to orders_summary"
    );

    // The first statement's sink should be materialized as the model's
    // canonical relation node (issue #32) so it unifies with consumer
    // references while preserving the configured relation type.
    let first_stmt = result
        .statements
        .first()
        .expect("Should have first statement");
    let sink_node = result
        .nodes_in_statement(first_stmt.statement_index)
        .find(|n| n.node_type.is_table_like() && n.label.as_ref() == "stg_orders");

    assert!(
        sink_node.is_some(),
        "First statement should have a table sink for the dbt model"
    );

    let sink = sink_node.unwrap();
    assert_eq!(
        sink.node_type,
        NodeType::View,
        "dbt model sink should preserve the dbt materialized='view' relation type"
    );
    assert_eq!(
        sink.qualified_name.as_ref().map(|s| s.as_ref()),
        Some("stg_orders"),
        "Model sink qualified_name should be the model name"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_same_named_ctes_in_different_models_stay_distinct() {
    let customers = r#"
{{ config(materialized='view') }}
WITH scoped_data AS (
    SELECT id
    FROM raw_customers
)
SELECT id
FROM scoped_data
"#;

    let orders = r#"
{{ config(materialized='view') }}
WITH scoped_data AS (
    SELECT id
    FROM raw_orders
)
SELECT id
FROM scoped_data
"#;

    let result = analyze_dbt_files(vec![
        FileSource {
            name: "models/marts/customers.sql".to_string(),
            content: customers.to_string(),
        },
        FileSource {
            name: "models/marts/orders.sql".to_string(),
            content: orders.to_string(),
        },
    ]);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );

    let cte_nodes: Vec<_> = result
        .nodes
        .iter()
        .filter(|node| {
            node.node_type == NodeType::Cte
                && node.canonical_name.as_ref().unwrap().name == "scoped_data"
        })
        .collect();

    assert_eq!(
        cte_nodes.len(),
        2,
        "same-named CTEs from different dbt models should stay distinct in global lineage"
    );
    assert!(
        cte_nodes.iter().all(|node| node.statement_ids.len() == 1),
        "dbt model-local CTEs should remain statement-local"
    );

    // Each dbt model's sink is materialized as a Table node labeled with the
    // model name (see issue #32).
    let sink_labels: Vec<_> = result
        .statements
        .iter()
        .filter_map(|statement| {
            result
                .nodes_in_statement(statement.statement_index)
                .find(|node| {
                    node.node_type.is_table_like()
                        && node
                            .qualified_name
                            .as_ref()
                            .map(|q| q.as_ref() == node.label.as_ref())
                            .unwrap_or(false)
                })
                .map(|node| node.label.to_string())
        })
        .collect();

    assert_eq!(
        sink_labels,
        vec!["customers".to_string(), "orders".to_string()]
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_cte_self_join_within_model_produces_distinct_nodes() {
    let model = r#"
{{ config(materialized='view') }}
WITH team AS (
    SELECT id, name, manager_id
    FROM raw_employees
)
SELECT t1.name AS employee, t2.name AS manager
FROM team t1
JOIN team t2 ON t1.manager_id = t2.id
"#;

    let result = analyze_dbt_files(vec![FileSource {
        name: "models/marts/hierarchy.sql".to_string(),
        content: model.to_string(),
    }]);

    assert!(
        !result.summary.has_errors,
        "Analysis should succeed: {:?}",
        result.issues
    );

    let stmt = result
        .statements
        .first()
        .expect("should have at least one statement");

    let cte_nodes: Vec<_> = result
        .nodes_in_statement(stmt.statement_index)
        .filter(|n| n.node_type == NodeType::Cte && n.label.as_ref() == "team")
        .collect();

    assert_eq!(
        cte_nodes.len(),
        2,
        "CTE self-join should produce 2 distinct CTE nodes within the model"
    );

    let unique_ids: std::collections::HashSet<_> = cte_nodes.iter().map(|n| &n.id).collect();
    assert_eq!(
        unique_ids.len(),
        2,
        "CTE self-join aliases should have distinct node IDs"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_recursive_cte_stays_statement_scoped() {
    let model = r#"
{{ config(materialized='view') }}
WITH RECURSIVE hierarchy AS (
    SELECT id, name, manager_id, 1 AS depth
    FROM raw_employees
    WHERE manager_id IS NULL
    UNION ALL
    SELECT e.id, e.name, e.manager_id, h.depth + 1
    FROM raw_employees e
    JOIN hierarchy h ON e.manager_id = h.id
)
SELECT id, name, depth
FROM hierarchy
"#;

    let result = analyze_dbt_files(vec![FileSource {
        name: "models/marts/org_chart.sql".to_string(),
        content: model.to_string(),
    }]);

    assert!(
        !result.summary.has_errors,
        "Recursive CTE analysis should succeed: {:?}",
        result.issues
    );

    let cte_nodes: Vec<_> = result
        .nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Cte && n.canonical_name.as_ref().unwrap().name == "hierarchy"
        })
        .collect();

    assert!(
        !cte_nodes.is_empty(),
        "recursive CTE should appear in global lineage"
    );

    assert!(
        cte_nodes.iter().all(|n| n.statement_ids.len() == 1),
        "recursive CTE should remain statement-scoped"
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

    // The model sink is now a Table node keyed by the extracted model name
    // (issue #32).
    let sink_node = result.statements.first().and_then(|s| {
        result
            .nodes_in_statement(s.statement_index)
            .find(|n| n.node_type.is_table_like() && n.label.as_ref() == "stg_customers")
    });

    assert!(sink_node.is_some(), "Should have model sink node");
    assert_eq!(
        sink_node.unwrap().label.as_ref(),
        "stg_customers",
        "Should extract 'stg_customers' from 'models/staging/stg_customers.sql'"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_view_models_materialize_as_view_nodes() {
    let model = r#"
{{ config(materialized='view') }}
SELECT 1 AS id
"#;

    let result = analyze_dbt_files(vec![FileSource {
        name: "models/marts/orders_view.sql".to_string(),
        content: model.to_string(),
    }]);

    let sink_node = result.statements.first().and_then(|statement| {
        result
            .nodes_in_statement(statement.statement_index)
            .find(|node| {
                node.label.as_ref() == "orders_view"
                    && matches!(node.node_type, NodeType::Table | NodeType::View)
            })
    });

    let sink_node = sink_node.expect("dbt view model should produce a sink node");
    assert_eq!(
        sink_node.node_type,
        NodeType::View,
        "dbt models configured as materialized='view' should surface as View nodes"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_later_config_call_controls_materialization() {
    let model = r#"
{{ config(tags=['daily']) }}
{{ config(materialized='view') }}
SELECT 1 AS id
"#;

    let result = analyze_dbt_files(vec![FileSource {
        name: "models/orders_view.sql".to_string(),
        content: model.to_string(),
    }]);

    let sink_node = result.statements.first().and_then(|statement| {
        result
            .nodes_in_statement(statement.statement_index)
            .find(|node| {
                node.label.as_ref() == "orders_view"
                    && matches!(node.node_type, NodeType::Table | NodeType::View)
            })
    });

    let sink_node = sink_node.expect("dbt model should produce a sink node");
    assert_eq!(
        sink_node.node_type,
        NodeType::View,
        "a later config(materialized=...) call should control the dbt sink relation type"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_ephemeral_models_surface_as_cte_nodes_instead_of_persistent_relations() {
    let consumer = "SELECT * FROM {{ ref('ephemeral_orders') }}";
    let producer = r#"
{{ config(materialized='ephemeral') }}
SELECT 1 AS id
"#;

    let result = analyze_dbt_files(vec![
        FileSource {
            name: "models/consumer.sql".to_string(),
            content: consumer.to_string(),
        },
        FileSource {
            name: "models/ephemeral_orders.sql".to_string(),
            content: producer.to_string(),
        },
    ]);

    let ephemeral_nodes: Vec<_> = result
        .nodes
        .iter()
        .filter(|node| {
            node.canonical_name
                .as_ref()
                .map(|canonical| canonical.name.as_str() == "ephemeral_orders")
                .unwrap_or(false)
        })
        .collect();

    assert_eq!(
        ephemeral_nodes.len(),
        1,
        "ephemeral dbt models should collapse producer and consumer refs onto one node"
    );

    let ephemeral_node = ephemeral_nodes[0];
    assert_eq!(
        ephemeral_node.node_type,
        NodeType::Cte,
        "ephemeral dbt models should be represented as CTE-like nodes, not persisted relations"
    );
    assert!(
        ephemeral_node.statement_ids.contains(&0) && ephemeral_node.statement_ids.contains(&1),
        "the shared ephemeral node should be referenced by both the consumer and producer"
    );
    assert!(
        result.nodes.iter().all(|node| {
            !(matches!(node.node_type, NodeType::Table | NodeType::View)
                && node
                    .canonical_name
                    .as_ref()
                    .map(|canonical| canonical.name.as_str() == "ephemeral_orders")
                    .unwrap_or(false))
        }),
        "ephemeral dbt models must not surface as persisted table/view nodes"
    );
}

#[test]
#[cfg(feature = "templating")]
fn hide_ctes_keeps_dbt_ephemeral_model_sinks() {
    let request = AnalyzeRequest {
        sql: String::new(),
        files: Some(vec![
            FileSource {
                name: "models/consumer.sql".to_string(),
                content: "WITH local_cte AS (SELECT 1 AS id) SELECT * FROM local_cte CROSS JOIN {{ ref('ephemeral_orders') }}".to_string(),
            },
            FileSource {
                name: "models/ephemeral_orders.sql".to_string(),
                content: "{{ config(materialized='ephemeral') }} SELECT 1 AS id".to_string(),
            },
        ]),
        dialect: Dialect::Postgres,
        source_name: None,
        options: Some(AnalysisOptions {
            hide_ctes: Some(true),
            ..AnalysisOptions::default()
        }),
        schema: None,
        template_config: Some(TemplateConfig {
            mode: TemplateMode::Dbt,
            context: HashMap::new(),
        }),
    };

    let result = analyze(&request);

    assert!(
        result.nodes.iter().any(|node| {
            node.node_type == NodeType::Cte
                && node
                    .canonical_name
                    .as_ref()
                    .map(|canonical| canonical.name.as_str() == "ephemeral_orders")
                    .unwrap_or(false)
        }),
        "hide_ctes should preserve dbt ephemeral model sinks in the global graph"
    );
    assert!(
        result
            .nodes
            .iter()
            .all(|node| node.label.as_ref() != "local_cte"),
        "hide_ctes should still remove statement-local CTEs"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_view_model_unifies_with_consumer_even_when_consumer_is_analyzed_first() {
    let consumer = "SELECT * FROM {{ ref('orders_view') }}";
    let producer = r#"
{{ config(materialized='view') }}
SELECT 1 AS id
"#;

    let result = analyze_dbt_files(vec![
        FileSource {
            name: "models/fct_orders.sql".to_string(),
            content: consumer.to_string(),
        },
        FileSource {
            name: "models/orders_view.sql".to_string(),
            content: producer.to_string(),
        },
    ]);

    let orders_view_nodes: Vec<_> = result
        .nodes
        .iter()
        .filter(|node| {
            node.node_type.is_table_like()
                && node
                    .canonical_name
                    .as_ref()
                    .map(|canonical| canonical.name.as_str() == "orders_view")
                    .unwrap_or(false)
        })
        .collect();

    assert_eq!(
        orders_view_nodes.len(),
        1,
        "consumer and producer should collapse into one canonical orders_view node"
    );
    let orders_view = orders_view_nodes[0];
    assert_eq!(
        orders_view.node_type,
        NodeType::View,
        "future dbt view producers should predeclare a view identity for earlier consumers"
    );
    assert!(
        orders_view.statement_ids.contains(&0) && orders_view.statement_ids.contains(&1),
        "unified orders_view node should be referenced by both the consumer and producer"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_forward_ref_to_model_does_not_remain_unresolved() {
    let request = AnalyzeRequest {
        sql: String::new(),
        files: Some(vec![
            FileSource {
                name: "models/consumer.sql".to_string(),
                content: "SELECT * FROM {{ ref('producer') }}".to_string(),
            },
            FileSource {
                name: "models/producer.sql".to_string(),
                content: "SELECT id FROM {{ source('raw', 'events') }}".to_string(),
            },
        ]),
        dialect: Dialect::Postgres,
        source_name: None,
        options: None,
        schema: Some(SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("raw".to_string()),
                name: "events".to_string(),
                columns: vec![ColumnSchema {
                    name: "id".to_string(),
                    data_type: Some("int".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                }],
            }],
            ..SchemaMetadata::default()
        }),
        template_config: Some(TemplateConfig {
            mode: TemplateMode::Dbt,
            context: HashMap::new(),
        }),
    };

    let result = analyze(&request);

    assert!(
        !result.issues.iter().any(|issue| {
            issue.code == flowscope_core::issue_codes::UNRESOLVED_REFERENCE
                && issue.message.contains("producer")
        }),
        "dbt model refs should be predeclared so forward references do not stay unresolved: {:?}",
        result.issues
    );

    let producer = result
        .nodes
        .iter()
        .find(|node| {
            node.node_type.is_table_like()
                && node
                    .canonical_name
                    .as_ref()
                    .map(|canonical| canonical.name.as_str() == "producer")
                    .unwrap_or(false)
        })
        .expect("producer node should exist");

    let has_placeholder = producer
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("placeholder"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    assert_eq!(producer.resolution_source, None);
    assert!(
        !has_placeholder,
        "producer node should not retain unresolved placeholder metadata once its model file is known"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_merged_model_node_keeps_producer_occurrence_metadata() {
    let producer = "SELECT 1 AS id";
    let consumer = "SELECT * FROM {{ ref('stg_constants') }}";

    let result = analyze_dbt_files(vec![
        FileSource {
            name: "models/stg_constants.sql".to_string(),
            content: producer.to_string(),
        },
        FileSource {
            name: "models/fct_constants.sql".to_string(),
            content: consumer.to_string(),
        },
    ]);

    let node = result
        .nodes
        .iter()
        .find(|node| {
            node.node_type.is_table_like()
                && node
                    .canonical_name
                    .as_ref()
                    .map(|canonical| canonical.name.as_str() == "stg_constants")
                    .unwrap_or(false)
        })
        .expect("merged stg_constants node should exist");

    let occurrence_source_names = node
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("occurrenceSourceNames"))
        .and_then(serde_json::Value::as_array)
        .expect("merged node should record occurrence source names");

    assert!(
        node.all_name_spans().len() >= 2,
        "merged node should keep both the producer definition span and consumer ref span"
    );
    assert_eq!(
        occurrence_source_names.first().and_then(serde_json::Value::as_str),
        Some("models/stg_constants.sql"),
        "producer file should be the first occurrence so graph reveal navigates to the model definition"
    );
    assert!(
        occurrence_source_names
            .iter()
            .any(|value| value.as_str() == Some("models/fct_constants.sql")),
        "consumer ref occurrences should still be retained on the merged node"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_consumer_first_node_marks_producer_definition_occurrence() {
    let consumer = "SELECT * FROM {{ ref('stg_constants') }}";
    let producer = "SELECT 1 AS id";

    let result = analyze_dbt_files(vec![
        FileSource {
            name: "models/fct_constants.sql".to_string(),
            content: consumer.to_string(),
        },
        FileSource {
            name: "models/stg_constants.sql".to_string(),
            content: producer.to_string(),
        },
    ]);

    let node = result
        .nodes
        .iter()
        .find(|node| {
            node.node_type.is_table_like()
                && node
                    .canonical_name
                    .as_ref()
                    .map(|canonical| canonical.name.as_str() == "stg_constants")
                    .unwrap_or(false)
        })
        .expect("merged stg_constants node should exist");

    let occurrence_source_names = node
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("occurrenceSourceNames"))
        .and_then(serde_json::Value::as_array)
        .expect("merged node should record occurrence source names");
    let definition_source_names = node
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("definitionOccurrenceSourceNames"))
        .and_then(serde_json::Value::as_array)
        .expect("merged node should record producer-definition occurrences");

    assert_eq!(
        occurrence_source_names.first().and_then(serde_json::Value::as_str),
        Some("models/fct_constants.sql"),
        "raw occurrence order should still reflect analysis/file order when the consumer appears first"
    );
    assert_eq!(
        definition_source_names.first().and_then(serde_json::Value::as_str),
        Some("models/stg_constants.sql"),
        "producer-definition metadata should identify the model file even when a consumer was analyzed first"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_chained_models_unify_producer_and_consumer_nodes() {
    // Regression test for https://github.com/pondpilot/flowscope/issues/32.
    //
    // Each dbt .sql file is a model that materializes a table with the same name as
    // the file. A downstream file references that table via `{{ ref(...) }}`. In the
    // lineage graph the producing model and every `ref()` consumer must collapse
    // into a single node for that table, so multi-hop chains (A -> B -> C) render
    // as a single connected lineage rather than three disconnected fragments.
    let stg_supplies = "select id, name, price from {{ source('raw', 'supplies') }}";
    let int_supplies = "select id, upper(name) as name, price from {{ ref('stg_supplies') }}";
    let fct_supplies =
        "select id, name, price * 1.1 as price_with_tax from {{ ref('int_supplies') }}";

    let result = analyze_dbt_files(vec![
        FileSource {
            name: "models/stg_supplies.sql".to_string(),
            content: stg_supplies.to_string(),
        },
        FileSource {
            name: "models/int_supplies.sql".to_string(),
            content: int_supplies.to_string(),
        },
        FileSource {
            name: "models/fct_supplies.sql".to_string(),
            content: fct_supplies.to_string(),
        },
    ]);

    assert!(
        !result.summary.has_errors,
        "dbt chain analysis should succeed: {:?}",
        result.issues
    );

    // Each model name must appear as exactly one table-like node in the merged
    // graph. Before the fix, the producer emitted an `Output` node and the
    // consumer emitted a separate `Table` node with the same canonical name,
    // so each model name collided into two distinct nodes.
    for model in ["stg_supplies", "int_supplies", "fct_supplies"] {
        let table_like_nodes: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| {
                n.node_type.is_table_like()
                    && n.canonical_name
                        .as_ref()
                        .map(|c| c.name.as_str() == model)
                        .unwrap_or(false)
            })
            .collect();
        assert_eq!(
            table_like_nodes.len(),
            1,
            "model '{model}' should have exactly one unified table node, found {}: {:?}",
            table_like_nodes.len(),
            table_like_nodes
        );

        // The unified node should also not coexist with a dangling Output node
        // labeled with the same model name.
        let stray_output = result
            .nodes
            .iter()
            .find(|n| n.node_type == NodeType::Output && n.label.as_ref() == model);
        assert!(
            stray_output.is_none(),
            "model '{model}' should not have a leftover Output-typed node: {:?}",
            stray_output
        );
    }

    // The producer statement and the consumer statement must both reference the
    // unified node — that's how downstream tools (mermaid export, column
    // lineage, etc.) know A feeds B.
    let stg_node = result
        .nodes
        .iter()
        .find(|n| {
            n.node_type.is_table_like()
                && n.canonical_name
                    .as_ref()
                    .map(|c| c.name.as_str() == "stg_supplies")
                    .unwrap_or(false)
        })
        .expect("stg_supplies unified node should exist");
    assert!(
        stg_node.statement_ids.contains(&0) && stg_node.statement_ids.contains(&1),
        "unified stg_supplies node should be referenced by both producer (stmt 0) and \
         consumer (stmt 1): statement_ids = {:?}",
        stg_node.statement_ids
    );

    // A multi-hop cross-statement edge chain must exist: a cross-statement edge
    // linking the producer of each model to its consumer.
    let cross_edges: Vec<_> = result
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::CrossStatement)
        .collect();
    // stg produced by 0 -> consumed by 1; int produced by 1 -> consumed by 2.
    let has_stg_edge = cross_edges
        .iter()
        .any(|e| e.statement_ids == vec![0usize, 1usize]);
    let has_int_edge = cross_edges
        .iter()
        .any(|e| e.statement_ids == vec![1usize, 2usize]);
    assert!(
        has_stg_edge && has_int_edge,
        "should have cross-statement edges for stg (0->1) and int (1->2), got {:?}",
        cross_edges
            .iter()
            .map(|e| &e.statement_ids)
            .collect::<Vec<_>>()
    );

    // Table-level DataFlow edges should connect the models, matching the
    // arrows produced by CTAS (`CREATE TABLE x AS SELECT ... FROM y`). Without
    // these, the mermaid table view would show disconnected boxes even though
    // the nodes unify correctly.
    let int_node_id = result
        .nodes
        .iter()
        .find(|n| {
            n.node_type.is_table_like()
                && n.canonical_name
                    .as_ref()
                    .map(|c| c.name.as_str() == "int_supplies")
                    .unwrap_or(false)
        })
        .map(|n| n.id.clone())
        .expect("int_supplies unified node should exist");
    let has_stg_to_int = result
        .edges
        .iter()
        .any(|e| e.edge_type == EdgeType::DataFlow && e.from == stg_node.id && e.to == int_node_id);
    assert!(
        has_stg_to_int,
        "should have a DataFlow edge stg_supplies -> int_supplies at the table level"
    );
}

#[test]
#[cfg(feature = "templating")]
fn snowflake_dbt_ref_columns_merge_with_model_definition_columns() {
    // Regression test for https://github.com/pondpilot/flowscope/issues/45.
    let stg_customers = r#"
{{ config(materialized='view') }}
WITH final AS (
    SELECT
        customer_id,
        first_name,
        last_name,
        first_name || ' ' || last_name AS full_name,
        created_at
    FROM {{ source('raw', 'customers') }}
)
SELECT customer_id, first_name, last_name, full_name, created_at
FROM final
"#;
    let customers = r#"
SELECT customer_id, first_name, last_name, full_name, created_at
FROM {{ ref('stg_customers') }}
"#;

    let result = analyze_dbt_files_with_dialect(
        vec![
            FileSource {
                name: "models/stg_customers.sql".to_string(),
                content: stg_customers.to_string(),
            },
            FileSource {
                name: "models/customers.sql".to_string(),
                content: customers.to_string(),
            },
        ],
        Dialect::Snowflake,
    );

    assert!(
        !result.summary.has_errors,
        "Snowflake dbt analysis should succeed: {:?}",
        result.issues
    );

    let stg_customers_node = result
        .nodes
        .iter()
        .find(|node| {
            node.node_type == NodeType::View
                && node
                    .canonical_name
                    .as_ref()
                    .is_some_and(|name| name.name == "STG_CUSTOMERS")
        })
        .expect("stg_customers model view should exist");
    let owned_column_ids: std::collections::HashSet<_> = result
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::Ownership && edge.from == stg_customers_node.id)
        .map(|edge| edge.to.as_ref())
        .collect();
    let mut owned_column_labels: Vec<_> = result
        .nodes
        .iter()
        .filter(|node| owned_column_ids.contains(node.id.as_ref()))
        .map(|node| node.label.as_ref())
        .collect();
    owned_column_labels.sort_unstable();

    assert_eq!(
        owned_column_labels,
        [
            "CREATED_AT",
            "CUSTOMER_ID",
            "FIRST_NAME",
            "FULL_NAME",
            "LAST_NAME",
        ],
        "the model definition and downstream ref must share Snowflake-normalized column identities"
    );
}

#[test]
#[cfg(feature = "templating")]
fn dbt_ref_column_merge_preserves_case_sensitive_dialect_identity() {
    let result = analyze_dbt_files_with_dialect(
        vec![
            FileSource {
                name: "models/stg_customers.sql".to_string(),
                content: "{{ config(materialized='view') }} SELECT Customer_ID FROM raw.customers"
                    .to_string(),
            },
            FileSource {
                name: "models/customers.sql".to_string(),
                content: "SELECT customer_id FROM {{ ref('stg_customers') }}".to_string(),
            },
        ],
        Dialect::Mysql,
    );

    assert!(
        !result.summary.has_errors,
        "case-sensitive dbt analysis should succeed: {:?}",
        result.issues
    );

    let stg_customers_node = result
        .nodes
        .iter()
        .find(|node| node.node_type == NodeType::View && node.label.as_ref() == "stg_customers")
        .expect("stg_customers model view should exist");
    let owned_column_ids: std::collections::HashSet<_> = result
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::Ownership && edge.from == stg_customers_node.id)
        .map(|edge| edge.to.as_ref())
        .collect();
    let mut owned_column_labels: Vec<_> = result
        .nodes
        .iter()
        .filter(|node| owned_column_ids.contains(node.id.as_ref()))
        .map(|node| node.label.as_ref())
        .collect();
    owned_column_labels.sort_unstable();

    assert_eq!(owned_column_labels, ["Customer_ID", "customer_id"]);
}

#[test]
#[cfg(feature = "templating")]
fn snowflake_dbt_consumer_first_column_keeps_producer_expression() {
    let result = analyze_dbt_files_with_dialect(
        vec![
            FileSource {
                name: "models/customers.sql".to_string(),
                content: "SELECT full_name FROM {{ ref('stg_customers') }}".to_string(),
            },
            FileSource {
                name: "models/stg_customers.sql".to_string(),
                content: "{{ config(materialized='view') }} SELECT first_name || last_name AS full_name FROM raw.customers"
                    .to_string(),
            },
        ],
        Dialect::Snowflake,
    );

    let full_name = result
        .nodes
        .iter()
        .find(|node| node.qualified_name.as_deref() == Some("STG_CUSTOMERS.FULL_NAME"))
        .expect("consumer and producer full_name columns should merge");

    assert_eq!(
        full_name.expression.as_deref(),
        Some("first_name || last_name")
    );
}
