//! LINT_LT_010: Layout select modifiers.
//!
//! SQLFluff LT10 parity (current scope): detect multiline SELECT modifiers in
//! inconsistent positions.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

pub struct LayoutSelectModifiers;

impl LintRule for LayoutSelectModifiers {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_010
    }

    fn name(&self) -> &'static str {
        "Layout select modifiers"
    }

    fn description(&self) -> &'static str {
        "SELECT modifiers should be placed consistently."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if has_multiline_select_modifier(ctx.statement_sql(), ctx.dialect()) {
            vec![Issue::info(
                issue_codes::LINT_LT_010,
                "SELECT modifiers (DISTINCT/ALL) should be consistently formatted.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn has_multiline_select_modifier(sql: &str, dialect: Dialect) -> bool {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return false;
    };

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
            return true;
        }
    }

    false
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutSelectModifiers;
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
    fn flags_distinct_on_next_line() {
        let issues = run("SELECT\nDISTINCT a\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_010);
    }

    #[test]
    fn does_not_flag_single_line_modifier() {
        assert!(run("SELECT DISTINCT a FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_modifier_text_in_string() {
        assert!(run("SELECT 'SELECT\nDISTINCT a' AS txt").is_empty());
    }
}
