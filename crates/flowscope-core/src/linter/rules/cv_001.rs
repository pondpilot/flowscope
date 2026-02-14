//! LINT_CV_001: Not-equal style.
//!
//! SQLFluff CV01 parity (current scope): flag statements that mix `<>` and
//! `!=` not-equal operators.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit::visit_expressions;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{BinaryOperator, Expr, Spanned, Statement};
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreferredNotEqualStyle {
    Consistent,
    CStyle,
    Ansi,
}

impl PreferredNotEqualStyle {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_CV_001, "preferred_not_equal_style")
            .unwrap_or("consistent")
            .to_ascii_lowercase()
            .as_str()
        {
            "c_style" => Self::CStyle,
            "ansi" => Self::Ansi,
            _ => Self::Consistent,
        }
    }

    fn violation(self, usage: &NotEqualUsage) -> bool {
        match self {
            Self::Consistent => usage.saw_angle_style && usage.saw_bang_style,
            Self::CStyle => usage.saw_angle_style,
            Self::Ansi => usage.saw_bang_style,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Consistent => "Use consistent not-equal style.",
            Self::CStyle => "Use `!=` for not-equal comparisons.",
            Self::Ansi => "Use `<>` for not-equal comparisons.",
        }
    }
}

pub struct ConventionNotEqual {
    preferred_style: PreferredNotEqualStyle,
}

impl ConventionNotEqual {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            preferred_style: PreferredNotEqualStyle::from_config(config),
        }
    }
}

impl Default for ConventionNotEqual {
    fn default() -> Self {
        Self {
            preferred_style: PreferredNotEqualStyle::Consistent,
        }
    }
}

impl LintRule for ConventionNotEqual {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_001
    }

    fn name(&self) -> &'static str {
        "Not-equal style"
    }

    fn description(&self) -> &'static str {
        "Use a consistent not-equal operator style."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let tokens =
            tokenized_for_context(ctx).or_else(|| tokenized(ctx.statement_sql(), ctx.dialect()));
        let usage = statement_not_equal_usage_with_tokens(
            statement,
            ctx.statement_sql(),
            tokens.as_deref(),
        );
        if self.preferred_style.violation(&usage) {
            vec![
                Issue::info(issue_codes::LINT_CV_001, self.preferred_style.message())
                    .with_statement(ctx.statement_index),
            ]
        } else {
            Vec::new()
        }
    }
}

#[derive(Default)]
struct NotEqualUsage {
    saw_angle_style: bool,
    saw_bang_style: bool,
}

enum NotEqualStyle {
    Angle,
    Bang,
}

fn statement_not_equal_usage_with_tokens(
    statement: &Statement,
    sql: &str,
    tokens: Option<&[LocatedToken]>,
) -> NotEqualUsage {
    let mut usage = NotEqualUsage::default();
    visit_expressions(statement, &mut |expr| {
        if usage.saw_angle_style && usage.saw_bang_style {
            return;
        }

        let style = match expr {
            Expr::BinaryOp { left, op, right } if *op == BinaryOperator::NotEq => {
                not_equal_style_between(sql, left.as_ref(), right.as_ref(), tokens)
            }
            Expr::AnyOp {
                left,
                compare_op,
                right,
                ..
            } if *compare_op == BinaryOperator::NotEq => {
                not_equal_style_between(sql, left.as_ref(), right.as_ref(), tokens)
            }
            Expr::AllOp {
                left,
                compare_op,
                right,
            } if *compare_op == BinaryOperator::NotEq => {
                not_equal_style_between(sql, left.as_ref(), right.as_ref(), tokens)
            }
            _ => None,
        };

        match style {
            Some(NotEqualStyle::Angle) => usage.saw_angle_style = true,
            Some(NotEqualStyle::Bang) => usage.saw_bang_style = true,
            None => {}
        }
    });

    usage
}

fn not_equal_style_between(
    sql: &str,
    left: &Expr,
    right: &Expr,
    tokens: Option<&[LocatedToken]>,
) -> Option<NotEqualStyle> {
    let left_end = left.span().end;
    let right_start = right.span().start;
    if left_end.line == 0
        || left_end.column == 0
        || right_start.line == 0
        || right_start.column == 0
    {
        return None;
    }

    let start = line_col_to_offset(sql, left_end.line as usize, left_end.column as usize)?;
    let end = line_col_to_offset(sql, right_start.line as usize, right_start.column as usize)?;
    if end < start {
        return None;
    }

    if let Some(tokens) = tokens {
        return not_equal_style_in_tokens(sql, tokens, start, end);
    }

    None
}

fn not_equal_style_in_tokens(
    sql: &str,
    tokens: &[LocatedToken],
    start: usize,
    end: usize,
) -> Option<NotEqualStyle> {
    for token in tokens {
        if token.end <= start || token.start >= end {
            continue;
        }
        if is_trivia_token(&token.token) {
            continue;
        }

        if !matches!(token.token, Token::Neq) {
            return None;
        }
        if token.end > sql.len() {
            return None;
        }

        let raw = &sql[token.start..token.end];
        return match raw {
            "<>" => Some(NotEqualStyle::Angle),
            "!=" => Some(NotEqualStyle::Bang),
            _ => None,
        };
    }

    None
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn tokenized(sql: &str, dialect: crate::types::Dialect) -> Option<Vec<LocatedToken>> {
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

fn tokenized_for_context(ctx: &LintContext) -> Option<Vec<LocatedToken>> {
    let statement_start = ctx.statement_range.start;
    let from_document = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        Some(
            tokens
                .iter()
                .filter_map(|token| {
                    let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                        return None;
                    };
                    if start < ctx.statement_range.start || end > ctx.statement_range.end {
                        return None;
                    }

                    Some(LocatedToken {
                        token: token.token.clone(),
                        start: start - statement_start,
                        end: end - statement_start,
                    })
                })
                .collect::<Vec<_>>(),
        )
    });

    if let Some(tokens) = from_document {
        return Some(tokens);
    }

    tokenized(ctx.statement_sql(), ctx.dialect())
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
        Some(sql.len())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionNotEqual::default();
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
    fn flags_mixed_not_equal_styles() {
        let issues = run("SELECT * FROM t WHERE a <> b AND c != d");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_001);
    }

    #[test]
    fn does_not_flag_single_not_equal_style() {
        assert!(run("SELECT * FROM t WHERE a <> b").is_empty());
        assert!(run("SELECT * FROM t WHERE a != b").is_empty());
    }

    #[test]
    fn does_not_flag_not_equal_tokens_inside_string_literal() {
        assert!(run("SELECT 'a <> b and c != d' AS txt FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_not_equal_tokens_inside_comments() {
        assert!(run("SELECT * FROM t -- a <> b and c != d").is_empty());
    }

    #[test]
    fn c_style_preference_flags_angle_bracket_operator() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.not_equal".to_string(),
                serde_json::json!({"preferred_not_equal_style": "c_style"}),
            )]),
        };
        let rule = ConventionNotEqual::from_config(&config);
        let statements = parse_sql("SELECT * FROM t WHERE a <> b").expect("parse");
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql: "SELECT * FROM t WHERE a <> b",
                statement_range: 0.."SELECT * FROM t WHERE a <> b".len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn ansi_preference_flags_bang_operator() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_001".to_string(),
                serde_json::json!({"preferred_not_equal_style": "ansi"}),
            )]),
        };
        let rule = ConventionNotEqual::from_config(&config);
        let statements = parse_sql("SELECT * FROM t WHERE a != b").expect("parse");
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql: "SELECT * FROM t WHERE a != b",
                statement_range: 0.."SELECT * FROM t WHERE a != b".len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
    }
}
