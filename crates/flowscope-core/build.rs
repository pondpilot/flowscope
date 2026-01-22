//! Build script for flowscope-core.
//!
//! Generates Rust code from dialect semantic specifications in `specs/dialect-semantics/`.
//! Generated files are written to `src/generated/` and should be committed to version control.
//!
//! Data sources:
//! - `dialects.json`: Full dialect metadata (normalization, quote chars, parser/generator settings)
//! - `functions.json`: Function definitions with arg types, categories, and dialect availability
//! - `scoping_rules.toml`: Manually curated alias visibility rules
//! - `dialect_behavior.toml`: Manually curated function argument rules
//! - `normalization_overrides.toml`: Manual corrections to normalization
//! - `type_system.toml`: Type categories, aliases, implicit casts, and dialect mappings

use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;

/// Known dialect variants that must match the Dialect enum in types/request.rs.
const KNOWN_DIALECTS: &[&str] = &[
    "bigquery",
    "clickhouse",
    "databricks",
    "duckdb",
    "hive",
    "mssql",
    "mysql",
    "postgres",
    "redshift",
    "snowflake",
    "sqlite",
    // Note: "generic" and "ansi" are in the enum but not in specs - they use defaults
];

fn main() {
    if let Err(e) = run() {
        eprintln!("Build script error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let in_packaged_dir = manifest_dir.contains("/target/package/");

    if env::var("CARGO_PUBLISH").is_ok() || in_packaged_dir {
        println!("cargo:rerun-if-changed=specs/dialect-semantics/");
        println!("cargo:rerun-if-changed=build.rs");
        return Ok(());
    }

    let spec_dir = Path::new("specs/dialect-semantics");

    // Verify spec directory exists
    if !spec_dir.exists() {
        return Err(format!(
            "Spec directory not found at {:?}. Expected at crates/flowscope-core/specs/dialect-semantics/",
            spec_dir.canonicalize().unwrap_or_else(|_| spec_dir.to_path_buf())
        ).into());
    }

    // Create generated directory
    let generated_dir = Path::new("src/generated");
    fs::create_dir_all(generated_dir)
        .map_err(|e| format!("Failed to create src/generated directory: {e}"))?;

    // Load and parse specs (JSON for full data, TOML for manually curated)
    let dialects = load_dialects_json(spec_dir)?;
    let functions = load_functions_json(spec_dir)?;
    let normalization_overrides = load_normalization_overrides(spec_dir)?;
    let scoping_rules = load_scoping_rules(spec_dir)?;
    let dialect_behavior = load_dialect_behavior(spec_dir)?;
    let type_system = load_type_system(spec_dir)?;

    // Validate dialect coverage
    validate_dialect_coverage(&dialects, &scoping_rules);

    // Generate code
    generate_mod_rs(generated_dir)?;
    generate_case_sensitivity(generated_dir, &dialects, &normalization_overrides)?;
    generate_scoping_rules(generated_dir, &scoping_rules)?;
    generate_function_rules(generated_dir, &dialect_behavior)?;
    generate_functions(generated_dir, &functions)?;
    generate_type_system(generated_dir, &type_system)?;

    // Tell Cargo to rerun if specs change
    println!("cargo:rerun-if-changed=specs/dialect-semantics/");
    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}

// ============================================================================
// JSON Spec Structures (full technical detail)
// ============================================================================

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields loaded for future use
struct DialectSpec {
    normalization: String,
    #[serde(default)]
    pseudocolumns: Vec<String>,
    #[serde(default)]
    quote_chars: Option<QuoteChars>,
    #[serde(default)]
    parser_settings: Option<ParserSettings>,
    #[serde(default)]
    generator_settings: Option<GeneratorSettings>,
    #[serde(default)]
    type_mapping_count: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)] // Fields loaded for future use
struct QuoteChars {
    /// Can be strings or arrays of pairs like ["[", "]"] for SQLite
    #[serde(default)]
    identifier_quotes: Vec<serde_json::Value>,
    #[serde(default)]
    string_escapes: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)] // Fields loaded for future use
struct ParserSettings {
    #[serde(default)]
    tablesample_csv: bool,
    #[serde(default)]
    log_defaults_to_ln: bool,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)] // Fields loaded for future use
struct GeneratorSettings {
    #[serde(default)]
    limit_fetch: Option<String>,
    #[serde(default)]
    tablesample_size_is_rows: bool,
    #[serde(default)]
    locking_reads_supported: bool,
    #[serde(default)]
    null_ordering_supported: Option<bool>,
    #[serde(default)]
    ignore_nulls_in_func: bool,
    #[serde(default)]
    can_implement_array_any: bool,
    #[serde(default)]
    supports_table_alias_columns: bool,
    #[serde(default)]
    unpivot_aliases_are_identifiers: bool,
    #[serde(default)]
    custom_transforms_count: Option<usize>,
}

fn load_dialects_json(spec_dir: &Path) -> Result<BTreeMap<String, DialectSpec>, Box<dyn Error>> {
    let path = spec_dir.join("dialects.json");
    let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read {path:?}: {e}"))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse {path:?}: {e}").into())
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields loaded for future use
struct FunctionDef {
    class: String,
    categories: Vec<String>,
    #[serde(default)]
    sql_names: Vec<String>,
    #[serde(default)]
    arg_types: IndexMap<String, serde_json::Value>,
    #[serde(default)]
    dialects: Vec<String>,
    #[serde(default)]
    dialect_specific: bool,
    #[serde(default)]
    return_type: Option<ReturnTypeSpec>,
}

#[derive(Debug, Deserialize)]
struct ReturnTypeSpec {
    rule: String,
}

fn load_functions_json(spec_dir: &Path) -> Result<BTreeMap<String, FunctionDef>, Box<dyn Error>> {
    let path = spec_dir.join("functions.json");
    let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read {path:?}: {e}"))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse {path:?}: {e}").into())
}

// ============================================================================
// TOML Spec Structures (manually curated)
// ============================================================================

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields loaded for future use
struct NormalizationOverride {
    normalization_strategy: String,
    has_custom_normalization: bool,
    #[serde(default)]
    override_reason: Option<String>,
    #[serde(default)]
    udf_case_sensitive: Option<bool>,
    #[serde(default)]
    qualified_table_case_sensitive: Option<bool>,
}

fn load_normalization_overrides(
    spec_dir: &Path,
) -> Result<BTreeMap<String, NormalizationOverride>, Box<dyn Error>> {
    let path = spec_dir.join("normalization_overrides.toml");
    let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read {path:?}: {e}"))?;

    toml::from_str(&content).map_err(|e| format!("Failed to parse {path:?}: {e}").into())
}

#[derive(Debug, Deserialize)]
struct ScopingRule {
    alias_in_group_by: bool,
    alias_in_having: bool,
    alias_in_order_by: bool,
    lateral_column_alias: bool,
}

fn load_scoping_rules(spec_dir: &Path) -> Result<BTreeMap<String, ScopingRule>, Box<dyn Error>> {
    let path = spec_dir.join("scoping_rules.toml");
    let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read {path:?}: {e}"))?;

    toml::from_str(&content).map_err(|e| format!("Failed to parse {path:?}: {e}").into())
}

#[derive(Debug, Deserialize)]
struct DialectBehavior {
    value_table_functions: ValueTableFunctions,
    null_ordering: BTreeMap<String, String>,
    unnest: UnnestBehavior,
    date_functions: BTreeMap<String, BTreeMap<String, toml::Value>>,
}

#[derive(Debug, Deserialize)]
struct ValueTableFunctions {
    common: Vec<String>,
    #[serde(default)]
    postgres: Vec<String>,
    #[serde(default)]
    bigquery: Vec<String>,
    #[serde(default)]
    snowflake: Vec<String>,
    #[serde(default)]
    redshift: Vec<String>,
    #[serde(default)]
    mysql: Vec<String>,
    #[serde(default)]
    mssql: Vec<String>,
    #[serde(default)]
    duckdb: Vec<String>,
    #[serde(default)]
    clickhouse: Vec<String>,
    #[serde(default)]
    databricks: Vec<String>,
    #[serde(default)]
    hive: Vec<String>,
    #[serde(default)]
    sqlite: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UnnestBehavior {
    implicit_unnest: Vec<String>,
}

fn load_dialect_behavior(spec_dir: &Path) -> Result<DialectBehavior, Box<dyn Error>> {
    let path = spec_dir.join("dialect_behavior.toml");
    let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read {path:?}: {e}"))?;

    toml::from_str(&content).map_err(|e| format!("Failed to parse {path:?}: {e}").into())
}

#[derive(Debug, Deserialize)]
struct TypeSystem {
    type_categories: BTreeMap<String, Vec<String>>,
    implicit_casts: BTreeMap<String, Vec<String>>,
    dialect_type_mapping: BTreeMap<String, BTreeMap<String, String>>,
}

fn load_type_system(spec_dir: &Path) -> Result<TypeSystem, Box<dyn Error>> {
    let path = spec_dir.join("type_system.toml");
    let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read {path:?}: {e}"))?;

    toml::from_str(&content).map_err(|e| format!("Failed to parse {path:?}: {e}").into())
}

// ============================================================================
// Validation
// ============================================================================

fn validate_dialect_coverage(
    dialects: &BTreeMap<String, DialectSpec>,
    scoping: &BTreeMap<String, ScopingRule>,
) {
    let mut warnings = Vec::new();

    for dialect in KNOWN_DIALECTS {
        if !dialects.contains_key(*dialect) {
            warnings.push(format!("Dialect '{dialect}' missing from dialects.json"));
        }
        if !scoping.contains_key(*dialect) {
            warnings.push(format!(
                "Dialect '{dialect}' missing from scoping_rules.toml"
            ));
        }
    }

    for warning in &warnings {
        println!("cargo:warning={warning}");
    }
}

// ============================================================================
// Code Generation
// ============================================================================

fn write_if_changed(path: &Path, content: &str) -> Result<(), Box<dyn Error>> {
    let write_needed = match fs::read_to_string(path) {
        Ok(existing) => existing != content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => true,
        Err(err) => return Err(format!("Failed to read {path:?}: {err}").into()),
    };

    if write_needed {
        if let Err(err) = fs::write(path, content) {
            return Err(format!("Failed to write {path:?}: {err}").into());
        }
    }

    Ok(())
}

fn generate_mod_rs(dir: &Path) -> Result<(), Box<dyn Error>> {
    let content = r#"//! Generated dialect semantic code.
//!
//! DO NOT EDIT MANUALLY - generated by build.rs from specs/dialect-semantics/

pub mod case_sensitivity;
pub mod function_rules;
pub mod functions;
mod scoping_rules;
pub mod type_system;

pub use case_sensitivity::*;
pub use function_rules::*;
pub use functions::*;
pub use type_system::*;
// scoping_rules adds methods to Dialect via impl, no re-export needed
"#;

    write_if_changed(&dir.join("mod.rs"), content)
}

fn generate_case_sensitivity(
    dir: &Path,
    dialects: &BTreeMap<String, DialectSpec>,
    overrides: &BTreeMap<String, NormalizationOverride>,
) -> Result<(), Box<dyn Error>> {
    let mut code = String::from(
        r#"//! Case sensitivity rules per dialect.
//!
//! Generated from dialects.json and normalization_overrides.toml
//!
//! This module defines how SQL identifiers (table names, column names, etc.)
//! should be normalized for comparison. Different SQL dialects have different
//! rules for identifier case sensitivity.

use std::borrow::Cow;

use crate::Dialect;

/// Normalization strategy for identifier handling.
///
/// SQL dialects differ in how they handle identifier case. This enum represents
/// the different strategies used for normalizing identifiers during analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizationStrategy {
    /// Fold to lowercase (Postgres, Redshift)
    Lowercase,
    /// Fold to uppercase (Snowflake, Oracle)
    Uppercase,
    /// Case-insensitive comparison without folding
    CaseInsensitive,
    /// Case-sensitive, preserve exactly
    CaseSensitive,
}

impl NormalizationStrategy {
    /// Applies this normalization strategy to a string.
    ///
    /// Returns a `Cow<str>` to avoid allocation when no transformation is needed
    /// (i.e., for `CaseSensitive` strategy or when the string is already in the
    /// correct case).
    ///
    /// For `CaseInsensitive`, lowercase folding is used as the canonical form.
    ///
    /// # Example
    ///
    /// ```
    /// use std::borrow::Cow;
    /// use flowscope_core::generated::NormalizationStrategy;
    ///
    /// let strategy = NormalizationStrategy::Lowercase;
    /// assert_eq!(strategy.apply("MyTable"), "mytable");
    ///
    /// // CaseSensitive returns a borrowed reference (no allocation)
    /// let strategy = NormalizationStrategy::CaseSensitive;
    /// assert!(matches!(strategy.apply("MyTable"), Cow::Borrowed(_)));
    /// ```
    pub fn apply<'a>(&self, s: &'a str) -> Cow<'a, str> {
        match self {
            Self::CaseSensitive => Cow::Borrowed(s),
            Self::Lowercase | Self::CaseInsensitive => {
                // Optimization: only allocate if the string contains uppercase chars
                if s.chars().any(|c| c.is_uppercase()) {
                    Cow::Owned(s.to_lowercase())
                } else {
                    Cow::Borrowed(s)
                }
            }
            Self::Uppercase => {
                // Optimization: only allocate if the string contains lowercase chars
                if s.chars().any(|c| c.is_lowercase()) {
                    Cow::Owned(s.to_uppercase())
                } else {
                    Cow::Borrowed(s)
                }
            }
        }
    }
}

impl Dialect {
    /// Get the normalization strategy for this dialect.
    pub const fn normalization_strategy(&self) -> NormalizationStrategy {
        match self {
"#,
    );

    // Generate match arms
    for (dialect, spec) in dialects {
        if let Some(variant) = dialect_to_variant(dialect) {
            let strategy = match spec.normalization.as_str() {
                "lowercase" => "NormalizationStrategy::Lowercase",
                "uppercase" => "NormalizationStrategy::Uppercase",
                "case_insensitive" => "NormalizationStrategy::CaseInsensitive",
                "case_sensitive" => "NormalizationStrategy::CaseSensitive",
                other => {
                    println!(
                        "cargo:warning=Unknown normalization '{other}' for dialect '{dialect}', using CaseInsensitive"
                    );
                    "NormalizationStrategy::CaseInsensitive"
                }
            };
            code.push_str(&format!("            Dialect::{variant} => {strategy},\n"));
        }
    }

    // Add defaults for Generic and Ansi
    code.push_str("            Dialect::Generic => NormalizationStrategy::CaseInsensitive,\n");
    code.push_str("            Dialect::Ansi => NormalizationStrategy::Uppercase,\n");
    code.push_str("        }\n    }\n\n");

    // Generate has_custom_normalization
    let custom_dialects: Vec<_> = overrides
        .iter()
        .filter(|(_, o)| o.has_custom_normalization)
        .map(|(d, _)| d)
        .collect();

    if custom_dialects.is_empty() {
        code.push_str(
            r#"    /// Returns true if this dialect has custom normalization logic
    /// that cannot be captured by a simple strategy.
    pub const fn has_custom_normalization(&self) -> bool {
        false
    }
"#,
        );
    } else {
        // Use matches! macro for cleaner clippy-compliant code
        let variants: Vec<_> = custom_dialects
            .iter()
            .filter_map(|d| dialect_to_variant(d))
            .map(|v| format!("Dialect::{v}"))
            .collect();
        code.push_str(&format!(
            r#"    /// Returns true if this dialect has custom normalization logic
    /// that cannot be captured by a simple strategy.
    pub const fn has_custom_normalization(&self) -> bool {{
        matches!(self, {})
    }}
"#,
            variants.join(" | ")
        ));
    }

    // Generate pseudocolumns
    code.push_str(
        r#"
    /// Get pseudocolumns for this dialect (implicit columns like _PARTITIONTIME).
    pub fn pseudocolumns(&self) -> &'static [&'static str] {
        match self {
"#,
    );

    for (dialect, spec) in dialects {
        if !spec.pseudocolumns.is_empty() {
            if let Some(variant) = dialect_to_variant(dialect) {
                let cols: Vec<_> = spec
                    .pseudocolumns
                    .iter()
                    .map(|s| format!("\"{s}\""))
                    .collect();
                let cols_str = cols.join(", ");
                code.push_str(&format!(
                    "            Dialect::{variant} => &[{cols_str}],\n"
                ));
            }
        }
    }
    code.push_str("            _ => &[],\n");
    code.push_str("        }\n    }\n");

    // Generate identifier_quotes
    code.push_str(
        r#"
    /// Get the identifier quote characters for this dialect.
    /// Note: Some dialects use paired quotes (like SQLite's []) which are represented
    /// as single characters here - the opening bracket.
    pub fn identifier_quotes(&self) -> &'static [&'static str] {
        match self {
"#,
    );

    for (dialect, spec) in dialects {
        if let Some(ref qc) = spec.quote_chars {
            if !qc.identifier_quotes.is_empty() {
                if let Some(variant) = dialect_to_variant(dialect) {
                    let quotes: Vec<_> = qc
                        .identifier_quotes
                        .iter()
                        .filter_map(|v| {
                            match v {
                                serde_json::Value::String(s) => {
                                    let escaped = s.escape_default();
                                    Some(format!("\"{escaped}\""))
                                }
                                serde_json::Value::Array(arr) => {
                                    // Paired quotes like ["[", "]"] - use opening char
                                    arr.first().and_then(|v| v.as_str()).map(|s| {
                                        let escaped = s.escape_default();
                                        format!("\"{escaped}\"")
                                    })
                                }
                                _ => None,
                            }
                        })
                        .collect();
                    if !quotes.is_empty() {
                        let quotes_str = quotes.join(", ");
                        code.push_str(&format!(
                            "            Dialect::{variant} => &[{quotes_str}],\n"
                        ));
                    }
                }
            }
        }
    }
    code.push_str("            _ => &[\"\\\"\"],\n"); // Default: double quote
    code.push_str("        }\n    }\n}\n");

    write_if_changed(&dir.join("case_sensitivity.rs"), &code)
}

fn generate_scoping_rules(
    dir: &Path,
    rules: &BTreeMap<String, ScopingRule>,
) -> Result<(), Box<dyn Error>> {
    let mut code = String::from(
        r#"//! Alias visibility and scoping rules per dialect.
//!
//! Generated from scoping_rules.toml

use crate::Dialect;

impl Dialect {
    /// Whether SELECT aliases can be referenced in GROUP BY.
    pub const fn alias_in_group_by(&self) -> bool {
        match self {
"#,
    );

    for (dialect, rule) in rules {
        if let Some(variant) = dialect_to_variant(dialect) {
            let val = rule.alias_in_group_by;
            code.push_str(&format!("            Dialect::{variant} => {val},\n"));
        }
    }
    code.push_str("            _ => false, // Default: strict (Postgres-like)\n");
    code.push_str("        }\n    }\n\n");

    // alias_in_having
    code.push_str(
        r#"    /// Whether SELECT aliases can be referenced in HAVING.
    pub const fn alias_in_having(&self) -> bool {
        match self {
"#,
    );

    for (dialect, rule) in rules {
        if let Some(variant) = dialect_to_variant(dialect) {
            let val = rule.alias_in_having;
            code.push_str(&format!("            Dialect::{variant} => {val},\n"));
        }
    }
    code.push_str("            _ => false,\n");
    code.push_str("        }\n    }\n\n");

    // alias_in_order_by
    code.push_str(
        r#"    /// Whether SELECT aliases can be referenced in ORDER BY.
    pub const fn alias_in_order_by(&self) -> bool {
        match self {
"#,
    );

    for (dialect, rule) in rules {
        if let Some(variant) = dialect_to_variant(dialect) {
            let val = rule.alias_in_order_by;
            code.push_str(&format!("            Dialect::{variant} => {val},\n"));
        }
    }
    code.push_str("            _ => true, // ORDER BY alias is widely supported\n");
    code.push_str("        }\n    }\n\n");

    // lateral_column_alias
    code.push_str(
        r#"    /// Whether lateral column aliases are supported (referencing earlier SELECT items).
    pub const fn lateral_column_alias(&self) -> bool {
        match self {
"#,
    );

    for (dialect, rule) in rules {
        if let Some(variant) = dialect_to_variant(dialect) {
            let val = rule.lateral_column_alias;
            code.push_str(&format!("            Dialect::{variant} => {val},\n"));
        }
    }
    code.push_str("            _ => false,\n");
    code.push_str("        }\n    }\n}\n");

    write_if_changed(&dir.join("scoping_rules.rs"), &code)
}

fn generate_function_rules(dir: &Path, behavior: &DialectBehavior) -> Result<(), Box<dyn Error>> {
    let mut code = String::from(
        r#"//! Function argument handling rules per dialect.
//!
//! Generated from dialect_behavior.toml
//!
//! This module provides dialect-aware rules for function argument handling,
//! particularly for date/time functions where certain arguments are keywords
//! (like `YEAR`, `MONTH`) rather than column references.

use crate::Dialect;

/// Returns argument indices to skip when extracting column references from a function call.
///
/// Certain SQL functions take keyword arguments (e.g., `DATEDIFF(YEAR, start, end)` in Snowflake)
/// that should not be treated as column references during lineage analysis. This function
/// returns the indices of such arguments for the given function and dialect.
///
/// # Arguments
///
/// * `dialect` - The SQL dialect being analyzed
/// * `func_name` - The function name (case-insensitive, underscore-insensitive)
///
/// # Returns
///
/// A slice of argument indices (0-based) to skip. Returns an empty slice for
/// unknown functions or functions without skip rules.
///
/// # Example
///
/// ```ignore
/// // In Snowflake, DATEDIFF takes a unit as the first argument
/// let skip = skip_args_for_function(Dialect::Snowflake, "DATEDIFF");
/// assert_eq!(skip, &[0]); // Skip first argument (the unit)
///
/// // Both DATEADD and DATE_ADD match the same rules
/// let skip1 = skip_args_for_function(Dialect::Snowflake, "DATEADD");
/// let skip2 = skip_args_for_function(Dialect::Snowflake, "DATE_ADD");
/// assert_eq!(skip1, skip2);
/// ```
pub fn skip_args_for_function(dialect: Dialect, func_name: &str) -> &'static [usize] {
    // Normalize: lowercase and remove underscores to handle both DATEADD and DATE_ADD variants
    let func_normalized: String = func_name
        .chars()
        .filter(|c| *c != '_')
        .map(|c| c.to_ascii_lowercase())
        .collect();
    match func_normalized.as_str() {
"#,
    );

    // Group by function name
    for (func_name, dialect_rules) in &behavior.date_functions {
        // Normalize: lowercase and remove underscores to match input normalization
        let func_normalized: String = func_name
            .chars()
            .filter(|c| *c != '_')
            .flat_map(|c| c.to_lowercase())
            .collect();

        // Check for _default rule and count non-default dialect rules
        let has_default = dialect_rules.contains_key("_default");
        let dialect_specific_rules: Vec<_> = dialect_rules
            .iter()
            .filter(|(d, _)| *d != "_default" && dialect_to_variant(d).is_some())
            .collect();

        // If there are no dialect-specific rules, just use the default directly
        if dialect_specific_rules.is_empty() {
            if has_default {
                let default_indices = parse_skip_indices(dialect_rules.get("_default").unwrap());
                if default_indices.is_empty() {
                    code.push_str(&format!("        \"{func_normalized}\" => &[],\n"));
                } else {
                    let idx_str = default_indices
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    code.push_str(&format!("        \"{func_normalized}\" => &[{idx_str}],\n"));
                }
            } else {
                code.push_str(&format!("        \"{func_normalized}\" => &[],\n"));
            }
            continue;
        }

        // Generate match expression for functions with dialect-specific rules
        code.push_str(&format!(
            "        \"{func_normalized}\" => match dialect {{\n"
        ));

        for (dialect, value) in dialect_rules {
            if dialect == "_default" {
                continue;
            }
            if let Some(variant) = dialect_to_variant(dialect) {
                let indices = parse_skip_indices(value);
                if indices.is_empty() {
                    code.push_str(&format!("            Dialect::{variant} => &[],\n"));
                } else {
                    let idx_str = indices
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    code.push_str(&format!(
                        "            Dialect::{variant} => &[{idx_str}],\n"
                    ));
                }
            }
        }

        // Add default case
        if has_default {
            let default_indices = parse_skip_indices(dialect_rules.get("_default").unwrap());
            if default_indices.is_empty() {
                code.push_str("            _ => &[],\n");
            } else {
                let idx_str = default_indices
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                code.push_str(&format!("            _ => &[{idx_str}],\n"));
            }
        } else {
            code.push_str("            _ => &[],\n");
        }
        code.push_str("        },\n");
    }

    code.push_str("        _ => &[],\n");
    code.push_str("    }\n}\n\n");

    // Generate NULL ordering
    code.push_str(
        r#"
/// NULL ordering behavior in ORDER BY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullOrdering {
    /// NULLs sort as larger than all other values (NULLS LAST for ASC)
    NullsAreLarge,
    /// NULLs sort as smaller than all other values (NULLS FIRST for ASC)
    NullsAreSmall,
    /// NULLs always sort last regardless of ASC/DESC
    NullsAreLast,
}

impl Dialect {
    /// Get the default NULL ordering behavior for this dialect.
    pub const fn null_ordering(&self) -> NullOrdering {
        match self {
"#,
    );

    for (dialect, ordering) in &behavior.null_ordering {
        if let Some(variant) = dialect_to_variant(dialect) {
            let ordering_variant = match ordering.as_str() {
                "nulls_are_large" => "NullOrdering::NullsAreLarge",
                "nulls_are_small" => "NullOrdering::NullsAreSmall",
                "nulls_are_last" => "NullOrdering::NullsAreLast",
                _ => "NullOrdering::NullsAreLast",
            };
            code.push_str(&format!(
                "            Dialect::{variant} => {ordering_variant},\n"
            ));
        }
    }
    code.push_str("            _ => NullOrdering::NullsAreLast,\n");
    code.push_str("        }\n    }\n\n");

    // Generate implicit UNNEST
    let implicit_variants: Vec<_> = behavior
        .unnest
        .implicit_unnest
        .iter()
        .filter_map(|d| dialect_to_variant(d))
        .map(|v| format!("Dialect::{v}"))
        .collect();

    if implicit_variants.is_empty() {
        code.push_str(
            r#"    /// Whether this dialect supports implicit UNNEST (no CROSS JOIN needed).
    pub const fn supports_implicit_unnest(&self) -> bool {
        false
    }
}
"#,
        );
    } else {
        code.push_str(&format!(
            r#"    /// Whether this dialect supports implicit UNNEST (no CROSS JOIN needed).
    pub const fn supports_implicit_unnest(&self) -> bool {{
        matches!(self, {})
    }}
}}
"#,
            implicit_variants.join(" | ")
        ));
    }

    // Generate is_value_table_function
    generate_value_table_function(&mut code, behavior);

    write_if_changed(&dir.join("function_rules.rs"), &code)
}

fn generate_value_table_function(code: &mut String, behavior: &DialectBehavior) {
    let vtf = &behavior.value_table_functions;

    // Build common functions pattern
    let common_funcs: Vec<_> = vtf
        .common
        .iter()
        .map(|s| format!("\"{}\"", s.to_ascii_uppercase()))
        .collect();

    code.push_str(
        r#"
/// Checks if a function is a value table function (returns rows) for the given dialect.
///
/// Value table functions (like UNNEST, GENERATE_SERIES, FLATTEN) return rows/tables
/// rather than scalar values. This classification is used during lineage analysis
/// to determine how FROM clause function calls should be handled.
///
/// # Arguments
///
/// * `dialect` - The SQL dialect being analyzed
/// * `func_name` - The function name (case-insensitive)
///
/// # Returns
///
/// `true` if the function is a value table function for the given dialect.
///
/// # Example
///
/// ```ignore
/// use flowscope_core::generated::is_value_table_function;
/// use flowscope_core::Dialect;
///
/// assert!(is_value_table_function(Dialect::Postgres, "UNNEST"));
/// assert!(is_value_table_function(Dialect::Snowflake, "FLATTEN"));
/// assert!(!is_value_table_function(Dialect::Postgres, "COUNT"));
/// ```
pub fn is_value_table_function(dialect: Dialect, func_name: &str) -> bool {
    let name = func_name.to_ascii_uppercase();
    // Check common functions
"#,
    );

    if common_funcs.is_empty() {
        code.push_str("    // No common value table functions defined\n");
    } else {
        code.push_str(&format!(
            "    if matches!(name.as_str(), {}) {{\n        return true;\n    }}\n",
            common_funcs.join(" | ")
        ));
    }

    code.push_str("    // Check dialect-specific functions\n");
    code.push_str("    match dialect {\n");

    // Generate match arms for each dialect with non-empty value table functions
    let dialect_configs = [
        ("postgres", &vtf.postgres, "Postgres"),
        ("bigquery", &vtf.bigquery, "Bigquery"),
        ("snowflake", &vtf.snowflake, "Snowflake"),
        ("redshift", &vtf.redshift, "Redshift"),
        ("mysql", &vtf.mysql, "Mysql"),
        ("mssql", &vtf.mssql, "Mssql"),
        ("duckdb", &vtf.duckdb, "Duckdb"),
        ("clickhouse", &vtf.clickhouse, "Clickhouse"),
        ("databricks", &vtf.databricks, "Databricks"),
        ("hive", &vtf.hive, "Hive"),
        ("sqlite", &vtf.sqlite, "Sqlite"),
    ];

    for (_dialect_name, funcs, variant) in dialect_configs {
        // Filter out functions that are already in common
        let dialect_specific: Vec<_> = funcs
            .iter()
            .filter(|f| !vtf.common.iter().any(|c| c.eq_ignore_ascii_case(f)))
            .map(|s| format!("\"{}\"", s.to_ascii_uppercase()))
            .collect();

        if !dialect_specific.is_empty() {
            code.push_str(&format!(
                "        Dialect::{} => matches!(name.as_str(), {}),\n",
                variant,
                dialect_specific.join(" | ")
            ));
        }
    }

    code.push_str("        _ => false,\n");
    code.push_str("    }\n}\n");
}

/// Converts a PascalCase class name to a SQL function name (lowercase with underscores).
///
/// This function handles the conversion of class names from the spec function
/// definitions into SQL-style snake_case function names. The key challenge is
/// correctly handling acronyms (sequences of uppercase letters) like "JSON", "JSONB",
/// or "AI" that should remain together rather than being split at each letter.
///
/// # Algorithm
///
/// The function tracks three pieces of state as it processes each character:
/// - Whether the previous character was uppercase (for detecting acronym boundaries)
/// - Whether the previous character was a letter (for detecting word starts)
/// - The current position (for avoiding a leading underscore)
///
/// An underscore is inserted before an uppercase letter when:
/// 1. We're not at the start of the string, AND
/// 2. The previous character was a letter, AND
/// 3. Either:
///    - The previous character was lowercase (transitioning into a new word), OR
///    - The next character is lowercase (end of an acronym, start of a new word)
///
/// The third condition handles the tricky case of acronyms followed by regular words:
/// - "JSONBObject" -> "jsonb_object" (underscore before 'O' because 'b' follows 'B')
/// - "JSONB" -> "jsonb" (no underscore, all uppercase)
///
/// # Examples
///
/// ```text
/// "JSONBObjectAgg"   -> "jsonb_object_agg"   // Acronym + word + word
/// "FirstValue"       -> "first_value"        // Standard PascalCase
/// "RowNumber"        -> "row_number"         // Standard PascalCase
/// "AISummarizeAgg"   -> "ai_summarize_agg"   // Short acronym + words
/// "Count"            -> "count"              // Single word
/// "UNNEST"           -> "unnest"             // All uppercase acronym
/// ```
fn class_name_to_sql_name(class_name: &str) -> String {
    let mut result = String::new();
    let mut prev_was_upper = false;
    let mut prev_was_letter = false;

    for (i, c) in class_name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 && prev_was_letter {
                // Look ahead to see if the next character is lowercase.
                // This detects the boundary between an acronym and a following word.
                // For example, in "JSONBObject", when we reach 'O', we see that:
                // - prev_was_upper is true (from 'B')
                // - next char 'b' is lowercase
                // So we insert an underscore: "jsonb_object"
                let next_is_lower = class_name
                    .chars()
                    .nth(i + 1)
                    .is_some_and(|nc| nc.is_lowercase());

                // Insert underscore if:
                // - Previous was lowercase (normal word boundary: "First" + "Value")
                // - OR we're at the end of an acronym (next is lowercase)
                if !prev_was_upper || next_is_lower {
                    result.push('_');
                }
            }
            result.push(c.to_ascii_lowercase());
            prev_was_upper = true;
        } else {
            result.push(c.to_ascii_lowercase());
            prev_was_upper = false;
        }
        prev_was_letter = c.is_alphabetic();
    }

    result
}

/// Parsed function signature for completion
struct FunctionSigData {
    name: String,
    display_name: String,        // Uppercase version for display
    params: Vec<(String, bool)>, // (param_name, is_required)
    return_rule: Option<String>,
    category: String, // "aggregate", "window", "scalar"
}

fn generate_functions(
    dir: &Path,
    functions: &BTreeMap<String, FunctionDef>,
) -> Result<(), Box<dyn Error>> {
    let mut aggregates: BTreeSet<String> = BTreeSet::new();
    let mut windows: BTreeSet<String> = BTreeSet::new();
    let mut udtfs: BTreeSet<String> = BTreeSet::new();
    // Map from sql_name (lowercase) to return_type rule
    let mut return_types: BTreeMap<String, String> = BTreeMap::new();
    // Collect function signatures for completion
    let mut signatures: Vec<FunctionSigData> = Vec::new();

    for def in functions.values() {
        // Use the class name to derive the SQL function name, as the dictionary keys
        // may have mangled names (e.g., "J_S_O_N_B_OBJECT_AGG" instead of "JSONB_OBJECT_AGG")
        let sql_name = class_name_to_sql_name(&def.class);

        // Determine primary category for completion.
        //
        // Category Priority: aggregate > window > scalar
        //
        // Some functions can serve multiple roles (e.g., SUM can be both an aggregate
        // and a window function when used with OVER). For completion purposes, we assign
        // a single primary category using this priority:
        //
        // 1. "aggregate" - Functions that aggregate multiple rows (SUM, COUNT, AVG, etc.)
        //    These get special treatment in GROUP BY contexts.
        // 2. "window" - Functions that compute over a window frame (ROW_NUMBER, RANK, etc.)
        //    These get boosted in OVER/WINDOW contexts.
        // 3. "scalar" - Regular functions that operate on single values.
        //
        // This means a function marked as both "aggregate" and "window" will be classified
        // as "aggregate" for completion scoring. This is intentional: aggregate functions
        // are more commonly used outside window contexts, so we prioritize that behavior.
        let category = if def.categories.contains(&"aggregate".to_string()) {
            "aggregate"
        } else if def.categories.contains(&"window".to_string()) {
            "window"
        } else {
            "scalar"
        };

        for cat in &def.categories {
            match cat.as_str() {
                "aggregate" => {
                    aggregates.insert(sql_name.clone());
                }
                "window" => {
                    windows.insert(sql_name.clone());
                }
                "udtf" => {
                    udtfs.insert(sql_name.clone());
                }
                _ => {}
            }
        }

        // Collect return types
        if let Some(ref rt) = def.return_type {
            return_types.insert(sql_name.clone(), rt.rule.clone());
        }

        // Build parameter list from arg_types
        // Filter out internal/metadata fields
        let skip_params = [
            "bracket_notation",
            "struct_name_inheritance",
            "ensure_variant",
            "nulls_excluded",
        ];
        let params: Vec<(String, bool)> = def
            .arg_types
            .iter()
            .filter(|(k, _)| !skip_params.contains(&k.as_str()))
            .map(|(k, v)| {
                let required = v.as_bool().unwrap_or(false);
                (k.clone(), required)
            })
            .collect();

        signatures.push(FunctionSigData {
            name: sql_name.clone(),
            display_name: sql_name.to_ascii_uppercase(),
            params,
            return_rule: def.return_type.as_ref().map(|rt| rt.rule.clone()),
            category: category.to_string(),
        });
    }

    let mut code = String::from(
        r#"//! Function classification sets.
//!
//! Generated from functions.json
//!
//! This module provides sets of SQL function names categorized by their behavior
//! (aggregate, window, table-generating). These classifications are used during
//! lineage analysis to determine how expressions should be analyzed.

use std::collections::HashSet;
use std::sync::LazyLock;

"#,
    );

    // Generate AGGREGATE_FUNCTIONS
    let agg_count = aggregates.len();
    code.push_str(&format!("/// Aggregate functions ({agg_count} total).\n"));
    code.push_str(
        "pub static AGGREGATE_FUNCTIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {\n",
    );
    code.push_str("    let mut set = HashSet::new();\n");
    for func in &aggregates {
        code.push_str(&format!("    set.insert(\"{func}\");\n"));
    }
    code.push_str("    set\n});\n\n");

    // Generate WINDOW_FUNCTIONS
    let win_count = windows.len();
    code.push_str(&format!("/// Window functions ({win_count} total).\n"));
    code.push_str(
        "pub static WINDOW_FUNCTIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {\n",
    );
    code.push_str("    let mut set = HashSet::new();\n");
    for func in &windows {
        code.push_str(&format!("    set.insert(\"{func}\");\n"));
    }
    code.push_str("    set\n});\n\n");

    // Generate UDTF_FUNCTIONS
    let udtf_count = udtfs.len();
    code.push_str(&format!(
        "/// Table-generating functions / UDTFs ({udtf_count} total).\n"
    ));
    code.push_str(
        "pub static UDTF_FUNCTIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {\n",
    );
    code.push_str("    let mut set = HashSet::new();\n");
    for func in &udtfs {
        code.push_str(&format!("    set.insert(\"{func}\");\n"));
    }
    code.push_str("    set\n});\n\n");

    // Generate helper functions with ASCII lowercase optimization
    code.push_str(
        r#"/// Checks if a function is an aggregate function (e.g., SUM, COUNT, AVG).
///
/// Aggregate functions combine multiple input rows into a single output value.
/// This classification is used to detect aggregation in SELECT expressions
/// and validate GROUP BY semantics.
///
/// The check is case-insensitive. Uses ASCII lowercase for performance since
/// SQL function names are always ASCII.
pub fn is_aggregate_function(name: &str) -> bool {
    // SQL function names are ASCII, so we can use the faster ASCII lowercase
    let lower = name.to_ascii_lowercase();
    AGGREGATE_FUNCTIONS.contains(lower.as_str())
}

/// Checks if a function is a window function (e.g., ROW_NUMBER, RANK, LAG).
///
/// Window functions perform calculations across a set of rows related to
/// the current row, without collapsing them into a single output.
///
/// The check is case-insensitive. Uses ASCII lowercase for performance since
/// SQL function names are always ASCII.
pub fn is_window_function(name: &str) -> bool {
    // SQL function names are ASCII, so we can use the faster ASCII lowercase
    let lower = name.to_ascii_lowercase();
    WINDOW_FUNCTIONS.contains(lower.as_str())
}

/// Checks if a function is a table-generating function / UDTF (e.g., UNNEST, EXPLODE).
///
/// UDTFs return multiple rows for each input row, expanding the result set.
/// This classification affects how lineage is tracked through these functions.
///
/// The check is case-insensitive. Uses ASCII lowercase for performance since
/// SQL function names are always ASCII.
pub fn is_udtf_function(name: &str) -> bool {
    // SQL function names are ASCII, so we can use the faster ASCII lowercase
    let lower = name.to_ascii_lowercase();
    UDTF_FUNCTIONS.contains(lower.as_str())
}
"#,
    );

    // Generate ReturnTypeRule enum
    code.push_str(
        r#"
/// Return type rule for function type inference.
///
/// This enum represents the different strategies for determining a function's
/// return type during type inference in SQL analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnTypeRule {
    /// Returns Integer (e.g., COUNT, ROW_NUMBER)
    Integer,
    /// Returns Number (e.g., SUM, AVG)
    Numeric,
    /// Returns Text (e.g., CONCAT, SUBSTRING)
    Text,
    /// Returns Timestamp (e.g., NOW, CURRENT_TIMESTAMP)
    Timestamp,
    /// Returns Boolean (e.g., AND, OR)
    Boolean,
    /// Returns Date (e.g., CURRENT_DATE)
    Date,
    /// Returns same type as first argument (e.g., MIN, MAX, COALESCE)
    MatchFirstArg,
}

/// Infers the return type rule for a SQL function.
///
/// This function returns the return type rule for known SQL functions,
/// enabling data-driven type inference. The check is case-insensitive.
///
/// # Arguments
///
/// * `name` - The function name (case-insensitive)
///
/// # Returns
///
/// `Some(ReturnTypeRule)` if the function has a known return type rule,
/// `None` otherwise (fallback to existing logic).
///
/// # Example
///
/// ```ignore
/// use flowscope_core::generated::infer_function_return_type;
///
/// assert_eq!(infer_function_return_type("COUNT"), Some(ReturnTypeRule::Integer));
/// assert_eq!(infer_function_return_type("MIN"), Some(ReturnTypeRule::MatchFirstArg));
/// assert_eq!(infer_function_return_type("UNKNOWN_FUNC"), None);
/// ```
pub fn infer_function_return_type(name: &str) -> Option<ReturnTypeRule> {
    // SQL function names are ASCII, so we can use the faster ASCII lowercase
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
"#,
    );

    // Generate match arms for each function with a return type
    for (func_name, rule) in &return_types {
        let variant = match rule.as_str() {
            "integer" => "ReturnTypeRule::Integer",
            "numeric" => "ReturnTypeRule::Numeric",
            "text" => "ReturnTypeRule::Text",
            "timestamp" => "ReturnTypeRule::Timestamp",
            "boolean" => "ReturnTypeRule::Boolean",
            "date" => "ReturnTypeRule::Date",
            "match_first_arg" => "ReturnTypeRule::MatchFirstArg",
            unknown => {
                println!(
                    "cargo:warning=Unknown return_type rule '{unknown}' for function '{func_name}'"
                );
                continue;
            }
        };
        code.push_str(&format!("        \"{func_name}\" => Some({variant}),\n"));
    }

    code.push_str(
        r#"        _ => None,
    }
}
"#,
    );

    // Generate FunctionCategory enum and FunctionSignature struct
    code.push_str(
        r#"
/// Function category for completion context filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionCategory {
    /// Aggregate functions (SUM, COUNT, AVG, etc.)
    Aggregate,
    /// Window functions (ROW_NUMBER, RANK, LAG, etc.)
    Window,
    /// Scalar functions (LOWER, CONCAT, ABS, etc.)
    Scalar,
}

/// Function parameter information for completion.
#[derive(Debug, Clone)]
pub struct FunctionParam {
    /// Parameter name (e.g., "expression", "separator")
    pub name: &'static str,
    /// Whether this parameter is required
    pub required: bool,
}

/// Function signature for smart completion.
///
/// Contains all metadata needed to display rich function completions
/// with parameter hints and return type information.
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Function name in lowercase (for lookup)
    pub name: &'static str,
    /// Display name in uppercase (for completion label)
    pub display_name: &'static str,
    /// Function parameters
    pub params: &'static [FunctionParam],
    /// Return type rule, if known
    pub return_type: Option<ReturnTypeRule>,
    /// Function category
    pub category: FunctionCategory,
}

impl FunctionSignature {
    /// Formats the function signature as "NAME(params) → TYPE"
    pub fn format_signature(&self) -> String {
        let params_str = self.params
            .iter()
            .map(|p| {
                if p.required {
                    p.name.to_string()
                } else {
                    format!("[{}]", p.name)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        let return_str = self.return_type
            .map(|rt| format!(" → {}", match rt {
                ReturnTypeRule::Integer => "INTEGER",
                ReturnTypeRule::Numeric => "NUMERIC",
                ReturnTypeRule::Text => "TEXT",
                ReturnTypeRule::Timestamp => "TIMESTAMP",
                ReturnTypeRule::Boolean => "BOOLEAN",
                ReturnTypeRule::Date => "DATE",
                ReturnTypeRule::MatchFirstArg => "T",
            }))
            .unwrap_or_default();

        format!("{}({}){}", self.display_name, params_str, return_str)
    }
}

"#,
    );

    // Generate static function signatures
    code.push_str("/// Static function parameter definitions.\n");

    // First, generate static parameter arrays for each function
    for sig in &signatures {
        if sig.params.is_empty() {
            continue;
        }
        let params_name = format!("PARAMS_{}", sig.name.to_ascii_uppercase());
        code.push_str(&format!("static {params_name}: &[FunctionParam] = &[\n"));
        for (param_name, required) in &sig.params {
            code.push_str(&format!(
                "    FunctionParam {{ name: \"{param_name}\", required: {required} }},\n"
            ));
        }
        code.push_str("];\n");
    }

    // Generate the lookup function
    code.push_str(
        r#"
/// Looks up a function signature by name.
///
/// Returns the complete function signature including parameters, return type,
/// and category. The lookup is case-insensitive.
///
/// # Arguments
///
/// * `name` - The function name (case-insensitive)
///
/// # Returns
///
/// `Some(FunctionSignature)` if the function is known, `None` otherwise.
pub fn get_function_signature(name: &str) -> Option<FunctionSignature> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
"#,
    );

    for sig in &signatures {
        let params_ref = if sig.params.is_empty() {
            "&[]".to_string()
        } else {
            format!("PARAMS_{}", sig.name.to_ascii_uppercase())
        };

        let return_type = sig
            .return_rule
            .as_ref()
            .and_then(|r| match r.as_str() {
                "integer" => Some("Some(ReturnTypeRule::Integer)"),
                "numeric" => Some("Some(ReturnTypeRule::Numeric)"),
                "text" => Some("Some(ReturnTypeRule::Text)"),
                "timestamp" => Some("Some(ReturnTypeRule::Timestamp)"),
                "boolean" => Some("Some(ReturnTypeRule::Boolean)"),
                "date" => Some("Some(ReturnTypeRule::Date)"),
                "match_first_arg" => Some("Some(ReturnTypeRule::MatchFirstArg)"),
                _ => None,
            })
            .unwrap_or("None");

        let category = match sig.category.as_str() {
            "aggregate" => "FunctionCategory::Aggregate",
            "window" => "FunctionCategory::Window",
            _ => "FunctionCategory::Scalar",
        };

        code.push_str(&format!(
            r#"        "{}" => Some(FunctionSignature {{
            name: "{}",
            display_name: "{}",
            params: {},
            return_type: {},
            category: {},
        }}),
"#,
            sig.name, sig.name, sig.display_name, params_ref, return_type, category
        ));
    }

    code.push_str(
        r#"        _ => None,
    }
}

/// Returns all function signatures for completion.
///
/// This provides access to all known SQL functions for populating
/// completion lists. Functions are returned in a static slice for efficiency.
pub fn all_function_signatures() -> impl Iterator<Item = FunctionSignature> {
    static NAMES: &[&str] = &[
"#,
    );

    for sig in &signatures {
        code.push_str(&format!("        \"{}\",\n", sig.name));
    }

    code.push_str(
        r#"    ];
    NAMES.iter().filter_map(|name| get_function_signature(name))
}
"#,
    );

    write_if_changed(&dir.join("functions.rs"), &code)
}

fn generate_type_system(dir: &Path, type_system: &TypeSystem) -> Result<(), Box<dyn Error>> {
    let mut code = String::from(
        r#"//! SQL Type System for cross-dialect type normalization and compatibility.
//!
//! Generated from type_system.toml
//!
//! This module provides canonical SQL types, type normalization from dialect-specific
//! names to canonical types, implicit cast checking, and dialect-specific type name mapping.

use crate::Dialect;
use std::fmt;

/// Canonical SQL types for cross-dialect type system.
///
/// These represent the fundamental SQL type categories that can be mapped
/// from various dialect-specific type names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanonicalType {
    Integer,
    Float,
    Text,
    Boolean,
    Timestamp,
    Date,
    Time,
    Binary,
    Json,
    Array,
}

impl CanonicalType {
    /// Returns the canonical type name as a lowercase string.
    pub const fn as_str(&self) -> &'static str {
        match self {
            CanonicalType::Integer => "integer",
            CanonicalType::Float => "float",
            CanonicalType::Text => "text",
            CanonicalType::Boolean => "boolean",
            CanonicalType::Timestamp => "timestamp",
            CanonicalType::Date => "date",
            CanonicalType::Time => "time",
            CanonicalType::Binary => "binary",
            CanonicalType::Json => "json",
            CanonicalType::Array => "array",
        }
    }

    /// Returns the canonical type name as an uppercase SQL-standard string.
    ///
    /// This is more efficient than `to_string()` when you only need a static string,
    /// as it avoids heap allocation.
    pub const fn as_uppercase_str(&self) -> &'static str {
        match self {
            CanonicalType::Integer => "INTEGER",
            CanonicalType::Float => "FLOAT",
            CanonicalType::Text => "TEXT",
            CanonicalType::Boolean => "BOOLEAN",
            CanonicalType::Timestamp => "TIMESTAMP",
            CanonicalType::Date => "DATE",
            CanonicalType::Time => "TIME",
            CanonicalType::Binary => "BINARY",
            CanonicalType::Json => "JSON",
            CanonicalType::Array => "ARRAY",
        }
    }
}

impl fmt::Display for CanonicalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Use uppercase for SQL-standard output representation
        match self {
            CanonicalType::Integer => write!(f, "INTEGER"),
            CanonicalType::Float => write!(f, "FLOAT"),
            CanonicalType::Text => write!(f, "TEXT"),
            CanonicalType::Boolean => write!(f, "BOOLEAN"),
            CanonicalType::Timestamp => write!(f, "TIMESTAMP"),
            CanonicalType::Date => write!(f, "DATE"),
            CanonicalType::Time => write!(f, "TIME"),
            CanonicalType::Binary => write!(f, "BINARY"),
            CanonicalType::Json => write!(f, "JSON"),
            CanonicalType::Array => write!(f, "ARRAY"),
        }
    }
}

/// Normalize a type name to its canonical type.
///
/// This function maps any dialect-specific type alias (e.g., "INT64", "VARCHAR",
/// "TIMESTAMPTZ") to its canonical type category.
///
/// # Arguments
///
/// * `type_name` - The type name to normalize (case-insensitive)
///
/// # Returns
///
/// `Some(CanonicalType)` if the type name is recognized, `None` otherwise.
///
/// # Example
///
/// ```ignore
/// use flowscope_core::generated::normalize_type_name;
/// use flowscope_core::generated::CanonicalType;
///
/// assert_eq!(normalize_type_name("INT64"), Some(CanonicalType::Integer));
/// assert_eq!(normalize_type_name("varchar"), Some(CanonicalType::Text));
/// assert_eq!(normalize_type_name("UNKNOWN_TYPE"), None);
/// ```
pub fn normalize_type_name(type_name: &str) -> Option<CanonicalType> {
    let lower = type_name.to_ascii_lowercase();
    match lower.as_str() {
"#,
    );

    // Generate match arms for each canonical type and its aliases
    for (canonical_type, aliases) in &type_system.type_categories {
        let variant = canonical_type_to_variant(canonical_type);
        let alias_patterns: Vec<_> = aliases.iter().map(|a| format!("\"{}\"", a)).collect();
        let patterns = alias_patterns.join(" | ");
        code.push_str(&format!(
            "        {patterns} => Some(CanonicalType::{variant}),\n"
        ));
    }

    code.push_str(
        r#"        _ => None,
    }
}

/// Check if a type can be implicitly cast to another type.
///
/// Implicit casts are automatic type conversions that SQL engines perform
/// without requiring explicit CAST expressions. This function returns true
/// if the source type can be implicitly converted to the target type.
///
/// Note: A type can always be implicitly cast to itself (identity cast).
///
/// # Arguments
///
/// * `from` - The source canonical type
/// * `to` - The target canonical type
///
/// # Returns
///
/// `true` if implicit cast is allowed, `false` otherwise.
///
/// # Example
///
/// ```ignore
/// use flowscope_core::generated::{can_implicitly_cast, CanonicalType};
///
/// // Integer can be cast to Float
/// assert!(can_implicitly_cast(CanonicalType::Integer, CanonicalType::Float));
/// // Float cannot be cast to Integer implicitly
/// assert!(!can_implicitly_cast(CanonicalType::Float, CanonicalType::Integer));
/// // Any type can be cast to itself
/// assert!(can_implicitly_cast(CanonicalType::Text, CanonicalType::Text));
/// ```
pub fn can_implicitly_cast(from: CanonicalType, to: CanonicalType) -> bool {
    if from == to {
        return true;
    }
    match from {
"#,
    );

    // Generate match arms for implicit casts
    for (from_type, to_types) in &type_system.implicit_casts {
        let from_variant = canonical_type_to_variant(from_type);
        if to_types.is_empty() {
            code.push_str(&format!(
                "        CanonicalType::{from_variant} => false,\n"
            ));
        } else {
            let to_variants: Vec<_> = to_types
                .iter()
                .map(|t| format!("CanonicalType::{}", canonical_type_to_variant(t)))
                .collect();
            let matches_pattern = to_variants.join(" | ");
            code.push_str(&format!(
                "        CanonicalType::{from_variant} => matches!(to, {matches_pattern}),\n"
            ));
        }
    }

    code.push_str(
        r#"        _ => false,
    }
}

/// Get the dialect-specific type name for a canonical type.
///
/// This function returns the preferred type name for a given canonical type
/// in a specific SQL dialect.
///
/// # Arguments
///
/// * `dialect` - The SQL dialect
/// * `canonical` - The canonical type
///
/// # Returns
///
/// The dialect-specific type name as a static string.
///
/// # Example
///
/// ```ignore
/// use flowscope_core::generated::{dialect_type_name, CanonicalType};
/// use flowscope_core::Dialect;
///
/// assert_eq!(dialect_type_name(Dialect::Bigquery, CanonicalType::Integer), "INT64");
/// assert_eq!(dialect_type_name(Dialect::Postgres, CanonicalType::Text), "text");
/// ```
pub fn dialect_type_name(dialect: Dialect, canonical: CanonicalType) -> &'static str {
    match canonical {
"#,
    );

    // Generate match arms for dialect type mapping
    for (canonical_type, dialect_mapping) in &type_system.dialect_type_mapping {
        let variant = canonical_type_to_variant(canonical_type);
        code.push_str(&format!(
            "        CanonicalType::{variant} => match dialect {{\n"
        ));

        for (dialect_name, type_name) in dialect_mapping {
            if let Some(dialect_variant) = dialect_to_variant(dialect_name) {
                code.push_str(&format!(
                    "            Dialect::{dialect_variant} => \"{type_name}\",\n"
                ));
            }
        }

        // Add default based on the canonical type name
        let default_name = canonical_type.to_uppercase();
        code.push_str(&format!("            _ => \"{default_name}\",\n"));
        code.push_str("        },\n");
    }

    // Handle types not in the dialect_type_mapping (time, binary, json, array)
    let mapped_types: BTreeSet<_> = type_system.dialect_type_mapping.keys().collect();
    let all_types = [
        "integer",
        "float",
        "text",
        "boolean",
        "timestamp",
        "date",
        "time",
        "binary",
        "json",
        "array",
    ];

    for type_name in all_types {
        if !mapped_types.contains(&type_name.to_string()) {
            let variant = canonical_type_to_variant(type_name);
            let default_name = type_name.to_uppercase();
            code.push_str(&format!(
                "        CanonicalType::{variant} => \"{default_name}\",\n"
            ));
        }
    }

    code.push_str(
        r#"    }
}
"#,
    );

    write_if_changed(&dir.join("type_system.rs"), &code)
}

/// Convert canonical type name to Rust enum variant.
fn canonical_type_to_variant(type_name: &str) -> &'static str {
    match type_name.to_lowercase().as_str() {
        "integer" => "Integer",
        "float" => "Float",
        "text" => "Text",
        "boolean" => "Boolean",
        "timestamp" => "Timestamp",
        "date" => "Date",
        "time" => "Time",
        "binary" => "Binary",
        "json" => "Json",
        "array" => "Array",
        other => {
            println!("cargo:warning=Unknown type category '{other}' in type_system.toml, defaulting to Text");
            "Text"
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert dialect name from spec to Rust enum variant.
fn dialect_to_variant(dialect: &str) -> Option<&'static str> {
    match dialect.to_lowercase().as_str() {
        "bigquery" => Some("Bigquery"),
        "clickhouse" => Some("Clickhouse"),
        "databricks" => Some("Databricks"),
        "duckdb" => Some("Duckdb"),
        "hive" => Some("Hive"),
        "mssql" | "tsql" => Some("Mssql"),
        "mysql" => Some("Mysql"),
        "postgres" => Some("Postgres"),
        "redshift" => Some("Redshift"),
        "snowflake" => Some("Snowflake"),
        "sqlite" => Some("Sqlite"),
        // Dialects in specs but not in our enum
        "doris" | "drill" | "oracle" | "presto" | "spark" | "starrocks" | "tableau"
        | "teradata" | "trino" => None,
        _ => {
            println!("cargo:warning=Unknown dialect '{dialect}' in specs");
            None
        }
    }
}

/// Parse skip indices from TOML value.
fn parse_skip_indices(value: &toml::Value) -> Vec<usize> {
    match value {
        toml::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_integer().map(|i| i as usize))
            .collect(),
        _ => vec![],
    }
}
