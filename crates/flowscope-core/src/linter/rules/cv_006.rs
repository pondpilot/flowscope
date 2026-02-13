//! LINT_CV_006: Statement terminator.
//!
//! Enforce consistent semicolon termination within a SQL document.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

#[derive(Default)]
pub struct ConventionTerminator {
    multiline_newline: bool,
    require_final_semicolon: bool,
}

impl ConventionTerminator {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            multiline_newline: config
                .rule_option_bool(issue_codes::LINT_CV_006, "multiline_newline")
                .unwrap_or(false),
            require_final_semicolon: config
                .rule_option_bool(issue_codes::LINT_CV_006, "require_final_semicolon")
                .unwrap_or(false),
        }
    }
}

impl LintRule for ConventionTerminator {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_006
    }

    fn name(&self) -> &'static str {
        "Statement terminator"
    }

    fn description(&self) -> &'static str {
        "Statements must end with a semi-colon."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_terminal_semicolon = terminal_semicolon_info(ctx).is_some();

        if self.require_final_semicolon && is_last_statement(ctx) && !has_terminal_semicolon {
            return vec![Issue::info(
                issue_codes::LINT_CV_006,
                "Final statement must end with a semi-colon.",
            )
            .with_statement(ctx.statement_index)];
        }

        if self.multiline_newline
            && statement_is_multiline(ctx)
            && has_terminal_semicolon
            && !semicolon_on_newline(ctx)
        {
            vec![Issue::info(
                issue_codes::LINT_CV_006,
                "Multi-line statements must place the semi-colon on a new line.",
            )
            .with_statement(ctx.statement_index)]
        } else if ctx.sql.contains(';') && !has_terminal_semicolon {
            vec![Issue::info(
                issue_codes::LINT_CV_006,
                "Statement terminator style is inconsistent.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn statement_is_multiline(ctx: &LintContext) -> bool {
    ctx.statement_sql().contains('\n')
}

fn semicolon_on_newline(ctx: &LintContext) -> bool {
    terminal_semicolon_info(ctx).is_some_and(|info| info.newline_before_semicolon)
}

fn terminal_semicolon_info(ctx: &LintContext) -> Option<TerminalSemicolon> {
    let statement_sql = ctx.statement_sql();
    let trimmed = statement_sql.trim_end_matches(|ch: char| ch.is_ascii_whitespace());
    if let Some(without_semicolon) = trimmed.strip_suffix(';') {
        let mut newline_before_semicolon = false;
        for ch in without_semicolon.chars().rev() {
            if ch == '\n' || ch == '\r' {
                newline_before_semicolon = true;
                break;
            }
            if !ch.is_ascii_whitespace() {
                break;
            }
        }
        return Some(TerminalSemicolon {
            newline_before_semicolon,
        });
    }

    let bytes = ctx.sql.as_bytes();
    let mut idx = ctx.statement_range.end;
    let mut newline_before_semicolon = false;
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        if bytes[idx] == b'\n' || bytes[idx] == b'\r' {
            newline_before_semicolon = true;
        }
        idx += 1;
    }

    (idx < bytes.len() && bytes[idx] == b';').then_some(TerminalSemicolon {
        newline_before_semicolon,
    })
}

struct TerminalSemicolon {
    newline_before_semicolon: bool,
}

fn is_last_statement(ctx: &LintContext) -> bool {
    let bytes = ctx.sql.as_bytes();
    let mut idx = ctx.statement_range.end;

    while idx < bytes.len() {
        match bytes[idx] {
            b' ' | b'\t' | b'\r' | b'\n' | b';' => idx += 1,
            b'-' if idx + 1 < bytes.len() && bytes[idx + 1] == b'-' => {
                idx += 2;
                while idx < bytes.len() && bytes[idx] != b'\n' && bytes[idx] != b'\r' {
                    idx += 1;
                }
            }
            b'#' => {
                idx += 1;
                while idx < bytes.len() && bytes[idx] != b'\n' && bytes[idx] != b'\r' {
                    idx += 1;
                }
            }
            b'/' if idx + 1 < bytes.len() && bytes[idx + 1] == b'*' => {
                idx += 2;
                while idx + 1 < bytes.len() {
                    if bytes[idx] == b'*' && bytes[idx + 1] == b'/' {
                        idx += 2;
                        break;
                    }
                    idx += 1;
                }
            }
            _ => return false,
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).expect("parse");
        let rule = ConventionTerminator::default();
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
    fn flags_when_file_has_mixed_terminator_style() {
        let issues = run("select 1; select 2");
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_006);
    }

    #[test]
    fn allows_consistent_terminated_statements() {
        let issues = run("select 1; select 2;");
        assert!(issues.is_empty());
    }

    #[test]
    fn require_final_semicolon_flags_last_statement_without_terminator() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.terminator".to_string(),
                serde_json::json!({"require_final_semicolon": true}),
            )]),
        };
        let rule = ConventionTerminator::from_config(&config);
        let sql = "SELECT 1";
        let stmts = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &stmts[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_006);
    }

    #[test]
    fn multiline_newline_flags_inline_semicolon_for_multiline_statement() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_006".to_string(),
                serde_json::json!({"multiline_newline": true}),
            )]),
        };
        let rule = ConventionTerminator::from_config(&config);
        let sql = "SELECT\n  1;";
        let stmts = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &stmts[0],
            &LintContext {
                sql,
                statement_range: 0.."SELECT\n  1".len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_006);
    }
}
