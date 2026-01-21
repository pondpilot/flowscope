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
//! - **`ref()`**: Returns a `RelationEmulator` that renders as the model name and supports
//!   attribute access like `.schema`, `.identifier`. Doesn't resolve package.yml dependencies
//!   or handle versioned models (`v=N` parameter is ignored)
//! - **`source()`**: Returns a `RelationEmulator` with `schema.table` format
//! - **`var()`**: Falls back to the variable name if undefined (1-arg form) rather than
//!   erroring like real dbt would
//! - **`is_incremental()`**: Always returns `false`
//! - **`this`**: Returns a `RelationEmulator` when `model_name` is provided in context
//! - **`execute`**: Always `false` for static analysis
//! - **Package namespaces** (dbt_utils, etc.): Return first string argument or placeholder
//!
//! For accurate lineage of complex dbt projects, consider using dbt's native `compile`
//! command and analyzing the rendered SQL.

use minijinja::{Environment, Value};
use std::collections::HashMap;
use std::sync::Arc;

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

/// Emulates a dbt Relation object for use in templates.
///
/// This allows templates to access attributes like `ref('model').identifier` or
/// `this.schema` while still rendering as a simple table reference for SQL parsing.
///
/// # Examples
///
/// ```jinja
/// SELECT * FROM {{ ref('users') }}                    -- renders as "users"
/// SELECT * FROM {{ ref('users').identifier }}         -- renders as "users"
/// SELECT * FROM {{ source('raw', 'events').schema }}  -- renders as "raw"
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
struct RelationEmulator {
    /// The database name (e.g., "warehouse")
    database: Option<String>,
    /// The schema name (e.g., "analytics", "raw")
    schema: Option<String>,
    /// The identifier/table name (e.g., "users", "events")
    identifier: String,
}

impl RelationEmulator {
    /// Creates a new RelationEmulator with just an identifier.
    fn new(identifier: impl Into<String>) -> Self {
        Self {
            database: None,
            schema: None,
            identifier: identifier.into(),
        }
    }

    /// Creates a RelationEmulator with schema and identifier.
    fn with_schema(schema: impl Into<String>, identifier: impl Into<String>) -> Self {
        Self {
            database: None,
            schema: Some(schema.into()),
            identifier: identifier.into(),
        }
    }

    /// Creates a RelationEmulator with database, schema, and identifier.
    fn with_database(
        database: impl Into<String>,
        schema: impl Into<String>,
        identifier: impl Into<String>,
    ) -> Self {
        Self {
            database: Some(database.into()),
            schema: Some(schema.into()),
            identifier: identifier.into(),
        }
    }
}

impl std::fmt::Display for RelationEmulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.database, &self.schema) {
            (Some(db), Some(schema)) => write!(f, "{}.{}.{}", db, schema, self.identifier),
            (None, Some(schema)) => write!(f, "{}.{}", schema, self.identifier),
            _ => write!(f, "{}", self.identifier),
        }
    }
}

impl minijinja::value::Object for RelationEmulator {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            // Core relation attributes
            "database" => Some(
                self.database
                    .as_ref()
                    .map(|s| Value::from(s.as_str()))
                    .unwrap_or(Value::UNDEFINED),
            ),
            "schema" => Some(
                self.schema
                    .as_ref()
                    .map(|s| Value::from(s.as_str()))
                    .unwrap_or(Value::UNDEFINED),
            ),
            "identifier" | "name" | "table" => Some(Value::from(self.identifier.as_str())),

            // Type checking attributes - return true for common relation type checks
            "is_table" | "is_view" | "is_cte" => Some(Value::from(true)),

            // Unknown attributes return None (undefined in MiniJinja).
            // In lenient mode this becomes undefined; in strict mode it errors.
            // This is different from call_method which is always lenient.
            _ => None,
        }
    }

    /// Handles method calls on RelationEmulator.
    ///
    /// # Lenient Behavior
    ///
    /// Unknown methods return `self` to allow chaining to proceed. This is
    /// deliberately lenient to keep templates compatible during static analysis,
    /// even when using dbt macros or methods we don't explicitly emulate.
    ///
    /// For example, `ref('x').some_custom_method()` will return the relation
    /// unchanged rather than erroring. This trades strictness for robustness
    /// in lineage analysis.
    fn call_method(
        self: &Arc<Self>,
        _state: &minijinja::State,
        method: &str,
        _args: &[Value],
    ) -> Result<Value, minijinja::Error> {
        match method {
            // Methods that return a string representation
            "render" => Ok(Value::from(self.to_string())),

            // Known methods that should allow chaining return the relation.
            // Clone the inner RelationEmulator, not the Arc.
            "quote" | "include" | "exclude" | "replace" => Ok(Value::from_object((**self).clone())),

            // Unknown methods also return self to allow chaining (see doc comment above)
            _ => Ok(Value::from_object((**self).clone())),
        }
    }

    /// Render the relation as a string (e.g., "schema.table" or just "table")
    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Use the Display implementation for rendering
        std::fmt::Display::fmt(self, f)
    }
}

/// Registers all dbt builtin macros with the MiniJinja environment.
pub(crate) fn register_dbt_builtins(
    env: &mut Environment,
    context: &HashMap<String, serde_json::Value>,
) {
    // Clone vars for use in closures
    let vars = extract_vars(context);

    // ref('model') or ref('project', 'model') -> returns RelationEmulator
    // In dbt, ref() with two args is ref('project', 'model'), not ref('schema', 'model')
    // For lineage purposes, we treat project as schema to produce schema.identifier output
    env.add_function("ref", |args: &[Value]| -> Result<Value, minijinja::Error> {
        match args.len() {
            1 => {
                let model = args[0].as_str().unwrap_or("model");
                Ok(Value::from_object(RelationEmulator::new(model)))
            }
            2 => {
                let project = args[0].as_str().unwrap_or("project");
                let model = args[1].as_str().unwrap_or("model");
                // Treat project as schema for qualified output (project.model)
                Ok(Value::from_object(RelationEmulator::with_schema(
                    project, model,
                )))
            }
            _ => Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                "ref() expects 1 or 2 arguments",
            )),
        }
    });

    // source('schema', 'table') -> returns RelationEmulator with schema.table format
    env.add_function(
        "source",
        |schema: Value, table: Value| -> Result<Value, minijinja::Error> {
            let schema_str = schema.as_str().unwrap_or("schema");
            let table_str = table.as_str().unwrap_or("table");
            Ok(Value::from_object(RelationEmulator::with_schema(
                schema_str, table_str,
            )))
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

    // this -> represents the current model as a RelationEmulator
    // If model_name is provided in context, create a proper RelationEmulator
    // Otherwise, use UNDEFINED for backwards compatibility
    let this_value = extract_this_relation(context);
    env.add_global("this", this_value);

    // execute -> always false for static analysis (prevents run_query execution)
    env.add_global("execute", Value::from(false));

    // env_var('name') or env_var('name', 'default') -> returns env var or default
    //
    // Security: Reads ONLY from context["env_vars"], never from the process environment.
    // This ensures deterministic, safe behavior for static analysis.
    //
    // The placeholder format includes the var name intentionally for debugging - the name
    // is already visible in the template source, so this doesn't leak new information.
    let env_vars = extract_env_vars(context);
    env.add_function(
        "env_var",
        move |args: &[Value]| -> Result<Value, minijinja::Error> {
            match args.len() {
                1 => {
                    let name = args[0].as_str().unwrap_or("");
                    match env_vars.get(name) {
                        Some(v) => Ok(v.clone()),
                        // Placeholder is SQL-safe and includes the name for debugging
                        None => Ok(Value::from(format!("__ENV_VAR_{name}__"))),
                    }
                }
                2 => {
                    let name = args[0].as_str().unwrap_or("");
                    let default = &args[1];
                    match env_vars.get(name) {
                        Some(v) => Ok(v.clone()),
                        None => Ok(default.clone()),
                    }
                }
                _ => Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "env_var() expects 1 or 2 arguments",
                )),
            }
        },
    );

    // run_query(sql) -> returns empty results structure for static analysis
    // Real dbt executes the query, but we return an empty iterable
    env.add_function("run_query", |_sql: Value| -> Value {
        // Return an empty list that can be iterated over
        // This allows templates like {% for row in run_query(sql) %} to work
        Value::from(Vec::<Value>::new())
    });

    // zip(list1, list2, ...) -> list of tuples (truncates to shortest)
    env.add_function("zip", |args: &[Value]| -> Result<Value, minijinja::Error> {
        zip_impl(args, false)
    });

    // zip_strict(list1, list2, ...) -> list of tuples (errors if lengths differ)
    env.add_function(
        "zip_strict",
        |args: &[Value]| -> Result<Value, minijinja::Error> { zip_impl(args, true) },
    );

    // Register default dbt package namespaces as passthrough objects
    for package in DEFAULT_DBT_PACKAGES {
        env.add_global(
            *package,
            Value::from_object(PassthroughNamespace::new(package)),
        );
    }

    // Register custom packages from context["dbt_packages"] if provided
    // Example: {"dbt_packages": ["my_package", "custom_utils"]}
    if let Some(serde_json::Value::Array(packages)) = context.get("dbt_packages") {
        for pkg in packages {
            if let Some(name) = pkg.as_str() {
                // Skip if already registered as a default package
                if !DEFAULT_DBT_PACKAGES.contains(&name) {
                    env.add_global(
                        name.to_string(),
                        Value::from_object(PassthroughNamespace::new(name)),
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

/// Converts a MiniJinja Value to a string for passthrough function output.
///
/// This is used by passthrough functions (unknown dbt macros, package namespaces)
/// to extract a reasonable string from their first argument.
///
/// # Accepted Types
///
/// - **Strings**: Returned as-is
/// - **Objects with render** (e.g., `RelationEmulator`): Uses their Display implementation
/// - **Numbers and booleans**: Converted to string
/// - **Undefined/None**: Returns `None`
/// - **Complex structures** (lists, maps without render): Returns `None` to avoid
///   unexpected stringification like `[object Object]`
pub(super) fn passthrough_arg_to_string(value: &Value) -> Option<String> {
    // Strings are returned directly
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }

    // Undefined and none values produce no output
    if value.is_undefined() || value.is_none() {
        return None;
    }

    // Objects with custom render (like RelationEmulator) use their Display impl
    // This check must come before primitives since objects might also be truthy/falsy
    if value.as_object().is_some() {
        return Some(value.to_string());
    }

    // Primitives (numbers, booleans) convert cleanly
    // Note: is_true() returns true for truthy values, including non-empty strings/collections
    // so we check number first, then use kind() for booleans
    if value.is_number() {
        return Some(value.to_string());
    }

    // Check for boolean kind specifically
    if matches!(value.kind(), minijinja::value::ValueKind::Bool) {
        return Some(value.to_string());
    }

    // Don't stringify complex structures (sequences, maps) without proper render
    // to avoid unexpected output like stringified JSON
    None
}

/// Maximum number of sequences that can be zipped together.
const MAX_ZIP_ARGS: usize = 100;

/// Maximum number of elements per sequence in zip operations.
const MAX_ZIP_SEQUENCE_LENGTH: usize = 10_000;

/// Shared implementation for zip() and zip_strict().
///
/// When `strict` is true, returns an error if sequences have different lengths.
/// When `strict` is false, truncates to the shortest sequence.
///
/// # Limits
///
/// To prevent DoS via memory exhaustion:
/// - Maximum 100 sequences can be zipped together
/// - Maximum 10,000 elements per sequence
fn zip_impl(args: &[Value], strict: bool) -> Result<Value, minijinja::Error> {
    if args.is_empty() {
        return Ok(Value::from(Vec::<Value>::new()));
    }

    // Safety limit: prevent excessive memory allocation from too many sequences
    if args.len() > MAX_ZIP_ARGS {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!(
                "zip: too many sequences ({}, max: {})",
                args.len(),
                MAX_ZIP_ARGS
            ),
        ));
    }

    // Convert all args to sequences with length validation
    let sequences: Vec<Vec<Value>> = args
        .iter()
        .map(|v| {
            let seq: Vec<Value> = v.try_iter()?.collect();
            // Safety limit: prevent excessive memory allocation from long sequences
            if seq.len() > MAX_ZIP_SEQUENCE_LENGTH {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!(
                        "zip: sequence too long ({} elements, max: {})",
                        seq.len(),
                        MAX_ZIP_SEQUENCE_LENGTH
                    ),
                ));
            }
            Ok(seq)
        })
        .collect::<Result<_, _>>()?;

    if sequences.is_empty() {
        return Ok(Value::from(Vec::<Value>::new()));
    }

    let result_len = if strict {
        // Validate all sequences have the same length
        let first_len = sequences[0].len();
        for (i, seq) in sequences.iter().enumerate().skip(1) {
            if seq.len() != first_len {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!(
                        "zip_strict: argument {} has length {} but argument 0 has length {}",
                        i,
                        seq.len(),
                        first_len
                    ),
                ));
            }
        }
        first_len
    } else {
        // Truncate to shortest
        sequences.iter().map(|s| s.len()).min().unwrap_or(0)
    };

    // Build result tuples
    let result: Vec<Value> = (0..result_len)
        .map(|i| Value::from(sequences.iter().map(|s| s[i].clone()).collect::<Vec<_>>()))
        .collect();

    Ok(Value::from(result))
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
            if let Some(rendered) = passthrough_arg_to_string(first) {
                return Ok(Value::from(rendered));
            }
        }
        // Return a placeholder that includes the method name
        Ok(Value::from(format!("__{}_{method}__", self.name)))
    }
}

/// Extracts a named object section from the context as a MiniJinja value map.
///
/// This is used to extract configuration objects like `vars`, `env_vars`, etc.
/// from the JSON context into a format suitable for MiniJinja.
///
/// # Arguments
///
/// * `context` - The JSON context map
/// * `key` - The key to extract (e.g., "vars", "env_vars")
///
/// # Returns
///
/// A HashMap of key-value pairs, or an empty map if the key doesn't exist
/// or isn't an object.
fn extract_context_object(
    context: &HashMap<String, serde_json::Value>,
    key: &str,
) -> HashMap<String, Value> {
    context
        .get(key)
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), Value::from_serialize(v)))
                .collect()
        })
        .unwrap_or_default()
}

/// Extracts the 'vars' section from the context if present.
fn extract_vars(context: &HashMap<String, serde_json::Value>) -> HashMap<String, Value> {
    extract_context_object(context, "vars")
}

/// Extracts the 'env_vars' section from the context if present.
fn extract_env_vars(context: &HashMap<String, serde_json::Value>) -> HashMap<String, Value> {
    extract_context_object(context, "env_vars")
}

/// Creates the `this` RelationEmulator from context if model info is provided.
///
/// Context keys used:
/// - `model_name`: The identifier for the current model (required for RelationEmulator)
/// - `schema`: The schema for the current model (optional)
/// - `database`: The database for the current model (optional)
fn extract_this_relation(context: &HashMap<String, serde_json::Value>) -> Value {
    let model_name = context
        .get("model_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    match model_name {
        Some(name) => {
            let schema = context
                .get("schema")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let database = context
                .get("database")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let relation = match (database, schema) {
                (Some(db), Some(sch)) => RelationEmulator::with_database(db, sch, name),
                (None, Some(sch)) => RelationEmulator::with_schema(sch, name),
                _ => RelationEmulator::new(name),
            };
            Value::from_object(relation)
        }
        // No model_name provided, keep backwards-compatible undefined
        None => Value::UNDEFINED,
    }
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

    #[test]
    fn dbt_utils_relation_argument_passthrough() {
        let ctx = HashMap::new();
        let template = "SELECT {{ dbt_utils.star(ref('orders')) }} FROM dual";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT orders FROM dual");
    }

    #[test]
    fn stubbed_custom_macro_preserves_relation_argument() {
        let ctx = HashMap::new();
        let template = "SELECT {{ custom_macro(ref('users')) }}";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT users");
    }

    // =========================================================================
    // RelationEmulator tests
    // =========================================================================

    #[test]
    fn ref_returns_relation_with_attribute_access() {
        let ctx = HashMap::new();
        // Access .identifier attribute on ref result
        let template = "SELECT * FROM {{ ref('users').identifier }}";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    #[test]
    fn source_returns_relation_with_schema_attribute() {
        let ctx = HashMap::new();
        // Access .schema attribute on source result
        let template = "SELECT '{{ source('raw', 'events').schema }}' as schema_name FROM {{ source('raw', 'events') }}";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT 'raw' as schema_name FROM raw.events");
    }

    #[test]
    fn ref_relation_include_method() {
        let ctx = HashMap::new();
        // .include() method should return relation for chaining
        let template = "SELECT * FROM {{ ref('users').include() }}";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    #[test]
    fn ref_relation_quote_method() {
        let ctx = HashMap::new();
        // .quote() should return relation so attribute access still works
        let template = "SELECT * FROM {{ ref('users').quote(identifier=False).identifier }}";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    // =========================================================================
    // this global tests
    // =========================================================================

    #[test]
    fn this_undefined_without_model_name() {
        let ctx = HashMap::new();
        // Without model_name in context, this is undefined (lenient mode renders empty)
        let template = "SELECT '{{ this }}' as this_value FROM users";
        let result = render_dbt(template, &ctx).unwrap();
        // In lenient mode, undefined renders as empty string
        assert_eq!(result, "SELECT '' as this_value FROM users");
    }

    #[test]
    fn this_with_model_name() {
        let mut ctx = HashMap::new();
        ctx.insert("model_name".to_string(), serde_json::json!("orders"));

        let template = "SELECT * FROM {{ this }}";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM orders");
    }

    #[test]
    fn this_with_model_name_and_schema() {
        let mut ctx = HashMap::new();
        ctx.insert("model_name".to_string(), serde_json::json!("orders"));
        ctx.insert("schema".to_string(), serde_json::json!("analytics"));

        let template = "SELECT * FROM {{ this }}";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM analytics.orders");
    }

    #[test]
    fn this_with_full_context() {
        let mut ctx = HashMap::new();
        ctx.insert("model_name".to_string(), serde_json::json!("orders"));
        ctx.insert("schema".to_string(), serde_json::json!("analytics"));
        ctx.insert("database".to_string(), serde_json::json!("warehouse"));

        let template = "SELECT * FROM {{ this }}";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM warehouse.analytics.orders");
    }

    #[test]
    fn this_attribute_access() {
        let mut ctx = HashMap::new();
        ctx.insert("model_name".to_string(), serde_json::json!("orders"));
        ctx.insert("schema".to_string(), serde_json::json!("analytics"));

        let template = "SELECT '{{ this.schema }}' as schema, '{{ this.identifier }}' as table_name FROM users";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(
            result,
            "SELECT 'analytics' as schema, 'orders' as table_name FROM users"
        );
    }

    // =========================================================================
    // execute flag tests
    // =========================================================================

    #[test]
    fn execute_flag_is_false() {
        let ctx = HashMap::new();
        let template = "{% if execute %}RUN THIS{% else %}SKIP THIS{% endif %}";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SKIP THIS");
    }

    // =========================================================================
    // env_var() tests
    // =========================================================================

    #[test]
    fn env_var_with_default() {
        let ctx = HashMap::new();
        let template = "SELECT '{{ env_var('DB_HOST', 'localhost') }}' as host FROM dual";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT 'localhost' as host FROM dual");
    }

    #[test]
    fn env_var_from_context() {
        let mut ctx = HashMap::new();
        ctx.insert(
            "env_vars".to_string(),
            serde_json::json!({ "DB_HOST": "prod-db.example.com" }),
        );

        let template = "SELECT '{{ env_var('DB_HOST', 'localhost') }}' as host FROM dual";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT 'prod-db.example.com' as host FROM dual");
    }

    #[test]
    fn env_var_without_default() {
        let ctx = HashMap::new();
        // Without default, returns a SQL-safe placeholder
        let template = "SELECT '{{ env_var('UNDEFINED_VAR') }}' as value FROM dual";
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(
            result,
            "SELECT '__ENV_VAR_UNDEFINED_VAR__' as value FROM dual"
        );
    }

    // =========================================================================
    // run_query() tests
    // =========================================================================

    #[test]
    fn run_query_returns_empty_iterable() {
        let ctx = HashMap::new();
        // run_query should return empty list, so loop produces nothing
        let template = r#"{% for row in run_query("SELECT 1") %}{{ row }}{% endfor %}SELECT 1"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT 1");
    }

    #[test]
    fn run_query_with_execute_check() {
        let ctx = HashMap::new();
        // Common pattern: only run query if execute is true
        let template = r#"{% if execute %}{% set results = run_query("SELECT 1") %}{% endif %}SELECT * FROM users"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    // =========================================================================
    // zip() function tests
    // =========================================================================

    #[test]
    fn zip_two_lists() {
        let ctx = HashMap::new();
        // zip() should combine two lists element-wise
        let template = r#"{% for a, b in zip(['x', 'y'], [1, 2]) %}{{ a }}{{ b }}{% if not loop.last %},{% endif %}{% endfor %}"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "x1,y2");
    }

    #[test]
    fn zip_three_lists() {
        let ctx = HashMap::new();
        // zip() should work with more than two lists
        let template = r#"{% for a, b, c in zip(['x', 'y'], [1, 2], ['!', '?']) %}{{ a }}{{ b }}{{ c }}{% if not loop.last %},{% endif %}{% endfor %}"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "x1!,y2?");
    }

    #[test]
    fn zip_unequal_lengths_truncates() {
        let ctx = HashMap::new();
        // zip() with unequal lengths should truncate to shortest
        let template = r#"{% for a, b in zip(['x', 'y', 'z'], [1, 2]) %}{{ a }}{{ b }}{% if not loop.last %},{% endif %}{% endfor %}"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "x1,y2");
    }

    #[test]
    fn zip_empty_list() {
        let ctx = HashMap::new();
        // zip() with empty list should produce empty result
        let template = r#"{% for a, b in zip([], [1, 2]) %}{{ a }}{{ b }}{% endfor %}DONE"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "DONE");
    }

    // =========================================================================
    // zip_strict() function tests
    // =========================================================================

    #[test]
    fn zip_strict_equal_lengths() {
        let ctx = HashMap::new();
        // zip_strict() should work like zip() when lengths are equal
        let template = r#"{% for a, b in zip_strict(['x', 'y'], [1, 2]) %}{{ a }}{{ b }}{% if not loop.last %},{% endif %}{% endfor %}"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "x1,y2");
    }

    #[test]
    fn zip_strict_unequal_lengths_errors() {
        let ctx = HashMap::new();
        // zip_strict() should error when lengths differ
        let template =
            r#"{% for a, b in zip_strict(['x', 'y', 'z'], [1, 2]) %}{{ a }}{{ b }}{% endfor %}"#;
        let result = render_dbt(template, &ctx);
        assert!(
            result.is_err(),
            "zip_strict with unequal lengths should error"
        );
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("zip_strict"),
            "Error should mention zip_strict: {}",
            err
        );
    }

    // =========================================================================
    // MiniJinja built-in feature tests (verifying they work in our context)
    // =========================================================================

    #[test]
    fn loop_first_variable() {
        let ctx = HashMap::new();
        // loop.first should be true on first iteration
        let template = r#"{% for item in ['a', 'b', 'c'] %}{% if loop.first %}FIRST:{% endif %}{{ item }}{% if not loop.last %},{% endif %}{% endfor %}"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "FIRST:a,b,c");
    }

    #[test]
    fn loop_index_variables() {
        let ctx = HashMap::new();
        // loop.index (1-based) and loop.index0 (0-based)
        let template = r#"{% for item in ['a', 'b'] %}{{ loop.index }}:{{ loop.index0 }}:{{ item }}{% if not loop.last %},{% endif %}{% endfor %}"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "1:0:a,2:1:b");
    }

    #[test]
    fn nested_loops_with_conditionals() {
        let ctx = HashMap::new();
        // Complex nesting of loops and conditionals
        let template = r#"{% for outer in ['X', 'Y'] %}{% for inner in [1, 2] %}{% if loop.first %}[{% endif %}{{ outer }}{{ inner }}{% if loop.last %}]{% endif %}{% endfor %}{% endfor %}"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "[X1X2][Y1Y2]");
    }

    #[test]
    fn whitespace_control_tags() {
        let ctx = HashMap::new();
        // {%- -%} strips surrounding whitespace
        let template = "SELECT\n  {%- for col in ['a', 'b'] %}\n  {{ col }}{%- if not loop.last %},{% endif %}\n  {%- endfor %}\nFROM t";
        let result = render_dbt(template, &ctx).unwrap();
        // Whitespace control should strip newlines before/after tags
        assert!(
            result.contains("a,"),
            "Should have 'a,' without extra whitespace: {}",
            result
        );
        assert!(
            !result.contains("\n\n\n"),
            "Should not have multiple blank lines: {}",
            result
        );
    }

    #[test]
    fn raw_block_preserves_syntax() {
        let ctx = HashMap::new();
        // {% raw %} block should output literal Jinja syntax
        let template = r#"{% raw %}{{ this_is_literal }}{% endraw %} and {{ ref('real') }}"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "{{ this_is_literal }} and real");
    }

    #[test]
    fn multi_variable_assignment_with() {
        let ctx = HashMap::new();
        // {% with %} block for scoped variables (MiniJinja's approach)
        let template = r#"{% with x = 'hello', y = 'world' %}{{ x }} {{ y }}{% endwith %}"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn inline_string_with_newlines() {
        let ctx = HashMap::new();
        // Strings can contain escaped newlines
        let template = r#"{{ "line1\nline2" }}"#;
        let result = render_dbt(template, &ctx).unwrap();
        assert_eq!(result, "line1\nline2");
    }
}
