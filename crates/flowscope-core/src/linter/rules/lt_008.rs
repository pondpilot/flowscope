//! LINT_LT_008: Layout CTE newline.
//!
//! SQLFluff LT08 parity (current scope): require a blank line between CTE body
//! closing parenthesis and following query/CTE text.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct LayoutCteNewline;

impl LintRule for LayoutCteNewline {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_008
    }

    fn name(&self) -> &'static str {
        "Layout CTE newline"
    }

    fn description(&self) -> &'static str {
        "Blank line should separate CTE blocks from following code."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        lt08_violation_spans(statement, ctx)
            .into_iter()
            .map(|(start, end)| {
                Issue::info(
                    issue_codes::LINT_LT_008,
                    "Blank line expected but not found after CTE closing bracket.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
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

fn lt08_violation_spans(statement: &Statement, ctx: &LintContext) -> Vec<(usize, usize)> {
    let Statement::Query(query) = statement else {
        return Vec::new();
    };
    let Some(with_clause) = &query.with else {
        return Vec::new();
    };

    let Some(tokens) = tokenize_with_offsets(ctx.sql, ctx.dialect()) else {
        return Vec::new();
    };

    let mut spans = Vec::new();

    for cte in &with_clause.cte_tables {
        let Some(close_abs) = token_start_offset(ctx.sql, &cte.closing_paren_token.0) else {
            continue;
        };

        if close_abs < ctx.statement_range.start || close_abs >= ctx.statement_range.end {
            continue;
        }

        let (blank_lines, next_code_span) = suffix_summary_after_offset(
            ctx.sql,
            &tokens,
            close_abs + 1,
            ctx.statement_range.end,
        );

        if blank_lines == 0 {
            if let Some((next_start, next_end)) = next_code_span {
                spans.push((
                    next_start - ctx.statement_range.start,
                    next_end - ctx.statement_range.start,
                ));
            }
        }
    }

    spans
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
        let Some(end) =
            line_col_to_offset(sql, token.span.end.line as usize, token.span.end.column as usize)
        else {
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

fn token_start_offset(sql: &str, token: &TokenWithSpan) -> Option<usize> {
    line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )
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

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
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
        let rule = LayoutCteNewline;
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
    fn flags_missing_blank_line_after_cte() {
        assert!(!run("WITH cte AS (SELECT 1) SELECT * FROM cte").is_empty());
        assert!(!run("WITH cte AS (SELECT 1)\nSELECT * FROM cte").is_empty());
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
}
