//! LINT_LT_012: Layout end of file.
//!
//! SQLFluff LT12 parity (current scope): SQL text should end with exactly one
//! trailing newline.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::Statement;

pub struct LayoutEndOfFile;

impl BuiltinLintRule for LayoutEndOfFile {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_012
    }

    fn name(&self) -> &'static str {
        "Layout end of file"
    }

    fn description(&self) -> &'static str {
        "Files must end with a single trailing newline."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let content_end = ctx
            .sql
            .trim_end_matches(|ch: char| ch.is_ascii_whitespace())
            .len();
        let is_last_statement = ctx.statement_range.end >= content_end;
        let (trailing_newlines, has_trailing_spaces) = trailing_newline_metrics(ctx.sql);
        let has_violation = is_last_statement && (trailing_newlines != 1 || has_trailing_spaces);

        if has_violation {
            let trailing_span = Span::new(content_end, ctx.sql.len());
            vec![Issue::info(
                issue_codes::LINT_LT_012,
                "SQL document should end with a single trailing newline.",
            )
            .with_statement(ctx.statement_index)
            .with_span(trailing_span)
            .with_autofix_edits(
                IssueAutofixApplicability::Safe,
                vec![IssuePatchEdit::new(trailing_span, "\n")],
            )]
        } else {
            Vec::new()
        }
    }
}

fn trailing_newline_metrics(sql: &str) -> (usize, bool) {
    let mut end = sql.len();
    while end > 0 {
        let ch = sql[..end]
            .chars()
            .next_back()
            .expect("string slice should not be empty");
        if ch == ' ' || ch == '\t' {
            end -= ch.len_utf8();
            continue;
        }
        break;
    }

    let has_trailing_spaces = end != sql.len();
    (trailing_newline_count(&sql[..end]), has_trailing_spaces)
}

fn trailing_newline_count(sql: &str) -> usize {
    sql.chars()
        .rev()
        .take_while(|ch| *ch == '\n' || *ch == '\r')
        .filter(|ch| *ch == '\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutEndOfFile;
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
    fn flags_missing_trailing_newline() {
        let sql = "SELECT 1\nFROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_012);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT 1\nFROM t\n");
    }

    #[test]
    fn does_not_flag_when_trailing_newline_present() {
        assert!(run("SELECT 1\nFROM t\n").is_empty());
    }

    #[test]
    fn flags_single_line_without_newline() {
        let sql = "SELECT 1";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_012);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT 1\n");
    }

    #[test]
    fn flags_multiple_trailing_newlines() {
        let sql = "SELECT 1\nFROM t\n\n";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_012);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT 1\nFROM t\n");
    }

    #[test]
    fn flags_trailing_spaces_after_newline() {
        let sql = "SELECT 1\nFROM t\n  ";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_012);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT 1\nFROM t\n");
    }

    #[test]
    fn statementless_flags_templated_without_raw_final_newline() {
        let sql = "{{ '\\n\\n' }}";
        let synthetic = parse_sql("SELECT 1").expect("parse");
        let rule = LayoutEndOfFile;
        let issues =
            rule.check_with_context(&synthetic[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_012);
    }

    #[test]
    fn statementless_allows_templated_line_with_raw_final_newline() {
        let sql = "select * from {{ 'trim_whitespace_table' -}}\n";
        let synthetic = parse_sql("SELECT 1").expect("parse");
        let rule = LayoutEndOfFile;
        let issues =
            rule.check_with_context(&synthetic[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }
}
