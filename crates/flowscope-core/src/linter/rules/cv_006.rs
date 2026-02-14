//! LINT_CV_006: Statement terminator.
//!
//! Enforce consistent semicolon termination within a SQL document.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

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
        let tokens = tokenize_with_offsets_for_context(ctx);
        let semicolon = terminal_semicolon_info(ctx, tokens.as_deref());
        let has_terminal_semicolon = semicolon.is_some();

        if self.require_final_semicolon
            && is_last_statement(ctx, tokens.as_deref())
            && !has_terminal_semicolon
        {
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
            if statement_is_multiline(ctx, tokens.as_deref()) {
                let invalid_newline_style = !semicolon.newline_before_semicolon
                    || semicolon.newline_count_before_semicolon != 1
                    || semicolon.has_comment_before_semicolon
                    || statement_has_trailing_comment_before_semicolon(
                        ctx,
                        semicolon.semicolon_offset,
                        tokens.as_deref(),
                    );
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

fn statement_is_multiline(ctx: &LintContext, tokens: Option<&[LocatedToken]>) -> bool {
    let Some(tokens) = tokens else {
        return count_line_breaks(ctx.statement_sql()) > 0;
    };

    tokens
        .iter()
        .filter(|token| {
            token.start >= ctx.statement_range.start && token.end <= ctx.statement_range.end
        })
        .any(|token| is_multiline_trivia_token(&token.token))
}

fn is_multiline_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Newline)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

fn terminal_semicolon_info(
    ctx: &LintContext,
    tokens: Option<&[LocatedToken]>,
) -> Option<TerminalSemicolon> {
    let tokens = tokens?;
    let mut newline_count_before_semicolon = 0usize;
    let mut has_comment_before_semicolon = false;

    for token in tokens
        .iter()
        .filter(|token| token.start >= ctx.statement_range.end)
    {
        match &token.token {
            Token::SemiColon => {
                return Some(TerminalSemicolon {
                    semicolon_offset: token.start,
                    newline_before_semicolon: newline_count_before_semicolon > 0,
                    newline_count_before_semicolon,
                    has_comment_before_semicolon,
                });
            }
            trivia if is_trivia_token(trivia) => {
                newline_count_before_semicolon +=
                    count_line_breaks(&ctx.sql[token.start..token.end]);
                if is_comment_token(trivia) {
                    has_comment_before_semicolon = true;
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

fn is_last_statement(ctx: &LintContext, tokens: Option<&[LocatedToken]>) -> bool {
    let Some(tokens) = tokens else {
        return false;
    };
    for token in tokens
        .iter()
        .filter(|token| token.start >= ctx.statement_range.end)
    {
        if matches!(token.token, Token::SemiColon)
            || is_trivia_token(&token.token)
            || is_go_batch_separator(token, ctx.sql, ctx.dialect())
        {
            continue;
        }
        return false;
    }
    true
}

fn statement_has_trailing_comment_before_semicolon(
    ctx: &LintContext,
    semicolon: usize,
    tokens: Option<&[LocatedToken]>,
) -> bool {
    let Some(tokens) = tokens else {
        return false;
    };

    tokens
        .iter()
        .filter(|token| {
            token.start >= ctx.statement_range.start
                && token.end <= semicolon
                && !is_spacing_whitespace(&token.token)
        })
        .next_back()
        .is_some_and(|token| is_comment_token(&token.token))
}

struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn tokenize_with_offsets_for_context(ctx: &LintContext) -> Option<Vec<LocatedToken>> {
    let tokens = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        Some(
            tokens
                .iter()
                .filter_map(|token| {
                    token_with_span_offsets(ctx.sql, token).map(|(start, end)| LocatedToken {
                        token: token.token.clone(),
                        start,
                        end,
                    })
                })
                .collect::<Vec<_>>(),
        )
    });

    if let Some(tokens) = tokens {
        return Some(tokens);
    }

    tokenize_with_offsets(ctx.sql, ctx.dialect())
}

fn tokenize_with_offsets(sql: &str, dialect: Dialect) -> Option<Vec<LocatedToken>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        let Some((start, end)) = token_with_span_offsets(sql, &token) else {
            continue;
        };
        out.push(LocatedToken {
            token: token.token,
            start,
            end,
        });
    }

    Some(out)
}

fn is_comment_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

fn token_with_span_offsets(sql: &str, token: &TokenWithSpan) -> Option<(usize, usize)> {
    let start = line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )?;
    let end = line_col_to_offset(
        sql,
        token.span.end.line as usize,
        token.span.end.column as usize,
    )?;
    Some((start, end))
}

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

fn is_spacing_whitespace(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
    )
}

fn is_go_batch_separator(token: &LocatedToken, sql: &str, dialect: Dialect) -> bool {
    if dialect != Dialect::Mssql {
        return false;
    }
    let Token::Word(word) = &token.token else {
        return false;
    };
    if !word.value.eq_ignore_ascii_case("GO") {
        return false;
    }

    line_text(sql, token.start).is_some_and(|line| line.trim().eq_ignore_ascii_case("GO"))
}

fn line_text(sql: &str, offset: usize) -> Option<&str> {
    if offset > sql.len() {
        return None;
    }

    let line_start = sql[..offset].rfind('\n').map_or(0, |idx| idx + 1);
    let line_end = sql[offset..]
        .find('\n')
        .map_or(sql.len(), |idx| offset + idx);
    sql.get(line_start..line_end)
}

fn count_line_breaks(text: &str) -> usize {
    let mut count = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\n' {
            count += 1;
            continue;
        }
        if ch == '\r' {
            count += 1;
            if matches!(chars.peek(), Some('\n')) {
                let _ = chars.next();
            }
        }
    }
    count
}

fn line_col_to_offset(sql: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }

    let mut current_line = 1usize;
    let mut current_col = 1usize;

    for (offset, ch) in sql.char_indices() {
        if current_line == line && current_col == column {
            return Some(offset);
        }

        if ch == '\n' {
            current_line += 1;
            current_col = 1;
        } else {
            current_col += 1;
        }
    }

    if current_line == line && current_col == column {
        return Some(sql.len());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::rule::with_active_dialect;
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

    #[test]
    fn multiline_newline_flags_comment_before_semicolon() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.terminator".to_string(),
                serde_json::json!({"multiline_newline": true}),
            )]),
        };
        let rule = ConventionTerminator::from_config(&config);
        let sql = "SELECT a\nFROM foo\n-- trailing\n;";
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

    #[test]
    fn multiline_newline_flags_trailing_comment_inside_statement_range() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.terminator".to_string(),
                serde_json::json!({"multiline_newline": true}),
            )]),
        };
        let rule = ConventionTerminator::from_config(&config);
        let sql = "SELECT a\nFROM foo\n-- trailing\n;";
        let stmts = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &stmts[0],
            &LintContext {
                sql,
                statement_range: 0.."SELECT a\nFROM foo\n-- trailing".len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_006);
    }

    #[test]
    fn require_final_semicolon_flags_missing_semicolon_before_trailing_go_batch_separator() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.terminator".to_string(),
                serde_json::json!({"require_final_semicolon": true}),
            )]),
        };
        let rule = ConventionTerminator::from_config(&config);
        let stmt = &parse_sql("SELECT 1").expect("parse")[0];
        let sql = "SELECT 1\nGO\n";
        let issues = with_active_dialect(Dialect::Mssql, || {
            rule.check(
                stmt,
                &LintContext {
                    sql,
                    statement_range: 0.."SELECT 1".len(),
                    statement_index: 0,
                },
            )
        });
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_006);
    }

    #[test]
    fn require_final_semicolon_does_not_flag_non_last_statement_before_go_batch_separator() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.terminator".to_string(),
                serde_json::json!({"require_final_semicolon": true}),
            )]),
        };
        let rule = ConventionTerminator::from_config(&config);
        let stmt = &parse_sql("SELECT 1").expect("parse")[0];
        let sql = "SELECT 1\nGO\nSELECT 2;";
        let issues = with_active_dialect(Dialect::Mssql, || {
            rule.check(
                stmt,
                &LintContext {
                    sql,
                    statement_range: 0.."SELECT 1".len(),
                    statement_index: 0,
                },
            )
        });
        assert!(issues.is_empty());
    }

    #[test]
    fn multiline_newline_allows_newline_within_string_literal() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.terminator".to_string(),
                serde_json::json!({"multiline_newline": true}),
            )]),
        };
        let rule = ConventionTerminator::from_config(&config);
        let sql = "SELECT 'line1\nline2';";
        let stmt = &parse_sql(sql).expect("parse")[0];
        let issues = rule.check(
            stmt,
            &LintContext {
                sql,
                statement_range: 0.."SELECT 'line1\nline2'".len(),
                statement_index: 0,
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn statement_is_multiline_fallback_handles_crlf_line_breaks() {
        let sql = "SELECT\r\n  1";
        let ctx = LintContext {
            sql,
            statement_range: 0..sql.len(),
            statement_index: 0,
        };
        assert!(statement_is_multiline(&ctx, None));
    }
}
