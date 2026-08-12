//! LINT_LT_010: Layout select modifiers.
//!
//! SQLFluff LT10 parity (current scope): detect multiline SELECT modifiers in
//! inconsistent positions.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{
    Location, Span as TokenSpan, Token, TokenWithSpan, Tokenizer, Whitespace,
};

pub struct LayoutSelectModifiers;

type SimpleCollapseSpans = Vec<(usize, usize)>;
type CommentAwareEdits = Vec<(usize, usize, String)>;
type Lt010ViolationResult = (bool, SimpleCollapseSpans, CommentAwareEdits);

impl BuiltinLintRule for LayoutSelectModifiers {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_010
    }

    fn name(&self) -> &'static str {
        "Layout select modifiers"
    }

    fn description(&self) -> &'static str {
        "'SELECT' modifiers (e.g. 'DISTINCT') must be on the same line as 'SELECT'."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let (has_violation, fixable_spans, comment_aware_edits) =
            select_modifier_violations_and_fixable_spans(ctx);
        if has_violation {
            let mut issue = Issue::info(
                issue_codes::LINT_LT_010,
                "SELECT modifiers (DISTINCT/ALL) should be consistently formatted.",
            )
            .with_statement(ctx.statement_index);

            if !comment_aware_edits.is_empty() {
                // Comment-aware edits: use the first edit's start for the span.
                let (start, end, _) = &comment_aware_edits[0];
                issue = issue.with_span(ctx.span_from_statement_offset(*start, *end));
                let edits = comment_aware_edits
                    .into_iter()
                    .map(|(edit_start, edit_end, replacement)| {
                        IssuePatchEdit::new(
                            ctx.span_from_statement_offset(edit_start, edit_end),
                            replacement,
                        )
                    })
                    .collect();
                issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, edits);
            } else if let Some((start, end)) = fixable_spans.first().copied() {
                issue = issue.with_span(ctx.span_from_statement_offset(start, end));
                let edits = fixable_spans
                    .into_iter()
                    .map(|(edit_start, edit_end)| {
                        IssuePatchEdit::new(
                            ctx.span_from_statement_offset(edit_start, edit_end),
                            " ",
                        )
                    })
                    .collect();
                issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, edits);
            }

            vec![issue]
        } else {
            Vec::new()
        }
    }
}

/// Returns (has_violation, simple_collapse_spans, comment_aware_edits).
/// `simple_collapse_spans` are (start, end) ranges to replace with " ".
/// `comment_aware_edits` are (start, end, replacement) triples for surgical edits.
fn select_modifier_violations_and_fixable_spans(ctx: &RuleContext) -> Lt010ViolationResult {
    let tokens =
        tokenized_for_context(ctx).or_else(|| tokenized(ctx.statement_sql(), ctx.dialect()));
    let Some(tokens) = tokens else {
        return (false, Vec::new(), Vec::new());
    };

    let mut has_violation = false;
    let mut fixable_spans = Vec::new();
    let mut comment_aware_edits = Vec::new();
    let sql = ctx.statement_sql();

    for (index, token) in tokens.iter().enumerate() {
        let Token::Word(word) = &token.token else {
            continue;
        };

        if word.keyword != Keyword::SELECT {
            continue;
        }

        let Some(next_index) = next_non_trivia_index(&tokens, index + 1) else {
            continue;
        };
        let Token::Word(next_word) = &tokens[next_index].token else {
            continue;
        };

        if !matches!(next_word.keyword, Keyword::DISTINCT | Keyword::ALL) {
            continue;
        }

        if tokens[next_index].span.start.line > token.span.end.line {
            has_violation = true;

            let Some(select_end) = line_col_to_offset(
                sql,
                token.span.end.line as usize,
                token.span.end.column as usize,
            ) else {
                continue;
            };
            let Some(modifier_start) = line_col_to_offset(
                sql,
                tokens[next_index].span.start.line as usize,
                tokens[next_index].span.start.column as usize,
            ) else {
                continue;
            };
            let Some(modifier_end) = line_col_to_offset(
                sql,
                tokens[next_index].span.end.line as usize,
                tokens[next_index].span.end.column as usize,
            ) else {
                continue;
            };

            if trivia_between_is_whitespace_only(&tokens, index, next_index) {
                // Simple case: no comments — collapse whitespace between SELECT
                // and modifier to a single space.
                if select_end < modifier_start {
                    fixable_spans.push((select_end, modifier_start));
                }
            } else {
                // Comment-aware case: place modifier after SELECT and remove
                // it from its original position. Uses surgical edits around
                // the comment's protected range.
                let modifier_text = &sql[modifier_start..modifier_end];

                // Find first comment token to determine where the gap before
                // it ends.
                let first_comment_start = (index + 1..next_index)
                    .filter(|&i| is_comment_token(&tokens[i].token))
                    .find_map(|i| {
                        line_col_to_offset(
                            sql,
                            tokens[i].span.start.line as usize,
                            tokens[i].span.start.column as usize,
                        )
                    });

                if let Some(comment_start) = first_comment_start {
                    // Determine the indent of the modifier's line for
                    // preserving alignment.
                    let indent = detect_indent(sql, modifier_start);
                    // Edit 1: Replace gap between SELECT and first comment
                    // with " MODIFIER\n indent".
                    comment_aware_edits.push((
                        select_end,
                        comment_start,
                        format!(" {modifier_text}\n{indent}"),
                    ));
                    // Edit 2: Remove the modifier + trailing space from its
                    // original line.
                    let remove_end = skip_trailing_space(sql, modifier_end);
                    comment_aware_edits.push((modifier_start, remove_end, String::new()));
                }
            }
        }
    }

    fixable_spans.sort_unstable();
    fixable_spans.dedup();
    (has_violation, fixable_spans, comment_aware_edits)
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn tokenized_for_context(ctx: &RuleContext) -> Option<Vec<TokenWithSpan>> {
    let (statement_start_line, statement_start_column) =
        offset_to_line_col(ctx.sql, ctx.statement_range.start)?;

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

            let Some(start_loc) = relative_location(
                token.span.start,
                statement_start_line,
                statement_start_column,
            ) else {
                continue;
            };
            let Some(end_loc) =
                relative_location(token.span.end, statement_start_line, statement_start_column)
            else {
                continue;
            };

            out.push(TokenWithSpan::new(
                token.token.clone(),
                TokenSpan::new(start_loc, end_loc),
            ));
        }

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    })
}

fn next_non_trivia_index(
    tokens: &[sqlparser::tokenizer::TokenWithSpan],
    mut index: usize,
) -> Option<usize> {
    while index < tokens.len() {
        if !is_trivia_token(&tokens[index].token) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
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

/// Detect the indentation prefix on the line where `offset` points.
fn detect_indent(sql: &str, offset: usize) -> String {
    let line_start = sql[..offset].rfind('\n').map(|pos| pos + 1).unwrap_or(0);
    sql[line_start..]
        .chars()
        .take_while(|ch| ch.is_whitespace() && *ch != '\n')
        .collect()
}

/// Skip trailing spaces after `offset`, stopping at newline or non-space.
fn skip_trailing_space(sql: &str, offset: usize) -> usize {
    let mut pos = offset;
    for ch in sql[offset..].chars() {
        if ch == ' ' {
            pos += 1;
        } else {
            break;
        }
    }
    pos
}

fn trivia_between_is_whitespace_only(tokens: &[TokenWithSpan], left: usize, right: usize) -> bool {
    if right <= left + 1 {
        return true;
    }

    tokens[left + 1..right].iter().all(|token| {
        matches!(
            token.token,
            Token::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
        )
    })
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

fn offset_to_line_col(sql: &str, offset: usize) -> Option<(usize, usize)> {
    if offset > sql.len() {
        return None;
    }
    if offset == sql.len() {
        let mut line = 1usize;
        let mut column = 1usize;
        for ch in sql.chars() {
            if ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        return Some((line, column));
    }

    let mut line = 1usize;
    let mut column = 1usize;
    for (index, ch) in sql.char_indices() {
        if index == offset {
            return Some((line, column));
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    None
}

fn relative_location(
    location: Location,
    statement_start_line: usize,
    statement_start_column: usize,
) -> Option<Location> {
    let line = location.line as usize;
    let column = location.column as usize;
    if line < statement_start_line {
        return None;
    }

    if line == statement_start_line {
        if column < statement_start_column {
            return None;
        }
        return Some(Location::new(
            1,
            (column - statement_start_column + 1) as u64,
        ));
    }

    Some(Location::new(
        (line - statement_start_line + 1) as u64,
        column as u64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutSelectModifiers;
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
    fn flags_distinct_on_next_line() {
        let sql = "SELECT\nDISTINCT a\nFROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_010);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT DISTINCT a\nFROM t");
    }

    #[test]
    fn does_not_flag_single_line_modifier() {
        assert!(run("SELECT DISTINCT a FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_modifier_text_in_string() {
        assert!(run("SELECT 'SELECT\nDISTINCT a' AS txt").is_empty());
    }

    #[test]
    fn comment_between_select_and_modifier_has_autofix() {
        let sql = "SELECT\n-- keep\nDISTINCT a\nFROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_010);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT DISTINCT\n-- keep\na\nFROM t");
    }

    #[test]
    fn comment_between_select_and_distinct_with_indent() {
        let sql = "SELECT\n    -- The table contains duplicates, so we use DISTINCT.\n    DISTINCT user_id\nFROM\n    safe_user";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "SELECT DISTINCT\n    -- The table contains duplicates, so we use DISTINCT.\n    user_id\nFROM\n    safe_user"
        );
    }
}
