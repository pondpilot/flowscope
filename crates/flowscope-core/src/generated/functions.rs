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
