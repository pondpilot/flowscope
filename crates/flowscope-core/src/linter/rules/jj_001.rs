//! LINT_JJ_001: Jinja padding.
//!
//! SQLFluff JJ01 parity (current scope): detect inconsistent whitespace around
//! Jinja delimiters.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

pub struct JinjaPadding;

impl LintRule for JinjaPadding {
    fn code(&self) -> &'static str {
        issue_codes::LINT_JJ_001
    }

    fn name(&self) -> &'static str {
        "Jinja padding"
    }

    fn description(&self) -> &'static str {
        "Jinja tags should use consistent padding."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_violation = has_inconsistent_jinja_padding(ctx.statement_sql());

        if has_violation {
            vec![Issue::info(
                issue_codes::LINT_JJ_001,
                "Jinja tag spacing appears inconsistent.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn has_inconsistent_jinja_padding(sql: &str) -> bool {
    let bytes = sql.as_bytes();

    let mut i = 0usize;
    while i + 2 <= bytes.len() {
        if is_jinja_open_delimiter(bytes[i], bytes[i + 1]) {
            if i + 2 < bytes.len() && !is_padding_or_trim_marker(bytes[i + 2]) {
                return true;
            }
            i += 2;
            continue;
        }

        if is_jinja_close_delimiter(bytes[i], bytes[i + 1]) {
            if i > 0 && !is_padding_or_trim_marker(bytes[i - 1]) {
                return true;
            }
            i += 2;
            continue;
        }

        i += 1;
    }

    false
}

fn is_jinja_open_delimiter(first: u8, second: u8) -> bool {
    first == b'{' && matches!(second, b'{' | b'%' | b'#')
}

fn is_jinja_close_delimiter(first: u8, second: u8) -> bool {
    (first == b'}' && second == b'}')
        || (first == b'%' && second == b'}')
        || (first == b'#' && second == b'}')
}

fn is_padding_or_trim_marker(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\t' | b'-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = JinjaPadding;
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
    fn flags_missing_padding_in_jinja_expression() {
        let issues = run("SELECT '{{foo}}' AS templated");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
    }

    #[test]
    fn does_not_flag_padded_jinja_expression() {
        assert!(run("SELECT '{{ foo }}' AS templated").is_empty());
    }

    #[test]
    fn flags_missing_padding_in_jinja_statement_tag() {
        let issues = run("SELECT '{%for x in y %}' AS templated");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
    }

    #[test]
    fn flags_missing_padding_before_statement_close_tag() {
        let issues = run("SELECT '{% for x in y%}' AS templated");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
    }

    #[test]
    fn flags_missing_padding_in_jinja_comment_tag() {
        let issues = run("SELECT '{#comment#}' AS templated");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_JJ_001);
    }

    #[test]
    fn allows_jinja_trim_markers() {
        assert!(run("SELECT '{{- foo -}}' AS templated").is_empty());
        assert!(run("SELECT '{%- if x -%}' AS templated").is_empty());
    }
}
