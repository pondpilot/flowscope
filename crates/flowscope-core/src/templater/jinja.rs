//! MiniJinja wrapper for template rendering.

use super::dbt::passthrough_arg_to_string;
use super::error::TemplateError;
use minijinja::{Environment, Value};
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use regex::Regex;
use std::sync::LazyLock;

/// Renders a Jinja2 template with the given context.
///
/// This is the core rendering function for plain Jinja templates
/// without dbt-specific macros.
/// Recursion limit for template rendering to prevent DoS via deeply nested templates.
/// Set lower than MiniJinja's default (500) for extra safety in WASM environments.
const RECURSION_LIMIT: usize = 100;

/// Maximum time allowed for the dbt render retry loop to prevent DoS via templates
/// with many distinct unknown macros.
const RENDER_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum template size for regex preprocessing (10 MB).
/// Templates larger than this skip preprocessing to avoid regex DoS.
const MAX_PREPROCESS_SIZE: usize = 10_000_000;

/// Regex to match dbt {% test ... %} ... {% endtest %} blocks.
/// These define test macros which should be stripped for lineage analysis.
///
/// Pattern uses `[^%]*` for tag contents to prevent pathological backtracking
/// on crafted input (can't match across the `%}` delimiter).
static TEST_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\{%-?\s*test\b[^%]*-?%\}.*?\{%-?\s*endtest\s*-?%\}").unwrap()
});

/// Regex to match dbt {% snapshot ... %} ... {% endsnapshot %} blocks.
/// We keep the inner content but strip the snapshot tags.
///
/// Pattern uses `[^%]*` for tag contents to prevent pathological backtracking.
static SNAPSHOT_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\{%-?\s*snapshot\b[^%]*-?%\}(.*?)\{%-?\s*endsnapshot\s*-?%\}").unwrap()
});

/// Preprocesses dbt-specific template tags that MiniJinja doesn't recognize.
///
/// This handles:
/// - `{% test ... %} ... {% endtest %}` - Removed entirely (test macro definitions)
/// - `{% snapshot ... %} ... {% endsnapshot %}` - Tags stripped, inner content preserved
///
/// # Arguments
///
/// * `template` - The template string that may contain dbt-specific tags
///
/// # Returns
///
/// The preprocessed template with dbt-specific tags handled.
/// Templates larger than `MAX_PREPROCESS_SIZE` are returned unchanged to prevent
/// regex DoS on very large inputs.
fn preprocess_dbt_tags(template: &str) -> String {
    // Skip preprocessing for very large templates to avoid regex DoS
    if template.len() > MAX_PREPROCESS_SIZE {
        return template.to_string();
    }

    // Remove test blocks entirely
    let result = TEST_BLOCK_RE.replace_all(template, "");

    // For snapshot blocks, keep the inner content
    let result = SNAPSHOT_BLOCK_RE.replace_all(&result, "$1");

    result.into_owned()
}

pub(crate) fn render_jinja(
    template: &str,
    context: &HashMap<String, serde_json::Value>,
) -> Result<String, TemplateError> {
    let mut env = Environment::new();

    // Configure environment for SQL templating
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);

    // Set recursion limit to prevent DoS via deeply nested templates
    env.set_recursion_limit(RECURSION_LIMIT);

    // Add the template
    env.add_template("sql", template)?;

    // Convert context to MiniJinja values
    let ctx = json_context_to_minijinja(context);

    // Render the template
    let tmpl = env.get_template("sql")?;
    let rendered = tmpl.render(ctx)?;

    Ok(rendered)
}

/// Renders a Jinja2 template with dbt builtins available.
///
/// This adds common dbt macros like `ref()`, `source()`, `config()`, and `var()`
/// as stub functions that return placeholder values suitable for lineage analysis.
///
/// Unknown macros (custom project macros, dbt_utils, etc.) are handled gracefully
/// by stubbing them on-the-fly. This allows lineage analysis even when the full
/// dbt project context isn't available.
pub(crate) fn render_dbt(
    template: &str,
    context: &HashMap<String, serde_json::Value>,
) -> Result<String, TemplateError> {
    // Preprocess dbt-specific tags that MiniJinja doesn't recognize
    let preprocessed = preprocess_dbt_tags(template);

    // Track which unknown functions we've already stubbed to avoid infinite loops
    let mut stubbed_functions: HashSet<String> = HashSet::new();
    let start_time = Instant::now();

    // Retry loop: keep trying until we succeed or hit a non-function error
    const MAX_RETRIES: usize = 50; // Prevent infinite loops
    for _ in 0..MAX_RETRIES {
        // Check timeout to prevent DoS via templates with many unknown macros
        if start_time.elapsed() > RENDER_TIMEOUT {
            return Err(TemplateError::RenderError(format!(
                "Template rendering timed out after {:?}. Stubbed {} unknown functions: {}",
                RENDER_TIMEOUT,
                stubbed_functions.len(),
                format_stubbed_list(&stubbed_functions)
            )));
        }
        let mut env = Environment::new();

        // Configure environment - use lenient mode for dbt since templates
        // may reference variables that aren't always defined
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);

        // Set recursion limit to prevent DoS via deeply nested templates
        env.set_recursion_limit(RECURSION_LIMIT);

        // Register dbt builtin macros
        super::dbt::register_dbt_builtins(&mut env, context);

        // Register stubs for any unknown functions we've discovered
        for func_name in &stubbed_functions {
            register_passthrough_function(&mut env, func_name);
        }

        // Add the preprocessed template
        env.add_template("sql", &preprocessed)?;

        // Convert context to MiniJinja values
        let ctx = json_context_to_minijinja(context);

        // Try to render the template
        let tmpl = env.get_template("sql")?;
        match tmpl.render(ctx) {
            Ok(rendered) => {
                #[cfg(feature = "tracing")]
                if !stubbed_functions.is_empty() {
                    let stubbed_list: Vec<_> = stubbed_functions.iter().cloned().collect();
                    tracing::debug!(
                        stubbed_functions = ?stubbed_list,
                        "Template rendered with stubbed unknown macros"
                    );
                }
                return Ok(rendered);
            }
            Err(e) => {
                // Check if this is an "unknown function" error
                if let Some(func_name) = extract_unknown_function(&e) {
                    if stubbed_functions.contains(&func_name) {
                        // Already stubbed this one, something else is wrong
                        return Err(TemplateError::RenderError(e.to_string()));
                    }

                    #[cfg(feature = "tracing")]
                    tracing::debug!(
                        function = %func_name,
                        stubbed_count = stubbed_functions.len() + 1,
                        "Stubbing unknown dbt macro"
                    );

                    stubbed_functions.insert(func_name);
                    // Retry with the new stub
                    continue;
                }
                // Not an unknown function error, propagate it
                return Err(TemplateError::RenderError(e.to_string()));
            }
        }
    }

    Err(TemplateError::RenderError(format!(
        "Too many unknown functions in template (limit: {MAX_RETRIES}). Stubbed: {}",
        format_stubbed_list(&stubbed_functions)
    )))
}

/// Registers a passthrough function that returns its first argument or empty string.
///
/// This is used for unknown dbt macros where we don't know the semantics,
/// but want to produce parseable SQL for lineage analysis.
fn register_passthrough_function(env: &mut Environment<'_>, name: &str) {
    let name_owned = name.to_string();
    env.add_function(name_owned.clone(), move |args: &[Value]| -> Value {
        // If the macro has arguments, return the first one (common pattern)
        // Otherwise return empty string
        if let Some(first) = args.first() {
            if let Some(rendered) = passthrough_arg_to_string(first) {
                return Value::from(rendered);
            }
        }
        // For macros like {{ generate_schema_name() }}, return the macro name
        // as a placeholder identifier
        Value::from(format!("__{name_owned}__"))
    });
}

/// Extracts the function name from an "unknown function" error.
///
/// Uses MiniJinja's `ErrorKind::UnknownFunction` for reliable detection
/// rather than parsing error message strings (which could change between versions).
fn extract_unknown_function(err: &minijinja::Error) -> Option<String> {
    use minijinja::ErrorKind;

    // Only handle unknown function errors
    if err.kind() != ErrorKind::UnknownFunction {
        return None;
    }

    // Extract the function name from the error message
    // MiniJinja error format: "unknown function: <name> is unknown"
    const PREFIX: &str = "unknown function: ";
    const SUFFIX: &str = " is unknown";

    let msg = err.to_string();
    let start = msg.find(PREFIX)? + PREFIX.len();
    let remaining = &msg[start..];
    let end = remaining.find(SUFFIX)?;
    let func_name = &remaining[..end];

    // Validate: must be non-empty, reasonable length, valid identifier chars
    if func_name.is_empty() || func_name.len() > 100 {
        return None;
    }
    if !func_name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        return None;
    }

    Some(func_name.to_string())
}

/// Formats a set of stubbed function names for error messages.
fn format_stubbed_list(stubbed: &HashSet<String>) -> String {
    if stubbed.is_empty() {
        "(none)".to_string()
    } else {
        let mut list: Vec<_> = stubbed.iter().cloned().collect();
        list.sort();
        list.join(", ")
    }
}

/// Converts a JSON context map to MiniJinja Value format.
fn json_context_to_minijinja(context: &HashMap<String, serde_json::Value>) -> Value {
    Value::from_serialize(context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_simple_variable() {
        let mut ctx = HashMap::new();
        ctx.insert("table_name".to_string(), serde_json::json!("users"));

        let result = render_jinja("SELECT * FROM {{ table_name }}", &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    #[test]
    fn renders_conditional() {
        let mut ctx = HashMap::new();
        ctx.insert("include_deleted".to_string(), serde_json::json!(true));

        let template =
            r#"SELECT * FROM users{% if include_deleted %} WHERE deleted = false{% endif %}"#;
        let result = render_jinja(template, &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM users WHERE deleted = false");
    }

    #[test]
    fn renders_loop() {
        let mut ctx = HashMap::new();
        ctx.insert(
            "columns".to_string(),
            serde_json::json!(["id", "name", "email"]),
        );

        let template = r#"SELECT {% for col in columns %}{{ col }}{% if not loop.last %}, {% endif %}{% endfor %} FROM users"#;
        let result = render_jinja(template, &ctx).unwrap();
        assert_eq!(result, "SELECT id, name, email FROM users");
    }

    #[test]
    fn errors_on_undefined_variable_in_strict_mode() {
        let ctx = HashMap::new();
        let result = render_jinja("SELECT * FROM {{ undefined_table }}", &ctx);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TemplateError::UndefinedVariable(_)
        ));
    }

    #[test]
    fn errors_on_syntax_error() {
        let ctx = HashMap::new();
        let result = render_jinja("SELECT * FROM {{ unclosed", &ctx);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TemplateError::SyntaxError(_)));
    }

    // =========================================================================
    // Tag preprocessing tests
    // =========================================================================

    #[test]
    fn preprocess_removes_test_blocks() {
        let template = r#"{% test my_test(model) %}
SELECT * FROM {{ model }} WHERE id IS NULL
{% endtest %}

SELECT * FROM users"#;

        let result = preprocess_dbt_tags(template);
        assert!(!result.contains("test my_test"));
        assert!(!result.contains("endtest"));
        assert!(result.contains("SELECT * FROM users"));
    }

    #[test]
    fn preprocess_removes_test_blocks_with_whitespace_control() {
        let template = r#"{%- test not_null(model, column_name) -%}
SELECT * FROM {{ model }} WHERE {{ column_name }} IS NULL
{%- endtest -%}
SELECT 1"#;

        let result = preprocess_dbt_tags(template);
        assert!(!result.contains("test not_null"));
        assert!(result.contains("SELECT 1"));
    }

    #[test]
    fn preprocess_keeps_snapshot_content() {
        let template = r#"{% snapshot orders_snapshot %}
SELECT * FROM orders
{% endsnapshot %}"#;

        let result = preprocess_dbt_tags(template);
        assert!(!result.contains("snapshot orders_snapshot"));
        assert!(!result.contains("endsnapshot"));
        assert!(result.contains("SELECT * FROM orders"));
    }

    #[test]
    fn preprocess_handles_multiple_blocks() {
        let template = r#"{% test test1() %}test sql{% endtest %}
{% snapshot snap1 %}SELECT 1{% endsnapshot %}
{% test test2() %}more test sql{% endtest %}
SELECT * FROM final"#;

        let result = preprocess_dbt_tags(template);
        assert!(!result.contains("test1"));
        assert!(!result.contains("test2"));
        assert!(result.contains("SELECT 1")); // snapshot content preserved
        assert!(result.contains("SELECT * FROM final"));
    }

    #[test]
    fn dbt_render_with_test_block() {
        // Full integration: test block should be stripped before rendering
        let ctx = HashMap::new();
        let template = r#"{% test my_test(model) %}
SELECT * FROM {{ ref('test_model') }}
{% endtest %}

SELECT * FROM {{ ref('users') }}"#;

        let result = render_dbt(template, &ctx).unwrap();
        assert!(!result.contains("test_model"));
        assert!(result.contains("users"));
    }

    #[test]
    fn dbt_render_with_snapshot_block() {
        let ctx = HashMap::new();
        let template = r#"{% snapshot my_snapshot %}
{{ config(unique_key='id') }}
SELECT * FROM {{ ref('source_table') }}
{% endsnapshot %}"#;

        let result = render_dbt(template, &ctx).unwrap();
        assert!(result.contains("SELECT * FROM source_table"));
        assert!(!result.contains("snapshot"));
    }
}
