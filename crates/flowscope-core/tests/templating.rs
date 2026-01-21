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
    // source('schema', 'table') returns "schema_table"
    assert!(
        has_table(&result, "raw_events"),
        "Should detect 'raw_events' table from source(): {:?}",
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
