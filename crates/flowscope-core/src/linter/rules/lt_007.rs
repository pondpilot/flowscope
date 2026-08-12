//! LINT_LT_007: Layout CTE bracket.
//!
//! SQLFluff LT07 parity (current scope): in multiline CTE bodies, the closing
//! bracket should appear on its own line.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::{CreateView, Query, Statement};
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};
use std::ops::Range;

pub struct LayoutCteBracket;

impl BuiltinLintRule for LayoutCteBracket {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_007
    }

    fn name(&self) -> &'static str {
        "Layout CTE bracket"
    }

    fn description(&self) -> &'static str {
        "'WITH' clause closing bracket should be on a new line."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let tokens = tokenize_with_offsets_for_context(ctx);
        let violation = misplaced_cte_closing_bracket_for_statement(
            statement,
            ctx,
            tokens.as_deref(),
        )
        .or_else(|| {
            misplaced_cte_closing_bracket(ctx.statement_sql(), ctx.dialect(), tokens.as_deref())
        });

        if let Some(((start, end), fix_span)) = violation {
            let mut issue = Issue::warning(
                issue_codes::LINT_LT_007,
                "CTE AS clause appears to be missing surrounding brackets.",
            )
            .with_statement(ctx.statement_index)
            .with_span(ctx.span_from_statement_offset(start, end));
            if let Some((fix_start, fix_end)) = fix_span {
                issue = issue.with_autofix_edits(
                    IssueAutofixApplicability::Safe,
                    vec![IssuePatchEdit::new(
                        ctx.span_from_statement_offset(fix_start, fix_end),
                        "\n",
                    )],
                );
            }
            vec![issue]
        } else {
            Vec::new()
        }
    }
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
    start_line: usize,
    end_line: usize,
}

type Lt07Span = (usize, usize);
type Lt07AutofixSpan = (usize, usize);
type Lt07Violation = (Lt07Span, Option<Lt07AutofixSpan>);

fn misplaced_cte_closing_bracket_for_statement(
    statement: &Statement,
    ctx: &RuleContext,
    tokens: Option<&[LocatedToken]>,
) -> Option<Lt07Violation> {
    let query = match statement {
        Statement::Query(query) => query.as_ref(),
        Statement::CreateView(CreateView { query, .. }) => query.as_ref(),
        _ => return None,
    };

    misplaced_cte_closing_bracket_in_query(query, ctx, tokens)
}

fn misplaced_cte_closing_bracket_in_query(
    query: &Query,
    ctx: &RuleContext,
    tokens: Option<&[LocatedToken]>,
) -> Option<Lt07Violation> {
    let with = query.with.as_ref()?;
    let sql = ctx.statement_sql();
    let owned_tokens;
    let tokens = if let Some(tokens) = tokens {
        tokens
    } else {
        owned_tokens = tokenize_with_offsets(sql, ctx.dialect())?;
        &owned_tokens
    };

    for cte in &with.cte_tables {
        let Some(close_abs) = token_start_offset(ctx.sql, &cte.closing_paren_token.0) else {
            continue;
        };
        if close_abs < ctx.statement_range.start || close_abs >= ctx.statement_range.end {
            continue;
        }
        let close_rel = close_abs - ctx.statement_range.start;
        let Some(close_idx) = tokens
            .iter()
            .position(|token| matches!(token.token, Token::RParen) && token.start == close_rel)
        else {
            continue;
        };
        let Some(open_idx) = matching_open_paren_index(tokens, close_idx) else {
            continue;
        };

        let body_end = tokens[close_idx].start;
        if body_end > sql.len() {
            continue;
        }
        if !cte_body_has_line_break(tokens, sql, open_idx, close_idx) {
            continue;
        }
        let Some(prev_idx) = last_non_spacing_token_before_on_same_line(tokens, close_idx) else {
            continue;
        };

        let report_span = (tokens[close_idx].start, tokens[close_idx].end);
        let fix_span = safe_newline_fix_span(sql, tokens, prev_idx, close_idx);
        return Some((report_span, fix_span));
    }

    None
}

fn matching_open_paren_index(tokens: &[LocatedToken], close_idx: usize) -> Option<usize> {
    if !matches!(tokens.get(close_idx)?.token, Token::RParen) {
        return None;
    }

    let mut depth = 0usize;
    for index in (0..=close_idx).rev() {
        match tokens[index].token {
            Token::RParen => depth += 1,
            Token::LParen => {
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
            start_line: token.span.start.line as usize,
            end_line: token.span.end.line as usize,
        });
    }

    Some(out)
}

fn tokenize_with_offsets_for_context(ctx: &RuleContext) -> Option<Vec<LocatedToken>> {
    let statement_start = ctx.statement_range.start;
    let from_document_tokens = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut out = Vec::new();
        let mut covered_ranges = Vec::new();
        for token in tokens {
            let (start, end) = token_with_span_offsets(ctx.sql, token)?;
            if start < ctx.statement_range.start || end > ctx.statement_range.end {
                continue;
            }
            // Document token spans are tied to rendered SQL. If the source
            // slice does not match, the streams are misaligned; fall back to
            // statement-local tokenization.
            if !token_span_matches_source(ctx.sql, start, end, &token.token) {
                return None;
            }
            covered_ranges.push(start..end);

            out.push(LocatedToken {
                token: token.token.clone(),
                start: start - statement_start,
                end: end - statement_start,
                start_line: token.span.start.line as usize,
                end_line: token.span.end.line as usize,
            });
        }

        if !gaps_are_whitespace_only(ctx.sql, ctx.statement_range.clone(), &covered_ranges) {
            return None;
        }
        Some(out)
    });

    if let Some(tokens) = from_document_tokens {
        return Some(tokens);
    }

    tokenize_with_offsets(ctx.statement_sql(), ctx.dialect())
}

fn token_span_matches_source(sql: &str, start: usize, end: usize, token: &Token) -> bool {
    if start > end || end > sql.len() {
        return false;
    }

    match token {
        Token::Word(word) => source_word_matches(sql, start, end, word.value.as_str()),
        Token::LParen => sql.get(start..end) == Some("("),
        Token::RParen => sql.get(start..end) == Some(")"),
        _ => true,
    }
}

fn source_word_matches(sql: &str, start: usize, end: usize, value: &str) -> bool {
    let Some(raw) = sql.get(start..end) else {
        return false;
    };
    let normalized = raw.trim_matches(|ch| matches!(ch, '"' | '`' | '[' | ']'));
    normalized.eq_ignore_ascii_case(value)
}

fn gaps_are_whitespace_only(sql: &str, range: Range<usize>, covered: &[Range<usize>]) -> bool {
    if range.start > range.end || range.end > sql.len() {
        return false;
    }

    let mut spans = covered.to_vec();
    spans.sort_by_key(|span| (span.start, span.end));

    let mut cursor = range.start;
    for span in spans {
        if span.end <= range.start || span.start >= range.end {
            continue;
        }
        let start = span.start.max(range.start);
        let end = span.end.min(range.end);
        if start > cursor {
            let Some(gap) = sql.get(cursor..start) else {
                return false;
            };
            if gap.chars().any(|ch| !ch.is_whitespace()) {
                return false;
            }
        }
        cursor = cursor.max(end);
    }

    if cursor < range.end {
        let Some(gap) = sql.get(cursor..range.end) else {
            return false;
        };
        if gap.chars().any(|ch| !ch.is_whitespace()) {
            return false;
        }
    }

    true
}

fn token_start_offset(sql: &str, token: &sqlparser::tokenizer::TokenWithSpan) -> Option<usize> {
    line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )
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

fn misplaced_cte_closing_bracket(
    sql: &str,
    dialect: Dialect,
    tokens: Option<&[LocatedToken]>,
) -> Option<Lt07Violation> {
    let owned_tokens;
    let tokens = if let Some(tokens) = tokens {
        tokens
    } else {
        owned_tokens = tokenize_with_offsets(sql, dialect)?;
        &owned_tokens
    };

    if tokens.is_empty() {
        return None;
    }

    let has_with = tokens
        .iter()
        .any(|token| matches!(token.token, Token::Word(ref word) if word.keyword == Keyword::WITH));
    if !has_with {
        return None;
    }

    let mut index = 0usize;
    while let Some(as_idx) = find_next_as_keyword(tokens, index) {
        let Some(open_idx) = next_non_trivia_index(tokens, as_idx + 1) else {
            index = as_idx + 1;
            continue;
        };
        if !matches!(tokens[open_idx].token, Token::LParen) {
            index = as_idx + 1;
            continue;
        }

        let Some(close_idx) = matching_close_paren_index(tokens, open_idx) else {
            index = open_idx + 1;
            continue;
        };

        let body_start = tokens[open_idx].end;
        let body_end = tokens[close_idx].start;
        if body_start < body_end
            && body_end <= sql.len()
            && cte_body_has_line_break(tokens, sql, open_idx, close_idx)
        {
            if let Some(prev_idx) = last_non_spacing_token_before_on_same_line(tokens, close_idx) {
                let report_span = (tokens[close_idx].start, tokens[close_idx].end);
                let fix_span = safe_newline_fix_span(sql, tokens, prev_idx, close_idx);
                return Some((report_span, fix_span));
            }
        }

        index = close_idx + 1;
    }

    None
}

fn cte_body_has_line_break(
    tokens: &[LocatedToken],
    sql: &str,
    open_idx: usize,
    close_idx: usize,
) -> bool {
    if close_idx <= open_idx + 1 {
        return false;
    }

    tokens
        .iter()
        .take(close_idx)
        .skip(open_idx + 1)
        .any(|token| token.end <= sql.len() && count_line_breaks(&sql[token.start..token.end]) > 0)
}

fn last_non_spacing_token_before_on_same_line(
    tokens: &[LocatedToken],
    close_idx: usize,
) -> Option<usize> {
    let close = tokens.get(close_idx)?;
    let line = close.start_line;

    for (index, token) in tokens[..close_idx].iter().enumerate().rev() {
        if token.end_line < line {
            break;
        }
        if token.start_line != line {
            continue;
        }
        if is_spacing_whitespace(&token.token) {
            continue;
        }
        return Some(index);
    }

    None
}

fn safe_newline_fix_span(
    sql: &str,
    tokens: &[LocatedToken],
    prev_idx: usize,
    close_idx: usize,
) -> Option<Lt07AutofixSpan> {
    let gap_start = tokens.get(prev_idx)?.end;
    let gap_end = tokens.get(close_idx)?.start;
    if gap_start > gap_end || gap_end > sql.len() {
        return None;
    }

    let gap = &sql[gap_start..gap_end];
    if gap.chars().all(char::is_whitespace) && !gap.contains('\n') && !gap.contains('\r') {
        Some((gap_start, gap_end))
    } else {
        None
    }
}

#[cfg(test)]
fn has_misplaced_cte_closing_bracket(sql: &str, dialect: Dialect) -> bool {
    misplaced_cte_closing_bracket(sql, dialect, None).is_some()
}

fn find_next_as_keyword(tokens: &[LocatedToken], mut index: usize) -> Option<usize> {
    while index < tokens.len() {
        if matches!(
            tokens[index].token,
            Token::Word(ref word) if word.keyword == Keyword::AS
        ) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn next_non_trivia_index(tokens: &[LocatedToken], mut index: usize) -> Option<usize> {
    while index < tokens.len() {
        if !is_trivia_token(&tokens[index].token) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn matching_close_paren_index(tokens: &[LocatedToken], open_idx: usize) -> Option<usize> {
    if !matches!(tokens.get(open_idx)?.token, Token::LParen) {
        return None;
    }

    let mut depth = 0usize;
    for (idx, token) in tokens.iter().enumerate().skip(open_idx) {
        match token.token {
            Token::LParen => depth += 1,
            Token::RParen => {
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

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

fn is_spacing_whitespace(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
    )
}

fn count_line_breaks(text: &str) -> usize {
    let mut count = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\n' {
            count += 1;
            continue;
        }
        if ch == '\r' {
            count += 1;
            if matches!(chars.peek(), Some('\n')) {
                let _ = chars.next();
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutCteBracket;
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
    fn flags_closing_paren_after_sql_code_in_multiline_cte() {
        let sql = "with cte_1 as (\n    select foo\n    from tbl_1)\nselect * from cte_1";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_007);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "with cte_1 as (\n    select foo\n    from tbl_1\n)\nselect * from cte_1"
        );
    }

    #[test]
    fn does_not_flag_single_line_cte_body() {
        assert!(run("WITH cte AS (SELECT 1) SELECT * FROM cte").is_empty());
    }

    #[test]
    fn does_not_flag_multiline_cte_with_own_line_close() {
        let sql = "with cte as (\n    select 1\n) select * from cte";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn flags_templated_close_paren_on_same_line_as_cte_body_code() {
        let sql =
            "with\n{% if true %}\n  cte as (\n      select 1)\n{% endif %}\nselect * from cte";
        assert!(has_misplaced_cte_closing_bracket(sql, Dialect::Generic));
    }

    #[test]
    fn flags_close_paren_when_comment_precedes_on_same_line() {
        let sql = "WITH cte AS (\n  SELECT 1 /* trailing comment */)\nSELECT * FROM cte";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_007);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "WITH cte AS (\n  SELECT 1 /* trailing comment */\n)\nSELECT * FROM cte"
        );
    }
}
