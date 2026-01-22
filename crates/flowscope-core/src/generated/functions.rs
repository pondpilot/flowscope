//! Function classification sets.
//!
//! Generated from functions.json
//!
//! This module provides sets of SQL function names categorized by their behavior
//! (aggregate, window, table-generating). These classifications are used during
//! lineage analysis to determine how expressions should be analyzed.

use std::collections::HashSet;
use std::sync::LazyLock;

/// Aggregate functions (57 total).
pub static AGGREGATE_FUNCTIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    set.insert("agg_func");
    set.insert("ai_agg");
    set.insert("ai_summarize_agg");
    set.insert("any_value");
    set.insert("approx_distinct");
    set.insert("approx_quantile");
    set.insert("approx_quantiles");
    set.insert("approx_top_k");
    set.insert("approx_top_k_accumulate");
    set.insert("approx_top_k_combine");
    set.insert("approx_top_sum");
    set.insert("approximate_similarity");
    set.insert("arg_max");
    set.insert("arg_min");
    set.insert("array_agg");
    set.insert("array_concat_agg");
    set.insert("array_union_agg");
    set.insert("array_unique_agg");
    set.insert("avg");
    set.insert("bitmap_construct_agg");
    set.insert("bitmap_or_agg");
    set.insert("bitwise_and_agg");
    set.insert("bitwise_or_agg");
    set.insert("bitwise_xor_agg");
    set.insert("boolxor_agg");
    set.insert("combined_agg_func");
    set.insert("combined_parameterized_agg");
    set.insert("corr");
    set.insert("count");
    set.insert("count_if");
    set.insert("covar_pop");
    set.insert("covar_samp");
    set.insert("first");
    set.insert("group_concat");
    set.insert("grouping");
    set.insert("grouping_id");
    set.insert("hll");
    set.insert("json_object_agg");
    set.insert("jsonb_object_agg");
    set.insert("last");
    set.insert("logical_and");
    set.insert("logical_or");
    set.insert("max");
    set.insert("median");
    set.insert("min");
    set.insert("minhash");
    set.insert("minhash_combine");
    set.insert("object_agg");
    set.insert("parameterized_agg");
    set.insert("quantile");
    set.insert("skewness");
    set.insert("stddev");
    set.insert("stddev_pop");
    set.insert("stddev_samp");
    set.insert("sum");
    set.insert("variance");
    set.insert("variance_pop");
    set
});

/// Window functions (13 total).
pub static WINDOW_FUNCTIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    set.insert("cume_dist");
    set.insert("dense_rank");
    set.insert("first_value");
    set.insert("lag");
    set.insert("last_value");
    set.insert("lead");
    set.insert("nth_value");
    set.insert("ntile");
    set.insert("percent_rank");
    set.insert("percentile_cont");
    set.insert("percentile_disc");
    set.insert("rank");
    set.insert("row_number");
    set
});

/// Table-generating functions / UDTFs (5 total).
pub static UDTF_FUNCTIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    set.insert("explode");
    set.insert("explode_outer");
    set.insert("posexplode");
    set.insert("posexplode_outer");
    set.insert("unnest");
    set
});

/// Checks if a function is an aggregate function (e.g., SUM, COUNT, AVG).
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
        "and" => Some(ReturnTypeRule::Boolean),
        "any_value" => Some(ReturnTypeRule::MatchFirstArg),
        "avg" => Some(ReturnTypeRule::Numeric),
        "coalesce" => Some(ReturnTypeRule::MatchFirstArg),
        "concat" => Some(ReturnTypeRule::Text),
        "concat_ws" => Some(ReturnTypeRule::Text),
        "count" => Some(ReturnTypeRule::Integer),
        "current_date" => Some(ReturnTypeRule::Date),
        "current_timestamp" => Some(ReturnTypeRule::Timestamp),
        "date_trunc" => Some(ReturnTypeRule::MatchFirstArg),
        "dense_rank" => Some(ReturnTypeRule::Integer),
        "first_value" => Some(ReturnTypeRule::MatchFirstArg),
        "lag" => Some(ReturnTypeRule::MatchFirstArg),
        "last_value" => Some(ReturnTypeRule::MatchFirstArg),
        "lead" => Some(ReturnTypeRule::MatchFirstArg),
        "lower" => Some(ReturnTypeRule::Text),
        "max" => Some(ReturnTypeRule::MatchFirstArg),
        "min" => Some(ReturnTypeRule::MatchFirstArg),
        "now" => Some(ReturnTypeRule::Timestamp),
        "ntile" => Some(ReturnTypeRule::Integer),
        "or" => Some(ReturnTypeRule::Boolean),
        "rank" => Some(ReturnTypeRule::Integer),
        "replace" => Some(ReturnTypeRule::Text),
        "row_number" => Some(ReturnTypeRule::Integer),
        "substring" => Some(ReturnTypeRule::Text),
        "sum" => Some(ReturnTypeRule::Numeric),
        "trim" => Some(ReturnTypeRule::Text),
        "upper" => Some(ReturnTypeRule::Text),
        _ => None,
    }
}

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

/// Static function parameter definitions.
static PARAMS_ABS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ACOS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ACOSH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ADD_MONTHS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_AGG_FUNC: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_AI_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_AI_CLASSIFY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "categories", required: true },
    FunctionParam { name: "config", required: false },
];
static PARAMS_AI_SUMMARIZE_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_AND: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_ANY_VALUE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_APPLY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_APPROXIMATE_SIMILARITY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_APPROX_DISTINCT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "accuracy", required: false },
];
static PARAMS_APPROX_QUANTILE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "quantile", required: true },
    FunctionParam { name: "accuracy", required: false },
    FunctionParam { name: "weight", required: false },
    FunctionParam { name: "error_tolerance", required: false },
];
static PARAMS_APPROX_QUANTILES: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_APPROX_TOP_K: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
    FunctionParam { name: "counters", required: false },
];
static PARAMS_APPROX_TOP_K_ACCUMULATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_APPROX_TOP_K_COMBINE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_APPROX_TOP_SUM: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "count", required: true },
];
static PARAMS_ARG_MAX: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "count", required: false },
];
static PARAMS_ARG_MIN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "count", required: false },
];
static PARAMS_ARRAY: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_ARRAY_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ARRAY_ALL: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_ARRAY_ANY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_ARRAY_CONCAT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_ARRAY_CONCAT_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ARRAY_CONSTRUCT_COMPACT: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_ARRAY_CONTAINS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_ARRAY_CONTAINS_ALL: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_ARRAY_FILTER: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_ARRAY_FIRST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ARRAY_INTERSECT: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_ARRAY_TO_STRING: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "null", required: false },
];
static PARAMS_ARRAY_LAST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ARRAY_SIZE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_ARRAY_OVERLAPS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_ARRAY_REMOVE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_ARRAY_REVERSE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ARRAY_SLICE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "start", required: true },
    FunctionParam { name: "end", required: false },
    FunctionParam { name: "step", required: false },
];
static PARAMS_ARRAY_SORT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_ARRAY_SUM: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_ARRAY_UNION_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ARRAY_UNIQUE_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ASCII: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ASIN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ASINH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ATAN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_ATAN2: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_ATANH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_AVG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BASE64DECODE_BINARY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "alphabet", required: false },
];
static PARAMS_BASE64DECODE_STRING: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "alphabet", required: false },
];
static PARAMS_BASE64ENCODE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "max_line_length", required: false },
    FunctionParam { name: "alphabet", required: false },
];
static PARAMS_BITMAP_BIT_POSITION: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BITMAP_BUCKET_NUMBER: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BITMAP_CONSTRUCT_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BITMAP_COUNT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BITMAP_OR_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BITWISE_AND_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BITWISE_COUNT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BITWISE_OR_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BITWISE_XOR_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BIT_LENGTH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BOOLAND: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_LOGICAL_AND: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BOOLNOT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BOOLOR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_LOGICAL_OR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BOOLXOR_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_BYTE_LENGTH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_CASE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
    FunctionParam { name: "ifs", required: true },
    FunctionParam { name: "default", required: false },
];
static PARAMS_CAST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "to", required: true },
    FunctionParam { name: "format", required: false },
    FunctionParam { name: "safe", required: false },
    FunctionParam { name: "action", required: false },
    FunctionParam { name: "default", required: false },
];
static PARAMS_CAST_TO_STR_TYPE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "to", required: true },
];
static PARAMS_CBRT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_CEIL: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "decimals", required: false },
    FunctionParam { name: "to", required: false },
];
static PARAMS_CHR: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
    FunctionParam { name: "charset", required: false },
];
static PARAMS_LENGTH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "binary", required: false },
    FunctionParam { name: "encoding", required: false },
];
static PARAMS_COALESCE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
    FunctionParam { name: "is_nvl", required: false },
    FunctionParam { name: "is_null", required: false },
];
static PARAMS_CODE_POINTS_TO_BYTES: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_CODE_POINTS_TO_STRING: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_COLLATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_COLLATION: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_COLUMNS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "unpack", required: false },
];
static PARAMS_COMBINED_AGG_FUNC: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_COMBINED_PARAMETERIZED_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: true },
    FunctionParam { name: "params", required: true },
];
static PARAMS_COMPRESS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "method", required: false },
];
static PARAMS_CONCAT: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
    FunctionParam { name: "safe", required: false },
    FunctionParam { name: "coalesce", required: false },
];
static PARAMS_CONCAT_WS: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
    FunctionParam { name: "safe", required: false },
    FunctionParam { name: "coalesce", required: false },
];
static PARAMS_CONNECT_BY_ROOT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_CONTAINS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "json_scope", required: false },
];
static PARAMS_CONVERT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "style", required: false },
    FunctionParam { name: "safe", required: false },
];
static PARAMS_CONVERT_TIMEZONE: &[FunctionParam] = &[
    FunctionParam { name: "source_tz", required: false },
    FunctionParam { name: "target_tz", required: true },
    FunctionParam { name: "timestamp", required: true },
    FunctionParam { name: "options", required: false },
];
static PARAMS_CONVERT_TO_CHARSET: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "dest", required: true },
    FunctionParam { name: "source", required: false },
];
static PARAMS_CORR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_COS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_COSH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_COSINE_DISTANCE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_COT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_COTH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_COUNT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
    FunctionParam { name: "expressions", required: false },
    FunctionParam { name: "big_int", required: false },
];
static PARAMS_COUNT_IF: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_COVAR_POP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_COVAR_SAMP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_CSC: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_CSCH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_CUME_DIST: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_CURRENT_DATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
];
static PARAMS_CURRENT_DATETIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
];
static PARAMS_CURRENT_SCHEMA: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
];
static PARAMS_CURRENT_TIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
];
static PARAMS_CURRENT_TIMESTAMP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
    FunctionParam { name: "sysdate", required: false },
];
static PARAMS_CURRENT_USER: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
];
static PARAMS_DATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
    FunctionParam { name: "zone", required: false },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_DATE_DIFF: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
    FunctionParam { name: "zone", required: false },
    FunctionParam { name: "big_int", required: false },
];
static PARAMS_DATE_FROM_PARTS: &[FunctionParam] = &[
    FunctionParam { name: "year", required: true },
    FunctionParam { name: "month", required: false },
    FunctionParam { name: "day", required: false },
];
static PARAMS_DATETIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_DATETIME_ADD: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_DATETIME_DIFF: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_DATETIME_SUB: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_DATETIME_TRUNC: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "unit", required: true },
    FunctionParam { name: "zone", required: false },
];
static PARAMS_DATE_ADD: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_DATE_BIN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
    FunctionParam { name: "zone", required: false },
    FunctionParam { name: "origin", required: false },
];
static PARAMS_DATE_FROM_UNIX_DATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_DATE_STR_TO_DATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_DATE_SUB: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_DATE_TO_DATE_STR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_DATE_TO_DI: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_DATE_TRUNC: &[FunctionParam] = &[
    FunctionParam { name: "unit", required: true },
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "zone", required: false },
];
static PARAMS_DAY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_DAY_OF_MONTH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_DAY_OF_WEEK: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_DAY_OF_WEEK_ISO: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_DAY_OF_YEAR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_DECODE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "charset", required: true },
    FunctionParam { name: "replace", required: false },
];
static PARAMS_DECODE_CASE: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_DECOMPRESS_BINARY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "method", required: true },
];
static PARAMS_DECOMPRESS_STRING: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "method", required: true },
];
static PARAMS_DEGREES: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_DENSE_RANK: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_DI_TO_DATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ENCODE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "charset", required: true },
];
static PARAMS_ENDS_WITH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_EQUAL_NULL: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_EUCLIDEAN_DISTANCE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_EXISTS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_EXP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_EXPLODE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_EXPLODE_OUTER: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_EXPLODING_GENERATE_SERIES: &[FunctionParam] = &[
    FunctionParam { name: "start", required: true },
    FunctionParam { name: "end", required: true },
    FunctionParam { name: "step", required: false },
    FunctionParam { name: "is_end_exclusive", required: false },
];
static PARAMS_EXTRACT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_FACTORIAL: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_FARM_FINGERPRINT: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_FEATURES_AT_TIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "time", required: false },
    FunctionParam { name: "num_rows", required: false },
    FunctionParam { name: "ignore_feature_nulls", required: false },
];
static PARAMS_FIRST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_FIRST_VALUE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_FLATTEN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_FLOAT64: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_FLOOR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "decimals", required: false },
    FunctionParam { name: "to", required: false },
];
static PARAMS_FORMAT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_FROM_BASE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_FROM_BASE32: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_FROM_BASE64: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_FROM_ISO8601TIMESTAMP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_GAP_FILL: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "ts_column", required: true },
    FunctionParam { name: "bucket_width", required: true },
    FunctionParam { name: "partitioning_columns", required: false },
    FunctionParam { name: "value_columns", required: false },
    FunctionParam { name: "origin", required: false },
    FunctionParam { name: "ignore_nulls", required: false },
];
static PARAMS_GENERATE_DATE_ARRAY: &[FunctionParam] = &[
    FunctionParam { name: "start", required: true },
    FunctionParam { name: "end", required: true },
    FunctionParam { name: "step", required: false },
];
static PARAMS_GENERATE_EMBEDDING: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "params_struct", required: false },
    FunctionParam { name: "is_text", required: false },
];
static PARAMS_GENERATE_SERIES: &[FunctionParam] = &[
    FunctionParam { name: "start", required: true },
    FunctionParam { name: "end", required: true },
    FunctionParam { name: "step", required: false },
    FunctionParam { name: "is_end_exclusive", required: false },
];
static PARAMS_GENERATE_TIMESTAMP_ARRAY: &[FunctionParam] = &[
    FunctionParam { name: "start", required: true },
    FunctionParam { name: "end", required: true },
    FunctionParam { name: "step", required: true },
];
static PARAMS_UUID: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
    FunctionParam { name: "name", required: false },
    FunctionParam { name: "is_string", required: false },
];
static PARAMS_GETBIT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_GET_EXTRACT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_GREATEST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
    FunctionParam { name: "null_if_any_null", required: false },
];
static PARAMS_GREATEST_IGNORE_NULLS: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_GROUPING: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_GROUPING_ID: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_GROUP_CONCAT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "separator", required: false },
    FunctionParam { name: "on_overflow", required: false },
];
static PARAMS_HEX: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_HEX_DECODE_STRING: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_HEX_ENCODE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "case", required: false },
];
static PARAMS_HLL: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_HOUR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_IF: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "true", required: true },
    FunctionParam { name: "false", required: false },
];
static PARAMS_INITCAP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_INLINE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_STUFF: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "start", required: true },
    FunctionParam { name: "length", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_INT64: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_IS_INF: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_IS_NAN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_IS_ASCII: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_IS_NULL_VALUE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_JAROWINKLER_SIMILARITY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_JSONB_CONTAINS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_JSONB_EXISTS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "path", required: true },
];
static PARAMS_JSONB_EXTRACT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_JSONB_EXTRACT_SCALAR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "json_type", required: false },
];
static PARAMS_JSON_ARRAY_APPEND: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_JSON_ARRAY_CONTAINS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "json_type", required: false },
];
static PARAMS_JSON_ARRAY_INSERT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_JSON_EXTRACT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "only_json_types", required: false },
    FunctionParam { name: "expressions", required: false },
    FunctionParam { name: "variant_extract", required: false },
    FunctionParam { name: "json_query", required: false },
    FunctionParam { name: "option", required: false },
    FunctionParam { name: "quote", required: false },
    FunctionParam { name: "on_condition", required: false },
    FunctionParam { name: "requires_json", required: false },
];
static PARAMS_JSON_EXTRACT_ARRAY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_JSON_EXTRACT_SCALAR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "only_json_types", required: false },
    FunctionParam { name: "expressions", required: false },
    FunctionParam { name: "json_type", required: false },
    FunctionParam { name: "scalar_only", required: false },
];
static PARAMS_JSON_FORMAT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
    FunctionParam { name: "options", required: false },
    FunctionParam { name: "is_json", required: false },
    FunctionParam { name: "to_json", required: false },
];
static PARAMS_PARSE_JSON: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
    FunctionParam { name: "safe", required: false },
];
static PARAMS_JSON_REMOVE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_JSON_SET: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_JSON_STRIP_NULLS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
    FunctionParam { name: "include_arrays", required: false },
    FunctionParam { name: "remove_empty", required: false },
];
static PARAMS_JSON_TYPE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_JUSTIFY_DAYS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_JUSTIFY_HOURS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_JUSTIFY_INTERVAL: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_JSON_ARRAY: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: false },
    FunctionParam { name: "null_handling", required: false },
    FunctionParam { name: "return_type", required: false },
    FunctionParam { name: "strict", required: false },
];
static PARAMS_JSON_ARRAY_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "order", required: false },
    FunctionParam { name: "null_handling", required: false },
    FunctionParam { name: "return_type", required: false },
    FunctionParam { name: "strict", required: false },
];
static PARAMS_JSON_BOOL: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_JSONB_CONTAINS_ALL_TOP_KEYS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_JSONB_CONTAINS_ANY_TOP_KEYS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_JSONB_DELETE_AT_PATH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_JSONB_OBJECT_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_JSON_CAST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "to", required: true },
    FunctionParam { name: "format", required: false },
    FunctionParam { name: "safe", required: false },
    FunctionParam { name: "action", required: false },
    FunctionParam { name: "default", required: false },
];
static PARAMS_JSON_EXISTS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "path", required: true },
    FunctionParam { name: "passing", required: false },
    FunctionParam { name: "on_condition", required: false },
];
static PARAMS_JSON_KEYS_AT_DEPTH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
    FunctionParam { name: "mode", required: false },
];
static PARAMS_JSON_OBJECT: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: false },
    FunctionParam { name: "null_handling", required: false },
    FunctionParam { name: "unique_keys", required: false },
    FunctionParam { name: "return_type", required: false },
    FunctionParam { name: "encoding", required: false },
];
static PARAMS_JSON_OBJECT_AGG: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: false },
    FunctionParam { name: "null_handling", required: false },
    FunctionParam { name: "unique_keys", required: false },
    FunctionParam { name: "return_type", required: false },
    FunctionParam { name: "encoding", required: false },
];
static PARAMS_JSON_TABLE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "schema", required: true },
    FunctionParam { name: "path", required: false },
    FunctionParam { name: "error_handling", required: false },
    FunctionParam { name: "empty_handling", required: false },
];
static PARAMS_JSON_VALUE_ARRAY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_LAG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "offset", required: false },
    FunctionParam { name: "default", required: false },
];
static PARAMS_LAST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_LAST_DAY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_LAST_VALUE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_LAX_BOOL: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_LAX_FLOAT64: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_LAX_INT64: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_LAX_STRING: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_LOWER: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_LEAD: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "offset", required: false },
    FunctionParam { name: "default", required: false },
];
static PARAMS_LEAST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
    FunctionParam { name: "null_if_any_null", required: false },
];
static PARAMS_LEAST_IGNORE_NULLS: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_LEFT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_LEVENSHTEIN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
    FunctionParam { name: "ins_cost", required: false },
    FunctionParam { name: "del_cost", required: false },
    FunctionParam { name: "sub_cost", required: false },
    FunctionParam { name: "max_dist", required: false },
];
static PARAMS_LIST: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_LN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_LOG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_LOWER_HEX: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_MAKE_INTERVAL: &[FunctionParam] = &[
    FunctionParam { name: "year", required: false },
    FunctionParam { name: "month", required: false },
    FunctionParam { name: "day", required: false },
    FunctionParam { name: "hour", required: false },
    FunctionParam { name: "minute", required: false },
    FunctionParam { name: "second", required: false },
];
static PARAMS_MAP: &[FunctionParam] = &[
    FunctionParam { name: "keys", required: false },
    FunctionParam { name: "values", required: false },
];
static PARAMS_MAP_FROM_ENTRIES: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_MATCH_AGAINST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: true },
    FunctionParam { name: "modifier", required: false },
];
static PARAMS_MAX: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_MD5: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_MD5DIGEST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_MEDIAN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_MIN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_MINHASH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_MINHASH_COMBINE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_MINUTE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_MONTH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_MONTHNAME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_MONTHS_BETWEEN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "roundoff", required: false },
];
static PARAMS_MD5NUMBER_LOWER64: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_MD5NUMBER_UPPER64: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_ML_FORECAST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
    FunctionParam { name: "params_struct", required: false },
];
static PARAMS_ML_TRANSLATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "params_struct", required: true },
];
static PARAMS_NEXT_DAY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_NEXT_VALUE_FOR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "order", required: false },
];
static PARAMS_NORMALIZE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "form", required: false },
    FunctionParam { name: "is_casefold", required: false },
];
static PARAMS_NTH_VALUE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "offset", required: true },
];
static PARAMS_NTILE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
];
static PARAMS_NULLIF: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_NUMBER_TO_STR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "format", required: true },
    FunctionParam { name: "culture", required: false },
];
static PARAMS_NVL2: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "true", required: true },
    FunctionParam { name: "false", required: false },
];
static PARAMS_OBJECT_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_OBJECT_INSERT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "key", required: true },
    FunctionParam { name: "value", required: true },
    FunctionParam { name: "update_flag", required: false },
];
static PARAMS_OPEN_JSON: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "path", required: false },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_OR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_OVERLAY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "from_", required: true },
    FunctionParam { name: "for_", required: false },
];
static PARAMS_PAD: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "fill_pattern", required: false },
    FunctionParam { name: "is_left", required: true },
];
static PARAMS_PARAMETERIZED_AGG: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: true },
    FunctionParam { name: "params", required: true },
];
static PARAMS_PARSE_BIGNUMERIC: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_PARSE_DATETIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "format", required: false },
    FunctionParam { name: "zone", required: false },
];
static PARAMS_PARSE_IP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "type", required: true },
    FunctionParam { name: "permissive", required: false },
];
static PARAMS_PARSE_NUMERIC: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_PARSE_TIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "format", required: true },
];
static PARAMS_PARSE_URL: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "part_to_extract", required: false },
    FunctionParam { name: "key", required: false },
    FunctionParam { name: "permissive", required: false },
];
static PARAMS_PERCENTILE_CONT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_PERCENTILE_DISC: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_PERCENT_RANK: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_POSEXPLODE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_POSEXPLODE_OUTER: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_POW: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_PREDICT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "params_struct", required: false },
];
static PARAMS_PREVIOUS_DAY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_QUANTILE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "quantile", required: true },
];
static PARAMS_QUARTER: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_RADIANS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_RAND: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
    FunctionParam { name: "lower", required: false },
    FunctionParam { name: "upper", required: false },
];
static PARAMS_RANDN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
];
static PARAMS_RANGE_BUCKET: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_RANGE_N: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: true },
    FunctionParam { name: "each", required: false },
];
static PARAMS_RANK: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_READ_CSV: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_READ_PARQUET: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
];
static PARAMS_REDUCE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "initial", required: true },
    FunctionParam { name: "merge", required: true },
    FunctionParam { name: "finish", required: false },
];
static PARAMS_REGEXP_COUNT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "position", required: false },
    FunctionParam { name: "parameters", required: false },
];
static PARAMS_REGEXP_EXTRACT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "position", required: false },
    FunctionParam { name: "occurrence", required: false },
    FunctionParam { name: "parameters", required: false },
    FunctionParam { name: "group", required: false },
];
static PARAMS_REGEXP_EXTRACT_ALL: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "group", required: false },
    FunctionParam { name: "parameters", required: false },
    FunctionParam { name: "position", required: false },
    FunctionParam { name: "occurrence", required: false },
];
static PARAMS_REGEXP_FULL_MATCH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "options", required: false },
];
static PARAMS_REGEXP_INSTR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "position", required: false },
    FunctionParam { name: "occurrence", required: false },
    FunctionParam { name: "option", required: false },
    FunctionParam { name: "parameters", required: false },
    FunctionParam { name: "group", required: false },
];
static PARAMS_REGEXP_I_LIKE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "flag", required: false },
];
static PARAMS_REGEXP_LIKE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "flag", required: false },
];
static PARAMS_REGEXP_REPLACE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "replacement", required: false },
    FunctionParam { name: "position", required: false },
    FunctionParam { name: "occurrence", required: false },
    FunctionParam { name: "modifiers", required: false },
    FunctionParam { name: "single_replace", required: false },
];
static PARAMS_REGEXP_SPLIT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "limit", required: false },
];
static PARAMS_REGR_AVGX: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_REGR_AVGY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_REGR_VALX: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_REGR_VALY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_REPEAT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "times", required: true },
];
static PARAMS_REPLACE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "replacement", required: false },
];
static PARAMS_REVERSE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_RIGHT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_ROUND: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "decimals", required: false },
    FunctionParam { name: "truncate", required: false },
];
static PARAMS_ROW_NUMBER: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
];
static PARAMS_RTRIMMED_LENGTH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SAFE_ADD: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_SAFE_CONVERT_BYTES_TO_STRING: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SAFE_DIVIDE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_SAFE_MULTIPLY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_SAFE_NEGATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SAFE_SUBTRACT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_SEARCH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "json_scope", required: false },
    FunctionParam { name: "analyzer", required: false },
    FunctionParam { name: "analyzer_options", required: false },
    FunctionParam { name: "search_mode", required: false },
];
static PARAMS_SEC: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SECH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SECOND: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SHA: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SHA2: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "length", required: false },
];
static PARAMS_SIGN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SIN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SINH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SKEWNESS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SORT_ARRAY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "asc", required: false },
    FunctionParam { name: "nulls_first", required: false },
];
static PARAMS_SOUNDEX: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SOUNDEX_P123: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SPACE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SPLIT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "limit", required: false },
];
static PARAMS_STRING_TO_ARRAY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
    FunctionParam { name: "null", required: false },
];
static PARAMS_SPLIT_PART: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "delimiter", required: false },
    FunctionParam { name: "part_index", required: false },
];
static PARAMS_SQRT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_STANDARD_HASH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_STARTS_WITH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_STAR_MAP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_STDDEV: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_STDDEV_POP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_STDDEV_SAMP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_STRING: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "zone", required: false },
];
static PARAMS_STRUCT: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_STRUCT_EXTRACT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_STR_POSITION: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "substr", required: true },
    FunctionParam { name: "position", required: false },
    FunctionParam { name: "occurrence", required: false },
];
static PARAMS_STR_TO_DATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "format", required: false },
    FunctionParam { name: "safe", required: false },
];
static PARAMS_STR_TO_MAP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "pair_delim", required: false },
    FunctionParam { name: "key_value_delim", required: false },
    FunctionParam { name: "duplicate_resolution_callback", required: false },
];
static PARAMS_STR_TO_TIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "format", required: true },
    FunctionParam { name: "zone", required: false },
    FunctionParam { name: "safe", required: false },
];
static PARAMS_STR_TO_UNIX: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
    FunctionParam { name: "format", required: false },
];
static PARAMS_ST_DISTANCE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "use_spheroid", required: false },
];
static PARAMS_ST_POINT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "null", required: false },
];
static PARAMS_SUBSTRING: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "start", required: false },
    FunctionParam { name: "length", required: false },
];
static PARAMS_SUBSTRING_INDEX: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "delimiter", required: true },
    FunctionParam { name: "count", required: true },
];
static PARAMS_SUM: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SHA1DIGEST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_SHA2DIGEST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "length", required: false },
];
static PARAMS_TAN: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TANH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
    FunctionParam { name: "zone", required: false },
];
static PARAMS_TIME_FROM_PARTS: &[FunctionParam] = &[
    FunctionParam { name: "hour", required: true },
    FunctionParam { name: "min", required: true },
    FunctionParam { name: "sec", required: true },
    FunctionParam { name: "nano", required: false },
    FunctionParam { name: "fractions", required: false },
    FunctionParam { name: "precision", required: false },
];
static PARAMS_TIMESTAMP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
    FunctionParam { name: "zone", required: false },
    FunctionParam { name: "with_tz", required: false },
];
static PARAMS_TIMESTAMP_DIFF: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_TIMESTAMP_FROM_PARTS: &[FunctionParam] = &[
    FunctionParam { name: "year", required: false },
    FunctionParam { name: "month", required: false },
    FunctionParam { name: "day", required: false },
    FunctionParam { name: "hour", required: false },
    FunctionParam { name: "min", required: false },
    FunctionParam { name: "sec", required: false },
    FunctionParam { name: "nano", required: false },
    FunctionParam { name: "zone", required: false },
    FunctionParam { name: "milli", required: false },
    FunctionParam { name: "this", required: false },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_TIMESTAMP_LTZ_FROM_PARTS: &[FunctionParam] = &[
    FunctionParam { name: "year", required: false },
    FunctionParam { name: "month", required: false },
    FunctionParam { name: "day", required: false },
    FunctionParam { name: "hour", required: false },
    FunctionParam { name: "min", required: false },
    FunctionParam { name: "sec", required: false },
    FunctionParam { name: "nano", required: false },
];
static PARAMS_TIMESTAMP_TZ_FROM_PARTS: &[FunctionParam] = &[
    FunctionParam { name: "year", required: false },
    FunctionParam { name: "month", required: false },
    FunctionParam { name: "day", required: false },
    FunctionParam { name: "hour", required: false },
    FunctionParam { name: "min", required: false },
    FunctionParam { name: "sec", required: false },
    FunctionParam { name: "nano", required: false },
    FunctionParam { name: "zone", required: false },
];
static PARAMS_TIMESTAMP_ADD: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_TIMESTAMP_SUB: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_TIMESTAMP_TRUNC: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "unit", required: true },
    FunctionParam { name: "zone", required: false },
];
static PARAMS_TIME_ADD: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_TIME_DIFF: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_TIME_SLICE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: true },
    FunctionParam { name: "kind", required: false },
];
static PARAMS_TIME_STR_TO_DATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TIME_STR_TO_TIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "zone", required: false },
];
static PARAMS_TIME_STR_TO_UNIX: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TIME_SUB: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_TIME_TO_STR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "format", required: true },
    FunctionParam { name: "culture", required: false },
    FunctionParam { name: "zone", required: false },
];
static PARAMS_TIME_TO_TIME_STR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TIME_TO_UNIX: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TIME_TRUNC: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "unit", required: true },
    FunctionParam { name: "zone", required: false },
];
static PARAMS_TO_ARRAY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TO_BASE32: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TO_BASE64: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TO_CHAR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "format", required: false },
    FunctionParam { name: "nlsparam", required: false },
    FunctionParam { name: "is_numeric", required: false },
];
static PARAMS_TO_CODE_POINTS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TO_DAYS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TO_DOUBLE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "format", required: false },
];
static PARAMS_TO_MAP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TO_NUMBER: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "format", required: false },
    FunctionParam { name: "nlsparam", required: false },
    FunctionParam { name: "precision", required: false },
    FunctionParam { name: "scale", required: false },
];
static PARAMS_TRANSFORM: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
];
static PARAMS_TRANSLATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "from_", required: true },
    FunctionParam { name: "to", required: true },
];
static PARAMS_TRIM: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
    FunctionParam { name: "position", required: false },
    FunctionParam { name: "collation", required: false },
];
static PARAMS_TRY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TRY_BASE64DECODE_BINARY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "alphabet", required: false },
];
static PARAMS_TRY_BASE64DECODE_STRING: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "alphabet", required: false },
];
static PARAMS_TRY_CAST: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "to", required: true },
    FunctionParam { name: "format", required: false },
    FunctionParam { name: "safe", required: false },
    FunctionParam { name: "action", required: false },
    FunctionParam { name: "default", required: false },
    FunctionParam { name: "requires_string", required: false },
];
static PARAMS_TRY_HEX_DECODE_BINARY: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TRY_HEX_DECODE_STRING: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TS_OR_DI_TO_DI: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TS_OR_DS_ADD: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
    FunctionParam { name: "return_type", required: false },
];
static PARAMS_TS_OR_DS_DIFF: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: true },
    FunctionParam { name: "unit", required: false },
];
static PARAMS_TS_OR_DS_TO_DATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "format", required: false },
    FunctionParam { name: "safe", required: false },
];
static PARAMS_TS_OR_DS_TO_DATETIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TS_OR_DS_TO_DATE_STR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TS_OR_DS_TO_TIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "format", required: false },
    FunctionParam { name: "safe", required: false },
];
static PARAMS_TS_OR_DS_TO_TIMESTAMP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_TYPEOF: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_UPPER: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_UNHEX: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expression", required: false },
];
static PARAMS_UNICODE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_UNIX_DATE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_UNIX_MICROS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_UNIX_MILLIS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_UNIX_SECONDS: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_UNIX_TO_STR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "format", required: false },
];
static PARAMS_UNIX_TO_TIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "scale", required: false },
    FunctionParam { name: "zone", required: false },
    FunctionParam { name: "hours", required: false },
    FunctionParam { name: "minutes", required: false },
    FunctionParam { name: "format", required: false },
];
static PARAMS_UNIX_TO_TIME_STR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_UNNEST: &[FunctionParam] = &[
    FunctionParam { name: "expressions", required: true },
    FunctionParam { name: "alias", required: false },
    FunctionParam { name: "offset", required: false },
    FunctionParam { name: "explode_array", required: false },
];
static PARAMS_UTC_TIME: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
];
static PARAMS_UTC_TIMESTAMP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
];
static PARAMS_VARIANCE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_VARIANCE_POP: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_VAR_MAP: &[FunctionParam] = &[
    FunctionParam { name: "keys", required: true },
    FunctionParam { name: "values", required: true },
];
static PARAMS_VECTOR_SEARCH: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "column_to_search", required: true },
    FunctionParam { name: "query_table", required: true },
    FunctionParam { name: "query_column_to_search", required: false },
    FunctionParam { name: "top_k", required: false },
    FunctionParam { name: "distance_type", required: false },
    FunctionParam { name: "options", required: false },
];
static PARAMS_WEEK: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "mode", required: false },
];
static PARAMS_WEEK_OF_YEAR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_WIDTH_BUCKET: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "min_value", required: true },
    FunctionParam { name: "max_value", required: true },
    FunctionParam { name: "num_buckets", required: true },
];
static PARAMS_XML_ELEMENT: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_XOR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: false },
    FunctionParam { name: "expression", required: false },
    FunctionParam { name: "expressions", required: false },
];
static PARAMS_XML_TABLE: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
    FunctionParam { name: "namespaces", required: false },
    FunctionParam { name: "passing", required: false },
    FunctionParam { name: "columns", required: false },
    FunctionParam { name: "by_ref", required: false },
];
static PARAMS_YEAR: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_YEAR_OF_WEEK: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];
static PARAMS_YEAR_OF_WEEK_ISO: &[FunctionParam] = &[
    FunctionParam { name: "this", required: true },
];

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
        "abs" => Some(FunctionSignature {
            name: "abs",
            display_name: "ABS",
            params: PARAMS_ABS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "acos" => Some(FunctionSignature {
            name: "acos",
            display_name: "ACOS",
            params: PARAMS_ACOS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "acosh" => Some(FunctionSignature {
            name: "acosh",
            display_name: "ACOSH",
            params: PARAMS_ACOSH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "add_months" => Some(FunctionSignature {
            name: "add_months",
            display_name: "ADD_MONTHS",
            params: PARAMS_ADD_MONTHS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "agg_func" => Some(FunctionSignature {
            name: "agg_func",
            display_name: "AGG_FUNC",
            params: PARAMS_AGG_FUNC,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "ai_agg" => Some(FunctionSignature {
            name: "ai_agg",
            display_name: "AI_AGG",
            params: PARAMS_AI_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "ai_classify" => Some(FunctionSignature {
            name: "ai_classify",
            display_name: "AI_CLASSIFY",
            params: PARAMS_AI_CLASSIFY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ai_summarize_agg" => Some(FunctionSignature {
            name: "ai_summarize_agg",
            display_name: "AI_SUMMARIZE_AGG",
            params: PARAMS_AI_SUMMARIZE_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "and" => Some(FunctionSignature {
            name: "and",
            display_name: "AND",
            params: PARAMS_AND,
            return_type: Some(ReturnTypeRule::Boolean),
            category: FunctionCategory::Scalar,
        }),
        "any_value" => Some(FunctionSignature {
            name: "any_value",
            display_name: "ANY_VALUE",
            params: PARAMS_ANY_VALUE,
            return_type: Some(ReturnTypeRule::MatchFirstArg),
            category: FunctionCategory::Aggregate,
        }),
        "apply" => Some(FunctionSignature {
            name: "apply",
            display_name: "APPLY",
            params: PARAMS_APPLY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "approximate_similarity" => Some(FunctionSignature {
            name: "approximate_similarity",
            display_name: "APPROXIMATE_SIMILARITY",
            params: PARAMS_APPROXIMATE_SIMILARITY,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "approx_distinct" => Some(FunctionSignature {
            name: "approx_distinct",
            display_name: "APPROX_DISTINCT",
            params: PARAMS_APPROX_DISTINCT,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "approx_quantile" => Some(FunctionSignature {
            name: "approx_quantile",
            display_name: "APPROX_QUANTILE",
            params: PARAMS_APPROX_QUANTILE,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "approx_quantiles" => Some(FunctionSignature {
            name: "approx_quantiles",
            display_name: "APPROX_QUANTILES",
            params: PARAMS_APPROX_QUANTILES,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "approx_top_k" => Some(FunctionSignature {
            name: "approx_top_k",
            display_name: "APPROX_TOP_K",
            params: PARAMS_APPROX_TOP_K,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "approx_top_k_accumulate" => Some(FunctionSignature {
            name: "approx_top_k_accumulate",
            display_name: "APPROX_TOP_K_ACCUMULATE",
            params: PARAMS_APPROX_TOP_K_ACCUMULATE,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "approx_top_k_combine" => Some(FunctionSignature {
            name: "approx_top_k_combine",
            display_name: "APPROX_TOP_K_COMBINE",
            params: PARAMS_APPROX_TOP_K_COMBINE,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "approx_top_sum" => Some(FunctionSignature {
            name: "approx_top_sum",
            display_name: "APPROX_TOP_SUM",
            params: PARAMS_APPROX_TOP_SUM,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "arg_max" => Some(FunctionSignature {
            name: "arg_max",
            display_name: "ARG_MAX",
            params: PARAMS_ARG_MAX,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "arg_min" => Some(FunctionSignature {
            name: "arg_min",
            display_name: "ARG_MIN",
            params: PARAMS_ARG_MIN,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "array" => Some(FunctionSignature {
            name: "array",
            display_name: "ARRAY",
            params: PARAMS_ARRAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_agg" => Some(FunctionSignature {
            name: "array_agg",
            display_name: "ARRAY_AGG",
            params: PARAMS_ARRAY_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "array_all" => Some(FunctionSignature {
            name: "array_all",
            display_name: "ARRAY_ALL",
            params: PARAMS_ARRAY_ALL,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_any" => Some(FunctionSignature {
            name: "array_any",
            display_name: "ARRAY_ANY",
            params: PARAMS_ARRAY_ANY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_concat" => Some(FunctionSignature {
            name: "array_concat",
            display_name: "ARRAY_CONCAT",
            params: PARAMS_ARRAY_CONCAT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_concat_agg" => Some(FunctionSignature {
            name: "array_concat_agg",
            display_name: "ARRAY_CONCAT_AGG",
            params: PARAMS_ARRAY_CONCAT_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "array_construct_compact" => Some(FunctionSignature {
            name: "array_construct_compact",
            display_name: "ARRAY_CONSTRUCT_COMPACT",
            params: PARAMS_ARRAY_CONSTRUCT_COMPACT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_contains" => Some(FunctionSignature {
            name: "array_contains",
            display_name: "ARRAY_CONTAINS",
            params: PARAMS_ARRAY_CONTAINS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_contains_all" => Some(FunctionSignature {
            name: "array_contains_all",
            display_name: "ARRAY_CONTAINS_ALL",
            params: PARAMS_ARRAY_CONTAINS_ALL,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_filter" => Some(FunctionSignature {
            name: "array_filter",
            display_name: "ARRAY_FILTER",
            params: PARAMS_ARRAY_FILTER,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_first" => Some(FunctionSignature {
            name: "array_first",
            display_name: "ARRAY_FIRST",
            params: PARAMS_ARRAY_FIRST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_intersect" => Some(FunctionSignature {
            name: "array_intersect",
            display_name: "ARRAY_INTERSECT",
            params: PARAMS_ARRAY_INTERSECT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_to_string" => Some(FunctionSignature {
            name: "array_to_string",
            display_name: "ARRAY_TO_STRING",
            params: PARAMS_ARRAY_TO_STRING,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_last" => Some(FunctionSignature {
            name: "array_last",
            display_name: "ARRAY_LAST",
            params: PARAMS_ARRAY_LAST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_size" => Some(FunctionSignature {
            name: "array_size",
            display_name: "ARRAY_SIZE",
            params: PARAMS_ARRAY_SIZE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_overlaps" => Some(FunctionSignature {
            name: "array_overlaps",
            display_name: "ARRAY_OVERLAPS",
            params: PARAMS_ARRAY_OVERLAPS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_remove" => Some(FunctionSignature {
            name: "array_remove",
            display_name: "ARRAY_REMOVE",
            params: PARAMS_ARRAY_REMOVE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_reverse" => Some(FunctionSignature {
            name: "array_reverse",
            display_name: "ARRAY_REVERSE",
            params: PARAMS_ARRAY_REVERSE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_slice" => Some(FunctionSignature {
            name: "array_slice",
            display_name: "ARRAY_SLICE",
            params: PARAMS_ARRAY_SLICE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_sort" => Some(FunctionSignature {
            name: "array_sort",
            display_name: "ARRAY_SORT",
            params: PARAMS_ARRAY_SORT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_sum" => Some(FunctionSignature {
            name: "array_sum",
            display_name: "ARRAY_SUM",
            params: PARAMS_ARRAY_SUM,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "array_union_agg" => Some(FunctionSignature {
            name: "array_union_agg",
            display_name: "ARRAY_UNION_AGG",
            params: PARAMS_ARRAY_UNION_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "array_unique_agg" => Some(FunctionSignature {
            name: "array_unique_agg",
            display_name: "ARRAY_UNIQUE_AGG",
            params: PARAMS_ARRAY_UNIQUE_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "ascii" => Some(FunctionSignature {
            name: "ascii",
            display_name: "ASCII",
            params: PARAMS_ASCII,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "asin" => Some(FunctionSignature {
            name: "asin",
            display_name: "ASIN",
            params: PARAMS_ASIN,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "asinh" => Some(FunctionSignature {
            name: "asinh",
            display_name: "ASINH",
            params: PARAMS_ASINH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "atan" => Some(FunctionSignature {
            name: "atan",
            display_name: "ATAN",
            params: PARAMS_ATAN,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "atan2" => Some(FunctionSignature {
            name: "atan2",
            display_name: "ATAN2",
            params: PARAMS_ATAN2,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "atanh" => Some(FunctionSignature {
            name: "atanh",
            display_name: "ATANH",
            params: PARAMS_ATANH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "avg" => Some(FunctionSignature {
            name: "avg",
            display_name: "AVG",
            params: PARAMS_AVG,
            return_type: Some(ReturnTypeRule::Numeric),
            category: FunctionCategory::Aggregate,
        }),
        "base64decode_binary" => Some(FunctionSignature {
            name: "base64decode_binary",
            display_name: "BASE64DECODE_BINARY",
            params: PARAMS_BASE64DECODE_BINARY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "base64decode_string" => Some(FunctionSignature {
            name: "base64decode_string",
            display_name: "BASE64DECODE_STRING",
            params: PARAMS_BASE64DECODE_STRING,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "base64encode" => Some(FunctionSignature {
            name: "base64encode",
            display_name: "BASE64ENCODE",
            params: PARAMS_BASE64ENCODE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "bitmap_bit_position" => Some(FunctionSignature {
            name: "bitmap_bit_position",
            display_name: "BITMAP_BIT_POSITION",
            params: PARAMS_BITMAP_BIT_POSITION,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "bitmap_bucket_number" => Some(FunctionSignature {
            name: "bitmap_bucket_number",
            display_name: "BITMAP_BUCKET_NUMBER",
            params: PARAMS_BITMAP_BUCKET_NUMBER,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "bitmap_construct_agg" => Some(FunctionSignature {
            name: "bitmap_construct_agg",
            display_name: "BITMAP_CONSTRUCT_AGG",
            params: PARAMS_BITMAP_CONSTRUCT_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "bitmap_count" => Some(FunctionSignature {
            name: "bitmap_count",
            display_name: "BITMAP_COUNT",
            params: PARAMS_BITMAP_COUNT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "bitmap_or_agg" => Some(FunctionSignature {
            name: "bitmap_or_agg",
            display_name: "BITMAP_OR_AGG",
            params: PARAMS_BITMAP_OR_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "bitwise_and_agg" => Some(FunctionSignature {
            name: "bitwise_and_agg",
            display_name: "BITWISE_AND_AGG",
            params: PARAMS_BITWISE_AND_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "bitwise_count" => Some(FunctionSignature {
            name: "bitwise_count",
            display_name: "BITWISE_COUNT",
            params: PARAMS_BITWISE_COUNT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "bitwise_or_agg" => Some(FunctionSignature {
            name: "bitwise_or_agg",
            display_name: "BITWISE_OR_AGG",
            params: PARAMS_BITWISE_OR_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "bitwise_xor_agg" => Some(FunctionSignature {
            name: "bitwise_xor_agg",
            display_name: "BITWISE_XOR_AGG",
            params: PARAMS_BITWISE_XOR_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "bit_length" => Some(FunctionSignature {
            name: "bit_length",
            display_name: "BIT_LENGTH",
            params: PARAMS_BIT_LENGTH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "booland" => Some(FunctionSignature {
            name: "booland",
            display_name: "BOOLAND",
            params: PARAMS_BOOLAND,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "logical_and" => Some(FunctionSignature {
            name: "logical_and",
            display_name: "LOGICAL_AND",
            params: PARAMS_LOGICAL_AND,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "boolnot" => Some(FunctionSignature {
            name: "boolnot",
            display_name: "BOOLNOT",
            params: PARAMS_BOOLNOT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "boolor" => Some(FunctionSignature {
            name: "boolor",
            display_name: "BOOLOR",
            params: PARAMS_BOOLOR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "logical_or" => Some(FunctionSignature {
            name: "logical_or",
            display_name: "LOGICAL_OR",
            params: PARAMS_LOGICAL_OR,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "boolxor_agg" => Some(FunctionSignature {
            name: "boolxor_agg",
            display_name: "BOOLXOR_AGG",
            params: PARAMS_BOOLXOR_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "byte_length" => Some(FunctionSignature {
            name: "byte_length",
            display_name: "BYTE_LENGTH",
            params: PARAMS_BYTE_LENGTH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "case" => Some(FunctionSignature {
            name: "case",
            display_name: "CASE",
            params: PARAMS_CASE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "cast" => Some(FunctionSignature {
            name: "cast",
            display_name: "CAST",
            params: PARAMS_CAST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "cast_to_str_type" => Some(FunctionSignature {
            name: "cast_to_str_type",
            display_name: "CAST_TO_STR_TYPE",
            params: PARAMS_CAST_TO_STR_TYPE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "cbrt" => Some(FunctionSignature {
            name: "cbrt",
            display_name: "CBRT",
            params: PARAMS_CBRT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ceil" => Some(FunctionSignature {
            name: "ceil",
            display_name: "CEIL",
            params: PARAMS_CEIL,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "chr" => Some(FunctionSignature {
            name: "chr",
            display_name: "CHR",
            params: PARAMS_CHR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "length" => Some(FunctionSignature {
            name: "length",
            display_name: "LENGTH",
            params: PARAMS_LENGTH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "coalesce" => Some(FunctionSignature {
            name: "coalesce",
            display_name: "COALESCE",
            params: PARAMS_COALESCE,
            return_type: Some(ReturnTypeRule::MatchFirstArg),
            category: FunctionCategory::Scalar,
        }),
        "code_points_to_bytes" => Some(FunctionSignature {
            name: "code_points_to_bytes",
            display_name: "CODE_POINTS_TO_BYTES",
            params: PARAMS_CODE_POINTS_TO_BYTES,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "code_points_to_string" => Some(FunctionSignature {
            name: "code_points_to_string",
            display_name: "CODE_POINTS_TO_STRING",
            params: PARAMS_CODE_POINTS_TO_STRING,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "collate" => Some(FunctionSignature {
            name: "collate",
            display_name: "COLLATE",
            params: PARAMS_COLLATE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "collation" => Some(FunctionSignature {
            name: "collation",
            display_name: "COLLATION",
            params: PARAMS_COLLATION,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "columns" => Some(FunctionSignature {
            name: "columns",
            display_name: "COLUMNS",
            params: PARAMS_COLUMNS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "combined_agg_func" => Some(FunctionSignature {
            name: "combined_agg_func",
            display_name: "COMBINED_AGG_FUNC",
            params: PARAMS_COMBINED_AGG_FUNC,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "combined_parameterized_agg" => Some(FunctionSignature {
            name: "combined_parameterized_agg",
            display_name: "COMBINED_PARAMETERIZED_AGG",
            params: PARAMS_COMBINED_PARAMETERIZED_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "compress" => Some(FunctionSignature {
            name: "compress",
            display_name: "COMPRESS",
            params: PARAMS_COMPRESS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "concat" => Some(FunctionSignature {
            name: "concat",
            display_name: "CONCAT",
            params: PARAMS_CONCAT,
            return_type: Some(ReturnTypeRule::Text),
            category: FunctionCategory::Scalar,
        }),
        "concat_ws" => Some(FunctionSignature {
            name: "concat_ws",
            display_name: "CONCAT_WS",
            params: PARAMS_CONCAT_WS,
            return_type: Some(ReturnTypeRule::Text),
            category: FunctionCategory::Scalar,
        }),
        "connect_by_root" => Some(FunctionSignature {
            name: "connect_by_root",
            display_name: "CONNECT_BY_ROOT",
            params: PARAMS_CONNECT_BY_ROOT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "contains" => Some(FunctionSignature {
            name: "contains",
            display_name: "CONTAINS",
            params: PARAMS_CONTAINS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "convert" => Some(FunctionSignature {
            name: "convert",
            display_name: "CONVERT",
            params: PARAMS_CONVERT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "convert_timezone" => Some(FunctionSignature {
            name: "convert_timezone",
            display_name: "CONVERT_TIMEZONE",
            params: PARAMS_CONVERT_TIMEZONE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "convert_to_charset" => Some(FunctionSignature {
            name: "convert_to_charset",
            display_name: "CONVERT_TO_CHARSET",
            params: PARAMS_CONVERT_TO_CHARSET,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "corr" => Some(FunctionSignature {
            name: "corr",
            display_name: "CORR",
            params: PARAMS_CORR,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "cos" => Some(FunctionSignature {
            name: "cos",
            display_name: "COS",
            params: PARAMS_COS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "cosh" => Some(FunctionSignature {
            name: "cosh",
            display_name: "COSH",
            params: PARAMS_COSH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "cosine_distance" => Some(FunctionSignature {
            name: "cosine_distance",
            display_name: "COSINE_DISTANCE",
            params: PARAMS_COSINE_DISTANCE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "cot" => Some(FunctionSignature {
            name: "cot",
            display_name: "COT",
            params: PARAMS_COT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "coth" => Some(FunctionSignature {
            name: "coth",
            display_name: "COTH",
            params: PARAMS_COTH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "count" => Some(FunctionSignature {
            name: "count",
            display_name: "COUNT",
            params: PARAMS_COUNT,
            return_type: Some(ReturnTypeRule::Integer),
            category: FunctionCategory::Aggregate,
        }),
        "count_if" => Some(FunctionSignature {
            name: "count_if",
            display_name: "COUNT_IF",
            params: PARAMS_COUNT_IF,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "covar_pop" => Some(FunctionSignature {
            name: "covar_pop",
            display_name: "COVAR_POP",
            params: PARAMS_COVAR_POP,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "covar_samp" => Some(FunctionSignature {
            name: "covar_samp",
            display_name: "COVAR_SAMP",
            params: PARAMS_COVAR_SAMP,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "csc" => Some(FunctionSignature {
            name: "csc",
            display_name: "CSC",
            params: PARAMS_CSC,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "csch" => Some(FunctionSignature {
            name: "csch",
            display_name: "CSCH",
            params: PARAMS_CSCH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "cume_dist" => Some(FunctionSignature {
            name: "cume_dist",
            display_name: "CUME_DIST",
            params: PARAMS_CUME_DIST,
            return_type: None,
            category: FunctionCategory::Window,
        }),
        "current_date" => Some(FunctionSignature {
            name: "current_date",
            display_name: "CURRENT_DATE",
            params: PARAMS_CURRENT_DATE,
            return_type: Some(ReturnTypeRule::Date),
            category: FunctionCategory::Scalar,
        }),
        "current_datetime" => Some(FunctionSignature {
            name: "current_datetime",
            display_name: "CURRENT_DATETIME",
            params: PARAMS_CURRENT_DATETIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "current_schema" => Some(FunctionSignature {
            name: "current_schema",
            display_name: "CURRENT_SCHEMA",
            params: PARAMS_CURRENT_SCHEMA,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "current_time" => Some(FunctionSignature {
            name: "current_time",
            display_name: "CURRENT_TIME",
            params: PARAMS_CURRENT_TIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "current_timestamp" => Some(FunctionSignature {
            name: "current_timestamp",
            display_name: "CURRENT_TIMESTAMP",
            params: PARAMS_CURRENT_TIMESTAMP,
            return_type: Some(ReturnTypeRule::Timestamp),
            category: FunctionCategory::Scalar,
        }),
        "current_timestamp_ltz" => Some(FunctionSignature {
            name: "current_timestamp_ltz",
            display_name: "CURRENT_TIMESTAMP_LTZ",
            params: &[],
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "current_user" => Some(FunctionSignature {
            name: "current_user",
            display_name: "CURRENT_USER",
            params: PARAMS_CURRENT_USER,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "date" => Some(FunctionSignature {
            name: "date",
            display_name: "DATE",
            params: PARAMS_DATE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "date_diff" => Some(FunctionSignature {
            name: "date_diff",
            display_name: "DATE_DIFF",
            params: PARAMS_DATE_DIFF,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "date_from_parts" => Some(FunctionSignature {
            name: "date_from_parts",
            display_name: "DATE_FROM_PARTS",
            params: PARAMS_DATE_FROM_PARTS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "datetime" => Some(FunctionSignature {
            name: "datetime",
            display_name: "DATETIME",
            params: PARAMS_DATETIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "datetime_add" => Some(FunctionSignature {
            name: "datetime_add",
            display_name: "DATETIME_ADD",
            params: PARAMS_DATETIME_ADD,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "datetime_diff" => Some(FunctionSignature {
            name: "datetime_diff",
            display_name: "DATETIME_DIFF",
            params: PARAMS_DATETIME_DIFF,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "datetime_sub" => Some(FunctionSignature {
            name: "datetime_sub",
            display_name: "DATETIME_SUB",
            params: PARAMS_DATETIME_SUB,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "datetime_trunc" => Some(FunctionSignature {
            name: "datetime_trunc",
            display_name: "DATETIME_TRUNC",
            params: PARAMS_DATETIME_TRUNC,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "date_add" => Some(FunctionSignature {
            name: "date_add",
            display_name: "DATE_ADD",
            params: PARAMS_DATE_ADD,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "date_bin" => Some(FunctionSignature {
            name: "date_bin",
            display_name: "DATE_BIN",
            params: PARAMS_DATE_BIN,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "date_from_unix_date" => Some(FunctionSignature {
            name: "date_from_unix_date",
            display_name: "DATE_FROM_UNIX_DATE",
            params: PARAMS_DATE_FROM_UNIX_DATE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "date_str_to_date" => Some(FunctionSignature {
            name: "date_str_to_date",
            display_name: "DATE_STR_TO_DATE",
            params: PARAMS_DATE_STR_TO_DATE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "date_sub" => Some(FunctionSignature {
            name: "date_sub",
            display_name: "DATE_SUB",
            params: PARAMS_DATE_SUB,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "date_to_date_str" => Some(FunctionSignature {
            name: "date_to_date_str",
            display_name: "DATE_TO_DATE_STR",
            params: PARAMS_DATE_TO_DATE_STR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "date_to_di" => Some(FunctionSignature {
            name: "date_to_di",
            display_name: "DATE_TO_DI",
            params: PARAMS_DATE_TO_DI,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "date_trunc" => Some(FunctionSignature {
            name: "date_trunc",
            display_name: "DATE_TRUNC",
            params: PARAMS_DATE_TRUNC,
            return_type: Some(ReturnTypeRule::MatchFirstArg),
            category: FunctionCategory::Scalar,
        }),
        "day" => Some(FunctionSignature {
            name: "day",
            display_name: "DAY",
            params: PARAMS_DAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "day_of_month" => Some(FunctionSignature {
            name: "day_of_month",
            display_name: "DAY_OF_MONTH",
            params: PARAMS_DAY_OF_MONTH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "day_of_week" => Some(FunctionSignature {
            name: "day_of_week",
            display_name: "DAY_OF_WEEK",
            params: PARAMS_DAY_OF_WEEK,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "day_of_week_iso" => Some(FunctionSignature {
            name: "day_of_week_iso",
            display_name: "DAY_OF_WEEK_ISO",
            params: PARAMS_DAY_OF_WEEK_ISO,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "day_of_year" => Some(FunctionSignature {
            name: "day_of_year",
            display_name: "DAY_OF_YEAR",
            params: PARAMS_DAY_OF_YEAR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "decode" => Some(FunctionSignature {
            name: "decode",
            display_name: "DECODE",
            params: PARAMS_DECODE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "decode_case" => Some(FunctionSignature {
            name: "decode_case",
            display_name: "DECODE_CASE",
            params: PARAMS_DECODE_CASE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "decompress_binary" => Some(FunctionSignature {
            name: "decompress_binary",
            display_name: "DECOMPRESS_BINARY",
            params: PARAMS_DECOMPRESS_BINARY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "decompress_string" => Some(FunctionSignature {
            name: "decompress_string",
            display_name: "DECOMPRESS_STRING",
            params: PARAMS_DECOMPRESS_STRING,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "degrees" => Some(FunctionSignature {
            name: "degrees",
            display_name: "DEGREES",
            params: PARAMS_DEGREES,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "dense_rank" => Some(FunctionSignature {
            name: "dense_rank",
            display_name: "DENSE_RANK",
            params: PARAMS_DENSE_RANK,
            return_type: Some(ReturnTypeRule::Integer),
            category: FunctionCategory::Window,
        }),
        "di_to_date" => Some(FunctionSignature {
            name: "di_to_date",
            display_name: "DI_TO_DATE",
            params: PARAMS_DI_TO_DATE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "encode" => Some(FunctionSignature {
            name: "encode",
            display_name: "ENCODE",
            params: PARAMS_ENCODE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ends_with" => Some(FunctionSignature {
            name: "ends_with",
            display_name: "ENDS_WITH",
            params: PARAMS_ENDS_WITH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "equal_null" => Some(FunctionSignature {
            name: "equal_null",
            display_name: "EQUAL_NULL",
            params: PARAMS_EQUAL_NULL,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "euclidean_distance" => Some(FunctionSignature {
            name: "euclidean_distance",
            display_name: "EUCLIDEAN_DISTANCE",
            params: PARAMS_EUCLIDEAN_DISTANCE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "exists" => Some(FunctionSignature {
            name: "exists",
            display_name: "EXISTS",
            params: PARAMS_EXISTS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "exp" => Some(FunctionSignature {
            name: "exp",
            display_name: "EXP",
            params: PARAMS_EXP,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "explode" => Some(FunctionSignature {
            name: "explode",
            display_name: "EXPLODE",
            params: PARAMS_EXPLODE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "explode_outer" => Some(FunctionSignature {
            name: "explode_outer",
            display_name: "EXPLODE_OUTER",
            params: PARAMS_EXPLODE_OUTER,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "exploding_generate_series" => Some(FunctionSignature {
            name: "exploding_generate_series",
            display_name: "EXPLODING_GENERATE_SERIES",
            params: PARAMS_EXPLODING_GENERATE_SERIES,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "extract" => Some(FunctionSignature {
            name: "extract",
            display_name: "EXTRACT",
            params: PARAMS_EXTRACT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "factorial" => Some(FunctionSignature {
            name: "factorial",
            display_name: "FACTORIAL",
            params: PARAMS_FACTORIAL,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "farm_fingerprint" => Some(FunctionSignature {
            name: "farm_fingerprint",
            display_name: "FARM_FINGERPRINT",
            params: PARAMS_FARM_FINGERPRINT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "features_at_time" => Some(FunctionSignature {
            name: "features_at_time",
            display_name: "FEATURES_AT_TIME",
            params: PARAMS_FEATURES_AT_TIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "first" => Some(FunctionSignature {
            name: "first",
            display_name: "FIRST",
            params: PARAMS_FIRST,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "first_value" => Some(FunctionSignature {
            name: "first_value",
            display_name: "FIRST_VALUE",
            params: PARAMS_FIRST_VALUE,
            return_type: Some(ReturnTypeRule::MatchFirstArg),
            category: FunctionCategory::Window,
        }),
        "flatten" => Some(FunctionSignature {
            name: "flatten",
            display_name: "FLATTEN",
            params: PARAMS_FLATTEN,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "float64" => Some(FunctionSignature {
            name: "float64",
            display_name: "FLOAT64",
            params: PARAMS_FLOAT64,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "floor" => Some(FunctionSignature {
            name: "floor",
            display_name: "FLOOR",
            params: PARAMS_FLOOR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "format" => Some(FunctionSignature {
            name: "format",
            display_name: "FORMAT",
            params: PARAMS_FORMAT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "from_base" => Some(FunctionSignature {
            name: "from_base",
            display_name: "FROM_BASE",
            params: PARAMS_FROM_BASE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "from_base32" => Some(FunctionSignature {
            name: "from_base32",
            display_name: "FROM_BASE32",
            params: PARAMS_FROM_BASE32,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "from_base64" => Some(FunctionSignature {
            name: "from_base64",
            display_name: "FROM_BASE64",
            params: PARAMS_FROM_BASE64,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "from_iso8601timestamp" => Some(FunctionSignature {
            name: "from_iso8601timestamp",
            display_name: "FROM_ISO8601TIMESTAMP",
            params: PARAMS_FROM_ISO8601TIMESTAMP,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "gap_fill" => Some(FunctionSignature {
            name: "gap_fill",
            display_name: "GAP_FILL",
            params: PARAMS_GAP_FILL,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "generate_date_array" => Some(FunctionSignature {
            name: "generate_date_array",
            display_name: "GENERATE_DATE_ARRAY",
            params: PARAMS_GENERATE_DATE_ARRAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "generate_embedding" => Some(FunctionSignature {
            name: "generate_embedding",
            display_name: "GENERATE_EMBEDDING",
            params: PARAMS_GENERATE_EMBEDDING,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "generate_series" => Some(FunctionSignature {
            name: "generate_series",
            display_name: "GENERATE_SERIES",
            params: PARAMS_GENERATE_SERIES,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "generate_timestamp_array" => Some(FunctionSignature {
            name: "generate_timestamp_array",
            display_name: "GENERATE_TIMESTAMP_ARRAY",
            params: PARAMS_GENERATE_TIMESTAMP_ARRAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "uuid" => Some(FunctionSignature {
            name: "uuid",
            display_name: "UUID",
            params: PARAMS_UUID,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "getbit" => Some(FunctionSignature {
            name: "getbit",
            display_name: "GETBIT",
            params: PARAMS_GETBIT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "get_extract" => Some(FunctionSignature {
            name: "get_extract",
            display_name: "GET_EXTRACT",
            params: PARAMS_GET_EXTRACT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "greatest" => Some(FunctionSignature {
            name: "greatest",
            display_name: "GREATEST",
            params: PARAMS_GREATEST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "greatest_ignore_nulls" => Some(FunctionSignature {
            name: "greatest_ignore_nulls",
            display_name: "GREATEST_IGNORE_NULLS",
            params: PARAMS_GREATEST_IGNORE_NULLS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "grouping" => Some(FunctionSignature {
            name: "grouping",
            display_name: "GROUPING",
            params: PARAMS_GROUPING,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "grouping_id" => Some(FunctionSignature {
            name: "grouping_id",
            display_name: "GROUPING_ID",
            params: PARAMS_GROUPING_ID,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "group_concat" => Some(FunctionSignature {
            name: "group_concat",
            display_name: "GROUP_CONCAT",
            params: PARAMS_GROUP_CONCAT,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "hex" => Some(FunctionSignature {
            name: "hex",
            display_name: "HEX",
            params: PARAMS_HEX,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "hex_decode_string" => Some(FunctionSignature {
            name: "hex_decode_string",
            display_name: "HEX_DECODE_STRING",
            params: PARAMS_HEX_DECODE_STRING,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "hex_encode" => Some(FunctionSignature {
            name: "hex_encode",
            display_name: "HEX_ENCODE",
            params: PARAMS_HEX_ENCODE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "hll" => Some(FunctionSignature {
            name: "hll",
            display_name: "HLL",
            params: PARAMS_HLL,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "hour" => Some(FunctionSignature {
            name: "hour",
            display_name: "HOUR",
            params: PARAMS_HOUR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "if" => Some(FunctionSignature {
            name: "if",
            display_name: "IF",
            params: PARAMS_IF,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "initcap" => Some(FunctionSignature {
            name: "initcap",
            display_name: "INITCAP",
            params: PARAMS_INITCAP,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "inline" => Some(FunctionSignature {
            name: "inline",
            display_name: "INLINE",
            params: PARAMS_INLINE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "stuff" => Some(FunctionSignature {
            name: "stuff",
            display_name: "STUFF",
            params: PARAMS_STUFF,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "int64" => Some(FunctionSignature {
            name: "int64",
            display_name: "INT64",
            params: PARAMS_INT64,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "is_inf" => Some(FunctionSignature {
            name: "is_inf",
            display_name: "IS_INF",
            params: PARAMS_IS_INF,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "is_nan" => Some(FunctionSignature {
            name: "is_nan",
            display_name: "IS_NAN",
            params: PARAMS_IS_NAN,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "is_ascii" => Some(FunctionSignature {
            name: "is_ascii",
            display_name: "IS_ASCII",
            params: PARAMS_IS_ASCII,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "is_null_value" => Some(FunctionSignature {
            name: "is_null_value",
            display_name: "IS_NULL_VALUE",
            params: PARAMS_IS_NULL_VALUE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "jarowinkler_similarity" => Some(FunctionSignature {
            name: "jarowinkler_similarity",
            display_name: "JAROWINKLER_SIMILARITY",
            params: PARAMS_JAROWINKLER_SIMILARITY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "jsonb_contains" => Some(FunctionSignature {
            name: "jsonb_contains",
            display_name: "JSONB_CONTAINS",
            params: PARAMS_JSONB_CONTAINS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "jsonb_exists" => Some(FunctionSignature {
            name: "jsonb_exists",
            display_name: "JSONB_EXISTS",
            params: PARAMS_JSONB_EXISTS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "jsonb_extract" => Some(FunctionSignature {
            name: "jsonb_extract",
            display_name: "JSONB_EXTRACT",
            params: PARAMS_JSONB_EXTRACT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "jsonb_extract_scalar" => Some(FunctionSignature {
            name: "jsonb_extract_scalar",
            display_name: "JSONB_EXTRACT_SCALAR",
            params: PARAMS_JSONB_EXTRACT_SCALAR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_array_append" => Some(FunctionSignature {
            name: "json_array_append",
            display_name: "JSON_ARRAY_APPEND",
            params: PARAMS_JSON_ARRAY_APPEND,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_array_contains" => Some(FunctionSignature {
            name: "json_array_contains",
            display_name: "JSON_ARRAY_CONTAINS",
            params: PARAMS_JSON_ARRAY_CONTAINS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_array_insert" => Some(FunctionSignature {
            name: "json_array_insert",
            display_name: "JSON_ARRAY_INSERT",
            params: PARAMS_JSON_ARRAY_INSERT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_extract" => Some(FunctionSignature {
            name: "json_extract",
            display_name: "JSON_EXTRACT",
            params: PARAMS_JSON_EXTRACT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_extract_array" => Some(FunctionSignature {
            name: "json_extract_array",
            display_name: "JSON_EXTRACT_ARRAY",
            params: PARAMS_JSON_EXTRACT_ARRAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_extract_scalar" => Some(FunctionSignature {
            name: "json_extract_scalar",
            display_name: "JSON_EXTRACT_SCALAR",
            params: PARAMS_JSON_EXTRACT_SCALAR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_format" => Some(FunctionSignature {
            name: "json_format",
            display_name: "JSON_FORMAT",
            params: PARAMS_JSON_FORMAT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "parse_json" => Some(FunctionSignature {
            name: "parse_json",
            display_name: "PARSE_JSON",
            params: PARAMS_PARSE_JSON,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_remove" => Some(FunctionSignature {
            name: "json_remove",
            display_name: "JSON_REMOVE",
            params: PARAMS_JSON_REMOVE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_set" => Some(FunctionSignature {
            name: "json_set",
            display_name: "JSON_SET",
            params: PARAMS_JSON_SET,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_strip_nulls" => Some(FunctionSignature {
            name: "json_strip_nulls",
            display_name: "JSON_STRIP_NULLS",
            params: PARAMS_JSON_STRIP_NULLS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_type" => Some(FunctionSignature {
            name: "json_type",
            display_name: "JSON_TYPE",
            params: PARAMS_JSON_TYPE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "justify_days" => Some(FunctionSignature {
            name: "justify_days",
            display_name: "JUSTIFY_DAYS",
            params: PARAMS_JUSTIFY_DAYS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "justify_hours" => Some(FunctionSignature {
            name: "justify_hours",
            display_name: "JUSTIFY_HOURS",
            params: PARAMS_JUSTIFY_HOURS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "justify_interval" => Some(FunctionSignature {
            name: "justify_interval",
            display_name: "JUSTIFY_INTERVAL",
            params: PARAMS_JUSTIFY_INTERVAL,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_array" => Some(FunctionSignature {
            name: "json_array",
            display_name: "JSON_ARRAY",
            params: PARAMS_JSON_ARRAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_array_agg" => Some(FunctionSignature {
            name: "json_array_agg",
            display_name: "JSON_ARRAY_AGG",
            params: PARAMS_JSON_ARRAY_AGG,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_bool" => Some(FunctionSignature {
            name: "json_bool",
            display_name: "JSON_BOOL",
            params: PARAMS_JSON_BOOL,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "jsonb_contains_all_top_keys" => Some(FunctionSignature {
            name: "jsonb_contains_all_top_keys",
            display_name: "JSONB_CONTAINS_ALL_TOP_KEYS",
            params: PARAMS_JSONB_CONTAINS_ALL_TOP_KEYS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "jsonb_contains_any_top_keys" => Some(FunctionSignature {
            name: "jsonb_contains_any_top_keys",
            display_name: "JSONB_CONTAINS_ANY_TOP_KEYS",
            params: PARAMS_JSONB_CONTAINS_ANY_TOP_KEYS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "jsonb_delete_at_path" => Some(FunctionSignature {
            name: "jsonb_delete_at_path",
            display_name: "JSONB_DELETE_AT_PATH",
            params: PARAMS_JSONB_DELETE_AT_PATH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "jsonb_object_agg" => Some(FunctionSignature {
            name: "jsonb_object_agg",
            display_name: "JSONB_OBJECT_AGG",
            params: PARAMS_JSONB_OBJECT_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "json_cast" => Some(FunctionSignature {
            name: "json_cast",
            display_name: "JSON_CAST",
            params: PARAMS_JSON_CAST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_exists" => Some(FunctionSignature {
            name: "json_exists",
            display_name: "JSON_EXISTS",
            params: PARAMS_JSON_EXISTS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_keys_at_depth" => Some(FunctionSignature {
            name: "json_keys_at_depth",
            display_name: "JSON_KEYS_AT_DEPTH",
            params: PARAMS_JSON_KEYS_AT_DEPTH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_object" => Some(FunctionSignature {
            name: "json_object",
            display_name: "JSON_OBJECT",
            params: PARAMS_JSON_OBJECT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_object_agg" => Some(FunctionSignature {
            name: "json_object_agg",
            display_name: "JSON_OBJECT_AGG",
            params: PARAMS_JSON_OBJECT_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "json_table" => Some(FunctionSignature {
            name: "json_table",
            display_name: "JSON_TABLE",
            params: PARAMS_JSON_TABLE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "json_value_array" => Some(FunctionSignature {
            name: "json_value_array",
            display_name: "JSON_VALUE_ARRAY",
            params: PARAMS_JSON_VALUE_ARRAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "lag" => Some(FunctionSignature {
            name: "lag",
            display_name: "LAG",
            params: PARAMS_LAG,
            return_type: Some(ReturnTypeRule::MatchFirstArg),
            category: FunctionCategory::Window,
        }),
        "last" => Some(FunctionSignature {
            name: "last",
            display_name: "LAST",
            params: PARAMS_LAST,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "last_day" => Some(FunctionSignature {
            name: "last_day",
            display_name: "LAST_DAY",
            params: PARAMS_LAST_DAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "last_value" => Some(FunctionSignature {
            name: "last_value",
            display_name: "LAST_VALUE",
            params: PARAMS_LAST_VALUE,
            return_type: Some(ReturnTypeRule::MatchFirstArg),
            category: FunctionCategory::Window,
        }),
        "lax_bool" => Some(FunctionSignature {
            name: "lax_bool",
            display_name: "LAX_BOOL",
            params: PARAMS_LAX_BOOL,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "lax_float64" => Some(FunctionSignature {
            name: "lax_float64",
            display_name: "LAX_FLOAT64",
            params: PARAMS_LAX_FLOAT64,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "lax_int64" => Some(FunctionSignature {
            name: "lax_int64",
            display_name: "LAX_INT64",
            params: PARAMS_LAX_INT64,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "lax_string" => Some(FunctionSignature {
            name: "lax_string",
            display_name: "LAX_STRING",
            params: PARAMS_LAX_STRING,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "lower" => Some(FunctionSignature {
            name: "lower",
            display_name: "LOWER",
            params: PARAMS_LOWER,
            return_type: Some(ReturnTypeRule::Text),
            category: FunctionCategory::Scalar,
        }),
        "lead" => Some(FunctionSignature {
            name: "lead",
            display_name: "LEAD",
            params: PARAMS_LEAD,
            return_type: Some(ReturnTypeRule::MatchFirstArg),
            category: FunctionCategory::Window,
        }),
        "least" => Some(FunctionSignature {
            name: "least",
            display_name: "LEAST",
            params: PARAMS_LEAST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "least_ignore_nulls" => Some(FunctionSignature {
            name: "least_ignore_nulls",
            display_name: "LEAST_IGNORE_NULLS",
            params: PARAMS_LEAST_IGNORE_NULLS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "left" => Some(FunctionSignature {
            name: "left",
            display_name: "LEFT",
            params: PARAMS_LEFT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "levenshtein" => Some(FunctionSignature {
            name: "levenshtein",
            display_name: "LEVENSHTEIN",
            params: PARAMS_LEVENSHTEIN,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "list" => Some(FunctionSignature {
            name: "list",
            display_name: "LIST",
            params: PARAMS_LIST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ln" => Some(FunctionSignature {
            name: "ln",
            display_name: "LN",
            params: PARAMS_LN,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "log" => Some(FunctionSignature {
            name: "log",
            display_name: "LOG",
            params: PARAMS_LOG,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "lower_hex" => Some(FunctionSignature {
            name: "lower_hex",
            display_name: "LOWER_HEX",
            params: PARAMS_LOWER_HEX,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "make_interval" => Some(FunctionSignature {
            name: "make_interval",
            display_name: "MAKE_INTERVAL",
            params: PARAMS_MAKE_INTERVAL,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "map" => Some(FunctionSignature {
            name: "map",
            display_name: "MAP",
            params: PARAMS_MAP,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "map_from_entries" => Some(FunctionSignature {
            name: "map_from_entries",
            display_name: "MAP_FROM_ENTRIES",
            params: PARAMS_MAP_FROM_ENTRIES,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "match_against" => Some(FunctionSignature {
            name: "match_against",
            display_name: "MATCH_AGAINST",
            params: PARAMS_MATCH_AGAINST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "max" => Some(FunctionSignature {
            name: "max",
            display_name: "MAX",
            params: PARAMS_MAX,
            return_type: Some(ReturnTypeRule::MatchFirstArg),
            category: FunctionCategory::Aggregate,
        }),
        "md5" => Some(FunctionSignature {
            name: "md5",
            display_name: "MD5",
            params: PARAMS_MD5,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "md5digest" => Some(FunctionSignature {
            name: "md5digest",
            display_name: "MD5DIGEST",
            params: PARAMS_MD5DIGEST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "median" => Some(FunctionSignature {
            name: "median",
            display_name: "MEDIAN",
            params: PARAMS_MEDIAN,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "min" => Some(FunctionSignature {
            name: "min",
            display_name: "MIN",
            params: PARAMS_MIN,
            return_type: Some(ReturnTypeRule::MatchFirstArg),
            category: FunctionCategory::Aggregate,
        }),
        "minhash" => Some(FunctionSignature {
            name: "minhash",
            display_name: "MINHASH",
            params: PARAMS_MINHASH,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "minhash_combine" => Some(FunctionSignature {
            name: "minhash_combine",
            display_name: "MINHASH_COMBINE",
            params: PARAMS_MINHASH_COMBINE,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "minute" => Some(FunctionSignature {
            name: "minute",
            display_name: "MINUTE",
            params: PARAMS_MINUTE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "month" => Some(FunctionSignature {
            name: "month",
            display_name: "MONTH",
            params: PARAMS_MONTH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "monthname" => Some(FunctionSignature {
            name: "monthname",
            display_name: "MONTHNAME",
            params: PARAMS_MONTHNAME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "months_between" => Some(FunctionSignature {
            name: "months_between",
            display_name: "MONTHS_BETWEEN",
            params: PARAMS_MONTHS_BETWEEN,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "md5number_lower64" => Some(FunctionSignature {
            name: "md5number_lower64",
            display_name: "MD5NUMBER_LOWER64",
            params: PARAMS_MD5NUMBER_LOWER64,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "md5number_upper64" => Some(FunctionSignature {
            name: "md5number_upper64",
            display_name: "MD5NUMBER_UPPER64",
            params: PARAMS_MD5NUMBER_UPPER64,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ml_forecast" => Some(FunctionSignature {
            name: "ml_forecast",
            display_name: "ML_FORECAST",
            params: PARAMS_ML_FORECAST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ml_translate" => Some(FunctionSignature {
            name: "ml_translate",
            display_name: "ML_TRANSLATE",
            params: PARAMS_ML_TRANSLATE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "next_day" => Some(FunctionSignature {
            name: "next_day",
            display_name: "NEXT_DAY",
            params: PARAMS_NEXT_DAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "next_value_for" => Some(FunctionSignature {
            name: "next_value_for",
            display_name: "NEXT_VALUE_FOR",
            params: PARAMS_NEXT_VALUE_FOR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "normalize" => Some(FunctionSignature {
            name: "normalize",
            display_name: "NORMALIZE",
            params: PARAMS_NORMALIZE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "now" => Some(FunctionSignature {
            name: "now",
            display_name: "NOW",
            params: &[],
            return_type: Some(ReturnTypeRule::Timestamp),
            category: FunctionCategory::Scalar,
        }),
        "nth_value" => Some(FunctionSignature {
            name: "nth_value",
            display_name: "NTH_VALUE",
            params: PARAMS_NTH_VALUE,
            return_type: None,
            category: FunctionCategory::Window,
        }),
        "ntile" => Some(FunctionSignature {
            name: "ntile",
            display_name: "NTILE",
            params: PARAMS_NTILE,
            return_type: Some(ReturnTypeRule::Integer),
            category: FunctionCategory::Window,
        }),
        "nullif" => Some(FunctionSignature {
            name: "nullif",
            display_name: "NULLIF",
            params: PARAMS_NULLIF,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "number_to_str" => Some(FunctionSignature {
            name: "number_to_str",
            display_name: "NUMBER_TO_STR",
            params: PARAMS_NUMBER_TO_STR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "nvl2" => Some(FunctionSignature {
            name: "nvl2",
            display_name: "NVL2",
            params: PARAMS_NVL2,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "object_agg" => Some(FunctionSignature {
            name: "object_agg",
            display_name: "OBJECT_AGG",
            params: PARAMS_OBJECT_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "object_insert" => Some(FunctionSignature {
            name: "object_insert",
            display_name: "OBJECT_INSERT",
            params: PARAMS_OBJECT_INSERT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "open_json" => Some(FunctionSignature {
            name: "open_json",
            display_name: "OPEN_JSON",
            params: PARAMS_OPEN_JSON,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "or" => Some(FunctionSignature {
            name: "or",
            display_name: "OR",
            params: PARAMS_OR,
            return_type: Some(ReturnTypeRule::Boolean),
            category: FunctionCategory::Scalar,
        }),
        "overlay" => Some(FunctionSignature {
            name: "overlay",
            display_name: "OVERLAY",
            params: PARAMS_OVERLAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "pad" => Some(FunctionSignature {
            name: "pad",
            display_name: "PAD",
            params: PARAMS_PAD,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "parameterized_agg" => Some(FunctionSignature {
            name: "parameterized_agg",
            display_name: "PARAMETERIZED_AGG",
            params: PARAMS_PARAMETERIZED_AGG,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "parse_bignumeric" => Some(FunctionSignature {
            name: "parse_bignumeric",
            display_name: "PARSE_BIGNUMERIC",
            params: PARAMS_PARSE_BIGNUMERIC,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "parse_datetime" => Some(FunctionSignature {
            name: "parse_datetime",
            display_name: "PARSE_DATETIME",
            params: PARAMS_PARSE_DATETIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "parse_ip" => Some(FunctionSignature {
            name: "parse_ip",
            display_name: "PARSE_IP",
            params: PARAMS_PARSE_IP,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "parse_numeric" => Some(FunctionSignature {
            name: "parse_numeric",
            display_name: "PARSE_NUMERIC",
            params: PARAMS_PARSE_NUMERIC,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "parse_time" => Some(FunctionSignature {
            name: "parse_time",
            display_name: "PARSE_TIME",
            params: PARAMS_PARSE_TIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "parse_url" => Some(FunctionSignature {
            name: "parse_url",
            display_name: "PARSE_URL",
            params: PARAMS_PARSE_URL,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "percentile_cont" => Some(FunctionSignature {
            name: "percentile_cont",
            display_name: "PERCENTILE_CONT",
            params: PARAMS_PERCENTILE_CONT,
            return_type: None,
            category: FunctionCategory::Window,
        }),
        "percentile_disc" => Some(FunctionSignature {
            name: "percentile_disc",
            display_name: "PERCENTILE_DISC",
            params: PARAMS_PERCENTILE_DISC,
            return_type: None,
            category: FunctionCategory::Window,
        }),
        "percent_rank" => Some(FunctionSignature {
            name: "percent_rank",
            display_name: "PERCENT_RANK",
            params: PARAMS_PERCENT_RANK,
            return_type: None,
            category: FunctionCategory::Window,
        }),
        "pi" => Some(FunctionSignature {
            name: "pi",
            display_name: "PI",
            params: &[],
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "posexplode" => Some(FunctionSignature {
            name: "posexplode",
            display_name: "POSEXPLODE",
            params: PARAMS_POSEXPLODE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "posexplode_outer" => Some(FunctionSignature {
            name: "posexplode_outer",
            display_name: "POSEXPLODE_OUTER",
            params: PARAMS_POSEXPLODE_OUTER,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "pow" => Some(FunctionSignature {
            name: "pow",
            display_name: "POW",
            params: PARAMS_POW,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "predict" => Some(FunctionSignature {
            name: "predict",
            display_name: "PREDICT",
            params: PARAMS_PREDICT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "previous_day" => Some(FunctionSignature {
            name: "previous_day",
            display_name: "PREVIOUS_DAY",
            params: PARAMS_PREVIOUS_DAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "quantile" => Some(FunctionSignature {
            name: "quantile",
            display_name: "QUANTILE",
            params: PARAMS_QUANTILE,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "quarter" => Some(FunctionSignature {
            name: "quarter",
            display_name: "QUARTER",
            params: PARAMS_QUARTER,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "radians" => Some(FunctionSignature {
            name: "radians",
            display_name: "RADIANS",
            params: PARAMS_RADIANS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "rand" => Some(FunctionSignature {
            name: "rand",
            display_name: "RAND",
            params: PARAMS_RAND,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "randn" => Some(FunctionSignature {
            name: "randn",
            display_name: "RANDN",
            params: PARAMS_RANDN,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "range_bucket" => Some(FunctionSignature {
            name: "range_bucket",
            display_name: "RANGE_BUCKET",
            params: PARAMS_RANGE_BUCKET,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "range_n" => Some(FunctionSignature {
            name: "range_n",
            display_name: "RANGE_N",
            params: PARAMS_RANGE_N,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "rank" => Some(FunctionSignature {
            name: "rank",
            display_name: "RANK",
            params: PARAMS_RANK,
            return_type: Some(ReturnTypeRule::Integer),
            category: FunctionCategory::Window,
        }),
        "read_csv" => Some(FunctionSignature {
            name: "read_csv",
            display_name: "READ_CSV",
            params: PARAMS_READ_CSV,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "read_parquet" => Some(FunctionSignature {
            name: "read_parquet",
            display_name: "READ_PARQUET",
            params: PARAMS_READ_PARQUET,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "reduce" => Some(FunctionSignature {
            name: "reduce",
            display_name: "REDUCE",
            params: PARAMS_REDUCE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regexp_count" => Some(FunctionSignature {
            name: "regexp_count",
            display_name: "REGEXP_COUNT",
            params: PARAMS_REGEXP_COUNT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regexp_extract" => Some(FunctionSignature {
            name: "regexp_extract",
            display_name: "REGEXP_EXTRACT",
            params: PARAMS_REGEXP_EXTRACT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regexp_extract_all" => Some(FunctionSignature {
            name: "regexp_extract_all",
            display_name: "REGEXP_EXTRACT_ALL",
            params: PARAMS_REGEXP_EXTRACT_ALL,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regexp_full_match" => Some(FunctionSignature {
            name: "regexp_full_match",
            display_name: "REGEXP_FULL_MATCH",
            params: PARAMS_REGEXP_FULL_MATCH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regexp_instr" => Some(FunctionSignature {
            name: "regexp_instr",
            display_name: "REGEXP_INSTR",
            params: PARAMS_REGEXP_INSTR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regexp_i_like" => Some(FunctionSignature {
            name: "regexp_i_like",
            display_name: "REGEXP_I_LIKE",
            params: PARAMS_REGEXP_I_LIKE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regexp_like" => Some(FunctionSignature {
            name: "regexp_like",
            display_name: "REGEXP_LIKE",
            params: PARAMS_REGEXP_LIKE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regexp_replace" => Some(FunctionSignature {
            name: "regexp_replace",
            display_name: "REGEXP_REPLACE",
            params: PARAMS_REGEXP_REPLACE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regexp_split" => Some(FunctionSignature {
            name: "regexp_split",
            display_name: "REGEXP_SPLIT",
            params: PARAMS_REGEXP_SPLIT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regr_avgx" => Some(FunctionSignature {
            name: "regr_avgx",
            display_name: "REGR_AVGX",
            params: PARAMS_REGR_AVGX,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regr_avgy" => Some(FunctionSignature {
            name: "regr_avgy",
            display_name: "REGR_AVGY",
            params: PARAMS_REGR_AVGY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regr_valx" => Some(FunctionSignature {
            name: "regr_valx",
            display_name: "REGR_VALX",
            params: PARAMS_REGR_VALX,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "regr_valy" => Some(FunctionSignature {
            name: "regr_valy",
            display_name: "REGR_VALY",
            params: PARAMS_REGR_VALY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "repeat" => Some(FunctionSignature {
            name: "repeat",
            display_name: "REPEAT",
            params: PARAMS_REPEAT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "replace" => Some(FunctionSignature {
            name: "replace",
            display_name: "REPLACE",
            params: PARAMS_REPLACE,
            return_type: Some(ReturnTypeRule::Text),
            category: FunctionCategory::Scalar,
        }),
        "reverse" => Some(FunctionSignature {
            name: "reverse",
            display_name: "REVERSE",
            params: PARAMS_REVERSE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "right" => Some(FunctionSignature {
            name: "right",
            display_name: "RIGHT",
            params: PARAMS_RIGHT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "round" => Some(FunctionSignature {
            name: "round",
            display_name: "ROUND",
            params: PARAMS_ROUND,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "row_number" => Some(FunctionSignature {
            name: "row_number",
            display_name: "ROW_NUMBER",
            params: PARAMS_ROW_NUMBER,
            return_type: Some(ReturnTypeRule::Integer),
            category: FunctionCategory::Window,
        }),
        "rtrimmed_length" => Some(FunctionSignature {
            name: "rtrimmed_length",
            display_name: "RTRIMMED_LENGTH",
            params: PARAMS_RTRIMMED_LENGTH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "safe_add" => Some(FunctionSignature {
            name: "safe_add",
            display_name: "SAFE_ADD",
            params: PARAMS_SAFE_ADD,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "safe_convert_bytes_to_string" => Some(FunctionSignature {
            name: "safe_convert_bytes_to_string",
            display_name: "SAFE_CONVERT_BYTES_TO_STRING",
            params: PARAMS_SAFE_CONVERT_BYTES_TO_STRING,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "safe_divide" => Some(FunctionSignature {
            name: "safe_divide",
            display_name: "SAFE_DIVIDE",
            params: PARAMS_SAFE_DIVIDE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "safe_multiply" => Some(FunctionSignature {
            name: "safe_multiply",
            display_name: "SAFE_MULTIPLY",
            params: PARAMS_SAFE_MULTIPLY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "safe_negate" => Some(FunctionSignature {
            name: "safe_negate",
            display_name: "SAFE_NEGATE",
            params: PARAMS_SAFE_NEGATE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "safe_subtract" => Some(FunctionSignature {
            name: "safe_subtract",
            display_name: "SAFE_SUBTRACT",
            params: PARAMS_SAFE_SUBTRACT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "search" => Some(FunctionSignature {
            name: "search",
            display_name: "SEARCH",
            params: PARAMS_SEARCH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "sec" => Some(FunctionSignature {
            name: "sec",
            display_name: "SEC",
            params: PARAMS_SEC,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "sech" => Some(FunctionSignature {
            name: "sech",
            display_name: "SECH",
            params: PARAMS_SECH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "second" => Some(FunctionSignature {
            name: "second",
            display_name: "SECOND",
            params: PARAMS_SECOND,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "sha" => Some(FunctionSignature {
            name: "sha",
            display_name: "SHA",
            params: PARAMS_SHA,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "sha2" => Some(FunctionSignature {
            name: "sha2",
            display_name: "SHA2",
            params: PARAMS_SHA2,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "sign" => Some(FunctionSignature {
            name: "sign",
            display_name: "SIGN",
            params: PARAMS_SIGN,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "sin" => Some(FunctionSignature {
            name: "sin",
            display_name: "SIN",
            params: PARAMS_SIN,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "sinh" => Some(FunctionSignature {
            name: "sinh",
            display_name: "SINH",
            params: PARAMS_SINH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "skewness" => Some(FunctionSignature {
            name: "skewness",
            display_name: "SKEWNESS",
            params: PARAMS_SKEWNESS,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "sort_array" => Some(FunctionSignature {
            name: "sort_array",
            display_name: "SORT_ARRAY",
            params: PARAMS_SORT_ARRAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "soundex" => Some(FunctionSignature {
            name: "soundex",
            display_name: "SOUNDEX",
            params: PARAMS_SOUNDEX,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "soundex_p123" => Some(FunctionSignature {
            name: "soundex_p123",
            display_name: "SOUNDEX_P123",
            params: PARAMS_SOUNDEX_P123,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "space" => Some(FunctionSignature {
            name: "space",
            display_name: "SPACE",
            params: PARAMS_SPACE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "split" => Some(FunctionSignature {
            name: "split",
            display_name: "SPLIT",
            params: PARAMS_SPLIT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "string_to_array" => Some(FunctionSignature {
            name: "string_to_array",
            display_name: "STRING_TO_ARRAY",
            params: PARAMS_STRING_TO_ARRAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "split_part" => Some(FunctionSignature {
            name: "split_part",
            display_name: "SPLIT_PART",
            params: PARAMS_SPLIT_PART,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "sqrt" => Some(FunctionSignature {
            name: "sqrt",
            display_name: "SQRT",
            params: PARAMS_SQRT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "standard_hash" => Some(FunctionSignature {
            name: "standard_hash",
            display_name: "STANDARD_HASH",
            params: PARAMS_STANDARD_HASH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "starts_with" => Some(FunctionSignature {
            name: "starts_with",
            display_name: "STARTS_WITH",
            params: PARAMS_STARTS_WITH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "star_map" => Some(FunctionSignature {
            name: "star_map",
            display_name: "STAR_MAP",
            params: PARAMS_STAR_MAP,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "stddev" => Some(FunctionSignature {
            name: "stddev",
            display_name: "STDDEV",
            params: PARAMS_STDDEV,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "stddev_pop" => Some(FunctionSignature {
            name: "stddev_pop",
            display_name: "STDDEV_POP",
            params: PARAMS_STDDEV_POP,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "stddev_samp" => Some(FunctionSignature {
            name: "stddev_samp",
            display_name: "STDDEV_SAMP",
            params: PARAMS_STDDEV_SAMP,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "string" => Some(FunctionSignature {
            name: "string",
            display_name: "STRING",
            params: PARAMS_STRING,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "struct" => Some(FunctionSignature {
            name: "struct",
            display_name: "STRUCT",
            params: PARAMS_STRUCT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "struct_extract" => Some(FunctionSignature {
            name: "struct_extract",
            display_name: "STRUCT_EXTRACT",
            params: PARAMS_STRUCT_EXTRACT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "str_position" => Some(FunctionSignature {
            name: "str_position",
            display_name: "STR_POSITION",
            params: PARAMS_STR_POSITION,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "str_to_date" => Some(FunctionSignature {
            name: "str_to_date",
            display_name: "STR_TO_DATE",
            params: PARAMS_STR_TO_DATE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "str_to_map" => Some(FunctionSignature {
            name: "str_to_map",
            display_name: "STR_TO_MAP",
            params: PARAMS_STR_TO_MAP,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "str_to_time" => Some(FunctionSignature {
            name: "str_to_time",
            display_name: "STR_TO_TIME",
            params: PARAMS_STR_TO_TIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "str_to_unix" => Some(FunctionSignature {
            name: "str_to_unix",
            display_name: "STR_TO_UNIX",
            params: PARAMS_STR_TO_UNIX,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "st_distance" => Some(FunctionSignature {
            name: "st_distance",
            display_name: "ST_DISTANCE",
            params: PARAMS_ST_DISTANCE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "st_point" => Some(FunctionSignature {
            name: "st_point",
            display_name: "ST_POINT",
            params: PARAMS_ST_POINT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "substring" => Some(FunctionSignature {
            name: "substring",
            display_name: "SUBSTRING",
            params: PARAMS_SUBSTRING,
            return_type: Some(ReturnTypeRule::Text),
            category: FunctionCategory::Scalar,
        }),
        "substring_index" => Some(FunctionSignature {
            name: "substring_index",
            display_name: "SUBSTRING_INDEX",
            params: PARAMS_SUBSTRING_INDEX,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "sum" => Some(FunctionSignature {
            name: "sum",
            display_name: "SUM",
            params: PARAMS_SUM,
            return_type: Some(ReturnTypeRule::Numeric),
            category: FunctionCategory::Aggregate,
        }),
        "sha1digest" => Some(FunctionSignature {
            name: "sha1digest",
            display_name: "SHA1DIGEST",
            params: PARAMS_SHA1DIGEST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "sha2digest" => Some(FunctionSignature {
            name: "sha2digest",
            display_name: "SHA2DIGEST",
            params: PARAMS_SHA2DIGEST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "tan" => Some(FunctionSignature {
            name: "tan",
            display_name: "TAN",
            params: PARAMS_TAN,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "tanh" => Some(FunctionSignature {
            name: "tanh",
            display_name: "TANH",
            params: PARAMS_TANH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time" => Some(FunctionSignature {
            name: "time",
            display_name: "TIME",
            params: PARAMS_TIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time_from_parts" => Some(FunctionSignature {
            name: "time_from_parts",
            display_name: "TIME_FROM_PARTS",
            params: PARAMS_TIME_FROM_PARTS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "timestamp" => Some(FunctionSignature {
            name: "timestamp",
            display_name: "TIMESTAMP",
            params: PARAMS_TIMESTAMP,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "timestamp_diff" => Some(FunctionSignature {
            name: "timestamp_diff",
            display_name: "TIMESTAMP_DIFF",
            params: PARAMS_TIMESTAMP_DIFF,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "timestamp_from_parts" => Some(FunctionSignature {
            name: "timestamp_from_parts",
            display_name: "TIMESTAMP_FROM_PARTS",
            params: PARAMS_TIMESTAMP_FROM_PARTS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "timestamp_ltz_from_parts" => Some(FunctionSignature {
            name: "timestamp_ltz_from_parts",
            display_name: "TIMESTAMP_LTZ_FROM_PARTS",
            params: PARAMS_TIMESTAMP_LTZ_FROM_PARTS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "timestamp_tz_from_parts" => Some(FunctionSignature {
            name: "timestamp_tz_from_parts",
            display_name: "TIMESTAMP_TZ_FROM_PARTS",
            params: PARAMS_TIMESTAMP_TZ_FROM_PARTS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "timestamp_add" => Some(FunctionSignature {
            name: "timestamp_add",
            display_name: "TIMESTAMP_ADD",
            params: PARAMS_TIMESTAMP_ADD,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "timestamp_sub" => Some(FunctionSignature {
            name: "timestamp_sub",
            display_name: "TIMESTAMP_SUB",
            params: PARAMS_TIMESTAMP_SUB,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "timestamp_trunc" => Some(FunctionSignature {
            name: "timestamp_trunc",
            display_name: "TIMESTAMP_TRUNC",
            params: PARAMS_TIMESTAMP_TRUNC,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time_add" => Some(FunctionSignature {
            name: "time_add",
            display_name: "TIME_ADD",
            params: PARAMS_TIME_ADD,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time_diff" => Some(FunctionSignature {
            name: "time_diff",
            display_name: "TIME_DIFF",
            params: PARAMS_TIME_DIFF,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time_slice" => Some(FunctionSignature {
            name: "time_slice",
            display_name: "TIME_SLICE",
            params: PARAMS_TIME_SLICE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time_str_to_date" => Some(FunctionSignature {
            name: "time_str_to_date",
            display_name: "TIME_STR_TO_DATE",
            params: PARAMS_TIME_STR_TO_DATE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time_str_to_time" => Some(FunctionSignature {
            name: "time_str_to_time",
            display_name: "TIME_STR_TO_TIME",
            params: PARAMS_TIME_STR_TO_TIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time_str_to_unix" => Some(FunctionSignature {
            name: "time_str_to_unix",
            display_name: "TIME_STR_TO_UNIX",
            params: PARAMS_TIME_STR_TO_UNIX,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time_sub" => Some(FunctionSignature {
            name: "time_sub",
            display_name: "TIME_SUB",
            params: PARAMS_TIME_SUB,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time_to_str" => Some(FunctionSignature {
            name: "time_to_str",
            display_name: "TIME_TO_STR",
            params: PARAMS_TIME_TO_STR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time_to_time_str" => Some(FunctionSignature {
            name: "time_to_time_str",
            display_name: "TIME_TO_TIME_STR",
            params: PARAMS_TIME_TO_TIME_STR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time_to_unix" => Some(FunctionSignature {
            name: "time_to_unix",
            display_name: "TIME_TO_UNIX",
            params: PARAMS_TIME_TO_UNIX,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "time_trunc" => Some(FunctionSignature {
            name: "time_trunc",
            display_name: "TIME_TRUNC",
            params: PARAMS_TIME_TRUNC,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "to_array" => Some(FunctionSignature {
            name: "to_array",
            display_name: "TO_ARRAY",
            params: PARAMS_TO_ARRAY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "to_base32" => Some(FunctionSignature {
            name: "to_base32",
            display_name: "TO_BASE32",
            params: PARAMS_TO_BASE32,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "to_base64" => Some(FunctionSignature {
            name: "to_base64",
            display_name: "TO_BASE64",
            params: PARAMS_TO_BASE64,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "to_char" => Some(FunctionSignature {
            name: "to_char",
            display_name: "TO_CHAR",
            params: PARAMS_TO_CHAR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "to_code_points" => Some(FunctionSignature {
            name: "to_code_points",
            display_name: "TO_CODE_POINTS",
            params: PARAMS_TO_CODE_POINTS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "to_days" => Some(FunctionSignature {
            name: "to_days",
            display_name: "TO_DAYS",
            params: PARAMS_TO_DAYS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "to_double" => Some(FunctionSignature {
            name: "to_double",
            display_name: "TO_DOUBLE",
            params: PARAMS_TO_DOUBLE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "to_map" => Some(FunctionSignature {
            name: "to_map",
            display_name: "TO_MAP",
            params: PARAMS_TO_MAP,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "to_number" => Some(FunctionSignature {
            name: "to_number",
            display_name: "TO_NUMBER",
            params: PARAMS_TO_NUMBER,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "transform" => Some(FunctionSignature {
            name: "transform",
            display_name: "TRANSFORM",
            params: PARAMS_TRANSFORM,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "translate" => Some(FunctionSignature {
            name: "translate",
            display_name: "TRANSLATE",
            params: PARAMS_TRANSLATE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "trim" => Some(FunctionSignature {
            name: "trim",
            display_name: "TRIM",
            params: PARAMS_TRIM,
            return_type: Some(ReturnTypeRule::Text),
            category: FunctionCategory::Scalar,
        }),
        "try" => Some(FunctionSignature {
            name: "try",
            display_name: "TRY",
            params: PARAMS_TRY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "try_base64decode_binary" => Some(FunctionSignature {
            name: "try_base64decode_binary",
            display_name: "TRY_BASE64DECODE_BINARY",
            params: PARAMS_TRY_BASE64DECODE_BINARY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "try_base64decode_string" => Some(FunctionSignature {
            name: "try_base64decode_string",
            display_name: "TRY_BASE64DECODE_STRING",
            params: PARAMS_TRY_BASE64DECODE_STRING,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "try_cast" => Some(FunctionSignature {
            name: "try_cast",
            display_name: "TRY_CAST",
            params: PARAMS_TRY_CAST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "try_hex_decode_binary" => Some(FunctionSignature {
            name: "try_hex_decode_binary",
            display_name: "TRY_HEX_DECODE_BINARY",
            params: PARAMS_TRY_HEX_DECODE_BINARY,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "try_hex_decode_string" => Some(FunctionSignature {
            name: "try_hex_decode_string",
            display_name: "TRY_HEX_DECODE_STRING",
            params: PARAMS_TRY_HEX_DECODE_STRING,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ts_or_di_to_di" => Some(FunctionSignature {
            name: "ts_or_di_to_di",
            display_name: "TS_OR_DI_TO_DI",
            params: PARAMS_TS_OR_DI_TO_DI,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ts_or_ds_add" => Some(FunctionSignature {
            name: "ts_or_ds_add",
            display_name: "TS_OR_DS_ADD",
            params: PARAMS_TS_OR_DS_ADD,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ts_or_ds_diff" => Some(FunctionSignature {
            name: "ts_or_ds_diff",
            display_name: "TS_OR_DS_DIFF",
            params: PARAMS_TS_OR_DS_DIFF,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ts_or_ds_to_date" => Some(FunctionSignature {
            name: "ts_or_ds_to_date",
            display_name: "TS_OR_DS_TO_DATE",
            params: PARAMS_TS_OR_DS_TO_DATE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ts_or_ds_to_datetime" => Some(FunctionSignature {
            name: "ts_or_ds_to_datetime",
            display_name: "TS_OR_DS_TO_DATETIME",
            params: PARAMS_TS_OR_DS_TO_DATETIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ts_or_ds_to_date_str" => Some(FunctionSignature {
            name: "ts_or_ds_to_date_str",
            display_name: "TS_OR_DS_TO_DATE_STR",
            params: PARAMS_TS_OR_DS_TO_DATE_STR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ts_or_ds_to_time" => Some(FunctionSignature {
            name: "ts_or_ds_to_time",
            display_name: "TS_OR_DS_TO_TIME",
            params: PARAMS_TS_OR_DS_TO_TIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "ts_or_ds_to_timestamp" => Some(FunctionSignature {
            name: "ts_or_ds_to_timestamp",
            display_name: "TS_OR_DS_TO_TIMESTAMP",
            params: PARAMS_TS_OR_DS_TO_TIMESTAMP,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "typeof" => Some(FunctionSignature {
            name: "typeof",
            display_name: "TYPEOF",
            params: PARAMS_TYPEOF,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "upper" => Some(FunctionSignature {
            name: "upper",
            display_name: "UPPER",
            params: PARAMS_UPPER,
            return_type: Some(ReturnTypeRule::Text),
            category: FunctionCategory::Scalar,
        }),
        "unhex" => Some(FunctionSignature {
            name: "unhex",
            display_name: "UNHEX",
            params: PARAMS_UNHEX,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "unicode" => Some(FunctionSignature {
            name: "unicode",
            display_name: "UNICODE",
            params: PARAMS_UNICODE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "unix_date" => Some(FunctionSignature {
            name: "unix_date",
            display_name: "UNIX_DATE",
            params: PARAMS_UNIX_DATE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "unix_micros" => Some(FunctionSignature {
            name: "unix_micros",
            display_name: "UNIX_MICROS",
            params: PARAMS_UNIX_MICROS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "unix_millis" => Some(FunctionSignature {
            name: "unix_millis",
            display_name: "UNIX_MILLIS",
            params: PARAMS_UNIX_MILLIS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "unix_seconds" => Some(FunctionSignature {
            name: "unix_seconds",
            display_name: "UNIX_SECONDS",
            params: PARAMS_UNIX_SECONDS,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "unix_to_str" => Some(FunctionSignature {
            name: "unix_to_str",
            display_name: "UNIX_TO_STR",
            params: PARAMS_UNIX_TO_STR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "unix_to_time" => Some(FunctionSignature {
            name: "unix_to_time",
            display_name: "UNIX_TO_TIME",
            params: PARAMS_UNIX_TO_TIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "unix_to_time_str" => Some(FunctionSignature {
            name: "unix_to_time_str",
            display_name: "UNIX_TO_TIME_STR",
            params: PARAMS_UNIX_TO_TIME_STR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "unnest" => Some(FunctionSignature {
            name: "unnest",
            display_name: "UNNEST",
            params: PARAMS_UNNEST,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "utc_date" => Some(FunctionSignature {
            name: "utc_date",
            display_name: "UTC_DATE",
            params: &[],
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "utc_time" => Some(FunctionSignature {
            name: "utc_time",
            display_name: "UTC_TIME",
            params: PARAMS_UTC_TIME,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "utc_timestamp" => Some(FunctionSignature {
            name: "utc_timestamp",
            display_name: "UTC_TIMESTAMP",
            params: PARAMS_UTC_TIMESTAMP,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "variance" => Some(FunctionSignature {
            name: "variance",
            display_name: "VARIANCE",
            params: PARAMS_VARIANCE,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "variance_pop" => Some(FunctionSignature {
            name: "variance_pop",
            display_name: "VARIANCE_POP",
            params: PARAMS_VARIANCE_POP,
            return_type: None,
            category: FunctionCategory::Aggregate,
        }),
        "var_map" => Some(FunctionSignature {
            name: "var_map",
            display_name: "VAR_MAP",
            params: PARAMS_VAR_MAP,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "vector_search" => Some(FunctionSignature {
            name: "vector_search",
            display_name: "VECTOR_SEARCH",
            params: PARAMS_VECTOR_SEARCH,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "week" => Some(FunctionSignature {
            name: "week",
            display_name: "WEEK",
            params: PARAMS_WEEK,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "week_of_year" => Some(FunctionSignature {
            name: "week_of_year",
            display_name: "WEEK_OF_YEAR",
            params: PARAMS_WEEK_OF_YEAR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "width_bucket" => Some(FunctionSignature {
            name: "width_bucket",
            display_name: "WIDTH_BUCKET",
            params: PARAMS_WIDTH_BUCKET,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "xml_element" => Some(FunctionSignature {
            name: "xml_element",
            display_name: "XML_ELEMENT",
            params: PARAMS_XML_ELEMENT,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "xor" => Some(FunctionSignature {
            name: "xor",
            display_name: "XOR",
            params: PARAMS_XOR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "xml_table" => Some(FunctionSignature {
            name: "xml_table",
            display_name: "XML_TABLE",
            params: PARAMS_XML_TABLE,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "year" => Some(FunctionSignature {
            name: "year",
            display_name: "YEAR",
            params: PARAMS_YEAR,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "year_of_week" => Some(FunctionSignature {
            name: "year_of_week",
            display_name: "YEAR_OF_WEEK",
            params: PARAMS_YEAR_OF_WEEK,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        "year_of_week_iso" => Some(FunctionSignature {
            name: "year_of_week_iso",
            display_name: "YEAR_OF_WEEK_ISO",
            params: PARAMS_YEAR_OF_WEEK_ISO,
            return_type: None,
            category: FunctionCategory::Scalar,
        }),
        _ => None,
    }
}

/// Returns all function signatures for completion.
///
/// This provides access to all known SQL functions for populating
/// completion lists. Functions are returned in a static slice for efficiency.
pub fn all_function_signatures() -> impl Iterator<Item = FunctionSignature> {
    static NAMES: &[&str] = &[
        "abs",
        "acos",
        "acosh",
        "add_months",
        "agg_func",
        "ai_agg",
        "ai_classify",
        "ai_summarize_agg",
        "and",
        "any_value",
        "apply",
        "approximate_similarity",
        "approx_distinct",
        "approx_quantile",
        "approx_quantiles",
        "approx_top_k",
        "approx_top_k_accumulate",
        "approx_top_k_combine",
        "approx_top_sum",
        "arg_max",
        "arg_min",
        "array",
        "array_agg",
        "array_all",
        "array_any",
        "array_concat",
        "array_concat_agg",
        "array_construct_compact",
        "array_contains",
        "array_contains_all",
        "array_filter",
        "array_first",
        "array_intersect",
        "array_to_string",
        "array_last",
        "array_size",
        "array_overlaps",
        "array_remove",
        "array_reverse",
        "array_slice",
        "array_sort",
        "array_sum",
        "array_union_agg",
        "array_unique_agg",
        "ascii",
        "asin",
        "asinh",
        "atan",
        "atan2",
        "atanh",
        "avg",
        "base64decode_binary",
        "base64decode_string",
        "base64encode",
        "bitmap_bit_position",
        "bitmap_bucket_number",
        "bitmap_construct_agg",
        "bitmap_count",
        "bitmap_or_agg",
        "bitwise_and_agg",
        "bitwise_count",
        "bitwise_or_agg",
        "bitwise_xor_agg",
        "bit_length",
        "booland",
        "logical_and",
        "boolnot",
        "boolor",
        "logical_or",
        "boolxor_agg",
        "byte_length",
        "case",
        "cast",
        "cast_to_str_type",
        "cbrt",
        "ceil",
        "chr",
        "length",
        "coalesce",
        "code_points_to_bytes",
        "code_points_to_string",
        "collate",
        "collation",
        "columns",
        "combined_agg_func",
        "combined_parameterized_agg",
        "compress",
        "concat",
        "concat_ws",
        "connect_by_root",
        "contains",
        "convert",
        "convert_timezone",
        "convert_to_charset",
        "corr",
        "cos",
        "cosh",
        "cosine_distance",
        "cot",
        "coth",
        "count",
        "count_if",
        "covar_pop",
        "covar_samp",
        "csc",
        "csch",
        "cume_dist",
        "current_date",
        "current_datetime",
        "current_schema",
        "current_time",
        "current_timestamp",
        "current_timestamp_ltz",
        "current_user",
        "date",
        "date_diff",
        "date_from_parts",
        "datetime",
        "datetime_add",
        "datetime_diff",
        "datetime_sub",
        "datetime_trunc",
        "date_add",
        "date_bin",
        "date_from_unix_date",
        "date_str_to_date",
        "date_sub",
        "date_to_date_str",
        "date_to_di",
        "date_trunc",
        "day",
        "day_of_month",
        "day_of_week",
        "day_of_week_iso",
        "day_of_year",
        "decode",
        "decode_case",
        "decompress_binary",
        "decompress_string",
        "degrees",
        "dense_rank",
        "di_to_date",
        "encode",
        "ends_with",
        "equal_null",
        "euclidean_distance",
        "exists",
        "exp",
        "explode",
        "explode_outer",
        "exploding_generate_series",
        "extract",
        "factorial",
        "farm_fingerprint",
        "features_at_time",
        "first",
        "first_value",
        "flatten",
        "float64",
        "floor",
        "format",
        "from_base",
        "from_base32",
        "from_base64",
        "from_iso8601timestamp",
        "gap_fill",
        "generate_date_array",
        "generate_embedding",
        "generate_series",
        "generate_timestamp_array",
        "uuid",
        "getbit",
        "get_extract",
        "greatest",
        "greatest_ignore_nulls",
        "grouping",
        "grouping_id",
        "group_concat",
        "hex",
        "hex_decode_string",
        "hex_encode",
        "hll",
        "hour",
        "if",
        "initcap",
        "inline",
        "stuff",
        "int64",
        "is_inf",
        "is_nan",
        "is_ascii",
        "is_null_value",
        "jarowinkler_similarity",
        "jsonb_contains",
        "jsonb_exists",
        "jsonb_extract",
        "jsonb_extract_scalar",
        "json_array_append",
        "json_array_contains",
        "json_array_insert",
        "json_extract",
        "json_extract_array",
        "json_extract_scalar",
        "json_format",
        "parse_json",
        "json_remove",
        "json_set",
        "json_strip_nulls",
        "json_type",
        "justify_days",
        "justify_hours",
        "justify_interval",
        "json_array",
        "json_array_agg",
        "json_bool",
        "jsonb_contains_all_top_keys",
        "jsonb_contains_any_top_keys",
        "jsonb_delete_at_path",
        "jsonb_object_agg",
        "json_cast",
        "json_exists",
        "json_keys_at_depth",
        "json_object",
        "json_object_agg",
        "json_table",
        "json_value_array",
        "lag",
        "last",
        "last_day",
        "last_value",
        "lax_bool",
        "lax_float64",
        "lax_int64",
        "lax_string",
        "lower",
        "lead",
        "least",
        "least_ignore_nulls",
        "left",
        "levenshtein",
        "list",
        "ln",
        "log",
        "lower_hex",
        "make_interval",
        "map",
        "map_from_entries",
        "match_against",
        "max",
        "md5",
        "md5digest",
        "median",
        "min",
        "minhash",
        "minhash_combine",
        "minute",
        "month",
        "monthname",
        "months_between",
        "md5number_lower64",
        "md5number_upper64",
        "ml_forecast",
        "ml_translate",
        "next_day",
        "next_value_for",
        "normalize",
        "now",
        "nth_value",
        "ntile",
        "nullif",
        "number_to_str",
        "nvl2",
        "object_agg",
        "object_insert",
        "open_json",
        "or",
        "overlay",
        "pad",
        "parameterized_agg",
        "parse_bignumeric",
        "parse_datetime",
        "parse_ip",
        "parse_numeric",
        "parse_time",
        "parse_url",
        "percentile_cont",
        "percentile_disc",
        "percent_rank",
        "pi",
        "posexplode",
        "posexplode_outer",
        "pow",
        "predict",
        "previous_day",
        "quantile",
        "quarter",
        "radians",
        "rand",
        "randn",
        "range_bucket",
        "range_n",
        "rank",
        "read_csv",
        "read_parquet",
        "reduce",
        "regexp_count",
        "regexp_extract",
        "regexp_extract_all",
        "regexp_full_match",
        "regexp_instr",
        "regexp_i_like",
        "regexp_like",
        "regexp_replace",
        "regexp_split",
        "regr_avgx",
        "regr_avgy",
        "regr_valx",
        "regr_valy",
        "repeat",
        "replace",
        "reverse",
        "right",
        "round",
        "row_number",
        "rtrimmed_length",
        "safe_add",
        "safe_convert_bytes_to_string",
        "safe_divide",
        "safe_multiply",
        "safe_negate",
        "safe_subtract",
        "search",
        "sec",
        "sech",
        "second",
        "sha",
        "sha2",
        "sign",
        "sin",
        "sinh",
        "skewness",
        "sort_array",
        "soundex",
        "soundex_p123",
        "space",
        "split",
        "string_to_array",
        "split_part",
        "sqrt",
        "standard_hash",
        "starts_with",
        "star_map",
        "stddev",
        "stddev_pop",
        "stddev_samp",
        "string",
        "struct",
        "struct_extract",
        "str_position",
        "str_to_date",
        "str_to_map",
        "str_to_time",
        "str_to_unix",
        "st_distance",
        "st_point",
        "substring",
        "substring_index",
        "sum",
        "sha1digest",
        "sha2digest",
        "tan",
        "tanh",
        "time",
        "time_from_parts",
        "timestamp",
        "timestamp_diff",
        "timestamp_from_parts",
        "timestamp_ltz_from_parts",
        "timestamp_tz_from_parts",
        "timestamp_add",
        "timestamp_sub",
        "timestamp_trunc",
        "time_add",
        "time_diff",
        "time_slice",
        "time_str_to_date",
        "time_str_to_time",
        "time_str_to_unix",
        "time_sub",
        "time_to_str",
        "time_to_time_str",
        "time_to_unix",
        "time_trunc",
        "to_array",
        "to_base32",
        "to_base64",
        "to_char",
        "to_code_points",
        "to_days",
        "to_double",
        "to_map",
        "to_number",
        "transform",
        "translate",
        "trim",
        "try",
        "try_base64decode_binary",
        "try_base64decode_string",
        "try_cast",
        "try_hex_decode_binary",
        "try_hex_decode_string",
        "ts_or_di_to_di",
        "ts_or_ds_add",
        "ts_or_ds_diff",
        "ts_or_ds_to_date",
        "ts_or_ds_to_datetime",
        "ts_or_ds_to_date_str",
        "ts_or_ds_to_time",
        "ts_or_ds_to_timestamp",
        "typeof",
        "upper",
        "unhex",
        "unicode",
        "unix_date",
        "unix_micros",
        "unix_millis",
        "unix_seconds",
        "unix_to_str",
        "unix_to_time",
        "unix_to_time_str",
        "unnest",
        "utc_date",
        "utc_time",
        "utc_timestamp",
        "variance",
        "variance_pop",
        "var_map",
        "vector_search",
        "week",
        "week_of_year",
        "width_bucket",
        "xml_element",
        "xor",
        "xml_table",
        "year",
        "year_of_week",
        "year_of_week_iso",
    ];
    NAMES.iter().filter_map(|name| get_function_signature(name))
}
