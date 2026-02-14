//! LINT_LT_014: Layout keyword newline.
//!
//! SQLFluff LT14 parity (current scope): detect inconsistent major-clause
//! keyword placement relative to the SELECT line.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

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

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let Some((keyword_start, keyword_end)) =
            keyword_newline_violation_span(ctx.statement_sql(), ctx.dialect())
        else {
            return Vec::new();
        };

        vec![Issue::info(
            issue_codes::LINT_LT_014,
            "Major clauses should be consistently line-broken.",
        )
        .with_statement(ctx.statement_index)
        .with_span(ctx.span_from_statement_offset(keyword_start, keyword_end))]
    }
}

#[derive(Clone, Copy)]
struct ClauseOccurrence {
    line: u64,
    start: usize,
    end: usize,
}

fn keyword_newline_violation_span(sql: &str, dialect: Dialect) -> Option<(usize, usize)> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let select_line = tokens.iter().find_map(|token| {
        let Token::Word(word) = &token.token else {
            return None;
        };
        if word.keyword != Keyword::SELECT {
            return None;
        }

        let select_start = line_col_to_offset(
            sql,
            token.span.start.line as usize,
            token.span.start.column as usize,
        )?;
        let line_start = sql[..select_start].rfind('\n').map_or(0, |idx| idx + 1);
        sql[line_start..select_start]
            .trim()
            .is_empty()
            .then_some(token.span.start.line)
    })?;

    let clauses = major_clause_occurrences(sql, &tokens)?;

    let mut clauses_on_select_line = clauses.iter().filter(|clause| clause.line == select_line);
    let first_clause_on_select_line = clauses_on_select_line.next()?;

    let has_second_clause_on_select_line = clauses_on_select_line.next().is_some();
    let has_major_clause_on_later_line = clauses.iter().any(|clause| clause.line > select_line);

    if !has_second_clause_on_select_line && !has_major_clause_on_later_line {
        return None;
    }

    Some((
        first_clause_on_select_line.start,
        first_clause_on_select_line.end,
    ))
}

fn major_clause_occurrences(sql: &str, tokens: &[TokenWithSpan]) -> Option<Vec<ClauseOccurrence>> {
    let significant: Vec<&TokenWithSpan> = tokens
        .iter()
        .filter(|token| !is_trivia_token(&token.token))
        .collect();

    let mut out = Vec::new();
    let mut index = 0usize;

    while index < significant.len() {
        let token = significant[index];
        let Token::Word(word) = &token.token else {
            index += 1;
            continue;
        };

        match word.keyword {
            Keyword::FROM | Keyword::WHERE => {
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
                out.push(ClauseOccurrence {
                    line: token.span.start.line,
                    start,
                    end,
                });
                index += 1;
            }
            Keyword::GROUP | Keyword::ORDER => {
                let Some(next) = significant.get(index + 1) else {
                    index += 1;
                    continue;
                };

                let is_by = matches!(&next.token, Token::Word(next_word) if next_word.keyword == Keyword::BY);
                if !is_by {
                    index += 1;
                    continue;
                }

                let start = line_col_to_offset(
                    sql,
                    token.span.start.line as usize,
                    token.span.start.column as usize,
                )?;
                let end = line_col_to_offset(
                    sql,
                    next.span.end.line as usize,
                    next.span.end.column as usize,
                )?;
                out.push(ClauseOccurrence {
                    line: token.span.start.line,
                    start,
                    end,
                });
                index += 2;
            }
            _ => index += 1,
        }
    }

    Some(out)
}

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutKeywordNewline;
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
    fn flags_inconsistent_major_clause_placement() {
        assert!(!run("SELECT a FROM t WHERE a = 1").is_empty());
        assert!(!run("SELECT a FROM t\nWHERE a = 1").is_empty());
    }

    #[test]
    fn does_not_flag_consistent_layout() {
        assert!(run("SELECT a FROM t").is_empty());
        assert!(run("SELECT a\nFROM t\nWHERE a = 1").is_empty());
    }

    #[test]
    fn does_not_flag_clause_words_in_string_literal() {
        assert!(run("SELECT 'FROM t WHERE x = 1' AS txt").is_empty());
    }
}
