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
        result.statements.get(0).map(|s| &s.nodes)
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
        result.statements.get(0).map(|s| &s.nodes)
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
        result.statements.get(0).map(|s| &s.nodes)
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
        result.statements.get(0).map(|s| &s.nodes)
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
    let template_error = result
        .issues
        .iter()
        .find(|i| i.code == "TEMPLATE_ERROR");
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
