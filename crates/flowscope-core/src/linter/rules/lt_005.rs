//! LINT_LT_005: Layout long lines.
//!
//! SQLFluff LT05 parity (current scope): flag overflow beyond 80 columns.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue, Span};
use sqlparser::ast::Statement;

pub struct LayoutLongLines {
    max_line_length: usize,
}

impl LayoutLongLines {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            max_line_length: config
                .rule_option_usize(issue_codes::LINT_LT_005, "max_line_length")
                .unwrap_or(80),
        }
    }
}

impl Default for LayoutLongLines {
    fn default() -> Self {
        Self {
            max_line_length: 80,
        }
    }
}

impl LintRule for LayoutLongLines {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_005
    }

    fn name(&self) -> &'static str {
        "Layout long lines"
    }

    fn description(&self) -> &'static str {
        "Avoid excessively long SQL lines."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if ctx.statement_index != 0 {
            return Vec::new();
        }

        long_line_overflow_spans(ctx.sql, self.max_line_length)
            .into_iter()
            .map(|(start, end)| {
                Issue::info(
                    issue_codes::LINT_LT_005,
                    "SQL contains excessively long lines.",
                )
                .with_statement(ctx.statement_index)
                .with_span(Span::new(start, end))
            })
            .collect()
    }
}

fn long_line_overflow_spans(sql: &str, max_len: usize) -> Vec<(usize, usize)> {
    let bytes = sql.as_bytes();
    let mut spans = Vec::new();
    let mut line_start = 0usize;

    for idx in 0..=bytes.len() {
        if idx < bytes.len() && bytes[idx] != b'\n' {
            continue;
        }

        let mut line_end = idx;
        if line_end > line_start && bytes[line_end - 1] == b'\r' {
            line_end -= 1;
        }

        let line = &sql[line_start..line_end];
        if line.chars().count() > max_len {
            let mut overflow_start = line_end;
            for (char_idx, (byte_off, _)) in line.char_indices().enumerate() {
                if char_idx == max_len {
                    overflow_start = line_start + byte_off;
                    break;
                }
            }

            if overflow_start < line_end {
                let overflow_end = sql[overflow_start..line_end]
                    .chars()
                    .next()
                    .map(|ch| overflow_start + ch.len_utf8())
                    .unwrap_or(overflow_start);
                spans.push((overflow_start, overflow_end));
            }
        }

        line_start = idx + 1;
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutLongLines::default();
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
    fn flags_single_long_line() {
        let long_line = format!("SELECT {} FROM t", "x".repeat(320));
        let issues = run(&long_line);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_005);
    }

    #[test]
    fn does_not_flag_short_line() {
        assert!(run("SELECT x FROM t").is_empty());
    }

    #[test]
    fn flags_each_overflowing_line_once() {
        let sql = format!(
            "SELECT {} AS a,\n       {} AS b FROM t",
            "x".repeat(90),
            "y".repeat(90)
        );
        let issues = run(&sql);
        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_005)
                .count(),
            2,
        );
    }

    #[test]
    fn configured_max_line_length_is_respected() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.long_lines".to_string(),
                serde_json::json!({"max_line_length": 20}),
            )]),
        };
        let rule = LayoutLongLines::from_config(&config);
        let sql = "SELECT this_line_is_long FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_005);
    }
}
