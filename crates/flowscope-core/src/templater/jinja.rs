//! MiniJinja wrapper for template rendering.

use super::error::TemplateError;
use minijinja::{Environment, Value};
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::{Duration, Instant};

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
    // Track which unknown functions we've already stubbed to avoid infinite loops
    let mut stubbed_functions: HashSet<String> = HashSet::new();
    let start_time = Instant::now();

    // Retry loop: keep trying until we succeed or hit a non-function error
    const MAX_RETRIES: usize = 50; // Prevent infinite loops
    for _ in 0..MAX_RETRIES {
        // Check timeout to prevent DoS via templates with many unknown macros
        if start_time.elapsed() > RENDER_TIMEOUT {
            let stubbed_list: Vec<_> = stubbed_functions.iter().cloned().collect();
            return Err(TemplateError::RenderError(format!(
                "Template rendering timed out after {:?}. Stubbed {} unknown functions: {}",
                RENDER_TIMEOUT,
                stubbed_list.len(),
                if stubbed_list.is_empty() {
                    "(none)".to_string()
                } else {
                    stubbed_list.join(", ")
                }
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

        // Add the template
        env.add_template("sql", template)?;

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

    let stubbed_list: Vec<_> = stubbed_functions.iter().cloned().collect();
    Err(TemplateError::RenderError(format!(
        "Too many unknown functions in template (limit: {MAX_RETRIES}). Stubbed: {}",
        if stubbed_list.is_empty() {
            "(none)".to_string()
        } else {
            stubbed_list.join(", ")
        }
    )))
}

/// Registers a passthrough function that returns its first argument or empty string.
///
/// This is used for unknown dbt macros where we don't know the semantics,
/// but want to produce parseable SQL for lineage analysis.
fn register_passthrough_function(env: &mut Environment<'_>, name: &str) {
    let name_owned = name.to_string();
    let name_for_closure = name_owned.clone();
    env.add_function(name_owned, move |args: &[Value]| -> Value {
        // If the macro has arguments, return the first one (common pattern)
        // Otherwise return empty string
        if let Some(first) = args.first() {
            if let Some(s) = first.as_str() {
                return Value::from(s);
            }
        }
        // For macros like {{ generate_schema_name() }}, return the macro name
        // as a placeholder identifier
        Value::from(format!("__{name_for_closure}__"))
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
}
