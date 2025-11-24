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
