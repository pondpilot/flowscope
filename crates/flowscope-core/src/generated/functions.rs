//! Function classification sets.
//!
//! Generated from functions.json

use std::collections::HashSet;
use std::sync::LazyLock;

/// Aggregate functions (57 total).
pub static AGGREGATE_FUNCTIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    set.insert("agg_func");
    set.insert("ai_agg");
    set.insert("ai_summarize_agg");
    set.insert("any_value");
    set.insert("approx_count_distinct");
    set.insert("approx_quantile");
    set.insert("approx_quantiles");
    set.insert("approx_top_k");
    set.insert("approx_top_k_accumulate");
    set.insert("approx_top_k_combine");
    set.insert("approx_top_sum");
    set.insert("approximate_jaccard_index");
    set.insert("argmax");
    set.insert("argmin");
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
    set.insert("booland_agg");
    set.insert("boolor_agg");
    set.insert("boolxor_agg");
    set.insert("combined_agg_func");
    set.insert("combined_parameterized_agg");
    set.insert("corr");
    set.insert("count");
    set.insert("countif");
    set.insert("covar_pop");
    set.insert("covar_samp");
    set.insert("first");
    set.insert("group_concat");
    set.insert("grouping");
    set.insert("grouping_id");
    set.insert("hll");
    set.insert("j_s_o_n_b_object_agg");
    set.insert("j_s_o_n_object_agg");
    set.insert("last");
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

/// Check if a function is an aggregate function.
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
