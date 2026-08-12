//! LINT_JJ_001: Jinja padding.
//!
//! SQLFluff JJ01 parity (current scope): detect inconsistent whitespace around
//! Jinja delimiters.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

pub struct JinjaPadding;

impl BuiltinLintRule for JinjaPadding {
    fn code(&self) -> &'static str {
        issue_codes::LINT_JJ_001
    }

    fn name(&self) -> &'static str {
        "Jinja padding"
    }

    fn description(&self) -> &'static str {
        "Jinja tags should have a single whitespace on either side."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let Some((start, end)) = jinja_padding_violation_span(ctx) else {
            return Vec::new();
        };

        let mut issue = Issue::info(
            issue_codes::LINT_JJ_001,
            "Jinja tag spacing appears inconsistent.",
        )
        .with_statement(ctx.statement_index)
        .with_span(ctx.span_from_statement_offset(start, end));

        let edits: Vec<IssuePatchEdit> = jinja_padding_autofix_edits(ctx.statement_sql())
            .into_iter()
            .map(|edit| {
                IssuePatchEdit::new(
                    ctx.span_from_statement_offset(edit.start, edit.end),
                    edit.replacement,
                )
            })
            .collect();
        if !edits.is_empty() {
            issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, edits);
        }

        vec![issue]
    }
}

fn jinja_padding_violation_span(ctx: &RuleContext) -> Option<(usize, usize)> {
    let sql = ctx.statement_sql();

    // Token-based detection (works well when sqlparser can tokenize the input).
    if let Some(tokens) = token_spans_for_context(ctx).or_else(|| token_spans(sql, ctx.dialect())) {
        for token in &tokens {
            if let Some(span) = token_text_violation(sql, token) {
                return Some(span);
            }
        }

        for pair in tokens.windows(2) {
            let left = &pair[0];
            let right = &pair[1];
            if is_open_delimiter_tokens(&left.token, &right.token) {
                let delimiter_start = left.start;
                let delimiter_end = right.end;
                if has_incorrect_padding_after(sql, delimiter_end) {
                    return Some((delimiter_start, delimiter_end));
                }
            }

            if is_close_delimiter_tokens(&left.token, &right.token) {
                let delimiter_start = left.start;
                let delimiter_end = right.end;
                if has_incorrect_padding_before(sql, delimiter_start) {
                    return Some((delimiter_start, delimiter_end));
                }
            }
        }
    }

    // Text-based fallback: check whether the autofix engine would produce any
    // edits. This catches multi-space violations and cases where the token-based
    // detection misses Jinja delimiters (e.g. when sqlparser splits them across
    // tokens differently than expected).
    let edits = jinja_padding_autofix_edits(sql);
    if let Some(edit) = edits.first() {
        return Some((edit.start, edit.end));
    }

    None
}

struct TokenSpan {
    token: Token,
    start: usize,
    end: usize,
}

fn token_spans(sql: &str, dialect: Dialect) -> Option<Vec<TokenSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens: Vec<TokenWithSpan> = tokenizer.tokenize_with_location().ok()?;

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
        if start < end {
            out.push(TokenSpan {
                token: token.token,
                start,
                end,
            });
        }
    }

    Some(out)
}

fn token_spans_for_context(ctx: &RuleContext) -> Option<Vec<TokenSpan>> {
    let offset = ctx.statement_range.start;
    ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut out = Vec::new();
        for token in tokens {
            let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                continue;
            };
            if start < ctx.statement_range.start || end > ctx.statement_range.end {
                continue;
            }
            if start < end {
                out.push(TokenSpan {
                    token: token.token.clone(),
                    start: start - offset,
                    end: end - offset,
                });
            }
        }

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    })
}

fn token_text_violation(sql: &str, token: &TokenSpan) -> Option<(usize, usize)> {
    let text = &sql[token.start..token.end];

    for pattern in &OPEN_DELIMITERS {
        for (idx, _) in text.match_indices(pattern) {
            let delimiter_start = token.start + idx;
            let delimiter_end = delimiter_start + pattern.len();
            if has_incorrect_padding_after(sql, delimiter_end) {
                return Some((delimiter_start, delimiter_end));
            }
        }
    }

    for pattern in &CLOSE_DELIMITERS {
        for (idx, _) in text.match_indices(pattern) {
            let delimiter_start = token.start + idx;
            if has_incorrect_padding_before(sql, delimiter_start) {
                return Some((delimiter_start, delimiter_start + pattern.len()));
            }
        }
    }

    None
}

#[derive(Debug)]
struct JinjaPaddingEdit {
    start: usize,
    end: usize,
    replacement: String,
}

fn jinja_padding_autofix_edits(sql: &str) -> Vec<JinjaPaddingEdit> {
    let mut edits =
        normalize_template_tag_padding_edits(sql, b"{{", b"}}", |b| b != b'{' && b != b'}');
    edits.extend(normalize_template_tag_padding_edits(
        sql,
        b"{%",
        b"%}",
        |b| b != b'%',
    ));
    edits.sort_by_key(|edit| (edit.start, edit.end));
    edits.dedup_by(|left, right| {
        left.start == right.start && left.end == right.end && left.replacement == right.replacement
    });
    edits
}

fn normalize_template_tag_padding_edits<F>(
    sql: &str,
    open: &[u8],
    close: &[u8],
    inner_ok: F,
) -> Vec<JinjaPaddingEdit>
where
    F: Fn(u8) -> bool,
{
    let bytes = sql.as_bytes();
    let mut edits = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let mut replaced = false;
        if i + open.len() <= bytes.len() && &bytes[i..i + open.len()] == open {
            let mut j = i + open.len();
            while j + close.len() <= bytes.len() {
                if &bytes[j..j + close.len()] == close {
                    let inner = &sql[i + open.len()..j];
                    if !inner.is_empty() && inner.as_bytes().iter().copied().all(&inner_ok) {
                        let open_text =
                            std::str::from_utf8(open).expect("template delimiter is ascii");
                        let close_text =
                            std::str::from_utf8(close).expect("template delimiter is ascii");

                        // Detect trim markers (+/-) attached to delimiters.
                        let trimmed = inner.trim();
                        let (open_marker, content, close_marker) = extract_trim_markers(trimmed);
                        let content = content.trim();

                        let replacement = format!(
                            "{open_text}{open_marker} {content} {close_marker}{close_text}"
                        );
                        let end = j + close.len();
                        if replacement != sql[i..end] {
                            edits.push(JinjaPaddingEdit {
                                start: i,
                                end,
                                replacement,
                            });
                        }
                        i = end;
                        replaced = true;
                    }
                    break;
                }
                j += 1;
            }
            if replaced {
                continue;
            }
        }

        i += 1;
    }

    edits
}

/// Extracts optional trim markers from Jinja tag content.
/// `+` and `-` at the start/end of content are trim markers.
/// Returns (open_marker, remaining_content, close_marker).
fn extract_trim_markers(content: &str) -> (&str, &str, &str) {
    let bytes = content.as_bytes();
    let mut start = 0;
    let mut end = bytes.len();

    let open_marker = if !bytes.is_empty() && (bytes[0] == b'+' || bytes[0] == b'-') {
        start = 1;
        &content[..1]
    } else {
        ""
    };

    let close_marker = if end > start && (bytes[end - 1] == b'+' || bytes[end - 1] == b'-') {
        end -= 1;
        &content[end..end + 1]
    } else {
        ""
    };

    (open_marker, &content[start..end], close_marker)
}

const OPEN_DELIMITERS: [&str; 3] = ["{{", "{%", "{#"];
const CLOSE_DELIMITERS: [&str; 3] = ["}}", "%}", "#}"];

fn is_open_delimiter_tokens(left: &Token, right: &Token) -> bool {
    matches!(
        (left, right),
        (Token::LBrace, Token::LBrace)
            | (Token::LBrace, Token::Mod)
            | (Token::LBrace, Token::Sharp)
    )
}

fn is_close_delimiter_tokens(left: &Token, right: &Token) -> bool {
    matches!(
        (left, right),
        (Token::RBrace, Token::RBrace)
            | (Token::Mod, Token::RBrace)
            | (Token::Sharp, Token::RBrace)
    )
}

fn has_incorrect_padding_after(sql: &str, delimiter_end: usize) -> bool {
    let remainder = match sql.get(delimiter_end..) {
        Some(r) => r,
        None => return true,
    };
    let mut chars = remainder.chars();
    let first = match chars.next() {
        Some(ch) => ch,
        None => return true,
    };

    // After a trim marker (+/-), require a space before content.
    if is_trim_marker(first) {
        return !matches!(chars.next(), Some(' '));
    }

    if first != ' ' {
        return true; // missing padding entirely
    }

    // Check for excess whitespace (more than one space).
    let spaces = 1 + chars.take_while(|ch| *ch == ' ').count();
    spaces > 1
}

fn has_incorrect_padding_before(sql: &str, delimiter_start: usize) -> bool {
    if delimiter_start == 0 {
        return true;
    }
    let mut rchars = sql[..delimiter_start].chars().rev();
    let prev = match rchars.next() {
        Some(ch) => ch,
        None => return true,
    };

    // Before a trim marker (+/-), require a space before the marker.
    if is_trim_marker(prev) {
        return !matches!(rchars.next(), Some(' '));
    }

    if prev != ' ' {
        return true; // missing padding entirely
    }

    // Check for excess whitespace (more than one space).
    let spaces = 1 + rchars.take_while(|ch| *ch == ' ').count();
    spaces > 1
}

fn is_trim_marker(ch: char) -> bool {
    ch == '-' || ch == '+'
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

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = JinjaPadding;
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn apply_issue_autofix(sql: &str, issue: &Issue) -> Option<String> {
        let autofix = issue.autofix.as_ref()?;
        let mut out = sql.to_string();
        let mut edits = autofix.edits.clone();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        for edit in edits.iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    #[test]
    fn flags_missing_padding_in_jinja_expression() {
        let sql = "SELECT '{{foo}}' AS templated";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(
            issues[0].span.expect("expected span").start,
            sql.find("{{").expect("expected opening delimiter"),
        );
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT '{{ foo }}' AS templated");
    }

    #[test]
    fn does_not_flag_padded_jinja_expression() {
        assert!(run("SELECT '{{ foo }}' AS templated").is_empty());
    }

    #[test]
    fn flags_missing_padding_in_jinja_statement_tag() {
        let sql = "SELECT '{%for x in y %}' AS templated";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT '{% for x in y %}' AS templated");
    }

    #[test]
    fn flags_missing_padding_before_statement_close_tag() {
        let sql = "SELECT '{% for x in y%}' AS templated";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT '{% for x in y %}' AS templated");
    }

    #[test]
    fn flags_missing_padding_in_jinja_comment_tag() {
        let issues = run("SELECT '{#comment#}' AS templated");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
        assert!(
            issues[0].autofix.is_none(),
            "comment-tag JJ001 findings are report-only in current core autofix scope"
        );
    }

    #[test]
    fn allows_jinja_trim_markers() {
        assert!(run("SELECT '{{- foo -}}' AS templated").is_empty());
        assert!(run("SELECT '{%- if x -%}' AS templated").is_empty());
        assert!(run("SELECT '{{+ foo +}}' AS templated").is_empty());
        assert!(run("SELECT '{%+ if x -%}' AS templated").is_empty());
    }

    #[test]
    fn allows_raw_jinja_with_trim_markers_and_correct_spacing() {
        // SQLFluff: test_simple_modified — should pass
        assert!(detect("SELECT 1 from {%+ if true -%} foo {%- endif %}\n").is_none());
    }

    fn detect(sql: &str) -> Option<(usize, usize)> {
        jinja_padding_violation_span(&RuleContext::new(sql, 0..sql.len(), 0))
    }

    #[test]
    fn flags_raw_jinja_expression_no_space() {
        // SQLFluff: test_fail_jinja_tags_no_space
        assert!(detect("SELECT 1 from {{ref('foo')}}\n").is_some());
    }

    #[test]
    fn flags_raw_jinja_expression_multiple_spaces() {
        // SQLFluff: test_fail_jinja_tags_multiple_spaces
        assert!(detect("SELECT 1 from {{      ref('foo')       }}\n").is_some());
    }

    #[test]
    fn flags_raw_jinja_expression_plus_trim_no_space() {
        // SQLFluff: test_fail_jinja_tags_no_space_2
        assert!(detect("SELECT 1 from {{+ref('foo')-}}\n").is_some());
    }

    #[test]
    fn flags_raw_jinja_no_content() {
        // SQLFluff: test_fail_jinja_tags_no_space_no_content
        assert!(detect("SELECT {{\"\"  -}}1\n").is_some());
    }
}
