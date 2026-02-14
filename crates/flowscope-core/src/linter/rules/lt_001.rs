//! LINT_LT_001: Layout spacing.
//!
//! SQLFluff LT01 parity (current scope): detect compact operator-style patterns
//! where spacing is expected.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct LayoutSpacing;

impl LintRule for LayoutSpacing {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_001
    }

    fn name(&self) -> &'static str {
        "Layout spacing"
    }

    fn description(&self) -> &'static str {
        "Operator spacing should be consistent."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        spacing_violation_spans(ctx.statement_sql(), ctx.dialect())
            .into_iter()
            .map(|(start, end)| {
                Issue::info(
                    issue_codes::LINT_LT_001,
                    "Operator spacing appears inconsistent.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
            })
            .collect()
    }
}

fn spacing_violation_spans(sql: &str, dialect: Dialect) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let Some(tokens) = tokenized(sql, dialect) else {
        return spans;
    };

    collect_json_arrow_spacing_violations(sql, &tokens, &mut spans);
    collect_compact_text_bracket_violations(sql, &tokens, &mut spans);
    collect_compact_numeric_scale_violations(sql, &tokens, &mut spans);
    collect_exists_line_paren_violations(sql, &tokens, &mut spans);

    spans.sort_unstable();
    spans.dedup();

    spans
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn collect_json_arrow_spacing_violations(
    sql: &str,
    tokens: &[TokenWithSpan],
    spans: &mut Vec<(usize, usize)>,
) {
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(token.token, Token::Arrow | Token::LongArrow) {
            continue;
        }

        let Some(prev_index) = prev_non_trivia_index(tokens, index) else {
            continue;
        };
        let Some(next_index) = next_non_trivia_index(tokens, index + 1) else {
            continue;
        };

        if !has_trivia_between(tokens, prev_index, index) {
            if let Some((start, _)) = token_offsets(sql, token) {
                spans.push(single_char_span(sql, start));
            }
        }

        if !has_trivia_between(tokens, index, next_index) {
            if let Some((start, _)) = token_offsets(sql, &tokens[next_index]) {
                spans.push(single_char_span(sql, start));
            }
        }
    }
}

fn collect_compact_text_bracket_violations(
    sql: &str,
    tokens: &[TokenWithSpan],
    spans: &mut Vec<(usize, usize)>,
) {
    for (index, token) in tokens.iter().enumerate() {
        let Token::Word(word) = &token.token else {
            continue;
        };
        if word.quote_style.is_some() || !word.value.eq_ignore_ascii_case("text") {
            continue;
        }

        let Some(next_index) = next_non_trivia_index(tokens, index + 1) else {
            continue;
        };
        if !matches!(tokens[next_index].token, Token::LBracket) {
            continue;
        }
        if has_trivia_between(tokens, index, next_index) {
            continue;
        }

        if let Some((start, _)) = token_offsets(sql, &tokens[next_index]) {
            spans.push(single_char_span(sql, start));
        }
    }
}

fn collect_compact_numeric_scale_violations(
    sql: &str,
    tokens: &[TokenWithSpan],
    spans: &mut Vec<(usize, usize)>,
) {
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(token.token, Token::Comma) {
            continue;
        }

        let Some(prev_index) = prev_non_trivia_index(tokens, index) else {
            continue;
        };
        let Some(next_index) = next_non_trivia_index(tokens, index + 1) else {
            continue;
        };
        if !matches!(tokens[prev_index].token, Token::Number(_, _))
            || !matches!(tokens[next_index].token, Token::Number(_, _))
        {
            continue;
        }
        if has_trivia_between(tokens, index, next_index) {
            continue;
        }

        let Some(before_prev_index) = prev_non_trivia_index(tokens, prev_index) else {
            continue;
        };
        let Some(after_next_index) = next_non_trivia_index(tokens, next_index + 1) else {
            continue;
        };
        if !matches!(tokens[before_prev_index].token, Token::LParen)
            || !matches!(tokens[after_next_index].token, Token::RParen)
        {
            continue;
        }

        if let Some((start, _)) = token_offsets(sql, &tokens[next_index]) {
            spans.push(single_char_span(sql, start));
        }
    }
}

fn collect_exists_line_paren_violations(
    sql: &str,
    tokens: &[TokenWithSpan],
    spans: &mut Vec<(usize, usize)>,
) {
    for (index, token) in tokens.iter().enumerate() {
        let Token::Word(word) = &token.token else {
            continue;
        };
        if word.keyword != Keyword::EXISTS {
            continue;
        }

        let Some(next_index) = next_non_trivia_index(tokens, index + 1) else {
            continue;
        };
        if !matches!(tokens[next_index].token, Token::LParen) {
            continue;
        }
        if !has_trivia_between(tokens, index, next_index) {
            continue;
        }

        if previous_line_ends_with_boolean_keyword(tokens, index) {
            continue;
        }

        let Some((exists_start, _)) = token_offsets(sql, token) else {
            continue;
        };
        if !line_prefix_is_whitespace(sql, exists_start) {
            continue;
        }

        if let Some((paren_start, _)) = token_offsets(sql, &tokens[next_index]) {
            spans.push(single_char_span(sql, paren_start));
        }
    }
}

fn previous_line_ends_with_boolean_keyword(tokens: &[TokenWithSpan], index: usize) -> bool {
    let Some(prev_index) = prev_non_trivia_index(tokens, index) else {
        return false;
    };
    let Token::Word(prev_word) = &tokens[prev_index].token else {
        return false;
    };
    if !matches!(
        prev_word.keyword,
        Keyword::AND | Keyword::OR | Keyword::NOT
    ) {
        return false;
    }

    tokens[prev_index].span.end.line < tokens[index].span.start.line
}

fn line_prefix_is_whitespace(sql: &str, offset: usize) -> bool {
    let line_start = sql[..offset].rfind('\n').map_or(0, |index| index + 1);
    sql[line_start..offset].chars().all(char::is_whitespace)
}

fn token_offsets(sql: &str, token: &TokenWithSpan) -> Option<(usize, usize)> {
    let start = line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )?;
    let end = line_col_to_offset(sql, token.span.end.line as usize, token.span.end.column as usize)?;
    Some((start, end))
}

fn single_char_span(sql: &str, start: usize) -> (usize, usize) {
    let end = (start + 1).min(sql.len());
    (start, end)
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

fn has_trivia_between(tokens: &[TokenWithSpan], left: usize, right: usize) -> bool {
    right > left + 1 && tokens[left + 1..right].iter().any(|token| is_trivia_token(&token.token))
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
        let rule = LayoutSpacing;
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
    fn does_not_flag_simple_spacing() {
        assert!(run("SELECT * FROM t WHERE a = 1").is_empty());
    }

    #[test]
    fn flags_compact_json_arrow_operator() {
        let issues = run("SELECT payload->>'id' FROM t");
        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_001)
                .count(),
            2,
        );
    }

    #[test]
    fn flags_compact_type_bracket_and_numeric_scale() {
        assert!(!run("SELECT ARRAY['x']::text[]").is_empty());
        assert!(!run("SELECT 1::numeric(5,2)").is_empty());
    }

    #[test]
    fn flags_exists_parenthesis_layout_case() {
        let issues = run("SELECT\n    EXISTS (\n        SELECT 1\n    ) AS has_row");
        assert!(!issues.is_empty());
    }

    #[test]
    fn does_not_flag_spacing_patterns_inside_literals_or_comments() {
        let issues = run("SELECT 'payload->>''id''' AS txt -- EXISTS (\nFROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_regular_comma_separated_projection() {
        assert!(run("SELECT 1,2").is_empty());
    }
}
