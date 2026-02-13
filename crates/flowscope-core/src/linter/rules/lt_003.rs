//! LINT_LT_003: Layout operators.
//!
//! SQLFluff LT03 parity (current scope): flag trailing operators at end of line.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperatorLinePosition {
    Leading,
    Trailing,
}

impl OperatorLinePosition {
    fn from_config(config: &LintConfig) -> Self {
        if let Some(value) = config.rule_option_str(issue_codes::LINT_LT_003, "line_position") {
            return match value.to_ascii_lowercase().as_str() {
                "trailing" => Self::Trailing,
                _ => Self::Leading,
            };
        }

        // SQLFluff legacy compatibility (`before`/`after`).
        match config
            .rule_option_str(issue_codes::LINT_LT_003, "operator_new_lines")
            .unwrap_or("after")
            .to_ascii_lowercase()
            .as_str()
        {
            "before" => Self::Trailing,
            _ => Self::Leading,
        }
    }
}

pub struct LayoutOperators {
    line_position: OperatorLinePosition,
}

impl LayoutOperators {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            line_position: OperatorLinePosition::from_config(config),
        }
    }
}

impl Default for LayoutOperators {
    fn default() -> Self {
        Self {
            line_position: OperatorLinePosition::Leading,
        }
    }
}

impl LintRule for LayoutOperators {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_003
    }

    fn name(&self) -> &'static str {
        "Layout operators"
    }

    fn description(&self) -> &'static str {
        "Operator line placement should be consistent."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if has_inconsistent_operator_layout(ctx.statement_sql(), self.line_position) {
            vec![Issue::info(
                issue_codes::LINT_LT_003,
                "Operator line placement appears inconsistent.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn has_inconsistent_operator_layout(sql: &str, line_position: OperatorLinePosition) -> bool {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return false;
    };

    for (index, token) in tokens.iter().enumerate() {
        if !is_layout_operator(&token.token) {
            continue;
        }

        let current_line = token.span.start.line;
        let prev_significant = tokens[..index]
            .iter()
            .rev()
            .find(|prev| !is_trivia_token(&prev.token));
        let next_significant = tokens
            .iter()
            .skip(index + 1)
            .find(|next| !is_trivia_token(&next.token));

        let (Some(prev_token), Some(next_token)) = (prev_significant, next_significant) else {
            continue;
        };

        let line_break_before = prev_token.span.end.line < current_line;
        let line_break_after = next_token.span.start.line > current_line;

        let has_violation = match line_position {
            OperatorLinePosition::Leading => line_break_after && !line_break_before,
            OperatorLinePosition::Trailing => line_break_before && !line_break_after,
        };
        if has_violation {
            return true;
        }
    }

    false
}

fn is_layout_operator(token: &Token) -> bool {
    matches!(
        token,
        Token::Plus
            | Token::Minus
            | Token::Mul
            | Token::Div
            | Token::Eq
            | Token::Neq
            | Token::Lt
            | Token::Gt
    )
}

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;

    fn run_with_rule(sql: &str, rule: &LayoutOperators) -> Vec<Issue> {
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
        run_with_rule(sql, &LayoutOperators::default())
    }

    #[test]
    fn flags_trailing_operator() {
        let issues = run("SELECT a +\n b FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_003);
    }

    #[test]
    fn does_not_flag_leading_operator() {
        assert!(run("SELECT a\n + b FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_operator_like_text_in_string() {
        assert!(run("SELECT 'a +\n b' AS txt").is_empty());
    }

    #[test]
    fn trailing_line_position_flags_leading_operator() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.operators".to_string(),
                serde_json::json!({"line_position": "trailing"}),
            )]),
        };
        let issues = run_with_rule(
            "SELECT a\n + b FROM t",
            &LayoutOperators::from_config(&config),
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_003);
    }

    #[test]
    fn legacy_operator_new_lines_before_maps_to_trailing_style() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_LT_003".to_string(),
                serde_json::json!({"operator_new_lines": "before"}),
            )]),
        };
        let issues = run_with_rule(
            "SELECT a +\n b FROM t",
            &LayoutOperators::from_config(&config),
        );
        assert!(issues.is_empty());
    }
}
