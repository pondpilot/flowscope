//! LINT_LT_006: Layout functions.
//!
//! SQLFluff LT06 parity (current scope): flag function-like tokens separated
//! from opening parenthesis.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct LayoutFunctions;

impl LintRule for LayoutFunctions {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_006
    }

    fn name(&self) -> &'static str {
        "Layout functions"
    }

    fn description(&self) -> &'static str {
        "Function call spacing should be consistent."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let Some((start, end)) = function_spacing_issue_span(ctx.statement_sql(), ctx.dialect())
        else {
            return Vec::new();
        };

        vec![Issue::info(
            issue_codes::LINT_LT_006,
            "Function call spacing appears inconsistent.",
        )
        .with_statement(ctx.statement_index)
        .with_span(ctx.span_from_statement_offset(start, end))]
    }
}

fn function_spacing_issue_span(sql: &str, dialect: Dialect) -> Option<(usize, usize)> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    for (index, token) in tokens.iter().enumerate() {
        let Token::Word(word) = &token.token else {
            continue;
        };

        if word.quote_style.is_some() || is_non_function_word(word.value.as_str()) {
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
            match &tokens[prev_index].token {
                Token::Period => continue,
                Token::Word(prev_word)
                    if matches!(
                        prev_word.keyword,
                        Keyword::INTO
                            | Keyword::FROM
                            | Keyword::JOIN
                            | Keyword::UPDATE
                            | Keyword::TABLE
                    ) =>
                {
                    continue;
                }
                _ => {}
            }
        }

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
        return Some((start, end));
    }

    None
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

fn is_non_function_word(word: &str) -> bool {
    matches!(
        word.to_ascii_uppercase().as_str(),
        "ALL"
            | "AND"
            | "ANY"
            | "AS"
            | "BETWEEN"
            | "BY"
            | "CASE"
            | "ELSE"
            | "END"
            | "EXISTS"
            | "FROM"
            | "GROUP"
            | "HAVING"
            | "IN"
            | "INTERSECT"
            | "IS"
            | "JOIN"
            | "LIKE"
            | "ILIKE"
            | "LIMIT"
            | "NOT"
            | "OFFSET"
            | "ON"
            | "OR"
            | "ORDER"
            | "OVER"
            | "PARTITION"
            | "SELECT"
            | "THEN"
            | "UNION"
            | "VALUES"
            | "WHEN"
            | "WHERE"
            | "WINDOW"
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
        let rule = LayoutFunctions;
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
}
