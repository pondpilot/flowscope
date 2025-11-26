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

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
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
    let spec_dir = Path::new("specs/dialect-semantics");

    // Verify spec directory exists
    if !spec_dir.exists() {
        panic!(
            "Spec directory not found at {:?}. Expected at crates/flowscope-core/specs/dialect-semantics/",
            spec_dir.canonicalize().unwrap_or_else(|_| spec_dir.to_path_buf())
        );
    }

    // Create generated directory
    let generated_dir = Path::new("src/generated");
    fs::create_dir_all(generated_dir).expect("Failed to create src/generated directory");

    // Load and parse specs (JSON for full data, TOML for manually curated)
    let dialects = load_dialects_json(spec_dir);
    let functions = load_functions_json(spec_dir);
    let normalization_overrides = load_normalization_overrides(spec_dir);
    let scoping_rules = load_scoping_rules(spec_dir);
    let dialect_behavior = load_dialect_behavior(spec_dir);

    // Validate dialect coverage
    validate_dialect_coverage(&dialects, &scoping_rules);

    // Generate code
    generate_mod_rs(generated_dir);
    generate_case_sensitivity(generated_dir, &dialects, &normalization_overrides);
    generate_scoping_rules(generated_dir, &scoping_rules);
    generate_function_rules(generated_dir, &dialect_behavior);
    generate_functions(generated_dir, &functions);

    // Tell Cargo to rerun if specs change
    println!("cargo:rerun-if-changed=specs/dialect-semantics/");
    println!("cargo:rerun-if-changed=build.rs");
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

fn load_dialects_json(spec_dir: &Path) -> BTreeMap<String, DialectSpec> {
    let path = spec_dir.join("dialects.json");
    let content =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {path:?}: {e}"));

    serde_json::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {path:?}: {e}"))
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields loaded for future use
struct FunctionDef {
    class: String,
    categories: Vec<String>,
    #[serde(default)]
    sql_names: Vec<String>,
    #[serde(default)]
    arg_types: HashMap<String, serde_json::Value>,
    #[serde(default)]
    dialects: Vec<String>,
    #[serde(default)]
    dialect_specific: bool,
}

fn load_functions_json(spec_dir: &Path) -> BTreeMap<String, FunctionDef> {
    let path = spec_dir.join("functions.json");
    let content =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {path:?}: {e}"));

    serde_json::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {path:?}: {e}"))
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

fn load_normalization_overrides(spec_dir: &Path) -> BTreeMap<String, NormalizationOverride> {
    let path = spec_dir.join("normalization_overrides.toml");
    let content =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {path:?}: {e}"));

    toml::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {path:?}: {e}"))
}

#[derive(Debug, Deserialize)]
struct ScopingRule {
    alias_in_group_by: bool,
    alias_in_having: bool,
    alias_in_order_by: bool,
    lateral_column_alias: bool,
}

fn load_scoping_rules(spec_dir: &Path) -> BTreeMap<String, ScopingRule> {
    let path = spec_dir.join("scoping_rules.toml");
    let content =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {path:?}: {e}"));

    toml::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {path:?}: {e}"))
}

#[derive(Debug, Deserialize)]
struct DialectBehavior {
    null_ordering: BTreeMap<String, String>,
    unnest: UnnestBehavior,
    date_functions: BTreeMap<String, BTreeMap<String, toml::Value>>,
}

#[derive(Debug, Deserialize)]
struct UnnestBehavior {
    implicit_unnest: Vec<String>,
}

fn load_dialect_behavior(spec_dir: &Path) -> DialectBehavior {
    let path = spec_dir.join("dialect_behavior.toml");
    let content =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {path:?}: {e}"));

    toml::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {path:?}: {e}"))
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

fn generate_mod_rs(dir: &Path) {
    let content = r#"//! Generated dialect semantic code.
//!
//! DO NOT EDIT MANUALLY - generated by build.rs from specs/dialect-semantics/

pub mod case_sensitivity;
pub mod function_rules;
pub mod functions;
mod scoping_rules;

pub use case_sensitivity::*;
pub use function_rules::*;
pub use functions::*;
// scoping_rules adds methods to Dialect via impl, no re-export needed
"#;

    fs::write(dir.join("mod.rs"), content).expect("Failed to write mod.rs");
}

fn generate_case_sensitivity(
    dir: &Path,
    dialects: &BTreeMap<String, DialectSpec>,
    overrides: &BTreeMap<String, NormalizationOverride>,
) {
    let mut code = String::from(
        r#"//! Case sensitivity rules per dialect.
//!
//! Generated from dialects.json and normalization_overrides.toml

use crate::Dialect;

/// Normalization strategy for identifier handling.
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

    fs::write(dir.join("case_sensitivity.rs"), code).expect("Failed to write case_sensitivity.rs");
}

fn generate_scoping_rules(dir: &Path, rules: &BTreeMap<String, ScopingRule>) {
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

    fs::write(dir.join("scoping_rules.rs"), code).expect("Failed to write scoping_rules.rs");
}

fn generate_function_rules(dir: &Path, behavior: &DialectBehavior) {
    let mut code = String::from(
        r#"//! Function argument handling rules per dialect.
//!
//! Generated from dialect_behavior.toml

use crate::Dialect;

/// Get argument indices to skip for a function in a specific dialect.
/// These are typically unit/part literals that shouldn't be treated as column references.
pub fn skip_args_for_function(dialect: Dialect, func_name: &str) -> &'static [usize] {
    let func_lower = func_name.to_lowercase();
    match func_lower.as_str() {
"#,
    );

    // Group by function name
    for (func_name, dialect_rules) in &behavior.date_functions {
        let func_lower = func_name.to_lowercase();

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
                    code.push_str(&format!("        \"{func_lower}\" => &[],\n"));
                } else {
                    let idx_str = default_indices
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    code.push_str(&format!("        \"{func_lower}\" => &[{idx_str}],\n"));
                }
            } else {
                code.push_str(&format!("        \"{func_lower}\" => &[],\n"));
            }
            continue;
        }

        // Generate match expression for functions with dialect-specific rules
        code.push_str(&format!("        \"{func_lower}\" => match dialect {{\n"));

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

    fs::write(dir.join("function_rules.rs"), code).expect("Failed to write function_rules.rs");
}

fn generate_functions(dir: &Path, functions: &BTreeMap<String, FunctionDef>) {
    let mut aggregates: BTreeSet<String> = BTreeSet::new();
    let mut windows: BTreeSet<String> = BTreeSet::new();
    let mut udtfs: BTreeSet<String> = BTreeSet::new();

    for (name, def) in functions {
        let name_lower = name.to_lowercase();
        for cat in &def.categories {
            match cat.as_str() {
                "aggregate" => {
                    aggregates.insert(name_lower.clone());
                }
                "window" => {
                    windows.insert(name_lower.clone());
                }
                "udtf" => {
                    udtfs.insert(name_lower.clone());
                }
                _ => {}
            }
        }
    }

    let mut code = String::from(
        r#"//! Function classification sets.
//!
//! Generated from functions.json

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

    // Generate helper functions
    code.push_str(
        r#"/// Check if a function is an aggregate function.
pub fn is_aggregate_function(name: &str) -> bool {
    AGGREGATE_FUNCTIONS.contains(name.to_lowercase().as_str())
}

/// Check if a function is a window function.
pub fn is_window_function(name: &str) -> bool {
    WINDOW_FUNCTIONS.contains(name.to_lowercase().as_str())
}

/// Check if a function is a table-generating function (UDTF).
pub fn is_udtf_function(name: &str) -> bool {
    UDTF_FUNCTIONS.contains(name.to_lowercase().as_str())
}
"#,
    );

    fs::write(dir.join("functions.rs"), code).expect("Failed to write functions.rs");
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
