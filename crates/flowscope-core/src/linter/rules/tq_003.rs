//! LINT_TQ_003: TSQL empty batch.
//!
//! SQLFluff TQ03 parity (current scope): detect empty batches between repeated
//! `GO` separators.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::tokenizer::{Token, Tokenizer};

pub struct TsqlEmptyBatch;

impl LintRule for TsqlEmptyBatch {
    fn code(&self) -> &'static str {
        issue_codes::LINT_TQ_003
    }

    fn name(&self) -> &'static str {
        "TSQL empty batch"
    }

    fn description(&self) -> &'static str {
        "Avoid empty TSQL batches between GO separators."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_violation = has_empty_go_batch_separator(ctx.statement_sql());
        if has_violation {
            vec![Issue::warning(
                issue_codes::LINT_TQ_003,
                "Empty TSQL batch detected between GO separators.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn has_empty_go_batch_separator(sql: &str) -> bool {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return false;
    };

    let mut go_lines = Vec::new();
    for token in tokens {
        let Token::Word(word) = token.token else {
            continue;
        };
        if !word.value.eq_ignore_ascii_case("GO") {
            continue;
        }

        let line = token.span.start.line as usize;
        if line_is_go_separator(sql, line) {
            go_lines.push(line);
        }
    }

    if go_lines.len() < 2 {
        return false;
    }

    go_lines.sort_unstable();
    go_lines.dedup();

    go_lines
        .windows(2)
        .any(|pair| lines_between_are_empty(sql, pair[0], pair[1]))
}

fn line_is_go_separator(sql: &str, line_number: usize) -> bool {
    line_text(sql, line_number).is_some_and(|line| line.trim().eq_ignore_ascii_case("GO"))
}

fn lines_between_are_empty(sql: &str, first_line: usize, second_line: usize) -> bool {
    if second_line <= first_line {
        return false;
    }

    if second_line == first_line + 1 {
        return true;
    }

    ((first_line + 1)..second_line)
        .all(|line_number| line_text(sql, line_number).is_some_and(|line| line.trim().is_empty()))
}

fn line_text(sql: &str, line_number: usize) -> Option<&str> {
    sql.lines().nth(line_number.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = TsqlEmptyBatch;
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
    fn detects_repeated_go_separator_lines() {
        assert!(has_empty_go_batch_separator("GO\nGO\n"));
        assert!(has_empty_go_batch_separator("GO\n\nGO\n"));
    }

    #[test]
    fn does_not_detect_single_go_separator_line() {
        assert!(!has_empty_go_batch_separator("GO\n"));
    }

    #[test]
    fn does_not_detect_go_text_inside_string_literal() {
        assert!(!has_empty_go_batch_separator(
            "SELECT '\nGO\nGO\n' AS sql_snippet"
        ));
    }

    #[test]
    fn rule_does_not_flag_go_text_inside_string_literal() {
        let issues = run("SELECT '\nGO\nGO\n' AS sql_snippet");
        assert!(issues.is_empty());
    }
}
