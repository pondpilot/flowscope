//! LINT_LT_006: Layout functions.
//!
//! SQLFluff LT06 parity (current scope): flag function-like tokens separated
//! from opening parenthesis.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::linter::visit::visit_expressions;
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::{Expr, Statement};
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Location, Span, Token, TokenWithSpan, Tokenizer, Whitespace};
use std::collections::HashSet;

pub struct LayoutFunctions;

impl BuiltinLintRule for LayoutFunctions {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_006
    }

    fn name(&self) -> &'static str {
        "Layout functions"
    }

    fn description(&self) -> &'static str {
        "Function name not immediately followed by parenthesis."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let Some(issue_span) = function_spacing_issue_span(statement, ctx) else {
            return Vec::new();
        };

        let function_span =
            ctx.span_from_statement_offset(issue_span.function_start, issue_span.function_end);
        let gap_span = ctx.span_from_statement_offset(issue_span.gap_start, issue_span.gap_end);

        vec![Issue::info(
            issue_codes::LINT_LT_006,
            "Function call spacing appears inconsistent.",
        )
        .with_statement(ctx.statement_index)
        .with_span(function_span)
        .with_autofix_edits(
            IssueAutofixApplicability::Safe,
            vec![IssuePatchEdit::new(gap_span, "")],
        )]
    }
}

#[derive(Clone, Copy, Debug)]
struct FunctionSpacingIssueSpan {
    function_start: usize,
    function_end: usize,
    gap_start: usize,
    gap_end: usize,
}

fn function_spacing_issue_span(
    statement: &Statement,
    ctx: &RuleContext,
) -> Option<FunctionSpacingIssueSpan> {
    let sql = ctx.statement_sql();
    let tracked_function_names = tracked_function_names(statement);

    let tokens = tokenized_for_context(ctx).or_else(|| tokenized(sql, ctx.dialect()))?;

    for (index, token) in tokens.iter().enumerate() {
        let Token::Word(word) = &token.token else {
            continue;
        };

        if word.quote_style.is_some() {
            continue;
        }

        let word_upper = word.value.to_ascii_uppercase();
        if !tracked_function_names.contains(&word_upper) && !is_always_function_keyword(&word_upper)
        {
            continue;
        }
        if word_upper == "EXISTS" && !is_select_projection_exists(&tokens, index) {
            continue;
        }

        let Some(next_index) = next_non_trivia_index(&tokens, index + 1) else {
            continue;
        };

        if !matches!(tokens[next_index].token, Token::LParen) {
            continue;
        }

        // No whitespace/comment tokens between name and `(` means no spacing issue.
        if next_index == index + 1 {
            continue;
        }

        if let Some(prev_index) = prev_non_trivia_index(&tokens, index) {
            if matches!(&tokens[prev_index].token, Token::Period) {
                continue;
            }
        }

        let function_start = line_col_to_offset(
            sql,
            token.span.start.line as usize,
            token.span.start.column as usize,
        )?;
        let function_end = line_col_to_offset(
            sql,
            token.span.end.line as usize,
            token.span.end.column as usize,
        )?;
        let gap_end = line_col_to_offset(
            sql,
            tokens[next_index].span.start.line as usize,
            tokens[next_index].span.start.column as usize,
        )?;
        if function_end >= gap_end {
            continue;
        }

        return Some(FunctionSpacingIssueSpan {
            function_start,
            function_end,
            gap_start: function_end,
            gap_end,
        });
    }

    None
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
                Span::new(start_loc, end_loc),
            ));
        }

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    })
}

fn tracked_function_names(statement: &Statement) -> HashSet<String> {
    let mut names = HashSet::new();
    visit_expressions(statement, &mut |expr| {
        if let Expr::Function(function) = expr {
            if let Some(last_part) = function.name.0.last() {
                names.insert(last_part.to_string().to_ascii_uppercase());
            }
        }
    });
    names
}

fn next_non_trivia_index(tokens: &[TokenWithSpan], mut index: usize) -> Option<usize> {
    while index < tokens.len() {
        if !is_trivia_token(&tokens[index].token) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn prev_non_trivia_index(tokens: &[TokenWithSpan], mut index: usize) -> Option<usize> {
    while index > 0 {
        index -= 1;
        if !is_trivia_token(&tokens[index].token) {
            return Some(index);
        }
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

fn is_always_function_keyword(word: &str) -> bool {
    matches!(
        word,
        "CAST" | "TRY_CAST" | "SAFE_CAST" | "CONVERT" | "EXISTS"
    )
}

fn is_select_projection_exists(tokens: &[TokenWithSpan], exists_index: usize) -> bool {
    let Some(prev_index) = prev_non_trivia_index(tokens, exists_index) else {
        return false;
    };

    match &tokens[prev_index].token {
        Token::Comma => true,
        Token::Word(word) => word.keyword == Keyword::SELECT,
        _ => false,
    }
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
        let rule = LayoutFunctions;
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
        let mut edits = autofix.edits.clone();
        edits.sort_by(|left, right| right.span.start.cmp(&left.span.start));

        let mut out = sql.to_string();
        for edit in edits {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    #[test]
    fn flags_space_between_function_name_and_paren() {
        let issues = run("SELECT COUNT (1) FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_006);
    }

    #[test]
    fn does_not_flag_normal_function_call() {
        assert!(run("SELECT COUNT(1) FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_table_name_followed_by_paren() {
        assert!(run("INSERT INTO metrics_table (id) VALUES (1)").is_empty());
    }

    #[test]
    fn does_not_flag_string_literal_function_like_text() {
        assert!(run("SELECT 'COUNT (1)' AS txt").is_empty());
    }

    #[test]
    fn flags_space_between_cast_keyword_and_paren() {
        let issues = run("SELECT CAST (1 AS INT)");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_006);
    }

    #[test]
    fn flags_space_between_exists_keyword_and_paren() {
        let sql = "SELECT EXISTS (SELECT 1) AS has_rows";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT EXISTS(SELECT 1) AS has_rows");
    }

    #[test]
    fn does_not_flag_where_exists_predicate_spacing() {
        assert!(run("SELECT 1 FROM t WHERE NOT EXISTS (SELECT 1)").is_empty());
    }

    #[test]
    fn emits_safe_autofix_patch_for_function_spacing() {
        let sql = "SELECT COUNT (1) FROM t";
        let issues = run(sql);
        let issue = &issues[0];
        let autofix = issue.autofix.as_ref().expect("autofix metadata");

        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        assert_eq!(autofix.edits[0].replacement, "");

        let fixed = apply_issue_autofix(sql, issue).expect("apply autofix");
        assert_eq!(fixed, "SELECT COUNT(1) FROM t");
    }
}
