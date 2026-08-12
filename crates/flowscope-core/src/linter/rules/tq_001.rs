//! LINT_TQ_001: TSQL `sp_` prefix.
//!
//! SQLFluff TQ01 parity (current scope): avoid stored procedure names starting
//! with `sp_`.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;

pub struct TsqlSpPrefix;

impl BuiltinLintRule for TsqlSpPrefix {
    fn code(&self) -> &'static str {
        issue_codes::LINT_TQ_001
    }

    fn name(&self) -> &'static str {
        "TSQL sp_ prefix"
    }

    fn description(&self) -> &'static str {
        "'SP_' prefix should not be used for user-defined stored procedures in T-SQL."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        if ctx.dialect() != Dialect::Mssql {
            return Vec::new();
        }

        // Evaluate once per source document so parser best-effort mode (where
        // the CREATE PROCEDURE slice may fail to parse) still gets checked.
        if ctx.statement_index != 0 {
            return Vec::new();
        }

        let has_violation = procedure_name_has_sp_prefix_in_sql(ctx.sql);

        if has_violation {
            vec![Issue::warning(
                issue_codes::LINT_TQ_001,
                "Avoid stored procedure names with sp_ prefix.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn procedure_name_has_sp_prefix_in_sql(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let Some(header_end) = procedure_header_end(bytes) else {
        return false;
    };
    let Some(name) = parse_procedure_name(bytes, header_end) else {
        return false;
    };
    name.to_ascii_lowercase().starts_with("sp_")
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

fn parse_procedure_name(bytes: &[u8], start: usize) -> Option<String> {
    let mut index = skip_ascii_whitespace_and_comments(bytes, start);
    let (mut next, mut last_part) = parse_identifier_part(bytes, index)?;

    loop {
        index = skip_ascii_whitespace_and_comments(bytes, next);
        if index >= bytes.len() || bytes[index] != b'.' {
            return Some(last_part);
        }

        index = skip_ascii_whitespace_and_comments(bytes, index + 1);
        let (part_end, part_value) = parse_identifier_part(bytes, index)?;
        next = part_end;
        last_part = part_value;
    }
}

fn parse_identifier_part(bytes: &[u8], start: usize) -> Option<(usize, String)> {
    if start >= bytes.len() {
        return None;
    }

    if bytes[start] == b'[' {
        return parse_bracket_identifier(bytes, start);
    }

    if bytes[start] == b'"' {
        return parse_double_quoted_identifier(bytes, start);
    }

    if !is_ascii_ident_start(bytes[start]) {
        return None;
    }

    let mut end = start + 1;
    while end < bytes.len() && is_ascii_ident_continue(bytes[end]) {
        end += 1;
    }

    Some((
        end,
        String::from_utf8_lossy(&bytes[start..end]).into_owned(),
    ))
}

fn parse_bracket_identifier(bytes: &[u8], start: usize) -> Option<(usize, String)> {
    let mut index = start + 1;
    let mut out = String::new();
    while index < bytes.len() {
        if bytes[index] == b']' {
            if index + 1 < bytes.len() && bytes[index + 1] == b']' {
                out.push(']');
                index += 2;
            } else {
                return Some((index + 1, out));
            }
        } else {
            out.push(bytes[index] as char);
            index += 1;
        }
    }
    None
}

fn parse_double_quoted_identifier(bytes: &[u8], start: usize) -> Option<(usize, String)> {
    let mut index = start + 1;
    let mut out = String::new();
    while index < bytes.len() {
        if bytes[index] == b'"' {
            if index + 1 < bytes.len() && bytes[index + 1] == b'"' {
                out.push('"');
                index += 2;
            } else {
                return Some((index + 1, out));
            }
        } else {
            out.push(bytes[index] as char);
            index += 1;
        }
    }
    None
}

fn skip_ascii_whitespace_and_comments(bytes: &[u8], mut index: usize) -> usize {
    loop {
        while index < bytes.len() && is_ascii_whitespace_byte(bytes[index]) {
            index += 1;
        }

        if index + 1 < bytes.len() && bytes[index] == b'-' && bytes[index + 1] == b'-' {
            index += 2;
            while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
                index += 1;
            }
            continue;
        }

        if index + 1 < bytes.len() && bytes[index] == b'/' && bytes[index + 1] == b'*' {
            index += 2;
            while index + 1 < bytes.len() {
                if bytes[index] == b'*' && bytes[index + 1] == b'/' {
                    index += 2;
                    break;
                }
                index += 1;
            }
            continue;
        }

        return index;
    }
}

fn is_ascii_whitespace_byte(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t' | 0x0b | 0x0c)
}

fn is_ascii_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::Dialect;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = TsqlSpPrefix;
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
        let placeholder = parse_sql("SELECT 1").expect("parse");
        let rule = TsqlSpPrefix;
        rule.check_with_context(
            &placeholder[0],
            &RuleContext::new(sql, 0..sql.len(), 0).with_dialect(Dialect::Mssql),
        )
    }

    #[test]
    fn flags_sp_prefixed_procedure_name() {
        let issues = run("CREATE PROCEDURE dbo.sp_legacy AS SELECT 1;");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_TQ_001);
    }

    #[test]
    fn does_not_flag_non_sp_prefixed_procedure_name() {
        let issues = run("CREATE PROCEDURE proc_legacy AS SELECT 1;");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_sp_prefixed_bracket_quoted_name_in_statementless_mode() {
        let issues = run_statementless("CREATE PROCEDURE dbo.[sp_legacy]\nAS\nSELECT 1");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_sp_prefixed_double_quoted_name_in_statementless_mode() {
        let issues = run_statementless("CREATE PROCEDURE dbo.\"sp_legacy\"\nAS\nSELECT 1");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn does_not_flag_non_sp_prefixed_quoted_names_in_statementless_mode() {
        let bracket = run_statementless("CREATE PROCEDURE dbo.[usp_legacy]\nAS\nSELECT 1");
        assert!(bracket.is_empty());
        let quoted = run_statementless("CREATE PROCEDURE dbo.\"usp_legacy\"\nAS\nSELECT 1");
        assert!(quoted.is_empty());
    }

    #[test]
    fn does_not_flag_sp_prefix_text_inside_string_literal() {
        let issues =
            run_statementless("SELECT 'CREATE PROCEDURE sp_legacy AS SELECT 1' AS sql_snippet");
        assert!(issues.is_empty());
    }
}
