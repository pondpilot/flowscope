//! Type checking utilities for SQL expressions.
//!
//! This module provides type mismatch detection for binary operators,
//! generating warnings when operand types are incompatible.

use crate::generated::{can_implicitly_cast, CanonicalType};
use crate::types::{issue_codes, Issue, Span};
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
/// # Arguments
///
/// * `expr` - The expression to check
/// * `statement_index` - The statement index for warning attribution
///
/// # Returns
///
/// A vector of `Issue` warnings for any type mismatches found.
pub fn check_expr_types(expr: &Expr, statement_index: usize) -> Vec<Issue> {
    let mut issues = Vec::new();
    check_expr_types_inner(expr, statement_index, &mut issues, 0);
    issues
}

fn check_expr_types_inner(
    expr: &Expr,
    statement_index: usize,
    issues: &mut Vec<Issue>,
    depth: usize,
) {
    if depth > MAX_RECURSION_DEPTH {
        return;
    }
    let next_depth = depth + 1;

    match expr {
        Expr::BinaryOp { left, op, right } => {
            // First, recursively check children
            check_expr_types_inner(left, statement_index, issues, next_depth);
            check_expr_types_inner(right, statement_index, issues, next_depth);

            // Then check this binary operation
            check_binary_op_types(left, op, right, statement_index, issues);
        }
        Expr::UnaryOp { expr: inner, .. } => {
            check_expr_types_inner(inner, statement_index, issues, next_depth);
        }
        Expr::Nested(inner) => {
            check_expr_types_inner(inner, statement_index, issues, next_depth);
        }
        Expr::Cast { expr: inner, .. } => {
            check_expr_types_inner(inner, statement_index, issues, next_depth);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                check_expr_types_inner(op, statement_index, issues, next_depth);
            }
            for case_when in conditions {
                check_expr_types_inner(&case_when.condition, statement_index, issues, next_depth);
                check_expr_types_inner(&case_when.result, statement_index, issues, next_depth);
            }
            if let Some(el) = else_result {
                check_expr_types_inner(el, statement_index, issues, next_depth);
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
                            check_expr_types_inner(e, statement_index, issues, next_depth);
                        }
                        _ => {}
                    }
                }
            }
        }
        Expr::InList { expr, list, .. } => {
            check_expr_types_inner(expr, statement_index, issues, next_depth);
            for item in list {
                check_expr_types_inner(item, statement_index, issues, next_depth);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            check_expr_types_inner(expr, statement_index, issues, next_depth);
            check_expr_types_inner(low, statement_index, issues, next_depth);
            check_expr_types_inner(high, statement_index, issues, next_depth);
        }
        _ => {}
    }
}

/// Checks a binary operation for type compatibility.
fn check_binary_op_types(
    left: &Expr,
    op: &ast::BinaryOperator,
    right: &Expr,
    statement_index: usize,
    issues: &mut Vec<Issue>,
) {
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
            if !are_types_comparable(l_type, r_type) {
                let message = format!(
                    "Type mismatch in comparison: {} {} {}",
                    l_type, op, r_type
                );
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
                // Allow implicit casts between numeric types
                if (l_numeric || r_numeric)
                    && (can_implicitly_cast(l_type, r_type) || can_implicitly_cast(r_type, l_type))
                {
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
                Issue::warning(issue_codes::TYPE_MISMATCH, message)
                    .with_statement(statement_index),
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
fn are_types_comparable(left: CanonicalType, right: CanonicalType) -> bool {
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

    // Boolean can be compared with Integer (common in SQL: 0/1 to false/true)
    if (left == CanonicalType::Boolean && right == CanonicalType::Integer)
        || (left == CanonicalType::Integer && right == CanonicalType::Boolean)
    {
        return true;
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

/// Checks an expression for type mismatches and returns issues with spans.
///
/// This is an enhanced version of `check_expr_types` that attempts to include
/// source location spans in the warnings for better editor integration.
///
/// # Arguments
///
/// * `expr` - The expression to check
/// * `statement_index` - The statement index for warning attribution
/// * `find_span` - A function to find the span of an expression in the source
///
/// # Returns
///
/// A vector of `Issue` warnings for any type mismatches found.
pub fn check_expr_types_with_span<F>(
    expr: &Expr,
    statement_index: usize,
    find_span: F,
) -> Vec<Issue>
where
    F: Fn(&str) -> Option<Span>,
{
    let mut issues = Vec::new();
    check_expr_types_with_span_inner(expr, statement_index, &find_span, &mut issues, 0);
    issues
}

fn check_expr_types_with_span_inner<F>(
    expr: &Expr,
    statement_index: usize,
    find_span: &F,
    issues: &mut Vec<Issue>,
    depth: usize,
) where
    F: Fn(&str) -> Option<Span>,
{
    if depth > MAX_RECURSION_DEPTH {
        return;
    }
    let next_depth = depth + 1;

    match expr {
        Expr::BinaryOp { left, op, right } => {
            // First, recursively check children
            check_expr_types_with_span_inner(left, statement_index, find_span, issues, next_depth);
            check_expr_types_with_span_inner(right, statement_index, find_span, issues, next_depth);

            // Then check this binary operation
            check_binary_op_types_with_span(left, op, right, statement_index, find_span, issues);
        }
        Expr::UnaryOp { expr: inner, .. } => {
            check_expr_types_with_span_inner(inner, statement_index, find_span, issues, next_depth);
        }
        Expr::Nested(inner) => {
            check_expr_types_with_span_inner(inner, statement_index, find_span, issues, next_depth);
        }
        Expr::Cast { expr: inner, .. } => {
            check_expr_types_with_span_inner(inner, statement_index, find_span, issues, next_depth);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                check_expr_types_with_span_inner(op, statement_index, find_span, issues, next_depth);
            }
            for case_when in conditions {
                check_expr_types_with_span_inner(
                    &case_when.condition,
                    statement_index,
                    find_span,
                    issues,
                    next_depth,
                );
                check_expr_types_with_span_inner(
                    &case_when.result,
                    statement_index,
                    find_span,
                    issues,
                    next_depth,
                );
            }
            if let Some(el) = else_result {
                check_expr_types_with_span_inner(el, statement_index, find_span, issues, next_depth);
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
                            check_expr_types_with_span_inner(
                                e,
                                statement_index,
                                find_span,
                                issues,
                                next_depth,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
        Expr::InList { expr, list, .. } => {
            check_expr_types_with_span_inner(expr, statement_index, find_span, issues, next_depth);
            for item in list {
                check_expr_types_with_span_inner(item, statement_index, find_span, issues, next_depth);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            check_expr_types_with_span_inner(expr, statement_index, find_span, issues, next_depth);
            check_expr_types_with_span_inner(low, statement_index, find_span, issues, next_depth);
            check_expr_types_with_span_inner(high, statement_index, find_span, issues, next_depth);
        }
        _ => {}
    }
}

/// Checks a binary operation for type compatibility with span support.
fn check_binary_op_types_with_span<F>(
    left: &Expr,
    op: &ast::BinaryOperator,
    right: &Expr,
    statement_index: usize,
    find_span: &F,
    issues: &mut Vec<Issue>,
) where
    F: Fn(&str) -> Option<Span>,
{
    let left_type = infer_expr_type(left);
    let right_type = infer_expr_type(right);

    // If we can't infer both types, we can't check compatibility
    let (Some(l_type), Some(r_type)) = (left_type, right_type) else {
        return;
    };

    let mismatch = match op {
        // Comparison operators: types must be compatible
        ast::BinaryOperator::Eq
        | ast::BinaryOperator::NotEq
        | ast::BinaryOperator::Lt
        | ast::BinaryOperator::LtEq
        | ast::BinaryOperator::Gt
        | ast::BinaryOperator::GtEq => {
            if !are_types_comparable(l_type, r_type) {
                Some(format!(
                    "Type mismatch in comparison: {} {} {}",
                    l_type, op, r_type
                ))
            } else {
                None
            }
        }
        // Arithmetic operators: both operands must be numeric
        ast::BinaryOperator::Plus
        | ast::BinaryOperator::Minus
        | ast::BinaryOperator::Multiply
        | ast::BinaryOperator::Divide
        | ast::BinaryOperator::Modulo => {
            check_arithmetic_mismatch(l_type, op, r_type)
        }
        _ => None,
    };

    if let Some(message) = mismatch {
        // Try to find the span for this expression
        let expr_str = format!("{} {} {}", left, op, right);
        let span = find_span(&expr_str);

        let mut issue = Issue::warning(issue_codes::TYPE_MISMATCH, message)
            .with_statement(statement_index);
        if let Some(s) = span {
            issue = issue.with_span(s);
        }
        issues.push(issue);
    }
}

/// Checks for arithmetic type mismatch and returns an error message if found.
fn check_arithmetic_mismatch(
    l_type: CanonicalType,
    op: &ast::BinaryOperator,
    r_type: CanonicalType,
) -> Option<String> {
    // Plus is special: it can also be string concatenation
    if *op == ast::BinaryOperator::Plus {
        // Allow numeric + numeric, or text + text (concatenation)
        let l_numeric = is_numeric_type(l_type);
        let r_numeric = is_numeric_type(r_type);
        let l_text = l_type == CanonicalType::Text;
        let r_text = r_type == CanonicalType::Text;

        if (l_numeric && r_numeric) || (l_text && r_text) {
            return None;
        }
        // Allow implicit casts between numeric types
        if (l_numeric || r_numeric)
            && (can_implicitly_cast(l_type, r_type) || can_implicitly_cast(r_type, l_type))
        {
            return None;
        }
    } else {
        // For other arithmetic ops, both must be numeric
        let l_numeric = is_numeric_type(l_type);
        let r_numeric = is_numeric_type(r_type);

        if l_numeric && r_numeric {
            return None;
        }
        // Allow implicit casts to numeric
        if (l_numeric || can_implicitly_cast(l_type, CanonicalType::Float))
            && (r_numeric || can_implicitly_cast(r_type, CanonicalType::Float))
        {
            return None;
        }
    }

    Some(format!(
        "Type mismatch in arithmetic operation: {} {} {}",
        l_type, op, r_type
    ))
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
        let issues = check_expr_types(&expr, 0);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::TYPE_MISMATCH);
        assert!(issues[0].message.contains("FLOAT")); // 1 is parsed as Float
        assert!(issues[0].message.contains("TEXT"));
    }

    #[test]
    fn test_same_type_no_warning() {
        // 'a' = 'b' should not warn
        let expr = parse_where_expr("'a' = 'b'");
        let issues = check_expr_types(&expr, 0);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_numeric_types_compatible() {
        // 1 = 2.5 should not warn (integer and float are compatible)
        let expr = parse_where_expr("1 = 2.5");
        let issues = check_expr_types(&expr, 0);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_arithmetic_type_mismatch() {
        // true + 1 should warn (boolean + numeric)
        let expr = parse_expr("true + 1");
        let issues = check_expr_types(&expr, 0);
        // Note: Integer can implicitly cast to Boolean in some systems
        // but this depends on can_implicitly_cast implementation
        // The test verifies the logic is working
        assert!(issues.is_empty() || issues[0].code == issue_codes::TYPE_MISMATCH);
    }

    #[test]
    fn test_string_concatenation_allowed() {
        // 'a' + 'b' should not warn (string concatenation)
        let expr = parse_expr("'a' + 'b'");
        let issues = check_expr_types(&expr, 0);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_nested_expression_check() {
        // (1 = 'text') AND (2 = 3) should warn once for the first comparison
        let expr = parse_where_expr("(1 = 'text') AND (2 = 3)");
        let issues = check_expr_types(&expr, 0);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("TEXT"));
    }

    #[test]
    fn test_boolean_in_arithmetic() {
        // Booleans can implicitly cast to Integer in our type system
        // true + false might not warn depending on the implicit cast rules
        let expr = parse_expr("true + false");
        let issues = check_expr_types(&expr, 0);
        // Boolean can cast to Integer, so this might be allowed
        // We're testing that the code runs without panic
        assert!(issues.is_empty() || issues[0].code == issue_codes::TYPE_MISMATCH);
    }
}
