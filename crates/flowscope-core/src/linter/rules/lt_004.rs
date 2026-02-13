//! LINT_LT_004: Layout commas.
//!
//! SQLFluff LT04 parity (current scope): detect compact or leading-space comma
//! patterns.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommaLinePosition {
    Trailing,
    Leading,
}

impl CommaLinePosition {
    fn from_config(config: &LintConfig) -> Self {
        if let Some(value) = config.rule_option_str(issue_codes::LINT_LT_004, "line_position") {
            return match value.to_ascii_lowercase().as_str() {
                "leading" => Self::Leading,
                _ => Self::Trailing,
            };
        }

        // SQLFluff legacy compatibility (`trailing`/`leading`).
        match config
            .rule_option_str(issue_codes::LINT_LT_004, "comma_style")
            .unwrap_or("trailing")
            .to_ascii_lowercase()
            .as_str()
        {
            "leading" => Self::Leading,
            _ => Self::Trailing,
        }
    }
}

pub struct LayoutCommas {
    line_position: CommaLinePosition,
}

impl LayoutCommas {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            line_position: CommaLinePosition::from_config(config),
        }
    }
}

impl Default for LayoutCommas {
    fn default() -> Self {
        Self {
            line_position: CommaLinePosition::Trailing,
        }
    }
}

impl LintRule for LayoutCommas {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_004
    }

    fn name(&self) -> &'static str {
        "Layout commas"
    }

    fn description(&self) -> &'static str {
        "Comma spacing should be consistent."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if has_inconsistent_comma_spacing(ctx.statement_sql(), self.line_position) {
            vec![Issue::info(
                issue_codes::LINT_LT_004,
                "Comma spacing appears inconsistent.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn has_inconsistent_comma_spacing(sql: &str, line_position: CommaLinePosition) -> bool {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return false;
    };

    for (index, token) in tokens.iter().enumerate() {
        if !matches!(token.token, Token::Comma) {
            continue;
        }

        let prev_sig_idx = tokens[..index]
            .iter()
            .rposition(|candidate| !is_trivia_token(&candidate.token));
        let Some(prev_sig_idx) = prev_sig_idx else {
            continue;
        };
        let next_sig_idx = tokens
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(_, candidate)| !is_trivia_token(&candidate.token))
            .map(|(idx, _)| idx);
        let Some(next_sig_idx) = next_sig_idx else {
            continue;
        };

        let comma_line = token.span.start.line;
        let prev_line = tokens[prev_sig_idx].span.end.line;
        let next_line = tokens[next_sig_idx].span.start.line;
        let line_break_before = prev_line < comma_line;
        let line_break_after = next_line > comma_line;

        let line_position_violation = match line_position {
            CommaLinePosition::Trailing => line_break_before && !line_break_after,
            CommaLinePosition::Leading => line_break_after && !line_break_before,
        };
        if line_position_violation {
            return true;
        }

        // Inline comma cases should have no pre-comma spacing.
        if prev_line == comma_line
            && tokens[prev_sig_idx + 1..index]
                .iter()
                .any(|candidate| is_inline_space_token(&candidate.token))
        {
            return true;
        }

        // Inline comma cases should have spacing after comma.
        if next_line == comma_line
            && !tokens[index + 1..next_sig_idx]
                .iter()
                .any(|candidate| is_inline_space_token(&candidate.token))
        {
            return true;
        }
    }

    false
}

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

fn is_inline_space_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;

    fn run_with_rule(sql: &str, rule: &LayoutCommas) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
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

    fn run(sql: &str) -> Vec<Issue> {
        run_with_rule(sql, &LayoutCommas::default())
    }

    #[test]
    fn flags_tight_comma_spacing() {
        let issues = run("SELECT a,b FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_004);
    }

    #[test]
    fn does_not_flag_spaced_commas() {
        assert!(run("SELECT a, b FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_comma_inside_string_literal() {
        assert!(run("SELECT 'a,b' AS txt, b FROM t").is_empty());
    }

    #[test]
    fn leading_line_position_flags_trailing_line_comma() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.commas".to_string(),
                serde_json::json!({"line_position": "leading"}),
            )]),
        };
        let issues = run_with_rule("SELECT a,\n b FROM t", &LayoutCommas::from_config(&config));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_004);
    }

    #[test]
    fn legacy_comma_style_leading_is_respected() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_LT_004".to_string(),
                serde_json::json!({"comma_style": "leading"}),
            )]),
        };
        let issues = run_with_rule("SELECT a\n, b FROM t", &LayoutCommas::from_config(&config));
        assert!(issues.is_empty());
    }
}
