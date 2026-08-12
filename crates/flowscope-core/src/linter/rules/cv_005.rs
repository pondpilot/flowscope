//! LINT_CV_005: Prefer IS [NOT] NULL over =/<> NULL.
//!
//! Comparisons like `col = NULL` or `col <> NULL` are not valid null checks in SQL.
//! Use `IS NULL` / `IS NOT NULL` instead.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::linter::visit;
use crate::types::{issue_codes, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::{Spanned, *};

pub struct NullComparison;

impl BuiltinLintRule for NullComparison {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_005
    }

    fn name(&self) -> &'static str {
        "Null comparison style"
    }

    fn description(&self) -> &'static str {
        "Comparisons with NULL should use \"IS\" or \"IS NOT\"."
    }

    fn check_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        visit::visit_expressions(stmt, &mut |expr| {
            let Expr::BinaryOp { left, op, right } = expr else {
                return;
            };

            if !is_null_expr(left) && !is_null_expr(right) {
                return;
            }

            let null_case = detect_null_case(ctx, expr);
            let (message, replacement_target, replacement_suffix) = match op {
                BinaryOperator::Eq => (
                    Some("Use IS NULL instead of = NULL."),
                    non_null_operand(left, right),
                    if null_case == KeywordCase::Lower {
                        " is null"
                    } else {
                        " IS NULL"
                    },
                ),
                BinaryOperator::NotEq => (
                    Some("Use IS NOT NULL instead of <> NULL or != NULL."),
                    non_null_operand(left, right),
                    if null_case == KeywordCase::Lower {
                        " is not null"
                    } else {
                        " IS NOT NULL"
                    },
                ),
                _ => (None, None, ""),
            };

            if let Some(message) = message {
                let mut issue = Issue::info(issue_codes::LINT_CV_005, message)
                    .with_statement(ctx.statement_index);
                if let (Some(target_expr), Some((start, end))) =
                    (replacement_target, expr_statement_offsets(ctx, expr))
                {
                    let span = ctx.span_from_statement_offset(start, end);
                    let replacement = format!("{target_expr}{replacement_suffix}");
                    issue = issue.with_span(span).with_autofix_edits(
                        IssueAutofixApplicability::Safe,
                        vec![IssuePatchEdit::new(span, replacement)],
                    );
                }
                issues.push(issue);
            }
        });
        issues
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeywordCase {
    Upper,
    Lower,
}

/// Detect whether the `NULL` keyword in the original SQL is uppercase or lowercase.
fn detect_null_case(ctx: &RuleContext, expr: &Expr) -> KeywordCase {
    if let Some((start, end)) = expr_statement_offsets(ctx, expr) {
        let fragment = &ctx.statement_sql()[start..end];
        // Look for the literal "null" (case-insensitive) in the expression text.
        if let Some(pos) = fragment.to_ascii_lowercase().rfind("null") {
            let null_text = &fragment[pos..pos + 4];
            if null_text == "null" {
                return KeywordCase::Lower;
            }
        }
    }
    KeywordCase::Upper
}

fn is_null_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Value(ValueWithSpan {
            value: Value::Null,
            ..
        })
    )
}

fn non_null_operand<'a>(left: &'a Expr, right: &'a Expr) -> Option<&'a Expr> {
    if is_null_expr(left) && !is_null_expr(right) {
        Some(right)
    } else if is_null_expr(right) && !is_null_expr(left) {
        Some(left)
    } else if is_null_expr(left) && is_null_expr(right) {
        Some(right)
    } else {
        None
    }
}

fn expr_statement_offsets(ctx: &RuleContext, expr: &Expr) -> Option<(usize, usize)> {
    if let Some((start, end)) = expr_span_offsets(ctx.statement_sql(), expr) {
        return Some((start, end));
    }

    let (start, end) = expr_span_offsets(ctx.sql, expr)?;
    if start < ctx.statement_range.start || end > ctx.statement_range.end {
        return None;
    }

    Some((
        start - ctx.statement_range.start,
        end - ctx.statement_range.start,
    ))
}

fn expr_span_offsets(sql: &str, expr: &Expr) -> Option<(usize, usize)> {
    let span = expr.span();
    if span.start.line == 0 || span.start.column == 0 || span.end.line == 0 || span.end.column == 0
    {
        return None;
    }

    let start = line_col_to_offset(sql, span.start.line as usize, span.start.column as usize)?;
    let end = line_col_to_offset(sql, span.end.line as usize, span.end.column as usize)?;
    if end < start {
        return None;
    }

    Some((start, end))
}

fn line_col_to_offset(sql: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }

    let mut current_line = 1usize;
    let mut line_start = 0usize;

    for (idx, ch) in sql.char_indices() {
        if current_line == line {
            break;
        }
        if ch == '\n' {
            current_line += 1;
            line_start = idx + ch.len_utf8();
        }
    }

    if current_line != line {
        return None;
    }

    let mut current_column = 1usize;
    for (rel_idx, ch) in sql[line_start..].char_indices() {
        if current_column == column {
            return Some(line_start + rel_idx);
        }
        if ch == '\n' {
            return None;
        }
        current_column += 1;
    }

    if current_column == column {
        return Some(sql.len());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = NullComparison;
        let ctx = RuleContext::new(sql, 0..sql.len(), 0);
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check_with_context(stmt, &ctx));
        }
        issues
    }

    fn apply_issue_autofix(sql: &str, issue: &Issue) -> Option<String> {
        let autofix = issue.autofix.as_ref()?;
        let mut edits = autofix.edits.clone();
        edits.sort_by(|left, right| right.span.start.cmp(&left.span.start));

        let mut out = sql.to_string();
        for edit in edits {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    #[test]
    fn test_eq_null_detected() {
        let issues = check_sql("SELECT * FROM t WHERE a = NULL");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_CV_005");
    }

    #[test]
    fn test_not_eq_null_detected() {
        let issues = check_sql("SELECT * FROM t WHERE a <> NULL");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_CV_005");
    }

    #[test]
    fn test_null_left_side_detected() {
        let issues = check_sql("SELECT * FROM t WHERE NULL = a");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_is_null_ok() {
        let issues = check_sql("SELECT * FROM t WHERE a IS NULL");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_is_not_null_ok() {
        let issues = check_sql("SELECT * FROM t WHERE a IS NOT NULL");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_eq_null_emits_safe_autofix_patch() {
        let sql = "SELECT * FROM t WHERE a = NULL";
        let issues = check_sql(sql);
        let issue = &issues[0];
        let autofix = issue.autofix.as_ref().expect("autofix metadata");

        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);

        let expected_start = sql.find("a = NULL").expect("comparison exists");
        let expected_end = expected_start + "a = NULL".len();
        assert_eq!(autofix.edits[0].span.start, expected_start);
        assert_eq!(autofix.edits[0].span.end, expected_end);
        assert_eq!(autofix.edits[0].replacement, "a IS NULL");

        let fixed = apply_issue_autofix(sql, issue).expect("apply autofix");
        assert_eq!(fixed, "SELECT * FROM t WHERE a IS NULL");
    }

    #[test]
    fn test_not_eq_lowercase_null_emits_lowercase_autofix() {
        let sql = "SELECT a FROM foo WHERE a <> null";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT a FROM foo WHERE a is not null");
    }

    #[test]
    fn test_null_left_not_eq_emits_safe_autofix_patch() {
        let sql = "SELECT * FROM t WHERE NULL <> a";
        let issues = check_sql(sql);
        let issue = &issues[0];
        let autofix = issue.autofix.as_ref().expect("autofix metadata");

        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);

        let expected_start = sql.find("NULL <> a").expect("comparison exists");
        let expected_end = expected_start + "NULL <> a".len();
        assert_eq!(autofix.edits[0].span.start, expected_start);
        assert_eq!(autofix.edits[0].span.end, expected_end);
        assert_eq!(autofix.edits[0].replacement, "a IS NOT NULL");

        let fixed = apply_issue_autofix(sql, issue).expect("apply autofix");
        assert_eq!(fixed, "SELECT * FROM t WHERE a IS NOT NULL");
    }
}
