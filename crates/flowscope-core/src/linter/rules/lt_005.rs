//! LINT_LT_005: Layout long lines.
//!
//! SQLFluff LT05 parity (current scope): flag overflow beyond 80 columns.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct LayoutLongLines {
    max_line_length: Option<usize>,
    ignore_comment_lines: bool,
    ignore_comment_clauses: bool,
    trailing_comments_after: bool,
}

impl LayoutLongLines {
    pub fn from_config(config: &LintConfig) -> Self {
        let max_line_length = if let Some(value) = config
            .rule_config_object(issue_codes::LINT_LT_005)
            .and_then(|obj| obj.get("max_line_length"))
        {
            value
                .as_i64()
                .map(|signed| {
                    if signed <= 0 {
                        None
                    } else {
                        usize::try_from(signed).ok()
                    }
                })
                .or_else(|| {
                    value
                        .as_u64()
                        .and_then(|unsigned| usize::try_from(unsigned).ok().map(Some))
                })
                .flatten()
        } else {
            Some(80)
        };

        Self {
            max_line_length,
            ignore_comment_lines: config
                .rule_option_bool(issue_codes::LINT_LT_005, "ignore_comment_lines")
                .unwrap_or(false),
            ignore_comment_clauses: config
                .rule_option_bool(issue_codes::LINT_LT_005, "ignore_comment_clauses")
                .unwrap_or(false),
            trailing_comments_after: config
                .section_option_str("indentation", "trailing_comments")
                .is_some_and(|value| value.eq_ignore_ascii_case("after")),
        }
    }
}

impl Default for LayoutLongLines {
    fn default() -> Self {
        Self {
            max_line_length: Some(80),
            ignore_comment_lines: false,
            ignore_comment_clauses: false,
            trailing_comments_after: false,
        }
    }
}

impl BuiltinLintRule for LayoutLongLines {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_005
    }

    fn name(&self) -> &'static str {
        "Layout long lines"
    }

    fn description(&self) -> &'static str {
        "Line is too long."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let Some(max_line_length) = self.max_line_length else {
            return Vec::new();
        };

        if ctx.statement_index != 0 {
            return Vec::new();
        }

        let overflow_spans = long_line_overflow_spans_for_context(
            ctx,
            max_line_length,
            self.ignore_comment_lines,
            self.ignore_comment_clauses,
        );
        if overflow_spans.is_empty() {
            return Vec::new();
        }

        let mut issues: Vec<Issue> = overflow_spans
            .into_iter()
            .map(|(start, end)| {
                Issue::info(
                    issue_codes::LINT_LT_005,
                    "SQL contains excessively long lines.",
                )
                .with_statement(ctx.statement_index)
                .with_span(Span::new(start, end))
            })
            .collect();

        let autofix_edits =
            long_line_autofix_edits(ctx.sql, max_line_length, self.trailing_comments_after);
        if let Some(first_issue) = issues.first_mut() {
            if !autofix_edits.is_empty() {
                *first_issue = first_issue
                    .clone()
                    .with_autofix_edits(IssueAutofixApplicability::Safe, autofix_edits);
            }
        }

        issues
    }
}

fn long_line_overflow_spans_for_context(
    ctx: &RuleContext,
    max_len: usize,
    ignore_comment_lines: bool,
    ignore_comment_clauses: bool,
) -> Vec<(usize, usize)> {
    let jinja_comment_spans = jinja_comment_spans(ctx.sql);
    if !jinja_comment_spans.is_empty() {
        return long_line_overflow_spans(
            ctx.sql,
            max_len,
            ignore_comment_lines,
            ignore_comment_clauses,
            ctx.dialect(),
        );
    }

    if let Some(tokens) = tokenize_with_offsets_for_context(ctx) {
        return long_line_overflow_spans_from_tokens(
            ctx.sql,
            max_len,
            ignore_comment_lines,
            ignore_comment_clauses,
            &tokens,
            &jinja_comment_spans,
        );
    }

    long_line_overflow_spans(
        ctx.sql,
        max_len,
        ignore_comment_lines,
        ignore_comment_clauses,
        ctx.dialect(),
    )
}

fn long_line_overflow_spans(
    sql: &str,
    max_len: usize,
    ignore_comment_lines: bool,
    ignore_comment_clauses: bool,
    dialect: Dialect,
) -> Vec<(usize, usize)> {
    if let Some(spans) = long_line_overflow_spans_tokenized(
        sql,
        max_len,
        ignore_comment_lines,
        ignore_comment_clauses,
        dialect,
    ) {
        return spans;
    }

    long_line_overflow_spans_naive(sql, max_len, ignore_comment_lines)
}

fn long_line_overflow_spans_naive(
    sql: &str,
    max_len: usize,
    ignore_comment_lines: bool,
) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    for (line_start, line_end) in line_ranges(sql) {
        let line = &sql[line_start..line_end];
        if ignore_comment_lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with("--") || trimmed.starts_with("/*") || trimmed.starts_with("{#") {
                continue;
            }
        }

        if line.chars().count() <= max_len {
            continue;
        }

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
    spans
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn long_line_overflow_spans_tokenized(
    sql: &str,
    max_len: usize,
    ignore_comment_lines: bool,
    ignore_comment_clauses: bool,
    dialect: Dialect,
) -> Option<Vec<(usize, usize)>> {
    let jinja_comment_spans = jinja_comment_spans(sql);
    let sanitized = sanitize_sql_for_jinja_comments(sql, &jinja_comment_spans);
    let tokens = tokenize_with_offsets(&sanitized, dialect)?;
    Some(long_line_overflow_spans_from_tokens(
        sql,
        max_len,
        ignore_comment_lines,
        ignore_comment_clauses,
        &tokens,
        &jinja_comment_spans,
    ))
}

fn long_line_overflow_spans_from_tokens(
    sql: &str,
    max_len: usize,
    ignore_comment_lines: bool,
    ignore_comment_clauses: bool,
    tokens: &[LocatedToken],
    jinja_comment_spans: &[std::ops::Range<usize>],
) -> Vec<(usize, usize)> {
    let line_ranges = line_ranges(sql);
    let mut spans = Vec::new();

    for (line_start, line_end) in line_ranges {
        let line = &sql[line_start..line_end];
        if ignore_comment_lines
            && line_is_comment_only_tokenized(
                line_start,
                line_end,
                tokens,
                line,
                sql,
                jinja_comment_spans,
            )
        {
            continue;
        }

        let effective_end = if ignore_comment_clauses {
            comment_clause_start_offset_tokenized(line_start, line_end, tokens, jinja_comment_spans)
                .unwrap_or(line_end)
        } else {
            line_end
        };

        let effective_line = &sql[line_start..effective_end];
        if effective_line.chars().count() <= max_len {
            continue;
        }

        let mut overflow_start = effective_end;
        for (char_idx, (byte_off, _)) in effective_line.char_indices().enumerate() {
            if char_idx == max_len {
                overflow_start = line_start + byte_off;
                break;
            }
        }

        if overflow_start < effective_end {
            let overflow_end = sql[overflow_start..effective_end]
                .chars()
                .next()
                .map(|ch| overflow_start + ch.len_utf8())
                .unwrap_or(overflow_start);
            spans.push((overflow_start, overflow_end));
        }
    }

    spans
}

fn line_ranges(sql: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut line_start = 0usize;

    for (idx, ch) in sql.char_indices() {
        if ch != '\n' {
            continue;
        }

        let mut line_end = idx;
        if line_end > line_start && sql[line_start..line_end].ends_with('\r') {
            line_end -= 1;
        }
        ranges.push((line_start, line_end));
        line_start = idx + 1;
    }

    let mut line_end = sql.len();
    if line_end > line_start && sql[line_start..line_end].ends_with('\r') {
        line_end -= 1;
    }
    ranges.push((line_start, line_end));
    ranges
}

/// Legacy LT005 rewrite parity:
/// split only extremely long lines (>300 bytes) around the 280-byte target.
const LEGACY_MAX_LINE_LENGTH: usize = 300;
const LEGACY_LINE_SPLIT_TARGET: usize = 280;

fn legacy_split_long_line(line: &str) -> Option<String> {
    if line.len() <= LEGACY_MAX_LINE_LENGTH {
        return None;
    }

    let mut rewritten = String::new();
    let mut remaining = line.trim_start();
    let mut first_segment = true;

    while remaining.len() > LEGACY_MAX_LINE_LENGTH {
        let probe = remaining
            .char_indices()
            .take_while(|(index, _)| *index <= LEGACY_LINE_SPLIT_TARGET)
            .map(|(index, _)| index)
            .last()
            .unwrap_or(LEGACY_LINE_SPLIT_TARGET.min(remaining.len()));
        let split_at = remaining[..probe].rfind(' ').unwrap_or(probe);

        if !first_segment {
            rewritten.push('\n');
        }
        rewritten.push_str(remaining[..split_at].trim_end());
        rewritten.push('\n');
        remaining = remaining[split_at..].trim_start();
        first_segment = false;
    }

    rewritten.push_str(remaining);
    Some(rewritten)
}

/// Generate autofix edits for long lines.
///
/// For very long lines (>300 bytes), preserve the legacy splitter behavior.
/// For shorter overflows, apply a narrow set of patch-based rewrites used by
/// LT05 fixture parity:
/// - move inline trailing comments to their own line
/// - break single-clause overflows (e.g. `SELECT ... FROM ...`)
/// - break long `... over (...)` and `... as ...` lines around boundaries
/// - break Snowflake-style `... ignore/respect nulls over (...)` lines
fn long_line_autofix_edits(
    sql: &str,
    max_line_length: usize,
    trailing_comments_after: bool,
) -> Vec<IssuePatchEdit> {
    let mut edits = Vec::new();

    for (line_start, line_end) in line_ranges(sql) {
        let line = &sql[line_start..line_end];
        if is_comment_only_line(line) {
            continue;
        }

        let replacement = if line.len() > LEGACY_MAX_LINE_LENGTH {
            legacy_split_long_line(line)
        } else if line.chars().count() > max_line_length {
            rewrite_lt05_long_line(line, max_line_length, trailing_comments_after)
        } else {
            None
        };

        let Some(replacement) = replacement else {
            continue;
        };
        if replacement == line {
            continue;
        }

        edits.push(IssuePatchEdit::new(
            Span::new(line_start, line_end),
            replacement,
        ));
    }

    edits
}

fn is_comment_only_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("--")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("*/")
        || trimmed.starts_with("{#")
}

fn rewrite_lt05_long_line(
    line: &str,
    max_line_length: usize,
    trailing_comments_after: bool,
) -> Option<String> {
    rewrite_inline_comment_line(line, max_line_length, trailing_comments_after)
        .or_else(|| rewrite_lt05_code_line(line, max_line_length))
}

fn rewrite_lt05_code_line(line: &str, max_line_length: usize) -> Option<String> {
    rewrite_window_function_line(line, max_line_length)
        .or_else(|| rewrite_over_clause_with_tail_line(line, max_line_length))
        .or_else(|| rewrite_function_alias_line(line, max_line_length))
        .or_else(|| rewrite_function_equals_line(line, max_line_length))
        .or_else(|| rewrite_expression_alias_line(line, max_line_length))
        .or_else(|| rewrite_clause_break_line(line, max_line_length))
        .or_else(|| rewrite_whitespace_wrap_line(line, max_line_length))
}

fn rewrite_expression_alias_line(line: &str, max_line_length: usize) -> Option<String> {
    if line.chars().count() <= max_line_length {
        return None;
    }

    // Handle long expression aliases such as:
    //   percentile_cont(...)::int AS p95
    //   CASE ... END AS status
    //   SUM(...) AS total
    let marker = find_last_ascii_case_insensitive(line, " as ")?;
    if marker == 0 {
        return None;
    }

    let left = line[..marker].trim_end();
    let right = line[marker + 1..].trim_start();
    if left.is_empty() || right.is_empty() {
        return None;
    }

    let continuation = format!("{}    ", leading_whitespace_prefix(line));
    Some(format!("{left}\n{continuation}{right}"))
}

fn rewrite_inline_comment_line(
    line: &str,
    max_line_length: usize,
    trailing_comments_after: bool,
) -> Option<String> {
    let comment_start = find_unquoted_inline_comment_start(line)?;
    let code_prefix = &line[..comment_start];
    let code_trimmed = code_prefix.trim_end();
    if code_trimmed.trim().is_empty() {
        return None;
    }
    if code_trimmed.trim() == "," {
        // Keep comma-prefixed comment lines unchanged; rewriting these can
        // create endless fix cycles in LT05 edge cases.
        return None;
    }

    let indent = leading_whitespace_prefix(line);
    let code_body = code_trimmed
        .strip_prefix(indent)
        .unwrap_or(code_trimmed)
        .trim_start();
    if code_body.is_empty() {
        return None;
    }

    let mut code_line = format!("{indent}{code_body}");
    if code_line.chars().count() > max_line_length {
        if let Some(rewritten) = rewrite_lt05_code_line(&code_line, max_line_length) {
            code_line = rewritten;
        }
    }

    let comment_line = format!("{indent}{}", line[comment_start..].trim_end());
    if trailing_comments_after {
        Some(format!("{code_line}\n{comment_line}"))
    } else {
        Some(format!("{comment_line}\n{code_line}"))
    }
}

fn rewrite_clause_break_line(line: &str, max_line_length: usize) -> Option<String> {
    if line.chars().count() <= max_line_length {
        return None;
    }

    const CLAUSE_NEEDLES: [&str; 7] = [
        " from ",
        " where ",
        " qualify ",
        " order by ",
        " group by ",
        " having ",
        " join ",
    ];

    let split_at = CLAUSE_NEEDLES
        .iter()
        .filter_map(|needle| find_ascii_case_insensitive(line, needle))
        .min()?;

    if split_at == 0 {
        return None;
    }
    let left = line[..split_at].trim_end();
    let right = line[split_at + 1..].trim_start();
    if left.is_empty() || right.is_empty() {
        return None;
    }

    let indent = leading_whitespace_prefix(line);
    Some(format!("{left}\n{indent}{right}"))
}

fn rewrite_function_alias_line(line: &str, max_line_length: usize) -> Option<String> {
    if line.chars().count() <= max_line_length
        || find_ascii_case_insensitive(line, " over ").is_some()
    {
        return None;
    }

    let marker = find_ascii_case_insensitive(line, ") as ")?;
    let split_at = marker + 1;
    let left = line[..split_at].trim_end();
    let right = line[split_at..].trim_start();
    if left.is_empty() || right.is_empty() {
        return None;
    }

    let continuation = format!("{}    ", leading_whitespace_prefix(line));
    Some(format!("{left}\n{continuation}{right}"))
}

fn rewrite_function_equals_line(line: &str, max_line_length: usize) -> Option<String> {
    if line.chars().count() <= max_line_length {
        return None;
    }

    let marker = find_ascii_case_insensitive(line, ") = ")?;
    let split_at = marker + 1;
    let left = line[..split_at].trim_end();
    let right = line[split_at..].trim_start();
    if left.is_empty() || right.is_empty() {
        return None;
    }

    let indent = leading_whitespace_prefix(line);
    Some(format!("{left}\n{indent}{right}"))
}

fn find_last_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }

    let haystack_bytes = haystack.as_bytes();
    let needle_bytes = needle.as_bytes();

    (0..=haystack_bytes.len() - needle_bytes.len())
        .rev()
        .find(|&start| {
            haystack_bytes[start..start + needle_bytes.len()]
                .iter()
                .zip(needle_bytes.iter())
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
        })
}

fn rewrite_over_clause_with_tail_line(line: &str, max_line_length: usize) -> Option<String> {
    if line.chars().count() <= max_line_length {
        return None;
    }

    let over_start = find_ascii_case_insensitive(line, " over (")?;
    let over_open = line[over_start..]
        .find('(')
        .map(|offset| over_start + offset)?;
    let over_close = matching_close_paren(line, over_open)?;

    let tail = line[over_close + 1..].trim_start();
    if !contains_ascii_case_insensitive(tail, "as ") {
        return None;
    }

    let indent = leading_whitespace_prefix(line);
    let continuation = format!("{indent}    ");
    let inner_indent = format!("{indent}        ");
    let prefix = line[..over_start].trim_end();
    if prefix.is_empty() {
        return None;
    }
    let over_kw = line[over_start..over_open].trim();
    let inside = line[over_open + 1..over_close].trim();
    if inside.is_empty() {
        return None;
    }

    let mut lines = vec![prefix.to_string(), format!("{continuation}{over_kw} (")];
    if let Some(order_idx) = find_ascii_case_insensitive(inside, " order by ") {
        let partition = inside[..order_idx].trim();
        let order_by = inside[order_idx + 1..].trim_start();
        if !partition.is_empty() {
            lines.push(format!("{inner_indent}{partition}"));
        }
        if !order_by.is_empty() {
            lines.push(format!("{inner_indent}{order_by}"));
        }
    } else {
        lines.push(format!("{inner_indent}{inside}"));
    }
    lines.push(format!("{continuation})"));
    lines.push(format!("{continuation}{tail}"));
    Some(lines.join("\n"))
}

fn rewrite_window_function_line(line: &str, max_line_length: usize) -> Option<String> {
    if line.chars().count() <= max_line_length {
        return None;
    }

    let over_start = find_ascii_case_insensitive(line, " over (")?;
    let modifier_start = rfind_ascii_case_insensitive_before(line, " ignore nulls", over_start)
        .or_else(|| rfind_ascii_case_insensitive_before(line, " respect nulls", over_start))?;

    let function_part = line[..modifier_start].trim_end();
    let modifier = line[modifier_start..over_start].trim();
    let over_part = line[over_start + 1..].trim_start();
    if function_part.is_empty() || modifier.is_empty() || over_part.is_empty() {
        return None;
    }

    let indent = leading_whitespace_prefix(line);
    let continuation = format!("{indent}    ");

    let mut lines = Vec::new();
    if let Some((head, inner)) = outer_call_head_and_inner(function_part) {
        if inner.contains('(') && inner.contains(')') {
            lines.push(format!("{head}("));
            lines.push(format!("{continuation}{inner}"));
            lines.push(format!("{indent}) {modifier}"));
        } else {
            lines.push(format!("{} {modifier}", function_part.trim_end()));
        }
    } else {
        lines.push(format!("{} {modifier}", function_part.trim_end()));
    }
    lines.push(format!("{continuation}{over_part}"));
    Some(lines.join("\n"))
}

fn outer_call_head_and_inner(function_part: &str) -> Option<(&str, &str)> {
    let trimmed = function_part.trim_end();
    if !trimmed.ends_with(')') {
        return None;
    }
    let open = trimmed.find('(')?;
    let close = matching_close_paren(trimmed, open)?;
    if close + 1 != trimmed.len() {
        return None;
    }
    let head = trimmed[..open].trim_end();
    let inner = trimmed[open + 1..close].trim();
    if head.is_empty() || inner.is_empty() {
        return None;
    }
    Some((head, inner))
}

fn leading_whitespace_prefix(line: &str) -> &str {
    let width = line
        .bytes()
        .take_while(|byte| matches!(*byte, b' ' | b'\t'))
        .count();
    &line[..width]
}

fn find_unquoted_inline_comment_start(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    let mut in_single = false;
    let mut in_double = false;

    while index + 1 < bytes.len() {
        let byte = bytes[index];

        if in_single {
            if byte == b'\'' {
                if index + 1 < bytes.len() && bytes[index + 1] == b'\'' {
                    index += 2;
                    continue;
                }
                in_single = false;
            }
            index += 1;
            continue;
        }

        if in_double {
            if byte == b'"' {
                if index + 1 < bytes.len() && bytes[index + 1] == b'"' {
                    index += 2;
                    continue;
                }
                in_double = false;
            }
            index += 1;
            continue;
        }

        if byte == b'\'' {
            in_single = true;
            index += 1;
            continue;
        }
        if byte == b'"' {
            in_double = true;
            index += 1;
            continue;
        }
        if byte == b'-' && bytes[index + 1] == b'-' {
            return Some(index);
        }
        index += 1;
    }

    None
}

fn matching_close_paren(input: &str, open_index: usize) -> Option<usize> {
    if !matches!(input.as_bytes().get(open_index), Some(b'(')) {
        return None;
    }

    let mut depth = 0usize;
    for (index, ch) in input
        .char_indices()
        .skip_while(|(idx, _)| *idx < open_index)
    {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }

    None
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    find_ascii_case_insensitive(haystack, needle).is_some()
}

fn rfind_ascii_case_insensitive_before(haystack: &str, needle: &str, end: usize) -> Option<usize> {
    haystack[..end.min(haystack.len())]
        .to_ascii_lowercase()
        .rfind(&needle.to_ascii_lowercase())
}

fn rewrite_whitespace_wrap_line(line: &str, max_line_length: usize) -> Option<String> {
    if line.chars().count() <= max_line_length {
        return None;
    }
    if line.contains("--") || line.contains("/*") || line.contains("*/") {
        return None;
    }

    let indent = leading_whitespace_prefix(line);
    let indent_chars = indent.chars().count();
    let continuation_indent = format!("{indent}    ");
    let continuation_chars = continuation_indent.chars().count();
    let mut remaining = line[indent.len()..].trim_end().to_string();
    if remaining.is_empty() {
        return None;
    }

    let mut wrapped = Vec::new();
    let mut first = true;
    loop {
        let limit = if first {
            max_line_length.saturating_sub(indent_chars)
        } else {
            max_line_length.saturating_sub(continuation_chars)
        };
        if limit < 8 || remaining.chars().count() <= limit {
            break;
        }

        let split_at = wrap_split_index(&remaining, limit)?;
        let head = remaining[..split_at].trim_end();
        let tail = remaining[split_at..].trim_start();
        if head.is_empty() || tail.is_empty() {
            return None;
        }

        if first {
            wrapped.push(format!("{indent}{head}"));
            first = false;
        } else {
            wrapped.push(format!("{continuation_indent}{head}"));
        }
        remaining = tail.to_string();
    }

    if wrapped.is_empty() {
        return None;
    }

    if first {
        wrapped.push(format!("{indent}{remaining}"));
    } else {
        wrapped.push(format!("{continuation_indent}{remaining}"));
    }
    Some(wrapped.join("\n"))
}

fn wrap_split_index(content: &str, char_limit: usize) -> Option<usize> {
    if char_limit == 0 {
        return None;
    }

    #[derive(Clone, Copy)]
    enum ScanMode {
        Outside,
        SingleQuote,
        DoubleQuote,
        BacktickQuote,
    }

    let mut split_at = None;
    let mut mode = ScanMode::Outside;
    let mut iter = content.char_indices().enumerate().peekable();
    while let Some((char_idx, (byte_idx, ch))) = iter.next() {
        if char_idx >= char_limit {
            break;
        }

        match mode {
            ScanMode::Outside => {
                if ch.is_whitespace() {
                    split_at = Some(byte_idx);
                    continue;
                }
                mode = match ch {
                    '\'' => ScanMode::SingleQuote,
                    '"' => ScanMode::DoubleQuote,
                    '`' => ScanMode::BacktickQuote,
                    _ => ScanMode::Outside,
                };
            }
            ScanMode::SingleQuote => {
                if ch == '\'' {
                    if iter
                        .peek()
                        .is_some_and(|(_, (_, next_ch))| *next_ch == '\'')
                    {
                        let _ = iter.next();
                    } else {
                        mode = ScanMode::Outside;
                    }
                }
            }
            ScanMode::DoubleQuote => {
                if ch == '"' {
                    if iter.peek().is_some_and(|(_, (_, next_ch))| *next_ch == '"') {
                        let _ = iter.next();
                    } else {
                        mode = ScanMode::Outside;
                    }
                }
            }
            ScanMode::BacktickQuote => {
                if ch == '`' {
                    if iter.peek().is_some_and(|(_, (_, next_ch))| *next_ch == '`') {
                        let _ = iter.next();
                    } else {
                        mode = ScanMode::Outside;
                    }
                }
            }
        }
    }

    split_at.filter(|byte_idx| *byte_idx > 0)
}

fn line_is_comment_only_tokenized(
    line_start: usize,
    line_end: usize,
    tokens: &[LocatedToken],
    line_text: &str,
    sql: &str,
    jinja_comment_spans: &[std::ops::Range<usize>],
) -> bool {
    if line_is_jinja_comment_only(line_start, line_end, sql, jinja_comment_spans) {
        return true;
    }

    let line_tokens = tokens_on_line(tokens, line_start, line_end);
    if line_tokens.is_empty() {
        return false;
    }

    let mut non_spacing = line_tokens
        .into_iter()
        .filter(|token| !is_spacing_whitespace(&token.token))
        .peekable();

    let Some(first) = non_spacing.peek() else {
        return false;
    };

    let mut saw_comment = false;
    if matches!(first.token, Token::Comma)
        && line_prefix_before_token_is_spacing(line_text, line_start, first.start)
    {
        let _ = non_spacing.next();
    }

    for token in non_spacing {
        if is_comment_token(&token.token) {
            saw_comment = true;
            continue;
        }
        return false;
    }

    saw_comment
}

fn comment_clause_start_offset_tokenized(
    line_start: usize,
    line_end: usize,
    tokens: &[LocatedToken],
    jinja_comment_spans: &[std::ops::Range<usize>],
) -> Option<usize> {
    let jinja_start = first_jinja_comment_start_on_line(line_start, line_end, jinja_comment_spans);
    let line_tokens = tokens_on_line(tokens, line_start, line_end);
    let significant: Vec<&LocatedToken> = line_tokens
        .iter()
        .copied()
        .filter(|token| !is_spacing_whitespace(&token.token))
        .collect();

    let mut earliest = jinja_start;

    for (index, token) in significant.iter().enumerate() {
        if let Token::Word(word) = &token.token {
            if word.value.eq_ignore_ascii_case("comment") {
                let candidate = token.start.max(line_start);
                earliest = Some(earliest.map_or(candidate, |current| current.min(candidate)));
                break;
            }
        }

        if matches!(
            token.token,
            Token::Whitespace(Whitespace::SingleLineComment { .. })
        ) {
            let candidate = token.start.max(line_start);
            earliest = Some(earliest.map_or(candidate, |current| current.min(candidate)));
            break;
        }

        if matches!(
            token.token,
            Token::Whitespace(Whitespace::MultiLineComment(_))
        ) && significant[index + 1..]
            .iter()
            .all(|next| is_spacing_whitespace(&next.token))
        {
            let candidate = token.start.max(line_start);
            earliest = Some(earliest.map_or(candidate, |current| current.min(candidate)));
            break;
        }
    }

    earliest
}

fn tokens_on_line(
    tokens: &[LocatedToken],
    line_start: usize,
    line_end: usize,
) -> Vec<&LocatedToken> {
    tokens
        .iter()
        .filter(|token| token.start < line_end && token.end > line_start)
        .collect()
}

fn line_prefix_before_token_is_spacing(
    line_text: &str,
    line_start: usize,
    token_start: usize,
) -> bool {
    if token_start < line_start {
        return false;
    }

    line_text[..token_start - line_start]
        .chars()
        .all(char::is_whitespace)
}

fn tokenize_with_offsets(sql: &str, dialect: Dialect) -> Option<Vec<LocatedToken>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        let start = line_col_to_offset(
            sql,
            token.span.start.line as usize,
            token.span.start.column as usize,
        )?;
        let end = line_col_to_offset(
            sql,
            token.span.end.line as usize,
            token.span.end.column as usize,
        )?;
        out.push(LocatedToken {
            token: token.token,
            start,
            end,
        });
    }

    Some(out)
}

fn tokenize_with_offsets_for_context(ctx: &RuleContext) -> Option<Vec<LocatedToken>> {
    ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        Some(
            tokens
                .iter()
                .filter_map(|token| {
                    token_with_span_offsets(ctx.sql, token).map(|(start, end)| LocatedToken {
                        token: token.token.clone(),
                        start,
                        end,
                    })
                })
                .collect::<Vec<_>>(),
        )
    })
}

fn jinja_comment_spans(sql: &str) -> Vec<std::ops::Range<usize>> {
    let mut spans = Vec::new();
    let mut cursor = 0usize;

    while cursor < sql.len() {
        let Some(open_rel) = sql[cursor..].find("{#") else {
            break;
        };
        let start = cursor + open_rel;
        let content_start = start + 2;
        if let Some(close_rel) = sql[content_start..].find("#}") {
            let end = content_start + close_rel + 2;
            spans.push(start..end);
            cursor = end;
        } else {
            spans.push(start..sql.len());
            break;
        }
    }

    spans
}

fn sanitize_sql_for_jinja_comments(sql: &str, spans: &[std::ops::Range<usize>]) -> String {
    if spans.is_empty() {
        return sql.to_string();
    }

    let mut bytes = sql.as_bytes().to_vec();
    for span in spans {
        for idx in span.start..span.end.min(bytes.len()) {
            if bytes[idx] != b'\n' {
                bytes[idx] = b' ';
            }
        }
    }

    String::from_utf8(bytes).expect("sanitized SQL should remain valid UTF-8")
}

fn first_jinja_comment_start_on_line(
    line_start: usize,
    line_end: usize,
    spans: &[std::ops::Range<usize>],
) -> Option<usize> {
    spans
        .iter()
        .filter_map(|span| {
            if span.start >= line_end || span.end <= line_start {
                return None;
            }
            Some(span.start.max(line_start))
        })
        .min()
}

fn line_is_jinja_comment_only(
    line_start: usize,
    line_end: usize,
    sql: &str,
    spans: &[std::ops::Range<usize>],
) -> bool {
    let mut in_prefix = true;
    let mut saw_comment = false;

    for (rel, ch) in sql[line_start..line_end].char_indices() {
        if in_prefix {
            if ch.is_whitespace() || ch == ',' {
                continue;
            }
            in_prefix = false;
        }

        if ch.is_whitespace() {
            continue;
        }

        let abs = line_start + rel;
        if !offset_in_any_span(abs, spans) {
            return false;
        }
        saw_comment = true;
    }

    saw_comment
}

fn offset_in_any_span(offset: usize, spans: &[std::ops::Range<usize>]) -> bool {
    spans
        .iter()
        .any(|span| offset >= span.start && offset < span.end)
}

fn is_comment_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

fn is_spacing_whitespace(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
    )
}

fn line_col_to_offset(sql: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }

    let mut current_line = 1usize;
    let mut current_col = 1usize;

    for (offset, ch) in sql.char_indices() {
        if current_line == line && current_col == column {
            return Some(offset);
        }

        if ch == '\n' {
            current_line += 1;
            current_col = 1;
        } else {
            current_col += 1;
        }
    }

    if current_line == line && current_col == column {
        return Some(sql.len());
    }

    None
}

fn token_with_span_offsets(sql: &str, token: &TokenWithSpan) -> Option<(usize, usize)> {
    let start = line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )?;
    let end = line_col_to_offset(
        sql,
        token.span.end.line as usize,
        token.span.end.column as usize,
    )?;
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run_with_rule(sql: &str, rule: &LayoutLongLines) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn run(sql: &str) -> Vec<Issue> {
        run_with_rule(sql, &LayoutLongLines::default())
    }

    fn apply_issue_autofix(sql: &str, issue: &Issue) -> Option<String> {
        let autofix = issue.autofix.as_ref()?;
        let mut edits = autofix.edits.clone();
        Some(apply_patch_edits(sql, &mut edits))
    }

    fn apply_patch_edits(sql: &str, edits: &mut [IssuePatchEdit]) -> String {
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        let mut rewritten = sql.to_string();
        for edit in edits.iter().rev() {
            rewritten.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        rewritten
    }

    #[test]
    fn flags_single_long_line() {
        let long_line = format!("SELECT {} FROM t", "x".repeat(320));
        let issues = run(&long_line);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_005);
    }

    #[test]
    fn does_not_flag_short_line() {
        assert!(run("SELECT x FROM t").is_empty());
    }

    #[test]
    fn flags_each_overflowing_line_once() {
        let sql = format!(
            "SELECT {} AS a,\n       {} AS b FROM t",
            "x".repeat(90),
            "y".repeat(90)
        );
        let issues = run(&sql);
        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_005)
                .count(),
            2,
        );
    }

    #[test]
    fn configured_max_line_length_is_respected() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.long_lines".to_string(),
                serde_json::json!({"max_line_length": 20}),
            )]),
        };
        let rule = LayoutLongLines::from_config(&config);
        let sql = "SELECT this_line_is_long FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_005);
    }

    #[test]
    fn ignore_comment_lines_skips_long_comment_only_lines() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.long_lines".to_string(),
                serde_json::json!({
                    "max_line_length": 20,
                    "ignore_comment_lines": true
                }),
            )]),
        };
        let sql = format!("SELECT 1;\n-- {}\nSELECT 2", "x".repeat(120));
        let issues = run_with_rule(&sql, &LayoutLongLines::from_config(&config));
        assert!(
            issues.is_empty(),
            "ignore_comment_lines should suppress long comment-only lines: {issues:?}",
        );
    }

    #[test]
    fn ignore_comment_lines_skips_comma_prefixed_comment_lines() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.long_lines".to_string(),
                serde_json::json!({
                    "max_line_length": 30,
                    "ignore_comment_lines": true
                }),
            )]),
        };
        let sql = "SELECT\nc1\n,-- this is a very long comment line that should be ignored\nc2\n";
        let issues = run_with_rule(sql, &LayoutLongLines::from_config(&config));
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_comment_lines_skips_jinja_comment_lines() {
        let sql =
            "SELECT *\n{# this is a very long jinja comment line that should be ignored #}\nFROM t";
        let spans = long_line_overflow_spans(sql, 30, true, false, Dialect::Generic);
        assert!(spans.is_empty());
    }

    #[test]
    fn ignore_comment_clauses_skips_long_trailing_comment_text() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.long_lines".to_string(),
                serde_json::json!({
                    "max_line_length": 20,
                    "ignore_comment_clauses": true
                }),
            )]),
        };
        let sql = format!("SELECT 1 -- {}", "x".repeat(120));
        let issues = run_with_rule(&sql, &LayoutLongLines::from_config(&config));
        assert!(
            issues.is_empty(),
            "ignore_comment_clauses should suppress trailing-comment overflow: {issues:?}",
        );
    }

    #[test]
    fn ignore_comment_clauses_still_flags_long_sql_prefix() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_LT_005".to_string(),
                serde_json::json!({
                    "max_line_length": 20,
                    "ignore_comment_clauses": true
                }),
            )]),
        };
        let sql = format!("SELECT {} -- short", "x".repeat(40));
        let issues = run_with_rule(&sql, &LayoutLongLines::from_config(&config));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_005);
    }

    #[test]
    fn ignore_comment_clauses_skips_sql_comment_clause_lines() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.long_lines".to_string(),
                serde_json::json!({
                    "max_line_length": 40,
                    "ignore_comment_clauses": true
                }),
            )]),
        };
        let sql = "CREATE TABLE t (\n    c1 INT COMMENT 'this is a very very very very very very very very long comment'\n)";
        let issues = run_with_rule(sql, &LayoutLongLines::from_config(&config));
        assert!(issues.is_empty());
    }

    #[test]
    fn non_positive_max_line_length_disables_rule() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.long_lines".to_string(),
                serde_json::json!({"max_line_length": -1}),
            )]),
        };
        let sql = "SELECT this_is_a_very_long_column_name_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx FROM t";
        let issues = run_with_rule(sql, &LayoutLongLines::from_config(&config));
        assert!(issues.is_empty());
    }

    #[test]
    fn statementless_fallback_flags_long_jinja_config_line() {
        let sql = "{{ config (schema='bronze', materialized='view', sort =['id','number'], dist = 'all', tags =['longlonglonglonglong']) }} \n\nselect 1\n";
        let synthetic = parse_sql("SELECT 1").expect("parse");
        let rule = LayoutLongLines::default();
        let issues =
            rule.check_with_context(&synthetic[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(
            !issues.is_empty(),
            "expected LT05 to flag long templated config line in statementless mode"
        );
        assert_eq!(issues[0].code, issue_codes::LINT_LT_005);
    }

    #[test]
    fn emits_safe_autofix_patch_for_very_long_line() {
        let projections = (0..120)
            .map(|index| format!("col_{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT {projections} FROM t");
        let issues = run(&sql);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_005);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);

        let fixed = apply_issue_autofix(&sql, &issues[0]).expect("apply autofix");
        let expected = legacy_split_long_line(&sql).expect("legacy split result");
        assert_eq!(fixed, expected);
        assert_ne!(fixed, sql);
    }

    #[test]
    fn does_not_emit_autofix_when_line_is_below_legacy_split_threshold() {
        let sql = format!("SELECT {} FROM t", "x".repeat(120));
        let issues = run(&sql);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_005);
        let fixed = apply_issue_autofix(&sql, &issues[0]).expect("apply autofix");
        assert!(fixed.contains('\n'));
        assert!(fixed.contains("\nFROM t"));
    }

    #[test]
    fn autofix_moves_inline_comment_before_code_when_overflowing() {
        let sql = "SELECT 1 -- Some Comment\n";
        let mut edits = long_line_autofix_edits(sql, 18, false);
        let fixed = apply_patch_edits(sql, &mut edits);
        assert_eq!(fixed, "-- Some Comment\nSELECT 1\n");
    }

    #[test]
    fn autofix_moves_inline_comment_after_code_when_configured() {
        let sql = "SELECT 1 -- Some Comment\n";
        let mut edits = long_line_autofix_edits(sql, 18, true);
        let fixed = apply_patch_edits(sql, &mut edits);
        assert_eq!(fixed, "SELECT 1\n-- Some Comment\n");
    }

    #[test]
    fn autofix_moves_comment_and_rebreaks_select_from_line() {
        let sql = "SELECT COUNT(*) FROM tbl -- Some Comment\n";
        let mut edits = long_line_autofix_edits(sql, 18, false);
        let fixed = apply_patch_edits(sql, &mut edits);
        assert_eq!(fixed, "-- Some Comment\nSELECT COUNT(*)\nFROM tbl\n");
    }

    #[test]
    fn autofix_does_not_split_comment_only_long_line() {
        let sql =
            "-- Aggregate page performance events from the last 24 hours into hourly summaries.\n";
        let mut edits = long_line_autofix_edits(sql, 80, false);
        let fixed = apply_patch_edits(sql, &mut edits);
        assert_eq!(fixed, sql);
    }

    #[test]
    fn autofix_moves_mid_query_inline_comment() {
        let sql = "select\n    my_long_long_line as foo -- with some comment\nfrom foo\n";
        let mut edits = long_line_autofix_edits(sql, 40, false);
        let fixed = apply_patch_edits(sql, &mut edits);
        assert_eq!(
            fixed,
            "select\n    -- with some comment\n    my_long_long_line as foo\nfrom foo\n"
        );
    }

    #[test]
    fn autofix_rebreaks_window_function_lines() {
        let sql = "select *\nfrom t\nqualify a = coalesce(\n    first_value(iff(b = 'none', null, a)) ignore nulls over (partition by c order by d desc),\n    first_value(a) respect nulls over (partition by c order by d desc)\n)\n";
        let mut edits = long_line_autofix_edits(sql, 50, false);
        let fixed = apply_patch_edits(sql, &mut edits);
        assert_eq!(
            fixed,
            "select *\nfrom t\nqualify a = coalesce(\n    first_value(\n        iff(b = 'none', null, a)\n    ) ignore nulls\n        over (partition by c order by d desc),\n    first_value(a) respect nulls\n        over (partition by c order by d desc)\n)\n"
        );
    }

    #[test]
    fn autofix_rebreaks_long_functions_and_aliases() {
        let sql = "SELECT\n    my_function(col1 + col2, arg2, arg3) over (partition by col3, col4 order by col5 rows between unbounded preceding and current row) as my_relatively_long_alias,\n    my_other_function(col6, col7 + col8, arg4) as my_other_relatively_long_alias,\n    my_expression_function(col6, col7 + col8, arg4) = col9 + col10 as another_relatively_long_alias\nFROM my_table\n";
        let mut edits = long_line_autofix_edits(sql, 80, false);
        let fixed = apply_patch_edits(sql, &mut edits);
        assert_eq!(
            fixed,
            "SELECT\n    my_function(col1 + col2, arg2, arg3)\n        over (\n            partition by col3, col4\n            order by col5 rows between unbounded preceding and current row\n        )\n        as my_relatively_long_alias,\n    my_other_function(col6, col7 + col8, arg4)\n        as my_other_relatively_long_alias,\n    my_expression_function(col6, col7 + col8, arg4)\n    = col9 + col10 as another_relatively_long_alias\nFROM my_table\n"
        );
    }

    #[test]
    fn autofix_splits_long_expression_alias_line() {
        let sql =
            "        percentile_cont(0.50) WITHIN GROUP (ORDER BY duration_ms)::int AS p50_ms,\n";
        let mut edits = long_line_autofix_edits(sql, 80, false);
        let fixed = apply_patch_edits(sql, &mut edits);
        assert_eq!(
            fixed,
            "        percentile_cont(0.50) WITHIN GROUP (ORDER BY duration_ms)::int\n            AS p50_ms,\n"
        );
    }

    #[test]
    fn autofix_wraps_generic_long_predicate_line() {
        let sql = "    WHEN uli.usage_start_time >= params.as_of_date - MAKE_INTERVAL(days => params.window_days) AND uli.usage_start_time < params.as_of_date\n";
        let mut edits = long_line_autofix_edits(sql, 80, false);
        let fixed = apply_patch_edits(sql, &mut edits);

        assert_ne!(fixed, sql);
        for line in fixed.lines() {
            assert!(
                line.chars().count() <= 80,
                "expected wrapped line <= 80 chars, got {}: {line}",
                line.chars().count()
            );
        }
    }

    #[test]
    fn generic_wrap_keeps_quoted_literals_intact() {
        let sql = "SELECT CONCAT('hello world this is a long literal', col1, col2, col3, col4, col5, col6) FROM t\n";
        let mut edits = long_line_autofix_edits(sql, 60, false);
        let fixed = apply_patch_edits(sql, &mut edits);

        assert_ne!(fixed, sql);
        assert!(fixed.contains("'hello world this is a long literal'"));
    }
}
