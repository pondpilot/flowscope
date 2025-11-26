use sqlparser::ast::{self as ast, Expr, Query, SetExpr};

/// Classify the type of a query
pub fn classify_query_type(query: &Query) -> String {
    if query.with.is_some() {
        "WITH".to_string()
    } else {
        match &*query.body {
            SetExpr::Select(_) => "SELECT".to_string(),
            SetExpr::SetOperation { op, .. } => match op {
                ast::SetOperator::Union => "UNION".to_string(),
                ast::SetOperator::Intersect => "INTERSECT".to_string(),
                ast::SetOperator::Except => "EXCEPT".to_string(),
                ast::SetOperator::Minus => "MINUS".to_string(),
            },
            SetExpr::Values(_) => "VALUES".to_string(),
            _ => "SELECT".to_string(),
        }
    }
}

/// Check if an expression is a simple column reference (no transformation)
pub fn is_simple_column_ref(expr: &Expr) -> bool {
    matches!(expr, Expr::Identifier(_) | Expr::CompoundIdentifier(_))
}
