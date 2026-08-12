//! LINT_LT_008: Layout CTE newline.
//!
//! SQLFluff LT08 parity (current scope): require a blank line between CTE body
//! closing parenthesis and following query/CTE text.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommaLinePosition {
    Leading,
    Trailing,
}

impl CommaLinePosition {
    fn from_config(config: &LintConfig) -> Self {
        let Some(layout_commas) = config.config_section_object("layout.commas") else {
            return Self::Trailing;
        };

        match layout_commas
            .get("line_position")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("trailing")
            .to_ascii_lowercase()
            .as_str()
        {
            "leading" => Self::Leading,
            _ => Self::Trailing,
        }
    }
}

pub struct LayoutCteNewline {
    comma_line_position: CommaLinePosition,
}

impl LayoutCteNewline {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            comma_line_position: CommaLinePosition::from_config(config),
        }
    }
}

impl Default for LayoutCteNewline {
    fn default() -> Self {
        Self {
            comma_line_position: CommaLinePosition::Trailing,
        }
    }
}

impl BuiltinLintRule for LayoutCteNewline {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_008
    }

    fn name(&self) -> &'static str {
        "Layout CTE newline"
    }

    fn description(&self) -> &'static str {
        "Blank line expected but not found after CTE closing bracket."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        lt08_violation_spans(statement, ctx, self.comma_line_position)
            .into_iter()
            .map(|((start, end), fix_span)| {
                let mut issue = Issue::info(
                    issue_codes::LINT_LT_008,
                    "Blank line expected but not found after CTE closing bracket.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end));
                if let Some((fix_start, fix_end)) = fix_span {
                    issue = issue.with_autofix_edits(
                        IssueAutofixApplicability::Safe,
                        vec![IssuePatchEdit::new(
                            ctx.span_from_statement_offset(fix_start, fix_end),
                            "\n\n",
                        )],
                    );
                }
                issue
            })
            .collect()
    }
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

type Lt08Span = (usize, usize);
type Lt08AutofixSpan = (usize, usize);
type Lt08Violation = (Lt08Span, Option<Lt08AutofixSpan>);

fn lt08_violation_spans(
    statement: &Statement,
    ctx: &RuleContext,
    comma_line_position: CommaLinePosition,
) -> Vec<Lt08Violation> {
    let Statement::Query(query) = statement else {
        return Vec::new();
    };
    let Some(with_clause) = &query.with else {
        return Vec::new();
    };

    let Some(tokens) = tokenize_with_offsets_for_context(ctx) else {
        return Vec::new();
    };

    let statement_start = ctx.statement_range.start;
    let mut spans = Vec::new();

    for cte in &with_clause.cte_tables {
        let Some(close_abs) = token_start_offset_for_context(ctx, &cte.closing_paren_token.0)
        else {
            continue;
        };

        if close_abs < ctx.statement_range.start || close_abs >= ctx.statement_range.end {
            continue;
        }

        let (blank_lines, next_code_span) =
            suffix_summary_after_offset(ctx.sql, &tokens, close_abs + 1, ctx.statement_range.end);

        if blank_lines == 0 {
            if let Some((next_start, next_end)) = next_code_span {
                let mut autofix_span = None;
                let gap_start = close_abs + 1;
                let next_token = &ctx.sql[next_start..next_end];

                if matches!(next_token, "," if matches!(comma_line_position, CommaLinePosition::Trailing))
                {
                    let comma_end = next_end;
                    let (_blank_lines_after_comma, next_after_comma) = suffix_summary_after_offset(
                        ctx.sql,
                        &tokens,
                        comma_end,
                        ctx.statement_range.end,
                    );
                    if let Some((after_comma_start, _after_comma_end)) = next_after_comma {
                        if let Some((fix_start, fix_end)) =
                            whitespace_gap_span(ctx.sql, comma_end, after_comma_start)
                        {
                            autofix_span =
                                Some((fix_start - statement_start, fix_end - statement_start));
                        } else if let Some(comment_start) =
                            first_comment_start_in_range(&tokens, comma_end, after_comma_start)
                        {
                            if let Some((fix_start, fix_end)) =
                                whitespace_gap_span(ctx.sql, comma_end, comment_start)
                            {
                                autofix_span =
                                    Some((fix_start - statement_start, fix_end - statement_start));
                            }
                        }
                    }
                } else if next_token.eq_ignore_ascii_case("SELECT")
                    || next_token.eq_ignore_ascii_case("INSERT")
                    || next_token.eq_ignore_ascii_case("UPDATE")
                    || next_token.eq_ignore_ascii_case("DELETE")
                {
                    if let Some((fix_start, fix_end)) =
                        whitespace_gap_span(ctx.sql, gap_start, next_start)
                    {
                        autofix_span =
                            Some((fix_start - statement_start, fix_end - statement_start));
                    }
                } else if matches!(comma_line_position, CommaLinePosition::Trailing) {
                    if let Some(after_comma) =
                        first_comma_end_in_range(&tokens, gap_start, next_start)
                    {
                        if let Some((fix_start, fix_end)) =
                            whitespace_gap_span(ctx.sql, after_comma, next_start)
                        {
                            autofix_span =
                                Some((fix_start - statement_start, fix_end - statement_start));
                        } else if let Some(comment_start) =
                            first_comment_start_in_range(&tokens, after_comma, next_start)
                        {
                            if let Some((fix_start, fix_end)) =
                                whitespace_gap_span(ctx.sql, after_comma, comment_start)
                            {
                                autofix_span =
                                    Some((fix_start - statement_start, fix_end - statement_start));
                            }
                        }
                    }
                } else if gap_start == next_start
                    && matches!(comma_line_position, CommaLinePosition::Leading)
                    && next_token == ","
                {
                    autofix_span = Some((gap_start - statement_start, gap_start - statement_start));
                }

                spans.push((
                    (next_start - statement_start, next_end - statement_start),
                    autofix_span,
                ));
            }
        }
    }

    if matches!(comma_line_position, CommaLinePosition::Leading) {
        if let Some(comma_abs) = find_oneline_leading_cte_comma(
            ctx.sql,
            ctx.statement_range.start,
            ctx.statement_range.end,
        ) {
            let relative = comma_abs - statement_start;
            let has_existing = spans
                .iter()
                .any(|((start, end), _)| *start <= relative && relative < *end);
            if !has_existing {
                spans.push(((relative, relative + 1), Some((relative, relative))));
            }
        }
    }

    spans
}

fn find_oneline_leading_cte_comma(
    sql: &str,
    statement_start: usize,
    statement_end: usize,
) -> Option<usize> {
    let statement_sql = &sql[statement_start..statement_end];
    if !statement_sql
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("with")
    {
        return None;
    }

    let bytes = statement_sql.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b',' {
            continue;
        }

        // Look backwards for `)` with only horizontal whitespace in between.
        let mut probe = index;
        while probe > 0 && matches!(bytes[probe - 1], b' ' | b'\t') {
            probe -= 1;
        }
        if probe == 0 || bytes[probe - 1] != b')' {
            continue;
        }
        if statement_sql[probe - 1..index].contains('\n')
            || statement_sql[probe - 1..index].contains('\r')
        {
            continue;
        }

        // Look forwards for `<cte_name> AS (` with only horizontal whitespace
        // around separators.
        let mut next = index + 1;
        while next < bytes.len() && matches!(bytes[next], b' ' | b'\t') {
            next += 1;
        }
        if next >= bytes.len() || !is_identifier_start(bytes[next]) {
            continue;
        }
        next += 1;
        while next < bytes.len() && is_identifier_part(bytes[next]) {
            next += 1;
        }

        let mut saw_space = false;
        while next < bytes.len() && matches!(bytes[next], b' ' | b'\t') {
            saw_space = true;
            next += 1;
        }
        if !saw_space {
            continue;
        }
        if !starts_with_ascii_case_insensitive(bytes, next, b"AS") {
            continue;
        }
        next += 2;
        while next < bytes.len() && matches!(bytes[next], b' ' | b'\t') {
            next += 1;
        }
        if next >= bytes.len() || bytes[next] != b'(' {
            continue;
        }

        return Some(statement_start + index);
    }

    None
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_part(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

fn starts_with_ascii_case_insensitive(haystack: &[u8], start: usize, needle: &[u8]) -> bool {
    if start + needle.len() > haystack.len() {
        return false;
    }
    haystack[start..start + needle.len()]
        .iter()
        .zip(needle.iter())
        .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

fn tokenize_with_offsets(sql: &str, dialect: Dialect) -> Option<Vec<LocatedToken>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        let Some(start) = line_col_to_offset(
            sql,
            token.span.start.line as usize,
            token.span.start.column as usize,
        ) else {
            continue;
        };
        let Some(end) = line_col_to_offset(
            sql,
            token.span.end.line as usize,
            token.span.end.column as usize,
        ) else {
            continue;
        };

        out.push(LocatedToken {
            token: token.token,
            start,
            end,
        });
    }

    Some(out)
}

fn tokenize_with_offsets_for_context(ctx: &RuleContext) -> Option<Vec<LocatedToken>> {
    let tokens = ctx.with_document_tokens(|tokens| {
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
    });

    if let Some(tokens) = tokens {
        return Some(tokens);
    }

    tokenize_with_offsets(ctx.sql, ctx.dialect())
}

fn token_start_offset(sql: &str, token: &TokenWithSpan) -> Option<usize> {
    line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )
}

fn token_start_offset_for_context(ctx: &RuleContext, token: &TokenWithSpan) -> Option<usize> {
    if ctx.statement_range.start > 0 {
        if let Some(abs_start) = token_start_offset(ctx.sql, token) {
            if abs_start >= ctx.statement_range.start && abs_start < ctx.statement_range.end {
                return Some(abs_start);
            }
        }

        if let Some(rel_start) = token_start_offset(ctx.statement_sql(), token) {
            let abs_start = ctx.statement_range.start + rel_start;
            if abs_start < ctx.statement_range.end {
                return Some(abs_start);
            }
        }

        return None;
    }

    token_start_offset(ctx.statement_sql(), token).or_else(|| token_start_offset(ctx.sql, token))
}

fn suffix_summary_after_offset(
    sql: &str,
    tokens: &[LocatedToken],
    start_offset: usize,
    statement_end: usize,
) -> (usize, Option<(usize, usize)>) {
    let mut blank_lines = 0usize;
    let mut line_blank = false;

    for token in tokens {
        if token.start < start_offset {
            continue;
        }
        if token.start >= statement_end {
            break;
        }

        match &token.token {
            Token::Comma => {
                line_blank = false;
            }
            trivia if is_trivia_token(trivia) => {
                consume_text_for_blank_lines(
                    &sql[token.start..token.end],
                    &mut blank_lines,
                    &mut line_blank,
                );
            }
            _ => return (blank_lines, Some((token.start, token.end))),
        }
    }

    (blank_lines, None)
}

fn consume_text_for_blank_lines(text: &str, blank_lines: &mut usize, line_blank: &mut bool) {
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\n' => {
                if *line_blank {
                    *blank_lines += 1;
                }
                *line_blank = true;
            }
            '\r' => {
                if matches!(chars.peek(), Some('\n')) {
                    let _ = chars.next();
                }
                if *line_blank {
                    *blank_lines += 1;
                }
                *line_blank = true;
            }
            c if c.is_whitespace() => {}
            _ => *line_blank = false,
        }
    }
}

fn whitespace_gap_span(sql: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    if start > end || end > sql.len() {
        return None;
    }
    let gap = &sql[start..end];
    if gap.chars().all(char::is_whitespace) {
        Some((start, end))
    } else {
        None
    }
}

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

fn is_comment_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

fn first_comment_start_in_range(
    tokens: &[LocatedToken],
    start_offset: usize,
    end_offset: usize,
) -> Option<usize> {
    tokens
        .iter()
        .find(|token| {
            token.start >= start_offset
                && token.start < end_offset
                && is_comment_token(&token.token)
        })
        .map(|token| token.start)
}

fn first_comma_end_in_range(
    tokens: &[LocatedToken],
    start_offset: usize,
    end_offset: usize,
) -> Option<usize> {
    tokens
        .iter()
        .find(|token| {
            token.start >= start_offset
                && token.start < end_offset
                && matches!(token.token, Token::Comma)
        })
        .map(|token| token.end)
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
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutCteNewline::default();
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
        for edit in edits.into_iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    #[test]
    fn flags_missing_blank_line_after_cte() {
        let inline_sql = "WITH cte AS (SELECT 1) SELECT * FROM cte";
        let inline_issues = run(inline_sql);
        assert!(!inline_issues.is_empty());
        let autofix = inline_issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(inline_sql, &inline_issues[0]).expect("apply autofix");
        assert_eq!(fixed, "WITH cte AS (SELECT 1)\n\nSELECT * FROM cte");

        let newline_sql = "WITH cte AS (SELECT 1)\nSELECT * FROM cte";
        let newline_issues = run(newline_sql);
        assert!(!newline_issues.is_empty());
        let newline_fixed =
            apply_issue_autofix(newline_sql, &newline_issues[0]).expect("apply newline autofix");
        assert_eq!(newline_fixed, "WITH cte AS (SELECT 1)\n\nSELECT * FROM cte");
    }

    #[test]
    fn does_not_flag_with_blank_line_after_cte() {
        assert!(run("WITH cte AS (SELECT 1)\n\nSELECT * FROM cte").is_empty());
    }

    #[test]
    fn flags_each_missing_separator_between_multiple_ctes() {
        let issues = run("WITH a AS (SELECT 1),
-- comment between CTEs
b AS (SELECT 2)
SELECT * FROM b");
        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_008)
                .count(),
            2,
        );
    }

    #[test]
    fn comment_only_line_is_not_a_blank_line_separator() {
        assert!(!run("WITH cte AS (SELECT 1)\n-- separator\nSELECT * FROM cte").is_empty());
    }

    #[test]
    fn leading_comma_oneline_cte_autofix_inserts_blank_line_before_comma() {
        let sql =
            "with my_cte as (select 1), other_cte as (select 1) select * from my_cte\ncross join other_cte\n";
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.commas".to_string(),
                serde_json::json!({"line_position": "leading"}),
            )]),
        };
        let rule = LayoutCteNewline::from_config(&config);
        let statements = parse_sql(sql).expect("parse");
        let issues = statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect::<Vec<_>>();

        let fixed = issues.iter().fold(sql.to_string(), |current, issue| {
            let Some(autofix) = issue.autofix.as_ref() else {
                return current;
            };
            let mut edits = autofix.edits.clone();
            edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
            let mut updated = current;
            for edit in edits.into_iter().rev() {
                updated.replace_range(edit.span.start..edit.span.end, &edit.replacement);
            }
            updated
        });
        assert_eq!(
            fixed,
            "with my_cte as (select 1)\n\n, other_cte as (select 1)\n\nselect * from my_cte\ncross join other_cte\n"
        );
    }

    #[test]
    fn trailing_comma_cte_autofix_inserts_blank_line_after_comma() {
        let sql = "WITH a AS (SELECT 1),\nb AS (SELECT 2)\nSELECT * FROM b";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let mut edits = issues
            .iter()
            .filter_map(|issue| issue.autofix.as_ref())
            .flat_map(|autofix| autofix.edits.clone())
            .collect::<Vec<_>>();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        let mut fixed = sql.to_string();
        for edit in edits.into_iter().rev() {
            fixed.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        assert_eq!(
            fixed,
            "WITH a AS (SELECT 1),\n\nb AS (SELECT 2)\n\nSELECT * FROM b"
        );
    }

    #[test]
    fn trailing_comma_cte_autofix_preserves_comment_between_ctes() {
        let sql = "WITH a AS (SELECT 1),\n-- keep this note\nb AS (SELECT 2)\nSELECT * FROM b";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let mut edits = issues
            .iter()
            .filter_map(|issue| issue.autofix.as_ref())
            .flat_map(|autofix| autofix.edits.clone())
            .collect::<Vec<_>>();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        let mut fixed = sql.to_string();
        for edit in edits.into_iter().rev() {
            fixed.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        assert_eq!(
            fixed,
            "WITH a AS (SELECT 1),\n\n-- keep this note\nb AS (SELECT 2)\n\nSELECT * FROM b"
        );
    }
}
