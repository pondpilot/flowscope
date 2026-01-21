//! MiniJinja wrapper for template rendering.

use super::error::TemplateError;
use minijinja::{Environment, Value};
use std::collections::HashMap;
use std::collections::HashSet;

/// Renders a Jinja2 template with the given context.
///
/// This is the core rendering function for plain Jinja templates
/// without dbt-specific macros.
pub(crate) fn render_jinja(
    template: &str,
    context: &HashMap<String, serde_json::Value>,
) -> Result<String, TemplateError> {
    let mut env = Environment::new();

    // Configure environment for SQL templating
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);

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

    // Retry loop: keep trying until we succeed or hit a non-function error
    const MAX_RETRIES: usize = 50; // Prevent infinite loops
    for _ in 0..MAX_RETRIES {
        let mut env = Environment::new();

        // Configure environment - use lenient mode for dbt since templates
        // may reference variables that aren't always defined
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);

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
            Ok(rendered) => return Ok(rendered),
            Err(e) => {
                // Check if this is an "unknown function" error
                if let Some(func_name) = extract_unknown_function(&e) {
                    if stubbed_functions.contains(&func_name) {
                        // Already stubbed this one, something else is wrong
                        return Err(TemplateError::RenderError(e.to_string()));
                    }
                    stubbed_functions.insert(func_name);
                    // Retry with the new stub
                    continue;
                }
                // Not an unknown function error, propagate it
                return Err(TemplateError::RenderError(e.to_string()));
            }
        }
    }

    Err(TemplateError::RenderError(
        "Too many unknown functions in template".to_string(),
    ))
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

/// Extracts the function name from an "unknown function" error message.
fn extract_unknown_function(err: &minijinja::Error) -> Option<String> {
    let msg = err.to_string();
    // MiniJinja error format: "unknown function: <name> is unknown"
    if msg.contains("unknown function:") {
        // Extract the function name between "unknown function: " and " is unknown"
        if let Some(start) = msg.find("unknown function: ") {
            let after_prefix = &msg[start + 18..];
            if let Some(end) = after_prefix.find(" is unknown") {
                return Some(after_prefix[..end].to_string());
            }
        }
    }
    None
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
