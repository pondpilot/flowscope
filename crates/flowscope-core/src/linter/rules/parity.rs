//! Additional SQLFluff-parity lint rules.
//!
//! These rules provide broad coverage for SQLFluff rule families that are not
//! deeply modeled in the core AST lints yet. They intentionally use conservative
//! heuristics (regex / token pattern matching on statement SQL) to avoid excessive
//! false positives.
//!
//! ## Differences from SQLFluff
//!
//! Each rule here maps to a SQLFluff rule code but has **narrower scope**:
//!
//! - **No configuration options** — SQLFluff rules often support `allow_*`,
//!   `prefer_*`, and case-style knobs. Parity rules use fixed defaults.
//! - **Regex-based detection** — Unlike SQLFluff's token-level traversal, these
//!   rules match patterns on the raw SQL text. They may miss complex cases and
//!   may produce false positives on SQL embedded inside string literals.
//! - **No auto-fix** — Parity rules are detection-only; `--fix` is not supported.
//!
//! See `docs/sqlfluff-gap-matrix.md` for the full mapping and per-rule notes.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue, Span};
use regex::Regex;
use sqlparser::ast::*;
use std::collections::HashSet;

macro_rules! define_predicate_rule {
    ($name:ident, $code:path, $rule_name:expr, $desc:expr, $severity:ident, $predicate:ident, $message:expr) => {
        pub struct $name;

        impl LintRule for $name {
            fn code(&self) -> &'static str {
                $code
            }

            fn name(&self) -> &'static str {
                $rule_name
            }

            fn description(&self) -> &'static str {
                $desc
            }

            fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
                if $predicate(stmt, ctx) {
                    vec![Issue::$severity($code, $message).with_statement(ctx.statement_index)]
                } else {
                    Vec::new()
                }
            }
        }
    };
}

fn stmt_sql<'a>(ctx: &'a LintContext<'a>) -> &'a str {
    ctx.statement_sql()
}

fn is_keyword(token: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "ALL",
        "ALTER",
        "AND",
        "ANY",
        "ANTI",
        "ARRAY",
        "AS",
        "ASC",
        "BEGIN",
        "BETWEEN",
        "BY",
        "CAST",
        "CASE",
        "CONFLICT",
        "CONSTRAINT",
        "CREATE",
        "CROSS",
        "CURRENT",
        "CURRENT_DATE",
        "CURRENT_TIME",
        "CURRENT_TIMESTAMP",
        "DATE",
        "DAY",
        "DECIMAL",
        "DELETE",
        "DESC",
        "DISTINCT",
        "DO",
        "DOUBLE",
        "DROP",
        "DOW",
        "DOY",
        "EPOCH",
        "ELSE",
        "END",
        "EXCEPT",
        "EXCLUDED",
        "EXISTS",
        "FALSE",
        "FETCH",
        "FILTER",
        "FIRST",
        "FLOAT",
        "FOLLOWING",
        "FOR",
        "FOREIGN",
        "FROM",
        "HOUR",
        "FULL",
        "GO",
        "GROUP",
        "HAVING",
        "IF",
        "ILIKE",
        "IN",
        "INNER",
        "INSERT",
        "INTEGER",
        "INTERSECT",
        "INTERVAL",
        "ISODOW",
        "ISOYEAR",
        "INTO",
        "IS",
        "JOIN",
        "KEY",
        "LAST",
        "LATERAL",
        "LEFT",
        "LIKE",
        "LIMIT",
        "LOCALTIME",
        "LOCALTIMESTAMP",
        "MATERIALIZED",
        "NATURAL",
        "NO",
        "MONTH",
        "NOT",
        "NULL",
        "NULLS",
        "NUMERIC",
        "OFFSET",
        "ON",
        "ONLY",
        "OR",
        "ORDER",
        "OUTER",
        "OVER",
        "OVERWRITE",
        "PARTITION",
        "PRECEDING",
        "PRIMARY",
        "QUALIFY",
        "RANGE",
        "RECURSIVE",
        "REFERENCES",
        "RETURNING",
        "RIGHT",
        "ROW",
        "ROWS",
        "SECOND",
        "SELECT",
        "SEMI",
        "SET",
        "SMALLINT",
        "SOME",
        "STRAIGHT",
        "TABLE",
        "TEXT",
        "THEN",
        "TIMESTAMP",
        "WEEK",
        "YEAR",
        "TINYINT",
        "TRUE",
        "UNBOUNDED",
        "UNION",
        "UNNEST",
        "UPDATE",
        "USING",
        "UUID",
        "VALUES",
        "VARCHAR",
        "VIEW",
        "WHEN",
        "WHERE",
        "WINDOW",
        "WITH",
        "WITHIN",
        "WITHOUT",
    ];
    KEYWORDS.contains(&token.to_ascii_uppercase().as_str())
}

fn has_re(sql: &str, pattern: &str) -> bool {
    Regex::new(pattern)
        .expect("valid parity regex")
        .is_match(sql)
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_keyword_at(sql: &str, idx: usize, keyword: &str) -> bool {
    let bytes = sql.as_bytes();
    let kw = keyword.as_bytes();
    if idx + kw.len() > bytes.len() {
        return false;
    }
    if idx > 0 && is_word_byte(bytes[idx - 1]) {
        return false;
    }
    if idx + kw.len() < bytes.len() && is_word_byte(bytes[idx + kw.len()]) {
        return false;
    }
    bytes[idx..idx + kw.len()].eq_ignore_ascii_case(kw)
}

fn skip_whitespace_and_comments(sql: &str, mut idx: usize) -> usize {
    let bytes = sql.as_bytes();
    while idx < bytes.len() {
        if bytes[idx].is_ascii_whitespace() {
            idx += 1;
            continue;
        }
        if idx + 1 < bytes.len() && bytes[idx] == b'-' && bytes[idx + 1] == b'-' {
            idx += 2;
            while idx < bytes.len() && bytes[idx] != b'\n' {
                idx += 1;
            }
            continue;
        }
        if idx + 1 < bytes.len() && bytes[idx] == b'/' && bytes[idx + 1] == b'*' {
            idx += 2;
            while idx + 1 < bytes.len() && !(bytes[idx] == b'*' && bytes[idx + 1] == b'/') {
                idx += 1;
            }
            if idx + 1 < bytes.len() {
                idx += 2;
            }
            continue;
        }
        break;
    }
    idx
}

fn matching_close_paren_ignoring_strings_and_comments(sql: &str, open_idx: usize) -> Option<usize> {
    let bytes = sql.as_bytes();
    if open_idx >= bytes.len() || bytes[open_idx] != b'(' {
        return None;
    }

    let mut idx = open_idx + 1;
    let mut depth = 1usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while idx < bytes.len() {
        if in_line_comment {
            if bytes[idx] == b'\n' {
                in_line_comment = false;
            }
            idx += 1;
            continue;
        }

        if in_block_comment {
            if idx + 1 < bytes.len() && bytes[idx] == b'*' && bytes[idx + 1] == b'/' {
                in_block_comment = false;
                idx += 2;
            } else {
                idx += 1;
            }
            continue;
        }

        if in_single {
            if bytes[idx] == b'\'' {
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'\'' {
                    idx += 2;
                } else {
                    in_single = false;
                    idx += 1;
                }
            } else {
                idx += 1;
            }
            continue;
        }

        if in_double {
            if bytes[idx] == b'"' {
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'"' {
                    idx += 2;
                } else {
                    in_double = false;
                    idx += 1;
                }
            } else {
                idx += 1;
            }
            continue;
        }

        if idx + 1 < bytes.len() && bytes[idx] == b'-' && bytes[idx + 1] == b'-' {
            in_line_comment = true;
            idx += 2;
            continue;
        }
        if idx + 1 < bytes.len() && bytes[idx] == b'/' && bytes[idx + 1] == b'*' {
            in_block_comment = true;
            idx += 2;
            continue;
        }
        if bytes[idx] == b'\'' {
            in_single = true;
            idx += 1;
            continue;
        }
        if bytes[idx] == b'"' {
            in_double = true;
            idx += 1;
            continue;
        }

        match bytes[idx] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
        idx += 1;
    }

    None
}

fn lt08_anchor_end(sql: &str, start: usize) -> usize {
    let bytes = sql.as_bytes();
    if start >= bytes.len() {
        return start;
    }

    if is_word_byte(bytes[start]) {
        let mut end = start + 1;
        while end < bytes.len() && is_word_byte(bytes[end]) {
            end += 1;
        }
        end
    } else {
        (start + 1).min(bytes.len())
    }
}

fn lt08_suffix_summary(sql: &str, mut idx: usize) -> (usize, Option<usize>, bool) {
    let bytes = sql.as_bytes();
    let mut blank_lines = 0usize;
    let mut line_blank = false;
    let mut saw_comma = false;

    while idx < bytes.len() {
        if idx + 1 < bytes.len() && bytes[idx] == b'-' && bytes[idx + 1] == b'-' {
            line_blank = false;
            idx += 2;
            while idx < bytes.len() && bytes[idx] != b'\n' {
                idx += 1;
            }
            continue;
        }

        if idx + 1 < bytes.len() && bytes[idx] == b'/' && bytes[idx + 1] == b'*' {
            line_blank = false;
            idx += 2;
            while idx + 1 < bytes.len() {
                if bytes[idx] == b'\n' {
                    if line_blank {
                        blank_lines += 1;
                    }
                    line_blank = true;
                    idx += 1;
                    continue;
                }

                if bytes[idx] == b'*' && bytes[idx + 1] == b'/' {
                    idx += 2;
                    break;
                }

                line_blank = false;
                idx += 1;
            }
            continue;
        }

        match bytes[idx] {
            b',' => {
                saw_comma = true;
                idx += 1;
            }
            b'\n' => {
                if line_blank {
                    blank_lines += 1;
                }
                line_blank = true;
                idx += 1;
            }
            b if b.is_ascii_whitespace() => idx += 1,
            _ => return (blank_lines, Some(idx), saw_comma),
        }
    }

    (blank_lines, None, saw_comma)
}

fn lt08_violation_spans(sql: &str) -> Vec<(usize, usize)> {
    let bytes = sql.as_bytes();
    let mut spans = Vec::new();

    let mut idx = skip_whitespace_and_comments(sql, 0);
    if !is_keyword_at(sql, idx, "WITH") {
        return spans;
    }
    idx += "WITH".len();
    idx = skip_whitespace_and_comments(sql, idx);
    if is_keyword_at(sql, idx, "RECURSIVE") {
        idx += "RECURSIVE".len();
    }

    while idx < bytes.len() {
        idx = skip_whitespace_and_comments(sql, idx);
        if idx >= bytes.len() {
            break;
        }

        if !is_keyword_at(sql, idx, "AS") {
            idx += 1;
            continue;
        }

        let mut body_start = skip_whitespace_and_comments(sql, idx + "AS".len());
        if is_keyword_at(sql, body_start, "NOT") {
            body_start = skip_whitespace_and_comments(sql, body_start + "NOT".len());
        }
        if is_keyword_at(sql, body_start, "MATERIALIZED") {
            body_start = skip_whitespace_and_comments(sql, body_start + "MATERIALIZED".len());
        }
        if body_start >= bytes.len() || bytes[body_start] != b'(' {
            idx += 1;
            continue;
        }

        let Some(close_idx) = matching_close_paren_ignoring_strings_and_comments(sql, body_start)
        else {
            break;
        };

        let (blank_lines, next_code_idx, saw_comma) = lt08_suffix_summary(sql, close_idx + 1);
        if blank_lines == 0 {
            if let Some(start) = next_code_idx {
                spans.push((start, lt08_anchor_end(sql, start)));
            }
        }

        if !saw_comma {
            break;
        }
        let Some(next_idx) = next_code_idx else {
            break;
        };
        idx = next_idx;
    }

    spans
}

fn capture_group(sql: &str, pattern: &str, group_idx: usize) -> Vec<String> {
    Regex::new(pattern)
        .expect("valid parity regex")
        .captures_iter(sql)
        .filter_map(|caps| caps.get(group_idx))
        .map(|m| m.as_str().to_string())
        .collect()
}

fn capture_group_with_spans(
    sql: &str,
    pattern: &str,
    group_idx: usize,
) -> Vec<(String, usize, usize)> {
    Regex::new(pattern)
        .expect("valid parity regex")
        .captures_iter(sql)
        .filter_map(|caps| caps.get(group_idx))
        .map(|m| (m.as_str().to_string(), m.start(), m.end()))
        .collect()
}

fn mask_comments_and_single_quoted_strings(sql: &str) -> String {
    enum State {
        Normal,
        LineComment,
        BlockComment,
        SingleQuoted,
    }

    let mut bytes = sql.as_bytes().to_vec();
    let mut i = 0usize;
    let mut state = State::Normal;

    while i < bytes.len() {
        match state {
            State::Normal => {
                if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                    bytes[i] = b' ';
                    bytes[i + 1] = b' ';
                    i += 2;
                    state = State::LineComment;
                } else if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    bytes[i] = b' ';
                    bytes[i + 1] = b' ';
                    i += 2;
                    state = State::BlockComment;
                } else if bytes[i] == b'\'' {
                    bytes[i] = b' ';
                    i += 1;
                    state = State::SingleQuoted;
                } else {
                    i += 1;
                }
            }
            State::LineComment => {
                if bytes[i] == b'\n' {
                    i += 1;
                    state = State::Normal;
                } else {
                    bytes[i] = b' ';
                    i += 1;
                }
            }
            State::BlockComment => {
                if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    bytes[i] = b' ';
                    bytes[i + 1] = b' ';
                    i += 2;
                    state = State::Normal;
                } else if bytes[i] == b'\n' {
                    i += 1;
                } else {
                    bytes[i] = b' ';
                    i += 1;
                }
            }
            State::SingleQuoted => {
                if bytes[i] == b'\'' {
                    // Escaped quote in SQL string literal: ''
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        bytes[i] = b' ';
                        bytes[i + 1] = b' ';
                        i += 2;
                    } else {
                        bytes[i] = b' ';
                        i += 1;
                        state = State::Normal;
                    }
                } else if bytes[i] == b'\n' {
                    i += 1;
                } else {
                    bytes[i] = b' ';
                    i += 1;
                }
            }
        }
    }

    String::from_utf8(bytes).expect("input SQL remains valid utf8 after masking")
}

fn qualifier_prefixes(sql: &str) -> Vec<String> {
    let re = Regex::new(r"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\.[A-Za-z_][A-Za-z0-9_]*\b")
        .expect("valid parity regex");
    re.captures_iter(sql)
        .filter_map(|caps| {
            let whole = caps.get(0)?;
            if is_source_qualifier_position(sql, whole.start()) {
                return None;
            }
            caps.get(1).map(|m| m.as_str().to_string())
        })
        .collect()
}

fn is_source_qualifier_position(sql: &str, qualifier_start: usize) -> bool {
    let prefix = sql[..qualifier_start].trim_end();

    has_re(prefix, r"(?i)\b(from|join|update|into|table)\s*$")
        || has_re(prefix, r"(?i)\bdelete\s+from\s*$")
}

fn duplicate_case_insensitive(values: &[String]) -> bool {
    let mut seen = HashSet::new();
    for value in values {
        let key = value.to_ascii_uppercase();
        if !seen.insert(key) {
            return true;
        }
    }
    false
}

fn first_duplicate_case_insensitive_value(values: &[String]) -> Option<String> {
    let mut seen = HashSet::new();
    for value in values {
        let key = value.to_ascii_uppercase();
        if !seen.insert(key) {
            return Some(value.clone());
        }
    }
    None
}

fn second_occurrence_case_insensitive_span(
    values: &[(String, usize, usize)],
    target: &str,
) -> Option<(usize, usize)> {
    let mut seen = 0usize;
    for (value, start, end) in values {
        if value.eq_ignore_ascii_case(target) {
            seen += 1;
            if seen == 2 {
                return Some((*start, *end));
            }
        }
    }
    None
}

fn table_refs(sql: &str) -> Vec<String> {
    capture_group(sql, r"(?i)\b(?:from|join)\s+([A-Za-z_][A-Za-z0-9_\.]*)", 1)
        .into_iter()
        .map(|name| name.rsplit('.').next().map(str::to_string).unwrap_or(name))
        .collect()
}

fn table_aliases_raw(sql: &str) -> Vec<String> {
    capture_group(
        sql,
        r"(?i)\b(?:from|join)\s+[A-Za-z_][A-Za-z0-9_\.]*\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*)",
        1,
    )
}

fn table_aliases(sql: &str) -> Vec<String> {
    table_aliases_raw(sql)
        .into_iter()
        .filter(|alias| !is_keyword(alias))
        .collect()
}

fn previous_significant_token(sql: &str, before: usize) -> Option<(String, usize)> {
    let bytes = sql.as_bytes();
    let mut idx = before;
    while idx > 0 && bytes[idx - 1].is_ascii_whitespace() {
        idx -= 1;
    }

    if idx == 0 {
        return None;
    }

    if bytes[idx - 1] == b',' {
        return Some((",".to_string(), idx - 1));
    }

    let token_end = idx;
    while idx > 0 {
        let b = bytes[idx - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            idx -= 1;
        } else {
            break;
        }
    }

    if idx == token_end {
        return None;
    }

    Some((sql[idx..token_end].to_ascii_uppercase(), idx))
}

fn matching_open_paren(sql: &str, close_paren_idx: usize) -> Option<usize> {
    let bytes = sql.as_bytes();
    let mut depth = 0usize;

    for idx in (0..=close_paren_idx).rev() {
        match bytes[idx] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }

    None
}

fn is_derived_table_alias(sql: &str, alias_start: usize) -> bool {
    let bytes = sql.as_bytes();
    let mut idx = alias_start;
    while idx > 0 && bytes[idx - 1].is_ascii_whitespace() {
        idx -= 1;
    }

    if idx == 0 || bytes[idx - 1] != b')' {
        return false;
    }

    let close_paren_idx = idx - 1;
    let Some(open_paren_idx) = matching_open_paren(sql, close_paren_idx) else {
        return false;
    };

    let Some((mut token, token_start)) = previous_significant_token(sql, open_paren_idx) else {
        return false;
    };

    if token == "LATERAL" {
        let Some((prev_token, _)) = previous_significant_token(sql, token_start) else {
            return false;
        };
        token = prev_token;
    }

    if token != "FROM" && token != "JOIN" && token != "," {
        return false;
    }

    let inner = &sql[open_paren_idx + 1..close_paren_idx];
    has_re(inner, r"(?i)\bselect\b")
}

fn implicit_table_alias_spans(sql: &str) -> Vec<(usize, usize)> {
    let mut spans: Vec<(usize, usize)> = Vec::new();

    for (alias, start, end) in capture_group_with_spans(
        sql,
        r"(?i)\b(?:from|join)\s+(?:only\s+)?(?:[A-Za-z_][A-Za-z0-9_\$]*)(?:\.[A-Za-z_][A-Za-z0-9_\$]*)*\s+([A-Za-z_][A-Za-z0-9_]*)",
        1,
    ) {
        if !is_keyword(&alias) {
            spans.push((start, end));
        }
    }

    for (alias, start, end) in capture_group_with_spans(
        sql,
        r"(?is)\b(?:from|join)\s+(?:lateral\s+)?[A-Za-z_][A-Za-z0-9_]*\s*\([^)]*\)\s+([A-Za-z_][A-Za-z0-9_]*)",
        1,
    ) {
        if !is_keyword(&alias) {
            spans.push((start, end));
        }
    }

    for (alias, start, end) in
        capture_group_with_spans(sql, r"(?i)\)\s+([A-Za-z_][A-Za-z0-9_]*)", 1)
    {
        if is_keyword(&alias) {
            continue;
        }
        if is_derived_table_alias(sql, start) {
            spans.push((start, end));
        }
    }

    spans.sort_unstable();
    spans.dedup();
    spans
}

fn table_aliases_with_spans(sql: &str) -> Vec<(String, usize, usize)> {
    capture_group_with_spans(
        sql,
        r"(?i)\b(?:from|join)\s+[A-Za-z_][A-Za-z0-9_\.]*\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*)",
        1,
    )
    .into_iter()
    .filter(|(alias, _, _)| !is_keyword(alias))
    .collect()
}

fn join_aliases(sql: &str) -> Vec<String> {
    capture_group(
        sql,
        r"(?i)\bjoin\s+[A-Za-z_][A-Za-z0-9_\.]*\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*)",
        1,
    )
    .into_iter()
    .filter(|alias| !is_keyword(alias))
    .collect()
}

fn update_target_refs(sql: &str) -> Vec<String> {
    capture_group(sql, r"(?i)\bupdate\s+([A-Za-z_][A-Za-z0-9_\.]*)", 1)
        .into_iter()
        .map(|name| name.rsplit('.').next().map(str::to_string).unwrap_or(name))
        .collect()
}

fn update_target_aliases(sql: &str) -> Vec<String> {
    capture_group(
        sql,
        r"(?i)\bupdate\s+[A-Za-z_][A-Za-z0-9_\.]*\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*)",
        1,
    )
    .into_iter()
    .filter(|alias| !is_keyword(alias))
    .collect()
}

fn column_aliases(sql: &str) -> Vec<String> {
    capture_group(sql, r"(?i)\bas\s+([A-Za-z_][A-Za-z0-9_]*)", 1)
}

fn column_aliases_with_spans(sql: &str) -> Vec<(String, usize, usize)> {
    capture_group_with_spans(sql, r"(?i)\bas\s+([A-Za-z_][A-Za-z0-9_]*)", 1)
}

fn table_factor_alias_name(table_factor: &TableFactor) -> Option<&str> {
    let alias = match table_factor {
        TableFactor::Table { alias, .. }
        | TableFactor::Derived { alias, .. }
        | TableFactor::TableFunction { alias, .. }
        | TableFactor::Function { alias, .. }
        | TableFactor::UNNEST { alias, .. }
        | TableFactor::JsonTable { alias, .. }
        | TableFactor::OpenJsonTable { alias, .. }
        | TableFactor::NestedJoin { alias, .. }
        | TableFactor::Pivot { alias, .. }
        | TableFactor::Unpivot { alias, .. }
        | TableFactor::MatchRecognize { alias, .. }
        | TableFactor::XmlTable { alias, .. }
        | TableFactor::SemanticView { alias, .. } => alias.as_ref(),
    }?;

    Some(alias.name.value.as_str())
}

fn collect_scope_table_aliases(table_with_joins: &TableWithJoins, aliases: &mut Vec<String>) {
    collect_scope_table_aliases_from_factor(&table_with_joins.relation, aliases);
    for join in &table_with_joins.joins {
        collect_scope_table_aliases_from_factor(&join.relation, aliases);
    }
}

fn collect_scope_table_aliases_from_factor(table_factor: &TableFactor, aliases: &mut Vec<String>) {
    if let Some(alias) = table_factor_alias_name(table_factor) {
        if !is_keyword(alias) {
            aliases.push(alias.to_string());
        }
    }

    match table_factor {
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => collect_scope_table_aliases(table_with_joins, aliases),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_scope_table_aliases_from_factor(table, aliases)
        }
        _ => {}
    }
}

fn first_duplicate_table_alias_in_statement(stmt: &Statement) -> Option<String> {
    match stmt {
        Statement::Query(query) => first_duplicate_table_alias_in_query(query),
        Statement::Insert(insert) => insert
            .source
            .as_deref()
            .and_then(first_duplicate_table_alias_in_query),
        Statement::CreateView { query, .. } => first_duplicate_table_alias_in_query(query),
        Statement::CreateTable(create) => create
            .query
            .as_deref()
            .and_then(first_duplicate_table_alias_in_query),
        _ => None,
    }
}

fn first_duplicate_table_alias_in_query(query: &Query) -> Option<String> {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            if let Some(duplicate) = first_duplicate_table_alias_in_query(&cte.query) {
                return Some(duplicate);
            }
        }
    }

    first_duplicate_table_alias_in_set_expr(&query.body)
}

fn first_duplicate_table_alias_in_set_expr(expr: &SetExpr) -> Option<String> {
    match expr {
        SetExpr::Select(select) => first_duplicate_table_alias_in_select(select),
        SetExpr::Query(query) => first_duplicate_table_alias_in_query(query),
        SetExpr::SetOperation { left, right, .. } => first_duplicate_table_alias_in_set_expr(left)
            .or_else(|| first_duplicate_table_alias_in_set_expr(right)),
        SetExpr::Insert(stmt)
        | SetExpr::Update(stmt)
        | SetExpr::Delete(stmt)
        | SetExpr::Merge(stmt) => first_duplicate_table_alias_in_statement(stmt),
        _ => None,
    }
}

fn first_duplicate_table_alias_in_select(select: &Select) -> Option<String> {
    let mut aliases = Vec::new();
    for table_with_joins in &select.from {
        collect_scope_table_aliases(table_with_joins, &mut aliases);
    }

    if let Some(duplicate) = first_duplicate_case_insensitive_value(&aliases) {
        return Some(duplicate);
    }

    for table_with_joins in &select.from {
        if let Some(duplicate) =
            first_duplicate_table_alias_in_table_with_joins_children(table_with_joins)
        {
            return Some(duplicate);
        }
    }

    None
}

fn first_duplicate_table_alias_in_table_with_joins_children(
    table_with_joins: &TableWithJoins,
) -> Option<String> {
    first_duplicate_table_alias_in_table_factor_children(&table_with_joins.relation).or_else(|| {
        for join in &table_with_joins.joins {
            if let Some(duplicate) =
                first_duplicate_table_alias_in_table_factor_children(&join.relation)
            {
                return Some(duplicate);
            }
        }
        None
    })
}

fn first_duplicate_table_alias_in_table_factor_children(
    table_factor: &TableFactor,
) -> Option<String> {
    match table_factor {
        TableFactor::Derived { subquery, .. } => first_duplicate_table_alias_in_query(subquery),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => first_duplicate_table_alias_in_select_like(table_with_joins),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            first_duplicate_table_alias_in_table_factor_children(table)
        }
        _ => None,
    }
}

fn first_duplicate_table_alias_in_select_like(table_with_joins: &TableWithJoins) -> Option<String> {
    let mut aliases = Vec::new();
    collect_scope_table_aliases(table_with_joins, &mut aliases);
    if let Some(duplicate) = first_duplicate_case_insensitive_value(&aliases) {
        return Some(duplicate);
    }

    first_duplicate_table_alias_in_table_with_joins_children(table_with_joins)
}

fn first_duplicate_column_alias_in_statement(stmt: &Statement) -> Option<String> {
    match stmt {
        Statement::Query(query) => first_duplicate_column_alias_in_query(query),
        Statement::Insert(insert) => insert
            .source
            .as_deref()
            .and_then(first_duplicate_column_alias_in_query),
        Statement::CreateView { query, .. } => first_duplicate_column_alias_in_query(query),
        Statement::CreateTable(create) => create
            .query
            .as_deref()
            .and_then(first_duplicate_column_alias_in_query),
        _ => None,
    }
}

fn first_duplicate_column_alias_in_query(query: &Query) -> Option<String> {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            if let Some(duplicate) = first_duplicate_column_alias_in_query(&cte.query) {
                return Some(duplicate);
            }
        }
    }

    first_duplicate_column_alias_in_set_expr(&query.body)
}

fn first_duplicate_column_alias_in_set_expr(expr: &SetExpr) -> Option<String> {
    match expr {
        SetExpr::Select(select) => first_duplicate_column_alias_in_select(select),
        SetExpr::Query(query) => first_duplicate_column_alias_in_query(query),
        SetExpr::SetOperation { left, right, .. } => first_duplicate_column_alias_in_set_expr(left)
            .or_else(|| first_duplicate_column_alias_in_set_expr(right)),
        SetExpr::Insert(stmt)
        | SetExpr::Update(stmt)
        | SetExpr::Delete(stmt)
        | SetExpr::Merge(stmt) => first_duplicate_column_alias_in_statement(stmt),
        _ => None,
    }
}

fn first_duplicate_column_alias_in_select(select: &Select) -> Option<String> {
    let mut aliases = Vec::new();
    for item in &select.projection {
        if let SelectItem::ExprWithAlias { alias, .. } = item {
            aliases.push(alias.value.clone());
        }
    }

    if let Some(duplicate) = first_duplicate_case_insensitive_value(&aliases) {
        return Some(duplicate);
    }

    for table_with_joins in &select.from {
        if let Some(duplicate) =
            first_duplicate_column_alias_in_table_with_joins_children(table_with_joins)
        {
            return Some(duplicate);
        }
    }

    None
}

fn first_duplicate_column_alias_in_table_with_joins_children(
    table_with_joins: &TableWithJoins,
) -> Option<String> {
    first_duplicate_column_alias_in_table_factor_children(&table_with_joins.relation).or_else(
        || {
            for join in &table_with_joins.joins {
                if let Some(duplicate) =
                    first_duplicate_column_alias_in_table_factor_children(&join.relation)
                {
                    return Some(duplicate);
                }
            }
            None
        },
    )
}

fn first_duplicate_column_alias_in_table_factor_children(
    table_factor: &TableFactor,
) -> Option<String> {
    match table_factor {
        TableFactor::Derived { subquery, .. } => first_duplicate_column_alias_in_query(subquery),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => first_duplicate_column_alias_in_select_like(table_with_joins),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            first_duplicate_column_alias_in_table_factor_children(table)
        }
        _ => None,
    }
}

fn first_duplicate_column_alias_in_select_like(
    table_with_joins: &TableWithJoins,
) -> Option<String> {
    first_duplicate_column_alias_in_table_with_joins_children(table_with_joins)
}

fn statement_any_select(stmt: &Statement, predicate: fn(&Select) -> bool) -> bool {
    match stmt {
        Statement::Query(query) => query_any_select(query, predicate),
        Statement::Insert(insert) => insert
            .source
            .as_deref()
            .is_some_and(|query| query_any_select(query, predicate)),
        Statement::CreateView { query, .. } => query_any_select(query, predicate),
        Statement::CreateTable(create) => create
            .query
            .as_deref()
            .is_some_and(|query| query_any_select(query, predicate)),
        _ => false,
    }
}

fn query_any_select(query: &Query, predicate: fn(&Select) -> bool) -> bool {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            if query_any_select(&cte.query, predicate) {
                return true;
            }
        }
    }

    set_expr_any_select(&query.body, predicate)
}

fn set_expr_any_select(expr: &SetExpr, predicate: fn(&Select) -> bool) -> bool {
    match expr {
        SetExpr::Select(select) => {
            predicate(select)
                || select.from.iter().any(|table_with_joins| {
                    table_with_joins_any_select(table_with_joins, predicate)
                })
        }
        SetExpr::Query(query) => query_any_select(query, predicate),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_any_select(left, predicate) || set_expr_any_select(right, predicate)
        }
        SetExpr::Insert(stmt)
        | SetExpr::Update(stmt)
        | SetExpr::Delete(stmt)
        | SetExpr::Merge(stmt) => statement_any_select(stmt, predicate),
        _ => false,
    }
}

fn table_with_joins_any_select(
    table_with_joins: &TableWithJoins,
    predicate: fn(&Select) -> bool,
) -> bool {
    table_factor_any_select(&table_with_joins.relation, predicate)
        || table_with_joins
            .joins
            .iter()
            .any(|join| table_factor_any_select(&join.relation, predicate))
}

fn table_factor_any_select(table_factor: &TableFactor, predicate: fn(&Select) -> bool) -> bool {
    match table_factor {
        TableFactor::Derived { subquery, .. } => query_any_select(subquery, predicate),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => table_with_joins_any_select(table_with_joins, predicate),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => table_factor_any_select(table, predicate),
        _ => false,
    }
}

fn statement_sum_select(stmt: &Statement, counter: fn(&Select) -> usize) -> usize {
    match stmt {
        Statement::Query(query) => query_sum_select(query, counter),
        Statement::Insert(insert) => insert
            .source
            .as_deref()
            .map_or(0, |query| query_sum_select(query, counter)),
        Statement::CreateView { query, .. } => query_sum_select(query, counter),
        Statement::CreateTable(create) => create
            .query
            .as_deref()
            .map_or(0, |query| query_sum_select(query, counter)),
        _ => 0,
    }
}

fn query_sum_select(query: &Query, counter: fn(&Select) -> usize) -> usize {
    let mut total = 0usize;

    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            total += query_sum_select(&cte.query, counter);
        }
    }

    total += set_expr_sum_select(&query.body, counter);

    if let Some(order_by) = &query.order_by {
        if let SetExpr::Select(select) = &*query.body {
            if select_source_count(select) > 1 {
                let aliases = select_projection_alias_set(select);
                total += count_unqualified_references_in_order_by(order_by, &aliases);
            }
        }
    }

    total
}

fn set_expr_sum_select(expr: &SetExpr, counter: fn(&Select) -> usize) -> usize {
    match expr {
        SetExpr::Select(select) => {
            counter(select)
                + select
                    .from
                    .iter()
                    .map(|table_with_joins| table_with_joins_sum_select(table_with_joins, counter))
                    .sum::<usize>()
        }
        SetExpr::Query(query) => query_sum_select(query, counter),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_sum_select(left, counter) + set_expr_sum_select(right, counter)
        }
        SetExpr::Insert(stmt)
        | SetExpr::Update(stmt)
        | SetExpr::Delete(stmt)
        | SetExpr::Merge(stmt) => statement_sum_select(stmt, counter),
        _ => 0,
    }
}

fn table_with_joins_sum_select(
    table_with_joins: &TableWithJoins,
    counter: fn(&Select) -> usize,
) -> usize {
    table_factor_sum_select(&table_with_joins.relation, counter)
        + table_with_joins
            .joins
            .iter()
            .map(|join| table_factor_sum_select(&join.relation, counter))
            .sum::<usize>()
}

fn table_factor_sum_select(table_factor: &TableFactor, counter: fn(&Select) -> usize) -> usize {
    match table_factor {
        TableFactor::Derived { subquery, .. } => query_sum_select(subquery, counter),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => table_with_joins_sum_select(table_with_joins, counter),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => table_factor_sum_select(table, counter),
        _ => 0,
    }
}

fn select_source_count(select: &Select) -> usize {
    let mut count = 0usize;
    for table_with_joins in &select.from {
        count += 1;
        count += table_with_joins.joins.len();
    }
    count
}

fn select_single_source_is_table_function(select: &Select) -> bool {
    if select.from.len() != 1 {
        return false;
    }

    let source = &select.from[0];
    if !source.joins.is_empty() {
        return false;
    }

    matches!(
        source.relation,
        TableFactor::UNNEST { .. }
            | TableFactor::Function { .. }
            | TableFactor::TableFunction { .. }
    )
}

fn select_mixed_reference_count_single_table(select: &Select) -> usize {
    let nested = select_nested_mixed_reference_count(select);

    if select_source_count(select) != 1 || select_single_source_is_table_function(select) {
        return nested;
    }

    let (qualified, unqualified) = select_reference_qualification_counts(select);
    if qualified == 0 || unqualified == 0 {
        return nested;
    }

    let qualified_prefix_count = select_qualified_prefix_count(select);
    let correlated_prefix_bonus = usize::from(unqualified == 1 && qualified_prefix_count > 1);
    let style_flip_bonus = usize::from(unqualified > 1 && qualified_prefix_count == 1);

    unqualified + correlated_prefix_bonus + style_flip_bonus + nested
}

fn select_nested_mixed_reference_count(select: &Select) -> usize {
    let mut total = 0usize;

    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                total += mixed_nested_subquery_count_in_expr(expr);
            }
            SelectItem::QualifiedWildcard(..) | SelectItem::Wildcard(..) => {}
        }
    }

    if let Some(selection) = &select.selection {
        total += mixed_nested_subquery_count_in_expr(selection);
    }

    if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
        total += exprs
            .iter()
            .map(mixed_nested_subquery_count_in_expr)
            .sum::<usize>();
    }

    if let Some(having) = &select.having {
        total += mixed_nested_subquery_count_in_expr(having);
    }

    for table_with_joins in &select.from {
        for join in &table_with_joins.joins {
            let Some(expr) = join_on_expr(&join.join_operator) else {
                continue;
            };
            total += mixed_nested_subquery_count_in_expr(expr);
        }
    }

    total
}

fn mixed_nested_subquery_count_in_expr(expr: &Expr) -> usize {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            mixed_nested_subquery_count_in_expr(left) + mixed_nested_subquery_count_in_expr(right)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => mixed_nested_subquery_count_in_expr(inner),
        Expr::InList { expr, list, .. } => {
            mixed_nested_subquery_count_in_expr(expr)
                + list
                    .iter()
                    .map(mixed_nested_subquery_count_in_expr)
                    .sum::<usize>()
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            mixed_nested_subquery_count_in_expr(expr)
                + mixed_nested_subquery_count_in_expr(low)
                + mixed_nested_subquery_count_in_expr(high)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let operand_count = operand
                .as_ref()
                .map_or(0, |expr| mixed_nested_subquery_count_in_expr(expr));
            let when_count = conditions
                .iter()
                .map(|case_when| {
                    mixed_nested_subquery_count_in_expr(&case_when.condition)
                        + mixed_nested_subquery_count_in_expr(&case_when.result)
                })
                .sum::<usize>();
            let else_count = else_result
                .as_ref()
                .map_or(0, |expr| mixed_nested_subquery_count_in_expr(expr));
            operand_count + when_count + else_count
        }
        Expr::Function(func) => {
            let args_count = match &func.args {
                FunctionArguments::List(arg_list) => arg_list
                    .args
                    .iter()
                    .map(|arg| match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(inner))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(inner),
                            ..
                        } => mixed_nested_subquery_count_in_expr(inner),
                        _ => 0,
                    })
                    .sum::<usize>(),
                FunctionArguments::Subquery(query) => {
                    query_sum_select(query, select_mixed_reference_count_single_table)
                }
                FunctionArguments::None => 0,
            };

            let filter_count = func
                .filter
                .as_ref()
                .map_or(0, |expr| mixed_nested_subquery_count_in_expr(expr));
            let within_group_count = func
                .within_group
                .iter()
                .map(|order_expr| mixed_nested_subquery_count_in_expr(&order_expr.expr))
                .sum::<usize>();
            let over_count = match &func.over {
                Some(WindowType::WindowSpec(spec)) => {
                    spec.partition_by
                        .iter()
                        .map(mixed_nested_subquery_count_in_expr)
                        .sum::<usize>()
                        + spec
                            .order_by
                            .iter()
                            .map(|order_expr| mixed_nested_subquery_count_in_expr(&order_expr.expr))
                            .sum::<usize>()
                }
                _ => 0,
            };
            args_count + filter_count + within_group_count + over_count
        }
        Expr::InSubquery { expr, subquery, .. } => {
            mixed_nested_subquery_count_in_expr(expr)
                + query_sum_select(subquery, select_mixed_reference_count_single_table)
        }
        Expr::Exists { subquery, .. } | Expr::Subquery(subquery) => {
            query_sum_select(subquery, select_mixed_reference_count_single_table)
        }
        Expr::AnyOp { left, right, .. } | Expr::AllOp { left, right, .. } => {
            let right_count = match right.as_ref() {
                Expr::Subquery(subquery) => {
                    query_sum_select(subquery, select_mixed_reference_count_single_table)
                }
                other => mixed_nested_subquery_count_in_expr(other),
            };
            mixed_nested_subquery_count_in_expr(left) + right_count
        }
        _ => 0,
    }
}

fn select_reference_qualification_counts(select: &Select) -> (usize, usize) {
    let mut qualified = 0usize;
    let mut unqualified = 0usize;

    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                let (q, u) = count_reference_qualification_in_expr(expr);
                qualified += q;
                unqualified += u;
            }
            SelectItem::QualifiedWildcard(..) | SelectItem::Wildcard(..) => {}
        }
    }

    if let Some(selection) = &select.selection {
        let (q, u) = count_reference_qualification_in_expr(selection);
        qualified += q;
        unqualified += u;
    }

    let aliases = select_projection_alias_set(select);

    if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            let (q, u) = count_reference_qualification_in_expr_excluding_aliases(expr, &aliases);
            qualified += q;
            unqualified += u;
        }
    }

    if let Some(having) = &select.having {
        let (q, u) = count_reference_qualification_in_expr_excluding_aliases(having, &aliases);
        qualified += q;
        unqualified += u;
    }

    for table_with_joins in &select.from {
        for join in &table_with_joins.joins {
            let Some(expr) = join_on_expr(&join.join_operator) else {
                continue;
            };
            let (q, u) = count_reference_qualification_in_expr(expr);
            qualified += q;
            unqualified += u;
        }
    }

    (qualified, unqualified)
}

fn join_on_expr(join_operator: &JoinOperator) -> Option<&Expr> {
    let constraint = match join_operator {
        JoinOperator::Join(constraint)
        | JoinOperator::Inner(constraint)
        | JoinOperator::Left(constraint)
        | JoinOperator::LeftOuter(constraint)
        | JoinOperator::Right(constraint)
        | JoinOperator::RightOuter(constraint)
        | JoinOperator::FullOuter(constraint)
        | JoinOperator::CrossJoin(constraint)
        | JoinOperator::Semi(constraint)
        | JoinOperator::LeftSemi(constraint)
        | JoinOperator::RightSemi(constraint)
        | JoinOperator::Anti(constraint)
        | JoinOperator::LeftAnti(constraint)
        | JoinOperator::RightAnti(constraint)
        | JoinOperator::StraightJoin(constraint) => constraint,
        _ => return None,
    };

    if let JoinConstraint::On(expr) = constraint {
        Some(expr)
    } else {
        None
    }
}

fn select_has_mixed_join_condition_qualification(select: &Select) -> bool {
    if select_source_count(select) <= 1 {
        return false;
    }

    for table_with_joins in &select.from {
        for join in &table_with_joins.joins {
            let Some(expr) = join_on_expr(&join.join_operator) else {
                continue;
            };
            let mut has_qualified = false;
            let mut has_unqualified = false;
            mark_expr_qualification(expr, &mut has_qualified, &mut has_unqualified);
            if has_qualified && has_unqualified {
                return true;
            }
        }
    }

    false
}

fn table_factor_reference_name(table_factor: &TableFactor) -> Option<String> {
    if let Some(alias) = table_factor_alias_name(table_factor) {
        return Some(alias.to_ascii_uppercase());
    }

    match table_factor {
        TableFactor::Table { name, .. } => name
            .to_string()
            .rsplit('.')
            .next()
            .map(|part| part.trim_matches('"').to_ascii_uppercase()),
        _ => None,
    }
}

fn expr_qualified_prefix(expr: &Expr) -> Option<String> {
    match expr {
        Expr::CompoundIdentifier(parts) if parts.len() > 1 => {
            parts.first().map(|ident| ident.value.to_ascii_uppercase())
        }
        Expr::Nested(inner)
        | Expr::UnaryOp { expr: inner, .. }
        | Expr::Cast { expr: inner, .. } => expr_qualified_prefix(inner),
        _ => None,
    }
}

fn has_reversed_join_pair_in_expr(
    expr: &Expr,
    current_source: &str,
    previous_source: &str,
) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            let direct_match = if *op == BinaryOperator::Eq {
                if let (Some(left_prefix), Some(right_prefix)) =
                    (expr_qualified_prefix(left), expr_qualified_prefix(right))
                {
                    left_prefix == current_source && right_prefix == previous_source
                } else {
                    false
                }
            } else {
                false
            };

            direct_match
                || has_reversed_join_pair_in_expr(left, current_source, previous_source)
                || has_reversed_join_pair_in_expr(right, current_source, previous_source)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            has_reversed_join_pair_in_expr(inner, current_source, previous_source)
        }
        Expr::InList { expr, list, .. } => {
            has_reversed_join_pair_in_expr(expr, current_source, previous_source)
                || list.iter().any(|item| {
                    has_reversed_join_pair_in_expr(item, current_source, previous_source)
                })
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            has_reversed_join_pair_in_expr(expr, current_source, previous_source)
                || has_reversed_join_pair_in_expr(low, current_source, previous_source)
                || has_reversed_join_pair_in_expr(high, current_source, previous_source)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand.as_ref().is_some_and(|expr| {
                has_reversed_join_pair_in_expr(expr, current_source, previous_source)
            }) || conditions.iter().any(|case_when| {
                has_reversed_join_pair_in_expr(
                    &case_when.condition,
                    current_source,
                    previous_source,
                ) || has_reversed_join_pair_in_expr(
                    &case_when.result,
                    current_source,
                    previous_source,
                )
            }) || else_result.as_ref().is_some_and(|expr| {
                has_reversed_join_pair_in_expr(expr, current_source, previous_source)
            })
        }
        _ => false,
    }
}

fn select_reversed_join_condition_count(select: &Select) -> usize {
    let mut has_reversed_join = false;
    let mut seen_sources: Vec<String> = Vec::new();

    for table_with_joins in &select.from {
        if let Some(base_name) = table_factor_reference_name(&table_with_joins.relation) {
            seen_sources.push(base_name);
        }

        for join in &table_with_joins.joins {
            let join_name = table_factor_reference_name(&join.relation);
            let previous_source = seen_sources.last().cloned();

            if let (Some(current_source), Some(previous_source), Some(expr)) = (
                join_name.as_ref(),
                previous_source.as_ref(),
                join_on_expr(&join.join_operator),
            ) {
                if has_reversed_join_pair_in_expr(expr, current_source, previous_source) {
                    has_reversed_join = true;
                }
            }

            if let Some(name) = join_name {
                seen_sources.push(name);
            }
        }
    }

    usize::from(has_reversed_join)
}

fn nested_reversed_join_count_in_expr(expr: &Expr) -> usize {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            nested_reversed_join_count_in_expr(left) + nested_reversed_join_count_in_expr(right)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => nested_reversed_join_count_in_expr(inner),
        Expr::InList { expr, list, .. } => {
            nested_reversed_join_count_in_expr(expr)
                + list
                    .iter()
                    .map(nested_reversed_join_count_in_expr)
                    .sum::<usize>()
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            nested_reversed_join_count_in_expr(expr)
                + nested_reversed_join_count_in_expr(low)
                + nested_reversed_join_count_in_expr(high)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let operand_count = operand
                .as_ref()
                .map_or(0, |expr| nested_reversed_join_count_in_expr(expr));
            let when_count = conditions
                .iter()
                .map(|case_when| {
                    nested_reversed_join_count_in_expr(&case_when.condition)
                        + nested_reversed_join_count_in_expr(&case_when.result)
                })
                .sum::<usize>();
            let else_count = else_result
                .as_ref()
                .map_or(0, |expr| nested_reversed_join_count_in_expr(expr));
            operand_count + when_count + else_count
        }
        Expr::Function(func) => {
            let args_count = match &func.args {
                FunctionArguments::List(arg_list) => arg_list
                    .args
                    .iter()
                    .map(|arg| match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(inner))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(inner),
                            ..
                        } => nested_reversed_join_count_in_expr(inner),
                        _ => 0,
                    })
                    .sum::<usize>(),
                FunctionArguments::Subquery(query) => {
                    query_sum_select(query, select_reversed_join_condition_count)
                }
                FunctionArguments::None => 0,
            };

            let filter_count = func
                .filter
                .as_ref()
                .map_or(0, |expr| nested_reversed_join_count_in_expr(expr));
            let within_group_count = func
                .within_group
                .iter()
                .map(|order_expr| nested_reversed_join_count_in_expr(&order_expr.expr))
                .sum::<usize>();
            let over_count = match &func.over {
                Some(WindowType::WindowSpec(spec)) => {
                    spec.partition_by
                        .iter()
                        .map(nested_reversed_join_count_in_expr)
                        .sum::<usize>()
                        + spec
                            .order_by
                            .iter()
                            .map(|order_expr| nested_reversed_join_count_in_expr(&order_expr.expr))
                            .sum::<usize>()
                }
                _ => 0,
            };
            args_count + filter_count + within_group_count + over_count
        }
        Expr::InSubquery { expr, subquery, .. } => {
            nested_reversed_join_count_in_expr(expr)
                + query_sum_select(subquery, select_reversed_join_condition_count)
        }
        Expr::Exists { subquery, .. } | Expr::Subquery(subquery) => {
            query_sum_select(subquery, select_reversed_join_condition_count)
        }
        Expr::AnyOp { left, right, .. } | Expr::AllOp { left, right, .. } => {
            let right_count = match right.as_ref() {
                Expr::Subquery(subquery) => {
                    query_sum_select(subquery, select_reversed_join_condition_count)
                }
                other => nested_reversed_join_count_in_expr(other),
            };
            nested_reversed_join_count_in_expr(left) + right_count
        }
        _ => 0,
    }
}

fn select_unqualified_reference_count(select: &Select, force_multi_source: bool) -> usize {
    let source_count = select_source_count(select);
    let qualified_prefix_count = select_qualified_prefix_count(select);

    // Match SQLFluff RF02 behavior more closely for correlated subqueries:
    // a subquery with one local source but two distinct qualifiers (local + outer)
    // is effectively multi-source for reference qualification.
    if !force_multi_source && source_count <= 1 && qualified_prefix_count < 2 {
        return select_forced_nested_subquery_count(select);
    }

    let mut total = 0usize;

    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                total += count_unqualified_references_in_expr(expr);
            }
            SelectItem::QualifiedWildcard(..) | SelectItem::Wildcard(..) => {}
        }
    }

    if let Some(selection) = &select.selection {
        total += count_unqualified_references_in_expr(selection);
    }

    let aliases = select_projection_alias_set(select);

    if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
        total += exprs
            .iter()
            .map(|expr| count_unqualified_references_in_expr_excluding_aliases(expr, &aliases))
            .sum::<usize>();
    }

    if let Some(having) = &select.having {
        total += count_unqualified_references_in_expr_excluding_aliases(having, &aliases);
    }

    for table_with_joins in &select.from {
        for join in &table_with_joins.joins {
            let Some(expr) = join_on_expr(&join.join_operator) else {
                continue;
            };
            total += count_unqualified_references_in_expr(expr);
        }
    }

    total
}

fn select_unqualified_reference_count_in_multi_table(select: &Select) -> usize {
    select_unqualified_reference_count(select, false)
}

fn select_unqualified_reference_count_forced(select: &Select) -> usize {
    select_unqualified_reference_count(select, true)
}

fn select_forced_nested_subquery_count(select: &Select) -> usize {
    let mut total = 0usize;

    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                total += forced_nested_subquery_count_in_expr(expr);
            }
            SelectItem::QualifiedWildcard(..) | SelectItem::Wildcard(..) => {}
        }
    }

    if let Some(selection) = &select.selection {
        total += forced_nested_subquery_count_in_expr(selection);
    }

    if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
        total += exprs
            .iter()
            .map(forced_nested_subquery_count_in_expr)
            .sum::<usize>();
    }

    if let Some(having) = &select.having {
        total += forced_nested_subquery_count_in_expr(having);
    }

    for table_with_joins in &select.from {
        for join in &table_with_joins.joins {
            let Some(expr) = join_on_expr(&join.join_operator) else {
                continue;
            };
            total += forced_nested_subquery_count_in_expr(expr);
        }
    }

    total
}

fn forced_nested_subquery_count_in_expr(expr: &Expr) -> usize {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            let mut total = forced_nested_subquery_count_in_expr(left)
                + forced_nested_subquery_count_in_expr(right);

            if let Expr::Subquery(subquery) = right.as_ref() {
                if expr_is_qualified_reference(left) {
                    total += query_sum_select(subquery, select_unqualified_reference_count_forced);
                }
            }
            if let Expr::Subquery(subquery) = left.as_ref() {
                if expr_is_qualified_reference(right) {
                    total += query_sum_select(subquery, select_unqualified_reference_count_forced);
                }
            }

            total
        }
        Expr::InSubquery { expr, subquery, .. } => {
            let nested = if expr_is_qualified_reference(expr) {
                query_sum_select(subquery, select_unqualified_reference_count_forced)
            } else {
                0
            };
            forced_nested_subquery_count_in_expr(expr) + nested
        }
        Expr::Exists { subquery, .. } => {
            query_sum_select(subquery, select_unqualified_reference_count_forced)
        }
        Expr::Subquery(_) => 0,
        Expr::AnyOp { left, right, .. } | Expr::AllOp { left, right, .. } => {
            let mut total = forced_nested_subquery_count_in_expr(left)
                + forced_nested_subquery_count_in_expr(right);
            if let Expr::Subquery(subquery) = right.as_ref() {
                if expr_is_qualified_reference(left) {
                    total += query_sum_select(subquery, select_unqualified_reference_count_forced);
                }
            }
            if let Expr::Subquery(subquery) = left.as_ref() {
                if expr_is_qualified_reference(right) {
                    total += query_sum_select(subquery, select_unqualified_reference_count_forced);
                }
            }
            total
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => forced_nested_subquery_count_in_expr(inner),
        Expr::InList { expr, list, .. } => {
            forced_nested_subquery_count_in_expr(expr)
                + list
                    .iter()
                    .map(forced_nested_subquery_count_in_expr)
                    .sum::<usize>()
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            forced_nested_subquery_count_in_expr(expr)
                + forced_nested_subquery_count_in_expr(low)
                + forced_nested_subquery_count_in_expr(high)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let operand_count = operand
                .as_ref()
                .map_or(0, |expr| forced_nested_subquery_count_in_expr(expr));
            let when_count = conditions
                .iter()
                .map(|case_when| {
                    forced_nested_subquery_count_in_expr(&case_when.condition)
                        + forced_nested_subquery_count_in_expr(&case_when.result)
                })
                .sum::<usize>();
            let else_count = else_result
                .as_ref()
                .map_or(0, |expr| forced_nested_subquery_count_in_expr(expr));
            operand_count + when_count + else_count
        }
        Expr::Function(func) => {
            let args_count = match &func.args {
                FunctionArguments::List(arg_list) => arg_list
                    .args
                    .iter()
                    .map(|arg| match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(inner))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(inner),
                            ..
                        } => forced_nested_subquery_count_in_expr(inner),
                        _ => 0,
                    })
                    .sum::<usize>(),
                FunctionArguments::Subquery(query) => {
                    query_sum_select(query, select_unqualified_reference_count_forced)
                }
                FunctionArguments::None => 0,
            };

            let filter_count = func
                .filter
                .as_ref()
                .map_or(0, |expr| forced_nested_subquery_count_in_expr(expr));

            let within_group_count = func
                .within_group
                .iter()
                .map(|order_expr| forced_nested_subquery_count_in_expr(&order_expr.expr))
                .sum::<usize>();

            let over_count = match &func.over {
                Some(WindowType::WindowSpec(spec)) => {
                    spec.partition_by
                        .iter()
                        .map(forced_nested_subquery_count_in_expr)
                        .sum::<usize>()
                        + spec
                            .order_by
                            .iter()
                            .map(|order_expr| {
                                forced_nested_subquery_count_in_expr(&order_expr.expr)
                            })
                            .sum::<usize>()
                }
                _ => 0,
            };

            args_count + filter_count + within_group_count + over_count
        }
        _ => 0,
    }
}

fn select_qualified_prefix_count(select: &Select) -> usize {
    let mut prefixes: HashSet<String> = HashSet::new();
    collect_qualified_prefixes_in_select(select, &mut prefixes);
    prefixes.len()
}

fn collect_qualified_prefixes_in_select(select: &Select, prefixes: &mut HashSet<String>) {
    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                collect_qualified_prefixes_in_expr(expr, prefixes)
            }
            SelectItem::QualifiedWildcard(..) => {}
            SelectItem::Wildcard(..) => {}
        }
    }

    if let Some(selection) = &select.selection {
        collect_qualified_prefixes_in_expr(selection, prefixes);
    }

    if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            collect_qualified_prefixes_in_expr(expr, prefixes);
        }
    }

    if let Some(having) = &select.having {
        collect_qualified_prefixes_in_expr(having, prefixes);
    }

    for table_with_joins in &select.from {
        for join in &table_with_joins.joins {
            let Some(expr) = join_on_expr(&join.join_operator) else {
                continue;
            };
            collect_qualified_prefixes_in_expr(expr, prefixes);
        }
    }
}

fn select_projection_alias_set(select: &Select) -> HashSet<String> {
    let mut aliases = HashSet::new();
    for item in &select.projection {
        if let SelectItem::ExprWithAlias { alias, .. } = item {
            aliases.insert(alias.value.to_ascii_uppercase());
        }
    }
    aliases
}

fn mark_expr_qualification(expr: &Expr, has_qualified: &mut bool, has_unqualified: &mut bool) {
    match expr {
        Expr::Identifier(_) => *has_unqualified = true,
        Expr::CompoundIdentifier(parts) => {
            if parts.len() > 1 {
                *has_qualified = true;
            } else {
                *has_unqualified = true;
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            mark_expr_qualification(left, has_qualified, has_unqualified);
            mark_expr_qualification(right, has_qualified, has_unqualified);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            mark_expr_qualification(inner, has_qualified, has_unqualified);
        }
        Expr::InList { expr, list, .. } => {
            mark_expr_qualification(expr, has_qualified, has_unqualified);
            for item in list {
                mark_expr_qualification(item, has_qualified, has_unqualified);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            mark_expr_qualification(expr, has_qualified, has_unqualified);
            mark_expr_qualification(low, has_qualified, has_unqualified);
            mark_expr_qualification(high, has_qualified, has_unqualified);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                mark_expr_qualification(op, has_qualified, has_unqualified);
            }
            for case_when in conditions {
                mark_expr_qualification(&case_when.condition, has_qualified, has_unqualified);
                mark_expr_qualification(&case_when.result, has_qualified, has_unqualified);
            }
            if let Some(otherwise) = else_result {
                mark_expr_qualification(otherwise, has_qualified, has_unqualified);
            }
        }
        Expr::Function(func) => {
            if let FunctionArguments::List(arg_list) = &func.args {
                for arg in &arg_list.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(inner))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(inner),
                            ..
                        } => mark_expr_qualification(inner, has_qualified, has_unqualified),
                        _ => {}
                    }
                }
            }
        }
        Expr::InSubquery { expr, .. } => {
            mark_expr_qualification(expr, has_qualified, has_unqualified);
        }
        _ => {}
    }
}

fn collect_qualified_prefixes_in_expr(expr: &Expr, prefixes: &mut HashSet<String>) {
    match expr {
        Expr::CompoundIdentifier(parts) => {
            if parts.len() > 1 {
                if let Some(first) = parts.first() {
                    prefixes.insert(first.value.to_ascii_uppercase());
                }
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_qualified_prefixes_in_expr(left, prefixes);
            collect_qualified_prefixes_in_expr(right, prefixes);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => collect_qualified_prefixes_in_expr(inner, prefixes),
        Expr::InList { expr, list, .. } => {
            collect_qualified_prefixes_in_expr(expr, prefixes);
            for item in list {
                collect_qualified_prefixes_in_expr(item, prefixes);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            collect_qualified_prefixes_in_expr(expr, prefixes);
            collect_qualified_prefixes_in_expr(low, prefixes);
            collect_qualified_prefixes_in_expr(high, prefixes);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                collect_qualified_prefixes_in_expr(op, prefixes);
            }
            for case_when in conditions {
                collect_qualified_prefixes_in_expr(&case_when.condition, prefixes);
                collect_qualified_prefixes_in_expr(&case_when.result, prefixes);
            }
            if let Some(otherwise) = else_result {
                collect_qualified_prefixes_in_expr(otherwise, prefixes);
            }
        }
        Expr::Function(func) => {
            if let FunctionArguments::List(arg_list) = &func.args {
                for arg in &arg_list.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(inner))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(inner),
                            ..
                        } => collect_qualified_prefixes_in_expr(inner, prefixes),
                        _ => {}
                    }
                }
            }

            if let Some(filter) = &func.filter {
                collect_qualified_prefixes_in_expr(filter, prefixes);
            }

            for order_expr in &func.within_group {
                collect_qualified_prefixes_in_expr(&order_expr.expr, prefixes);
            }

            if let Some(WindowType::WindowSpec(spec)) = &func.over {
                for expr in &spec.partition_by {
                    collect_qualified_prefixes_in_expr(expr, prefixes);
                }
                for order_expr in &spec.order_by {
                    collect_qualified_prefixes_in_expr(&order_expr.expr, prefixes);
                }
            }
        }
        Expr::InSubquery { expr, .. } => {
            collect_qualified_prefixes_in_expr(expr, prefixes);
        }
        Expr::Exists { .. } | Expr::Subquery(_) => {}
        Expr::AnyOp { left, right, .. } | Expr::AllOp { left, right, .. } => {
            collect_qualified_prefixes_in_expr(left, prefixes);
            collect_qualified_prefixes_in_expr(right, prefixes);
        }
        _ => {}
    }
}

fn count_unqualified_references_in_expr_excluding_aliases(
    expr: &Expr,
    aliases: &HashSet<String>,
) -> usize {
    match expr {
        Expr::Identifier(ident) => {
            usize::from(!aliases.contains(&ident.value.to_ascii_uppercase()))
        }
        Expr::CompoundIdentifier(parts) => usize::from(parts.len() == 1),
        Expr::BinaryOp { left, right, .. } => {
            count_unqualified_references_in_expr_excluding_aliases(left, aliases)
                + count_unqualified_references_in_expr_excluding_aliases(right, aliases)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            count_unqualified_references_in_expr_excluding_aliases(inner, aliases)
        }
        Expr::InList { expr, list, .. } => {
            count_unqualified_references_in_expr_excluding_aliases(expr, aliases)
                + list
                    .iter()
                    .map(|expr| {
                        count_unqualified_references_in_expr_excluding_aliases(expr, aliases)
                    })
                    .sum::<usize>()
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            count_unqualified_references_in_expr_excluding_aliases(expr, aliases)
                + count_unqualified_references_in_expr_excluding_aliases(low, aliases)
                + count_unqualified_references_in_expr_excluding_aliases(high, aliases)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let operand_count = operand.as_ref().map_or(0, |expr| {
                count_unqualified_references_in_expr_excluding_aliases(expr, aliases)
            });
            let when_count = conditions
                .iter()
                .map(|case_when| {
                    count_unqualified_references_in_expr_excluding_aliases(
                        &case_when.condition,
                        aliases,
                    ) + count_unqualified_references_in_expr_excluding_aliases(
                        &case_when.result,
                        aliases,
                    )
                })
                .sum::<usize>();
            let else_count = else_result.as_ref().map_or(0, |expr| {
                count_unqualified_references_in_expr_excluding_aliases(expr, aliases)
            });
            operand_count + when_count + else_count
        }
        Expr::Function(func) => {
            let args_count = match &func.args {
                FunctionArguments::List(arg_list) => arg_list
                    .args
                    .iter()
                    .map(|arg| match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(inner))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(inner),
                            ..
                        } => count_unqualified_references_in_expr_excluding_aliases(inner, aliases),
                        _ => 0,
                    })
                    .sum::<usize>(),
                FunctionArguments::Subquery(query) => {
                    query_sum_select(query, select_unqualified_reference_count_in_multi_table)
                }
                FunctionArguments::None => 0,
            };

            let filter_count = func.filter.as_ref().map_or(0, |expr| {
                count_unqualified_references_in_expr_excluding_aliases(expr, aliases)
            });
            let within_group_count = func
                .within_group
                .iter()
                .map(|order_expr| {
                    count_unqualified_references_in_expr_excluding_aliases(
                        &order_expr.expr,
                        aliases,
                    )
                })
                .sum::<usize>();
            let over_count = match &func.over {
                Some(WindowType::WindowSpec(spec)) => {
                    spec.partition_by
                        .iter()
                        .map(|expr| {
                            count_unqualified_references_in_expr_excluding_aliases(expr, aliases)
                        })
                        .sum::<usize>()
                        + spec
                            .order_by
                            .iter()
                            .map(|order_expr| {
                                count_unqualified_references_in_expr_excluding_aliases(
                                    &order_expr.expr,
                                    aliases,
                                )
                            })
                            .sum::<usize>()
                }
                _ => 0,
            };
            args_count + filter_count + within_group_count + over_count
        }
        Expr::InSubquery { expr, subquery, .. } => {
            let force_nested = expr_is_qualified_reference(expr);
            let nested_count = if force_nested {
                query_sum_select(subquery, select_unqualified_reference_count_forced)
            } else {
                query_sum_select(subquery, select_unqualified_reference_count_in_multi_table)
            };
            count_unqualified_references_in_expr_excluding_aliases(expr, aliases) + nested_count
        }
        Expr::Exists { subquery, .. } | Expr::Subquery(subquery) => {
            query_sum_select(subquery, select_unqualified_reference_count_in_multi_table)
        }
        Expr::AnyOp { left, right, .. } | Expr::AllOp { left, right, .. } => {
            let right_count = match right.as_ref() {
                Expr::Subquery(subquery) => {
                    query_sum_select(subquery, select_unqualified_reference_count_in_multi_table)
                }
                other => count_unqualified_references_in_expr_excluding_aliases(other, aliases),
            };
            count_unqualified_references_in_expr_excluding_aliases(left, aliases) + right_count
        }
        _ => 0,
    }
}

fn expr_is_qualified_reference(expr: &Expr) -> bool {
    matches!(expr, Expr::CompoundIdentifier(parts) if parts.len() > 1)
}

fn count_unqualified_references_in_expr(expr: &Expr) -> usize {
    match expr {
        Expr::Identifier(_) => 1,
        Expr::CompoundIdentifier(parts) => usize::from(parts.len() == 1),
        Expr::BinaryOp { left, right, .. } => {
            count_unqualified_references_in_expr(left) + count_unqualified_references_in_expr(right)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => count_unqualified_references_in_expr(inner),
        Expr::InList { expr, list, .. } => {
            count_unqualified_references_in_expr(expr)
                + list
                    .iter()
                    .map(count_unqualified_references_in_expr)
                    .sum::<usize>()
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            count_unqualified_references_in_expr(expr)
                + count_unqualified_references_in_expr(low)
                + count_unqualified_references_in_expr(high)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let operand_count = operand
                .as_ref()
                .map_or(0, |expr| count_unqualified_references_in_expr(expr));
            let when_count = conditions
                .iter()
                .map(|case_when| {
                    count_unqualified_references_in_expr(&case_when.condition)
                        + count_unqualified_references_in_expr(&case_when.result)
                })
                .sum::<usize>();
            let else_count = else_result
                .as_ref()
                .map_or(0, |expr| count_unqualified_references_in_expr(expr));
            operand_count + when_count + else_count
        }
        Expr::Function(func) => {
            let args_count = match &func.args {
                FunctionArguments::List(arg_list) => arg_list
                    .args
                    .iter()
                    .map(|arg| match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(inner))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(inner),
                            ..
                        } => count_unqualified_references_in_expr(inner),
                        _ => 0,
                    })
                    .sum::<usize>(),
                FunctionArguments::Subquery(query) => {
                    query_sum_select(query, select_unqualified_reference_count_in_multi_table)
                }
                FunctionArguments::None => 0,
            };

            let filter_count = func
                .filter
                .as_ref()
                .map_or(0, |expr| count_unqualified_references_in_expr(expr));

            let within_group_count = func
                .within_group
                .iter()
                .map(|order_expr| count_unqualified_references_in_expr(&order_expr.expr))
                .sum::<usize>();

            let over_count = match &func.over {
                Some(WindowType::WindowSpec(spec)) => {
                    spec.partition_by
                        .iter()
                        .map(count_unqualified_references_in_expr)
                        .sum::<usize>()
                        + spec
                            .order_by
                            .iter()
                            .map(|order_expr| {
                                count_unqualified_references_in_expr(&order_expr.expr)
                            })
                            .sum::<usize>()
                }
                _ => 0,
            };

            args_count + filter_count + within_group_count + over_count
        }
        Expr::InSubquery { expr, subquery, .. } => {
            let force_nested = expr_is_qualified_reference(expr);
            let nested_count = if force_nested {
                query_sum_select(subquery, select_unqualified_reference_count_forced)
            } else {
                query_sum_select(subquery, select_unqualified_reference_count_in_multi_table)
            };
            count_unqualified_references_in_expr(expr) + nested_count
        }
        Expr::Exists { subquery, .. } | Expr::Subquery(subquery) => {
            query_sum_select(subquery, select_unqualified_reference_count_in_multi_table)
        }
        Expr::AnyOp { left, right, .. } | Expr::AllOp { left, right, .. } => {
            let right_count = match right.as_ref() {
                Expr::Subquery(subquery) => {
                    query_sum_select(subquery, select_unqualified_reference_count_in_multi_table)
                }
                other => count_unqualified_references_in_expr(other),
            };
            count_unqualified_references_in_expr(left) + right_count
        }
        _ => 0,
    }
}

fn count_reference_qualification_in_expr_excluding_aliases(
    expr: &Expr,
    aliases: &HashSet<String>,
) -> (usize, usize) {
    match expr {
        Expr::Identifier(ident) => {
            if aliases.contains(&ident.value.to_ascii_uppercase()) {
                (0, 0)
            } else {
                (0, 1)
            }
        }
        Expr::CompoundIdentifier(parts) => {
            if parts.len() > 1 {
                (1, 0)
            } else {
                let token = parts
                    .first()
                    .map(|ident| ident.value.to_ascii_uppercase())
                    .unwrap_or_default();
                if aliases.contains(&token) {
                    (0, 0)
                } else {
                    (0, 1)
                }
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            let (lq, lu) = count_reference_qualification_in_expr_excluding_aliases(left, aliases);
            let (rq, ru) = count_reference_qualification_in_expr_excluding_aliases(right, aliases);
            (lq + rq, lu + ru)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            count_reference_qualification_in_expr_excluding_aliases(inner, aliases)
        }
        Expr::InList { expr, list, .. } => {
            let (mut qualified, mut unqualified) =
                count_reference_qualification_in_expr_excluding_aliases(expr, aliases);
            for item in list {
                let (q, u) = count_reference_qualification_in_expr_excluding_aliases(item, aliases);
                qualified += q;
                unqualified += u;
            }
            (qualified, unqualified)
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            let (eq, eu) = count_reference_qualification_in_expr_excluding_aliases(expr, aliases);
            let (lq, lu) = count_reference_qualification_in_expr_excluding_aliases(low, aliases);
            let (hq, hu) = count_reference_qualification_in_expr_excluding_aliases(high, aliases);
            (eq + lq + hq, eu + lu + hu)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let mut qualified = 0usize;
            let mut unqualified = 0usize;

            if let Some(expr) = operand {
                let (q, u) = count_reference_qualification_in_expr_excluding_aliases(expr, aliases);
                qualified += q;
                unqualified += u;
            }

            for case_when in conditions {
                let (cq, cu) = count_reference_qualification_in_expr_excluding_aliases(
                    &case_when.condition,
                    aliases,
                );
                let (rq, ru) = count_reference_qualification_in_expr_excluding_aliases(
                    &case_when.result,
                    aliases,
                );
                qualified += cq + rq;
                unqualified += cu + ru;
            }

            if let Some(expr) = else_result {
                let (q, u) = count_reference_qualification_in_expr_excluding_aliases(expr, aliases);
                qualified += q;
                unqualified += u;
            }

            (qualified, unqualified)
        }
        Expr::Function(func) => {
            let mut qualified = 0usize;
            let mut unqualified = 0usize;

            if let FunctionArguments::List(arg_list) = &func.args {
                for arg in &arg_list.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(inner))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(inner),
                            ..
                        } => {
                            let (q, u) = count_reference_qualification_in_expr_excluding_aliases(
                                inner, aliases,
                            );
                            qualified += q;
                            unqualified += u;
                        }
                        _ => {}
                    }
                }
            }

            if let Some(filter) = &func.filter {
                let (q, u) =
                    count_reference_qualification_in_expr_excluding_aliases(filter, aliases);
                qualified += q;
                unqualified += u;
            }

            for order_expr in &func.within_group {
                let (q, u) = count_reference_qualification_in_expr_excluding_aliases(
                    &order_expr.expr,
                    aliases,
                );
                qualified += q;
                unqualified += u;
            }

            if let Some(WindowType::WindowSpec(spec)) = &func.over {
                for expr in &spec.partition_by {
                    let (q, u) =
                        count_reference_qualification_in_expr_excluding_aliases(expr, aliases);
                    qualified += q;
                    unqualified += u;
                }
                for order_expr in &spec.order_by {
                    let (q, u) = count_reference_qualification_in_expr_excluding_aliases(
                        &order_expr.expr,
                        aliases,
                    );
                    qualified += q;
                    unqualified += u;
                }
            }

            (qualified, unqualified)
        }
        Expr::InSubquery { expr, .. } => {
            count_reference_qualification_in_expr_excluding_aliases(expr, aliases)
        }
        Expr::AnyOp { left, right, .. } | Expr::AllOp { left, right, .. } => {
            let (lq, lu) = count_reference_qualification_in_expr_excluding_aliases(left, aliases);
            let (rq, ru) = count_reference_qualification_in_expr_excluding_aliases(right, aliases);
            (lq + rq, lu + ru)
        }
        Expr::Exists { .. } | Expr::Subquery(_) => (0, 0),
        _ => (0, 0),
    }
}

fn count_reference_qualification_in_expr(expr: &Expr) -> (usize, usize) {
    let empty_aliases = HashSet::new();
    count_reference_qualification_in_expr_excluding_aliases(expr, &empty_aliases)
}

fn count_unqualified_references_in_order_by(
    order_by: &OrderBy,
    aliases: &HashSet<String>,
) -> usize {
    if let OrderByKind::Expressions(order_exprs) = &order_by.kind {
        order_exprs
            .iter()
            .map(|order_expr| {
                count_unqualified_references_in_expr_excluding_aliases(&order_expr.expr, aliases)
            })
            .sum::<usize>()
    } else {
        0
    }
}

fn select_clause_with_span(sql: &str) -> Option<(String, usize)> {
    Regex::new(r"(?is)\bselect\b(.*?)\bfrom\b")
        .expect("valid parity regex")
        .captures(sql)
        .and_then(|caps| caps.get(1))
        .map(|m| (m.as_str().to_string(), m.start()))
}

fn select_clause(sql: &str) -> Option<String> {
    select_clause_with_span(sql).map(|(clause, _)| clause)
}

fn split_top_level_commas(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_single = false;
    let mut in_double = false;

    for ch in input.chars() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(ch);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(ch);
            }
            '(' if !in_single && !in_double => {
                depth += 1;
                current.push(ch);
            }
            ')' if !in_single && !in_double && depth > 0 => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !in_single && !in_double && depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    parts
}

fn select_items(sql: &str) -> Vec<String> {
    select_clause(sql)
        .map(|clause| split_top_level_commas(&clause))
        .unwrap_or_default()
}

fn item_has_as_alias(item: &str) -> bool {
    has_re(item, r"(?i)\bas\s+[A-Za-z_][A-Za-z0-9_]*\s*$")
}

fn item_has_implicit_alias(item: &str) -> bool {
    let trimmed = item.trim();
    if trimmed.is_empty() || trimmed == "*" || trimmed.ends_with(".*") || item_has_as_alias(trimmed)
    {
        return false;
    }

    let mut depth = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut split_at: Option<usize> = None;

    for (idx, ch) in trimmed.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '(' if !in_single && !in_double => depth += 1,
            ')' if !in_single && !in_double && depth > 0 => depth -= 1,
            c if c.is_whitespace() && !in_single && !in_double && depth == 0 => {
                split_at = Some(idx)
            }
            _ => {}
        }
    }

    let Some(split_idx) = split_at else {
        return false;
    };

    let expr = trimmed[..split_idx].trim_end();
    let alias = trimmed[split_idx..].trim_start();
    if expr.is_empty()
        || alias.is_empty()
        || !has_re(alias, r"(?i)^[A-Za-z_][A-Za-z0-9_]*$")
        || is_keyword(alias)
    {
        return false;
    }

    let expr_ends_with_operator = [
        '+', '-', '*', '/', '%', '^', '|', '&', '=', '<', '>', ',', '(',
    ]
    .iter()
    .any(|ch| expr.ends_with(*ch));
    !expr_ends_with_operator
}

fn item_is_simple_identifier(item: &str) -> bool {
    let trimmed = item.trim();
    has_re(trimmed, r"(?i)^[A-Za-z_][A-Za-z0-9_\.]*$")
        || has_re(
            trimmed,
            r"(?i)^[A-Za-z_][A-Za-z0-9_\.]*\s+(?:as\s+)?[A-Za-z_][A-Za-z0-9_]*$",
        )
}

fn case_style(token: &str) -> &'static str {
    let alpha: String = token.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    if alpha.is_empty() {
        return "mixed";
    }
    if alpha.chars().all(|c| c.is_ascii_uppercase()) {
        "upper"
    } else if alpha.chars().all(|c| c.is_ascii_lowercase()) {
        "lower"
    } else {
        "mixed"
    }
}

fn mixed_case_for_tokens(tokens: &[String]) -> bool {
    let mut styles = HashSet::new();
    for token in tokens {
        styles.insert(case_style(token));
    }
    styles.len() > 1
}

fn first_style_mismatch_span(tokens: &[(String, usize, usize)]) -> Option<(usize, usize)> {
    let first_style = tokens.first().map(|(token, _, _)| case_style(token))?;

    for (token, start, end) in tokens.iter().skip(1) {
        if case_style(token) != first_style {
            return Some((*start, *end));
        }
    }

    let mut seen = HashSet::new();
    for (token, _, _) in tokens {
        seen.insert(case_style(token));
    }
    if seen.len() > 1 {
        tokens.first().map(|(_, start, end)| (*start, *end))
    } else {
        None
    }
}

fn keyword_tokens(sql: &str) -> Vec<String> {
    let re = Regex::new(
        r"(?i)\b(select|from|where|join|left|right|inner|outer|full|cross|group|order|having|with|as|union|intersect|except|insert|update|delete|create|view|table|on|using)\b",
    )
    .expect("valid parity regex");
    re.captures_iter(sql)
        .filter_map(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .collect()
}

#[allow(dead_code)]
fn function_tokens(sql: &str) -> Vec<String> {
    function_tokens_with_spans(sql)
        .into_iter()
        .map(|(name, _, _)| name)
        .collect()
}

fn function_tokens_with_spans(sql: &str) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();

    for (name, start, end) in
        capture_group_with_spans(sql, r"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\s*\(", 1)
    {
        if is_keyword(&name) && !name.eq_ignore_ascii_case("date") {
            continue;
        }

        let prev_word = sql[..start]
            .split_whitespace()
            .last()
            .unwrap_or("")
            .to_ascii_uppercase();
        if matches!(
            prev_word.as_str(),
            "INTO" | "FROM" | "JOIN" | "UPDATE" | "TABLE"
        ) {
            continue;
        }

        // Skip schema-qualified object references (e.g. metrics.table_name (...)).
        if start > 0 && sql.as_bytes()[start - 1] == b'.' {
            continue;
        }

        out.push((name, start, end));
    }

    out
}

fn literal_tokens(sql: &str) -> Vec<String> {
    capture_group(sql, r"(?i)\b(null|true|false)\b", 1)
}

fn type_tokens(sql: &str) -> Vec<String> {
    capture_group(
        sql,
        r"(?i)\b(int|integer|bigint|smallint|tinyint|varchar|char|text|boolean|bool|date|timestamp|numeric|decimal|float|double)\b",
        1,
    )
}

#[allow(dead_code)]
fn identifier_tokens(sql: &str) -> Vec<String> {
    identifier_tokens_with_spans(sql)
        .into_iter()
        .map(|(token, _, _)| token)
        .collect()
}

fn identifier_tokens_with_spans(sql: &str) -> Vec<(String, usize, usize)> {
    capture_group_with_spans(sql, r#"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\b"#, 1)
        .into_iter()
        .filter(|(token, _, _)| !is_keyword(token))
        .collect()
}

fn plain_join_count(sql: &str) -> usize {
    let re = Regex::new(r"(?i)\bjoin\b").expect("valid parity regex");
    let mut count = 0usize;

    for mat in re.find_iter(sql) {
        let prefix = &sql[..mat.start()];
        let prev_word = prefix
            .split_whitespace()
            .last()
            .unwrap_or("")
            .to_ascii_uppercase();
        let explicit = matches!(
            prev_word.as_str(),
            "LEFT" | "RIGHT" | "INNER" | "FULL" | "CROSS" | "OUTER" | "SEMI" | "ANTI" | "STRAIGHT"
        );
        if !explicit {
            count += 1;
        }
    }

    count
}

fn contains_plain_join(sql: &str) -> bool {
    plain_join_count(sql) > 0
}

fn issue_if_regex(stmt: &Statement, ctx: &LintContext, pattern: &str) -> bool {
    let _ = stmt;
    has_re(stmt_sql(ctx), pattern)
}

fn rule_al_02(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let items = select_items(stmt_sql(ctx));
    if items.is_empty() {
        return false;
    }
    let mut has_explicit_alias = false;
    let mut has_implicit_alias = false;
    for item in items {
        if item_has_as_alias(&item) {
            has_explicit_alias = true;
        } else if item_has_implicit_alias(&item) {
            has_implicit_alias = true;
        }
    }
    has_explicit_alias && has_implicit_alias
}

#[allow(dead_code)]
fn rule_al_04(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    duplicate_case_insensitive(&table_aliases(stmt_sql(ctx)))
}

fn rule_al_06(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    table_aliases(stmt_sql(ctx))
        .iter()
        .any(|alias| alias.len() > 30)
}

fn rule_al_07(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    table_refs(sql).len() == 1 && !table_aliases(sql).is_empty()
}

#[allow(dead_code)]
fn rule_al_08(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    duplicate_case_insensitive(&column_aliases(stmt_sql(ctx)))
}

fn self_alias_count(sql: &str) -> usize {
    let re = Regex::new(
        r"(?i)\b(?:[A-Za-z_][A-Za-z0-9_]*\.)?([A-Za-z_][A-Za-z0-9_]*)\s+as\s+([A-Za-z_][A-Za-z0-9_]*)\b",
    )
    .expect("valid parity regex");

    re.captures_iter(sql)
        .filter(|caps| {
            let left = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let right = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
            left.eq_ignore_ascii_case(right)
        })
        .count()
}

fn rule_am_03(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?i)\border\s+by\s+\d+\b")
}

fn rule_am_06(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = ctx;
    statement_any_select(stmt, select_has_mixed_join_condition_qualification)
}

fn rule_am_07(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    has_re(sql, r"(?i)\b(union|intersect|except)\b") && has_re(sql, r"(?i)\bselect\s+\*")
}

fn rule_am_08(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?i)\bjoin\b[^;]*\bon\s+(?:true|1\s*=\s*1)\b")
}

fn rule_cp_01(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
    mixed_case_for_tokens(&keyword_tokens(&sql))
}

#[allow(dead_code)]
fn rule_cp_02(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
    let function_names: HashSet<String> = function_tokens(&sql)
        .into_iter()
        .map(|name| name.to_ascii_uppercase())
        .collect();
    let identifiers: Vec<String> = identifier_tokens(&sql)
        .into_iter()
        .filter(|ident| !function_names.contains(&ident.to_ascii_uppercase()))
        .collect();
    mixed_case_for_tokens(&identifiers)
}

#[allow(dead_code)]
fn rule_cp_03(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
    mixed_case_for_tokens(&function_tokens(&sql))
}

fn rule_cp_04(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
    mixed_case_for_tokens(&literal_tokens(&sql))
}

fn rule_cp_05(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
    mixed_case_for_tokens(&type_tokens(&sql))
}

fn rule_cv_01(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
    has_re(&sql, r"<>") && has_re(&sql, r"!=")
}

fn rule_cv_03(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?is)\bselect\b[^;]*,\s*\bfrom\b")
}

fn rule_cv_06(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    // Be conservative: only flag when semicolons exist in the file but this
    // statement SQL snippet itself doesn't end with one.
    ctx.sql.contains(';') && !stmt_sql(ctx).trim_end().ends_with(';')
}

fn rule_cv_07(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx).trim();
    sql.starts_with('(') && sql.ends_with(')')
}

fn rule_cv_09(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?i)\b(todo|fixme|foo|bar)\b")
}

fn rule_cv_10(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r#""[^"]+""#)
}

fn rule_cv_11(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
    has_re(&sql, r"::") && has_re(&sql, r"(?i)\bcast\s*\(")
}

fn rule_cv_12(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    contains_plain_join(sql)
        && !has_re(sql, r"(?i)\bjoin\b[^;]*\bon\b[^;]*=")
        && !has_re(sql, r"(?i)\bjoin\b[^;]*\busing\b")
}

fn rule_jj_01(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    has_re(sql, r"\{\{[^ \n]") || has_re(sql, r"[^ \n]\}\}") || has_re(sql, r"\{%[^ \n]")
}

#[allow(dead_code)]
fn rule_lt_01(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?i)\w(?:=|<>|!=|<|>|<=|>=|\+|-|\*|/)\w")
}

fn rule_lt_02(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    if !sql.contains('\n') {
        return false;
    }
    sql.lines().skip(1).any(|line| {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return false;
        }
        let indent = line.len() - trimmed.len();
        indent % 2 != 0
    })
}

fn rule_lt_03(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?m)(\+|-|\*|/|=|<>|!=|<|>)\s*$")
}

fn rule_lt_04(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    has_re(sql, r"\s+,") || has_re(sql, r",[^\s\n]")
}

fn long_line_overflow_spans(sql: &str, max_len: usize) -> Vec<(usize, usize)> {
    let bytes = sql.as_bytes();
    let mut spans = Vec::new();
    let mut line_start = 0usize;

    for idx in 0..=bytes.len() {
        if idx < bytes.len() && bytes[idx] != b'\n' {
            continue;
        }

        let mut line_end = idx;
        if line_end > line_start && bytes[line_end - 1] == b'\r' {
            line_end -= 1;
        }

        let line = &sql[line_start..line_end];
        if line.chars().count() > max_len {
            let mut overflow_start = line_end;
            for (char_idx, (byte_off, _)) in line.char_indices().enumerate() {
                if char_idx == max_len {
                    overflow_start = line_start + byte_off;
                    break;
                }
            }

            if overflow_start < line_end {
                let overflow_end = sql[overflow_start..line_end]
                    .chars()
                    .next()
                    .map(|ch| overflow_start + ch.len_utf8())
                    .unwrap_or(overflow_start);
                spans.push((overflow_start, overflow_end));
            }
        }

        line_start = idx + 1;
    }

    spans
}

#[allow(dead_code)]
fn rule_lt_06(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
    let re = Regex::new(r"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\s+\(").expect("valid parity regex");
    let has_violation = re.captures_iter(&sql).any(|caps| {
        let token = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        !is_keyword(token)
    });
    has_violation
}

fn rule_lt_07(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(
        stmt,
        ctx,
        r"(?is)\bwith\b\s+[A-Za-z_][A-Za-z0-9_]*\s+as\s+select\b",
    )
}

fn select_line_top_level_comma_count(segment: &str) -> usize {
    let mut count = 0usize;
    let mut depth = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let bytes = segment.as_bytes();
    let mut idx = 0usize;

    while idx < bytes.len() {
        let b = bytes[idx];

        if in_single {
            if b == b'\'' {
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'\'' {
                    idx += 2;
                } else {
                    in_single = false;
                    idx += 1;
                }
            } else {
                idx += 1;
            }
            continue;
        }

        if in_double {
            if b == b'"' {
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'"' {
                    idx += 2;
                } else {
                    in_double = false;
                    idx += 1;
                }
            } else {
                idx += 1;
            }
            continue;
        }

        match b {
            b'\'' => in_single = true,
            b'"' => in_double = true,
            b'(' => depth += 1,
            b')' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            b',' if depth == 0 => count += 1,
            _ => {}
        }

        idx += 1;
    }

    count
}

fn lt09_violation_spans(sql: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let masked = mask_comments_and_single_quoted_strings(sql);

    for (_token, start, end) in capture_group_with_spans(&masked, r"(?i)\bselect\b", 0) {
        let line_end = sql[end..].find('\n').map_or(sql.len(), |off| end + off);
        let select_tail = &sql[end..line_end];

        if select_line_top_level_comma_count(select_tail) > 0 {
            spans.push((start, end));
        }
    }

    spans
}

fn rule_lt_10(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?is)\bselect\s*\n+\s*(distinct|all)\b")
}

fn rule_lt_11(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    if !has_re(sql, r"(?i)\b(union|intersect|except)\b") || !sql.contains('\n') {
        return false;
    }

    sql.lines().any(|line| {
        let trimmed = line.trim().to_ascii_lowercase();
        match trimmed.as_str() {
            "union" | "union all" | "intersect" | "except" => false,
            _ => has_re(&trimmed, r"\b(union|intersect|except)\b"),
        }
    })
}

fn rule_lt_12(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    ctx.statement_range.end == ctx.sql.len() && ctx.sql.contains('\n') && !ctx.sql.ends_with('\n')
}

fn rule_lt_13(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    ctx.statement_index == 0 && has_re(ctx.sql, r"^\s*\n")
}

#[allow(dead_code)]
fn rule_lt_14(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    sql.contains('\n')
        && has_re(
            sql,
            r"(?im)^\s*select\b[^\n]*\b(from|where|group by|order by)\b",
        )
}

fn rule_lt_15(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"\n\s*\n\s*\n+")
}

fn rule_rf_01(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
    let mut known: HashSet<String> = HashSet::new();
    for name in table_refs(&sql) {
        known.insert(name.to_ascii_uppercase());
    }
    for alias in table_aliases(&sql) {
        known.insert(alias.to_ascii_uppercase());
    }
    for target in update_target_refs(&sql) {
        known.insert(target.to_ascii_uppercase());
    }
    for alias in update_target_aliases(&sql) {
        known.insert(alias.to_ascii_uppercase());
    }
    /* Common pseudo table aliases that can appear outside FROM/JOIN clauses. */
    for pseudo in ["EXCLUDED", "INSERTED", "DELETED", "NEW", "OLD"] {
        known.insert(pseudo.to_string());
    }
    qualifier_prefixes(&sql)
        .into_iter()
        .any(|prefix| !known.contains(&prefix.to_ascii_uppercase()))
}

fn rule_rf_04(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    capture_group(
        stmt_sql(ctx),
        r"(?i)\b(?:from|join)\s+[A-Za-z_][A-Za-z0-9_\.]*\s+as\s+([A-Za-z_][A-Za-z0-9_]*)",
        1,
    )
    .into_iter()
    .any(|alias| is_keyword(&alias))
}

fn rule_rf_05(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    capture_group(stmt_sql(ctx), r#""([^"]+)""#, 1)
        .into_iter()
        .any(|ident| !has_re(&ident, r"^[A-Za-z0-9_]+$"))
}

fn rule_rf_06(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    capture_group(stmt_sql(ctx), r#""([^"]+)""#, 1)
        .into_iter()
        .any(|ident| has_re(&ident, r"^[A-Za-z_][A-Za-z0-9_]*$"))
}

fn rule_st_02(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx).to_ascii_lowercase();
    if let Some(caps) = Regex::new(r"case\s+when\s+([a-z_][a-z0-9_\.]*)\s*=")
        .expect("valid parity regex")
        .captures(&sql)
    {
        if let Some(lhs) = caps.get(1) {
            let pattern = format!(r"when\s+{}\s*=", regex::escape(lhs.as_str()));
            let repeated_when_count = Regex::new(&pattern)
                .expect("valid parity regex")
                .find_iter(&sql)
                .count();
            return repeated_when_count >= 2;
        }
    }
    false
}

#[allow(dead_code)]
fn rule_st_05(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?is)\b(from|where|in)\s*\(\s*select\b")
}

#[allow(dead_code)]
fn rule_st_06(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let items = select_items(stmt_sql(ctx));
    let mut seen_expression = false;
    for item in items {
        if item_is_simple_identifier(&item) {
            if seen_expression {
                return true;
            }
        } else {
            seen_expression = true;
        }
    }
    false
}

fn rule_st_08(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?is)\bselect\s+distinct\s*\(")
}

fn rule_st_10(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(
        stmt,
        ctx,
        r"(?i)\b(1\s*=\s*1|1\s*=\s*0|true\s+(and|or)|false\s+(and|or))\b",
    )
}

fn rule_st_11(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    for alias in join_aliases(sql) {
        let pat = format!(r"(?i)\b{}\.", regex::escape(&alias));
        let count = Regex::new(&pat)
            .expect("valid parity regex")
            .find_iter(sql)
            .count();
        if count == 0 {
            return true;
        }
    }
    false
}

fn rule_st_12(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    ctx.statement_index == 0 && has_re(ctx.sql, r";\s*;")
}

fn rule_tq_01(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(
        stmt,
        ctx,
        r"(?i)\bcreate\s+(?:proc|procedure)\s+sp_[A-Za-z0-9_]*",
    )
}

fn rule_tq_02(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    has_re(sql, r"(?i)\bcreate\s+(?:proc|procedure)\b")
        && !(has_re(sql, r"(?i)\bbegin\b") && has_re(sql, r"(?i)\bend\b"))
}

fn rule_tq_03(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?im)^\s*GO\s*$\s*(?:\r?\n\s*GO\s*$)+")
}

pub struct AliasingTableStyle;

impl LintRule for AliasingTableStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_003
    }

    fn name(&self) -> &'static str {
        "Table alias style"
    }

    fn description(&self) -> &'static str {
        "Use explicit AS when aliasing tables."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let _ = stmt;
        let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
        let spans = implicit_table_alias_spans(&sql);
        if spans.is_empty() {
            return Vec::new();
        }

        spans
            .into_iter()
            .map(|(start, end)| {
                Issue::warning(
                    issue_codes::LINT_AL_003,
                    "Use explicit AS when aliasing tables.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
            })
            .collect()
    }
}
define_predicate_rule!(
    AliasingColumnStyle,
    issue_codes::LINT_AL_004,
    "Column alias style",
    "Avoid mixing explicit and implicit aliasing for expressions.",
    info,
    rule_al_02,
    "Avoid mixing explicit and implicit expression aliases."
);
pub struct AliasingUniqueTable;

impl LintRule for AliasingUniqueTable {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_005
    }

    fn name(&self) -> &'static str {
        "Unique table alias"
    }

    fn description(&self) -> &'static str {
        "Table aliases should be unique within a statement."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let Some(duplicate_alias) = first_duplicate_table_alias_in_statement(stmt) else {
            return Vec::new();
        };

        let mut issue = Issue::warning(issue_codes::LINT_AL_005, "Table aliases should be unique.")
            .with_statement(ctx.statement_index);

        if let Some((start, end)) = second_occurrence_case_insensitive_span(
            &table_aliases_with_spans(stmt_sql(ctx)),
            &duplicate_alias,
        ) {
            issue = issue.with_span(ctx.span_from_statement_offset(start, end));
        }

        vec![issue]
    }
}
define_predicate_rule!(
    AliasingLength,
    issue_codes::LINT_AL_006,
    "Alias length",
    "Alias names should be readable and not excessively long.",
    info,
    rule_al_06,
    "Alias length should not exceed 30 characters."
);
define_predicate_rule!(
    AliasingForbidSingleTable,
    issue_codes::LINT_AL_007,
    "Forbid unnecessary alias",
    "Single-table queries should avoid unnecessary aliases.",
    info,
    rule_al_07,
    "Avoid unnecessary aliases in single-table queries."
);
pub struct AliasingUniqueColumn;

impl LintRule for AliasingUniqueColumn {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_008
    }

    fn name(&self) -> &'static str {
        "Unique column alias"
    }

    fn description(&self) -> &'static str {
        "Column aliases should be unique in projection lists."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let Some(duplicate_alias) = first_duplicate_column_alias_in_statement(stmt) else {
            return Vec::new();
        };

        let mut issue = Issue::warning(
            issue_codes::LINT_AL_008,
            "Column aliases should be unique within SELECT projection.",
        )
        .with_statement(ctx.statement_index);

        if let Some((start, end)) = second_occurrence_case_insensitive_span(
            &column_aliases_with_spans(stmt_sql(ctx)),
            &duplicate_alias,
        ) {
            issue = issue.with_span(ctx.span_from_statement_offset(start, end));
        }

        vec![issue]
    }
}
pub struct AliasingSelfAliasColumn;

impl LintRule for AliasingSelfAliasColumn {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_009
    }

    fn name(&self) -> &'static str {
        "Self alias column"
    }

    fn description(&self) -> &'static str {
        "Avoid aliasing a column/expression to the same name."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let _ = stmt;
        let count = self_alias_count(stmt_sql(ctx));
        if count == 0 {
            return Vec::new();
        }

        (0..count)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_AL_009,
                    "Avoid self-aliasing columns or expressions.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

define_predicate_rule!(
    AmbiguousOrderByOrdinal,
    issue_codes::LINT_AM_005,
    "Ambiguous ORDER BY",
    "Avoid positional ORDER BY references.",
    warning,
    rule_am_03,
    "Avoid positional ORDER BY references (e.g., ORDER BY 1)."
);
pub struct AmbiguousJoinStyle;

impl LintRule for AmbiguousJoinStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_006
    }

    fn name(&self) -> &'static str {
        "Ambiguous join style"
    }

    fn description(&self) -> &'static str {
        "Join clauses should be fully qualified."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let _ = stmt;
        let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
        let count = plain_join_count(&sql);
        if count == 0 {
            return Vec::new();
        }

        (0..count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_AM_006,
                    "Join clauses should be fully qualified.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}
define_predicate_rule!(
    AmbiguousColumnRefs,
    issue_codes::LINT_AM_007,
    "Ambiguous column references",
    "Avoid mixing qualified and unqualified references.",
    info,
    rule_am_06,
    "Avoid mixing qualified and unqualified column references."
);
define_predicate_rule!(
    AmbiguousSetColumns,
    issue_codes::LINT_AM_008,
    "Ambiguous set columns",
    "Avoid wildcard projections in set operations.",
    warning,
    rule_am_07,
    "Avoid wildcard projections in UNION/INTERSECT/EXCEPT branches."
);
define_predicate_rule!(
    AmbiguousJoinCondition,
    issue_codes::LINT_AM_009,
    "Ambiguous join condition",
    "Join conditions should be explicit and meaningful.",
    warning,
    rule_am_08,
    "Join condition appears ambiguous (e.g., ON TRUE / ON 1=1)."
);

define_predicate_rule!(
    CapitalisationKeywords,
    issue_codes::LINT_CP_001,
    "Keyword capitalisation",
    "SQL keywords should use a consistent case style.",
    info,
    rule_cp_01,
    "SQL keywords use inconsistent capitalisation."
);
pub struct CapitalisationIdentifiers;

impl LintRule for CapitalisationIdentifiers {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_002
    }

    fn name(&self) -> &'static str {
        "Identifier capitalisation"
    }

    fn description(&self) -> &'static str {
        "Identifiers should use a consistent case style."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
        let function_names: HashSet<String> = function_tokens_with_spans(&sql)
            .into_iter()
            .map(|(name, _, _)| name.to_ascii_uppercase())
            .collect();

        let identifiers: Vec<(String, usize, usize)> =
            capture_group_with_spans(&sql, r#"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\b"#, 1)
                .into_iter()
                .filter(|(ident, _, _)| {
                    let upper = ident.to_ascii_uppercase();
                    (!is_keyword(ident) || upper == "EXCLUDED") && !function_names.contains(&upper)
                })
                .collect();

        let excluded_issues: Vec<Issue> = identifiers
            .iter()
            .filter(|(ident, _, _)| {
                ident.eq_ignore_ascii_case("EXCLUDED") && ident != &ident.to_ascii_lowercase()
            })
            .map(|(_, start, end)| {
                Issue::info(
                    issue_codes::LINT_CP_002,
                    "Identifiers use inconsistent capitalisation.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(*start, *end))
            })
            .collect();

        if !excluded_issues.is_empty() {
            return excluded_issues;
        }

        let names: Vec<String> = identifiers
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect();
        if !mixed_case_for_tokens(&names) {
            return Vec::new();
        }

        let (start, end) = first_style_mismatch_span(&identifiers)
            .or_else(|| identifiers.first().map(|(_, s, e)| (*s, *e)))
            .unwrap_or((0, 0));

        vec![Issue::info(
            issue_codes::LINT_CP_002,
            "Identifiers use inconsistent capitalisation.",
        )
        .with_statement(ctx.statement_index)
        .with_span(ctx.span_from_statement_offset(start, end))]
    }
}

pub struct CapitalisationFunctions;

impl LintRule for CapitalisationFunctions {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_003
    }

    fn name(&self) -> &'static str {
        "Function capitalisation"
    }

    fn description(&self) -> &'static str {
        "Functions should use a consistent case style."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
        let functions = function_tokens_with_spans(&sql);
        if functions.is_empty() {
            return Vec::new();
        }

        let preferred_style = functions
            .iter()
            .map(|(name, _, _)| case_style(name))
            .find(|style| *style == "lower" || *style == "upper")
            .unwrap_or("lower");

        let issues: Vec<Issue> = functions
            .into_iter()
            .filter(|(name, _, _)| {
                let style = case_style(name);
                (style == "lower" || style == "upper" || style == "mixed")
                    && style != preferred_style
            })
            .map(|(_, start, end)| {
                Issue::info(
                    issue_codes::LINT_CP_003,
                    "Function names use inconsistent capitalisation.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
            })
            .collect();

        issues
    }
}
define_predicate_rule!(
    CapitalisationLiterals,
    issue_codes::LINT_CP_004,
    "Literal capitalisation",
    "NULL/TRUE/FALSE should use a consistent case style.",
    info,
    rule_cp_04,
    "Literal keywords (NULL/TRUE/FALSE) use inconsistent capitalisation."
);
define_predicate_rule!(
    CapitalisationTypes,
    issue_codes::LINT_CP_005,
    "Type capitalisation",
    "Type names should use a consistent case style.",
    info,
    rule_cp_05,
    "Type names use inconsistent capitalisation."
);

define_predicate_rule!(
    ConventionNotEqual,
    issue_codes::LINT_CV_005,
    "Not-equal style",
    "Use a consistent not-equal operator style.",
    info,
    rule_cv_01,
    "Use consistent not-equal style (prefer !=)."
);
define_predicate_rule!(
    ConventionSelectTrailingComma,
    issue_codes::LINT_CV_006,
    "Select trailing comma",
    "Avoid trailing comma before FROM.",
    warning,
    rule_cv_03,
    "Avoid trailing comma before FROM in SELECT clause."
);
define_predicate_rule!(
    ConventionTerminator,
    issue_codes::LINT_CV_007,
    "Statement terminator",
    "Statements should use consistent semicolon termination.",
    info,
    rule_cv_06,
    "Statement terminator style is inconsistent."
);
define_predicate_rule!(
    ConventionStatementBrackets,
    issue_codes::LINT_CV_008,
    "Statement brackets",
    "Avoid unnecessary wrapping brackets around full statements.",
    info,
    rule_cv_07,
    "Avoid wrapping the full statement in unnecessary brackets."
);
define_predicate_rule!(
    ConventionBlockedWords,
    issue_codes::LINT_CV_009,
    "Blocked words",
    "Avoid blocked placeholder words.",
    warning,
    rule_cv_09,
    "Blocked placeholder words detected (e.g., TODO/FIXME/foo/bar)."
);
define_predicate_rule!(
    ConventionQuotedLiterals,
    issue_codes::LINT_CV_010,
    "Quoted literals style",
    "Quoted literal style is inconsistent with SQL convention.",
    info,
    rule_cv_10,
    "Quoted literal style appears inconsistent."
);
define_predicate_rule!(
    ConventionCastingStyle,
    issue_codes::LINT_CV_011,
    "Casting style",
    "Use consistent casting style.",
    info,
    rule_cv_11,
    "Use consistent casting style (avoid mixing :: and CAST)."
);
define_predicate_rule!(
    ConventionJoinCondition,
    issue_codes::LINT_CV_012,
    "Join condition convention",
    "JOIN clauses should use explicit, meaningful join predicates.",
    warning,
    rule_cv_12,
    "JOIN clause appears to lack a meaningful join condition."
);

define_predicate_rule!(
    JinjaPadding,
    issue_codes::LINT_JJ_001,
    "Jinja padding",
    "Jinja tags should use consistent padding.",
    info,
    rule_jj_01,
    "Jinja tag spacing appears inconsistent."
);

pub struct LayoutSpacing;

impl LintRule for LayoutSpacing {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_001
    }

    fn name(&self) -> &'static str {
        "Layout spacing"
    }

    fn description(&self) -> &'static str {
        "Operator spacing should be consistent."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = stmt_sql(ctx);
        let mut issues = Vec::new();

        for (matched, start, end) in
            capture_group_with_spans(sql, r"(?i)\b[A-Za-z_][A-Za-z0-9_\.]*->>'[^']*'", 0)
        {
            if let Some(op_idx) = matched.find("->>") {
                let left_start = start + op_idx;
                let right_start = left_start + 3;
                let left_end = (left_start + 1).min(end);
                let right_end = (right_start + 1).min(end);

                if left_start < left_end {
                    issues.push(
                        Issue::info(
                            issue_codes::LINT_LT_001,
                            "Operator spacing appears inconsistent.",
                        )
                        .with_statement(ctx.statement_index)
                        .with_span(ctx.span_from_statement_offset(left_start, left_end)),
                    );
                }
                if right_start < right_end {
                    issues.push(
                        Issue::info(
                            issue_codes::LINT_LT_001,
                            "Operator spacing appears inconsistent.",
                        )
                        .with_statement(ctx.statement_index)
                        .with_span(ctx.span_from_statement_offset(right_start, right_end)),
                    );
                }
            }
        }

        for (_matched, _start, end) in capture_group_with_spans(sql, r"(?i)\btext\[", 0) {
            let bracket_start = end.saturating_sub(1);
            if bracket_start < end {
                issues.push(
                    Issue::info(
                        issue_codes::LINT_LT_001,
                        "Operator spacing appears inconsistent.",
                    )
                    .with_statement(ctx.statement_index)
                    .with_span(ctx.span_from_statement_offset(bracket_start, end)),
                );
            }
        }

        for (_matched, start, _end) in capture_group_with_spans(sql, r",\d", 0) {
            let number_start = start + 1;
            issues.push(
                Issue::info(
                    issue_codes::LINT_LT_001,
                    "Operator spacing appears inconsistent.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(number_start, number_start + 1)),
            );
        }

        for (matched, start, end) in capture_group_with_spans(sql, r"(?im)^\s*exists\s+\(", 0) {
            let line_start = sql[..start].rfind('\n').map_or(0, |idx| idx + 1);
            let prev_token = sql[..line_start]
                .lines()
                .rev()
                .map(str::trim)
                .find(|line| !line.is_empty() && !line.starts_with("--"));
            if matches!(prev_token, Some("OR") | Some("AND") | Some("NOT")) {
                continue;
            }

            if let Some(paren_off) = matched.rfind('(') {
                let paren_start = start + paren_off;
                if paren_start < end {
                    issues.push(
                        Issue::info(
                            issue_codes::LINT_LT_001,
                            "Operator spacing appears inconsistent.",
                        )
                        .with_statement(ctx.statement_index)
                        .with_span(ctx.span_from_statement_offset(paren_start, paren_start + 1)),
                    );
                }
            }
        }

        issues
    }
}
define_predicate_rule!(
    LayoutIndent,
    issue_codes::LINT_LT_002,
    "Layout indent",
    "Indentation should use consistent step sizes.",
    info,
    rule_lt_02,
    "Indentation appears inconsistent."
);
define_predicate_rule!(
    LayoutOperators,
    issue_codes::LINT_LT_003,
    "Layout operators",
    "Operator line placement should be consistent.",
    info,
    rule_lt_03,
    "Operator line placement appears inconsistent."
);
define_predicate_rule!(
    LayoutCommas,
    issue_codes::LINT_LT_004,
    "Layout commas",
    "Comma spacing should be consistent.",
    info,
    rule_lt_04,
    "Comma spacing appears inconsistent."
);
pub struct LayoutLongLines;

impl LintRule for LayoutLongLines {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_005
    }

    fn name(&self) -> &'static str {
        "Layout long lines"
    }

    fn description(&self) -> &'static str {
        "Avoid excessively long SQL lines."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if ctx.statement_index != 0 {
            return Vec::new();
        }

        long_line_overflow_spans(ctx.sql, 80)
            .into_iter()
            .map(|(start, end)| {
                Issue::info(
                    issue_codes::LINT_LT_005,
                    "SQL contains excessively long lines.",
                )
                .with_statement(ctx.statement_index)
                .with_span(Span::new(start, end))
            })
            .collect()
    }
}
pub struct LayoutFunctions;

impl LintRule for LayoutFunctions {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_006
    }

    fn name(&self) -> &'static str {
        "Layout functions"
    }

    fn description(&self) -> &'static str {
        "Function call spacing should be consistent."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
        let re = Regex::new(r"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\s+\(").expect("valid parity regex");

        for caps in re.captures_iter(&sql) {
            let token = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            if is_keyword(token) {
                continue;
            }
            if let Some(name) = caps.get(1) {
                let prev_word = sql[..name.start()]
                    .split_whitespace()
                    .last()
                    .unwrap_or("")
                    .to_ascii_uppercase();

                // Skip table/target contexts like INSERT INTO table_name (...).
                if matches!(
                    prev_word.as_str(),
                    "INTO" | "FROM" | "JOIN" | "UPDATE" | "TABLE"
                ) {
                    continue;
                }

                // Skip schema-qualified object references (e.g. metrics.table_name (...)).
                if name.start() > 0 && sql.as_bytes()[name.start() - 1] == b'.' {
                    continue;
                }

                return vec![Issue::info(
                    issue_codes::LINT_LT_006,
                    "Function call spacing appears inconsistent.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(name.start(), name.end()))];
            }
        }

        Vec::new()
    }
}
define_predicate_rule!(
    LayoutCteBracket,
    issue_codes::LINT_LT_007,
    "Layout CTE bracket",
    "CTE bodies should be wrapped in brackets.",
    warning,
    rule_lt_07,
    "CTE AS clause appears to be missing surrounding brackets."
);
pub struct LayoutCteNewline;

impl LintRule for LayoutCteNewline {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_008
    }

    fn name(&self) -> &'static str {
        "Layout CTE newline"
    }

    fn description(&self) -> &'static str {
        "Blank line should separate CTE blocks from following code."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        lt08_violation_spans(stmt_sql(ctx))
            .into_iter()
            .map(|(start, end)| {
                Issue::info(
                    issue_codes::LINT_LT_008,
                    "Blank line expected but not found after CTE closing bracket.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
            })
            .collect()
    }
}
pub struct LayoutSelectTargets;

impl LintRule for LayoutSelectTargets {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_009
    }

    fn name(&self) -> &'static str {
        "Layout select targets"
    }

    fn description(&self) -> &'static str {
        "Select targets should be on a new line unless there is only one target."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        lt09_violation_spans(stmt_sql(ctx))
            .into_iter()
            .map(|(start, end)| {
                Issue::info(
                    issue_codes::LINT_LT_009,
                    "Select targets should be on a new line unless there is only one target.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
            })
            .collect()
    }
}
define_predicate_rule!(
    LayoutSelectModifiers,
    issue_codes::LINT_LT_010,
    "Layout select modifiers",
    "SELECT modifiers should be placed consistently.",
    info,
    rule_lt_10,
    "SELECT modifiers (DISTINCT/ALL) should be consistently formatted."
);
define_predicate_rule!(
    LayoutSetOperators,
    issue_codes::LINT_LT_011,
    "Layout set operators",
    "Set operators should be consistently line-broken.",
    info,
    rule_lt_11,
    "Set operators should be on their own line in multiline queries."
);
define_predicate_rule!(
    LayoutEndOfFile,
    issue_codes::LINT_LT_012,
    "Layout end of file",
    "File should end with newline.",
    info,
    rule_lt_12,
    "SQL document should end with a trailing newline."
);
define_predicate_rule!(
    LayoutStartOfFile,
    issue_codes::LINT_LT_013,
    "Layout start of file",
    "Avoid leading blank lines at file start.",
    info,
    rule_lt_13,
    "Avoid leading blank lines at the start of SQL file."
);
pub struct LayoutKeywordNewline;

impl LintRule for LayoutKeywordNewline {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_014
    }

    fn name(&self) -> &'static str {
        "Layout keyword newline"
    }

    fn description(&self) -> &'static str {
        "Major clauses should be consistently line-broken."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
        let select_line_re = Regex::new(r"(?im)^\s*select\b([^\n]*)").expect("valid parity regex");
        let major_clause_re =
            Regex::new(r"(?i)\b(from|where|group\s+by|order\s+by)\b").expect("valid parity regex");

        let Some(select_caps) = select_line_re.captures(&sql) else {
            return Vec::new();
        };
        let Some(select_tail) = select_caps.get(1) else {
            return Vec::new();
        };

        let mut clause_iter = major_clause_re.find_iter(select_tail.as_str());
        let Some(first_clause) = clause_iter.next() else {
            return Vec::new();
        };

        let has_second_clause_on_select_line = clause_iter.next().is_some();
        let has_major_clause_on_later_line = major_clause_re.is_match(&sql[select_tail.end()..]);
        if !has_second_clause_on_select_line && !has_major_clause_on_later_line {
            return Vec::new();
        }

        let keyword_start = select_tail.start() + first_clause.start();
        let keyword_end = select_tail.start() + first_clause.end();

        vec![Issue::info(
            issue_codes::LINT_LT_014,
            "Major clauses should be consistently line-broken.",
        )
        .with_statement(ctx.statement_index)
        .with_span(ctx.span_from_statement_offset(keyword_start, keyword_end))]
    }
}
define_predicate_rule!(
    LayoutNewlines,
    issue_codes::LINT_LT_015,
    "Layout newlines",
    "Avoid excessive blank lines.",
    info,
    rule_lt_15,
    "SQL contains excessive blank lines."
);

define_predicate_rule!(
    ReferencesFrom,
    issue_codes::LINT_RF_001,
    "References from",
    "Qualified references should resolve to known FROM/JOIN sources.",
    warning,
    rule_rf_01,
    "Reference prefix appears unresolved from FROM/JOIN sources."
);
pub struct ReferencesQualification;

impl LintRule for ReferencesQualification {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_002
    }

    fn name(&self) -> &'static str {
        "References qualification"
    }

    fn description(&self) -> &'static str {
        "Use qualification consistently in multi-table queries."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let count = statement_sum_select(stmt, select_unqualified_reference_count_in_multi_table);
        if count == 0 {
            return Vec::new();
        }

        (0..count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_RF_002,
                    "Use qualified references in multi-table queries.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}
pub struct ReferencesConsistent;

impl LintRule for ReferencesConsistent {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_003
    }

    fn name(&self) -> &'static str {
        "References consistent"
    }

    fn description(&self) -> &'static str {
        "Avoid mixing qualified and unqualified references."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let count = statement_sum_select(stmt, select_mixed_reference_count_single_table);
        if count == 0 {
            return Vec::new();
        }

        (0..count)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_RF_003,
                    "Avoid mixing qualified and unqualified references.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}
define_predicate_rule!(
    ReferencesKeywords,
    issue_codes::LINT_RF_004,
    "References keywords",
    "Avoid SQL keywords as identifiers.",
    warning,
    rule_rf_04,
    "Avoid SQL keywords as identifiers."
);
define_predicate_rule!(
    ReferencesSpecialChars,
    issue_codes::LINT_RF_005,
    "References special chars",
    "Identifiers should avoid special characters.",
    warning,
    rule_rf_05,
    "Identifier contains special characters."
);
define_predicate_rule!(
    ReferencesQuoting,
    issue_codes::LINT_RF_006,
    "References quoting",
    "Avoid unnecessary identifier quoting.",
    info,
    rule_rf_06,
    "Identifier quoting appears unnecessary."
);

define_predicate_rule!(
    StructureSimpleCase,
    issue_codes::LINT_ST_005,
    "Structure simple case",
    "Prefer simple CASE form where applicable.",
    info,
    rule_st_02,
    "CASE expression may be simplified to simple CASE form."
);
pub struct StructureSubquery;

impl LintRule for StructureSubquery {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_006
    }

    fn name(&self) -> &'static str {
        "Structure subquery"
    }

    fn description(&self) -> &'static str {
        "Avoid unnecessary nested subqueries."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = mask_comments_and_single_quoted_strings(stmt_sql(ctx));
        // Focus on trivial wrapper subqueries: FROM (SELECT * FROM ...)
        // More complex derived tables (windowing/aggregation/filtering) are often intentional.
        let re = Regex::new(
            r"(?is)\bfrom\s*\(\s*select\s+\*\s+from\s+[A-Za-z_][A-Za-z0-9_\.]*\s*\)\s+[A-Za-z_][A-Za-z0-9_]*",
        )
        .expect("valid parity regex");
        let Some(found) = re.find(&sql) else {
            return Vec::new();
        };

        let from_start = found.start();
        let from_end = from_start + 4;

        vec![Issue::info(
            issue_codes::LINT_ST_006,
            "Subquery detected; consider refactoring with CTEs.",
        )
        .with_statement(ctx.statement_index)
        .with_span(ctx.span_from_statement_offset(from_start, from_end))]
    }
}
pub struct StructureColumnOrder;

impl LintRule for StructureColumnOrder {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_007
    }

    fn name(&self) -> &'static str {
        "Structure column order"
    }

    fn description(&self) -> &'static str {
        "Place simple columns before complex expressions."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = stmt_sql(ctx);
        let Some((clause, clause_start)) = select_clause_with_span(sql) else {
            return Vec::new();
        };
        let items = split_top_level_commas(&clause);

        let mut seen_expression = false;
        let mut seen_simple = false;
        let mut search_from = 0usize;
        for item in items {
            let raw = item.trim();
            if raw.is_empty() {
                continue;
            }

            let Some(found_rel) = clause[search_from..].find(raw) else {
                continue;
            };
            let item_start = clause_start + search_from + found_rel;
            let item_end = item_start + raw.len();
            search_from += found_rel + raw.len();

            if item_is_simple_identifier(raw) {
                // Only flag when a SELECT list starts with complex expressions
                // and then later switches to simple column references.
                if seen_expression && !seen_simple {
                    return vec![Issue::info(
                        issue_codes::LINT_ST_007,
                        "Prefer simple columns before complex expressions in SELECT.",
                    )
                    .with_statement(ctx.statement_index)
                    .with_span(ctx.span_from_statement_offset(item_start, item_end))];
                }
                seen_simple = true;
            } else {
                seen_expression = true;
            }
        }

        Vec::new()
    }
}
define_predicate_rule!(
    StructureDistinct,
    issue_codes::LINT_ST_008,
    "Structure distinct",
    "DISTINCT usage appears structurally suboptimal.",
    info,
    rule_st_08,
    "DISTINCT usage appears structurally suboptimal."
);
pub struct StructureJoinConditionOrder;

impl LintRule for StructureJoinConditionOrder {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_009
    }

    fn name(&self) -> &'static str {
        "Structure join condition order"
    }

    fn description(&self) -> &'static str {
        "Join condition ordering appears reversed."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut count = statement_sum_select(stmt, select_reversed_join_condition_count);
        if let Statement::Update { selection, .. } = stmt {
            if let Some(expr) = selection {
                count += nested_reversed_join_count_in_expr(expr);
            }
        }

        if count == 0 {
            return Vec::new();
        }

        (0..count)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_ST_009,
                    "Join condition ordering appears reversed.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}
define_predicate_rule!(
    StructureConstantExpression,
    issue_codes::LINT_ST_010,
    "Structure constant expression",
    "Avoid constant boolean expressions in predicates.",
    warning,
    rule_st_10,
    "Constant boolean expression detected in predicate."
);
define_predicate_rule!(
    StructureUnusedJoin,
    issue_codes::LINT_ST_011,
    "Structure unused join",
    "Joined sources should be referenced meaningfully.",
    warning,
    rule_st_11,
    "Joined source appears unused."
);
define_predicate_rule!(
    StructureConsecutiveSemicolons,
    issue_codes::LINT_ST_012,
    "Structure consecutive semicolons",
    "Avoid consecutive semicolons.",
    warning,
    rule_st_12,
    "Consecutive semicolons detected."
);

define_predicate_rule!(
    TsqlSpPrefix,
    issue_codes::LINT_TQ_001,
    "TSQL sp_ prefix",
    "Avoid sp_ procedure prefix in TSQL.",
    warning,
    rule_tq_01,
    "Avoid stored procedure names with sp_ prefix."
);
define_predicate_rule!(
    TsqlProcedureBeginEnd,
    issue_codes::LINT_TQ_002,
    "TSQL procedure BEGIN/END",
    "TSQL procedures should include BEGIN/END block.",
    warning,
    rule_tq_02,
    "CREATE PROCEDURE should include BEGIN/END block."
);
define_predicate_rule!(
    TsqlEmptyBatch,
    issue_codes::LINT_TQ_003,
    "TSQL empty batch",
    "Avoid empty TSQL batches between GO separators.",
    warning,
    rule_tq_03,
    "Empty TSQL batch detected between GO separators."
);

/// Returns all parity rule implementations defined in this module.
pub fn parity_rules() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(AliasingTableStyle),
        Box::new(AliasingColumnStyle),
        Box::new(AliasingUniqueTable),
        Box::new(AliasingLength),
        Box::new(AliasingForbidSingleTable),
        Box::new(AliasingUniqueColumn),
        Box::new(AliasingSelfAliasColumn),
        Box::new(AmbiguousOrderByOrdinal),
        Box::new(AmbiguousJoinStyle),
        Box::new(AmbiguousColumnRefs),
        Box::new(AmbiguousSetColumns),
        Box::new(AmbiguousJoinCondition),
        Box::new(CapitalisationKeywords),
        Box::new(CapitalisationIdentifiers),
        Box::new(CapitalisationFunctions),
        Box::new(CapitalisationLiterals),
        Box::new(CapitalisationTypes),
        Box::new(ConventionNotEqual),
        Box::new(ConventionSelectTrailingComma),
        Box::new(ConventionTerminator),
        Box::new(ConventionStatementBrackets),
        Box::new(ConventionBlockedWords),
        Box::new(ConventionQuotedLiterals),
        Box::new(ConventionCastingStyle),
        Box::new(ConventionJoinCondition),
        Box::new(JinjaPadding),
        Box::new(LayoutSpacing),
        Box::new(LayoutIndent),
        Box::new(LayoutOperators),
        Box::new(LayoutCommas),
        Box::new(LayoutLongLines),
        Box::new(LayoutFunctions),
        Box::new(LayoutCteBracket),
        Box::new(LayoutCteNewline),
        Box::new(LayoutSelectTargets),
        Box::new(LayoutSelectModifiers),
        Box::new(LayoutSetOperators),
        Box::new(LayoutEndOfFile),
        Box::new(LayoutStartOfFile),
        Box::new(LayoutKeywordNewline),
        Box::new(LayoutNewlines),
        Box::new(ReferencesFrom),
        Box::new(ReferencesQualification),
        Box::new(ReferencesConsistent),
        Box::new(ReferencesKeywords),
        Box::new(ReferencesSpecialChars),
        Box::new(ReferencesQuoting),
        Box::new(StructureSimpleCase),
        Box::new(StructureSubquery),
        Box::new(StructureColumnOrder),
        Box::new(StructureDistinct),
        Box::new(StructureJoinConditionOrder),
        Box::new(StructureConstantExpression),
        Box::new(StructureUnusedJoin),
        Box::new(StructureConsecutiveSemicolons),
        Box::new(TsqlSpPrefix),
        Box::new(TsqlProcedureBeginEnd),
        Box::new(TsqlEmptyBatch),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::issue_codes;

    fn run_rule(rule: &dyn LintRule, sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).expect("test SQL should parse");
        let mut issues = Vec::new();
        for (idx, stmt) in stmts.iter().enumerate() {
            let ctx = LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: idx,
            };
            issues.extend(rule.check(stmt, &ctx));
        }
        issues
    }

    #[test]
    fn aliasing_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(
            &AliasingTableStyle,
            "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
        );
        assert_rule_not_triggers(
            &AliasingTableStyle,
            "SELECT * FROM users AS u JOIN orders AS o ON u.id = o.user_id",
        );
        assert_rule_not_triggers(
            &AliasingTableStyle,
            "SELECT * FROM users JOIN orders ON users.id = orders.user_id",
        );
        assert_rule_triggers(&AliasingTableStyle, "SELECT * FROM (SELECT 1 AS id) sub");

        let aliasing_table_style_issues = run_rule(
            &AliasingTableStyle,
            "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
        );
        assert_eq!(
            aliasing_table_style_issues
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_AL_003)
                .count(),
            2,
            "expected one AL_003 issue per implicit table alias"
        );

        assert_rule_triggers(&AliasingColumnStyle, "SELECT a + 1 AS x, b + 2 y FROM t");
        assert_rule_not_triggers(&AliasingColumnStyle, "SELECT a + 1 AS x, b + 2 AS y FROM t");
        assert_rule_not_triggers(&AliasingColumnStyle, "SELECT a + 1 AS x, b + 2 FROM t");
        assert_rule_not_triggers(&AliasingColumnStyle, "SELECT a + 1 x, b + 2 y FROM t");

        assert_rule_triggers(
            &AliasingUniqueTable,
            "SELECT * FROM users u JOIN orders u ON u.id = u.user_id",
        );
        assert_rule_not_triggers(
            &AliasingUniqueTable,
            "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
        );

        assert_rule_triggers(
            &AliasingLength,
            "SELECT * FROM users this_alias_name_is_longer_than_thirty",
        );
        assert_rule_not_triggers(&AliasingLength, "SELECT * FROM users u");

        assert_rule_triggers(&AliasingForbidSingleTable, "SELECT * FROM users u");
        assert_rule_not_triggers(
            &AliasingForbidSingleTable,
            "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
        );

        assert_rule_triggers(&AliasingUniqueColumn, "SELECT a AS x, b AS x FROM t");
        assert_rule_not_triggers(&AliasingUniqueColumn, "SELECT a AS x, b AS y FROM t");
        assert_rule_not_triggers(
            &AliasingUniqueTable,
            "WITH left_side AS (SELECT * FROM users u), right_side AS (SELECT * FROM orders u) SELECT * FROM left_side ls JOIN right_side rs ON ls.id = rs.id",
        );
        assert_rule_not_triggers(
            &AliasingUniqueColumn,
            "WITH left_side AS (SELECT id AS shared_name FROM users), right_side AS (SELECT id AS shared_name FROM orders) SELECT ls.shared_name, rs.shared_name FROM left_side ls JOIN right_side rs ON ls.shared_name = rs.shared_name",
        );

        assert_rule_triggers(&AliasingSelfAliasColumn, "SELECT a AS a FROM t");
        assert_rule_not_triggers(&AliasingSelfAliasColumn, "SELECT a AS b FROM t");
    }

    #[test]
    fn ambiguous_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(&AmbiguousOrderByOrdinal, "SELECT name FROM t ORDER BY 1");
        assert_rule_not_triggers(&AmbiguousOrderByOrdinal, "SELECT name FROM t ORDER BY name");

        assert_rule_triggers(&AmbiguousJoinStyle, "SELECT * FROM a JOIN b ON a.id = b.id");
        assert_rule_not_triggers(
            &AmbiguousJoinStyle,
            "SELECT * FROM a INNER JOIN b ON a.id = b.id",
        );

        assert_rule_triggers(
            &AmbiguousColumnRefs,
            "SELECT * FROM users u JOIN orders o ON id = o.user_id",
        );
        assert_rule_not_triggers(
            &AmbiguousColumnRefs,
            "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
        );
        assert_rule_not_triggers(&AmbiguousColumnRefs, "SELECT a.id, name FROM a");
        assert_rule_not_triggers(&AmbiguousColumnRefs, "SELECT a.id, a.name FROM a");

        assert_rule_triggers(
            &AmbiguousSetColumns,
            "SELECT * FROM a UNION SELECT * FROM b",
        );
        assert_rule_not_triggers(
            &AmbiguousSetColumns,
            "SELECT a FROM a UNION SELECT b FROM b",
        );

        assert_rule_triggers(&AmbiguousJoinCondition, "SELECT * FROM a JOIN b ON TRUE");
        assert_rule_not_triggers(
            &AmbiguousJoinCondition,
            "SELECT * FROM a JOIN b ON a.id = b.id",
        );
    }

    #[test]
    fn capitalisation_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(&CapitalisationKeywords, "SELECT a from t");
        assert_rule_not_triggers(&CapitalisationKeywords, "SELECT a FROM t");

        assert_rule_triggers(&CapitalisationIdentifiers, "SELECT Col, col FROM t");
        assert_rule_not_triggers(&CapitalisationIdentifiers, "SELECT col_one, col_two FROM t");

        assert_rule_triggers(&CapitalisationFunctions, "SELECT COUNT(*), count(x) FROM t");
        assert_rule_not_triggers(&CapitalisationFunctions, "SELECT lower(x), upper(y) FROM t");

        assert_rule_triggers(&CapitalisationLiterals, "SELECT NULL, true FROM t");
        assert_rule_not_triggers(&CapitalisationLiterals, "SELECT NULL, TRUE FROM t");

        assert_rule_triggers(
            &CapitalisationTypes,
            "CREATE TABLE t (a INT, b varchar(10))",
        );
        assert_rule_not_triggers(
            &CapitalisationTypes,
            "CREATE TABLE t (a int, b varchar(10))",
        );
    }

    #[test]
    fn convention_and_jinja_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(
            &ConventionNotEqual,
            "SELECT * FROM t WHERE a <> b AND c != d",
        );
        assert_rule_not_triggers(&ConventionNotEqual, "SELECT * FROM t WHERE a <> b");
        assert_rule_not_triggers(&ConventionNotEqual, "SELECT * FROM t WHERE a != b");

        assert_rule_triggers(&ConventionSelectTrailingComma, "SELECT a, FROM t");
        assert_rule_not_triggers(&ConventionSelectTrailingComma, "SELECT a, b FROM t");

        assert_rule_triggers(&ConventionTerminator, "SELECT 1; SELECT 2");
        assert_rule_not_triggers(&ConventionTerminator, "SELECT 1; SELECT 2;");

        assert_rule_triggers(&ConventionStatementBrackets, "(SELECT 1)");
        assert_rule_not_triggers(&ConventionStatementBrackets, "SELECT 1");

        assert_rule_triggers(&ConventionBlockedWords, "SELECT foo FROM t");
        assert_rule_not_triggers(&ConventionBlockedWords, "SELECT customer_id FROM t");

        assert_rule_triggers(&ConventionQuotedLiterals, "SELECT \"abc\" FROM t");
        assert_rule_not_triggers(&ConventionQuotedLiterals, "SELECT 'abc' FROM t");

        assert_rule_triggers(
            &ConventionCastingStyle,
            "SELECT CAST(amount AS INT)::TEXT FROM t",
        );
        assert_rule_not_triggers(&ConventionCastingStyle, "SELECT amount::INT FROM t");
        assert_rule_not_triggers(&ConventionCastingStyle, "SELECT CAST(amount AS INT) FROM t");

        assert_rule_triggers(
            &ConventionJoinCondition,
            "SELECT * FROM a JOIN b ON b.id > 0",
        );
        assert_rule_not_triggers(
            &ConventionJoinCondition,
            "SELECT * FROM a JOIN b ON a.id = b.id",
        );

        assert_rule_triggers(&JinjaPadding, "SELECT '{{foo}}' AS templated");
        assert_rule_not_triggers(&JinjaPadding, "SELECT '{{ foo }}' AS templated");
    }

    #[test]
    fn layout_rules_cover_fail_and_pass_cases() {
        assert_rule_not_triggers(&LayoutSpacing, "SELECT * FROM t WHERE a = 1");

        let lt01_json = run_rule(&LayoutSpacing, "SELECT payload->>'id' FROM t");
        assert_eq!(
            lt01_json
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_001)
                .count(),
            2,
            "expected two LT_001 issues around compact ->> JSON extraction"
        );
        assert_rule_triggers(&LayoutSpacing, "SELECT ARRAY['x']::text[]");
        assert_rule_triggers(&LayoutSpacing, "SELECT 1::numeric(5,2)");
        assert_rule_triggers(
            &LayoutSpacing,
            "SELECT
    EXISTS (
        SELECT 1
    ) AS has_row",
        );

        assert_rule_triggers(&LayoutIndent, "SELECT a\n   , b\nFROM t");
        assert_rule_not_triggers(&LayoutIndent, "SELECT a\n    , b\nFROM t");

        assert_rule_triggers(&LayoutOperators, "SELECT a +\n b FROM t");
        assert_rule_not_triggers(&LayoutOperators, "SELECT a\n + b FROM t");

        assert_rule_triggers(&LayoutCommas, "SELECT a,b FROM t");
        assert_rule_not_triggers(&LayoutCommas, "SELECT a, b FROM t");

        let long_line = format!("SELECT {} FROM t", "x".repeat(320));
        assert_rule_triggers(&LayoutLongLines, &long_line);
        assert_rule_not_triggers(&LayoutLongLines, "SELECT x FROM t");

        let lt05_multi = run_rule(
            &LayoutLongLines,
            &format!(
                "SELECT {} AS a,
       {} AS b FROM t",
                "x".repeat(90),
                "y".repeat(90)
            ),
        );
        assert_eq!(
            lt05_multi
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_005)
                .count(),
            2,
            "expected one LT_005 issue per overlong line"
        );

        assert_rule_triggers(&LayoutFunctions, "SELECT COUNT (1) FROM t");
        assert_rule_not_triggers(&LayoutFunctions, "SELECT COUNT(1) FROM t");

        let lt07 = run_rule(
            &LayoutCteBracket,
            "SELECT 'WITH cte AS SELECT 1' AS sql_snippet",
        );
        assert!(
            lt07.iter()
                .any(|issue| issue.code == issue_codes::LINT_LT_007),
            "expected {} to trigger; got: {lt07:?}",
            issue_codes::LINT_LT_007,
        );
        assert_rule_not_triggers(
            &LayoutCteBracket,
            "WITH cte AS (SELECT 1) SELECT * FROM cte",
        );

        assert_rule_triggers(
            &LayoutCteNewline,
            "WITH cte AS (SELECT 1) SELECT * FROM cte",
        );
        assert_rule_triggers(
            &LayoutCteNewline,
            "WITH cte AS (SELECT 1)
SELECT * FROM cte",
        );
        assert_rule_not_triggers(
            &LayoutCteNewline,
            "WITH cte AS (SELECT 1)

SELECT * FROM cte",
        );

        let lt08_multi = run_rule(
            &LayoutCteNewline,
            "WITH a AS (SELECT 1),
-- comment between CTEs
b AS (SELECT 2)
SELECT * FROM b",
        );
        assert_eq!(
            lt08_multi
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_008)
                .count(),
            2,
            "expected one LT_008 issue after each CTE without a separating blank line"
        );

        assert_rule_triggers(&LayoutSelectTargets, "SELECT a,b,c,d,e FROM t");
        assert_rule_not_triggers(&LayoutSelectTargets, "SELECT a FROM t");

        let lt09_multi = run_rule(
            &LayoutSelectTargets,
            "SELECT a, b FROM t UNION ALL SELECT c, d FROM t",
        );
        assert_eq!(
            lt09_multi
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_009)
                .count(),
            2,
            "expected one LT_009 issue per SELECT line with multiple top-level targets"
        );

        assert_rule_triggers(&LayoutSelectModifiers, "SELECT\nDISTINCT a\nFROM t");
        assert_rule_not_triggers(&LayoutSelectModifiers, "SELECT DISTINCT a FROM t");

        assert_rule_triggers(
            &LayoutSetOperators,
            "SELECT 1 UNION SELECT 2\nUNION SELECT 3",
        );
        assert_rule_not_triggers(
            &LayoutSetOperators,
            "SELECT 1\nUNION\nSELECT 2\nUNION\nSELECT 3",
        );

        assert_rule_triggers(&LayoutEndOfFile, "SELECT 1\nFROM t");
        assert_rule_not_triggers(&LayoutEndOfFile, "SELECT 1\nFROM t\n");

        assert_rule_triggers(&LayoutStartOfFile, "\n\nSELECT 1");
        assert_rule_not_triggers(&LayoutStartOfFile, "SELECT 1");

        assert_rule_triggers(&LayoutKeywordNewline, "SELECT a FROM t WHERE a = 1");
        assert_rule_triggers(&LayoutKeywordNewline, "SELECT a FROM t\nWHERE a = 1");
        assert_rule_not_triggers(&LayoutKeywordNewline, "SELECT a FROM t");
        assert_rule_not_triggers(&LayoutKeywordNewline, "SELECT a\nFROM t\nWHERE a = 1");

        assert_rule_triggers(&LayoutNewlines, "SELECT 1\n\n\nFROM t");
        assert_rule_not_triggers(&LayoutNewlines, "SELECT 1\n\nFROM t");
    }

    #[test]
    fn references_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(&ReferencesFrom, "SELECT x.id FROM users");
        assert_rule_not_triggers(&ReferencesFrom, "SELECT users.id FROM users");
        assert_rule_not_triggers(
            &ReferencesFrom,
            "SELECT workspace_id FROM ledger.resource_ownership",
        );
        assert_rule_not_triggers(
            &ReferencesFrom,
            "INSERT INTO t (id) VALUES (1) ON CONFLICT (id) DO UPDATE SET x = excluded.x",
        );
        assert_rule_not_triggers(
            &ReferencesFrom,
            "UPDATE insight.insight i SET status = 'resolved' WHERE i.id = 1",
        );

        assert_rule_triggers(
            &ReferencesQualification,
            "SELECT id FROM users u JOIN orders o ON u.id = o.user_id",
        );
        assert_rule_triggers(
            &ReferencesQualification,
            "SELECT u.id FROM users u JOIN orders o ON id = o.user_id",
        );
        assert_rule_triggers(
            &ReferencesQualification,
            "SELECT u.id, COUNT(*) FROM users u JOIN orders o ON u.id = o.user_id GROUP BY id",
        );
        assert_rule_not_triggers(
            &ReferencesQualification,
            "SELECT u.id FROM users u JOIN orders o ON u.id = o.user_id",
        );
        assert_rule_not_triggers(
            &ReferencesQualification,
            "SELECT u.id FROM users u JOIN orders o ON u.id = o.user_id WHERE o.status = 'open'",
        );

        let rf02_multi = run_rule(
            &ReferencesQualification,
            "SELECT id, name FROM users u JOIN orders o ON u.id = o.user_id",
        );
        assert!(
            rf02_multi
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_RF_002)
                .count()
                >= 2,
            "expected multiple {} issues; got: {rf02_multi:?}",
            issue_codes::LINT_RF_002,
        );

        assert_rule_triggers(&ReferencesConsistent, "SELECT u.id, id FROM users u");
        assert_rule_not_triggers(
            &ReferencesConsistent,
            "SELECT u.id, id FROM users u JOIN orders o ON u.id = o.user_id",
        );
        assert_rule_not_triggers(&ReferencesConsistent, "SELECT u.id, u.name FROM users u");

        let rf04 = run_rule(
            &ReferencesKeywords,
            "SELECT 'FROM tbl AS SELECT' AS sql_snippet",
        );
        assert!(
            rf04.iter()
                .any(|issue| issue.code == issue_codes::LINT_RF_004),
            "expected {} to trigger; got: {rf04:?}",
            issue_codes::LINT_RF_004,
        );
        assert_rule_not_triggers(
            &ReferencesKeywords,
            "SELECT 'FROM tbl AS alias_name' AS sql_snippet",
        );

        assert_rule_triggers(&ReferencesSpecialChars, "SELECT \"bad-name\" FROM t");
        assert_rule_not_triggers(&ReferencesSpecialChars, "SELECT \"good_name\" FROM t");

        assert_rule_triggers(&ReferencesQuoting, "SELECT \"good_name\" FROM t");
        assert_rule_not_triggers(&ReferencesQuoting, "SELECT \"bad-name\" FROM t");
    }

    #[test]
    fn structure_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(
            &StructureSimpleCase,
            "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' END FROM t",
        );
        assert_rule_not_triggers(
            &StructureSimpleCase,
            "SELECT CASE WHEN x = 1 THEN 'a' WHEN y = 2 THEN 'b' END FROM t",
        );

        assert_rule_triggers(&StructureSubquery, "SELECT * FROM (SELECT * FROM t) sub");
        assert_rule_not_triggers(
            &StructureSubquery,
            "WITH cte AS (SELECT 1) SELECT * FROM cte",
        );
        assert_rule_not_triggers(
            &StructureSubquery,
            "SELECT * FROM t WHERE id IN (SELECT id FROM t2)",
        );

        assert_rule_triggers(&StructureColumnOrder, "SELECT a + 1, a FROM t");
        assert_rule_not_triggers(&StructureColumnOrder, "SELECT a, a + 1 FROM t");
        assert_rule_not_triggers(&StructureColumnOrder, "SELECT a, a + 1, b FROM t");
        assert_rule_not_triggers(&StructureColumnOrder, "SELECT a AS first_a, b FROM t");

        assert_rule_triggers(&StructureDistinct, "SELECT DISTINCT(a) FROM t");
        assert_rule_not_triggers(&StructureDistinct, "SELECT DISTINCT a FROM t");

        assert_rule_triggers(
            &StructureJoinConditionOrder,
            "SELECT * FROM users u JOIN orders o ON o.user_id = u.id",
        );

        let st09_multi = run_rule(
            &StructureJoinConditionOrder,
            "SELECT * FROM ledger.query_history q LEFT JOIN ledger.warehouse wh ON wh.id = q.warehouse_id LEFT JOIN ledger.workspace ws ON ws.id = q.workspace_id",
        );
        assert_eq!(
            st09_multi
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_ST_009)
                .count(),
            1,
            "expected one ST_009 issue for immediate reversed LEFT JOIN ordering"
        );
        assert_rule_not_triggers(
            &StructureJoinConditionOrder,
            "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
        );

        assert_rule_triggers(&StructureConstantExpression, "SELECT * FROM t WHERE 1 = 1");
        assert_rule_not_triggers(&StructureConstantExpression, "SELECT * FROM t WHERE id = 1");

        assert_rule_triggers(
            &StructureUnusedJoin,
            "SELECT u.id FROM users u JOIN orders o ON u.id = u.id",
        );
        assert_rule_not_triggers(
            &StructureUnusedJoin,
            "SELECT u.id, o.id FROM users u JOIN orders o ON o.user_id = u.id",
        );

        assert_rule_triggers(&StructureConsecutiveSemicolons, "SELECT 1;;");
        assert_rule_not_triggers(&StructureConsecutiveSemicolons, "SELECT 1;");
    }

    #[test]
    fn tsql_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(
            &TsqlSpPrefix,
            "SELECT 'CREATE PROCEDURE sp_legacy AS SELECT 1' AS sql_snippet",
        );
        assert_rule_not_triggers(
            &TsqlSpPrefix,
            "SELECT 'CREATE PROCEDURE proc_legacy AS SELECT 1' AS sql_snippet",
        );

        assert_rule_triggers(
            &TsqlProcedureBeginEnd,
            "SELECT 'CREATE PROCEDURE p AS SELECT 1' AS sql_snippet",
        );
        assert_rule_not_triggers(
            &TsqlProcedureBeginEnd,
            "SELECT 'CREATE PROCEDURE p AS BEGIN SELECT 1 END' AS sql_snippet",
        );

        assert_rule_triggers(&TsqlEmptyBatch, "SELECT '\nGO\nGO\n' AS sql_snippet");
        assert_rule_not_triggers(&TsqlEmptyBatch, "SELECT '\nGO\n' AS sql_snippet");
    }

    #[test]
    fn join_on_expr_handles_left_join_operator() {
        let stmts = parse_sql("SELECT * FROM a x LEFT JOIN b y ON y.id = x.id")
            .expect("test SQL should parse");
        let Some(Statement::Query(query)) = stmts.first() else {
            panic!("expected query statement");
        };
        let SetExpr::Select(select) = &*query.body else {
            panic!("expected select query body");
        };
        let Some(join) = select.from.first().and_then(|from| from.joins.first()) else {
            panic!("expected one join");
        };

        assert!(
            join_on_expr(&join.join_operator).is_some(),
            "left join operator should carry ON expression"
        );
    }

    #[test]
    fn masking_comments_and_single_quoted_strings_preserves_sql_shape() {
        let sql = "-- comment\nSELECT 'a''b', col /* block\ncomment */ FROM t\nWHERE x = 'y'";
        let masked = mask_comments_and_single_quoted_strings(sql);

        assert_eq!(
            masked.len(),
            sql.len(),
            "masked SQL should preserve byte length"
        );
        assert_eq!(
            masked.lines().count(),
            sql.lines().count(),
            "masked SQL should preserve line structure"
        );
        assert!(masked.contains("SELECT"));
        assert!(masked.contains("FROM t"));
        assert!(!masked.contains("comment"));
        assert!(!masked.contains("a''b"));
        assert!(!masked.contains("y"));
    }

    fn assert_rule_triggers(rule: &dyn LintRule, sql: &str) {
        let issues = run_rule(rule, sql);
        assert!(
            issues.iter().any(|issue| issue.code == rule.code()),
            "expected {} to trigger for SQL: {sql}; got: {issues:?}",
            rule.code(),
        );
    }

    fn assert_rule_not_triggers(rule: &dyn LintRule, sql: &str) {
        let issues = run_rule(rule, sql);
        assert!(
            !issues.iter().any(|issue| issue.code == rule.code()),
            "did not expect {} for SQL: {sql}; got: {issues:?}",
            rule.code(),
        );
    }
}
