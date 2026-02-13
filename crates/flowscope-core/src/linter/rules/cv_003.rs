//! LINT_CV_003: Select trailing comma.
//!
//! Avoid trailing comma before FROM.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

pub struct ConventionSelectTrailingComma;

impl LintRule for ConventionSelectTrailingComma {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_003
    }

    fn name(&self) -> &'static str {
        "Select trailing comma"
    }

    fn description(&self) -> &'static str {
        "Trailing commas within select clause."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if has_select_trailing_comma(ctx.statement_sql()) {
            vec![Issue::warning(
                issue_codes::LINT_CV_003,
                "Avoid trailing comma before FROM in SELECT clause.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SignificantToken {
    Comma,
    Other,
}

#[derive(Clone, Copy)]
struct SelectClauseState {
    depth: usize,
    last_significant: Option<SignificantToken>,
}

fn has_select_trailing_comma(sql: &str) -> bool {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return false;
    };

    let mut depth = 0usize;
    let mut select_stack: Vec<SelectClauseState> = Vec::new();

    for token in tokens {
        if is_trivia_token(&token) {
            continue;
        }

        let significant = match token {
            Token::Comma => Some(SignificantToken::Comma),
            _ => Some(SignificantToken::Other),
        };

        if let Token::Word(ref word) = token {
            if word.keyword == Keyword::SELECT {
                select_stack.push(SelectClauseState {
                    depth,
                    last_significant: None,
                });
                continue;
            }
        }

        if let Token::Word(ref word) = token {
            if word.keyword == Keyword::FROM {
                if let Some(state) = select_stack.last_mut() {
                    if state.depth == depth {
                        if state.last_significant == Some(SignificantToken::Comma) {
                            return true;
                        }
                        select_stack.pop();
                        continue;
                    }
                }
            }
        }

        if let Some(state) = select_stack.last_mut() {
            if state.depth == depth {
                state.last_significant = significant;
            }
        }

        match token {
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            _ => {}
        }

        while let Some(state) = select_stack.last() {
            if state.depth > depth {
                select_stack.pop();
            } else {
                break;
            }
        }
    }

    false
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
        let stmts = parse_sql(sql).expect("parse");
        let rule = ConventionSelectTrailingComma;
        stmts
            .iter()
            .enumerate()
            .flat_map(|(index, stmt)| {
                rule.check(
                    stmt,
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
    fn flags_trailing_comma_before_from() {
        let issues = run("select a, from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_003);
    }

    #[test]
    fn allows_no_trailing_comma() {
        let issues = run("select a from t");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_nested_select_trailing_comma() {
        let issues = run("SELECT (SELECT a, FROM t) AS x FROM outer_t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_003);
    }

    #[test]
    fn does_not_flag_comma_in_string_or_comment() {
        let issues = run("SELECT 'a, from t' AS txt -- select a, from t\nFROM t");
        assert!(issues.is_empty());
    }
}
