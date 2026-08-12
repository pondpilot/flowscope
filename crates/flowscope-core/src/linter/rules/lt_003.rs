//! LINT_LT_003: Layout operators.
//!
//! SQLFluff LT03 parity (current scope): flag trailing operators at end of line.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
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

impl BuiltinLintRule for LayoutOperators {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_003
    }

    fn name(&self) -> &'static str {
        "Layout operators"
    }

    fn description(&self) -> &'static str {
        "Operators should follow a standard for being before/after newlines."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let violations = operator_layout_violations(ctx, self.line_position);

        violations
            .into_iter()
            .map(|((start, end), edits)| {
                let mut issue = Issue::info(
                    issue_codes::LINT_LT_003,
                    "Operator line placement appears inconsistent.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end));
                if !edits.is_empty() {
                    let patch_edits = edits
                        .into_iter()
                        .map(|(edit_start, edit_end, replacement)| {
                            IssuePatchEdit::new(
                                ctx.span_from_statement_offset(edit_start, edit_end),
                                &replacement,
                            )
                        })
                        .collect();
                    issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, patch_edits);
                }
                issue
            })
            .collect()
    }
}

type Lt03Span = (usize, usize);
type Lt03AutofixEdit = (usize, usize, String);
type Lt03Violation = (Lt03Span, Vec<Lt03AutofixEdit>);

fn operator_layout_violations(
    ctx: &RuleContext,
    line_position: OperatorLinePosition,
) -> Vec<Lt03Violation> {
    let tokens =
        tokenized_for_context(ctx).or_else(|| tokenized(ctx.statement_sql(), ctx.dialect()));
    let Some(tokens) = tokens else {
        return operator_layout_violations_template_fallback(ctx.statement_sql(), line_position);
    };
    let sql = ctx.statement_sql();
    let token_offsets: Vec<Option<(usize, usize)>> = tokens
        .iter()
        .map(|token| token_with_span_offsets(sql, token))
        .collect();
    let non_trivia_neighbors = build_non_trivia_neighbors(&tokens);
    let mut violations = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        if !is_layout_operator(&token.token) {
            continue;
        }

        let current_line = token.span.start.line;
        let Some(prev_idx) = non_trivia_neighbors.prev[index] else {
            continue;
        };
        let Some(next_idx) = non_trivia_neighbors.next[index] else {
            continue;
        };
        let prev_token = &tokens[prev_idx];
        let next_token = &tokens[next_idx];

        let line_break_before = prev_token.span.end.line < current_line;
        let line_break_after = next_token.span.start.line > current_line;

        let has_violation = match line_position {
            OperatorLinePosition::Leading => line_break_after && !line_break_before,
            OperatorLinePosition::Trailing => line_break_before && !line_break_after,
        };
        if has_violation {
            let Some((start, end)) = token_offsets[index] else {
                continue;
            };
            let prefer_inline_join =
                matches!(token.token, Token::Mul) && is_interval_keyword_token(&next_token.token);
            let edits = safe_operator_autofix_edits(
                sql,
                index,
                line_position,
                line_break_before,
                line_break_after,
                prefer_inline_join,
                &token_offsets,
                prev_idx,
                next_idx,
            )
            .unwrap_or_default();
            violations.push(((start, end), edits));
        }
    }

    violations
}

fn operator_layout_violations_template_fallback(
    sql: &str,
    line_position: OperatorLinePosition,
) -> Vec<Lt03Violation> {
    if !contains_template_marker(sql) {
        return Vec::new();
    }

    if !matches!(line_position, OperatorLinePosition::Leading) {
        return Vec::new();
    }

    let mut violations = Vec::new();
    let line_ranges = line_ranges(sql);

    for (index, (line_start, line_end)) in line_ranges.iter().copied().enumerate() {
        let line = &sql[line_start..line_end];
        let trimmed = line.trim_end();
        let Some((op_start, op_end)) = trailing_operator_span_in_line(line, trimmed) else {
            continue;
        };

        let Some(next_non_empty) = line_ranges
            .iter()
            .copied()
            .skip(index + 1)
            .find(|(start, end)| !sql[*start..*end].trim().is_empty())
        else {
            continue;
        };
        let next_line = sql[next_non_empty.0..next_non_empty.1].trim_start();
        if !next_line.starts_with("{{")
            && !next_line.starts_with("{%")
            && !next_line.starts_with("{#")
        {
            continue;
        }

        violations.push(((line_start + op_start, line_start + op_end), Vec::new()));
    }

    violations
}

fn trailing_operator_span_in_line(line: &str, trimmed: &str) -> Option<(usize, usize)> {
    if trimmed.is_empty() {
        return None;
    }

    let candidate = [
        "AND", "OR", "||", ">=", "<=", "!=", "<>", "=", "+", "-", "*", "/", "<", ">",
    ];
    for op in candidate {
        if let Some(start) = trimmed.rfind(op) {
            let end = start + op.len();
            let suffix = &trimmed[end..];
            if !suffix.chars().all(char::is_whitespace) {
                continue;
            }
            if op.chars().all(|ch| ch.is_ascii_alphabetic()) {
                let left_ok = start == 0
                    || !trimmed[..start]
                        .chars()
                        .next_back()
                        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
                let right_ok = end >= trimmed.len()
                    || !trimmed[end..]
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
                if !left_ok || !right_ok {
                    continue;
                }
            }
            if line[start..].trim_end().len() == op.len() {
                return Some((start, end));
            }
        }
    }

    None
}

fn contains_template_marker(sql: &str) -> bool {
    sql.contains("{{") || sql.contains("{%") || sql.contains("{#")
}

fn is_interval_keyword_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Word(word)
            if word.keyword == Keyword::INTERVAL || word.value.eq_ignore_ascii_case("INTERVAL")
    )
}

fn line_ranges(sql: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0usize;
    for (idx, ch) in sql.char_indices() {
        if ch == '\n' {
            let mut end = idx;
            if end > start && sql[start..end].ends_with('\r') {
                end -= 1;
            }
            ranges.push((start, end));
            start = idx + 1;
        }
    }
    let mut end = sql.len();
    if end > start && sql[start..end].ends_with('\r') {
        end -= 1;
    }
    ranges.push((start, end));
    ranges
}

#[allow(clippy::too_many_arguments)]
fn safe_operator_autofix_edits(
    sql: &str,
    operator_idx: usize,
    line_position: OperatorLinePosition,
    line_break_before: bool,
    line_break_after: bool,
    prefer_inline_join: bool,
    token_offsets: &[Option<(usize, usize)>],
    prev_idx: usize,
    next_idx: usize,
) -> Option<Vec<Lt03AutofixEdit>> {
    match line_position {
        OperatorLinePosition::Leading if !line_break_before && line_break_after => {
            // Trailing operator → move to leading: "a +\n  b" → "a\n+ b"
            safe_operator_move_edits(
                sql,
                operator_idx,
                true,
                prefer_inline_join,
                token_offsets,
                prev_idx,
                next_idx,
            )
        }
        OperatorLinePosition::Trailing if line_break_before && !line_break_after => {
            // Leading operator → move to trailing: "a\n+ b" → "a +\n  b"
            safe_operator_move_edits(
                sql,
                operator_idx,
                false,
                false,
                token_offsets,
                prev_idx,
                next_idx,
            )
        }
        _ => None,
    }
}

/// Move an operator across a line break.
///
/// When `to_leading` is true, the operator currently trails on the previous line
/// and should be moved to lead on the next line.
/// When `to_leading` is false, the operator currently leads on the next line and
/// should be moved to trail on the previous line.
///
/// Edits are split to avoid spanning comment protected ranges.
fn safe_operator_move_edits(
    sql: &str,
    operator_idx: usize,
    to_leading: bool,
    prefer_inline_join: bool,
    token_offsets: &[Option<(usize, usize)>],
    prev_idx: usize,
    next_idx: usize,
) -> Option<Vec<Lt03AutofixEdit>> {
    let (_, prev_end) = token_offsets.get(prev_idx).copied().flatten()?;
    let (op_start, op_end) = token_offsets.get(operator_idx).copied().flatten()?;
    let (next_start, _) = token_offsets.get(next_idx).copied().flatten()?;

    if prev_end > op_start || op_end > next_start || next_start > sql.len() {
        return None;
    }

    let before_gap = &sql[prev_end..op_start];
    let after_gap = &sql[op_end..next_start];
    let has_comments = gap_has_comment(before_gap) || gap_has_comment(after_gap);
    let op_text = &sql[op_start..op_end];

    if !has_comments {
        // Simple case: no comments.
        if to_leading {
            if !before_gap.chars().all(char::is_whitespace)
                || before_gap.contains('\n')
                || before_gap.contains('\r')
            {
                return None;
            }
            if !after_gap.chars().all(char::is_whitespace)
                || (!after_gap.contains('\n') && !after_gap.contains('\r'))
            {
                return None;
            }
            if prefer_inline_join {
                // PostgreSQL interval arithmetic often appears as:
                // "... *\n    interval '1 day'". Moving `*` to leading style can
                // trigger LT02 indentation cascades; join this break inline.
                return Some(vec![(op_end, next_start, " ".to_string())]);
            }
            // Preserve existing indentation on the next line to avoid LT02
            // regressions when moving the operator.
            let delete_start = whitespace_before_on_same_line(sql, op_start, prev_end);
            return Some(vec![
                (delete_start, op_end, String::new()),
                (next_start, next_start, format!("{op_text} ")),
            ]);
        } else {
            if !before_gap.chars().all(char::is_whitespace)
                || (!before_gap.contains('\n') && !before_gap.contains('\r'))
            {
                return None;
            }
            if !after_gap.chars().all(char::is_whitespace)
                || after_gap.contains('\n')
                || after_gap.contains('\r')
            {
                return None;
            }
            // Preserve existing indentation on the following line.
            let delete_end = skip_inline_whitespace(sql, op_end);
            return Some(vec![
                (prev_end, prev_end, format!(" {op_text}")),
                (op_start, delete_end, String::new()),
            ]);
        }
    }

    // Comment-aware operator move: surgical edits that avoid comment spans.
    if to_leading {
        // Trailing → leading: "a +\n  b" → "a\n  + b"
        // Also handles: "a + -- foo\n  b" → "a -- foo\n  + b"
        // Also handles: "a AND\n  -- c1\n  -- c2\n  b" → "a\n  -- c1\n  -- c2\n  AND b"
        let mut edits = Vec::new();

        // 1) Delete operator and whitespace before it on the same line.
        let delete_start = whitespace_before_on_same_line(sql, op_start, prev_end);
        edits.push((delete_start, op_end, String::new()));

        // 2) Insert operator before the next significant token.
        //    Insert right at next_start to avoid spanning over any block comments
        //    on the same line.
        edits.push((next_start, next_start, format!("{op_text} ")));

        Some(edits)
    } else {
        // Leading → trailing: "a\n  + b" → "a +\n  b"
        // Also handles: "a -- foo\n  + b" → "a + -- foo\n  b"
        // Also handles: "a\n  -- c1\n  -- c2\n  + b" → "a +\n  -- c1\n  -- c2\n  b"
        let mut edits = Vec::new();

        // 1) Insert operator after prev token.
        //    Use the same trick as LT04: if a comment starts immediately at
        //    prev_end, extend one byte into the prev token to avoid touching
        //    the comment protected range.
        if prev_end > 0 && gap_has_comment(&sql[prev_end..op_start]) {
            let anchor = prev_end - 1;
            let ch = &sql[anchor..prev_end];
            edits.push((anchor, prev_end, format!("{ch} {op_text}")));
        } else {
            edits.push((prev_end, prev_end, format!(" {op_text}")));
        }

        // 2) Delete operator and trailing whitespace from its current position.
        let delete_end = skip_inline_whitespace(sql, op_end);
        edits.push((op_start, delete_end, String::new()));

        Some(edits)
    }
}

fn gap_has_comment(gap: &str) -> bool {
    gap.contains("--") || gap.contains("/*")
}

fn skip_inline_whitespace(sql: &str, offset: usize) -> usize {
    let mut pos = offset;
    let bytes = sql.as_bytes();
    while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
        pos += 1;
    }
    pos
}

fn whitespace_before_on_same_line(sql: &str, offset: usize, floor: usize) -> usize {
    let mut pos = offset;
    let bytes = sql.as_bytes();
    while pos > floor && (bytes[pos - 1] == b' ' || bytes[pos - 1] == b'\t') {
        pos -= 1;
    }
    pos
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
            | Token::Mod
            | Token::StringConcat
            | Token::Pipe
            | Token::Caret
            | Token::ShiftLeft
            | Token::ShiftRight
            | Token::Eq
            | Token::Neq
            | Token::Lt
            | Token::Gt
            | Token::LtEq
            | Token::GtEq
            | Token::Spaceship
            | Token::DoubleEq
            | Token::Arrow
            | Token::LongArrow
            | Token::HashArrow
            | Token::AtArrow
            | Token::ArrowAt
    ) || matches!(
        token,
        Token::Word(word) if matches!(word.keyword, Keyword::AND | Keyword::OR)
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

struct NonTriviaNeighbors {
    prev: Vec<Option<usize>>,
    next: Vec<Option<usize>>,
}

fn build_non_trivia_neighbors(tokens: &[TokenWithSpan]) -> NonTriviaNeighbors {
    let mut prev = vec![None; tokens.len()];
    let mut next = vec![None; tokens.len()];

    let mut prev_non_trivia = None;
    for (idx, token) in tokens.iter().enumerate() {
        prev[idx] = prev_non_trivia;
        if !is_trivia_token(&token.token) {
            prev_non_trivia = Some(idx);
        }
    }

    let mut next_non_trivia = None;
    for idx in (0..tokens.len()).rev() {
        next[idx] = next_non_trivia;
        if !is_trivia_token(&tokens[idx].token) {
            next_non_trivia = Some(idx);
        }
    }

    NonTriviaNeighbors { prev, next }
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
    use crate::types::IssueAutofixApplicability;

    fn run_with_rule(sql: &str, rule: &LayoutOperators) -> Vec<Issue> {
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
        run_with_rule(sql, &LayoutOperators::default())
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
    fn flags_trailing_operator() {
        let sql = "SELECT a +\n b FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_003);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT a\n + b FROM t");
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
        let sql = "SELECT a\n + b FROM t";
        let issues = run_with_rule(sql, &LayoutOperators::from_config(&config));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_003);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT a +\n b FROM t");
    }

    #[test]
    fn flags_trailing_and_operator() {
        let sql = "SELECT * FROM t WHERE a AND\nb";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_003);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT * FROM t WHERE a\nAND b");
    }

    #[test]
    fn flags_trailing_or_operator() {
        let sql = "SELECT * FROM t WHERE a OR\nb";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT * FROM t WHERE a\nOR b");
    }

    #[test]
    fn does_not_flag_leading_and_operator() {
        assert!(run("SELECT * FROM t WHERE a\nAND b").is_empty());
    }

    #[test]
    fn trailing_config_flags_leading_operator_with_comments() {
        // SQLFluff: fails_on_before_override_with_comment_order
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.operators".to_string(),
                serde_json::json!({"line_position": "trailing"}),
            )]),
        };
        let sql =
            "select\n    a -- comment1!\n    -- comment2!\n    -- comment3!\n    + b\nfrom foo";
        let issues = run_with_rule(sql, &LayoutOperators::from_config(&config));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_003);
    }

    #[test]
    fn trailing_config_allows_trailing_operator() {
        // SQLFluff: passes_on_after_override
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.operators".to_string(),
                serde_json::json!({"line_position": "trailing"}),
            )]),
        };
        let sql = "select\n    a +\n    b\nfrom foo";
        let issues = run_with_rule(sql, &LayoutOperators::from_config(&config));
        assert!(issues.is_empty());
    }

    #[test]
    fn trailing_config_flags_leading_operator() {
        // SQLFluff: fails_on_before_override
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.operators".to_string(),
                serde_json::json!({"line_position": "trailing"}),
            )]),
        };
        let sql = "select\n    a\n    + b\nfrom foo";
        let issues = run_with_rule(sql, &LayoutOperators::from_config(&config));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_003);
    }

    #[test]
    fn leading_mode_moves_trailing_and_with_comments() {
        // SQLFluff: fails_on_after_with_comment_order_preserved
        let sql = "select\n    a AND\n    -- comment1!\n    -- comment2!\n    b\nfrom foo";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "select\n    a\n    -- comment1!\n    -- comment2!\n    AND b\nfrom foo"
        );
    }

    #[test]
    fn trailing_mode_moves_leading_plus_with_comments() {
        // SQLFluff: fails_on_before_override_with_comment_order
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.operators".to_string(),
                serde_json::json!({"line_position": "trailing"}),
            )]),
        };
        let sql =
            "select\n    a -- comment1!\n    -- comment2!\n    -- comment3!\n    + b\nfrom foo";
        let issues = run_with_rule(sql, &LayoutOperators::from_config(&config));
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "select\n    a + -- comment1!\n    -- comment2!\n    -- comment3!\n    b\nfrom foo"
        );
    }

    #[test]
    fn leading_mode_moves_trailing_plus_with_inline_comment() {
        // SQLFluff: fails_on_after_override_with_comment_order
        let sql =
            "select\n    a + -- comment1!\n    -- comment2!\n    -- comment3!\n    b\nfrom foo";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "select\n    a -- comment1!\n    -- comment2!\n    -- comment3!\n    + b\nfrom foo"
        );
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

    #[test]
    fn statementless_template_line_break_after_operator_is_flagged() {
        let sql = "{% macro binary_literal(expression) %}\n  X'{{ expression }}'\n{% endmacro %}\n\nselect\n    *\nfrom my_table\nwhere\n    a =\n        {{ binary_literal(\"0000\") }}\n";
        let synthetic = parse_sql("SELECT 1").expect("parse");
        let rule = LayoutOperators::default();
        let issues =
            rule.check_with_context(&synthetic[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_003);
    }

    #[test]
    fn emits_one_issue_per_trailing_operator() {
        let sql = "SELECT a /\n b -\n c FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_003);
        assert_eq!(issues[1].code, issue_codes::LINT_LT_003);
    }

    #[test]
    fn flags_trailing_comparison_operator() {
        let sql = "SELECT * FROM t WHERE a >=\n b";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_003);
    }

    #[test]
    fn flags_trailing_json_operator() {
        let sql = "SELECT usage_metadata ->>\n 'endpoint_id' FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_003);
    }
}
