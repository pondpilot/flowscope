use sqlparser::ast::{Ident, ObjectName};

use crate::types::CanonicalName;

// =============================================================================
// ObjectName-based helpers (work directly with AST types)
// =============================================================================

/// Extract the identifier value from an ObjectName part.
///
/// Returns the string value for Identifier parts, or the Display representation
/// for Function parts.
fn object_name_part_value(part: &sqlparser::ast::ObjectNamePart) -> String {
    part.as_ident()
        .map(|ident| ident.value.clone())
        .unwrap_or_else(|| part.to_string())
}

/// Extract the simple (unqualified) name from an ObjectName.
///
/// Works directly with the AST to avoid string parsing.
///
/// # Examples
/// - `schema.table` → `"table"`
/// - `catalog.schema.table` → `"table"`
pub fn extract_simple_name_from_object_name(name: &ObjectName) -> String {
    name.0
        .last()
        .map(object_name_part_value)
        .unwrap_or_default()
}

/// Build a CanonicalName from an ObjectName without string round-trips.
///
/// This is more efficient than converting to string and parsing, as it
/// works directly with the already-parsed identifiers. For names that have
/// more than three segments we follow the legacy string-based helper and
/// treat the entire identifier as a single `name` so callers do not lose
/// the leading qualifiers (e.g. SQL Server's `server.database.schema.table`).
///
/// All name parts are stored unquoted for consistency. For example,
/// `"my.schema"."my.table"` becomes `my.schema.my.table` in the name field
/// when there are more than 3 parts.
pub fn canonical_name_from_object_name(name: &ObjectName) -> CanonicalName {
    // Match on length first to avoid intermediate Vec allocation for common cases
    match name.0.len() {
        0 => CanonicalName::table(None, None, String::new()),
        1 => CanonicalName::table(None, None, object_name_part_value(&name.0[0])),
        2 => CanonicalName::table(
            None,
            Some(object_name_part_value(&name.0[0])),
            object_name_part_value(&name.0[1]),
        ),
        3 => CanonicalName::table(
            Some(object_name_part_value(&name.0[0])),
            Some(object_name_part_value(&name.0[1])),
            object_name_part_value(&name.0[2]),
        ),
        _ => {
            // For >3 parts, join all unquoted values with dots to maintain
            // consistency with the 1-3 part branches (which use unquoted values)
            let joined = name
                .0
                .iter()
                .map(object_name_part_value)
                .collect::<Vec<_>>()
                .join(".");
            CanonicalName::table(None, None, joined)
        }
    }
}

/// Get the unquoted value from an Ident.
///
/// sqlparser's Ident already stores the unquoted value in `.value`,
/// so this is just a convenience accessor that mirrors the string-based
/// `unquote_identifier` function.
pub fn ident_value(ident: &Ident) -> &str {
    &ident.value
}

// =============================================================================
// String-based helpers (for user input and backward compatibility)
// =============================================================================

pub fn extract_simple_name(name: &str) -> String {
    let mut parts = split_qualified_identifiers(name);
    parts.pop().unwrap_or_else(|| name.to_string())
}

pub fn split_qualified_identifiers(name: &str) -> Vec<String> {
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

pub fn is_quoted_identifier(part: &str) -> bool {
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

pub fn unquote_identifier(part: &str) -> String {
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

pub fn parse_canonical_name(name: &str) -> CanonicalName {
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
