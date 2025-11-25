use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Defines how to handle function arguments for lineage analysis.
/// Some functions have keyword arguments that look like identifiers but shouldn't
/// be treated as column references (e.g., DATEDIFF(day, ...) where 'day' is a unit keyword).
#[derive(Debug, Clone)]
pub(crate) struct FunctionArgRule {
    /// Argument indices (0-based) that should be skipped during column reference extraction
    pub(crate) skip_indices: HashSet<usize>,
}

impl FunctionArgRule {
    pub(crate) fn skip(indices: &[usize]) -> Self {
        Self {
            skip_indices: indices.iter().copied().collect(),
        }
    }
}

/// Registry of special function argument handling rules.
/// Maps lowercase function name to its argument handling rules.
pub(crate) static FUNCTION_ARG_RULES: LazyLock<HashMap<&'static str, FunctionArgRule>> =
    LazyLock::new(|| {
        let mut rules = HashMap::new();

        // Date/time functions with unit keyword as first argument
        // DATEDIFF(day, start_date, end_date)
        rules.insert("datediff", FunctionArgRule::skip(&[0]));

        // DATE_DIFF(start_date, end_date, DAY) - BigQuery style, last arg is unit
        rules.insert("date_diff", FunctionArgRule::skip(&[2]));

        // DATEADD(day, number, date)
        rules.insert("dateadd", FunctionArgRule::skip(&[0]));

        // DATE_ADD(date, INTERVAL n DAY) - handled differently in AST usually

        // DATEPART(day, date)
        rules.insert("datepart", FunctionArgRule::skip(&[0]));

        // DATE_PART('day', date) - usually quoted string, but cover unquoted
        rules.insert("date_part", FunctionArgRule::skip(&[0]));

        // TIMESTAMPDIFF(unit, start, end)
        rules.insert("timestampdiff", FunctionArgRule::skip(&[0]));

        // TIMESTAMPADD(unit, amount, timestamp)
        rules.insert("timestampadd", FunctionArgRule::skip(&[0]));

        // DATE_TRUNC(unit, date) or DATE_TRUNC('unit', date)
        rules.insert("date_trunc", FunctionArgRule::skip(&[0]));

        // TRUNC(date, 'unit') - Oracle style
        rules.insert("trunc", FunctionArgRule::skip(&[1]));

        // EXTRACT is usually handled separately in AST (Expr::Extract)
        // but some dialects might parse it as a function

        // TIME_BUCKET(interval, timestamp) - TimescaleDB
        // First arg is interval literal, usually not a column ref

        rules
    });

/// Check if a function argument at a given index should be skipped for column reference extraction.
pub(crate) fn should_skip_function_arg(func_name: &str, arg_index: usize) -> bool {
    let func_lower = func_name.to_lowercase();
    if let Some(rule) = FUNCTION_ARG_RULES.get(func_lower.as_str()) {
        return rule.skip_indices.contains(&arg_index);
    }
    false
}

/// Set of known SQL aggregate functions.
/// These functions collapse multiple rows into a single result (1:many → 1).
pub(crate) static AGGREGATE_FUNCTIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut funcs = HashSet::new();

    // Standard SQL aggregate functions
    funcs.insert("count");
    funcs.insert("sum");
    funcs.insert("avg");
    funcs.insert("min");
    funcs.insert("max");

    // Statistical aggregates
    funcs.insert("stddev");
    funcs.insert("stddev_pop");
    funcs.insert("stddev_samp");
    funcs.insert("variance");
    funcs.insert("var_pop");
    funcs.insert("var_samp");
    funcs.insert("covar_pop");
    funcs.insert("covar_samp");
    funcs.insert("corr");
    funcs.insert("regr_slope");
    funcs.insert("regr_intercept");
    funcs.insert("regr_count");
    funcs.insert("regr_r2");
    funcs.insert("regr_avgx");
    funcs.insert("regr_avgy");
    funcs.insert("regr_sxx");
    funcs.insert("regr_syy");
    funcs.insert("regr_sxy");

    // Array/List aggregates
    funcs.insert("array_agg");
    funcs.insert("list_agg");
    funcs.insert("listagg");
    funcs.insert("string_agg");
    funcs.insert("group_concat");

    // JSON aggregates
    funcs.insert("json_agg");
    funcs.insert("jsonb_agg");
    funcs.insert("json_object_agg");
    funcs.insert("jsonb_object_agg");

    // Boolean aggregates
    funcs.insert("bool_and");
    funcs.insert("bool_or");
    funcs.insert("every");

    // Bit aggregates
    funcs.insert("bit_and");
    funcs.insert("bit_or");
    funcs.insert("bit_xor");

    // Other common aggregates
    funcs.insert("any_value");
    funcs.insert("first");
    funcs.insert("last");
    funcs.insert("first_value");
    funcs.insert("last_value");
    funcs.insert("median");
    funcs.insert("mode");
    funcs.insert("percentile_cont");
    funcs.insert("percentile_disc");
    funcs.insert("approx_count_distinct");
    funcs.insert("approx_percentile");
    funcs.insert("hll_count");
    funcs.insert("hyperloglog");

    // BigQuery specific
    funcs.insert("countif");
    funcs.insert("logical_and");
    funcs.insert("logical_or");

    // Snowflake specific
    funcs.insert("bitand_agg");
    funcs.insert("bitor_agg");
    funcs.insert("bitxor_agg");
    funcs.insert("booland_agg");
    funcs.insert("boolor_agg");

    funcs
});

/// Check if a function name is a known aggregate function.
pub(crate) fn is_aggregate_function(func_name: &str) -> bool {
    let func_lower = func_name.to_lowercase();
    AGGREGATE_FUNCTIONS.contains(func_lower.as_str())
}

/// Information about an aggregate function call found in an expression.
#[derive(Debug, Clone)]
pub(crate) struct AggregateCall {
    /// The aggregate function name (uppercase, e.g., "SUM", "COUNT")
    pub(crate) function: String,
    /// Whether DISTINCT was specified
    pub(crate) distinct: bool,
}
