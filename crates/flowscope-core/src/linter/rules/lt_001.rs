//! LINT_LT_001: Layout spacing.
//!
//! SQLFluff LT01 parity: comprehensive spacing checks covering operators,
//! commas, brackets, keywords, literals, trailing whitespace, excessive
//! whitespace, and cast operators.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Location, Span, Token, TokenWithSpan, Tokenizer, Whitespace};
use std::collections::HashSet;

pub struct LayoutSpacing {
    ignore_templated_areas: bool,
    align_alias_expression: bool,
    align_data_type: bool,
    align_column_constraint: bool,
    align_with_tabs: bool,
    tab_space_size: usize,
}

impl LayoutSpacing {
    pub fn from_config(config: &LintConfig) -> Self {
        let spacing_before_align = |type_name: &str| {
            config
                .config_section_object("layout.keyword_newline")
                .and_then(|layout| layout.get(type_name))
                .and_then(serde_json::Value::as_object)
                .and_then(|entry| entry.get("spacing_before"))
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value.to_ascii_lowercase().starts_with("align"))
        };

        Self {
            ignore_templated_areas: config
                .core_option_bool("ignore_templated_areas")
                .unwrap_or(true),
            align_alias_expression: spacing_before_align("alias_expression"),
            align_data_type: spacing_before_align("data_type"),
            align_column_constraint: spacing_before_align("column_constraint_segment"),
            align_with_tabs: config
                .section_option_str("indentation", "indent_unit")
                .or_else(|| config.section_option_str("rules", "indent_unit"))
                .is_some_and(|value| value.eq_ignore_ascii_case("tab")),
            tab_space_size: config
                .section_option_usize("indentation", "tab_space_size")
                .or_else(|| config.section_option_usize("rules", "tab_space_size"))
                .unwrap_or(4)
                .max(1),
        }
    }

    fn alignment_options(&self) -> Lt01AlignmentOptions {
        Lt01AlignmentOptions {
            alias_expression: self.align_alias_expression,
            data_type: self.align_data_type,
            column_constraint: self.align_column_constraint,
            align_with_tabs: self.align_with_tabs,
            tab_space_size: self.tab_space_size,
        }
    }
}

impl Default for LayoutSpacing {
    fn default() -> Self {
        Self {
            ignore_templated_areas: true,
            align_alias_expression: false,
            align_data_type: false,
            align_column_constraint: false,
            align_with_tabs: false,
            tab_space_size: 4,
        }
    }
}

impl BuiltinLintRule for LayoutSpacing {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_001
    }

    fn name(&self) -> &'static str {
        "Layout spacing"
    }

    fn description(&self) -> &'static str {
        "Inappropriate Spacing."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut violations =
            spacing_violations(ctx, self.ignore_templated_areas, self.alignment_options());
        let has_remaining_non_whitespace = ctx.sql[ctx.statement_range.end..]
            .chars()
            .any(|ch| !ch.is_whitespace());
        let parser_fragment_fallback = ctx.statement_index == 0
            && ctx.statement_range.start == 0
            && ctx.statement_range.end < ctx.sql.len()
            && has_remaining_non_whitespace
            && !ctx.statement_sql().trim_end().ends_with(';');
        let template_fragment_fallback = ctx.statement_index == 0
            && contains_template_marker(ctx.sql)
            && (ctx.statement_range.start > 0 || ctx.statement_range.end < ctx.sql.len());
        if parser_fragment_fallback || template_fragment_fallback {
            let full_ctx = ctx.statement_view(ctx.sql, 0..ctx.sql.len(), 0);
            violations.extend(spacing_violations(
                &full_ctx,
                self.ignore_templated_areas,
                self.alignment_options(),
            ));
            merge_violations_by_span(&mut violations);
        }

        violations
            .into_iter()
            .map(|((start, end), edits)| {
                let mut issue =
                    Issue::info(issue_codes::LINT_LT_001, "Inappropriate spacing found.")
                        .with_statement(ctx.statement_index)
                        .with_span(ctx.span_from_statement_offset(start, end));
                if !edits.is_empty() {
                    let edits = edits
                        .into_iter()
                        .map(|(edit_start, edit_end, replacement)| {
                            IssuePatchEdit::new(
                                ctx.span_from_statement_offset(edit_start, edit_end),
                                replacement.to_string(),
                            )
                        })
                        .collect();
                    issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, edits);
                }
                issue
            })
            .collect()
    }
}

type Lt01Span = (usize, usize);
type Lt01AutofixEdit = (usize, usize, String);
type Lt01Violation = (Lt01Span, Vec<Lt01AutofixEdit>);
type Lt01TemplateSpan = (usize, usize);

fn merge_violations_by_span(violations: &mut Vec<Lt01Violation>) {
    violations.sort_unstable_by_key(|(span, _)| *span);
    let mut merged: Vec<Lt01Violation> = Vec::with_capacity(violations.len());

    for (span, edits) in violations.drain(..) {
        if let Some((last_span, last_edits)) = merged.last_mut() {
            if *last_span == span {
                if last_edits.is_empty() && !edits.is_empty() {
                    *last_edits = edits;
                } else if !last_edits.is_empty() && !edits.is_empty() {
                    for edit in edits {
                        if !last_edits.contains(&edit) {
                            last_edits.push(edit);
                        }
                    }
                }
                continue;
            }
        }

        merged.push((span, edits));
    }

    *violations = merged;
}

#[derive(Clone, Copy)]
struct Lt01AlignmentOptions {
    alias_expression: bool,
    data_type: bool,
    column_constraint: bool,
    align_with_tabs: bool,
    tab_space_size: usize,
}

fn spacing_violations(
    ctx: &RuleContext,
    ignore_templated_areas: bool,
    alignment: Lt01AlignmentOptions,
) -> Vec<Lt01Violation> {
    let sql = ctx.statement_sql();
    let mut violations = Vec::new();
    let templated_spans = template_spans(sql);
    let prefer_raw_template_tokens = ctx.is_templated() && contains_template_marker(sql);
    let tokens = if prefer_raw_template_tokens {
        tokenized(sql, ctx.dialect()).or_else(|| tokenized_for_context(ctx))
    } else {
        tokenized_for_context(ctx).or_else(|| tokenized(sql, ctx.dialect()))
    };
    let Some(tokens) = tokens else {
        return violations;
    };

    let dialect = ctx.dialect();

    collect_trailing_whitespace_violations(sql, &mut violations);
    collect_pair_spacing_violations(sql, &tokens, dialect, &templated_spans, &mut violations);
    collect_ansi_national_string_literal_violations(
        sql,
        &tokens,
        dialect,
        &templated_spans,
        &mut violations,
    );
    if !ignore_templated_areas {
        collect_template_string_spacing_violations(sql, dialect, &templated_spans, &mut violations);
    }
    collect_alignment_detection_violations(sql, alignment, &mut violations);

    violations.sort_unstable_by_key(|(span, _)| *span);
    violations.dedup_by_key(|(span, _)| *span);

    violations
}

// ---------------------------------------------------------------------------
// Trailing whitespace
// ---------------------------------------------------------------------------

fn collect_trailing_whitespace_violations(sql: &str, violations: &mut Vec<Lt01Violation>) {
    let mut offset = 0;
    for line in sql.split('\n') {
        let trimmed = line.trim_end_matches([' ', '\t']);
        let trailing_start = offset + trimmed.len();
        let trailing_end = offset + line.len();
        if trailing_end > trailing_start {
            let span = (trailing_start, trailing_end);
            let edit = (trailing_start, trailing_end, String::new());
            violations.push((span, vec![edit]));
        }
        offset += line.len() + 1; // +1 for the \n
    }
}

fn collect_alignment_detection_violations(
    sql: &str,
    alignment: Lt01AlignmentOptions,
    violations: &mut Vec<Lt01Violation>,
) {
    if alignment.alias_expression {
        collect_alias_alignment_detection(
            sql,
            alignment.tab_space_size,
            alignment.align_with_tabs,
            violations,
        );
    }
    if alignment.data_type || alignment.column_constraint {
        collect_create_table_alignment_detection(sql, alignment.tab_space_size, violations);
    }
}

#[derive(Clone, Copy)]
struct AliasAlignmentEntry {
    as_start: usize,
    visual_col: usize,
    separator_uses_tabs: bool,
}

fn collect_alias_alignment_detection(
    sql: &str,
    tab_space_size: usize,
    align_with_tabs: bool,
    violations: &mut Vec<Lt01Violation>,
) {
    let lines: Vec<&str> = sql.split('\n').collect();
    if lines.len() < 2 {
        return;
    }

    let mut offset = 0usize;
    let mut current_group: Vec<AliasAlignmentEntry> = Vec::new();

    for line in &lines {
        let lower = line.to_ascii_lowercase();
        let alias_pos = lower.find(" as ");
        let is_alias_line = alias_pos.is_some() && !lower.trim_start().starts_with("from ");

        if is_alias_line {
            let as_index = alias_pos.unwrap_or_default() + 1;
            current_group.push(AliasAlignmentEntry {
                as_start: offset + as_index,
                visual_col: visual_width(&line[..as_index], tab_space_size),
                separator_uses_tabs: alias_separator_uses_tabs(line, as_index),
            });
        } else if !current_group.is_empty() {
            emit_alias_alignment_group(&current_group, align_with_tabs, violations);
            current_group.clear();
        }

        offset += line.len() + 1;
    }

    if !current_group.is_empty() {
        emit_alias_alignment_group(&current_group, align_with_tabs, violations);
    }
}

fn alias_separator_uses_tabs(line: &str, as_index: usize) -> bool {
    let prefix = &line[..as_index];
    let separator_start = prefix
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);
    let separator = &prefix[separator_start..];
    !separator.is_empty() && separator.chars().all(|ch| ch == '\t')
}

fn emit_alias_alignment_group(
    group: &[AliasAlignmentEntry],
    align_with_tabs: bool,
    violations: &mut Vec<Lt01Violation>,
) {
    if group.len() < 2 {
        return;
    }
    let target_col = group
        .iter()
        .map(|entry| entry.visual_col)
        .max()
        .unwrap_or(0);
    for entry in group {
        if entry.visual_col != target_col || (align_with_tabs && !entry.separator_uses_tabs) {
            let end = entry.as_start + 2;
            violations.push(((entry.as_start, end), Vec::new()));
        }
    }
}

fn collect_create_table_alignment_detection(
    sql: &str,
    tab_space_size: usize,
    violations: &mut Vec<Lt01Violation>,
) {
    let lines: Vec<&str> = sql.split('\n').collect();
    let mut offset = 0usize;
    let mut in_create_table = false;
    let mut entries: Vec<(usize, usize)> = Vec::new();

    for line in &lines {
        let trimmed = line.trim_start();
        let upper = trimmed.to_ascii_uppercase();
        if !in_create_table && upper.starts_with("CREATE TABLE") {
            in_create_table = true;
        } else if in_create_table && (trimmed.starts_with(')') || trimmed.starts_with(';')) {
            emit_create_table_alignment_group(&entries, violations);
            entries.clear();
            in_create_table = false;
        }

        if in_create_table
            && !trimmed.is_empty()
            && !trimmed.starts_with('(')
            && !trimmed.starts_with(')')
            && !trimmed.starts_with("--")
            && !upper.starts_with("CREATE TABLE")
        {
            if let Some(data_type_start) = second_token_start(trimmed) {
                let prefix_len = line.len() - trimmed.len();
                let absolute = offset + prefix_len + data_type_start;
                let visual = visual_width(&trimmed[..data_type_start], tab_space_size);
                entries.push((absolute, visual));
            }
        }

        offset += line.len() + 1;
    }

    if in_create_table && !entries.is_empty() {
        emit_create_table_alignment_group(&entries, violations);
    }
}

fn emit_create_table_alignment_group(
    group: &[(usize, usize)],
    violations: &mut Vec<Lt01Violation>,
) {
    if group.len() < 2 {
        return;
    }
    let target_col = group.iter().map(|(_, col)| *col).max().unwrap_or(0);
    for (start, col) in group {
        if *col != target_col {
            let end = *start + 1;
            violations.push(((*start, end), Vec::new()));
        }
    }
}

fn second_token_start(line: &str) -> Option<usize> {
    let mut seen_first = false;
    let mut in_token = false;

    for (index, ch) in line.char_indices() {
        if ch.is_whitespace() {
            if in_token {
                in_token = false;
                seen_first = true;
            }
            continue;
        }

        if seen_first && !in_token {
            return Some(index);
        }
        in_token = true;
    }
    None
}

fn visual_width(text: &str, tab_space_size: usize) -> usize {
    let mut width = 0usize;
    for ch in text.chars() {
        if ch == '\t' {
            let next_tab = ((width / tab_space_size) + 1) * tab_space_size;
            width = next_tab;
        } else {
            width += 1;
        }
    }
    width
}

// ---------------------------------------------------------------------------
// Pair-based spacing: walk consecutive non-trivia token pairs
// ---------------------------------------------------------------------------

/// Expected spacing between two adjacent non-trivia tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ExpectedSpacing {
    /// Exactly one space required (or newline acceptable).
    Single,
    /// No space allowed (tokens must be adjacent).
    None,
    /// No space allowed, including across newlines.
    NoneInline,
    /// Do not check this pair (e.g. start/end of statement).
    Skip,
    /// Single space required, and if there's a newline between, replace with single space.
    SingleInline,
}

fn collect_pair_spacing_violations(
    sql: &str,
    tokens: &[TokenWithSpan],
    dialect: Dialect,
    templated_spans: &[Lt01TemplateSpan],
    violations: &mut Vec<Lt01Violation>,
) {
    let non_trivia: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| !is_trivia_token(&t.token) && !matches!(t.token, Token::EOF))
        .map(|(i, _)| i)
        .collect();
    let type_angle_tokens = if supports_type_angle_spacing(dialect) {
        type_angle_token_indices(tokens, &non_trivia)
    } else {
        HashSet::new()
    };
    let snowflake_pattern_tokens = if dialect == Dialect::Snowflake {
        snowflake_pattern_token_indices(tokens, &non_trivia)
    } else {
        HashSet::new()
    };

    for window in non_trivia.windows(2) {
        let left_idx = window[0];
        let right_idx = window[1];
        if dialect == Dialect::Snowflake
            && (snowflake_pattern_tokens.contains(&left_idx)
                || snowflake_pattern_tokens.contains(&right_idx))
        {
            continue;
        }
        let left = &tokens[left_idx];
        let right = &tokens[right_idx];

        let Some((left_start, left_end)) = token_offsets(sql, left) else {
            continue;
        };
        let Some((right_start, _)) = token_offsets(sql, right) else {
            continue;
        };

        if left_end > right_start || right_start > sql.len() || left_end > sql.len() {
            continue;
        }
        if overlaps_template_span(templated_spans, left_start, right_start) {
            continue;
        }

        let gap = &sql[left_end..right_start];
        let has_newline = gap.contains('\n') || gap.contains('\r');
        let has_comment = has_comment_between(tokens, left_idx, right_idx);

        let expected = if supports_type_angle_spacing(dialect)
            && is_type_angle_spacing_pair(left, right, left_idx, right_idx, &type_angle_tokens)
        {
            ExpectedSpacing::None
        } else {
            expected_spacing(left, right, tokens, left_idx, right_idx, dialect)
        };

        match expected {
            ExpectedSpacing::Skip => continue,
            ExpectedSpacing::None => {
                // Tokens should be adjacent, no whitespace allowed.
                if !gap.is_empty() && !has_newline && !has_comment {
                    let span = (left_end, right_start);
                    let edit = (left_end, right_start, String::new());
                    violations.push((span, vec![edit]));
                }
            }
            ExpectedSpacing::NoneInline => {
                if !gap.is_empty() && !has_comment {
                    let span = (left_end, right_start);
                    let edit = (left_end, right_start, String::new());
                    violations.push((span, vec![edit]));
                }
            }
            ExpectedSpacing::Single => {
                if has_comment {
                    continue;
                }
                if has_newline {
                    // Newline is acceptable as a separator for single-space contexts.
                    // But check if there's excessive inline space on the same line
                    // before or after the newline.
                    continue;
                }
                if gap == " " {
                    // Correct single space.
                    continue;
                }
                if gap.is_empty() && matches!(left.token, Token::Comma) {
                    // Avoid zero-width insert edits touching the next token.
                    // Replacing the comma token itself allows CP02/LT01 fixes
                    // to coexist in the same pass.
                    let replacement = format!("{} ", &sql[left_start..left_end]);
                    let span = (left_start, left_end);
                    let edit = (left_start, left_end, replacement);
                    violations.push((span, vec![edit]));
                    continue;
                }
                if gap.is_empty() && is_exists_keyword_token(&left.token) {
                    // Zero-width inserts are filtered by the fix planner.
                    // Replace the EXISTS token itself to preserve fixability.
                    let replacement = format!("{} ", &sql[left_start..left_end]);
                    let span = (left_start, left_end);
                    let edit = (left_start, left_end, replacement);
                    violations.push((span, vec![edit]));
                    continue;
                }
                // Either missing space (gap is empty) or excessive space (multiple spaces).
                let span = (left_end, right_start);
                let edit = (left_end, right_start, " ".to_string());
                violations.push((span, vec![edit]));
            }
            ExpectedSpacing::SingleInline => {
                if has_comment {
                    continue;
                }
                if gap == " " {
                    continue;
                }
                // Replace whatever gap (including newlines) with single space.
                let span = (left_end, right_start);
                let edit = (left_end, right_start, " ".to_string());
                violations.push((span, vec![edit]));
            }
        }
    }
}

/// Determine expected spacing between two adjacent non-trivia tokens.
fn expected_spacing(
    left: &TokenWithSpan,
    right: &TokenWithSpan,
    tokens: &[TokenWithSpan],
    left_idx: usize,
    right_idx: usize,
    dialect: Dialect,
) -> ExpectedSpacing {
    // --- Period (dot) for qualified identifiers: no space around ---
    if matches!(left.token, Token::Period) || matches!(right.token, Token::Period) {
        return ExpectedSpacing::NoneInline;
    }

    // --- Cast operator (::) ---
    if matches!(left.token, Token::DoubleColon) || matches!(right.token, Token::DoubleColon) {
        return ExpectedSpacing::NoneInline;
    }

    // --- Snowflake colon (semi-structured access): no space around ---
    if dialect == Dialect::Snowflake
        && (matches!(left.token, Token::Colon) || matches!(right.token, Token::Colon))
    {
        // Snowflake a:b:c syntax — no spaces around colon
        return ExpectedSpacing::NoneInline;
    }

    // --- Split compound comparison operators (>,<,!) + = ---
    if is_split_compound_comparison_pair(left, right) {
        return ExpectedSpacing::NoneInline;
    }

    // --- TSQL compound assignment operators (+=, -=, etc.) ---
    if dialect == Dialect::Mssql && is_tsql_compound_assignment_pair(left, right) {
        return ExpectedSpacing::NoneInline;
    }

    // --- Left paren: usually no space before (function calls) ---
    if matches!(right.token, Token::LParen) {
        return expected_spacing_before_lparen(left, tokens, left_idx, dialect);
    }

    // --- Right paren followed by something ---
    if matches!(left.token, Token::RParen) {
        return expected_spacing_after_rparen(right, tokens, right_idx);
    }

    // --- Left bracket: no space before in most contexts ---
    if matches!(right.token, Token::LBracket) {
        // text[] type syntax needs a space, but array access doesn't.
        if is_type_keyword_for_bracket(&left.token) {
            return ExpectedSpacing::Single;
        }
        return ExpectedSpacing::None;
    }

    // --- Right bracket ---
    if matches!(left.token, Token::RBracket) {
        // After ] usually no space before :: or . or [ or )
        if matches!(
            right.token,
            Token::DoubleColon | Token::Period | Token::LBracket | Token::RParen
        ) {
            return ExpectedSpacing::None;
        }
        return ExpectedSpacing::Single;
    }

    // --- Comma: no space before, single space after ---
    if matches!(right.token, Token::Comma) {
        return ExpectedSpacing::None;
    }
    if matches!(left.token, Token::Comma) {
        return ExpectedSpacing::Single;
    }

    // --- Semicolon: no space before ---
    if matches!(right.token, Token::SemiColon) {
        return ExpectedSpacing::Skip;
    }
    if matches!(left.token, Token::SemiColon) {
        return ExpectedSpacing::Skip;
    }

    // --- Inside parens: no space after ( or before ) ---
    if matches!(left.token, Token::LParen) {
        return ExpectedSpacing::None;
    }
    if matches!(right.token, Token::RParen) {
        return ExpectedSpacing::None;
    }

    // --- BigQuery project identifiers can include hyphens before dataset/table ---
    if dialect == Dialect::Bigquery
        && is_bigquery_hyphenated_identifier_pair(left, right, tokens, left_idx, right_idx)
    {
        return ExpectedSpacing::None;
    }

    if is_filesystem_path_pair(left, right, tokens, left_idx, right_idx, dialect) {
        return ExpectedSpacing::NoneInline;
    }

    // --- Binary operators: single space on each side ---
    if is_binary_operator(&left.token) || is_binary_operator(&right.token) {
        // Special: unary minus/plus (sign indicators) — skip
        if is_unary_operator_pair(left, right, tokens, left_idx) {
            return ExpectedSpacing::Skip;
        }
        return ExpectedSpacing::Single;
    }

    // --- Comparison operators: single space around ---
    if is_comparison_operator(&left.token) || is_comparison_operator(&right.token) {
        if dialect == Dialect::Mssql
            && is_tsql_assignment_rhs_pair(left, right, tokens, left_idx, right_idx)
        {
            return ExpectedSpacing::Single;
        }
        return ExpectedSpacing::Single;
    }

    // --- JSON operators (arrow, long arrow, etc.) ---
    if is_json_operator(&left.token) || is_json_operator(&right.token) {
        return ExpectedSpacing::Single;
    }

    // --- Star/Mul as wildcard inside COUNT(*) etc. ---
    if matches!(left.token, Token::Mul) || matches!(right.token, Token::Mul) {
        // If inside parens: skip (could be wildcard)
        return ExpectedSpacing::Skip;
    }

    // --- Keywords and identifiers: single space between ---
    if is_word_like(&left.token) && is_word_like(&right.token) {
        return ExpectedSpacing::Single;
    }

    // --- Word followed by literal or vice versa ---
    if (is_word_like(&left.token) && is_literal(&right.token))
        || (is_literal(&left.token) && is_word_like(&right.token))
    {
        return ExpectedSpacing::Single;
    }

    // --- Literal followed by literal ---
    if is_literal(&left.token) && is_literal(&right.token) {
        return ExpectedSpacing::Single;
    }

    // --- Number followed by word or vice versa ---
    if (matches!(left.token, Token::Number(_, _)) && is_word_like(&right.token))
        || (is_word_like(&left.token) && matches!(right.token, Token::Number(_, _)))
    {
        return ExpectedSpacing::Single;
    }

    ExpectedSpacing::Skip
}

// ---------------------------------------------------------------------------
// Token classification helpers
// ---------------------------------------------------------------------------

fn is_binary_operator(token: &Token) -> bool {
    matches!(
        token,
        Token::Plus
            | Token::Minus
            | Token::Div
            | Token::Mod
            | Token::StringConcat
            | Token::Ampersand
            | Token::Pipe
            | Token::Caret
            | Token::ShiftLeft
            | Token::ShiftRight
            | Token::Assignment
    )
}

fn is_comparison_operator(token: &Token) -> bool {
    matches!(
        token,
        Token::Eq
            | Token::Neq
            | Token::Lt
            | Token::Gt
            | Token::LtEq
            | Token::GtEq
            | Token::Spaceship
            | Token::DoubleEq
            | Token::TildeEqual
    )
}

fn is_split_compound_comparison_pair(left: &TokenWithSpan, right: &TokenWithSpan) -> bool {
    matches!(
        (&left.token, &right.token),
        (Token::Gt, Token::Eq)
            | (Token::Lt, Token::Eq)
            | (Token::Lt, Token::Gt)
            | (Token::Neq, Token::Eq)
    )
}

fn is_assignment_operator_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Plus
            | Token::Minus
            | Token::Mul
            | Token::Div
            | Token::Mod
            | Token::Ampersand
            | Token::Pipe
            | Token::Caret
    )
}

fn is_tsql_compound_assignment_pair(left: &TokenWithSpan, right: &TokenWithSpan) -> bool {
    matches!(right.token, Token::Eq) && is_assignment_operator_token(&left.token)
}

fn is_tsql_assignment_rhs_pair(
    left: &TokenWithSpan,
    _right: &TokenWithSpan,
    tokens: &[TokenWithSpan],
    left_idx: usize,
    _right_idx: usize,
) -> bool {
    if !matches!(left.token, Token::Eq) {
        return false;
    }
    prev_non_trivia_index(tokens, left_idx)
        .map(|index| is_assignment_operator_token(&tokens[index].token))
        .unwrap_or(false)
}

fn is_json_operator(token: &Token) -> bool {
    matches!(
        token,
        Token::Arrow
            | Token::LongArrow
            | Token::HashArrow
            | Token::HashLongArrow
            | Token::AtArrow
            | Token::ArrowAt
    )
}

fn is_word_like(token: &Token) -> bool {
    matches!(token, Token::Word(_) | Token::Placeholder(_))
}

fn is_literal(token: &Token) -> bool {
    matches!(
        token,
        Token::SingleQuotedString(_)
            | Token::DoubleQuotedString(_)
            | Token::TripleSingleQuotedString(_)
            | Token::TripleDoubleQuotedString(_)
            | Token::NationalStringLiteral(_)
            | Token::EscapedStringLiteral(_)
            | Token::UnicodeStringLiteral(_)
            | Token::HexStringLiteral(_)
            | Token::SingleQuotedByteStringLiteral(_)
            | Token::DoubleQuotedByteStringLiteral(_)
            | Token::Number(_, _)
    )
}

fn is_type_keyword_for_bracket(token: &Token) -> bool {
    if let Token::Word(w) = token {
        if w.quote_style.is_some() {
            return false;
        }
        matches!(
            w.value.to_ascii_uppercase().as_str(),
            "TEXT"
                | "UUID"
                | "INT"
                | "INTEGER"
                | "BIGINT"
                | "SMALLINT"
                | "VARCHAR"
                | "CHAR"
                | "BOOLEAN"
                | "BOOL"
                | "NUMERIC"
                | "DECIMAL"
                | "FLOAT"
                | "DOUBLE"
                | "DATE"
                | "TIME"
                | "TIMESTAMP"
                | "INTERVAL"
                | "JSONB"
                | "JSON"
                | "BYTEA"
                | "REAL"
                | "SERIAL"
                | "BIGSERIAL"
                | "INET"
                | "CIDR"
                | "MACADDR"
        )
    } else {
        false
    }
}

fn is_exists_keyword_token(token: &Token) -> bool {
    matches!(token, Token::Word(word) if word.keyword == Keyword::EXISTS)
}

/// Check if a token is a DDL keyword after which the next word is an object name
/// (table, view, index, etc.) — not a function call.
fn is_ddl_object_keyword(token: &Token) -> bool {
    if let Token::Word(w) = token {
        matches!(
            w.keyword,
            Keyword::TABLE
                | Keyword::VIEW
                | Keyword::INDEX
                | Keyword::FUNCTION
                | Keyword::PROCEDURE
                | Keyword::TRIGGER
                | Keyword::SEQUENCE
                | Keyword::TYPE
                | Keyword::SCHEMA
                | Keyword::DATABASE
        )
    } else {
        false
    }
}

fn is_qualified_ddl_object_name(tokens: &[TokenWithSpan], word_index: usize) -> bool {
    let mut cursor = word_index;

    loop {
        let Some(prev_idx) = prev_non_trivia_index(tokens, cursor) else {
            return false;
        };

        if matches!(tokens[prev_idx].token, Token::Period) {
            let Some(prev_word_idx) = prev_non_trivia_index(tokens, prev_idx) else {
                return false;
            };
            if !is_word_like(&tokens[prev_word_idx].token) {
                return false;
            }
            cursor = prev_word_idx;
            continue;
        }

        if !is_ddl_object_keyword(&tokens[prev_idx].token) {
            return false;
        }
        return is_ddl_object_definition_context(tokens, prev_idx);
    }
}

fn is_reference_target_name(tokens: &[TokenWithSpan], word_index: usize) -> bool {
    let mut cursor = word_index;

    loop {
        let Some(prev_idx) = prev_non_trivia_index(tokens, cursor) else {
            return false;
        };

        if matches!(tokens[prev_idx].token, Token::Period) {
            let Some(prev_word_idx) = prev_non_trivia_index(tokens, prev_idx) else {
                return false;
            };
            if !is_word_like(&tokens[prev_word_idx].token) {
                return false;
            }
            cursor = prev_word_idx;
            continue;
        }

        let Token::Word(prev_word) = &tokens[prev_idx].token else {
            return false;
        };

        return prev_word.keyword == Keyword::REFERENCES;
    }
}

fn is_copy_into_target_name(tokens: &[TokenWithSpan], word_index: usize) -> bool {
    let mut cursor = word_index;
    let mut steps = 0usize;

    while let Some(prev_idx) = prev_non_trivia_index(tokens, cursor) {
        match &tokens[prev_idx].token {
            Token::Word(word) if word.keyword == Keyword::INTO => {
                let Some(copy_idx) = prev_non_trivia_index(tokens, prev_idx) else {
                    return false;
                };
                return matches!(
                    &tokens[copy_idx].token,
                    Token::Word(copy_word) if copy_word.keyword == Keyword::COPY
                );
            }
            Token::Word(word)
                if matches!(
                    word.keyword,
                    Keyword::FROM
                        | Keyword::SELECT
                        | Keyword::WHERE
                        | Keyword::JOIN
                        | Keyword::ON
                        | Keyword::HAVING
                ) =>
            {
                return false;
            }
            Token::SemiColon | Token::Comma | Token::LParen | Token::RParen => return false,
            _ => {}
        }

        cursor = prev_idx;
        steps += 1;
        if steps > 64 {
            return false;
        }
    }

    false
}

/// Check if `word_index` is the table/view name in an `INSERT INTO schema.table` context.
fn is_insert_into_target_name(tokens: &[TokenWithSpan], word_index: usize) -> bool {
    let mut cursor = word_index;
    let mut steps = 0usize;

    while let Some(prev_idx) = prev_non_trivia_index(tokens, cursor) {
        match &tokens[prev_idx].token {
            Token::Word(word) if word.keyword == Keyword::INTO => {
                // Check for INSERT before INTO.
                let Some(insert_idx) = prev_non_trivia_index(tokens, prev_idx) else {
                    return false;
                };
                return matches!(
                    &tokens[insert_idx].token,
                    Token::Word(w) if w.keyword == Keyword::INSERT
                );
            }
            // Walk through schema qualifiers (schema.table).
            Token::Period => {}
            // Accept any unquoted word as a schema/table identifier — the name
            // may coincide with a SQL keyword (e.g. `metrics`, `daily`).
            Token::Word(word) if word.quote_style.is_none() => {}
            _ => return false,
        }

        cursor = prev_idx;
        steps += 1;
        if steps > 16 {
            return false;
        }
    }

    false
}

fn is_ddl_object_definition_context(tokens: &[TokenWithSpan], ddl_keyword_index: usize) -> bool {
    let Some(prev_idx) = prev_non_trivia_index(tokens, ddl_keyword_index) else {
        return false;
    };
    let Token::Word(prev_word) = &tokens[prev_idx].token else {
        return false;
    };

    if matches!(
        prev_word.keyword,
        Keyword::CREATE | Keyword::ALTER | Keyword::DROP | Keyword::TRUNCATE
    ) {
        return true;
    }

    if prev_word.keyword == Keyword::OR {
        if let Some(prev_prev_idx) = prev_non_trivia_index(tokens, prev_idx) {
            if let Token::Word(prev_prev_word) = &tokens[prev_prev_idx].token {
                return matches!(prev_prev_word.keyword, Keyword::CREATE | Keyword::ALTER);
            }
        }
    }

    false
}

/// Check if this pair involves a unary +/- (sign indicator) rather than binary.
fn is_unary_operator_pair(
    left: &TokenWithSpan,
    right: &TokenWithSpan,
    tokens: &[TokenWithSpan],
    left_idx: usize,
) -> bool {
    // Case 1: right token is +/- and left context suggests unary
    if matches!(right.token, Token::Plus | Token::Minus)
        && is_unary_prefix_context(&tokens[left_idx].token)
    {
        return true;
    }
    // Case 2: left token is +/- and the token before it suggests unary
    if matches!(left.token, Token::Plus | Token::Minus) {
        if let Some(prev_idx) = prev_non_trivia_index(tokens, left_idx) {
            if is_unary_prefix_context(&tokens[prev_idx].token) {
                return true;
            }
        } else {
            // No previous token — start of statement, so it's unary
            return true;
        }
    }
    false
}

fn is_bigquery_hyphenated_identifier_pair(
    left: &TokenWithSpan,
    right: &TokenWithSpan,
    tokens: &[TokenWithSpan],
    left_idx: usize,
    right_idx: usize,
) -> bool {
    if matches!(right.token, Token::Minus) {
        if !matches!(left.token, Token::Word(_)) {
            return false;
        }
        let Some(next_word_idx) = next_non_trivia_index(tokens, right_idx + 1) else {
            return false;
        };
        if !matches!(tokens[next_word_idx].token, Token::Word(_)) {
            return false;
        }
        let Some(next_after_word_idx) = next_non_trivia_index(tokens, next_word_idx + 1) else {
            return false;
        };
        return matches!(tokens[next_after_word_idx].token, Token::Period);
    }

    if matches!(left.token, Token::Minus) {
        if !matches!(right.token, Token::Word(_)) {
            return false;
        }
        let Some(prev_word_idx) = prev_non_trivia_index(tokens, left_idx) else {
            return false;
        };
        if !matches!(tokens[prev_word_idx].token, Token::Word(_)) {
            return false;
        }
        let Some(next_idx) = next_non_trivia_index(tokens, right_idx + 1) else {
            return false;
        };
        return matches!(tokens[next_idx].token, Token::Period);
    }

    false
}

fn is_filesystem_path_pair(
    left: &TokenWithSpan,
    right: &TokenWithSpan,
    tokens: &[TokenWithSpan],
    left_idx: usize,
    right_idx: usize,
    dialect: Dialect,
) -> bool {
    if !matches!(
        dialect,
        Dialect::Databricks | Dialect::Clickhouse | Dialect::Snowflake
    ) {
        return false;
    }

    let div_index = if matches!(left.token, Token::Div) {
        Some(left_idx)
    } else if matches!(right.token, Token::Div) {
        let left_is_context_keyword = is_path_context_keyword_token(&left.token);
        let left_is_path_segment = prev_non_trivia_index(tokens, left_idx)
            .is_some_and(|idx| matches!(tokens[idx].token, Token::Div));
        if left_is_context_keyword && !left_is_path_segment {
            return false;
        }
        Some(right_idx)
    } else {
        None
    };
    let Some(div_index) = div_index else {
        return false;
    };

    let prev_idx = prev_non_trivia_index(tokens, div_index);
    let next_idx = next_non_trivia_index(tokens, div_index + 1);
    let prev_ok = prev_idx.is_some_and(|idx| matches!(tokens[idx].token, Token::Word(_)));
    let next_ok = next_idx.is_some_and(|idx| matches!(tokens[idx].token, Token::Word(_)));
    if !(prev_ok || next_ok) {
        return false;
    }

    if dialect == Dialect::Snowflake {
        return snowflake_stage_path_context_within(tokens, div_index, 12);
    }

    path_context_keyword_within(tokens, div_index, 6)
}

fn is_path_context_keyword_token(token: &Token) -> bool {
    let Token::Word(word) = token else {
        return false;
    };
    word.value.eq_ignore_ascii_case("JAR") || word.value.eq_ignore_ascii_case("MODEL")
}

fn path_context_keyword_within(tokens: &[TokenWithSpan], from_idx: usize, limit: usize) -> bool {
    let mut cursor = from_idx;
    let mut steps = 0usize;
    while let Some(prev_idx) = prev_non_trivia_index(tokens, cursor) {
        if let Token::Word(word) = &tokens[prev_idx].token {
            if matches!(word.keyword, Keyword::JAR) {
                return true;
            }
            if word.value.eq_ignore_ascii_case("JAR") || word.value.eq_ignore_ascii_case("MODEL") {
                return true;
            }
        }
        cursor = prev_idx;
        steps += 1;
        if steps >= limit {
            break;
        }
    }
    false
}

fn snowflake_stage_path_context_within(
    tokens: &[TokenWithSpan],
    from_idx: usize,
    limit: usize,
) -> bool {
    let mut cursor = from_idx;
    let mut steps = 0usize;
    while let Some(prev_idx) = prev_non_trivia_index(tokens, cursor) {
        match &tokens[prev_idx].token {
            Token::AtSign => return true,
            Token::Word(word) if word.value.starts_with('@') => return true,
            _ => {}
        }
        cursor = prev_idx;
        steps += 1;
        if steps >= limit {
            break;
        }
    }
    false
}

/// Check if a token is a context where the following +/- is unary.
fn is_unary_prefix_context(token: &Token) -> bool {
    if matches!(
        token,
        Token::Comma
            | Token::LParen
            | Token::Eq
            | Token::Neq
            | Token::Lt
            | Token::Gt
            | Token::LtEq
            | Token::GtEq
    ) {
        return true;
    }
    if let Token::Word(w) = token {
        if matches!(
            w.keyword,
            Keyword::SELECT
                | Keyword::WHERE
                | Keyword::WHEN
                | Keyword::THEN
                | Keyword::ELSE
                | Keyword::AND
                | Keyword::OR
                | Keyword::ON
                | Keyword::SET
                | Keyword::CASE
                | Keyword::BETWEEN
                | Keyword::IN
                | Keyword::VALUES
                | Keyword::INTERVAL
                | Keyword::YEAR
                | Keyword::MONTH
                | Keyword::DAY
                | Keyword::HOUR
                | Keyword::MINUTE
                | Keyword::SECOND
                | Keyword::RETURN
                | Keyword::RETURNS
        ) {
            return true;
        }
    }
    false
}

/// Expected spacing before left-paren.
fn expected_spacing_before_lparen(
    left: &TokenWithSpan,
    tokens: &[TokenWithSpan],
    left_idx: usize,
    dialect: Dialect,
) -> ExpectedSpacing {
    match &left.token {
        // Function call: no space between function name and (
        Token::Word(w) if w.quote_style.is_none() => {
            if dialect == Dialect::Snowflake {
                if w.value.eq_ignore_ascii_case("MATCH_RECOGNIZE")
                    || w.value.eq_ignore_ascii_case("PATTERN")
                {
                    return ExpectedSpacing::Single;
                }
                if w.value.eq_ignore_ascii_case("MATCH_CONDITION") {
                    return ExpectedSpacing::NoneInline;
                }
            }
            if w.value.eq_ignore_ascii_case("EXISTS") {
                if exists_requires_space_before_lparen(tokens, left_idx) {
                    return ExpectedSpacing::Single;
                }
                return ExpectedSpacing::NoneInline;
            }
            // Keywords that should have a space before (
            if is_keyword_requiring_space_before_paren(w.keyword) {
                // AS in CTE: `AS (` should be single-inline (collapse newlines to space)
                // USING, FROM, etc.: single space (newline acceptable)
                if matches!(w.keyword, Keyword::AS) {
                    return ExpectedSpacing::SingleInline;
                }
                return ExpectedSpacing::Single;
            }
            // INSERT INTO table_name ( — the ( opens a column list.
            // Checked before the NoKeyword guard because the table name may
            // coincide with a SQL keyword (e.g., metrics.daily → daily is Keyword).
            if is_insert_into_target_name(tokens, left_idx) {
                return ExpectedSpacing::Single;
            }
            // Check if this word is a table/view name after CREATE TABLE/VIEW —
            // the ( opens a column list, not a function call, so skip.
            if w.keyword == Keyword::NoKeyword {
                if is_reference_target_name(tokens, left_idx) {
                    return ExpectedSpacing::Single;
                }
                if is_copy_into_target_name(tokens, left_idx) {
                    return ExpectedSpacing::Single;
                }
                if is_qualified_ddl_object_name(tokens, left_idx) {
                    return ExpectedSpacing::Skip;
                }
            }
            // Regular function call or type name: no space
            ExpectedSpacing::NoneInline
        }
        // After closing paren/bracket: single space (subquery, etc.)
        Token::RParen | Token::RBracket => ExpectedSpacing::Single,
        // After literal: single space
        _ if is_literal(&left.token) => ExpectedSpacing::Single,
        // After number: no space (could be type precision like numeric(5,2))
        Token::Number(_, _) => ExpectedSpacing::None,
        // After comma: single space
        Token::Comma => ExpectedSpacing::Single,
        // After operator: skip
        _ if is_binary_operator(&left.token) || is_comparison_operator(&left.token) => {
            ExpectedSpacing::Skip
        }
        _ => ExpectedSpacing::Skip,
    }
}

fn exists_requires_space_before_lparen(tokens: &[TokenWithSpan], left_idx: usize) -> bool {
    let Some(prev_idx) = prev_non_trivia_index(tokens, left_idx) else {
        return false;
    };

    match &tokens[prev_idx].token {
        Token::Word(word) => {
            matches!(
                word.keyword,
                Keyword::AND
                    | Keyword::OR
                    | Keyword::NOT
                    | Keyword::WHERE
                    | Keyword::HAVING
                    | Keyword::WHEN
                    | Keyword::THEN
                    | Keyword::ELSE
            ) || matches!(
                word.value.to_ascii_uppercase().as_str(),
                "AND" | "OR" | "NOT" | "WHERE" | "HAVING" | "WHEN" | "THEN" | "ELSE"
            )
        }
        Token::RParen
        | Token::LParen
        | Token::Eq
        | Token::Neq
        | Token::Lt
        | Token::Gt
        | Token::LtEq
        | Token::GtEq => true,
        _ => false,
    }
}

/// Keywords that should have a space before `(`.
fn is_keyword_requiring_space_before_paren(keyword: Keyword) -> bool {
    matches!(
        keyword,
        Keyword::AS
            | Keyword::USING
            | Keyword::FROM
            | Keyword::JOIN
            | Keyword::ON
            | Keyword::WHERE
            | Keyword::IN
            | Keyword::BETWEEN
            | Keyword::WHEN
            | Keyword::THEN
            | Keyword::ELSE
            | Keyword::AND
            | Keyword::OR
            | Keyword::NOT
            | Keyword::HAVING
            | Keyword::OVER
            | Keyword::PARTITION
            | Keyword::ORDER
            | Keyword::GROUP
            | Keyword::LIMIT
            | Keyword::UNION
            | Keyword::INTERSECT
            | Keyword::EXCEPT
            | Keyword::RECURSIVE
            | Keyword::WITH
            | Keyword::SELECT
            | Keyword::INTO
            | Keyword::TABLE
            | Keyword::VALUES
            | Keyword::SET
            | Keyword::RETURNS
            | Keyword::FILTER
            | Keyword::CONFLICT
            | Keyword::BY
    )
}

/// Expected spacing after right-paren.
fn expected_spacing_after_rparen(
    right: &TokenWithSpan,
    _tokens: &[TokenWithSpan],
    _right_idx: usize,
) -> ExpectedSpacing {
    match &right.token {
        // ) followed by . or :: or [ — no space
        Token::Period | Token::DoubleColon | Token::LBracket | Token::RBracket => {
            ExpectedSpacing::None
        }
        // ) followed by , — no space before comma
        Token::Comma => ExpectedSpacing::None,
        // ) followed by ; — no space
        Token::SemiColon => ExpectedSpacing::Skip,
        // ) followed by ) — no space
        Token::RParen => ExpectedSpacing::None,
        // ) followed by ( — single space
        Token::LParen => ExpectedSpacing::Single,
        // ) followed by keyword or identifier — single space
        _ => ExpectedSpacing::Single,
    }
}

fn has_comment_between(tokens: &[TokenWithSpan], left: usize, right: usize) -> bool {
    tokens[left + 1..right].iter().any(|t| {
        matches!(
            t.token,
            Token::Whitespace(Whitespace::SingleLineComment { .. })
                | Token::Whitespace(Whitespace::MultiLineComment(_))
        )
    })
}

fn template_spans(sql: &str) -> Vec<Lt01TemplateSpan> {
    let mut spans = Vec::new();
    let mut index = 0usize;
    while let Some((open, close)) = find_next_template_open(sql, index) {
        let payload_start = open + 2;
        if let Some(rel_close) = sql[payload_start..].find(close) {
            let close_index = payload_start + rel_close + close.len();
            spans.push((open, close_index));
            index = close_index;
        } else {
            spans.push((open, sql.len()));
            break;
        }
    }
    spans
}

fn find_next_template_open(sql: &str, from: usize) -> Option<(usize, &'static str)> {
    let rest = sql.get(from..)?;
    [("{{", "}}"), ("{%", "%}"), ("{#", "#}")]
        .into_iter()
        .filter_map(|(open, close)| rest.find(open).map(|offset| (from + offset, close)))
        .min_by_key(|(index, _)| *index)
}

fn contains_template_marker(sql: &str) -> bool {
    sql.contains("{{") || sql.contains("{%") || sql.contains("{#")
}

fn overlaps_template_span(spans: &[Lt01TemplateSpan], start: usize, end: usize) -> bool {
    spans
        .iter()
        .any(|(template_start, template_end)| start < *template_end && end > *template_start)
}

fn collect_ansi_national_string_literal_violations(
    sql: &str,
    tokens: &[TokenWithSpan],
    dialect: Dialect,
    templated_spans: &[Lt01TemplateSpan],
    violations: &mut Vec<Lt01Violation>,
) {
    if matches!(dialect, Dialect::Mssql) {
        return;
    }

    for token in tokens {
        let Token::NationalStringLiteral(_) = token.token else {
            continue;
        };
        let Some((start, end)) = token_offsets(sql, token) else {
            continue;
        };
        if start >= end || end > sql.len() || overlaps_template_span(templated_spans, start, end) {
            continue;
        }
        let raw = &sql[start..end];
        if raw.len() < 3 {
            continue;
        }
        let Some(prefix) = raw.chars().next() else {
            continue;
        };
        if !(prefix == 'N' || prefix == 'n') || !raw[1..].starts_with('\'') {
            continue;
        }
        let replacement = format!("{prefix} {}", &raw[1..]);
        violations.push(((start, end), vec![(start, end, replacement)]));
    }
}

fn collect_template_string_spacing_violations(
    sql: &str,
    dialect: Dialect,
    templated_spans: &[Lt01TemplateSpan],
    violations: &mut Vec<Lt01Violation>,
) {
    for (template_start, template_end) in templated_spans {
        let mut cursor = *template_start;
        while cursor < *template_end {
            let Some((quote_start, quote_char)) = next_quote_in_range(sql, cursor, *template_end)
            else {
                break;
            };
            let Some(quote_end) =
                find_closing_quote(sql, quote_start + 1, *template_end, quote_char)
            else {
                break;
            };
            let content = &sql[quote_start + 1..quote_end];
            let Some(tokens) = tokenized(content, dialect) else {
                cursor = quote_end + 1;
                continue;
            };

            let mut fragment_violations = Vec::new();
            collect_pair_spacing_violations(
                content,
                &tokens,
                dialect,
                &[],
                &mut fragment_violations,
            );
            collect_ansi_national_string_literal_violations(
                content,
                &tokens,
                dialect,
                &[],
                &mut fragment_violations,
            );

            for ((start, end), _) in fragment_violations {
                if start >= end || end > content.len() {
                    continue;
                }
                let absolute_start = quote_start + 1 + start;
                let absolute_end = quote_start + 1 + end;
                violations.push(((absolute_start, absolute_end), Vec::new()));
            }

            cursor = quote_end + 1;
        }
    }
}

fn next_quote_in_range(sql: &str, start: usize, end: usize) -> Option<(usize, char)> {
    let mut index = start;
    while index < end {
        let ch = sql[index..].chars().next()?;
        if ch == '\'' || ch == '"' {
            return Some((index, ch));
        }
        index += ch.len_utf8();
    }
    None
}

fn find_closing_quote(sql: &str, start: usize, end: usize, quote: char) -> Option<usize> {
    let mut index = start;
    while index < end {
        let ch = sql[index..].chars().next()?;
        if ch == '\\' {
            let next = index + ch.len_utf8();
            if next < end {
                let escaped = sql[next..].chars().next()?;
                index = next + escaped.len_utf8();
                continue;
            }
        }
        if ch == quote {
            return Some(index);
        }
        index += ch.len_utf8();
    }
    None
}

fn snowflake_pattern_token_indices(
    tokens: &[TokenWithSpan],
    non_trivia: &[usize],
) -> HashSet<usize> {
    let mut out = HashSet::new();
    let mut cursor = 0usize;

    while cursor < non_trivia.len() {
        let token_index = non_trivia[cursor];
        let Token::Word(word) = &tokens[token_index].token else {
            cursor += 1;
            continue;
        };
        if !word.value.eq_ignore_ascii_case("PATTERN") {
            cursor += 1;
            continue;
        }

        let Some(paren_pos) = ((cursor + 1)..non_trivia.len())
            .find(|idx| matches!(tokens[non_trivia[*idx]].token, Token::LParen))
        else {
            cursor += 1;
            continue;
        };

        let mut depth = 0usize;
        let mut end_pos = None;
        for (pos, idx) in non_trivia.iter().copied().enumerate().skip(paren_pos) {
            match tokens[idx].token {
                Token::LParen => depth += 1,
                Token::RParen => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    if depth == 0 {
                        end_pos = Some(pos);
                        break;
                    }
                }
                _ => {}
            }
        }

        let Some(end_pos) = end_pos else {
            cursor += 1;
            continue;
        };
        for idx in non_trivia.iter().take(end_pos + 1).skip(paren_pos) {
            out.insert(*idx);
        }
        cursor = end_pos + 1;
    }

    out
}

fn type_angle_token_indices(tokens: &[TokenWithSpan], non_trivia: &[usize]) -> HashSet<usize> {
    let mut out = HashSet::new();
    let mut stack = Vec::<usize>::new();

    for (pos, token_idx) in non_trivia.iter().copied().enumerate() {
        let token = &tokens[token_idx].token;
        match token {
            Token::Lt => {
                let prev_idx = pos
                    .checked_sub(1)
                    .and_then(|value| non_trivia.get(value).copied());
                if prev_idx.is_some_and(|idx| is_type_constructor(&tokens[idx].token)) {
                    out.insert(token_idx);
                    stack.push(token_idx);
                }
            }
            Token::Gt if !stack.is_empty() => {
                out.insert(token_idx);
                stack.pop();
            }
            Token::ShiftRight if stack.len() >= 2 => {
                out.insert(token_idx);
                stack.pop();
                stack.pop();
            }
            _ => {}
        }
    }

    out
}

fn supports_type_angle_spacing(dialect: Dialect) -> bool {
    matches!(
        dialect,
        Dialect::Bigquery | Dialect::Hive | Dialect::Databricks
    )
}

fn is_type_constructor(token: &Token) -> bool {
    let Token::Word(word) = token else {
        return false;
    };
    word.value.eq_ignore_ascii_case("ARRAY")
        || word.value.eq_ignore_ascii_case("STRUCT")
        || word.value.eq_ignore_ascii_case("MAP")
}

fn is_type_angle_spacing_pair(
    left: &TokenWithSpan,
    right: &TokenWithSpan,
    left_idx: usize,
    right_idx: usize,
    type_angle_tokens: &HashSet<usize>,
) -> bool {
    let left_is_type_angle = type_angle_tokens.contains(&left_idx);
    let right_is_type_angle = type_angle_tokens.contains(&right_idx);

    if right_is_type_angle && matches!(right.token, Token::Lt | Token::Gt | Token::ShiftRight) {
        return true;
    }
    if left_is_type_angle && matches!(left.token, Token::Lt) {
        return true;
    }
    if left_is_type_angle
        && matches!(left.token, Token::Gt | Token::ShiftRight)
        && matches!(
            right.token,
            Token::Comma | Token::RParen | Token::RBracket | Token::LBracket | Token::Gt
        )
    {
        return true;
    }

    false
}

// ---------------------------------------------------------------------------
// Token utilities
// ---------------------------------------------------------------------------

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

fn token_offsets(sql: &str, token: &TokenWithSpan) -> Option<(usize, usize)> {
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

fn next_non_trivia_index(tokens: &[TokenWithSpan], mut index: usize) -> Option<usize> {
    while index < tokens.len() {
        if !is_trivia_token(&tokens[index].token) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn prev_non_trivia_index(tokens: &[TokenWithSpan], mut index: usize) -> Option<usize> {
    while index > 0 {
        index -= 1;
        if !is_trivia_token(&tokens[index].token) {
            return Some(index);
        }
    }
    None
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
        let line = 1 + sql.as_bytes().iter().filter(|byte| **byte == b'\n').count();
        let column = sql
            .rsplit_once('\n')
            .map_or(sql.chars().count() + 1, |(_, tail)| {
                tail.chars().count() + 1
            });
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
    Some((line, column))
}

fn relative_location(
    location: Location,
    statement_start_line: usize,
    statement_start_column: usize,
) -> Option<Location> {
    if location.line == 0 || location.column == 0 {
        return None;
    }

    let line = location.line as usize;
    let column = location.column as usize;
    if line < statement_start_line {
        return None;
    }

    let relative_line = line - statement_start_line + 1;
    let relative_column = if line == statement_start_line {
        if column < statement_start_column {
            return None;
        }
        column - statement_start_column + 1
    } else {
        column
    };

    Some(Location::new(relative_line as u64, relative_column as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::{Dialect, IssueAutofixApplicability};

    fn run(sql: &str) -> Vec<Issue> {
        run_with_dialect(sql, Dialect::Generic)
    }

    fn run_with_dialect(sql: &str, dialect: Dialect) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutSpacing::default();
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(
                    statement,
                    &RuleContext::new(sql, 0..sql.len(), index).with_dialect(dialect),
                )
            })
            .collect()
    }

    fn run_statementless_with_dialect(sql: &str, dialect: Dialect) -> Vec<Issue> {
        run_statementless_with_rule(sql, dialect, LayoutSpacing::default())
    }

    fn run_statementless_with_rule(sql: &str, dialect: Dialect, rule: LayoutSpacing) -> Vec<Issue> {
        let placeholder = parse_sql("SELECT 1").expect("parse placeholder");
        rule.check_with_context(
            &placeholder[0],
            &RuleContext::new(sql, 0..sql.len(), 0).with_dialect(dialect),
        )
    }

    fn apply_all_issue_autofixes(sql: &str, issues: &[Issue]) -> String {
        let mut out = sql.to_string();
        let mut edits = issues
            .iter()
            .filter_map(|issue| issue.autofix.as_ref())
            .flat_map(|autofix| autofix.edits.clone())
            .collect::<Vec<_>>();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        for edit in edits.into_iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        out
    }

    #[test]
    fn allows_bigquery_array_type_angle_brackets_without_spaces() {
        let issues = run_with_dialect(
            "SELECT ARRAY<FLOAT64>[1, 2, 3] AS floats;",
            Dialect::Bigquery,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_create_table_with_qualified_name_before_column_list() {
        let issues = run("CREATE TABLE db.schema_name.tbl_name (id INT)");
        assert!(issues.is_empty());
    }

    #[test]
    fn fixes_reference_target_column_list_spacing() {
        let sql = "create table tab1 (b int references tab2(b))";
        let issues = run_statementless_with_dialect(sql, Dialect::Ansi);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "create table tab1 (b int references tab2 (b))");
    }

    #[test]
    fn allows_bigquery_hyphenated_project_identifier() {
        let issues = run_statementless_with_dialect(
            "SELECT col_foo FROM foo-bar.foo.bar",
            Dialect::Bigquery,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_bigquery_function_array_offset_access() {
        let sql = "SELECT testFunction(a)[OFFSET(0)].* FROM table1";
        let issues = run_statementless_with_dialect(sql, Dialect::Bigquery);
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_hive_struct_and_array_datatype_angles() {
        let sql = "select col1::STRUCT<foo: int>, col2::ARRAY<int> from t";
        let issues = run_statementless_with_dialect(sql, Dialect::Hive);
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_sparksql_file_literal_path() {
        let sql = "ADD JAR path/to/some.jar;";
        let issues = run_statementless_with_dialect(sql, Dialect::Databricks);
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_clickhouse_system_model_path() {
        let sql = "SYSTEM RELOAD MODEL /model/path;";
        let issues = run_statementless_with_dialect(sql, Dialect::Clickhouse);
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn detects_alias_alignment_when_configured() {
        let sql = "SELECT\n\tcol1 AS a,\n\tlonger_col AS b\nFROM t";
        let issues = run_statementless_with_rule(
            sql,
            Dialect::Ansi,
            LayoutSpacing {
                align_alias_expression: true,
                tab_space_size: 4,
                ..LayoutSpacing::default()
            },
        );
        assert!(!issues.is_empty());
    }

    #[test]
    fn detects_alias_alignment_with_tabs_when_columns_are_equal_width() {
        let sql = "SELECT\n\tcol1 AS alias1,\n\tcol2 AS alias2\nFROM table1";
        let issues = run_statementless_with_rule(
            sql,
            Dialect::Ansi,
            LayoutSpacing {
                align_alias_expression: true,
                align_with_tabs: true,
                tab_space_size: 4,
                ..LayoutSpacing::default()
            },
        );
        assert!(
            !issues.is_empty(),
            "tab indentation alignment should flag spaces before AS"
        );
    }

    #[test]
    fn detects_create_table_datatype_alignment_when_configured() {
        let sql = "CREATE TABLE tbl (\n    foo VARCHAR(25) NOT NULL,\n    barbar INT NULL\n)";
        let issues = run_statementless_with_rule(
            sql,
            Dialect::Ansi,
            LayoutSpacing {
                align_data_type: true,
                ..LayoutSpacing::default()
            },
        );
        assert!(!issues.is_empty());
    }

    #[test]
    fn does_not_flag_create_table_alignment_when_columns_are_already_aligned() {
        let sql = "CREATE TABLE foo (\n    x INT NOT NULL PRIMARY KEY,\n    y INT NULL,\n    z INT NULL\n);";
        let issues = run_statementless_with_rule(
            sql,
            Dialect::Ansi,
            LayoutSpacing {
                align_data_type: true,
                align_column_constraint: true,
                ..LayoutSpacing::default()
            },
        );
        assert!(
            issues.is_empty(),
            "expected no LT01 alignment issues: {issues:?}"
        );
    }

    #[test]
    fn statementless_fixes_comment_on_function_spacing() {
        let sql = "COMMENT ON FUNCTION x (foo) IS 'y';";
        let issues = run_statementless_with_dialect(sql, Dialect::Postgres);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "COMMENT ON FUNCTION x(foo) IS 'y';");
    }

    #[test]
    fn statementless_fixes_split_tsql_comparison_operator() {
        let sql = "SELECT col1 FROM table1 WHERE 1 > = 1";
        let issues = run_statementless_with_dialect(sql, Dialect::Mssql);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT col1 FROM table1 WHERE 1 >= 1");
    }

    #[test]
    fn statementless_fixes_tsql_compound_assignment_operator() {
        let sql = "SET @param1+=1";
        let issues = run_statementless_with_dialect(sql, Dialect::Mssql);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SET @param1 += 1");
    }

    #[test]
    fn allows_sparksql_multi_unit_interval_minus() {
        let sql = "SELECT INTERVAL -2 HOUR '3' MINUTE AS col;";
        let issues = run_statementless_with_dialect(sql, Dialect::Databricks);
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_templated_areas_skips_template_artifacts() {
        let sql = "{{ 'SELECT 1, 4' }}, 5, 6";
        let issues = run_statementless_with_rule(
            sql,
            Dialect::Generic,
            LayoutSpacing {
                ignore_templated_areas: true,
                ..LayoutSpacing::default()
            },
        );
        assert!(issues.is_empty(), "template-only spacing should be ignored");
    }

    #[test]
    fn ignore_templated_areas_still_fixes_non_template_region() {
        let sql = "{{ 'SELECT 1, 4' }}, 5 , 6";
        let issues = run_statementless_with_rule(
            sql,
            Dialect::Generic,
            LayoutSpacing {
                ignore_templated_areas: true,
                ..LayoutSpacing::default()
            },
        );
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "{{ 'SELECT 1, 4' }}, 5, 6");
    }

    #[test]
    fn templated_string_content_is_checked_when_not_ignored() {
        let sql = "{{ 'SELECT 1 ,4' }}";
        let issues = run_statementless_with_rule(
            sql,
            Dialect::Generic,
            LayoutSpacing {
                ignore_templated_areas: false,
                ..LayoutSpacing::default()
            },
        );
        assert!(!issues.is_empty());
        assert!(
            issues.iter().all(|issue| issue.autofix.is_none()),
            "template-internal checks are detection-only"
        );
    }

    #[test]
    fn templated_string_content_passes_when_clean() {
        let sql = "{{ 'SELECT 1, 4' }}";
        let issues = run_statementless_with_rule(
            sql,
            Dialect::Generic,
            LayoutSpacing {
                ignore_templated_areas: false,
                ..LayoutSpacing::default()
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_snowflake_match_recognize_pattern_spacing() {
        let sql = "select * from stock_price_history\n  match_recognize (\n    pattern ((A | B){5} C+)\n  )";
        let issues = run_statementless_with_dialect(sql, Dialect::Snowflake);
        assert!(issues.is_empty(), "snowflake pattern syntax should pass");
    }

    #[test]
    fn fixes_snowflake_match_condition_newline_before_paren() {
        let sql = "select\n    table1.pk1\nfrom table1\n    asof join\n    table2\n    match_condition\n    (t1 > t2)";
        let issues = run_with_dialect(sql, Dialect::Snowflake);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert!(
            fixed.contains("match_condition(t1 > t2)"),
            "expected inline match_condition: {fixed}"
        );
    }

    #[test]
    fn fixes_snowflake_copy_into_target_column_list_spacing() {
        let sql = "copy into DB.SCHEMA.ProblemHere(col1)\nfrom @my_stage/file";
        let issues = run_statementless_with_dialect(sql, Dialect::Snowflake);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert!(
            fixed.contains("DB.SCHEMA.ProblemHere (col1)"),
            "fixed: {fixed}"
        );
    }

    #[test]
    fn fixes_snowflake_copy_into_target_column_list_spacing_with_placeholder_prefix() {
        let sql = "copy into ${env}_ENT_LANDING.SCHEMA_NAME.ProblemHere(col1)\nfrom @my_stage/file";
        let issues = run_statementless_with_dialect(sql, Dialect::Snowflake);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert!(
            fixed.contains(".SCHEMA_NAME.ProblemHere (col1)"),
            "fixed: {fixed}"
        );
    }

    #[test]
    fn allows_snowflake_stage_path_without_spacing_around_slash() {
        let sql = "copy into t from @my_stage/file";
        let issues = run_statementless_with_dialect(sql, Dialect::Snowflake);
        assert!(
            issues.is_empty(),
            "snowflake stage path should not force spaces around slash: {issues:?}"
        );
    }

    // --- Trailing whitespace tests ---

    #[test]
    fn flags_trailing_whitespace() {
        let sql = "SELECT 1     \n";
        let issues = run(sql);
        assert!(!issues.is_empty(), "should flag trailing whitespace");
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT 1\n");
    }

    #[test]
    fn flags_trailing_whitespace_on_initial_blank_line() {
        let sql = " \nSELECT 1     \n";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "\nSELECT 1\n");
    }

    // --- Operator spacing tests ---

    #[test]
    fn flags_compact_operator() {
        let sql = "SELECT 1+2";
        let issues = run(sql);
        assert!(!issues.is_empty(), "should flag compact 1+2");
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT 1 + 2");
    }

    #[test]
    fn flags_compact_operator_expression() {
        let sql = "select\n    field,\n    date(field_1)-date(field_2) as diff\nfrom tbl";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert!(
            fixed.contains("date(field_1) - date(field_2)"),
            "should fix operator spacing: {fixed}"
        );
    }

    #[test]
    fn flags_plus_between_identifier_and_literal() {
        let sql = "SELECT a +'b'+ 'c' FROM tbl";
        let issues = run(sql);
        assert!(
            !issues.is_empty(),
            "should flag operator spacing around string literals"
        );
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT a + 'b' + 'c' FROM tbl");
    }

    #[test]
    fn does_not_flag_simple_spacing() {
        assert!(run("SELECT * FROM t WHERE a = 1").is_empty());
    }

    #[test]
    fn does_not_flag_sign_indicators() {
        let issues = run("SELECT 1, +2, -4");
        // Sign indicators before numbers should not be flagged
        assert!(
            issues.is_empty(),
            "unary signs should not be flagged: {issues:?}"
        );
    }

    #[test]
    fn does_not_flag_newline_operator() {
        assert!(run("SELECT 1\n+ 2").is_empty());
        assert!(run("SELECT 1\n    + 2").is_empty());
    }

    // --- Comma spacing tests ---

    #[test]
    fn flags_space_before_comma() {
        let sql = "SELECT 1 ,4";
        let issues = run(sql);
        assert!(!issues.is_empty(), "should flag space before comma");
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT 1, 4");
    }

    #[test]
    fn flags_no_space_after_comma() {
        let sql = "SELECT 1,4";
        let issues = run(sql);
        assert!(!issues.is_empty(), "should flag missing space after comma");
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT 1, 4");
    }

    #[test]
    fn flags_excessive_space_after_comma() {
        let sql = "SELECT 1,   4";
        let issues = run(sql);
        assert!(
            !issues.is_empty(),
            "should flag excessive space after comma"
        );
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT 1, 4");
    }

    // --- Bracket spacing tests ---

    #[test]
    fn flags_missing_space_before_paren_after_keyword() {
        let sql = "SELECT * FROM(SELECT 1 AS C1)AS T1;";
        let issues = run(sql);
        assert!(!issues.is_empty(), "should flag FROM( and )AS: {issues:?}");
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT * FROM (SELECT 1 AS C1) AS T1;");
    }

    // --- Missing space tests ---

    #[test]
    fn flags_cte_missing_space_after_as() {
        let sql = "WITH a AS(select 1) select * from a";
        let issues = run(sql);
        assert!(!issues.is_empty(), "should flag AS(");
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "WITH a AS (select 1) select * from a");
    }

    #[test]
    fn flags_cte_multiple_spaces_after_as() {
        let sql = "WITH a AS  (select 1) select * from a";
        let issues = run(sql);
        assert!(!issues.is_empty(), "should flag AS  (");
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "WITH a AS (select 1) select * from a");
    }

    #[test]
    fn flags_missing_space_after_using() {
        let sql = "select * from a JOIN b USING(x)";
        let issues = run(sql);
        assert!(!issues.is_empty(), "should flag USING(");
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "select * from a JOIN b USING (x)");
    }

    // --- Excessive whitespace tests ---

    #[test]
    fn flags_excessive_whitespace() {
        let sql = "SELECT     1";
        let issues = run(sql);
        assert!(!issues.is_empty(), "should flag excessive whitespace");
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT 1");
    }

    #[test]
    fn flags_excessive_whitespace_multi() {
        let sql = "select\n    1 + 2     + 3     + 4        -- Comment\nfrom     foo";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    1 + 2 + 3 + 4        -- Comment\nfrom foo"
        );
    }

    // --- Literal spacing tests ---

    #[test]
    fn flags_literal_operator_spacing() {
        let sql = "SELECT ('foo'||'bar') as buzz";
        let issues = run(sql);
        assert!(
            !issues.is_empty(),
            "should flag compact || operator: {issues:?}"
        );
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT ('foo' || 'bar') as buzz");
    }

    #[test]
    fn flags_literal_as_spacing() {
        let sql = "SELECT\n    'foo'AS   bar\nFROM foo";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT\n    'foo' AS bar\nFROM foo");
    }

    #[test]
    fn flags_ansi_national_string_literal_spacing() {
        let sql = "SELECT a + N'b' + N'c' FROM tbl;";
        let issues = run_with_dialect(sql, Dialect::Ansi);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT a + N 'b' + N 'c' FROM tbl;");
    }

    // --- Function spacing tests ---

    #[test]
    fn does_not_flag_function_call() {
        assert!(run("SELECT foo(5) FROM T1;").is_empty());
        assert!(run("SELECT COUNT(*) FROM tbl\n\n").is_empty());
    }

    // --- Cast operator tests ---

    #[test]
    fn flags_spaced_cast_operator() {
        let sql = "SELECT '1' :: INT;";
        let issues = run(sql);
        assert!(!issues.is_empty(), "should flag space around ::");
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT '1'::INT;");
    }

    // --- JSON arrow tests ---

    #[test]
    fn flags_compact_json_arrow_operator() {
        let sql = "SELECT payload->>'id' FROM t";
        let issues = run(sql);
        assert!(
            issues.len() >= 2,
            "should flag 2+ violations for compact json-arrow"
        );
        assert!(
            issues
                .iter()
                .all(|issue| issue.autofix.as_ref().is_some_and(
                    |autofix| autofix.applicability == IssueAutofixApplicability::Safe
                )),
            "expected safe autofix metadata"
        );

        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT payload ->> 'id' FROM t");
    }

    #[test]
    fn does_not_flag_exists_without_space_before_parenthesis() {
        let no_space = "SELECT\n    EXISTS(\n        SELECT 1\n    ) AS has_row\nFROM t";
        assert!(run(no_space).is_empty());
    }

    #[test]
    fn flags_space_before_exists_parenthesis_in_select_list() {
        let sql = "SELECT 1,\n    EXISTS (\n        SELECT 1\n    ) AS has_row\nFROM t";
        let issues = run(sql);
        assert!(
            !issues.is_empty(),
            "expected EXISTS-space violation in select list"
        );
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert!(
            fixed.contains("EXISTS(\n"),
            "expected EXISTS( after fix, got: {fixed}"
        );
    }

    #[test]
    fn requires_space_before_exists_parenthesis_after_where() {
        let sql = "SELECT 1\nWHERE EXISTS(\n    SELECT 1\n)";
        let issues = run(sql);
        assert!(
            !issues.is_empty(),
            "expected missing-space violation for WHERE EXISTS("
        );
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert!(
            fixed.contains("WHERE EXISTS (\n"),
            "expected WHERE EXISTS ( after fix, got: {fixed}"
        );
    }

    #[test]
    fn merge_violations_prefers_fixable_duplicate_span() {
        let mut violations = vec![
            ((10, 10), Vec::new()),
            ((10, 10), vec![(10, 10, " ".to_string())]),
        ];
        merge_violations_by_span(&mut violations);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].0, (10, 10));
        assert_eq!(violations[0].1, vec![(10, 10, " ".to_string())]);
    }

    // --- Safe pass cases ---

    #[test]
    fn does_not_flag_spacing_patterns_inside_literals_or_comments() {
        let issues = run("SELECT 'payload->>''id''' AS txt -- EXISTS (\nFROM t");
        assert!(
            issues.is_empty(),
            "should not flag content inside literals/comments: {issues:?}"
        );
    }

    #[test]
    fn does_not_flag_correct_comma_spacing() {
        assert!(run("SELECT 1, 4").is_empty());
    }

    #[test]
    fn does_not_flag_correct_cast() {
        assert!(run("SELECT '1'::INT;").is_empty());
    }

    #[test]
    fn does_not_flag_qualified_identifiers() {
        // Dot-separated identifiers should not have spaces
        assert!(run("SELECT a.b FROM c.d").is_empty());
    }

    #[test]
    fn does_not_flag_newline_after_using() {
        assert!(
            run("select * from a JOIN b USING\n(x)").is_empty(),
            "newline between USING and ( should be acceptable"
        );
    }

    #[test]
    fn flags_cte_newline_after_as() {
        let sql = "WITH a AS\n(\n  select 1\n)\nselect * from a";
        let issues = run(sql);
        assert!(!issues.is_empty(), "should flag AS + newline + (");
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "WITH a AS (\n  select 1\n)\nselect * from a");
    }

    #[test]
    fn flags_cte_newline_and_spaces_after_as() {
        let sql = "WITH a AS\n\n\n    (\n  select 1\n)\nselect * from a";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_all_issue_autofixes(sql, &issues);
        assert_eq!(fixed, "WITH a AS (\n  select 1\n)\nselect * from a");
    }

    #[test]
    fn does_not_flag_comment_after_as() {
        // When there's a comment between AS and (, it should pass
        assert!(
            run("WITH\na AS -- comment\n(\nselect 1\n)\nselect * from a").is_empty(),
            "comment between AS and ( should be acceptable"
        );
    }

    #[test]
    fn insert_into_table_paren_allows_space() {
        // Space before ( in INSERT INTO table ( should be fine.
        let issues = run("INSERT INTO metrics.cold_start_daily (\n    workspace_id\n) SELECT 1");
        let lt01 = issues
            .iter()
            .filter(|i| i.code == "LT01")
            .collect::<Vec<_>>();
        assert!(
            lt01.is_empty(),
            "INSERT INTO table ( should not flag LT01, got: {lt01:?}"
        );
    }

    #[test]
    fn insert_into_table_paren_with_cte() {
        // CTE + INSERT INTO: both parsed-statement and fallback paths.
        let sql = "WITH starts AS (\n    SELECT 1\n)\nINSERT INTO metrics.cold_start_daily (\n    workspace_id\n) SELECT workspace_id FROM starts";
        let issues = run_with_dialect(sql, Dialect::Postgres);
        let lt01 = issues
            .iter()
            .filter(|i| i.code == "LT01")
            .collect::<Vec<_>>();
        assert!(
            lt01.is_empty(),
            "INSERT INTO table ( with CTE should not flag LT01, got: {lt01:?}"
        );
    }

    #[test]
    fn insert_into_table_paren_on_conflict() {
        // Regression: CTE + INSERT INTO + ON CONFLICT via statementless path.
        let sql = "\
WITH cte AS (
    SELECT workspace_id
    FROM ledger.query_history
    WHERE start_time >= $1
)

INSERT INTO metrics.cold_start_daily (
    workspace_id
)
SELECT workspace_id
FROM cte
ON CONFLICT (workspace_id) DO UPDATE
    SET workspace_id = excluded.workspace_id";
        let issues = run_statementless_with_dialect(sql, Dialect::Postgres);
        let lt01 = issues
            .iter()
            .filter(|i| i.code == "LT01")
            .collect::<Vec<_>>();
        assert!(
            lt01.is_empty(),
            "INSERT INTO table ( with ON CONFLICT should not flag LT01, got: {lt01:?}"
        );
    }
}
