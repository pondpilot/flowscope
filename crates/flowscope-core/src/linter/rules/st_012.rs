//! LINT_ST_012: Structure consecutive semicolons.
//!
//! SQLFluff ST12 parity (current scope): detect consecutive semicolons in the
//! document text.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

pub struct StructureConsecutiveSemicolons;

impl LintRule for StructureConsecutiveSemicolons {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_012
    }

    fn name(&self) -> &'static str {
        "Structure consecutive semicolons"
    }

    fn description(&self) -> &'static str {
        "Avoid consecutive semicolons."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_violation = ctx.statement_index == 0 && sql_has_consecutive_semicolons(ctx.sql);
        if has_violation {
            vec![
                Issue::warning(issue_codes::LINT_ST_012, "Consecutive semicolons detected.")
                    .with_statement(ctx.statement_index),
            ]
        } else {
            Vec::new()
        }
    }
}

fn sql_has_consecutive_semicolons(sql: &str) -> bool {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return false;
    };

    let mut saw_previous_semicolon = false;

    for token in tokens {
        match token {
            Token::SemiColon => {
                if saw_previous_semicolon {
                    return true;
                }
                saw_previous_semicolon = true;
            }
            Token::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_)) => {}
            _ => saw_previous_semicolon = false,
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = StructureConsecutiveSemicolons;
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
    fn flags_consecutive_semicolons() {
        let issues = run("SELECT 1;;");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_012);
    }

    #[test]
    fn does_not_flag_single_semicolon() {
        let issues = run("SELECT 1;");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_semicolons_inside_string_literal() {
        let issues = run("SELECT 'a;;b';");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_normal_statement_separator() {
        let issues = run("SELECT 1; SELECT 2;");
        assert!(issues.is_empty());
    }
}
