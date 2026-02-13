//! LINT_LT_009: Layout select targets.
//!
//! SQLFluff LT09 parity (current scope): require multi-target SELECT lists to
//! be line-broken.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer};

pub struct LayoutSelectTargets;

impl LintRule for LayoutSelectTargets {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_009
    }

    fn name(&self) -> &'static str {
        "Layout select targets"
    }

    fn description(&self) -> &'static str {
        "Select targets should be on a new line unless there is only one target."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        lt09_violation_spans(ctx.statement_sql())
            .into_iter()
            .map(|(start, end)| {
                Issue::info(
                    issue_codes::LINT_LT_009,
                    "Select targets should be on a new line unless there is only one target.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
            })
            .collect()
    }
}

fn select_line_top_level_comma_count(segment: &str) -> usize {
    let mut count = 0usize;
    let mut depth = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let bytes = segment.as_bytes();
    let mut idx = 0usize;

    while idx < bytes.len() {
        let b = bytes[idx];

        if in_single {
            if b == b'\'' {
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'\'' {
                    idx += 2;
                } else {
                    in_single = false;
                    idx += 1;
                }
            } else {
                idx += 1;
            }
            continue;
        }

        if in_double {
            if b == b'"' {
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'"' {
                    idx += 2;
                } else {
                    in_double = false;
                    idx += 1;
                }
            } else {
                idx += 1;
            }
            continue;
        }

        match b {
            b'\'' => in_single = true,
            b'"' => in_double = true,
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
            }
            b',' if depth == 0 => count += 1,
            _ => {}
        }

        idx += 1;
    }

    count
}

fn lt09_violation_spans(sql: &str) -> Vec<(usize, usize)> {
    let dialect = sqlparser::dialect::GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return Vec::new();
    };

    let mut spans = Vec::new();

    for token in tokens {
        let Token::Word(word) = token.token else {
            continue;
        };
        if word.keyword != Keyword::SELECT {
            continue;
        }

        let Some(start) = line_col_to_offset(
            sql,
            token.span.start.line as usize,
            token.span.start.column as usize,
        ) else {
            continue;
        };
        let Some(end) = line_col_to_offset(
            sql,
            token.span.end.line as usize,
            token.span.end.column as usize,
        ) else {
            continue;
        };

        let line_end = sql[end..].find('\n').map_or(sql.len(), |off| end + off);
        let select_tail = &sql[end..line_end];

        if select_line_top_level_comma_count(select_tail) > 0 {
            spans.push((start, end));
        }
    }

    spans
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
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutSelectTargets;
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
    fn flags_multiple_targets_on_same_select_line() {
        assert!(!run("SELECT a,b,c,d,e FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_single_target() {
        assert!(run("SELECT a FROM t").is_empty());
    }

    #[test]
    fn flags_each_select_line_with_multiple_targets() {
        let issues = run("SELECT a, b FROM t UNION ALL SELECT c, d FROM t");
        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_009)
                .count(),
            2,
        );
    }

    #[test]
    fn does_not_flag_select_word_inside_single_quoted_string() {
        assert!(run("SELECT 'SELECT a, b' AS txt").is_empty());
    }
}
