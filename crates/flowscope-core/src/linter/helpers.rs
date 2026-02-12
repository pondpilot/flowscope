//! Shared lint helper utilities.

use sqlparser::ast::*;

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
