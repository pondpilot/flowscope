//! Type checking utilities for SQL expressions.
//!
//! This module provides type mismatch detection for binary operators,
//! generating warnings when operand types are incompatible.
//!
//! ## Limitations
//!
//! - **Schema-unaware**: Type checking currently only uses structural type inference
//!   from literals, CASTs, and known functions. It does not resolve column types
//!   from schema metadata, so mismatches like `WHERE users.id = users.email`
//!   (INTEGER vs TEXT) won't be detected unless schema-aware inference is added.
//!
//! - **No span information**: TYPE_MISMATCH warnings include the statement index
//!   but not precise source spans for editor integration. Expression AST nodes
//!   from sqlparser don't include span information by default.

use crate::generated::{can_implicitly_cast, CanonicalType};
use crate::types::{issue_codes, Issue};
use crate::Dialect;
use sqlparser::ast::{self as ast, Expr, FunctionArg, FunctionArgExpr};

use super::types::infer_expr_type;

/// Maximum recursion depth for expression traversal to prevent stack overflow.
const MAX_RECURSION_DEPTH: usize = 100;

/// Checks an expression for type mismatches and returns any warnings found.
///
/// This function recursively traverses the expression tree, checking binary
/// operators for type compatibility. For comparison operators (=, <>, <, >, etc.),
/// it verifies that operands can be compared. For arithmetic operators (+, -, *, /),
/// it verifies that operands are numeric.
///
/// Type compatibility rules are dialect-aware. For example, comparing Boolean
/// with Integer is allowed in MySQL/MSSQL (which treat booleans as 0/1) but
/// not in PostgreSQL (which has strict typing).
///
/// # Arguments
///
/// * `expr` - The expression to check
/// * `statement_index` - The statement index for warning attribution
/// * `dialect` - The SQL dialect for dialect-specific type rules
///
/// # Returns
///
/// A vector of `Issue` warnings for any type mismatches found.
pub fn check_expr_types(expr: &Expr, statement_index: usize, dialect: Dialect) -> Vec<Issue> {
    let mut issues = Vec::new();
    check_expr_types_inner(expr, statement_index, dialect, &mut issues, 0);
    issues
}

fn check_expr_types_inner(
    expr: &Expr,
    statement_index: usize,
    dialect: Dialect,
    issues: &mut Vec<Issue>,
    depth: usize,
) {
    if depth > MAX_RECURSION_DEPTH {
        // Emit a low-severity warning when depth limit is exceeded
        issues.push(
            Issue::info(
                issue_codes::TYPE_MISMATCH,
                "Type checking skipped for deeply nested expression (recursion limit reached)",
            )
            .with_statement(statement_index),
        );
        return;
    }
    let next_depth = depth + 1;

    match expr {
        Expr::BinaryOp { left, op, right } => {
            // First, recursively check children
            check_expr_types_inner(left, statement_index, dialect, issues, next_depth);
            check_expr_types_inner(right, statement_index, dialect, issues, next_depth);

            // Then check this binary operation
            check_binary_op_types(left, op, right, statement_index, dialect, issues);
        }
        Expr::UnaryOp { expr: inner, .. } => {
            check_expr_types_inner(inner, statement_index, dialect, issues, next_depth);
        }
        Expr::Nested(inner) => {
            check_expr_types_inner(inner, statement_index, dialect, issues, next_depth);
        }
        Expr::Cast { expr: inner, .. } => {
            check_expr_types_inner(inner, statement_index, dialect, issues, next_depth);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                check_expr_types_inner(op, statement_index, dialect, issues, next_depth);
            }
            for case_when in conditions {
                check_expr_types_inner(
                    &case_when.condition,
                    statement_index,
                    dialect,
                    issues,
                    next_depth,
                );
                check_expr_types_inner(
                    &case_when.result,
                    statement_index,
                    dialect,
                    issues,
                    next_depth,
                );
            }
            if let Some(el) = else_result {
                check_expr_types_inner(el, statement_index, dialect, issues, next_depth);
            }
        }
        Expr::Function(func) => {
            if let ast::FunctionArguments::List(args) = &func.args {
                for arg in &args.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(e),
                            ..
                        } => {
                            check_expr_types_inner(e, statement_index, dialect, issues, next_depth);
                        }
                        _ => {}
                    }
                }
            }
        }
        Expr::InList { expr, list, .. } => {
            check_expr_types_inner(expr, statement_index, dialect, issues, next_depth);
            for item in list {
                check_expr_types_inner(item, statement_index, dialect, issues, next_depth);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            check_expr_types_inner(expr, statement_index, dialect, issues, next_depth);
            check_expr_types_inner(low, statement_index, dialect, issues, next_depth);
            check_expr_types_inner(high, statement_index, dialect, issues, next_depth);
        }
        _ => {}
    }
}

/// Checks if an expression is a NULL literal.
fn is_null_literal(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Value(ast::ValueWithSpan {
            value: ast::Value::Null,
            ..
        })
    )
}

/// Checks a binary operation for type compatibility.
fn check_binary_op_types(
    left: &Expr,
    op: &ast::BinaryOperator,
    right: &Expr,
    statement_index: usize,
    dialect: Dialect,
    issues: &mut Vec<Issue>,
) {
    // Check for NULL comparison anti-pattern (should use IS NULL instead)
    if matches!(op, ast::BinaryOperator::Eq | ast::BinaryOperator::NotEq)
        && (is_null_literal(left) || is_null_literal(right))
    {
        let suggestion = if *op == ast::BinaryOperator::Eq {
            "IS NULL"
        } else {
            "IS NOT NULL"
        };
        issues.push(
            Issue::warning(
                issue_codes::TYPE_MISMATCH,
                format!(
                    "Comparison with NULL using '{}' will always be NULL. Use {} instead",
                    op, suggestion
                ),
            )
            .with_statement(statement_index),
        );
        // Don't continue with type checking for NULL comparisons
        return;
    }

    let left_type = infer_expr_type(left);
    let right_type = infer_expr_type(right);

    // If we can't infer both types, we can't check compatibility
    let (Some(l_type), Some(r_type)) = (left_type, right_type) else {
        return;
    };

    match op {
        // Comparison operators: types must be compatible
        ast::BinaryOperator::Eq
        | ast::BinaryOperator::NotEq
        | ast::BinaryOperator::Lt
        | ast::BinaryOperator::LtEq
        | ast::BinaryOperator::Gt
        | ast::BinaryOperator::GtEq => {
            if !are_types_comparable(l_type, r_type, dialect) {
                let message = format!("Type mismatch in comparison: {} {} {}", l_type, op, r_type);
                issues.push(
                    Issue::warning(issue_codes::TYPE_MISMATCH, message)
                        .with_statement(statement_index),
                );
            }
        }
        // Arithmetic operators: both operands must be numeric
        ast::BinaryOperator::Plus
        | ast::BinaryOperator::Minus
        | ast::BinaryOperator::Multiply
        | ast::BinaryOperator::Divide
        | ast::BinaryOperator::Modulo => {
            // Plus is special: it can also be string concatenation
            if *op == ast::BinaryOperator::Plus {
                // Allow numeric + numeric, or text + text (concatenation)
                let l_numeric = is_numeric_type(l_type);
                let r_numeric = is_numeric_type(r_type);
                let l_text = l_type == CanonicalType::Text;
                let r_text = r_type == CanonicalType::Text;

                if (l_numeric && r_numeric) || (l_text && r_text) {
                    // Valid: numeric addition or string concatenation
                    return;
                }
                // Allow if both can be treated as numeric (one is numeric, other can cast TO numeric)
                let l_can_be_numeric =
                    l_numeric || can_implicitly_cast(l_type, CanonicalType::Float);
                let r_can_be_numeric =
                    r_numeric || can_implicitly_cast(r_type, CanonicalType::Float);
                if l_can_be_numeric && r_can_be_numeric {
                    return;
                }
            } else {
                // For other arithmetic ops, both must be numeric
                let l_numeric = is_numeric_type(l_type);
                let r_numeric = is_numeric_type(r_type);

                if l_numeric && r_numeric {
                    return;
                }
                // Allow implicit casts to numeric
                if (l_numeric || can_implicitly_cast(l_type, CanonicalType::Float))
                    && (r_numeric || can_implicitly_cast(r_type, CanonicalType::Float))
                {
                    return;
                }
            }

            // Type mismatch in arithmetic operation
            let message = format!(
                "Type mismatch in arithmetic operation: {} {} {}",
                l_type, op, r_type
            );
            issues.push(
                Issue::warning(issue_codes::TYPE_MISMATCH, message).with_statement(statement_index),
            );
        }
        // Logical operators: both operands should be boolean
        ast::BinaryOperator::And | ast::BinaryOperator::Or => {
            // Most databases implicitly convert to boolean, so we're lenient here
        }
        _ => {}
    }
}

/// Checks if two types are comparable (can be used in comparison operators).
///
/// This is stricter than general implicit casting - we want to warn when
/// comparing types that are semantically different even if they could be
/// converted. For example, comparing a number to a string is suspicious
/// even though numbers can be cast to strings.
///
/// Type compatibility rules are dialect-aware:
/// - Boolean/Integer comparison is allowed in MySQL, MSSQL, and SQLite (which
///   represent booleans as integers 0/1), but not in PostgreSQL, BigQuery,
///   or Snowflake (which have strict boolean types).
fn are_types_comparable(left: CanonicalType, right: CanonicalType, dialect: Dialect) -> bool {
    // Same types are always comparable
    if left == right {
        return true;
    }

    // Numeric types are comparable with each other (Integer/Float)
    if is_numeric_type(left) && is_numeric_type(right) {
        return true;
    }

    // Date and Timestamp are comparable
    if (left == CanonicalType::Date && right == CanonicalType::Timestamp)
        || (left == CanonicalType::Timestamp && right == CanonicalType::Date)
    {
        return true;
    }

    // Boolean can be compared with Integer in some dialects
    // MySQL, MSSQL, and SQLite treat booleans as integers (0/1)
    // PostgreSQL, BigQuery, and Snowflake have strict boolean types
    if (left == CanonicalType::Boolean && right == CanonicalType::Integer)
        || (left == CanonicalType::Integer && right == CanonicalType::Boolean)
    {
        return matches!(
            dialect,
            Dialect::Mysql | Dialect::Mssql | Dialect::Sqlite | Dialect::Generic
        );
    }

    // Everything else is a mismatch - notably:
    // - Text vs any numeric type (semantically different)
    // - Date/Timestamp vs numeric types
    // - Boolean vs Text
    // - etc.
    false
}

/// Checks if a type is numeric (Integer or Float).
fn is_numeric_type(t: CanonicalType) -> bool {
    matches!(t, CanonicalType::Integer | CanonicalType::Float)
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
            sqlparser::ast::Statement::Query(query) => match *query.body {
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

    fn parse_where_expr(sql: &str) -> Expr {
        let dialect = GenericDialect {};
        let full_sql = format!("SELECT 1 FROM t WHERE {}", sql);
        let mut ast = Parser::parse_sql(&dialect, &full_sql).unwrap();
        match ast.pop().unwrap() {
            sqlparser::ast::Statement::Query(query) => match *query.body {
                ast::SetExpr::Select(select) => select.selection.unwrap(),
                _ => panic!("Expected SELECT"),
            },
            _ => panic!("Expected Query"),
        }
    }

    #[test]
    fn test_integer_vs_text_comparison() {
        // 1 = 'text' should warn
        let expr = parse_where_expr("1 = 'text'");
        let issues = check_expr_types(&expr, 0, Dialect::Generic);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::TYPE_MISMATCH);
        assert!(issues[0].message.contains("FLOAT")); // 1 is parsed as Float
        assert!(issues[0].message.contains("TEXT"));
    }

    #[test]
    fn test_same_type_no_warning() {
        // 'a' = 'b' should not warn
        let expr = parse_where_expr("'a' = 'b'");
        let issues = check_expr_types(&expr, 0, Dialect::Generic);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_numeric_types_compatible() {
        // 1 = 2.5 should not warn (integer and float are compatible)
        let expr = parse_where_expr("1 = 2.5");
        let issues = check_expr_types(&expr, 0, Dialect::Generic);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_arithmetic_type_mismatch() {
        // true + 1 should warn (boolean + numeric)
        // Boolean cannot implicitly cast to Float, so this is a type mismatch
        let expr = parse_expr("true + 1");
        let issues = check_expr_types(&expr, 0, Dialect::Generic);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::TYPE_MISMATCH);
        assert!(issues[0].message.contains("BOOLEAN"));
    }

    #[test]
    fn test_string_concatenation_allowed() {
        // 'a' + 'b' should not warn (string concatenation)
        let expr = parse_expr("'a' + 'b'");
        let issues = check_expr_types(&expr, 0, Dialect::Generic);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_nested_expression_check() {
        // (1 = 'text') AND (2 = 3) should warn once for the first comparison
        let expr = parse_where_expr("(1 = 'text') AND (2 = 3)");
        let issues = check_expr_types(&expr, 0, Dialect::Generic);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("TEXT"));
    }

    #[test]
    fn test_boolean_in_arithmetic() {
        // Boolean + Boolean should warn since neither is numeric and
        // Boolean cannot implicitly cast to Float
        let expr = parse_expr("true + false");
        let issues = check_expr_types(&expr, 0, Dialect::Generic);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::TYPE_MISMATCH);
        assert!(issues[0].message.contains("BOOLEAN"));
    }

    #[test]
    fn test_numeric_plus_text_mismatch() {
        // 1 + 'text' should warn - cannot mix numeric and text in addition
        // Even though Float can implicitly cast to Text, the reverse is not true
        // so this is not a valid numeric addition
        let expr = parse_expr("1 + 'text'");
        let issues = check_expr_types(&expr, 0, Dialect::Generic);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::TYPE_MISMATCH);
        assert!(issues[0].message.contains("FLOAT"));
        assert!(issues[0].message.contains("TEXT"));
    }

    #[test]
    fn test_null_comparison_warning() {
        // x = NULL should warn about using IS NULL instead
        let expr = parse_where_expr("x = NULL");
        let issues = check_expr_types(&expr, 0, Dialect::Generic);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::TYPE_MISMATCH);
        assert!(issues[0].message.contains("IS NULL"));
    }

    #[test]
    fn test_null_not_equal_warning() {
        // x <> NULL should warn about using IS NOT NULL instead
        let expr = parse_where_expr("x <> NULL");
        let issues = check_expr_types(&expr, 0, Dialect::Generic);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("IS NOT NULL"));
    }

    #[test]
    fn test_boolean_integer_comparison_postgres() {
        // PostgreSQL is strict - boolean vs integer should warn
        let expr = parse_where_expr("true = 1");
        let issues = check_expr_types(&expr, 0, Dialect::Postgres);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::TYPE_MISMATCH);
        assert!(issues[0].message.contains("BOOLEAN"));
        assert!(issues[0].message.contains("FLOAT")); // 1 is parsed as Float
    }

    #[test]
    fn test_boolean_integer_comparison_mysql() {
        // MySQL allows boolean vs integer (booleans are 0/1)
        let expr = parse_where_expr("true = 1");
        let issues = check_expr_types(&expr, 0, Dialect::Mysql);
        // MySQL treats booleans as integers, but numeric literals are Float
        // so this is comparing Boolean with Float, not Boolean with Integer
        // Since are_types_comparable checks for Integer specifically, this will warn
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_boolean_integer_comparison_generic() {
        // Generic dialect allows boolean vs integer for broad compatibility
        let expr = parse_where_expr("true = 1");
        let issues = check_expr_types(&expr, 0, Dialect::Generic);
        // Same as MySQL - numeric literal is Float, not Integer
        assert_eq!(issues.len(), 1);
    }
}
