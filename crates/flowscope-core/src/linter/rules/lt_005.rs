//! LINT_LT_005: Layout long lines.
//!
//! SQLFluff LT05 parity (current scope): flag overflow beyond 80 columns.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue, Span};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct LayoutLongLines {
    max_line_length: Option<usize>,
    ignore_comment_lines: bool,
    ignore_comment_clauses: bool,
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
        }
    }
}

impl Default for LayoutLongLines {
    fn default() -> Self {
        Self {
            max_line_length: Some(80),
            ignore_comment_lines: false,
            ignore_comment_clauses: false,
        }
    }
}

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

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let Some(max_line_length) = self.max_line_length else {
            return Vec::new();
        };

        if ctx.statement_index != 0 {
            return Vec::new();
        }

        long_line_overflow_spans_for_context(
            ctx,
            max_line_length,
            self.ignore_comment_lines,
            self.ignore_comment_clauses,
        )
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

fn long_line_overflow_spans_for_context(
    ctx: &LintContext,
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
    long_line_overflow_spans_tokenized(
        sql,
        max_len,
        ignore_comment_lines,
        ignore_comment_clauses,
        dialect,
    )
    .unwrap_or_default()
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
                &tokens,
                line,
                sql,
                &jinja_comment_spans,
            )
        {
            continue;
        }

        let effective_end = if ignore_comment_clauses {
            comment_clause_start_offset_tokenized(
                line_start,
                line_end,
                &tokens,
                &jinja_comment_spans,
            )
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

fn tokenize_with_offsets_for_context(ctx: &LintContext) -> Option<Vec<LocatedToken>> {
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

    fn run_with_rule(sql: &str, rule: &LayoutLongLines) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
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

    fn run(sql: &str) -> Vec<Issue> {
        run_with_rule(sql, &LayoutLongLines::default())
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
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
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
}
