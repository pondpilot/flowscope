//! LINT_LT_011: Layout set operators.
//!
//! SQLFluff LT11 parity (current scope): enforce own-line placement for set
//! operators in multiline statements.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct LayoutSetOperators;

impl LintRule for LayoutSetOperators {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_011
    }

    fn name(&self) -> &'static str {
        "Layout set operators"
    }

    fn description(&self) -> &'static str {
        "Set operators should be consistently line-broken."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if has_inline_set_operator_in_multiline_statement(ctx.statement_sql()) {
            vec![Issue::info(
                issue_codes::LINT_LT_011,
                "Set operators should be on their own line in multiline queries.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn has_inline_set_operator_in_multiline_statement(sql: &str) -> bool {
    if !sql.contains('\n') {
        return false;
    }

    let dialect = sqlparser::dialect::GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return false;
    };

    let significant_tokens: Vec<(usize, &TokenWithSpan)> = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| !is_trivia_token(&token.token))
        .collect();

    let has_set_operator = significant_tokens
        .iter()
        .any(|(_, token)| set_operator_keyword(&token.token).is_some());
    if !has_set_operator {
        return false;
    }

    for (position, (_, token)) in significant_tokens.iter().enumerate() {
        let Some(keyword) = set_operator_keyword(&token.token) else {
            continue;
        };

        let line = token.span.start.line;
        let line_positions: Vec<usize> = significant_tokens
            .iter()
            .enumerate()
            .filter_map(|(idx, (_, t))| (t.span.start.line == line).then_some(idx))
            .collect();

        let is_union_all_own_line = keyword == Keyword::UNION
            && line_positions.len() == 2
            && line_positions[0] == position
            && line_positions[1] == position + 1
            && matches!(
                significant_tokens.get(position + 1).map(|(_, t)| &t.token),
                Some(Token::Word(word)) if word.keyword == Keyword::ALL
            );

        let is_single_operator_own_line =
            line_positions.len() == 1 && line_positions[0] == position;

        if !is_single_operator_own_line && !is_union_all_own_line {
            return true;
        }
    }

    false
}

fn set_operator_keyword(token: &Token) -> Option<Keyword> {
    let Token::Word(word) = token else {
        return None;
    };

    match word.keyword {
        Keyword::UNION | Keyword::INTERSECT | Keyword::EXCEPT => Some(word.keyword),
        _ => None,
    }
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
        let rule = LayoutSetOperators;
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
    fn flags_inline_set_operator_in_multiline_statement() {
        let issues = run("SELECT 1 UNION SELECT 2\nUNION SELECT 3");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_011);
    }

    #[test]
    fn does_not_flag_own_line_set_operators() {
        let issues = run("SELECT 1\nUNION\nSELECT 2\nUNION\nSELECT 3");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_own_line_union_all() {
        let issues = run("SELECT 1\nUNION ALL\nSELECT 2");
        assert!(issues.is_empty());
    }
}
