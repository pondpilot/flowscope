//! LINT_LT_001: Layout spacing.
//!
//! SQLFluff LT01 parity (current scope): detect compact operator-style patterns
//! where spacing is expected.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

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

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        spacing_violation_spans(ctx.statement_sql())
            .into_iter()
            .map(|(start, end)| {
                Issue::info(
                    issue_codes::LINT_LT_001,
                    "Operator spacing appears inconsistent.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
            })
            .collect()
    }
}

fn spacing_violation_spans(sql: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let bytes = sql.as_bytes();

    // Pattern group 1: compact JSON arrow expression like payload->>'id'.
    for index in 0..bytes.len().saturating_sub(2) {
        if &bytes[index..index + 3] != b"->>" {
            continue;
        }
        if !matches_compact_json_arrow(sql, index) {
            continue;
        }

        spans.push((index, (index + 1).min(bytes.len())));
        if index + 3 < bytes.len() {
            spans.push((index + 3, (index + 4).min(bytes.len())));
        }
    }

    // Pattern group 2: compact `text[` cast/index form.
    for index in find_ascii_case_insensitive(sql, "text[") {
        if index > 0 && is_word_char(bytes[index - 1]) {
            continue;
        }
        let bracket_start = index + 4;
        if bracket_start < bytes.len() {
            spans.push((bracket_start, bracket_start + 1));
        }
    }

    // Pattern group 3: compact numeric precision form like `,2`.
    for index in 0..bytes.len().saturating_sub(1) {
        if bytes[index] == b',' && bytes[index + 1].is_ascii_digit() {
            spans.push((index + 1, index + 2));
        }
    }

    // Pattern group 4: `EXISTS (` layout at line start.
    let lines = collect_lines_with_offsets(sql);
    for (line_index, (line_start, line)) in lines.iter().enumerate() {
        let Some(paren_offset) = exists_line_paren_offset(line) else {
            continue;
        };

        let prev_token = lines[..line_index]
            .iter()
            .rev()
            .map(|(_, prev_line)| prev_line.trim())
            .find(|prev_line| !prev_line.is_empty() && !prev_line.starts_with("--"));
        if matches!(prev_token, Some("OR") | Some("AND") | Some("NOT")) {
            continue;
        }

        let start = line_start + paren_offset;
        spans.push((start, start + 1));
    }

    spans
}

fn matches_compact_json_arrow(sql: &str, arrow_index: usize) -> bool {
    let bytes = sql.as_bytes();

    // Left side must be an identifier/path token.
    let mut start = arrow_index;
    while start > 0 && is_ident_or_dot(bytes[start - 1]) {
        start -= 1;
    }
    if start == arrow_index {
        return false;
    }

    let left = &bytes[start..arrow_index];
    if !left.first().copied().is_some_and(is_identifier_start) {
        return false;
    }

    // Right side must be a single-quoted literal (`'[^']*'`).
    let mut pos = arrow_index + 3;
    if pos >= bytes.len() || bytes[pos] != b'\'' {
        return false;
    }
    pos += 1;
    while pos < bytes.len() && bytes[pos] != b'\'' {
        pos += 1;
    }
    pos < bytes.len()
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Vec<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for index in 0..=h.len() - n.len() {
        if h[index..index + n.len()].eq_ignore_ascii_case(n) {
            out.push(index);
        }
    }
    out
}

fn collect_lines_with_offsets(sql: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut offset = 0usize;

    for line in sql.split_inclusive('\n') {
        let line = line.strip_suffix('\n').unwrap_or(line);
        out.push((offset, line));
        offset += line.len();
        if sql.as_bytes().get(offset) == Some(&b'\n') {
            offset += 1;
        }
    }

    if out.is_empty() {
        out.push((0, sql));
    }

    out
}

fn exists_line_paren_offset(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }

    let exists = b"exists";
    if index + exists.len() > bytes.len() {
        return None;
    }
    if !bytes[index..index + exists.len()].eq_ignore_ascii_case(exists) {
        return None;
    }
    index += exists.len();

    let ws_start = index;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    if index == ws_start {
        return None;
    }

    (index < bytes.len() && bytes[index] == b'(').then_some(index)
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_or_dot(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.'
}

fn is_word_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutSpacing;
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check(
                    statement,
                    &LintContext {
                        sql,
                        statement_range: 0..sql.len(),
                        statement_index: index,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn does_not_flag_simple_spacing() {
        assert!(run("SELECT * FROM t WHERE a = 1").is_empty());
    }

    #[test]
    fn flags_compact_json_arrow_operator() {
        let issues = run("SELECT payload->>'id' FROM t");
        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_001)
                .count(),
            2,
        );
    }

    #[test]
    fn flags_compact_type_bracket_and_numeric_scale() {
        assert!(!run("SELECT ARRAY['x']::text[]").is_empty());
        assert!(!run("SELECT 1::numeric(5,2)").is_empty());
    }

    #[test]
    fn flags_exists_parenthesis_layout_case() {
        let issues = run("SELECT\n    EXISTS (\n        SELECT 1\n    ) AS has_row");
        assert!(!issues.is_empty());
    }
}
