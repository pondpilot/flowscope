//! LINT_CP_005: Type capitalisation.
//!
//! SQLFluff CP05 parity (current scope): detect mixed-case type names.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::tokenizer::{Token, Tokenizer};

pub struct CapitalisationTypes;

impl LintRule for CapitalisationTypes {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_005
    }

    fn name(&self) -> &'static str {
        "Type capitalisation"
    }

    fn description(&self) -> &'static str {
        "Type names should use a consistent case style."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if mixed_case_for_tokens(&type_tokens(ctx.statement_sql())) {
            vec![Issue::info(
                issue_codes::LINT_CP_005,
                "Type names use inconsistent capitalisation.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn type_tokens(sql: &str) -> Vec<String> {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return Vec::new();
    };

    tokens
        .into_iter()
        .filter_map(|token| match token {
            Token::Word(word)
                if word.quote_style.is_none() && is_tracked_type_name(word.value.as_str()) =>
            {
                Some(word.value)
            }
            _ => None,
        })
        .collect()
}

fn is_tracked_type_name(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "INT"
            | "INTEGER"
            | "BIGINT"
            | "SMALLINT"
            | "TINYINT"
            | "VARCHAR"
            | "CHAR"
            | "TEXT"
            | "BOOLEAN"
            | "BOOL"
            | "DATE"
            | "TIMESTAMP"
            | "NUMERIC"
            | "DECIMAL"
            | "FLOAT"
            | "DOUBLE"
    )
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
        let rule = CapitalisationTypes;
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
    fn flags_mixed_type_case() {
        let issues = run("CREATE TABLE t (a INT, b varchar(10))");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_005);
    }

    #[test]
    fn does_not_flag_consistent_type_case() {
        assert!(run("CREATE TABLE t (a int, b varchar(10))").is_empty());
    }

    #[test]
    fn does_not_flag_type_words_in_strings_or_comments() {
        let sql = "SELECT 'INT varchar BOOLEAN' AS txt -- INT varchar\nFROM t";
        assert!(run(sql).is_empty());
    }
}
