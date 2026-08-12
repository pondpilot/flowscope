//! LINT_LT_011: Layout set operators.
//!
//! SQLFluff LT11 parity (current scope): enforce own-line placement for set
//! operators in multiline statements.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{
    Location, Span as TokenSpan, Token, TokenWithSpan, Tokenizer, Whitespace,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SetOperatorLinePosition {
    AloneStrict,
    Leading,
    Trailing,
}

impl SetOperatorLinePosition {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_LT_011, "line_position")
            .unwrap_or("alone:strict")
            .to_ascii_lowercase()
            .as_str()
        {
            "leading" => Self::Leading,
            "trailing" => Self::Trailing,
            _ => Self::AloneStrict,
        }
    }
}

pub struct LayoutSetOperators {
    line_position: SetOperatorLinePosition,
}

impl LayoutSetOperators {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            line_position: SetOperatorLinePosition::from_config(config),
        }
    }
}

impl Default for LayoutSetOperators {
    fn default() -> Self {
        Self {
            line_position: SetOperatorLinePosition::AloneStrict,
        }
    }
}

impl BuiltinLintRule for LayoutSetOperators {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_011
    }

    fn name(&self) -> &'static str {
        "Layout set operators"
    }

    fn description(&self) -> &'static str {
        "Set operators should be surrounded by newlines."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let (has_violation, edit_spans) =
            set_operator_layout_violation_and_fixable_spans(ctx, self.line_position);
        if has_violation {
            let mut issue = Issue::info(
                issue_codes::LINT_LT_011,
                "Set operator line placement appears inconsistent.",
            )
            .with_statement(ctx.statement_index);

            if let Some((start, end)) = edit_spans.first().copied() {
                issue = issue.with_span(ctx.span_from_statement_offset(start, end));
                let edits = edit_spans
                    .into_iter()
                    .map(|(edit_start, edit_end)| {
                        IssuePatchEdit::new(
                            ctx.span_from_statement_offset(edit_start, edit_end),
                            "\n",
                        )
                    })
                    .collect();
                issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, edits);
            }

            vec![issue]
        } else {
            Vec::new()
        }
    }
}

fn set_operator_layout_violation_and_fixable_spans(
    ctx: &RuleContext,
    line_position: SetOperatorLinePosition,
) -> (bool, Vec<(usize, usize)>) {
    let tokens =
        tokenized_for_context(ctx).or_else(|| tokenized(ctx.statement_sql(), ctx.dialect()));
    let Some(tokens) = tokens else {
        return (false, Vec::new());
    };
    let sql = ctx.statement_sql();

    let significant_tokens: Vec<(usize, &TokenWithSpan)> = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| !is_trivia_token(&token.token))
        .collect();

    let has_set_operator = significant_tokens
        .iter()
        .any(|(_, token)| set_operator_keyword(&token.token).is_some());
    if !has_set_operator {
        return (false, Vec::new());
    }
    let mut has_violation = false;
    let mut edit_spans = Vec::new();

    for (position, (_, token)) in significant_tokens.iter().enumerate() {
        let Some(keyword) = set_operator_keyword(&token.token) else {
            continue;
        };

        let operator_end = if keyword == Keyword::UNION
            && matches!(
                significant_tokens.get(position + 1).map(|(_, t)| &t.token),
                Some(Token::Word(word)) if word.keyword == Keyword::ALL
            ) {
            position + 1
        } else {
            position
        };

        let Some((_, prev_token)) = position
            .checked_sub(1)
            .and_then(|idx| significant_tokens.get(idx))
        else {
            continue;
        };
        let Some((_, next_token)) = significant_tokens.get(operator_end + 1) else {
            continue;
        };

        let operator_line = token.span.start.line;
        let line_break_before = prev_token.span.start.line < operator_line;
        let line_break_after = next_token.span.start.line > operator_line;

        let placement_violation = match line_position {
            SetOperatorLinePosition::AloneStrict => !line_break_before || !line_break_after,
            SetOperatorLinePosition::Leading => !line_break_before || line_break_after,
            SetOperatorLinePosition::Trailing => line_break_before || !line_break_after,
        };

        if placement_violation {
            has_violation = true;
        } else {
            continue;
        }

        if !matches!(line_position, SetOperatorLinePosition::AloneStrict) {
            continue;
        }

        if !line_break_before {
            let Some(gap_start) = token_end_offset(sql, prev_token) else {
                continue;
            };
            let Some(gap_end) = token_start_offset(sql, token) else {
                continue;
            };
            if gap_start <= gap_end && gap_is_whitespace_only(sql, gap_start, gap_end) {
                edit_spans.push((gap_start, gap_end));
            }
        }

        if !line_break_after {
            let Some((_, operator_end_token)) = significant_tokens.get(operator_end) else {
                continue;
            };
            let Some(gap_start) = token_end_offset(sql, operator_end_token) else {
                continue;
            };
            let Some(gap_end) = token_start_offset(sql, next_token) else {
                continue;
            };
            if gap_start <= gap_end && gap_is_whitespace_only(sql, gap_start, gap_end) {
                edit_spans.push((gap_start, gap_end));
            }
        }
    }

    edit_spans.sort_unstable();
    edit_spans.dedup();
    (has_violation, edit_spans)
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn tokenized_for_context(ctx: &RuleContext) -> Option<Vec<TokenWithSpan>> {
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
                TokenSpan::new(start_loc, end_loc),
            ));
        }

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    })
}

fn set_operator_keyword(token: &Token) -> Option<Keyword> {
    let Token::Word(word) = token else {
        return None;
    };

    match word.keyword {
        Keyword::UNION | Keyword::INTERSECT | Keyword::EXCEPT => Some(word.keyword),
        _ => None,
    }
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

fn token_start_offset(sql: &str, token: &TokenWithSpan) -> Option<usize> {
    line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )
}

fn token_end_offset(sql: &str, token: &TokenWithSpan) -> Option<usize> {
    line_col_to_offset(
        sql,
        token.span.end.line as usize,
        token.span.end.column as usize,
    )
}

fn gap_is_whitespace_only(sql: &str, start: usize, end: usize) -> bool {
    if start > end || end > sql.len() {
        return false;
    }

    sql[start..end].chars().all(char::is_whitespace)
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
    use crate::types::IssueAutofixApplicability;

    fn run_with_rule(sql: &str, rule: &LayoutSetOperators) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn run(sql: &str) -> Vec<Issue> {
        run_with_rule(sql, &LayoutSetOperators::default())
    }

    fn apply_issue_autofix(sql: &str, issue: &Issue) -> Option<String> {
        let autofix = issue.autofix.as_ref()?;
        let mut out = sql.to_string();
        let mut edits = autofix.edits.clone();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        for edit in edits.into_iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    #[test]
    fn flags_inline_set_operator_in_multiline_statement() {
        let sql = "SELECT 1 UNION SELECT 2\nUNION SELECT 3";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_011);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT 1\nUNION\nSELECT 2\nUNION\nSELECT 3");
    }

    #[test]
    fn flags_inline_set_operator_in_single_line_statement() {
        let issues = run("SELECT 1 UNION SELECT 2");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_011);
    }

    #[test]
    fn does_not_flag_own_line_set_operators() {
        let issues = run("SELECT 1\nUNION\nSELECT 2\nUNION\nSELECT 3");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_own_line_union_all() {
        let issues = run("SELECT 1\nUNION ALL\nSELECT 2");
        assert!(issues.is_empty());
    }

    #[test]
    fn leading_line_position_accepts_leading_operators() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.set_operators".to_string(),
                serde_json::json!({"line_position": "leading"}),
            )]),
        };
        let issues = run_with_rule(
            "SELECT 1\nUNION SELECT 2\nUNION SELECT 3",
            &LayoutSetOperators::from_config(&config),
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn trailing_line_position_flags_leading_operators() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_LT_011".to_string(),
                serde_json::json!({"line_position": "trailing"}),
            )]),
        };
        let issues = run_with_rule(
            "SELECT 1\nUNION SELECT 2",
            &LayoutSetOperators::from_config(&config),
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_011);
    }

    #[test]
    fn inline_set_operator_with_comment_gap_preserves_comment() {
        let sql = "SELECT 1 /* keep */ UNION SELECT 2";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_011);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert!(
            fixed.contains("/* keep */"),
            "LT011 autofix should preserve comment trivia: {fixed}"
        );
    }
}
