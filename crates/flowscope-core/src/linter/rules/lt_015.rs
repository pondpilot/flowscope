//! LINT_LT_015: Layout newlines.
//!
//! SQLFluff LT15 parity (current scope): detect excessive blank lines.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

pub struct LayoutNewlines {
    maximum_empty_lines_inside_statements: usize,
    maximum_empty_lines_between_statements: usize,
}

impl LayoutNewlines {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            maximum_empty_lines_inside_statements: config
                .rule_option_usize(
                    issue_codes::LINT_LT_015,
                    "maximum_empty_lines_inside_statements",
                )
                .unwrap_or(1),
            maximum_empty_lines_between_statements: config
                .rule_option_usize(
                    issue_codes::LINT_LT_015,
                    "maximum_empty_lines_between_statements",
                )
                .unwrap_or(1),
        }
    }
}

impl Default for LayoutNewlines {
    fn default() -> Self {
        Self {
            maximum_empty_lines_inside_statements: 1,
            maximum_empty_lines_between_statements: 1,
        }
    }
}

impl LintRule for LayoutNewlines {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_015
    }

    fn name(&self) -> &'static str {
        "Layout newlines"
    }

    fn description(&self) -> &'static str {
        "Avoid excessive blank lines."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let statement_sql = ctx
            .statement_sql()
            .trim_matches(|ch: char| ch.is_ascii_whitespace());
        let excessive_inside = max_consecutive_blank_lines(statement_sql, ctx.dialect())
            > self.maximum_empty_lines_inside_statements;
        let excessive_between = ctx.statement_index > 0
            && max_consecutive_blank_lines(
                inter_statement_gap(ctx.sql, ctx.statement_range.start),
                ctx.dialect(),
            ) > self.maximum_empty_lines_between_statements;

        if excessive_inside || excessive_between {
            vec![Issue::info(
                issue_codes::LINT_LT_015,
                "SQL contains excessive blank lines.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn max_consecutive_blank_lines(sql: &str, dialect: Dialect) -> usize {
    max_consecutive_blank_lines_tokenized(sql, dialect)
        .unwrap_or_else(|| max_consecutive_blank_lines_fallback(sql))
}

fn max_consecutive_blank_lines_tokenized(sql: &str, dialect: Dialect) -> Option<usize> {
    if sql.is_empty() {
        return Some(0);
    }

    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut non_blank_lines = std::collections::BTreeSet::new();
    for token in tokens {
        if is_spacing_whitespace_token(&token.token) {
            continue;
        }
        let start_line = token.span.start.line as usize;
        let end_line = token.span.end.line as usize;
        for line in start_line..=end_line {
            non_blank_lines.insert(line);
        }
    }

    let mut blank_run = 0usize;
    let mut max_run = 0usize;
    let line_count = sql.lines().count();

    for line in 1..=line_count {
        if non_blank_lines.contains(&line) {
            blank_run = 0;
        } else {
            blank_run += 1;
            max_run = max_run.max(blank_run);
        }
    }

    Some(max_run)
}

fn max_consecutive_blank_lines_fallback(sql: &str) -> usize {
    let mut blank_run = 0usize;
    let mut max_run = 0usize;

    for line in sql.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            max_run = max_run.max(blank_run);
        } else {
            blank_run = 0;
        }
    }

    max_run
}

fn is_spacing_whitespace_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
    )
}

fn inter_statement_gap(sql: &str, statement_start: usize) -> &str {
    let before = &sql[..statement_start];
    let boundary = before
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_ascii_whitespace())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);
    &sql[boundary..statement_start]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;

    fn run_with_rule(sql: &str, rule: &LayoutNewlines) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let mut ranges = Vec::with_capacity(statements.len());
        let mut search_start = 0usize;
        for index in 0..statements.len() {
            if index > 0 {
                search_start = first_non_whitespace_offset(sql, search_start);
            }
            let end = if index + 1 < statements.len() {
                sql[search_start..]
                    .find(';')
                    .map(|offset| search_start + offset + 1)
                    .unwrap_or(sql.len())
            } else {
                sql.len()
            };
            ranges.push(search_start..end);
            search_start = end;
        }

        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check(
                    statement,
                    &LintContext {
                        sql,
                        statement_range: ranges[index].clone(),
                        statement_index: index,
                    },
                )
            })
            .collect()
    }

    fn first_non_whitespace_offset(sql: &str, from: usize) -> usize {
        let mut offset = from;
        for ch in sql[from..].chars() {
            if ch.is_ascii_whitespace() {
                offset += ch.len_utf8();
            } else {
                break;
            }
        }
        offset
    }

    fn run(sql: &str) -> Vec<Issue> {
        run_with_rule(sql, &LayoutNewlines::default())
    }

    #[test]
    fn flags_excessive_blank_lines() {
        let issues = run("SELECT 1\n\n\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }

    #[test]
    fn does_not_flag_single_blank_line() {
        assert!(run("SELECT 1\n\nFROM t").is_empty());
    }

    #[test]
    fn flags_blank_lines_with_whitespace() {
        let issues = run("SELECT 1\n\n   \nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }

    #[test]
    fn configured_inside_limit_allows_two_blank_lines() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.newlines".to_string(),
                serde_json::json!({"maximum_empty_lines_inside_statements": 2}),
            )]),
        };
        let issues = run_with_rule(
            "SELECT 1\n\n\nFROM t",
            &LayoutNewlines::from_config(&config),
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn configured_between_limit_flags_statement_gap() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_LT_015".to_string(),
                serde_json::json!({"maximum_empty_lines_between_statements": 1}),
            )]),
        };
        let issues = run_with_rule(
            "SELECT 1;\n\n\nSELECT 2",
            &LayoutNewlines::from_config(&config),
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }
}
