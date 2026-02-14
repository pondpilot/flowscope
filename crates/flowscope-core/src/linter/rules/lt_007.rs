//! LINT_LT_007: Layout CTE bracket.
//!
//! SQLFluff LT07 parity (current scope): in multiline CTE bodies, the closing
//! bracket should appear on its own line.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::{Query, Statement};
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct LayoutCteBracket;

impl LintRule for LayoutCteBracket {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_007
    }

    fn name(&self) -> &'static str {
        "Layout CTE bracket"
    }

    fn description(&self) -> &'static str {
        "CTE bodies should be wrapped in brackets."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let tokens = if has_template_markers(ctx.statement_sql()) {
            None
        } else {
            tokenize_with_offsets_for_context(ctx)
        };
        let has_violation =
            has_misplaced_cte_closing_bracket_for_statement(statement, ctx, tokens.as_deref())
                .unwrap_or_else(|| {
                    has_misplaced_cte_closing_bracket(
                        ctx.statement_sql(),
                        ctx.dialect(),
                        tokens.as_deref(),
                    )
                });

        if has_violation {
            vec![Issue::warning(
                issue_codes::LINT_LT_007,
                "CTE AS clause appears to be missing surrounding brackets.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn has_template_markers(sql: &str) -> bool {
    sql.contains("{{") || sql.contains("{%") || sql.contains("{#")
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn has_misplaced_cte_closing_bracket_for_statement(
    statement: &Statement,
    ctx: &LintContext,
    tokens: Option<&[LocatedToken]>,
) -> Option<bool> {
    let query = match statement {
        Statement::Query(query) => query.as_ref(),
        Statement::CreateView { query, .. } => query.as_ref(),
        _ => return None,
    };

    has_misplaced_cte_closing_bracket_in_query(query, ctx, tokens)
}

fn has_misplaced_cte_closing_bracket_in_query(
    query: &Query,
    ctx: &LintContext,
    tokens: Option<&[LocatedToken]>,
) -> Option<bool> {
    let with = query.with.as_ref()?;
    let sql = ctx.statement_sql();
    let owned_tokens;
    let tokens = if let Some(tokens) = tokens {
        tokens
    } else {
        owned_tokens = tokenize_with_offsets(sql, ctx.dialect())?;
        &owned_tokens
    };

    let mut evaluated_any = false;

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
        let Some(open_idx) = matching_open_paren_index(&tokens, close_idx) else {
            continue;
        };

        evaluated_any = true;

        let body_start = tokens[open_idx].end;
        let body_end = tokens[close_idx].start;
        if body_start >= body_end || body_end > sql.len() {
            continue;
        }
        let body = &sql[body_start..body_end];
        if body.contains('\n') && !line_prefix_before(sql, body_end).trim().is_empty() {
            return Some(true);
        }
    }

    if evaluated_any {
        Some(false)
    } else {
        None
    }
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
        });
    }

    Some(out)
}

fn tokenize_with_offsets_for_context(ctx: &LintContext) -> Option<Vec<LocatedToken>> {
    let statement_start = ctx.statement_range.start;
    let from_document_tokens = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        Some(
            tokens
                .iter()
                .filter_map(|token| {
                    let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                        return None;
                    };
                    if start < ctx.statement_range.start || end > ctx.statement_range.end {
                        return None;
                    }

                    Some(LocatedToken {
                        token: token.token.clone(),
                        start: start - statement_start,
                        end: end - statement_start,
                    })
                })
                .collect::<Vec<_>>(),
        )
    });

    if let Some(tokens) = from_document_tokens {
        return Some(tokens);
    }

    tokenize_with_offsets(ctx.statement_sql(), ctx.dialect())
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

fn has_misplaced_cte_closing_bracket(
    sql: &str,
    dialect: Dialect,
    tokens: Option<&[LocatedToken]>,
) -> bool {
    let owned_tokens;
    let tokens = if let Some(tokens) = tokens {
        tokens
    } else {
        owned_tokens = match tokenize_with_offsets(sql, dialect) {
            Some(tokens) => tokens,
            None => return false,
        };
        &owned_tokens
    };

    if tokens.is_empty() {
        return false;
    }

    let has_with = tokens
        .iter()
        .any(|token| matches!(token.token, Token::Word(ref word) if word.keyword == Keyword::WITH));
    if !has_with {
        return false;
    }

    let mut index = 0usize;
    while let Some(as_idx) = find_next_as_keyword(&tokens, index) {
        let Some(open_idx) = next_non_trivia_index(&tokens, as_idx + 1) else {
            index = as_idx + 1;
            continue;
        };
        if !matches!(tokens[open_idx].token, Token::LParen) {
            index = as_idx + 1;
            continue;
        }

        let Some(close_idx) = matching_close_paren_index(&tokens, open_idx) else {
            index = open_idx + 1;
            continue;
        };

        let body_start = tokens[open_idx].end;
        let body_end = tokens[close_idx].start;
        if body_start < body_end && body_end <= sql.len() {
            let body = &sql[body_start..body_end];
            if body.contains('\n') && !line_prefix_before(sql, body_end).trim().is_empty() {
                return true;
            }
        }

        index = close_idx + 1;
    }

    false
}

fn line_prefix_before(sql: &str, idx: usize) -> &str {
    let line_start = sql[..idx].rfind('\n').map_or(0, |pos| pos + 1);
    &sql[line_start..idx]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutCteBracket;
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
    fn flags_closing_paren_after_sql_code_in_multiline_cte() {
        let issues = run("with cte_1 as (\n    select foo\n    from tbl_1) select * from cte_1");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_007);
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
        assert!(has_misplaced_cte_closing_bracket(
            sql,
            Dialect::Generic,
            None
        ));
    }
}
