//! Type inference utilities for SQL expressions.
//!
//! This module provides basic type inference for SQL expressions, attempting to
//! determine the data type of columns and expressions based on their structure.

use sqlparser::ast::{self as ast, Expr};

/// Basic type inference for expressions
pub fn infer_expr_type(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Value(val) => match val {
            ast::Value::Number(_, _) => Some("NUMBER".to_string()),
            ast::Value::SingleQuotedString(_) | ast::Value::DollarQuotedString(_) => {
                Some("TEXT".to_string())
            }
            ast::Value::Boolean(_) => Some("BOOLEAN".to_string()),
            ast::Value::Null => None,
            _ => None,
        },
        Expr::Cast { data_type, .. } => Some(data_type.to_string()),
        Expr::TypedString { data_type, .. } => Some(data_type.to_string()),
        Expr::BinaryOp {
            op: ast::BinaryOperator::And | ast::BinaryOperator::Or,
            ..
        } => Some("BOOLEAN".to_string()),
        Expr::Function(func) => {
            let name = func.name.to_string().to_uppercase();
            if name == "COUNT" {
                Some("INTEGER".to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}
