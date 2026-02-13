//! LINT_CP_002: Identifier capitalisation.
//!
//! SQLFluff CP02 parity (current scope): detect inconsistent identifier case.

use std::collections::HashSet;

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

pub struct CapitalisationIdentifiers;

impl LintRule for CapitalisationIdentifiers {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_002
    }

    fn name(&self) -> &'static str {
        "Identifier capitalisation"
    }

    fn description(&self) -> &'static str {
        "Identifiers should use a consistent case style."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let identifiers = identifier_tokens(ctx.statement_sql());

        let excluded_issues: Vec<Issue> = identifiers
            .iter()
            .filter(|ident| {
                ident.eq_ignore_ascii_case("EXCLUDED") && *ident != &ident.to_ascii_lowercase()
            })
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_CP_002,
                    "Identifiers use inconsistent capitalisation.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect();

        if !excluded_issues.is_empty() {
            return excluded_issues;
        }

        if !mixed_case_for_tokens(&identifiers) {
            return Vec::new();
        }

        vec![Issue::info(
            issue_codes::LINT_CP_002,
            "Identifiers use inconsistent capitalisation.",
        )
        .with_statement(ctx.statement_index)]
    }
}

fn identifier_tokens(sql: &str) -> Vec<String> {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return Vec::new();
    };

    let function_indices = function_token_indices(&tokens);

    tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            let Token::Word(word) = token else {
                return None;
            };

            if function_indices.contains(&index) {
                return None;
            }

            if word.quote_style.is_some() {
                return None;
            }

            if word.keyword != Keyword::NoKeyword && !word.value.eq_ignore_ascii_case("EXCLUDED") {
                return None;
            }

            Some(word.value.clone())
        })
        .collect()
}

fn function_token_indices(tokens: &[Token]) -> HashSet<usize> {
    let mut out = HashSet::new();

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

        let Some(next_index) = next_non_trivia_index(tokens, index + 1) else {
            continue;
        };
        if !matches!(tokens[next_index], Token::LParen) {
            continue;
        }

        if let Some(prev_index) = prev_non_trivia_index(tokens, index) {
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

        out.insert(index);
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

fn mixed_case_for_tokens(tokens: &[String]) -> bool {
    let mut styles = HashSet::new();
    for token in tokens {
        styles.insert(case_style(token));
    }
    styles.len() > 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CapitalisationIdentifiers;
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
    fn flags_mixed_identifier_case() {
        let issues = run("SELECT Col, col FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_002);
    }

    #[test]
    fn does_not_flag_consistent_identifiers() {
        assert!(run("SELECT col_one, col_two FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_identifier_like_words_in_strings_or_comments() {
        let sql = "SELECT 'Col col' AS txt -- Col col\nFROM t";
        assert!(run(sql).is_empty());
    }
}
