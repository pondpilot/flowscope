//! Type inference utilities for SQL expressions.
//!
//! This module provides basic type inference for SQL expressions, attempting to
//! determine the data type of columns and expressions based on their structure.

use sqlparser::ast::{self as ast, Expr, FunctionArg, FunctionArgExpr};
use std::fmt;

/// Inferred SQL data type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlType {
    Number,
    Integer,
    Text,
    Boolean,
    Date,
    Timestamp,
}

impl fmt::Display for SqlType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SqlType::Number => write!(f, "NUMBER"),
            SqlType::Integer => write!(f, "INTEGER"),
            SqlType::Text => write!(f, "TEXT"),
            SqlType::Boolean => write!(f, "BOOLEAN"),
            SqlType::Date => write!(f, "DATE"),
            SqlType::Timestamp => write!(f, "TIMESTAMP"),
        }
    }
}

/// Basic type inference for expressions
pub fn infer_expr_type(expr: &Expr) -> Option<SqlType> {
    match expr {
        Expr::Value(val) => match val {
            ast::Value::Number(_, _) => Some(SqlType::Number),
            ast::Value::SingleQuotedString(_) | ast::Value::DollarQuotedString(_) => {
                Some(SqlType::Text)
            }
            ast::Value::Boolean(_) => Some(SqlType::Boolean),
            ast::Value::Null => None,
            _ => None,
        },
        Expr::Cast { data_type, .. } => sql_type_from_data_type(data_type),
        Expr::TypedString { data_type, .. } => sql_type_from_data_type(data_type),
        Expr::Nested(inner) => infer_expr_type(inner),
        Expr::UnaryOp { op, expr } => match op {
            ast::UnaryOperator::Not => Some(SqlType::Boolean),
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
            | ast::BinaryOperator::GtEq => Some(SqlType::Boolean),
            ast::BinaryOperator::Plus => {
                let l_type = infer_expr_type(left);
                let r_type = infer_expr_type(right);
                if is_numeric_type(l_type) || is_numeric_type(r_type) {
                    Some(SqlType::Number)
                } else if l_type == Some(SqlType::Text) || r_type == Some(SqlType::Text) {
                    Some(SqlType::Text)
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
                if is_numeric_type(l_type) || is_numeric_type(r_type) {
                    Some(SqlType::Number)
                } else {
                    l_type.or(r_type)
                }
            }
            _ => None,
        },
        Expr::Function(func) => {
            let name = func.name.to_string().to_uppercase();
            match name.as_str() {
                "COUNT" | "ROW_NUMBER" | "RANK" | "DENSE_RANK" | "NTILE" => Some(SqlType::Integer),
                "SUM" | "AVG" => Some(SqlType::Number),
                "MIN" | "MAX" => infer_first_arg_type(func),
                "CONCAT" | "CONCAT_WS" | "SUBSTRING" | "LEFT" | "RIGHT" | "LOWER" | "UPPER"
                | "TRIM" | "LTRIM" | "RTRIM" | "REPLACE" | "CHR" | "INITCAP" => Some(SqlType::Text),
                "NOW" | "CURRENT_TIMESTAMP" | "GETDATE" | "SYSDATE" | "TIMEOFDAY" => {
                    Some(SqlType::Timestamp)
                }
                "CURRENT_DATE" | "CURDATE" | "TODAY" => Some(SqlType::Date),
                "COALESCE" | "IFNULL" | "NVL" => {
                    if let ast::FunctionArguments::List(args) = &func.args {
                        for arg in &args.args {
                            if let FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) = arg {
                                if let Some(t) = infer_expr_type(e) {
                                    return Some(t);
                                }
                            }
                        }
                    }
                    None
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Infer type of the first argument in a function call
fn infer_first_arg_type(func: &ast::Function) -> Option<SqlType> {
    if let ast::FunctionArguments::List(args) = &func.args {
        if let Some(FunctionArg::Unnamed(FunctionArgExpr::Expr(e))) = args.args.first() {
            return infer_expr_type(e);
        }
    }
    None
}

/// Convert SQL data type to SqlType
fn sql_type_from_data_type(data_type: &ast::DataType) -> Option<SqlType> {
    match data_type {
        ast::DataType::Int(_)
        | ast::DataType::Integer(_)
        | ast::DataType::BigInt(_)
        | ast::DataType::SmallInt(_)
        | ast::DataType::TinyInt(_) => Some(SqlType::Integer),
        ast::DataType::Float(_)
        | ast::DataType::Double
        | ast::DataType::DoublePrecision
        | ast::DataType::Real
        | ast::DataType::Decimal(_)
        | ast::DataType::Numeric(_) => Some(SqlType::Number),
        ast::DataType::Char(_)
        | ast::DataType::Varchar(_)
        | ast::DataType::Text
        | ast::DataType::String(_) => Some(SqlType::Text),
        ast::DataType::Boolean => Some(SqlType::Boolean),
        ast::DataType::Date => Some(SqlType::Date),
        ast::DataType::Timestamp(_, _) | ast::DataType::Datetime(_) => Some(SqlType::Timestamp),
        _ => None,
    }
}

fn is_numeric_type(t: Option<SqlType>) -> bool {
    matches!(t, Some(SqlType::Number) | Some(SqlType::Integer))
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
        assert_eq!(infer_expr_type(&parse_expr("123")), Some(SqlType::Number));
        assert_eq!(infer_expr_type(&parse_expr("'abc'")), Some(SqlType::Text));
        assert_eq!(infer_expr_type(&parse_expr("true")), Some(SqlType::Boolean));
        assert_eq!(infer_expr_type(&parse_expr("NULL")), None);
    }

    #[test]
    fn test_infer_binary_ops() {
        assert_eq!(infer_expr_type(&parse_expr("1 + 2")), Some(SqlType::Number));
        assert_eq!(
            infer_expr_type(&parse_expr("a > b")),
            Some(SqlType::Boolean)
        );
        // Recursive
        assert_eq!(
            infer_expr_type(&parse_expr("(1 + 2) * 3")),
            Some(SqlType::Number)
        );
    }

    #[test]
    fn test_infer_string_concatenation() {
        assert_eq!(
            infer_expr_type(&parse_expr("'a' + 'b'")),
            Some(SqlType::Text)
        );
    }

    #[test]
    fn test_infer_unary_ops() {
        assert_eq!(infer_expr_type(&parse_expr("-10")), Some(SqlType::Number));
        assert_eq!(
            infer_expr_type(&parse_expr("NOT (a > b)")),
            Some(SqlType::Boolean)
        );
    }

    #[test]
    fn test_infer_functions() {
        assert_eq!(
            infer_expr_type(&parse_expr("COUNT(*)")),
            Some(SqlType::Integer)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("ROW_NUMBER() OVER(ORDER BY x)")),
            Some(SqlType::Integer)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("CONCAT(a, b)")),
            Some(SqlType::Text)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("NOW()")),
            Some(SqlType::Timestamp)
        );
    }

    #[test]
    fn test_infer_aggregate_functions() {
        assert_eq!(
            infer_expr_type(&parse_expr("SUM(amount)")),
            Some(SqlType::Number)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("AVG(price)")),
            Some(SqlType::Number)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("MIN(123)")),
            Some(SqlType::Number)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("MAX('text')")),
            Some(SqlType::Text)
        );
    }

    #[test]
    fn test_infer_coalesce() {
        assert_eq!(
            infer_expr_type(&parse_expr("COALESCE(NULL, 1)")),
            Some(SqlType::Number)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("COALESCE(NULL, 'a')")),
            Some(SqlType::Text)
        );
    }

    #[test]
    fn test_infer_nested_coalesce() {
        assert_eq!(
            infer_expr_type(&parse_expr("COALESCE(NULL, COUNT(*))")),
            Some(SqlType::Integer)
        );
    }

    #[test]
    fn test_infer_cast() {
        assert_eq!(
            infer_expr_type(&parse_expr("CAST(x AS INTEGER)")),
            Some(SqlType::Integer)
        );
        assert_eq!(
            infer_expr_type(&parse_expr("CAST(x AS VARCHAR(100))")),
            Some(SqlType::Text)
        );
    }

    #[test]
    fn test_unknown_function_returns_none() {
        assert_eq!(infer_expr_type(&parse_expr("UNKNOWN_FUNC(x)")), None);
    }

    #[test]
    fn test_sql_type_display() {
        assert_eq!(SqlType::Number.to_string(), "NUMBER");
        assert_eq!(SqlType::Integer.to_string(), "INTEGER");
        assert_eq!(SqlType::Text.to_string(), "TEXT");
        assert_eq!(SqlType::Boolean.to_string(), "BOOLEAN");
        assert_eq!(SqlType::Date.to_string(), "DATE");
        assert_eq!(SqlType::Timestamp.to_string(), "TIMESTAMP");
    }
}
