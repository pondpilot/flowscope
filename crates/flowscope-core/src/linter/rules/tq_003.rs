//! LINT_TQ_003: TSQL empty batch.
//!
//! SQLFluff TQ03 parity (current scope): detect empty batches between repeated
//! `GO` separators.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};
use std::collections::{BTreeMap, BTreeSet};

pub struct TsqlEmptyBatch;

impl BuiltinLintRule for TsqlEmptyBatch {
    fn code(&self) -> &'static str {
        issue_codes::LINT_TQ_003
    }

    fn name(&self) -> &'static str {
        "TSQL empty batch"
    }

    fn description(&self) -> &'static str {
        "Remove empty batches."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        if ctx.dialect() != Dialect::Mssql {
            return Vec::new();
        }

        // TQ003 is document-level: GO separators can sit outside statement
        // spans, so evaluate once against the full SQL document.
        if ctx.statement_index != 0 {
            return Vec::new();
        }

        let has_violation = has_empty_go_batch_separator(ctx.sql, ctx.dialect(), None);
        if !has_violation {
            return Vec::new();
        }

        let mut issue = Issue::warning(
            issue_codes::LINT_TQ_003,
            "Empty TSQL batch detected between GO separators.",
        )
        .with_statement(ctx.statement_index);

        let autofix_edits = empty_go_batch_separator_edits(ctx.sql)
            .into_iter()
            .map(|edit| {
                IssuePatchEdit::new(
                    crate::types::Span::new(edit.start, edit.end),
                    edit.replacement,
                )
            })
            .collect::<Vec<_>>();

        if !autofix_edits.is_empty() {
            issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, autofix_edits);
        }

        vec![issue]
    }
}

fn has_empty_go_batch_separator(
    sql: &str,
    dialect: Dialect,
    tokens: Option<&[TokenWithSpan]>,
) -> bool {
    let owned_tokens;
    let tokens = if let Some(tokens) = tokens {
        tokens
    } else {
        owned_tokens = match tokenized(sql, dialect) {
            Some(tokens) => tokens,
            None => return false,
        };
        &owned_tokens
    };

    let mut line_summary = BTreeMap::<usize, LineSummary>::new();
    let mut go_candidate_lines = BTreeSet::<usize>::new();

    for token in tokens {
        update_line_summary(&mut line_summary, token);
        if let Token::Word(word) = &token.token {
            if word.value.eq_ignore_ascii_case("GO") {
                go_candidate_lines.insert(token.span.start.line as usize);
            }
        }
    }

    let mut go_lines = go_candidate_lines
        .into_iter()
        .filter(|line| {
            line_summary
                .get(line)
                .is_some_and(|summary| summary.is_go_separator())
        })
        .collect::<Vec<_>>();

    if go_lines.len() < 2 {
        return false;
    }

    go_lines.sort_unstable();
    go_lines.dedup();

    go_lines
        .windows(2)
        .any(|pair| lines_between_are_empty(&line_summary, pair[0], pair[1]))
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

#[derive(Default, Clone, Copy)]
struct LineSummary {
    go_count: usize,
    other_count: usize,
}

impl LineSummary {
    fn is_go_separator(self) -> bool {
        self.go_count == 1 && self.other_count == 0
    }
}

fn update_line_summary(summary: &mut BTreeMap<usize, LineSummary>, token: &TokenWithSpan) {
    let start_line = token.span.start.line as usize;
    let end_line = token.span.end.line as usize;

    match &token.token {
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline) => {}
        Token::Whitespace(Whitespace::SingleLineComment { .. }) => {
            summary.entry(start_line).or_default().other_count += 1;
        }
        Token::Whitespace(Whitespace::MultiLineComment(_)) => {
            for line in start_line..=end_line {
                summary.entry(line).or_default().other_count += 1;
            }
        }
        Token::Word(word) if word.value.eq_ignore_ascii_case("GO") && start_line == end_line => {
            summary.entry(start_line).or_default().go_count += 1;
        }
        _ => {
            for line in start_line..=end_line {
                summary.entry(line).or_default().other_count += 1;
            }
        }
    }
}

fn lines_between_are_empty(
    line_summary: &BTreeMap<usize, LineSummary>,
    first_line: usize,
    second_line: usize,
) -> bool {
    if second_line <= first_line {
        return false;
    }

    if second_line == first_line + 1 {
        return true;
    }

    ((first_line + 1)..second_line).all(|line_number| !line_summary.contains_key(&line_number))
}

struct Tq003AutofixEdit {
    start: usize,
    end: usize,
    replacement: String,
}

fn empty_go_batch_separator_edits(sql: &str) -> Vec<Tq003AutofixEdit> {
    let bytes = sql.as_bytes();
    let mut edits = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] != b'\n' {
            index += 1;
            continue;
        }

        let mut cursor = index;
        let mut batch_count = 0usize;
        while cursor < bytes.len() && bytes[cursor] == b'\n' {
            let mut go_start = cursor + 1;
            while go_start < bytes.len() && is_ascii_whitespace_non_newline_byte(bytes[go_start]) {
                go_start += 1;
            }
            let Some(go_end) = match_ascii_keyword_at(bytes, go_start, b"GO") else {
                break;
            };
            let mut after_go = go_end;
            while after_go < bytes.len() && is_ascii_whitespace_non_newline_byte(bytes[after_go]) {
                after_go += 1;
            }
            batch_count += 1;
            cursor = after_go;
        }

        if batch_count >= 2 {
            edits.push(Tq003AutofixEdit {
                start: index,
                end: cursor,
                replacement: "\nGO".to_string(),
            });
            index = cursor;
        } else {
            index += 1;
        }
    }

    edits
}

fn is_ascii_whitespace_non_newline_byte(byte: u8) -> bool {
    byte.is_ascii_whitespace() && byte != b'\n'
}

fn is_ascii_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_word_boundary_for_keyword(bytes: &[u8], idx: usize) -> bool {
    idx == 0 || idx >= bytes.len() || !is_ascii_ident_continue(bytes[idx])
}

fn match_ascii_keyword_at(bytes: &[u8], start: usize, keyword_upper: &[u8]) -> Option<usize> {
    let end = start.checked_add(keyword_upper.len())?;
    if end > bytes.len() {
        return None;
    }
    if !is_word_boundary_for_keyword(bytes, start.saturating_sub(1))
        || !is_word_boundary_for_keyword(bytes, end)
    {
        return None;
    }
    let matches = bytes[start..end]
        .iter()
        .zip(keyword_upper.iter())
        .all(|(actual, expected)| actual.to_ascii_uppercase() == *expected);
    if matches {
        Some(end)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = TsqlEmptyBatch;
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(
                    statement,
                    &RuleContext::new(sql, 0..sql.len(), index).with_dialect(Dialect::Mssql),
                )
            })
            .collect()
    }

    fn run_for_statement_sql(sql: &str) -> Vec<Issue> {
        let statements = parse_sql("SELECT 1").expect("parse placeholder statement");
        let rule = TsqlEmptyBatch;
        rule.check_with_context(
            &statements[0],
            &RuleContext::new(sql, 0..sql.len(), 0).with_dialect(Dialect::Mssql),
        )
    }

    fn apply_issue_autofix(sql: &str, issue: &Issue) -> Option<String> {
        let autofix = issue.autofix.as_ref()?;
        let mut out = sql.to_string();
        let mut edits = autofix.edits.clone();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        for edit in edits.into_iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    #[test]
    fn detects_repeated_go_separator_lines() {
        assert!(has_empty_go_batch_separator(
            "GO\nGO\n",
            Dialect::Generic,
            None
        ));
        assert!(has_empty_go_batch_separator(
            "GO\n\nGO\n",
            Dialect::Generic,
            None
        ));
    }

    #[test]
    fn does_not_detect_single_go_separator_line() {
        assert!(!has_empty_go_batch_separator(
            "GO\n",
            Dialect::Generic,
            None
        ));
    }

    #[test]
    fn does_not_detect_go_text_inside_string_literal() {
        assert!(!has_empty_go_batch_separator(
            "SELECT '\nGO\nGO\n' AS sql_snippet",
            Dialect::Generic,
            None,
        ));
    }

    #[test]
    fn detects_empty_go_batches_between_statements() {
        assert!(has_empty_go_batch_separator(
            "SELECT 1\nGO\nGO\nSELECT 2\n",
            Dialect::Generic,
            None,
        ));
    }

    #[test]
    fn emits_safe_autofix_for_empty_go_batches() {
        let sql = "SELECT 1\nGO\nGO\nSELECT 2\n";
        let issues = run_for_statement_sql(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT 1\nGO\nSELECT 2\n");
    }

    #[test]
    fn does_not_treat_comment_line_between_go_as_empty_batch() {
        assert!(!has_empty_go_batch_separator(
            "GO\n-- keep batch non-empty\nGO\n",
            Dialect::Generic,
            None,
        ));
    }

    #[test]
    fn rule_does_not_flag_go_text_inside_string_literal() {
        let issues = run("SELECT '\nGO\nGO\n' AS sql_snippet");
        assert!(issues.is_empty());
    }

    #[test]
    fn rule_does_not_run_for_non_mssql_dialect() {
        let statements = parse_sql("SELECT 1").expect("parse placeholder statement");
        let rule = TsqlEmptyBatch;
        let sql = "SELECT 1\nGO\nGO\n";
        let issues = rule.check_with_context(
            &statements[0],
            &RuleContext::new(sql, 0..sql.len(), 0).with_dialect(Dialect::Postgres),
        );
        assert!(issues.is_empty());
    }
}
