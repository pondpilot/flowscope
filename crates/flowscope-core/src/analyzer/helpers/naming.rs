use crate::types::CanonicalName;

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
