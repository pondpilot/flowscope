//! dbt builtin macro implementations.
//!
//! This module provides stub implementations of common dbt macros for use in
//! template rendering. These stubs return placeholder values that allow the
//! SQL to be parsed for lineage analysis without requiring a full dbt runtime.
//!
//! # Limitations
//!
//! These are simplified stubs for static analysis, not full dbt macro implementations:
//!
//! - **`ref()`**: Returns the model name directly, doesn't resolve package.yml dependencies
//!   or handle versioned models (`v=N` parameter is ignored)
//! - **`source()`**: Returns `schema.table` format, doesn't validate source definitions
//! - **`var()`**: Falls back to the variable name if undefined (1-arg form) rather than
//!   erroring like real dbt would
//! - **`is_incremental()`**: Always returns `false`
//! - **Package namespaces** (dbt_utils, etc.): Return first string argument or placeholder
//!
//! For accurate lineage of complex dbt projects, consider using dbt's native `compile`
//! command and analyzing the rendered SQL.

use minijinja::{Environment, Value};
use std::collections::HashMap;

/// Default dbt package namespaces that are automatically registered as passthrough objects.
/// These allow templates with calls like `{{ dbt_utils.star(...) }}` to render successfully.
const DEFAULT_DBT_PACKAGES: &[&str] = &[
    "dbt_utils",
    "dbt_expectations",
    "dbt_date",
    "audit_helper",
    "codegen",
    "metrics",
    "elementary",
    "fivetran_utils",
];

/// Registers all dbt builtin macros with the MiniJinja environment.
pub(crate) fn register_dbt_builtins(
    env: &mut Environment,
    context: &HashMap<String, serde_json::Value>,
) {
    // Clone vars for use in closures
    let vars = extract_vars(context);

    // ref('model') or ref('project', 'model') -> returns quoted table name
    env.add_function("ref", |args: &[Value]| -> Result<Value, minijinja::Error> {
        match args.len() {
            1 => {
                let model = args[0].as_str().unwrap_or("model");
                Ok(Value::from(model))
            }
            2 => {
                let project = args[0].as_str().unwrap_or("project");
                let model = args[1].as_str().unwrap_or("model");
                Ok(Value::from(format!("{project}.{model}")))
            }
            _ => Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                "ref() expects 1 or 2 arguments",
            )),
        }
    });

    // source('schema', 'table') -> returns "schema.table"
    env.add_function(
        "source",
        |schema: Value, table: Value| -> Result<Value, minijinja::Error> {
            let schema_str = schema.as_str().unwrap_or("schema");
            let table_str = table.as_str().unwrap_or("table");
            Ok(Value::from(format!("{schema_str}.{table_str}")))
        },
    );

    // config(...) -> returns empty string (configuration macro, no SQL output)
    env.add_function("config", |_args: &[Value]| -> Value { Value::from("") });

    // var('name') or var('name', 'default') -> returns variable value or default
    let vars_clone = vars.clone();
    env.add_function(
        "var",
        move |args: &[Value]| -> Result<Value, minijinja::Error> {
            match args.len() {
                1 => {
                    let name = args[0].as_str().unwrap_or("");
                    match vars_clone.get(name) {
                        Some(v) => Ok(v.clone()),
                        None => Ok(Value::from(name)), // Return the var name as fallback
                    }
                }
                2 => {
                    let name = args[0].as_str().unwrap_or("");
                    let default = &args[1];
                    match vars_clone.get(name) {
                        Some(v) => Ok(v.clone()),
                        None => Ok(default.clone()),
                    }
                }
                _ => Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "var() expects 1 or 2 arguments",
                )),
            }
        },
    );

    // is_incremental() -> returns false (for lineage analysis, always false)
    env.add_function("is_incremental", || -> Value { Value::from(false) });

    // this -> represents the current model (returns Value::UNDEFINED for lineage)
    env.add_global("this", Value::UNDEFINED);

    // Register default dbt package namespaces as passthrough objects
    for package in DEFAULT_DBT_PACKAGES {
        env.add_global(*package, Value::from_object(PassthroughNamespace::new(package)));
    }

    // Register custom packages from context["dbt_packages"] if provided
    // Example: {"dbt_packages": ["my_package", "custom_utils"]}
    if let Some(serde_json::Value::Array(packages)) = context.get("dbt_packages") {
        for pkg in packages {
            if let Some(name) = pkg.as_str() {
                // Skip if already registered as a default package
                if !DEFAULT_DBT_PACKAGES.contains(&name) {
                    let name_owned = name.to_string();
                    env.add_global(
                        name_owned.clone(),
                        Value::from_object(PassthroughNamespace::new(&name_owned)),
                    );
                }
            }
        }
    }
}

/// A passthrough namespace object that accepts any method call.
///
/// This allows templates with calls like `{{ dbt_utils.star(...) }}` to render
/// successfully even without the actual macro implementation.
#[derive(Debug)]
struct PassthroughNamespace {
    name: String,
}

impl PassthroughNamespace {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl std::fmt::Display for PassthroughNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "__{}_namespace__", self.name)
    }
}

impl minijinja::value::Object for PassthroughNamespace {
    fn call_method(
        self: &std::sync::Arc<Self>,
        _state: &minijinja::State,
        method: &str,
        args: &[Value],
    ) -> Result<Value, minijinja::Error> {
        // Return the first argument if it's a string, otherwise return a placeholder
        if let Some(first) = args.first() {
            if let Some(s) = first.as_str() {
                return Ok(Value::from(s));
            }
        }
        // Return a placeholder that includes the method name
        Ok(Value::from(format!("__{}_{method}__", self.name)))
    }
}

/// Extracts the 'vars' section from the context if present.
fn extract_vars(context: &HashMap<String, serde_json::Value>) -> HashMap<String, Value> {
    let mut vars = HashMap::new();

    if let Some(serde_json::Value::Object(vars_obj)) = context.get("vars") {
        for (key, value) in vars_obj {
            vars.insert(key.clone(), Value::from_serialize(value));
        }
    }

    vars
}

#[cfg(test)]
mod tests {
    use super::super::jinja::render_dbt;
    use std::collections::HashMap;

    #[test]
    fn ref_single_arg() {
        let ctx = HashMap::new();
        let result = render_dbt("SELECT * FROM {{ ref('users') }}", &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    #[test]
    fn ref_two_args() {
        let ctx = HashMap::new();
        let result = render_dbt("SELECT * FROM {{ ref('analytics', 'users') }}", &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM analytics.users");
    }

    #[test]
    fn source_macro() {
        let ctx = HashMap::new();
        let result = render_dbt("SELECT * FROM {{ source('raw', 'events') }}", &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM raw.events");
    }

    #[test]
    fn config_macro_returns_empty() {
        let ctx = HashMap::new();
        let result = render_dbt(
            "{{ config(materialized='table') }}SELECT * FROM users",
            &ctx,
        )
        .unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    #[test]
    fn var_with_default() {
        let ctx = HashMap::new();
        let result = render_dbt("SELECT * FROM {{ var('schema', 'public') }}.users", &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM public.users");
    }

    #[test]
    fn var_from_context() {
        let mut ctx = HashMap::new();
        ctx.insert(
            "vars".to_string(),
            serde_json::json!({ "schema": "analytics" }),
        );

        let result = render_dbt("SELECT * FROM {{ var('schema', 'public') }}.users", &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM analytics.users");
    }

    #[test]
    fn is_incremental_returns_false() {
        let ctx = HashMap::new();
        let template = r#"{% if is_incremental() %}WHERE updated_at > (SELECT MAX(updated_at) FROM {{ this }}){% endif %}SELECT * FROM users"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    #[test]
    fn complex_dbt_model() {
        let ctx = HashMap::new();
        let template = r#"{{ config(materialized='incremental') }}

SELECT
    id,
    name,
    created_at
FROM {{ ref('stg_users') }}
{% if is_incremental() %}
WHERE created_at > (SELECT MAX(created_at) FROM {{ this }})
{% endif %}"#;

        let result = render_dbt(template, &ctx).unwrap();
        assert!(result.contains("FROM stg_users"));
        assert!(!result.contains("is_incremental"));
    }

    #[test]
    fn custom_dbt_package_from_context() {
        let mut ctx = HashMap::new();
        ctx.insert(
            "dbt_packages".to_string(),
            serde_json::json!(["my_custom_pkg"]),
        );

        // Custom package method should return first arg or placeholder
        let template = "SELECT {{ my_custom_pkg.generate_column('user_id') }} FROM users";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT user_id FROM users");
    }

    #[test]
    fn default_dbt_utils_package() {
        let ctx = HashMap::new();
        // dbt_utils is a default package, should work without explicit registration
        let template = "SELECT {{ dbt_utils.star('users') }} FROM users";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT users FROM users");
    }
}
