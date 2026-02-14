//! LINT_LT_015: Layout newlines.
//!
//! SQLFluff LT15 parity (current scope): detect excessive blank lines.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Location, Span, Token, TokenWithSpan, Tokenizer, Whitespace};
use std::ops::Range;

pub struct LayoutNewlines {
    maximum_empty_lines_inside_statements: usize,
    maximum_empty_lines_between_statements: usize,
}

impl LayoutNewlines {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            maximum_empty_lines_inside_statements: config
                .rule_option_usize(
                    issue_codes::LINT_LT_015,
                    "maximum_empty_lines_inside_statements",
                )
                .unwrap_or(1),
            maximum_empty_lines_between_statements: config
                .rule_option_usize(
                    issue_codes::LINT_LT_015,
                    "maximum_empty_lines_between_statements",
                )
                .unwrap_or(1),
        }
    }
}

impl Default for LayoutNewlines {
    fn default() -> Self {
        Self {
            maximum_empty_lines_inside_statements: 1,
            maximum_empty_lines_between_statements: 1,
        }
    }
}

impl LintRule for LayoutNewlines {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_015
    }

    fn name(&self) -> &'static str {
        "Layout newlines"
    }

    fn description(&self) -> &'static str {
        "Avoid excessive blank lines."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let (inside_range, statement_sql) = trimmed_statement_range_and_sql(ctx);
        let inside_tokens = tokenized_for_range(ctx, inside_range);
        let excessive_inside =
            max_consecutive_blank_lines(statement_sql, ctx.dialect(), inside_tokens.as_deref())
                > self.maximum_empty_lines_inside_statements;

        let excessive_between = if ctx.statement_index > 0 {
            let gap_range = inter_statement_gap_range(ctx.sql, ctx.statement_range.start);
            let gap_sql = &ctx.sql[gap_range.clone()];
            let gap_tokens = tokenized_for_range(ctx, gap_range);
            max_consecutive_blank_lines(gap_sql, ctx.dialect(), gap_tokens.as_deref())
                > self.maximum_empty_lines_between_statements
        } else {
            false
        };

        if excessive_inside || excessive_between {
            vec![Issue::info(
                issue_codes::LINT_LT_015,
                "SQL contains excessive blank lines.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn trimmed_statement_range_and_sql<'a>(ctx: &'a LintContext) -> (Range<usize>, &'a str) {
    let statement_sql = ctx.statement_sql();
    let mut start = 0usize;
    while start < statement_sql.len() && statement_sql.as_bytes()[start].is_ascii_whitespace() {
        start += 1;
    }

    let mut end = statement_sql.len();
    while end > start && statement_sql.as_bytes()[end - 1].is_ascii_whitespace() {
        end -= 1;
    }

    (
        (ctx.statement_range.start + start)..(ctx.statement_range.start + end),
        &statement_sql[start..end],
    )
}

fn max_consecutive_blank_lines(
    sql: &str,
    dialect: Dialect,
    tokens: Option<&[TokenWithSpan]>,
) -> usize {
    max_consecutive_blank_lines_tokenized(sql, dialect, tokens)
}

fn max_consecutive_blank_lines_tokenized(
    sql: &str,
    dialect: Dialect,
    tokens: Option<&[TokenWithSpan]>,
) -> usize {
    if sql.is_empty() {
        return 0;
    }

    let owned_tokens;
    let tokens = if let Some(tokens) = tokens {
        tokens
    } else {
        owned_tokens = match tokenized(sql, dialect) {
            Some(tokens) => tokens,
            None => return 0,
        };
        &owned_tokens
    };

    let mut non_blank_lines = std::collections::BTreeSet::new();
    for token in tokens {
        if is_spacing_whitespace_token(&token.token) {
            continue;
        }
        let start_line = token.span.start.line as usize;
        let end_line = match &token.token {
            Token::Whitespace(Whitespace::SingleLineComment { .. }) => start_line,
            _ => token.span.end.line as usize,
        };
        for line in start_line..=end_line {
            non_blank_lines.insert(line);
        }
    }

    let mut blank_run = 0usize;
    let mut max_run = 0usize;
    let line_count = sql.lines().count();

    for line in 1..=line_count {
        if non_blank_lines.contains(&line) {
            blank_run = 0;
        } else {
            blank_run += 1;
            max_run = max_run.max(blank_run);
        }
    }

    max_run
}

fn is_spacing_whitespace_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
    )
}

fn inter_statement_gap_range(sql: &str, statement_start: usize) -> Range<usize> {
    let before = &sql[..statement_start];
    let boundary = before
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_ascii_whitespace())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);
    boundary..statement_start
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn tokenized_for_range(ctx: &LintContext, range: Range<usize>) -> Option<Vec<TokenWithSpan>> {
    if range.is_empty() {
        return Some(Vec::new());
    }

    let (range_start_line, range_start_column) = offset_to_line_col(ctx.sql, range.start)?;
    ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut out = Vec::new();
        for token in tokens {
            let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                continue;
            };
            if start < range.start || end > range.end {
                continue;
            }

            let Some(start_loc) =
                relative_location(token.span.start, range_start_line, range_start_column)
            else {
                continue;
            };
            let Some(end_loc) =
                relative_location(token.span.end, range_start_line, range_start_column)
            else {
                continue;
            };

            out.push(TokenWithSpan::new(
                token.token.clone(),
                Span::new(start_loc, end_loc),
            ));
        }

        Some(out)
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
    range_start_line: usize,
    range_start_column: usize,
) -> Option<Location> {
    let line = location.line as usize;
    let column = location.column as usize;
    if line < range_start_line {
        return None;
    }

    if line == range_start_line {
        if column < range_start_column {
            return None;
        }
        return Some(Location::new(1, (column - range_start_column + 1) as u64));
    }

    Some(Location::new(
        (line - range_start_line + 1) as u64,
        column as u64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;

    fn run_with_rule(sql: &str, rule: &LayoutNewlines) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let mut ranges = Vec::with_capacity(statements.len());
        let mut search_start = 0usize;
        for index in 0..statements.len() {
            if index > 0 {
                search_start = first_non_whitespace_offset(sql, search_start);
            }
            let end = if index + 1 < statements.len() {
                sql[search_start..]
                    .find(';')
                    .map(|offset| search_start + offset + 1)
                    .unwrap_or(sql.len())
            } else {
                sql.len()
            };
            ranges.push(search_start..end);
            search_start = end;
        }

        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check(
                    statement,
                    &LintContext {
                        sql,
                        statement_range: ranges[index].clone(),
                        statement_index: index,
                    },
                )
            })
            .collect()
    }

    fn first_non_whitespace_offset(sql: &str, from: usize) -> usize {
        let mut offset = from;
        for ch in sql[from..].chars() {
            if ch.is_ascii_whitespace() {
                offset += ch.len_utf8();
            } else {
                break;
            }
        }
        offset
    }

    fn run(sql: &str) -> Vec<Issue> {
        run_with_rule(sql, &LayoutNewlines::default())
    }

    #[test]
    fn flags_excessive_blank_lines() {
        let issues = run("SELECT 1\n\n\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }

    #[test]
    fn does_not_flag_single_blank_line() {
        assert!(run("SELECT 1\n\nFROM t").is_empty());
    }

    #[test]
    fn flags_blank_lines_with_whitespace() {
        let issues = run("SELECT 1\n\n   \nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }

    #[test]
    fn configured_inside_limit_allows_two_blank_lines() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.newlines".to_string(),
                serde_json::json!({"maximum_empty_lines_inside_statements": 2}),
            )]),
        };
        let issues = run_with_rule(
            "SELECT 1\n\n\nFROM t",
            &LayoutNewlines::from_config(&config),
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn configured_between_limit_flags_statement_gap() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_LT_015".to_string(),
                serde_json::json!({"maximum_empty_lines_between_statements": 1}),
            )]),
        };
        let issues = run_with_rule(
            "SELECT 1;\n\n\nSELECT 2",
            &LayoutNewlines::from_config(&config),
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }

    #[test]
    fn flags_blank_lines_after_inline_comment() {
        let issues = run("SELECT 1 -- inline\n\n\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }

    #[test]
    fn flags_blank_lines_between_statements_with_comment_gap() {
        let issues = run("SELECT 1;\n-- there was a comment\n\n\nSELECT 2");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }
}
