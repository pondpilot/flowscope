//! LINT_LT_013: Layout start of file.
//!
//! SQLFluff LT13 parity (current scope): avoid leading blank lines.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

pub struct LayoutStartOfFile;

impl LintRule for LayoutStartOfFile {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_013
    }

    fn name(&self) -> &'static str {
        "Layout start of file"
    }

    fn description(&self) -> &'static str {
        "Avoid leading blank lines at file start."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_violation =
            ctx.statement_index == 0 && has_leading_blank_lines(ctx.sql, ctx.dialect());
        if has_violation {
            vec![Issue::info(
                issue_codes::LINT_LT_013,
                "Avoid leading blank lines at the start of SQL file.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn has_leading_blank_lines(sql: &str, dialect: Dialect) -> bool {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return false;
    };

    for token in tokens {
        match token {
            Token::Whitespace(Whitespace::Space | Whitespace::Tab) => continue,
            Token::Whitespace(Whitespace::Newline) => return true,
            Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_)) => return false,
            _ => return false,
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
        let rule = LayoutStartOfFile;
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
    fn flags_leading_blank_lines() {
        let issues = run("\n\nSELECT 1");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_013);
    }

    #[test]
    fn does_not_flag_clean_start() {
        assert!(run("SELECT 1").is_empty());
    }

    #[test]
    fn does_not_flag_leading_comment() {
        assert!(run("-- comment\nSELECT 1").is_empty());
    }

    #[test]
    fn flags_blank_line_before_comment() {
        let issues = run("  \n-- comment\nSELECT 1");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_013);
    }
}
