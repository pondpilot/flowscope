//! LINT_TQ_002: TSQL procedure BEGIN/END block.
//!
//! SQLFluff TQ02 parity: procedures with multiple statements should include a
//! `BEGIN`/`END` block.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::Statement;

pub struct TsqlProcedureBeginEnd;

impl BuiltinLintRule for TsqlProcedureBeginEnd {
    fn code(&self) -> &'static str {
        issue_codes::LINT_TQ_002
    }

    fn name(&self) -> &'static str {
        "TSQL procedure BEGIN/END"
    }

    fn description(&self) -> &'static str {
        "Procedure bodies with multiple statements should be wrapped in BEGIN/END."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        if ctx.dialect() != Dialect::Mssql {
            return Vec::new();
        }

        // Treat TQ02 as source/document scoped so best-effort parser recovery
        // (which may parse only later statement slices) still allows detection.
        if ctx.statement_index != 0 {
            return Vec::new();
        }

        let has_violation = procedure_requires_begin_end_from_sql(ctx.sql);

        if has_violation {
            let mut issue = Issue::warning(
                issue_codes::LINT_TQ_002,
                "Stored procedures with multiple statements should include BEGIN/END block.",
            )
            .with_statement(ctx.statement_index);

            let autofix_edits = tq002_autofix_edits(ctx.sql)
                .into_iter()
                .map(|edit| IssuePatchEdit::new(Span::new(edit.start, edit.end), edit.replacement))
                .collect::<Vec<_>>();
            if !autofix_edits.is_empty() {
                issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, autofix_edits);
            }

            vec![issue]
        } else {
            Vec::new()
        }
    }
}

struct Tq002AutofixEdit {
    start: usize,
    end: usize,
    replacement: String,
}

#[derive(Clone, Copy)]
struct ProcedureBodyLayout {
    as_end: usize,
    body_start: usize,
    body_end: usize,
    has_begin: bool,
    statement_count: usize,
}

fn procedure_requires_begin_end_from_sql(sql: &str) -> bool {
    let Some(layout) = procedure_body_layout(sql) else {
        return false;
    };
    !layout.has_begin && layout.statement_count > 1
}

fn tq002_autofix_edits(sql: &str) -> Vec<Tq002AutofixEdit> {
    let Some(layout) = procedure_body_layout(sql) else {
        return Vec::new();
    };
    if layout.has_begin || layout.statement_count <= 1 {
        return Vec::new();
    }

    let multiline_body = sql.as_bytes()[layout.as_end..layout.body_start]
        .iter()
        .any(|byte| matches!(*byte, b'\n' | b'\r'));

    let begin_replacement = if multiline_body { "BEGIN\n" } else { "BEGIN " };
    let end_replacement = if multiline_body { "\nEND" } else { " END" };

    vec![
        Tq002AutofixEdit {
            start: layout.body_start,
            end: layout.body_start,
            replacement: begin_replacement.to_string(),
        },
        Tq002AutofixEdit {
            start: layout.body_end,
            end: layout.body_end,
            replacement: end_replacement.to_string(),
        },
    ]
}

fn procedure_body_layout(sql: &str) -> Option<ProcedureBodyLayout> {
    let bytes = sql.as_bytes();
    let header_end = procedure_header_end(bytes)?;
    let as_end = find_next_keyword(bytes, header_end, b"AS")?;
    let body_start = skip_ascii_whitespace(bytes, as_end);
    if body_start >= bytes.len() {
        return None;
    }

    let body_end = trim_ascii_whitespace_end(bytes);
    if body_end <= body_start {
        return None;
    }

    let has_begin = match_ascii_keyword_at(bytes, body_start, b"BEGIN").is_some();
    let statement_count = count_body_statements(&sql[body_start..body_end]);

    Some(ProcedureBodyLayout {
        as_end,
        body_start,
        body_end,
        has_begin,
        statement_count,
    })
}

fn procedure_header_end(bytes: &[u8]) -> Option<usize> {
    let mut index = skip_ascii_whitespace_and_comments(bytes, 0);

    if let Some(create_end) = match_ascii_keyword_at(bytes, index, b"CREATE") {
        index = skip_ascii_whitespace_and_comments(bytes, create_end);
        if let Some(or_end) = match_ascii_keyword_at(bytes, index, b"OR") {
            index = skip_ascii_whitespace_and_comments(bytes, or_end);
            let alter_end = match_ascii_keyword_at(bytes, index, b"ALTER")?;
            index = skip_ascii_whitespace_and_comments(bytes, alter_end);
        }
        let proc_end = match_procedure_keyword(bytes, index)?;
        return Some(skip_ascii_whitespace_and_comments(bytes, proc_end));
    }

    if let Some(alter_end) = match_ascii_keyword_at(bytes, index, b"ALTER") {
        index = skip_ascii_whitespace_and_comments(bytes, alter_end);
        let proc_end = match_procedure_keyword(bytes, index)?;
        return Some(skip_ascii_whitespace_and_comments(bytes, proc_end));
    }

    None
}

fn match_procedure_keyword(bytes: &[u8], start: usize) -> Option<usize> {
    match_ascii_keyword_at(bytes, start, b"PROCEDURE")
        .or_else(|| match_ascii_keyword_at(bytes, start, b"PROC"))
}

fn find_next_keyword(bytes: &[u8], mut index: usize, keyword_upper: &[u8]) -> Option<usize> {
    while index < bytes.len() {
        index = skip_ascii_whitespace_and_comments(bytes, index);
        if index >= bytes.len() {
            return None;
        }

        if let Some(end) = match_ascii_keyword_at(bytes, index, keyword_upper) {
            return Some(end);
        }

        if bytes[index] == b'\'' {
            index = skip_single_quoted_literal(bytes, index);
            continue;
        }
        if bytes[index] == b'"' {
            index = skip_double_quoted_literal(bytes, index);
            continue;
        }
        if bytes[index] == b'[' {
            index = skip_bracket_identifier(bytes, index);
            continue;
        }

        index += 1;
    }

    None
}

fn count_body_statements(sql: &str) -> usize {
    let bytes = sql.as_bytes();
    let mut index = 0usize;
    let mut statement_count = 0usize;
    let mut statement_has_code = false;
    let mut paren_depth = 0usize;

    while index < bytes.len() {
        if bytes[index] == b'-' && index + 1 < bytes.len() && bytes[index + 1] == b'-' {
            index = skip_line_comment(bytes, index);
            continue;
        }
        if bytes[index] == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'*' {
            index = skip_block_comment(bytes, index);
            continue;
        }
        if bytes[index] == b'\'' {
            statement_has_code = true;
            index = skip_single_quoted_literal(bytes, index);
            continue;
        }
        if bytes[index] == b'"' {
            statement_has_code = true;
            index = skip_double_quoted_literal(bytes, index);
            continue;
        }
        if bytes[index] == b'[' {
            statement_has_code = true;
            index = skip_bracket_identifier(bytes, index);
            continue;
        }

        match bytes[index] {
            b'(' => {
                statement_has_code = true;
                paren_depth += 1;
                index += 1;
            }
            b')' => {
                statement_has_code = true;
                paren_depth = paren_depth.saturating_sub(1);
                index += 1;
            }
            b';' if paren_depth == 0 => {
                if statement_has_code {
                    statement_count += 1;
                    statement_has_code = false;
                }
                index += 1;
            }
            byte if is_ascii_whitespace_byte(byte) => {
                index += 1;
            }
            _ => {
                statement_has_code = true;
                index += 1;
            }
        }
    }

    if statement_has_code {
        statement_count += 1;
    }

    statement_count
}

fn trim_ascii_whitespace_end(bytes: &[u8]) -> usize {
    let mut tail = bytes.len();
    while tail > 0 && is_ascii_whitespace_byte(bytes[tail - 1]) {
        tail -= 1;
    }
    tail
}

fn skip_ascii_whitespace_and_comments(bytes: &[u8], mut index: usize) -> usize {
    loop {
        index = skip_ascii_whitespace(bytes, index);
        if index >= bytes.len() {
            return index;
        }
        if bytes[index] == b'-' && index + 1 < bytes.len() && bytes[index + 1] == b'-' {
            index = skip_line_comment(bytes, index);
            continue;
        }
        if bytes[index] == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'*' {
            index = skip_block_comment(bytes, index);
            continue;
        }
        return index;
    }
}

fn skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    index += 2;
    while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
        index += 1;
    }
    index
}

fn skip_block_comment(bytes: &[u8], mut index: usize) -> usize {
    index += 2;
    while index + 1 < bytes.len() {
        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
}

fn skip_single_quoted_literal(bytes: &[u8], mut index: usize) -> usize {
    index += 1;
    while index < bytes.len() {
        if bytes[index] == b'\'' {
            if index + 1 < bytes.len() && bytes[index + 1] == b'\'' {
                index += 2;
            } else {
                return index + 1;
            }
        } else {
            index += 1;
        }
    }
    bytes.len()
}

fn skip_double_quoted_literal(bytes: &[u8], mut index: usize) -> usize {
    index += 1;
    while index < bytes.len() {
        if bytes[index] == b'"' {
            if index + 1 < bytes.len() && bytes[index + 1] == b'"' {
                index += 2;
            } else {
                return index + 1;
            }
        } else {
            index += 1;
        }
    }
    bytes.len()
}

fn skip_bracket_identifier(bytes: &[u8], mut index: usize) -> usize {
    index += 1;
    while index < bytes.len() {
        if bytes[index] == b']' {
            if index + 1 < bytes.len() && bytes[index + 1] == b']' {
                index += 2;
            } else {
                return index + 1;
            }
        } else {
            index += 1;
        }
    }
    bytes.len()
}

fn is_ascii_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_word_boundary_for_keyword(bytes: &[u8], index: usize) -> bool {
    index == 0 || index >= bytes.len() || !is_ascii_ident_continue(bytes[index])
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

fn is_ascii_whitespace_byte(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t' | 0x0b | 0x0c)
}

fn skip_ascii_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && is_ascii_whitespace_byte(bytes[index]) {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_sql, parse_sql_with_dialect};
    use crate::types::IssueAutofixApplicability;
    use crate::Dialect;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql_with_dialect(sql, Dialect::Mssql).expect("parse");
        let rule = TsqlProcedureBeginEnd;
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

    fn run_statementless(sql: &str) -> Vec<Issue> {
        let synthetic = parse_sql("SELECT 1").expect("parse synthetic statement");
        let rule = TsqlProcedureBeginEnd;
        rule.check_with_context(
            &synthetic[0],
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
    fn does_not_flag_single_statement_procedure_without_begin_end() {
        let sql = "CREATE PROCEDURE p AS SELECT 1;";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_procedure_with_begin_end() {
        let issues = run("CREATE PROCEDURE p AS BEGIN SELECT 1; END;");
        assert!(issues.is_empty());
    }

    #[test]
    fn detects_multi_statement_create_procedure_in_statementless_mode() {
        let sql = "CREATE PROCEDURE p AS SELECT 1; SELECT 2;";
        let issues = run_statementless(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_TQ_002);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "CREATE PROCEDURE p AS BEGIN SELECT 1; SELECT 2; END");
    }

    #[test]
    fn detects_alter_procedure_in_statementless_mode() {
        let sql = "ALTER PROCEDURE dbo.p AS SELECT 1; SELECT 2;";
        let issues = run_statementless(sql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn detects_create_or_alter_procedure_in_statementless_mode() {
        let sql = "CREATE OR ALTER PROCEDURE dbo.p AS SELECT 1; SELECT 2;";
        let issues = run_statementless(sql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn does_not_flag_external_name_statementless_procedure() {
        let sql = "CREATE PROCEDURE dbo.ExternalProc AS EXTERNAL NAME Assembly.Class.Method;";
        let issues = run_statementless(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_procedure_text_inside_string_literal() {
        let issues = run_statementless("SELECT 'CREATE PROCEDURE p AS SELECT 1' AS sql_snippet");
        assert!(issues.is_empty());
    }
}
