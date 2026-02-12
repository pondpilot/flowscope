//! Shared pattern detection helpers for lint rules and auto-fix.
//!
//! These helpers are used by both the lint rule implementations (detection)
//! and the CLI auto-fixer (rewriting), ensuring consistent pattern matching.

use sqlparser::ast::*;

/// Simple structural equality check for expressions via their Display output.
pub fn exprs_equal(a: &Expr, b: &Expr) -> bool {
    format!("{a}") == format!("{b}")
}

/// Returns true if the expression is a NULL literal.
pub fn is_null_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Value(ValueWithSpan {
            value: Value::Null,
            ..
        })
    )
}

/// Checks if an expression is a COALESCE-equivalent CASE pattern:
/// `CASE WHEN x IS NULL THEN y ELSE x END`
///
/// Returns true for detection (lint rules), or use [`coalesce_replacement`]
/// to extract the components for rewriting.
pub fn is_coalesce_pattern(expr: &Expr) -> bool {
    coalesce_replacement(expr).is_some()
}

/// If the expression matches `CASE WHEN x IS NULL THEN y ELSE x END`,
/// returns `Some((x, y))` for rewriting to `COALESCE(x, y)`.
pub fn coalesce_replacement(expr: &Expr) -> Option<(Expr, Expr)> {
    if let Expr::Case {
        operand: None,
        conditions,
        else_result: Some(else_expr),
        ..
    } = expr
    {
        if conditions.len() == 1 {
            if let Expr::IsNull(check_expr) = &conditions[0].condition {
                if exprs_equal(check_expr, else_expr) {
                    return Some((*check_expr.clone(), conditions[0].result.clone()));
                }
            }
        }
    }

    None
}
