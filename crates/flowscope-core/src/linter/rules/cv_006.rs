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
        let semicolon = terminal_semicolon_info(ctx);
        let has_terminal_semicolon = semicolon.is_some();

        if self.require_final_semicolon && is_last_statement(ctx) && !has_terminal_semicolon {
            return vec![Issue::info(
                issue_codes::LINT_CV_006,
                "Final statement must end with a semi-colon.",
            )
            .with_statement(ctx.statement_index)];
        }

        let Some(semicolon) = semicolon else {
            return Vec::new();
        };

        if self.multiline_newline {
            if statement_is_multiline(ctx) {
                let invalid_newline_style = !semicolon.newline_before_semicolon
                    || semicolon.newline_count_before_semicolon != 1
                    || semicolon.has_comment_before_semicolon
                    || statement_has_trailing_block_comment(ctx)
                    || statement_has_detached_trailing_line_comment(ctx);
                if invalid_newline_style {
                    return vec![Issue::info(
                        issue_codes::LINT_CV_006,
                        "Multi-line statements must place the semi-colon on a new line.",
                    )
                    .with_statement(ctx.statement_index)];
                }
            } else if semicolon.semicolon_offset != ctx.statement_range.end {
                return vec![Issue::info(
                    issue_codes::LINT_CV_006,
                    "Statement terminator style is inconsistent.",
                )
                .with_statement(ctx.statement_index)];
            }
            return Vec::new();
        }

        if semicolon.semicolon_offset != ctx.statement_range.end {
            return vec![Issue::info(
                issue_codes::LINT_CV_006,
                "Statement terminator style is inconsistent.",
            )
            .with_statement(ctx.statement_index)];
        }

        Vec::new()
    }
}

fn statement_is_multiline(ctx: &LintContext) -> bool {
    ctx.statement_sql().contains('\n')
}

fn terminal_semicolon_info(ctx: &LintContext) -> Option<TerminalSemicolon> {
    let bytes = ctx.sql.as_bytes();
    let mut idx = ctx.statement_range.end;
    let mut newline_count_before_semicolon = 0usize;
    let mut has_comment_before_semicolon = false;

    while idx < bytes.len() {
        match bytes[idx] {
            b';' => {
                return Some(TerminalSemicolon {
                    semicolon_offset: idx,
                    newline_before_semicolon: newline_count_before_semicolon > 0,
                    newline_count_before_semicolon,
                    has_comment_before_semicolon,
                });
            }
            b' ' | b'\t' => {
                idx += 1;
            }
            b'\n' => {
                newline_count_before_semicolon += 1;
                idx += 1;
            }
            b'\r' => {
                newline_count_before_semicolon += 1;
                idx += 1;
                if idx < bytes.len() && bytes[idx] == b'\n' {
                    idx += 1;
                }
            }
            b'-' if idx + 1 < bytes.len() && bytes[idx + 1] == b'-' => {
                has_comment_before_semicolon = true;
                idx += 2;
                while idx < bytes.len() && bytes[idx] != b'\n' && bytes[idx] != b'\r' {
                    idx += 1;
                }
            }
            b'#' => {
                has_comment_before_semicolon = true;
                idx += 1;
                while idx < bytes.len() && bytes[idx] != b'\n' && bytes[idx] != b'\r' {
                    idx += 1;
                }
            }
            b'/' if idx + 1 < bytes.len() && bytes[idx + 1] == b'*' => {
                has_comment_before_semicolon = true;
                idx += 2;
                while idx + 1 < bytes.len() {
                    if bytes[idx] == b'*' && bytes[idx + 1] == b'/' {
                        idx += 2;
                        break;
                    }
                    if bytes[idx] == b'\n' {
                        newline_count_before_semicolon += 1;
                    } else if bytes[idx] == b'\r' {
                        newline_count_before_semicolon += 1;
                        if idx + 1 < bytes.len() && bytes[idx + 1] == b'\n' {
                            idx += 1;
                        }
                    }
                    idx += 1;
                }
            }
            _ => return None,
        }
    }

    None
}

struct TerminalSemicolon {
    semicolon_offset: usize,
    newline_before_semicolon: bool,
    newline_count_before_semicolon: usize,
    has_comment_before_semicolon: bool,
}

fn statement_has_trailing_block_comment(ctx: &LintContext) -> bool {
    ctx.statement_sql().trim_end().ends_with("*/")
}

fn statement_has_detached_trailing_line_comment(ctx: &LintContext) -> bool {
    let mut non_empty_lines: Vec<&str> = Vec::new();
    for line in ctx.statement_sql().lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            non_empty_lines.push(trimmed);
        }
    }

    if non_empty_lines.len() < 2 {
        return false;
    }

    non_empty_lines
        .last()
        .is_some_and(|line| line.starts_with("--") || line.starts_with('#'))
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
    fn default_allows_missing_final_semicolon_in_multi_statement_file() {
        let issues = run("select 1; select 2");
        assert!(issues.is_empty());
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

    #[test]
    fn default_flags_space_before_semicolon() {
        let sql = "SELECT a FROM foo  ;";
        let stmts = parse_sql(sql).expect("parse");
        let issues = ConventionTerminator::default().check(
            &stmts[0],
            &LintContext {
                sql,
                statement_range: 0.."SELECT a FROM foo".len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_006);
    }

    #[test]
    fn multiline_newline_flags_extra_blank_line_before_semicolon() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.terminator".to_string(),
                serde_json::json!({"multiline_newline": true}),
            )]),
        };
        let rule = ConventionTerminator::from_config(&config);
        let sql = "SELECT a\nFROM foo\n\n;";
        let stmts = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &stmts[0],
            &LintContext {
                sql,
                statement_range: 0.."SELECT a\nFROM foo".len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_006);
    }
}
