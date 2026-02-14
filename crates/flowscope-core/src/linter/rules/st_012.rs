//! LINT_ST_012: Structure consecutive semicolons.
//!
//! SQLFluff ST12 parity (current scope): detect consecutive semicolons in the
//! document text.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
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
        let has_violation =
            ctx.statement_index == 0 && sql_has_consecutive_semicolons(ctx.sql, ctx.dialect());
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

fn sql_has_consecutive_semicolons(sql: &str, dialect: Dialect) -> bool {
    let Some(tokens) = tokenize_with_offsets(sql, dialect) else {
        return false;
    };

    let mut previous_semicolon_end = None;

    for token in tokens {
        if is_trivia_token(&token.token) {
            continue;
        }

        if matches!(token.token, Token::SemiColon) {
            if previous_semicolon_end.is_some_and(|previous_end| previous_end <= token.start) {
                return true;
            }
            previous_semicolon_end = Some(token.end);
        } else {
            previous_semicolon_end = None;
        }
    }

    false
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn tokenize_with_offsets(sql: &str, dialect: Dialect) -> Option<Vec<LocatedToken>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
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
        out.push(LocatedToken {
            token: token.token,
            start,
            end,
        });
    }

    Some(out)
}

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
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
    use crate::linter::rule::with_active_dialect;
    use crate::parser::{parse_sql, parse_sql_with_dialect};
    use crate::types::Dialect;

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

    fn run_in_dialect(sql: &str, dialect: Dialect) -> Vec<Issue> {
        let statements = parse_sql_with_dialect(sql, dialect).expect("parse");
        let rule = StructureConsecutiveSemicolons;
        let mut issues = Vec::new();

        with_active_dialect(dialect, || {
            for (index, statement) in statements.iter().enumerate() {
                issues.extend(rule.check(
                    statement,
                    &LintContext {
                        sql,
                        statement_range: 0..sql.len(),
                        statement_index: index,
                    },
                ));
            }
        });

        issues
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
    fn does_not_flag_semicolons_inside_comments() {
        let issues = run("SELECT 1 /* ;; */;");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_consecutive_semicolons_separated_by_comment() {
        let issues = run("SELECT 1; /* keep */ ;");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_012);
    }

    #[test]
    fn does_not_flag_normal_statement_separator() {
        let issues = run("SELECT 1; SELECT 2;");
        assert!(issues.is_empty());
    }

    #[test]
    fn mysql_hash_comment_is_treated_as_trivia() {
        let sql = "SELECT 1; # dialect-specific comment\n;";
        assert!(!sql_has_consecutive_semicolons(sql, Dialect::Generic));
        assert!(sql_has_consecutive_semicolons(sql, Dialect::Mysql));

        let issues = run_in_dialect(sql, Dialect::Mysql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_012);
    }
}
