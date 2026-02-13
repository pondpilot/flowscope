//! LINT_CP_003: Function capitalisation.
//!
//! SQLFluff CP03 parity (current scope): detect inconsistent function name
//! capitalisation.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

pub struct CapitalisationFunctions;

impl LintRule for CapitalisationFunctions {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_003
    }

    fn name(&self) -> &'static str {
        "Function capitalisation"
    }

    fn description(&self) -> &'static str {
        "Functions should use a consistent case style."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let functions = function_tokens(ctx.statement_sql());
        if functions.is_empty() {
            return Vec::new();
        }

        let preferred_style = functions
            .iter()
            .map(|name| case_style(name))
            .find(|style| *style == "lower" || *style == "upper")
            .unwrap_or("lower");

        let has_mismatch = functions.iter().any(|name| {
            let style = case_style(name);
            (style == "lower" || style == "upper" || style == "mixed") && style != preferred_style
        });

        if has_mismatch {
            vec![Issue::info(
                issue_codes::LINT_CP_003,
                "Function names use inconsistent capitalisation.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn case_style(token: &str) -> &'static str {
    if token.is_empty() {
        return "unknown";
    }
    if token == token.to_ascii_uppercase() {
        "upper"
    } else if token == token.to_ascii_lowercase() {
        "lower"
    } else if token
        .chars()
        .all(|ch| !ch.is_ascii_alphabetic() || ch.is_ascii_uppercase())
    {
        "upper"
    } else if token
        .chars()
        .all(|ch| !ch.is_ascii_alphabetic() || ch.is_ascii_lowercase())
    {
        "lower"
    } else {
        "mixed"
    }
}

fn function_tokens(sql: &str) -> Vec<String> {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return Vec::new();
    };

    let mut out = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        let Token::Word(word) = token else {
            continue;
        };

        if word.quote_style.is_some() {
            continue;
        }

        if is_non_function_word(word.value.as_str()) {
            continue;
        }

        let Some(next_index) = next_non_trivia_index(&tokens, index + 1) else {
            continue;
        };
        if !matches!(tokens[next_index], Token::LParen) {
            continue;
        }

        if let Some(prev_index) = prev_non_trivia_index(&tokens, index) {
            match &tokens[prev_index] {
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

        out.push(word.value.clone());
    }

    out
}

fn next_non_trivia_index(tokens: &[Token], mut index: usize) -> Option<usize> {
    while index < tokens.len() {
        if !is_trivia_token(&tokens[index]) {
            return Some(index);
        }
        index += 1;
    }
    None
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
            | "WHEN"
            | "WHERE"
            | "WINDOW"
    )
}

fn prev_non_trivia_index(tokens: &[Token], mut index: usize) -> Option<usize> {
    while index > 0 {
        index -= 1;
        if !is_trivia_token(&tokens[index]) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CapitalisationFunctions;
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
    fn flags_mixed_function_case() {
        let issues = run("SELECT COUNT(*), count(x) FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_003);
    }

    #[test]
    fn does_not_flag_consistent_function_case() {
        assert!(run("SELECT lower(x), upper(y) FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_function_like_text_in_strings_or_comments() {
        let sql = "SELECT 'COUNT(x) count(y)' AS txt -- COUNT(x)\nFROM t";
        assert!(run(sql).is_empty());
    }
}
