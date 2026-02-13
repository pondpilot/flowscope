//! LINT_AL_002: Column alias style.
//!
//! Require explicit `AS` when aliasing SELECT expressions.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;

pub struct AliasingColumnStyle;

impl LintRule for AliasingColumnStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_002
    }

    fn name(&self) -> &'static str {
        "Column alias style"
    }

    fn description(&self) -> &'static str {
        "Implicit/explicit aliasing of columns."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = ctx.statement_sql();
        let mut issues = Vec::new();

        for (clause, clause_start) in select_clauses_with_spans(sql) {
            for (item, item_start) in split_top_level_commas_with_offsets(&clause) {
                if item_has_implicit_alias(&item) {
                    let start = clause_start + item_start;
                    let end = start + item.len();
                    issues.push(
                        Issue::info(
                            issue_codes::LINT_AL_002,
                            "Use explicit AS when aliasing columns.",
                        )
                        .with_statement(ctx.statement_index)
                        .with_span(ctx.span_from_statement_offset(start, end)),
                    );
                }
            }
        }

        issues
    }
}

fn has_re(haystack: &str, pattern: &str) -> bool {
    Regex::new(pattern).expect("valid regex").is_match(haystack)
}

fn select_clauses_with_spans(sql: &str) -> Vec<(String, usize)> {
    Regex::new(r"(?is)\bselect\b(.*?)\bfrom\b")
        .expect("valid regex")
        .captures_iter(sql)
        .filter_map(|caps| caps.get(1).map(|m| (m.as_str().to_string(), m.start())))
        .collect()
}

fn split_top_level_commas_with_offsets(input: &str) -> Vec<(String, usize)> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut part_start = 0usize;

    for (idx, ch) in input.char_indices() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(ch);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(ch);
            }
            '(' if !in_single && !in_double => {
                depth += 1;
                current.push(ch);
            }
            ')' if !in_single && !in_double && depth > 0 => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !in_single && !in_double && depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    let trim_offset = current.find(trimmed.as_str()).unwrap_or(0);
                    parts.push((trimmed, part_start + trim_offset));
                }
                current.clear();
                part_start = idx + ch.len_utf8();
            }
            _ => current.push(ch),
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        let trim_offset = current.find(trimmed.as_str()).unwrap_or(0);
        parts.push((trimmed, part_start + trim_offset));
    }

    parts
}

fn item_has_as_alias(item: &str) -> bool {
    has_re(item, r"(?i)\bas\s+[A-Za-z_][A-Za-z0-9_]*\s*$")
}

fn is_keyword(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "SELECT"
            | "FROM"
            | "WHERE"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "FULL"
            | "OUTER"
            | "INNER"
            | "CROSS"
            | "ON"
            | "USING"
            | "AS"
            | "GROUP"
            | "ORDER"
            | "HAVING"
            | "LIMIT"
            | "OFFSET"
            | "UNION"
            | "ALL"
            | "DISTINCT"
            | "BY"
            | "WHEN"
            | "THEN"
            | "ELSE"
            | "END"
    )
}

fn item_has_implicit_alias(item: &str) -> bool {
    let trimmed = item.trim();
    if trimmed.is_empty() || trimmed == "*" || trimmed.ends_with(".*") || item_has_as_alias(trimmed)
    {
        return false;
    }

    let mut depth = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut split_at: Option<usize> = None;

    for (idx, ch) in trimmed.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '(' if !in_single && !in_double => depth += 1,
            ')' if !in_single && !in_double && depth > 0 => depth -= 1,
            c if c.is_whitespace() && !in_single && !in_double && depth == 0 => {
                split_at = Some(idx)
            }
            _ => {}
        }
    }

    let Some(split_idx) = split_at else {
        return false;
    };

    let expr = trimmed[..split_idx].trim_end();
    let alias = trimmed[split_idx..].trim_start();
    if expr.is_empty()
        || alias.is_empty()
        || !has_re(alias, r"(?i)^[A-Za-z_][A-Za-z0-9_]*$")
        || is_keyword(alias)
    {
        return false;
    }

    let expr_ends_with_operator = [
        '+', '-', '*', '/', '%', '^', '|', '&', '=', '<', '>', ',', '(',
    ]
    .iter()
    .any(|ch| expr.ends_with(*ch));

    !expr_ends_with_operator
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).expect("parse");
        let rule = AliasingColumnStyle;
        stmts
            .iter()
            .enumerate()
            .flat_map(|(index, stmt)| {
                rule.check(
                    stmt,
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
    fn flags_implicit_column_alias() {
        let issues = run("select a + 1 total from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_002);
    }

    #[test]
    fn allows_explicit_column_alias() {
        let issues = run("select a + 1 as total from t");
        assert!(issues.is_empty());
    }
}
