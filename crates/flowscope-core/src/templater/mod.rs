//! SQL template preprocessing for Jinja2 and dbt-style templates.
//!
//! This module provides preprocessing support for templated SQL, allowing FlowScope
//! to analyze SQL files that use Jinja2 syntax or dbt macros.
//!
//! # Architecture
//!
//! Templating is a preprocessing step that runs before SQL parsing:
//!
//! ```text
//! Templated SQL → [templater] → Raw SQL → [parser] → AST → [analyzer] → Lineage
//! ```
//!
//! # Modes
//!
//! - **Raw**: No templating, SQL is passed through unchanged (default)
//! - **Jinja**: Standard Jinja2 template rendering with strict variable checking
//! - **Dbt**: Jinja2 with dbt builtin macros (`ref`, `source`, `config`, `var`, etc.)
//!
//! # Example
//!
//! ```
//! use flowscope_core::templater::{template_sql, TemplateConfig, TemplateMode};
//! use std::collections::HashMap;
//!
//! // dbt-style template
//! let template = r#"
//! {{ config(materialized='table') }}
//! SELECT * FROM {{ ref('users') }}
//! WHERE created_at > '{{ var("start_date", "2024-01-01") }}'
//! "#;
//!
//! let config = TemplateConfig {
//!     mode: TemplateMode::Dbt,
//!     context: HashMap::new(),
//! };
//!
//! let rendered = template_sql(template, &config).unwrap();
//! assert!(rendered.contains("FROM users"));
//! ```

mod dbt;
mod error;
mod jinja;

pub use error::TemplateError;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for SQL template preprocessing.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct TemplateConfig {
    /// The templating mode to use.
    #[serde(default)]
    pub mode: TemplateMode,

    /// Context variables available to the template.
    ///
    /// For dbt mode, variables under the "vars" key are accessible via `var()`.
    #[serde(default)]
    pub context: HashMap<String, serde_json::Value>,
}

/// Templating mode for SQL preprocessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum TemplateMode {
    /// No templating - SQL is passed through unchanged.
    #[default]
    Raw,

    /// Standard Jinja2 template rendering.
    ///
    /// Uses strict mode: undefined variables cause an error.
    Jinja,

    /// dbt-style templating with builtin macros.
    ///
    /// Includes stub implementations of:
    /// - `ref('model')` / `ref('project', 'model')` - model references
    /// - `source('schema', 'table')` - source table references
    /// - `config(...)` - model configuration (returns empty string)
    /// - `var('name')` / `var('name', 'default')` - variable access
    /// - `is_incremental()` - always returns false for lineage analysis
    /// - `this` - undefined (incremental model self-reference)
    Dbt,
}

/// Renders a SQL template according to the specified configuration.
///
/// This is the main entry point for template preprocessing. It dispatches
/// to the appropriate renderer based on the configured mode.
///
/// # Arguments
///
/// * `sql` - The SQL template string to render
/// * `config` - Configuration specifying the mode and context variables
///
/// # Returns
///
/// The rendered SQL string, or an error if rendering fails.
///
/// # Errors
///
/// - `TemplateError::SyntaxError` - Invalid template syntax
/// - `TemplateError::UndefinedVariable` - Undefined variable in Jinja mode
/// - `TemplateError::RenderError` - Other rendering failures
pub fn template_sql(sql: &str, config: &TemplateConfig) -> Result<String, TemplateError> {
    match config.mode {
        TemplateMode::Raw => Ok(sql.to_string()),
        TemplateMode::Jinja => jinja::render_jinja(sql, &config.context),
        TemplateMode::Dbt => jinja::render_dbt(sql, &config.context),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_mode_passes_through() {
        let sql = "SELECT * FROM {{ not_a_template }}";
        let config = TemplateConfig::default();

        let result = template_sql(sql, &config).unwrap();
        assert_eq!(result, sql);
    }

    #[test]
    fn jinja_mode_renders_variables() {
        let sql = "SELECT * FROM {{ table }}";
        let mut context = HashMap::new();
        context.insert("table".to_string(), serde_json::json!("users"));

        let config = TemplateConfig {
            mode: TemplateMode::Jinja,
            context,
        };

        let result = template_sql(sql, &config).unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    #[test]
    fn dbt_mode_renders_ref() {
        let sql = "SELECT * FROM {{ ref('users') }}";
        let config = TemplateConfig {
            mode: TemplateMode::Dbt,
            context: HashMap::new(),
        };

        let result = template_sql(sql, &config).unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    #[test]
    fn config_serialization() {
        let config = TemplateConfig {
            mode: TemplateMode::Dbt,
            context: HashMap::new(),
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"mode\":\"dbt\""));

        let parsed: TemplateConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mode, TemplateMode::Dbt);
    }

    #[test]
    fn config_deserialization_with_defaults() {
        let json = r#"{ "mode": "jinja" }"#;
        let config: TemplateConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.mode, TemplateMode::Jinja);
        assert!(config.context.is_empty());
    }
}
