//! LINT_CV_001: Not-equal style.
//!
//! SQLFluff CV01 parity (current scope): flag statements that mix `<>` and
//! `!=` not-equal operators.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

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

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let usage = statement_not_equal_usage(ctx.statement_sql(), ctx.dialect());
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

fn statement_not_equal_usage(sql: &str, dialect: Dialect) -> NotEqualUsage {
    let mut usage = NotEqualUsage::default();

    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return usage;
    };

    for token in &tokens {
        if !matches!(token.token, Token::Neq) {
            continue;
        }

        match not_equal_style_for_token(sql, token) {
            Some(NotEqualStyle::Angle) => usage.saw_angle_style = true,
            Some(NotEqualStyle::Bang) => usage.saw_bang_style = true,
            None => {}
        }

        if usage.saw_angle_style && usage.saw_bang_style {
            return usage;
        }
    }

    usage
}

fn not_equal_style_for_token(sql: &str, token: &TokenWithSpan) -> Option<NotEqualStyle> {
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
    let raw = sql.get(start..end)?;
    match raw {
        "<>" => Some(NotEqualStyle::Angle),
        "!=" => Some(NotEqualStyle::Bang),
        _ => None,
    }
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
