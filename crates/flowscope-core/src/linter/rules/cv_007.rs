//! LINT_CV_007: Statement brackets.
//!
//! SQLFluff CV07 parity (current scope): avoid wrapping an entire statement in
//! unnecessary outer brackets.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::{SetExpr, Statement};

pub struct ConventionStatementBrackets;

impl BuiltinLintRule for ConventionStatementBrackets {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_007
    }

    fn name(&self) -> &'static str {
        "Statement brackets"
    }

    fn description(&self) -> &'static str {
        "Top-level statements should not be wrapped in brackets."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let bracket_depth = wrapper_bracket_depth(statement);
        if bracket_depth > 0 {
            let mut issue = Issue::info(
                issue_codes::LINT_CV_007,
                "Avoid wrapping the full statement in unnecessary brackets.",
            )
            .with_statement(ctx.statement_index);
            if let Some(pairs) = wrapper_bracket_offsets(ctx.statement_sql(), bracket_depth) {
                let outer_left = pairs[0].0;
                let mut edits = Vec::with_capacity(pairs.len() * 2);
                for (left_idx, right_idx) in pairs {
                    edits.push(IssuePatchEdit::new(
                        ctx.span_from_statement_offset(left_idx, left_idx + 1),
                        "",
                    ));
                    edits.push(IssuePatchEdit::new(
                        ctx.span_from_statement_offset(right_idx, right_idx + 1),
                        "",
                    ));
                }
                issue = issue
                    .with_span(ctx.span_from_statement_offset(outer_left, outer_left + 1))
                    .with_autofix_edits(IssueAutofixApplicability::Safe, edits);
            }
            vec![issue]
        } else {
            Vec::new()
        }
    }
}

fn wrapper_bracket_depth(statement: &Statement) -> usize {
    let Statement::Query(query) = statement else {
        return 0;
    };

    let mut depth = 0;
    let mut body = query.body.as_ref();
    while let SetExpr::Query(inner_query) = body {
        depth += 1;
        body = inner_query.body.as_ref();
    }
    depth
}

fn wrapper_bracket_offsets(sql: &str, depth: usize) -> Option<Vec<(usize, usize)>> {
    if depth == 0 {
        return Some(Vec::new());
    }

    let mut left_bound = 0usize;
    let mut right_bound = sql.len();
    let mut pairs = Vec::with_capacity(depth);

    for _ in 0..depth {
        let left_idx = sql[left_bound..right_bound]
            .char_indices()
            .find_map(|(offset, ch)| (!ch.is_whitespace()).then_some((left_bound + offset, ch)))
            .and_then(|(idx, ch)| (ch == '(').then_some(idx))?;

        let right_idx = sql[left_bound..right_bound]
            .char_indices()
            .rev()
            .find_map(|(offset, ch)| (!ch.is_whitespace()).then_some((left_bound + offset, ch)))
            .and_then(|(idx, ch)| (ch == ')').then_some(idx))?;

        if right_idx <= left_idx {
            return None;
        }

        pairs.push((left_idx, right_idx));
        left_bound = left_idx + 1;
        right_bound = right_idx;
    }

    Some(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionStatementBrackets;
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
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
    fn flags_wrapped_statement() {
        let issues = run("(SELECT 1)");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_007);
    }

    #[test]
    fn does_not_flag_normal_statement() {
        assert!(run("SELECT 1").is_empty());
    }

    #[test]
    fn does_not_flag_parenthesized_subquery_in_from_clause() {
        assert!(run("SELECT * FROM (SELECT 1) AS t").is_empty());
    }

    #[test]
    fn wrapped_statement_emits_safe_autofix_patch() {
        let sql = "(SELECT 1)";
        let issues = run(sql);
        let issue = &issues[0];
        let autofix = issue.autofix.as_ref().expect("autofix metadata");

        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 2);
        assert_eq!(autofix.edits[0].span.start, 0);
        assert_eq!(autofix.edits[0].span.end, 1);
        assert_eq!(autofix.edits[1].span.start, sql.len() - 1);
        assert_eq!(autofix.edits[1].span.end, sql.len());

        let fixed = apply_issue_autofix(sql, issue).expect("apply autofix");
        assert_eq!(fixed, "SELECT 1");
    }

    #[test]
    fn nested_wrapped_statement_autofix_removes_all_outer_pairs() {
        let sql = "((SELECT 1))";
        let issues = run(sql);
        let issue = &issues[0];
        let autofix = issue.autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.edits.len(), 4);

        let fixed = apply_issue_autofix(sql, issue).expect("apply autofix");
        assert_eq!(fixed, "SELECT 1");
    }
}
