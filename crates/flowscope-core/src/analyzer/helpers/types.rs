//! Type inference utilities for SQL expressions.
//!
//! This module provides basic type inference for SQL expressions, attempting to
//! determine the data type of columns and expressions based on their structure.

use crate::generated::{
    infer_function_return_type, normalize_type_name, CanonicalType, ReturnTypeRule,
};
use sqlparser::ast::{self as ast, Expr, FunctionArg, FunctionArgExpr};

/// Normalizes a schema type string to a canonical type display string.
///
/// Converts dialect-specific type names (e.g., "varchar", "int4", "TIMESTAMP_NTZ")
/// to canonical uppercase type names (e.g., "TEXT", "INTEGER", "TIMESTAMP").
/// If the type cannot be normalized, returns the original type string.
///
/// This is used consistently across the analyzer to normalize types from schema
/// metadata and CTE output columns.
pub fn normalize_schema_type(type_name: &str) -> String {
    normalize_type_name(type_name)
        .map(|canonical| canonical.as_uppercase_str().to_string())
        .unwrap_or_else(|| type_name.to_string())
}

/// Best-effort type inference for SQL expressions.
///
/// Attempts to infer the canonical type of an expression based on its structure.
/// This is used for completion hints and is intentionally permissive - it does not
/// validate type consistency (e.g., mixed-type CASE branches are not flagged).
///
/// # Supported expressions
/// - Literals: numbers → Float, strings → Text, booleans → Boolean, NULL → None
/// - CAST/type annotations
/// - Unary operators (NOT → Boolean, +/- → preserves operand type)
/// - Binary operators (comparisons → Boolean, arithmetic → Float)
/// - Function calls (via `infer_function_return_type`)
/// - CASE expressions (returns first inferable branch type)
/// - Nested expressions
///
/// # CASE expression behavior
/// For CASE expressions, returns the type of the first THEN branch that can be
/// typed, or falls back to ELSE. This is a heuristic - it does not verify that
/// all branches have compatible types. For example:
/// ```sql
/// CASE WHEN x > 0 THEN 1 WHEN y > 0 THEN 'text' END
/// ```
/// Returns `Float` (from the first branch) without flagging the type mismatch.
/// NULL branches are skipped since they can match any type.
pub fn infer_expr_type(expr: &Expr) -> Option<CanonicalType> {
    match expr {
        Expr::Value(val) => match &val.value {
            ast::Value::Number(_, _) => Some(CanonicalType::Float),
            ast::Value::SingleQuotedString(_) | ast::Value::DollarQuotedString(_) => {
                Some(CanonicalType::Text)
            }
            ast::Value::Boolean(_) => Some(CanonicalType::Boolean),
            ast::Value::Null => None,
            _ => None,
        },
        Expr::Cast { data_type, .. } => canonical_type_from_data_type(data_type),
        Expr::TypedString(typed_string) => canonical_type_from_data_type(&typed_string.data_type),
        Expr::Nested(inner) => infer_expr_type(inner),
        Expr::UnaryOp { op, expr } => match op {
            ast::UnaryOperator::Not => Some(CanonicalType::Boolean),
            ast::UnaryOperator::Plus | ast::UnaryOperator::Minus => infer_expr_type(expr),
            _ => None,
        },
        Expr::BinaryOp { left, op, right } => match op {
            ast::BinaryOperator::And
            | ast::BinaryOperator::Or
            | ast::BinaryOperator::Eq
            | ast::BinaryOperator::NotEq
            | ast::BinaryOperator::Lt
            | ast::BinaryOperator::LtEq
            | ast::BinaryOperator::Gt
            | ast::BinaryOperator::GtEq => Some(CanonicalType::Boolean),
            ast::BinaryOperator::Plus => {
                let l_type = infer_expr_type(left);
                let r_type = infer_expr_type(right);
                if is_numeric_type(&l_type) || is_numeric_type(&r_type) {
                    Some(CanonicalType::Float)
                } else if l_type == Some(CanonicalType::Text) || r_type == Some(CanonicalType::Text)
                {
                    Some(CanonicalType::Text)
                } else {
                    l_type.or(r_type)
                }
            }
            ast::BinaryOperator::Minus
            | ast::BinaryOperator::Multiply
            | ast::BinaryOperator::Divide
            | ast::BinaryOperator::Modulo => {
                let l_type = infer_expr_type(left);
                let r_type = infer_expr_type(right);
                if is_numeric_type(&l_type) || is_numeric_type(&r_type) {
                    Some(CanonicalType::Float)
                } else {
                    l_type.or(r_type)
                }
            }
            _ => None,
        },
        Expr::Case {
            conditions,
            else_result,
            ..
        } => {
            // Best-effort: return first inferable type from THEN branches or ELSE.
            // Skips NULL branches (None) and does not validate type consistency.
            // The `operand` field (for simple CASE) is ignored - we only care about result types.
            for cond in conditions {
                if let Some(t) = infer_expr_type(&cond.result) {
                    return Some(t);
                }
            }
            if let Some(else_expr) = else_result {
                return infer_expr_type(else_expr);
            }
            None
        }
        Expr::Function(func) => {
            let name = func.name.to_string();
            // Try data-driven type inference first
            if let Some(rule) = infer_function_return_type(&name) {
                return match rule {
                    ReturnTypeRule::Integer => Some(CanonicalType::Integer),
                    ReturnTypeRule::Numeric => Some(CanonicalType::Float),
                    ReturnTypeRule::Text => Some(CanonicalType::Text),
                    ReturnTypeRule::Timestamp => Some(CanonicalType::Timestamp),
                    ReturnTypeRule::Boolean => Some(CanonicalType::Boolean),
                    ReturnTypeRule::Date => Some(CanonicalType::Date),
                    ReturnTypeRule::MatchFirstArg => {
                        // Special handling for COALESCE/IFNULL/NVL: iterate through args
                        // to find first non-null type since first arg might be NULL
                        let name_upper = name.to_uppercase();
                        if matches!(name_upper.as_str(), "COALESCE" | "IFNULL" | "NVL") {
                            if let ast::FunctionArguments::List(args) = &func.args {
                                for arg in &args.args {
                                    if let FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) = arg {
                                        if let Some(t) = infer_expr_type(e) {
                                            return Some(t);
                                        }
                                    }
                                }
                            }
                            return None;
                        }
                        infer_first_arg_type(func)
                    }
                };
            }
            // Fallback for functions not yet in functions.json
            let name_upper = name.to_uppercase();
            match name_upper.as_str() {
                // String functions not yet in functions.json
                "LEFT" | "RIGHT" | "LTRIM" | "RTRIM" | "CHR" | "INITCAP" => {
                    Some(CanonicalType::Text)
                }
                // Timestamp functions not yet in functions.json
                "GETDATE" | "SYSDATE" | "TIMEOFDAY" => Some(CanonicalType::Timestamp),
                // Date functions not yet in functions.json
                "CURDATE" | "TODAY" => Some(CanonicalType::Date),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Infer type of the first argument in a function call
fn infer_first_arg_type(func: &ast::Function) -> Option<CanonicalType> {
    if let ast::FunctionArguments::List(args) = &func.args {
        if let Some(FunctionArg::Unnamed(FunctionArgExpr::Expr(e))) = args.args.first() {
            return infer_expr_type(e);
        }
    }
    None
}

/// Convert SQL data type to CanonicalType
pub fn canonical_type_from_data_type(data_type: &ast::DataType) -> Option<CanonicalType> {
    match data_type {
        ast::DataType::Int(_)
        | ast::DataType::Integer(_)
        | ast::DataType::BigInt(_)
        | ast::DataType::SmallInt(_)
        | ast::DataType::TinyInt(_)
        | ast::DataType::Int64
        | ast::DataType::Int128
        | ast::DataType::Int256
        | ast::DataType::Int4(_)
        | ast::DataType::Int8(_)
        | ast::DataType::Int2(_)
        | ast::DataType::UInt8
        | ast::DataType::UInt16
        | ast::DataType::UInt32
        | ast::DataType::UInt64
        | ast::DataType::UInt128
        | ast::DataType::UInt256 => Some(CanonicalType::Integer),
        ast::DataType::Float(_)
        | ast::DataType::Double(_)
        | ast::DataType::DoublePrecision
        | ast::DataType::Real
        | ast::DataType::Decimal(_)
        | ast::DataType::Numeric(_) => Some(CanonicalType::Float),
        ast::DataType::Char(_)
        | ast::DataType::Varchar(_)
        | ast::DataType::Text
        | ast::DataType::String(_) => Some(CanonicalType::Text),
        ast::DataType::Boolean => Some(CanonicalType::Boolean),
        ast::DataType::Date => Some(CanonicalType::Date),
        ast::DataType::Time(_, _) => Some(CanonicalType::Time),
        ast::DataType::Timestamp(_, _) | ast::DataType::Datetime(_) => {
            Some(CanonicalType::Timestamp)
        }
        ast::DataType::Bytea
        | ast::DataType::Binary(_)
        | ast::DataType::Varbinary(_)
        | ast::DataType::Blob(_) => Some(CanonicalType::Binary),
        ast::DataType::JSON | ast::DataType::JSONB => Some(CanonicalType::Json),
        ast::DataType::Array(_) => Some(CanonicalType::Array),
        // For custom types, try to normalize using the type system
        ast::DataType::Custom(obj_name, _) => normalize_type_name(&obj_name.to_string()),
        // For other types, return None (unknown type)
        _ => None,
    }
}

fn is_numeric_type(t: &Option<CanonicalType>) -> bool {
    matches!(t, Some(CanonicalType::Float) | Some(CanonicalType::Integer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::dialect::GenericDialect;
    use sqlparser::parser::Parser;

    fn parse_expr(sql: &str) -> Expr {
        let dialect = GenericDialect {};
        let mut ast = Parser::parse_sql(&dialect, &format!("SELECT {}", sql)).unwrap();
        match ast.pop().unwrap() {
            ast::Statement::Query(query) => match *query.body {
                ast::SetExpr::Select(select) => match select.projection.into_iter().next().unwrap()
                {
                    ast::SelectItem::UnnamedExpr(expr) => expr,
                    _ => panic!("Expected expression"),
                },
                _ => panic!("Expected SELECT"),
            },
            _ => panic!("Expected Query"),
        }
    }

    #[test]
    fn test_infer_literals() {
        assert_eq!(
            infer_expr_type(&parse_expr("123")),
            Some(CanonicalType::Float)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("'abc'")),
            Some(CanonicalType::Text)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("true")),
            Some(CanonicalType::Boolean)
        );
        assert_eq!(infer_expr_type(&parse_expr("NULL")), None);
    }

    #[test]
    fn test_infer_binary_ops() {
        assert_eq!(
            infer_expr_type(&parse_expr("1 + 2")),
            Some(CanonicalType::Float)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("a > b")),
            Some(CanonicalType::Boolean)
        );
        // Recursive
        assert_eq!(
            infer_expr_type(&parse_expr("(1 + 2) * 3")),
            Some(CanonicalType::Float)
        );
    }

    #[test]
    fn test_infer_string_concatenation() {
        assert_eq!(
            infer_expr_type(&parse_expr("'a' + 'b'")),
            Some(CanonicalType::Text)
        );
    }

    #[test]
    fn test_infer_unary_ops() {
        assert_eq!(
            infer_expr_type(&parse_expr("-10")),
            Some(CanonicalType::Float)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("NOT (a > b)")),
            Some(CanonicalType::Boolean)
        );
    }

    #[test]
    fn test_infer_functions() {
        assert_eq!(
            infer_expr_type(&parse_expr("COUNT(*)")),
            Some(CanonicalType::Integer)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("ROW_NUMBER() OVER(ORDER BY x)")),
            Some(CanonicalType::Integer)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("CONCAT(a, b)")),
            Some(CanonicalType::Text)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("NOW()")),
            Some(CanonicalType::Timestamp)
        );
    }

    #[test]
    fn test_infer_aggregate_functions() {
        assert_eq!(
            infer_expr_type(&parse_expr("SUM(amount)")),
            Some(CanonicalType::Float)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("AVG(price)")),
            Some(CanonicalType::Float)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("MIN(123)")),
            Some(CanonicalType::Float)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("MAX('text')")),
            Some(CanonicalType::Text)
        );
    }

    #[test]
    fn test_infer_coalesce() {
        assert_eq!(
            infer_expr_type(&parse_expr("COALESCE(NULL, 1)")),
            Some(CanonicalType::Float)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("COALESCE(NULL, 'a')")),
            Some(CanonicalType::Text)
        );
    }

    #[test]
    fn test_infer_nested_coalesce() {
        assert_eq!(
            infer_expr_type(&parse_expr("COALESCE(NULL, COUNT(*))")),
            Some(CanonicalType::Integer)
        );
    }

    #[test]
    fn test_infer_cast() {
        assert_eq!(
            infer_expr_type(&parse_expr("CAST(x AS INTEGER)")),
            Some(CanonicalType::Integer)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("CAST(x AS VARCHAR(100))")),
            Some(CanonicalType::Text)
        );
        // Custom types that can be normalized
        assert_eq!(
            infer_expr_type(&parse_expr("CAST(x AS INT64)")),
            Some(CanonicalType::Integer)
        );
        // VARIANT maps to Json in the type system (Snowflake semi-structured)
        assert_eq!(
            infer_expr_type(&parse_expr("CAST(x AS VARIANT)")),
            Some(CanonicalType::Json)
        );
        // Truly unknown custom types return None
        assert_eq!(
            infer_expr_type(&parse_expr("CAST(x AS MY_CUSTOM_UDT)")),
            None
        );
    }

    #[test]
    fn test_unknown_function_returns_none() {
        assert_eq!(infer_expr_type(&parse_expr("UNKNOWN_FUNC(x)")), None);
    }

    #[test]
    fn test_infer_case_expression_with_then() {
        // CASE with string result in THEN branch
        assert_eq!(
            infer_expr_type(&parse_expr("CASE WHEN x > 0 THEN 'positive' ELSE 'negative' END")),
            Some(CanonicalType::Text)
        );
    }

    #[test]
    fn test_infer_case_expression_with_numeric() {
        // CASE with numeric result
        assert_eq!(
            infer_expr_type(&parse_expr("CASE WHEN x > 0 THEN 1 ELSE 0 END")),
            Some(CanonicalType::Float) // Numbers are inferred as Float
        );
    }

    #[test]
    fn test_infer_case_expression_with_null_then() {
        // CASE where first THEN is NULL, should fall through to second THEN or ELSE
        assert_eq!(
            infer_expr_type(&parse_expr(
                "CASE WHEN x IS NULL THEN NULL WHEN x > 0 THEN 'yes' ELSE 'no' END"
            )),
            Some(CanonicalType::Text)
        );
    }

    #[test]
    fn test_infer_case_expression_else_fallback() {
        // CASE where THEN conditions can't be typed but ELSE can
        assert_eq!(
            infer_expr_type(&parse_expr("CASE WHEN x > 0 THEN NULL ELSE 'default' END")),
            Some(CanonicalType::Text)
        );
    }

    #[test]
    fn test_infer_case_all_null() {
        // CASE where all branches are NULL
        assert_eq!(
            infer_expr_type(&parse_expr("CASE WHEN x > 0 THEN NULL ELSE NULL END")),
            None
        );
    }

    #[test]
    fn test_infer_case_mixed_types_returns_first() {
        // Best-effort: returns first typed branch, does NOT validate type consistency.
        // This documents current behavior - mixed types would be a SQL error at runtime.
        assert_eq!(
            infer_expr_type(&parse_expr(
                "CASE WHEN x > 0 THEN 1 WHEN y > 0 THEN 'text' ELSE TRUE END"
            )),
            Some(CanonicalType::Float) // Returns Float from first branch, ignores mismatch
        );
    }

    #[test]
    fn test_infer_case_without_else() {
        // CASE without ELSE, first THEN has type
        assert_eq!(
            infer_expr_type(&parse_expr("CASE WHEN x > 0 THEN 'yes' END")),
            Some(CanonicalType::Text)
        );
    }

    #[test]
    fn test_canonical_type_display() {
        assert_eq!(CanonicalType::Float.to_string(), "FLOAT");
        assert_eq!(CanonicalType::Integer.to_string(), "INTEGER");
        assert_eq!(CanonicalType::Text.to_string(), "TEXT");
        assert_eq!(CanonicalType::Boolean.to_string(), "BOOLEAN");
        assert_eq!(CanonicalType::Date.to_string(), "DATE");
        assert_eq!(CanonicalType::Timestamp.to_string(), "TIMESTAMP");
        assert_eq!(CanonicalType::Time.to_string(), "TIME");
        assert_eq!(CanonicalType::Binary.to_string(), "BINARY");
        assert_eq!(CanonicalType::Json.to_string(), "JSON");
        assert_eq!(CanonicalType::Array.to_string(), "ARRAY");
    }

    #[test]
    fn test_canonical_type_from_data_type_extended() {
        // Test Time type
        assert_eq!(
            infer_expr_type(&parse_expr("CAST(x AS TIME)")),
            Some(CanonicalType::Time)
        );
        // Test JSON type
        assert_eq!(
            infer_expr_type(&parse_expr("CAST(x AS JSON)")),
            Some(CanonicalType::Json)
        );
    }
}
