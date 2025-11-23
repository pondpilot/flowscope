use crate::types::CanonicalName;
use sqlparser::ast::{self as ast, Expr, Query, SetExpr};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Generate a deterministic node ID based on type and name
pub(crate) fn generate_node_id(node_type: &str, name: &str) -> String {
    let mut hasher = DefaultHasher::new();
    node_type.hash(&mut hasher);
    name.hash(&mut hasher);
    let hash = hasher.finish();

    format!("{node_type}_{hash:016x}")
}

/// Generate a deterministic edge ID
pub(crate) fn generate_edge_id(from: &str, to: &str) -> String {
    let mut hasher = DefaultHasher::new();
    from.hash(&mut hasher);
    to.hash(&mut hasher);
    let hash = hasher.finish();

    format!("edge_{hash:016x}")
}

/// Generate a deterministic column node ID
pub(crate) fn generate_column_node_id(parent_id: Option<&str>, column_name: &str) -> String {
    let mut hasher = DefaultHasher::new();
    "column".hash(&mut hasher);
    if let Some(parent) = parent_id {
        parent.hash(&mut hasher);
    }
    column_name.hash(&mut hasher);
    let hash = hasher.finish();

    format!("column_{hash:016x}")
}

/// Check if an expression is a simple column reference (no transformation)
pub(crate) fn is_simple_column_ref(expr: &Expr) -> bool {
    matches!(expr, Expr::Identifier(_) | Expr::CompoundIdentifier(_))
}

/// Extract simple name from potentially qualified name
pub(crate) fn extract_simple_name(name: &str) -> String {
    let mut parts = split_qualified_identifiers(name);
    parts.pop().unwrap_or_else(|| name.to_string())
}

/// Parse a qualified name string into CanonicalName
pub(crate) fn parse_canonical_name(name: &str) -> CanonicalName {
    let parts = split_qualified_identifiers(name);
    match parts.len() {
        0 => CanonicalName::table(None, None, String::new()),
        1 => CanonicalName::table(None, None, parts[0].clone()),
        2 => CanonicalName::table(None, Some(parts[0].clone()), parts[1].clone()),
        3 => CanonicalName::table(
            Some(parts[0].clone()),
            Some(parts[1].clone()),
            parts[2].clone(),
        ),
        _ => CanonicalName::table(None, None, name.to_string()),
    }
}

pub(crate) fn split_qualified_identifiers(name: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = name.chars().peekable();
    let mut active_quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        if let Some(q) = active_quote {
            current.push(ch);
            if ch == q {
                if matches!(q, '"' | '\'' | '`') {
                    if let Some(next) = chars.peek() {
                        if *next == q {
                            current.push(chars.next().unwrap());
                            continue;
                        }
                    }
                }
                active_quote = None;
            } else if q == ']' && ch == ']' {
                active_quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' | '`' => {
                active_quote = Some(ch);
                current.push(ch);
            }
            '[' => {
                active_quote = Some(']');
                current.push(ch);
            }
            '.' => {
                if !current.is_empty() {
                    parts.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        parts.push(current.trim().to_string());
    }

    if parts.is_empty() && !name.is_empty() {
        vec![name.trim().to_string()]
    } else {
        parts
    }
}

pub(crate) fn is_quoted_identifier(part: &str) -> bool {
    let trimmed = part.trim();
    if trimmed.len() < 2 {
        return false;
    }
    let first = trimmed.chars().next().unwrap();
    let last = trimmed.chars().last().unwrap();
    matches!(
        (first, last),
        ('"', '"') | ('`', '`') | ('[', ']') | ('\'', '\'')
    )
}

pub(crate) fn unquote_identifier(part: &str) -> String {
    let trimmed = part.trim();
    if trimmed.len() < 2 {
        return trimmed.to_string();
    }

    if is_quoted_identifier(trimmed) {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Classify the type of a query
pub(crate) fn classify_query_type(query: &Query) -> String {
    if query.with.is_some() {
        "WITH".to_string()
    } else {
        match &*query.body {
            SetExpr::Select(_) => "SELECT".to_string(),
            SetExpr::SetOperation { op, .. } => match op {
                ast::SetOperator::Union => "UNION".to_string(),
                ast::SetOperator::Intersect => "INTERSECT".to_string(),
                ast::SetOperator::Except => "EXCEPT".to_string(),
            },
            SetExpr::Values(_) => "VALUES".to_string(),
            _ => "SELECT".to_string(),
        }
    }
}
