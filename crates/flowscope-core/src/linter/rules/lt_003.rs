//! LINT_LT_003: Layout operators.
//!
//! SQLFluff LT03 parity (current scope): flag trailing operators at end of line.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Location, Span, Token, TokenWithSpan, Tokenizer, Whitespace};

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
        if has_inconsistent_operator_layout(ctx, self.line_position) {
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

fn has_inconsistent_operator_layout(
    ctx: &LintContext,
    line_position: OperatorLinePosition,
) -> bool {
    let tokens =
        tokenized_for_context(ctx).or_else(|| tokenized(ctx.statement_sql(), ctx.dialect()));
    let Some(tokens) = tokens else {
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

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn tokenized_for_context(ctx: &LintContext) -> Option<Vec<TokenWithSpan>> {
    let (statement_start_line, statement_start_column) =
        offset_to_line_col(ctx.sql, ctx.statement_range.start)?;

    ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut out = Vec::new();
        for token in tokens {
            let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                continue;
            };
            if start < ctx.statement_range.start || end > ctx.statement_range.end {
                continue;
            }

            let Some(start_loc) = relative_location(
                token.span.start,
                statement_start_line,
                statement_start_column,
            ) else {
                continue;
            };
            let Some(end_loc) =
                relative_location(token.span.end, statement_start_line, statement_start_column)
            else {
                continue;
            };

            out.push(TokenWithSpan::new(
                token.token.clone(),
                Span::new(start_loc, end_loc),
            ));
        }

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    })
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

fn offset_to_line_col(sql: &str, offset: usize) -> Option<(usize, usize)> {
    if offset > sql.len() {
        return None;
    }
    if offset == sql.len() {
        let mut line = 1usize;
        let mut column = 1usize;
        for ch in sql.chars() {
            if ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        return Some((line, column));
    }

    let mut line = 1usize;
    let mut column = 1usize;
    for (index, ch) in sql.char_indices() {
        if index == offset {
            return Some((line, column));
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    None
}

fn relative_location(
    location: Location,
    statement_start_line: usize,
    statement_start_column: usize,
) -> Option<Location> {
    let line = location.line as usize;
    let column = location.column as usize;
    if line < statement_start_line {
        return None;
    }

    if line == statement_start_line {
        if column < statement_start_column {
            return None;
        }
        return Some(Location::new(
            1,
            (column - statement_start_column + 1) as u64,
        ));
    }

    Some(Location::new(
        (line - statement_start_line + 1) as u64,
        column as u64,
    ))
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
