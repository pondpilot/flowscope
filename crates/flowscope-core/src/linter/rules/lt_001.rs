//! LINT_LT_001: Layout spacing.
//!
//! SQLFluff LT01 parity (current scope): detect compact operator-style patterns
//! where spacing is expected.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct LayoutSpacing;

impl LintRule for LayoutSpacing {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_001
    }

    fn name(&self) -> &'static str {
        "Layout spacing"
    }

    fn description(&self) -> &'static str {
        "Operator spacing should be consistent."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = ctx.statement_sql();
        let mut issues = Vec::new();

        for (matched, start, end) in
            capture_group_with_spans(sql, r"(?i)\b[A-Za-z_][A-Za-z0-9_\.]*->>'[^']*'", 0)
        {
            if let Some(op_idx) = matched.find("->>") {
                let left_start = start + op_idx;
                let right_start = left_start + 3;
                let left_end = (left_start + 1).min(end);
                let right_end = (right_start + 1).min(end);

                if left_start < left_end {
                    issues.push(
                        Issue::info(
                            issue_codes::LINT_LT_001,
                            "Operator spacing appears inconsistent.",
                        )
                        .with_statement(ctx.statement_index)
                        .with_span(ctx.span_from_statement_offset(left_start, left_end)),
                    );
                }
                if right_start < right_end {
                    issues.push(
                        Issue::info(
                            issue_codes::LINT_LT_001,
                            "Operator spacing appears inconsistent.",
                        )
                        .with_statement(ctx.statement_index)
                        .with_span(ctx.span_from_statement_offset(right_start, right_end)),
                    );
                }
            }
        }

        for (_matched, _start, end) in capture_group_with_spans(sql, r"(?i)\btext\[", 0) {
            let bracket_start = end.saturating_sub(1);
            if bracket_start < end {
                issues.push(
                    Issue::info(
                        issue_codes::LINT_LT_001,
                        "Operator spacing appears inconsistent.",
                    )
                    .with_statement(ctx.statement_index)
                    .with_span(ctx.span_from_statement_offset(bracket_start, end)),
                );
            }
        }

        for (_matched, start, _end) in capture_group_with_spans(sql, r",\d", 0) {
            let number_start = start + 1;
            issues.push(
                Issue::info(
                    issue_codes::LINT_LT_001,
                    "Operator spacing appears inconsistent.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(number_start, number_start + 1)),
            );
        }

        for (matched, start, end) in capture_group_with_spans(sql, r"(?im)^\s*exists\s+\(", 0) {
            let line_start = sql[..start].rfind('\n').map_or(0, |idx| idx + 1);
            let prev_token = sql[..line_start]
                .lines()
                .rev()
                .map(str::trim)
                .find(|line| !line.is_empty() && !line.starts_with("--"));
            if matches!(prev_token, Some("OR") | Some("AND") | Some("NOT")) {
                continue;
            }

            if let Some(paren_off) = matched.rfind('(') {
                let paren_start = start + paren_off;
                if paren_start < end {
                    issues.push(
                        Issue::info(
                            issue_codes::LINT_LT_001,
                            "Operator spacing appears inconsistent.",
                        )
                        .with_statement(ctx.statement_index)
                        .with_span(ctx.span_from_statement_offset(paren_start, paren_start + 1)),
                    );
                }
            }
        }

        issues
    }
}

fn capture_group_with_spans(
    sql: &str,
    pattern: &str,
    group_idx: usize,
) -> Vec<(String, usize, usize)> {
    Regex::new(pattern)
        .expect("valid regex")
        .captures_iter(sql)
        .filter_map(|caps| caps.get(group_idx))
        .map(|m| (m.as_str().to_string(), m.start(), m.end()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutSpacing;
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
    fn does_not_flag_simple_spacing() {
        assert!(run("SELECT * FROM t WHERE a = 1").is_empty());
    }

    #[test]
    fn flags_compact_json_arrow_operator() {
        let issues = run("SELECT payload->>'id' FROM t");
        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_001)
                .count(),
            2,
        );
    }

    #[test]
    fn flags_compact_type_bracket_and_numeric_scale() {
        assert!(!run("SELECT ARRAY['x']::text[]").is_empty());
        assert!(!run("SELECT 1::numeric(5,2)").is_empty());
    }

    #[test]
    fn flags_exists_parenthesis_layout_case() {
        let issues = run("SELECT
    EXISTS (
        SELECT 1
    ) AS has_row");
        assert!(!issues.is_empty());
    }
}
