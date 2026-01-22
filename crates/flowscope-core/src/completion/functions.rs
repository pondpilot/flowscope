//! Smart function completions for SQL.
//!
//! This module provides function completion items with full signatures,
//! return types, and context-aware filtering based on clause type.
//!
//! # Performance
//!
//! Function completion items are pre-computed and cached at startup using
//! `LazyLock`. The cache contains one `CompletionItem` per SQL function (450+).
//! On each completion request, we filter and clone items from the cache,
//! avoiding the cost of re-computing signatures and formatting strings.

use std::sync::LazyLock;

use crate::generated::{
    all_function_signatures, get_function_signature, FunctionCategory, FunctionSignature,
    ReturnTypeRule,
};
use crate::types::{CompletionClause, CompletionItem, CompletionItemCategory, CompletionItemKind};

/// Cached function completion items, pre-computed at startup.
/// Each item contains the base CompletionItem with score=0, ready to be cloned
/// and have context-based scoring applied.
static FUNCTION_COMPLETION_CACHE: LazyLock<Vec<CachedFunctionItem>> = LazyLock::new(|| {
    all_function_signatures()
        .map(|sig| CachedFunctionItem {
            item: function_to_completion_item(&sig),
            name_lower: sig.name.to_string(),
            category: sig.category,
        })
        .collect()
});

/// Cached function item with additional metadata for filtering.
struct CachedFunctionItem {
    /// Pre-computed completion item (with score=0)
    item: CompletionItem,
    /// Lowercase function name for prefix matching
    name_lower: String,
    /// Function category for context-based scoring
    category: FunctionCategory,
}

/// Score bonus for aggregate functions when in GROUP BY context (has_group_by = true)
const SCORE_AGGREGATE_IN_GROUP_BY_CONTEXT: i32 = 200;
/// Score penalty for aggregate functions outside GROUP BY context
const SCORE_AGGREGATE_NO_GROUP_BY: i32 = -100;
/// Score bonus for window functions in OVER/WINDOW context
const SCORE_WINDOW_IN_WINDOW_CONTEXT: i32 = 150;
/// Score penalty for aggregates in WHERE clause (usually invalid)
const SCORE_AGGREGATE_IN_WHERE_PENALTY: i32 = -300;
/// Functions that behave like bare keywords (no parentheses in default form)
const KEYWORD_STYLE_FUNCTIONS: &[&str] = &[
    "current_catalog",
    "current_date",
    "current_datetime",
    "current_database",
    "current_path",
    "current_role",
    "current_schema",
    "current_session",
    "current_time",
    "current_timestamp",
    "current_timestamp_ltz",
    "current_timestamp_ntz",
    "current_timestamp_tz",
    "current_user",
    "localtime",
    "localtimestamp",
    "session_user",
    "system_user",
    "user",
];

fn uses_keyword_call_style(sig: &FunctionSignature) -> bool {
    KEYWORD_STYLE_FUNCTIONS
        .iter()
        .any(|name| sig.name.eq_ignore_ascii_case(name))
}

/// Converts a function signature to a completion item.
///
/// The completion item includes:
/// - Label: Function name in uppercase
/// - Insert text: Function name with opening parenthesis
/// - Detail: Full signature with parameters and return type
/// - Category: Based on function classification (aggregate, window, scalar)
pub fn function_to_completion_item(sig: &FunctionSignature) -> CompletionItem {
    let category = match sig.category {
        FunctionCategory::Aggregate => CompletionItemCategory::Aggregate,
        FunctionCategory::Window | FunctionCategory::Scalar => CompletionItemCategory::Function,
    };

    // Format detail as signature: "NAME(params) → TYPE"
    let detail = Some(sig.format_signature());

    CompletionItem {
        label: sig.display_name.to_string(),
        insert_text: if uses_keyword_call_style(sig) {
            sig.display_name.to_string()
        } else {
            format!("{}(", sig.display_name)
        },
        kind: CompletionItemKind::Function,
        category,
        score: 0, // Will be adjusted by context scoring
        clause_specific: false,
        detail,
    }
}

/// Returns the return type display string for a function, if known.
pub fn function_return_type_display(name: &str) -> Option<&'static str> {
    get_function_signature(name).and_then(|sig| {
        sig.return_type.map(|rt| match rt {
            ReturnTypeRule::Integer => "INTEGER",
            ReturnTypeRule::Numeric => "NUMERIC",
            ReturnTypeRule::Text => "TEXT",
            ReturnTypeRule::Timestamp => "TIMESTAMP",
            ReturnTypeRule::Boolean => "BOOLEAN",
            ReturnTypeRule::Date => "DATE",
            ReturnTypeRule::MatchFirstArg => "T",
        })
    })
}

/// Context for function completion filtering and scoring.
#[derive(Debug, Clone, Default)]
pub struct FunctionCompletionContext {
    /// Current SQL clause
    pub clause: CompletionClause,
    /// Whether the query has a GROUP BY clause
    pub has_group_by: bool,
    /// Whether we're in an OVER clause (window context)
    pub in_window_context: bool,
    /// Optional prefix filter
    pub prefix: Option<String>,
}

/// Gets function completions filtered and scored by context.
///
/// This function returns SQL functions as completion items, filtered by prefix
/// and with scoring adjustments based on the current context:
/// - Aggregate functions are boosted when GROUP BY is present
/// - Aggregate functions are penalized in WHERE clause
/// - Window functions are boosted in OVER/WINDOW context
///
/// # Performance
///
/// Uses a pre-computed cache of function items. On each call, we:
/// 1. Filter the cache by prefix (if provided)
/// 2. Clone matching items
/// 3. Apply context-based scoring
///
/// This avoids re-computing signatures and format strings on every request.
pub fn get_function_completions(ctx: &FunctionCompletionContext) -> Vec<CompletionItem> {
    let prefix_lower = ctx.prefix.as_ref().map(|p| p.to_ascii_lowercase());

    FUNCTION_COMPLETION_CACHE
        .iter()
        .filter(|cached| {
            // Apply prefix filter if present
            match &prefix_lower {
                Some(prefix) => cached.name_lower.starts_with(prefix.as_str()),
                None => true,
            }
        })
        .map(|cached| {
            let mut item = cached.item.clone();

            // Apply context-based scoring adjustments
            let score_adjustment =
                compute_function_score_adjustment_by_category(cached.category, ctx);
            item.score = score_adjustment;

            // Mark as clause-specific if we boosted it for the current context
            if score_adjustment > 0 {
                item.clause_specific = true;
            }

            item
        })
        .collect()
}

/// Computes score adjustment for a function category based on completion context.
fn compute_function_score_adjustment_by_category(
    category: FunctionCategory,
    ctx: &FunctionCompletionContext,
) -> i32 {
    let mut adjustment = 0;

    match category {
        FunctionCategory::Aggregate => {
            // Aggregates in GROUP BY context get a boost
            if ctx.has_group_by {
                adjustment += SCORE_AGGREGATE_IN_GROUP_BY_CONTEXT;
            } else {
                adjustment += SCORE_AGGREGATE_NO_GROUP_BY;
            }

            // Aggregates in WHERE clause are usually invalid (except in subqueries)
            if ctx.clause == CompletionClause::Where {
                adjustment += SCORE_AGGREGATE_IN_WHERE_PENALTY;
            }

            // Aggregates in HAVING get a significant boost
            if ctx.clause == CompletionClause::Having {
                adjustment += SCORE_AGGREGATE_IN_GROUP_BY_CONTEXT;
            }
        }
        FunctionCategory::Window => {
            // Window functions in window context get a boost
            if ctx.in_window_context || ctx.clause == CompletionClause::Window {
                adjustment += SCORE_WINDOW_IN_WINDOW_CONTEXT;
            }
        }
        FunctionCategory::Scalar => {
            // Scalar functions are generally always valid
            // No special adjustment needed
        }
    }

    adjustment
}

/// Returns true if the function is an aggregate function.
pub fn is_aggregate(name: &str) -> bool {
    get_function_signature(name)
        .map(|sig| sig.category == FunctionCategory::Aggregate)
        .unwrap_or(false)
}

/// Returns true if the function is a window function.
pub fn is_window(name: &str) -> bool {
    get_function_signature(name)
        .map(|sig| sig.category == FunctionCategory::Window)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_to_completion_item() {
        let sig = get_function_signature("count").expect("COUNT should exist");
        let item = function_to_completion_item(&sig);

        assert_eq!(item.label, "COUNT");
        assert_eq!(item.insert_text, "COUNT(");
        assert_eq!(item.kind, CompletionItemKind::Function);
        assert_eq!(item.category, CompletionItemCategory::Aggregate);
        assert!(item.detail.is_some());
    }

    #[test]
    fn test_keyword_function_inserts_plain_identifier() {
        let sig = get_function_signature("current_date").expect("CURRENT_DATE should exist");
        let item = function_to_completion_item(&sig);
        assert_eq!(item.insert_text, "CURRENT_DATE");
    }

    #[test]
    fn test_zero_arg_regular_function_still_opens_parenthesis() {
        let sig = get_function_signature("pi").expect("PI should exist");
        let item = function_to_completion_item(&sig);
        assert_eq!(item.insert_text, "PI(");
    }

    #[test]
    fn test_function_completion_with_return_type() {
        let sig = get_function_signature("count").expect("COUNT should exist");
        let formatted = sig.format_signature();

        // COUNT should show INTEGER return type
        assert!(
            formatted.contains("INTEGER"),
            "Expected INTEGER in signature: {}",
            formatted
        );
    }

    #[test]
    fn test_aggregate_boosted_with_group_by() {
        let ctx = FunctionCompletionContext {
            clause: CompletionClause::Select,
            has_group_by: true,
            in_window_context: false,
            prefix: Some("sum".to_string()),
        };

        let items = get_function_completions(&ctx);
        let sum_item = items.iter().find(|i| i.label == "SUM");

        assert!(sum_item.is_some(), "SUM should be in completions");
        let sum = sum_item.unwrap();
        assert!(
            sum.score > 0,
            "SUM should have positive score with GROUP BY"
        );
    }

    #[test]
    fn test_aggregate_penalized_in_where() {
        let ctx = FunctionCompletionContext {
            clause: CompletionClause::Where,
            has_group_by: false,
            in_window_context: false,
            prefix: Some("sum".to_string()),
        };

        let items = get_function_completions(&ctx);
        let sum_item = items.iter().find(|i| i.label == "SUM");

        assert!(sum_item.is_some(), "SUM should still appear in completions");
        let sum = sum_item.unwrap();
        assert!(
            sum.score < 0,
            "SUM should have negative score in WHERE clause"
        );
    }

    #[test]
    fn test_prefix_filtering() {
        let ctx = FunctionCompletionContext {
            clause: CompletionClause::Select,
            has_group_by: false,
            in_window_context: false,
            prefix: Some("row_".to_string()),
        };

        let items = get_function_completions(&ctx);

        // Should only include functions starting with "row_"
        assert!(items.iter().all(|i| i.label.starts_with("ROW_")));
        assert!(items.iter().any(|i| i.label == "ROW_NUMBER"));
    }

    #[test]
    fn test_window_function_in_window_context() {
        let ctx = FunctionCompletionContext {
            clause: CompletionClause::Window,
            has_group_by: false,
            in_window_context: true,
            prefix: Some("row_".to_string()),
        };

        let items = get_function_completions(&ctx);
        let row_number = items.iter().find(|i| i.label == "ROW_NUMBER");

        assert!(row_number.is_some());
        assert!(
            row_number.unwrap().score > 0,
            "ROW_NUMBER should have positive score in window context"
        );
    }

    #[test]
    fn test_function_signature_parameter_order_preserved() {
        let sig = get_function_signature("substring").expect("SUBSTRING should exist");
        let names: Vec<_> = sig.params.iter().map(|p| p.name).collect();

        assert_eq!(names, vec!["this", "start", "length"]);
        assert!(sig.params[0].required);
        assert!(!sig.params[1].required);
        assert!(!sig.params[2].required);
    }
}
