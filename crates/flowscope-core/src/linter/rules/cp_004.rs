//! LINT_CP_004: Literal capitalisation.
//!
//! SQLFluff CP04 parity (current scope): detect mixed-case usage for
//! NULL/TRUE/FALSE literal keywords.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::tokenizer::{Token, Tokenizer};

pub struct CapitalisationLiterals;

impl LintRule for CapitalisationLiterals {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_004
    }

    fn name(&self) -> &'static str {
        "Literal capitalisation"
    }

    fn description(&self) -> &'static str {
        "NULL/TRUE/FALSE should use a consistent case style."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if mixed_case_for_tokens(&literal_tokens(ctx.statement_sql())) {
            vec![Issue::info(
                issue_codes::LINT_CP_004,
                "Literal keywords (NULL/TRUE/FALSE) use inconsistent capitalisation.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn literal_tokens(sql: &str) -> Vec<String> {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return Vec::new();
    };

    tokens
        .into_iter()
        .filter_map(|token| match token {
            Token::Word(word)
                if matches!(
                    word.value.to_ascii_uppercase().as_str(),
                    "NULL" | "TRUE" | "FALSE"
                ) =>
            {
                Some(word.value)
            }
            _ => None,
        })
        .collect()
}

fn mixed_case_for_tokens(tokens: &[String]) -> bool {
    if tokens.len() < 2 {
        return false;
    }

    let mut saw_upper = false;
    let mut saw_lower = false;
    let mut saw_mixed = false;

    for token in tokens {
        let upper = token.to_ascii_uppercase();
        let lower = token.to_ascii_lowercase();
        if token == &upper {
            saw_upper = true;
        } else if token == &lower {
            saw_lower = true;
        } else {
            saw_mixed = true;
        }
    }

    saw_mixed || (saw_upper && saw_lower)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CapitalisationLiterals;
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
    fn flags_mixed_literal_case() {
        let issues = run("SELECT NULL, true FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_004);
    }

    #[test]
    fn does_not_flag_consistent_literal_case() {
        assert!(run("SELECT NULL, TRUE FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_literal_words_in_strings_or_comments() {
        let sql = "SELECT 'null true false' AS txt -- NULL true\nFROM t";
        assert!(run(sql).is_empty());
    }
}
