//! LINT_TQ_003: TSQL empty batch.
//!
//! SQLFluff TQ03 parity (current scope): detect empty batches between repeated
//! `GO` separators.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Location, Span, Token, TokenWithSpan, Tokenizer};

pub struct TsqlEmptyBatch;

impl LintRule for TsqlEmptyBatch {
    fn code(&self) -> &'static str {
        issue_codes::LINT_TQ_003
    }

    fn name(&self) -> &'static str {
        "TSQL empty batch"
    }

    fn description(&self) -> &'static str {
        "Avoid empty TSQL batches between GO separators."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let tokens = tokenized_for_context(ctx);
        let has_violation =
            has_empty_go_batch_separator(ctx.statement_sql(), ctx.dialect(), tokens.as_deref());
        if has_violation {
            vec![Issue::warning(
                issue_codes::LINT_TQ_003,
                "Empty TSQL batch detected between GO separators.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn has_empty_go_batch_separator(
    sql: &str,
    dialect: Dialect,
    tokens: Option<&[TokenWithSpan]>,
) -> bool {
    let owned_tokens;
    let tokens = if let Some(tokens) = tokens {
        tokens
    } else {
        owned_tokens = match tokenized(sql, dialect) {
            Some(tokens) => tokens,
            None => return false,
        };
        &owned_tokens
    };

    let mut go_lines = Vec::new();
    for token in tokens {
        let Token::Word(word) = &token.token else {
            continue;
        };
        if !word.value.eq_ignore_ascii_case("GO") {
            continue;
        }

        let line = token.span.start.line as usize;
        if line_is_go_separator(sql, line) {
            go_lines.push(line);
        }
    }

    if go_lines.len() < 2 {
        return false;
    }

    go_lines.sort_unstable();
    go_lines.dedup();

    go_lines
        .windows(2)
        .any(|pair| lines_between_are_empty(sql, pair[0], pair[1]))
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn tokenized_for_context(ctx: &LintContext) -> Option<Vec<TokenWithSpan>> {
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

fn line_is_go_separator(sql: &str, line_number: usize) -> bool {
    line_text(sql, line_number).is_some_and(|line| line.trim().eq_ignore_ascii_case("GO"))
}

fn lines_between_are_empty(sql: &str, first_line: usize, second_line: usize) -> bool {
    if second_line <= first_line {
        return false;
    }

    if second_line == first_line + 1 {
        return true;
    }

    ((first_line + 1)..second_line)
        .all(|line_number| line_text(sql, line_number).is_some_and(|line| line.trim().is_empty()))
}

fn line_text(sql: &str, line_number: usize) -> Option<&str> {
    sql.lines().nth(line_number.saturating_sub(1))
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

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = TsqlEmptyBatch;
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
    fn detects_repeated_go_separator_lines() {
        assert!(has_empty_go_batch_separator(
            "GO\nGO\n",
            Dialect::Generic,
            None
        ));
        assert!(has_empty_go_batch_separator(
            "GO\n\nGO\n",
            Dialect::Generic,
            None
        ));
    }

    #[test]
    fn does_not_detect_single_go_separator_line() {
        assert!(!has_empty_go_batch_separator(
            "GO\n",
            Dialect::Generic,
            None
        ));
    }

    #[test]
    fn does_not_detect_go_text_inside_string_literal() {
        assert!(!has_empty_go_batch_separator(
            "SELECT '\nGO\nGO\n' AS sql_snippet",
            Dialect::Generic,
            None,
        ));
    }

    #[test]
    fn rule_does_not_flag_go_text_inside_string_literal() {
        let issues = run("SELECT '\nGO\nGO\n' AS sql_snippet");
        assert!(issues.is_empty());
    }
}
