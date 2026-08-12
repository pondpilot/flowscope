//! LINT_LT_009: Layout select targets.
//!
//! SQLFluff LT09 parity: enforce select-target line layout for single-target
//! and multi-target SELECT clauses, with wildcard-policy behavior.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::linter::rules::semantic_helpers::visit_selects_in_statement;
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::{SelectItem, Statement};
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Location, Span, Token, TokenWithSpan, Tokenizer};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WildcardPolicy {
    Single,
    Multiple,
}

impl WildcardPolicy {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_LT_009, "wildcard_policy")
            .unwrap_or("single")
            .to_ascii_lowercase()
            .as_str()
        {
            "multiple" | "multi" | "allow_multiple" => Self::Multiple,
            _ => Self::Single,
        }
    }
}

pub struct LayoutSelectTargets {
    wildcard_policy: WildcardPolicy,
}

impl LayoutSelectTargets {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            wildcard_policy: WildcardPolicy::from_config(config),
        }
    }
}

impl Default for LayoutSelectTargets {
    fn default() -> Self {
        Self {
            wildcard_policy: WildcardPolicy::Single,
        }
    }
}

impl BuiltinLintRule for LayoutSelectTargets {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_009
    }

    fn name(&self) -> &'static str {
        "Layout select targets"
    }

    fn description(&self) -> &'static str {
        "Select targets should be on a new line unless there is only one select target."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        lt09_violation_spans(statement, ctx, self.wildcard_policy)
            .into_iter()
            .map(|((start, end), fix_span)| {
                let mut issue = Issue::info(
                    issue_codes::LINT_LT_009,
                    "Select targets should be on a new line unless there is only one target.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end));
                if let Some(fix_edits) = fix_span {
                    let edits: Vec<IssuePatchEdit> = fix_edits
                        .into_iter()
                        .map(|(fix_start, fix_end, replacement)| {
                            IssuePatchEdit::new(
                                ctx.span_from_statement_offset(fix_start, fix_end),
                                replacement,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AstSelectSpec {
    target_count: usize,
    has_wildcard: bool,
}

#[derive(Clone, Debug)]
struct SelectClauseLayout {
    select_idx: usize,
    from_idx: Option<usize>,
    target_ranges: Vec<(usize, usize)>,
}

type Lt09Span = (usize, usize);
type Lt09AutofixEdits = Vec<(usize, usize, String)>;
type Lt09Violation = (Lt09Span, Option<Lt09AutofixEdits>);

fn lt09_violation_spans(
    statement: &Statement,
    ctx: &RuleContext,
    wildcard_policy: WildcardPolicy,
) -> Vec<Lt09Violation> {
    let sql = ctx.statement_sql();
    let tokens = tokenized_for_context(ctx).or_else(|| tokenized(sql, ctx.dialect()));
    let Some(tokens) = tokens else {
        return Vec::new();
    };

    let ast_specs = collect_ast_select_specs(statement);
    let layouts = collect_select_clause_layouts(&tokens);
    let mut spans = Vec::new();

    for (idx, layout) in layouts.iter().enumerate() {
        if layout.target_ranges.is_empty() {
            continue;
        }

        let token_target_count = layout.target_ranges.len();
        let token_single_wildcard =
            token_target_count == 1 && target_range_is_wildcard(&tokens, layout.target_ranges[0]);

        let mut effective_target_count = token_target_count;
        let mut has_wildcard = token_single_wildcard;
        if let Some(spec) = ast_specs.get(idx) {
            if spec.target_count == token_target_count {
                effective_target_count = spec.target_count;
                has_wildcard = spec.has_wildcard;
            } else if token_target_count == 1 {
                has_wildcard = spec.has_wildcard || token_single_wildcard;
            }
        }

        let is_single_target = effective_target_count == 1
            && (!has_wildcard || matches!(wildcard_policy, WildcardPolicy::Single));

        let violation = if is_single_target {
            single_target_layout_violation(layout, &tokens)
        } else {
            multiple_target_layout_violation(layout, &tokens)
        };

        if !violation {
            continue;
        }

        let token = &tokens[layout.select_idx];
        let Some((start, end)) = token_with_span_offsets(sql, token) else {
            continue;
        };
        let fix_span = if is_single_target {
            safe_single_target_collapse_span(sql, &tokens, layout)
        } else {
            safe_multi_target_reflow_span(sql, &tokens, layout)
                .or_else(|| safe_from_newline_fix_span(sql, &tokens, layout))
        };

        spans.push(((start, end), fix_span));
    }

    spans
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

fn collect_ast_select_specs(statement: &Statement) -> Vec<AstSelectSpec> {
    let mut specs = Vec::new();
    visit_selects_in_statement(statement, &mut |select| {
        let has_wildcard = select.projection.iter().any(|item| {
            matches!(
                item,
                SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _)
            )
        });
        specs.push(AstSelectSpec {
            target_count: select.projection.len(),
            has_wildcard,
        });
    });
    specs
}

fn collect_select_clause_layouts(tokens: &[TokenWithSpan]) -> Vec<SelectClauseLayout> {
    let mut depth = 0usize;
    let mut layouts = Vec::new();

    for (idx, token) in tokens.iter().enumerate() {
        if is_select_keyword(&token.token) {
            let (clause_end, from_idx) = find_select_clause_end(tokens, idx, depth);
            if let Some(first_target_idx) = find_first_target_idx(tokens, idx + 1, clause_end) {
                let target_ranges =
                    split_target_ranges(tokens, first_target_idx, clause_end, depth);
                layouts.push(SelectClauseLayout {
                    select_idx: idx,
                    from_idx,
                    target_ranges,
                });
            } else {
                layouts.push(SelectClauseLayout {
                    select_idx: idx,
                    from_idx,
                    target_ranges: Vec::new(),
                });
            }
        }

        match token.token {
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            _ => {}
        }
    }

    layouts
}

fn is_select_keyword(token: &Token) -> bool {
    matches!(token, Token::Word(word) if word.keyword == Keyword::SELECT)
}

fn is_select_modifier_keyword(keyword: Keyword) -> bool {
    matches!(keyword, Keyword::DISTINCT | Keyword::ALL)
}

fn is_select_clause_boundary_keyword(keyword: Keyword) -> bool {
    matches!(
        keyword,
        Keyword::WHERE
            | Keyword::GROUP
            | Keyword::HAVING
            | Keyword::QUALIFY
            | Keyword::ORDER
            | Keyword::LIMIT
            | Keyword::FETCH
            | Keyword::UNION
            | Keyword::EXCEPT
            | Keyword::INTERSECT
            | Keyword::WINDOW
            | Keyword::INTO
            | Keyword::PREWHERE
            | Keyword::CLUSTER
            | Keyword::DISTRIBUTE
            | Keyword::SORT
            | Keyword::CONNECT
    )
}

fn find_select_clause_end(
    tokens: &[TokenWithSpan],
    select_idx: usize,
    select_depth: usize,
) -> (usize, Option<usize>) {
    let mut depth = select_depth;
    for (idx, token) in tokens.iter().enumerate().skip(select_idx + 1) {
        match &token.token {
            Token::LParen => depth += 1,
            Token::RParen => {
                if depth == select_depth {
                    return (idx, None);
                }
                depth = depth.saturating_sub(1);
            }
            Token::SemiColon if depth == select_depth => return (idx, None),
            Token::Word(word) if depth == select_depth => {
                if word.keyword == Keyword::FROM {
                    return (idx, Some(idx));
                }
                if is_select_clause_boundary_keyword(word.keyword) {
                    return (idx, None);
                }
            }
            _ => {}
        }
    }

    (tokens.len(), None)
}

fn is_ignorable_layout_token(token: &Token) -> bool {
    matches!(token, Token::Whitespace(_))
}

fn find_first_target_idx(tokens: &[TokenWithSpan], start: usize, end: usize) -> Option<usize> {
    let mut i = start;
    while i < end {
        let token = &tokens[i];
        match &token.token {
            t if is_ignorable_layout_token(t) => {}
            Token::Word(word) if is_select_modifier_keyword(word.keyword) => {
                // PostgreSQL DISTINCT ON (...) — skip past the parenthesized list.
                if word.keyword == Keyword::DISTINCT {
                    if let Some(on_idx) = skip_distinct_on_clause(tokens, i + 1, end) {
                        i = on_idx;
                    }
                }
            }
            _ => return Some(i),
        }
        i += 1;
    }
    None
}

/// If the tokens after DISTINCT start with `ON (...)`, return the index of
/// the closing `)` so the caller can skip the entire DISTINCT ON clause.
fn skip_distinct_on_clause(tokens: &[TokenWithSpan], start: usize, end: usize) -> Option<usize> {
    let mut i = start;
    // Skip whitespace/comments to find ON.
    while i < end {
        if is_ignorable_layout_token(&tokens[i].token) {
            i += 1;
            continue;
        }
        break;
    }
    if i >= end {
        return None;
    }
    let Token::Word(word) = &tokens[i].token else {
        return None;
    };
    if word.keyword != Keyword::ON {
        return None;
    }
    i += 1;
    // Skip whitespace/comments to find (.
    while i < end {
        if is_ignorable_layout_token(&tokens[i].token) {
            i += 1;
            continue;
        }
        break;
    }
    if i >= end || !matches!(tokens[i].token, Token::LParen) {
        return None;
    }
    // Skip past the matching ).
    let mut depth = 1usize;
    i += 1;
    while i < end && depth > 0 {
        match tokens[i].token {
            Token::LParen => depth += 1,
            Token::RParen => depth -= 1,
            _ => {}
        }
        if depth > 0 {
            i += 1;
        }
    }
    if depth == 0 {
        Some(i)
    } else {
        None
    }
}

fn split_target_ranges(
    tokens: &[TokenWithSpan],
    start: usize,
    end: usize,
    select_depth: usize,
) -> Vec<(usize, usize)> {
    let mut depth = select_depth;
    let mut ranges = Vec::new();
    let mut range_start = start;

    for (idx, token) in tokens.iter().enumerate().take(end).skip(start) {
        match token.token {
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            Token::Comma if depth == select_depth => {
                if let Some(trimmed) = trim_target_range(tokens, range_start, idx) {
                    ranges.push(trimmed);
                }
                range_start = idx + 1;
            }
            _ => {}
        }
    }

    if let Some(trimmed) = trim_target_range(tokens, range_start, end) {
        ranges.push(trimmed);
    }

    ranges
}

fn trim_target_range(
    tokens: &[TokenWithSpan],
    mut start: usize,
    mut end: usize,
) -> Option<(usize, usize)> {
    while start < end && is_ignorable_layout_token(&tokens[start].token) {
        start += 1;
    }

    while start < end && is_ignorable_layout_token(&tokens[end - 1].token) {
        end -= 1;
    }

    (start < end).then_some((start, end))
}

fn target_range_is_wildcard(tokens: &[TokenWithSpan], range: (usize, usize)) -> bool {
    let (start, end) = range;
    let code_tokens: Vec<&Token> = tokens[start..end]
        .iter()
        .map(|token| &token.token)
        .filter(|token| !is_ignorable_layout_token(token))
        .collect();

    if !matches!(code_tokens.last(), Some(Token::Mul)) {
        return false;
    }

    if code_tokens.len() == 1 {
        return true;
    }

    code_tokens[..code_tokens.len() - 1]
        .iter()
        .enumerate()
        .all(|(idx, token)| {
            if idx % 2 == 0 {
                matches!(token, Token::Word(_))
            } else {
                matches!(token, Token::Period)
            }
        })
}

fn last_code_line_before(tokens: &[TokenWithSpan], start: usize, end: usize) -> Option<u64> {
    let mut line = None;
    for token in tokens.iter().take(end).skip(start) {
        if is_ignorable_layout_token(&token.token) || matches!(token.token, Token::Comma) {
            continue;
        }
        line = Some(token.span.end.line);
    }
    line
}

fn single_target_layout_violation(layout: &SelectClauseLayout, tokens: &[TokenWithSpan]) -> bool {
    let Some((target_start, target_end)) = layout.target_ranges.first().copied() else {
        return false;
    };

    let select_line = tokens[layout.select_idx].span.start.line;
    let target_start_line = tokens[target_start].span.start.line;
    if target_start_line <= select_line {
        return false;
    }

    let target_end_line = tokens[target_end - 1].span.end.line;
    target_end_line == target_start_line
}

fn multiple_target_layout_violation(layout: &SelectClauseLayout, tokens: &[TokenWithSpan]) -> bool {
    for (idx, (target_start, _target_end)) in layout.target_ranges.iter().enumerate() {
        let target_line = tokens[*target_start].span.start.line;
        if last_code_line_before(tokens, layout.select_idx, *target_start)
            .is_some_and(|prev_line| prev_line == target_line)
        {
            return true;
        }

        if idx + 1 == layout.target_ranges.len()
            && layout
                .from_idx
                .is_some_and(|from_idx| tokens[from_idx].span.start.line == target_line)
        {
            return true;
        }
    }

    false
}

/// For single-target violations, collapse `SELECT\n  target` → `SELECT target`
/// by replacing the whitespace gap with a single space.
/// For single-target violations, collapse `SELECT\n  target` → `SELECT target`
/// by replacing the whitespace gap with a single space.
fn safe_single_target_collapse_span(
    sql: &str,
    tokens: &[TokenWithSpan],
    layout: &SelectClauseLayout,
) -> Option<Lt09AutofixEdits> {
    let (target_start_idx, target_end_idx) = layout.target_ranges.first().copied()?;

    // Find the last token before the target (SELECT keyword or modifier like DISTINCT).
    let last_pre_target_idx = (layout.select_idx..target_start_idx)
        .rev()
        .find(|&idx| !is_ignorable_layout_token(&tokens[idx].token))?;

    let (_, gap_start) = token_with_span_offsets(sql, &tokens[last_pre_target_idx])?;
    let (gap_end, _) = token_with_span_offsets(sql, &tokens[target_start_idx])?;
    if gap_start > gap_end || gap_end > sql.len() {
        return None;
    }

    let gap = &sql[gap_start..gap_end];
    let (_, target_text_end) = token_with_span_offsets(sql, &tokens[target_end_idx - 1])?;
    let target_line = tokens[target_start_idx].span.start.line;

    // Check for comments in the gap (between SELECT/modifier and target).
    let has_gap_comments = (last_pre_target_idx + 1..target_start_idx)
        .any(|idx| comment_token_text(&tokens[idx]).is_some());

    // Check for trailing inline comments after the target on the same line.
    let has_trailing_comments = tokens
        .iter()
        .skip(target_end_idx)
        .take_while(|t| t.span.start.line == target_line)
        .any(|t| comment_token_text(t).is_some());

    let gap_is_whitespace_only = gap.chars().all(char::is_whitespace) && gap.contains('\n');

    // Simple case: no comments at all — collapse to single space.
    if !has_gap_comments && !has_trailing_comments && gap_is_whitespace_only {
        return Some(vec![(gap_start, gap_end, " ".to_string())]);
    }

    // Comment-aware collapse: move the target text onto the SELECT line and
    // relocate adjacent comments. Edits must avoid spanning comment protected
    // ranges — we produce multiple surgical edits around comments instead.
    let target_text = sql[gap_end..target_text_end].to_string();
    let target_indent = detect_indent(sql, gap_end);

    // Collect comment token indices.
    let gap_comment_indices: Vec<usize> = tokens
        .iter()
        .enumerate()
        .take(target_start_idx)
        .skip(last_pre_target_idx + 1)
        .filter_map(|(idx, token)| comment_token_text(token).map(|_| idx))
        .collect();
    let mut trailing_comment_indices: Vec<usize> = Vec::new();
    for (offset, t) in tokens.iter().enumerate().skip(target_end_idx) {
        if t.span.start.line != target_line {
            break;
        }
        if comment_token_text(t).is_some() {
            trailing_comment_indices.push(offset);
        }
    }

    let has_subsequent_content = layout.from_idx.is_some()
        || tokens.iter().skip(target_end_idx).any(|t| {
            t.span.start.line > target_line
                && !is_ignorable_layout_token(&t.token)
                && comment_token_text(t).is_none()
        });

    // Determine whether the two-edit strategy (split around comments) would
    // overlap. The two-edit strategy uses:
    //   Edit 1: (gap_start, first_comment_start, ...)
    //   Edit 2: (target_line_nl, target_text_end, ...)
    // These overlap when first_comment_start > target_line_nl, meaning the
    // comment is between the newline and the target on the same line.
    let target_line_nl = sql[..gap_end].rfind('\n');
    let first_gap_comment_start = gap_comment_indices
        .first()
        .and_then(|&idx| token_with_span_offsets(sql, &tokens[idx]).map(|(s, _)| s));
    let two_edit_would_overlap = target_line_nl
        .zip(first_gap_comment_start)
        .is_some_and(|(nl, cs)| cs > nl);

    let mut edits = Vec::new();

    if !gap_comment_indices.is_empty()
        && trailing_comment_indices.is_empty()
        && !two_edit_would_overlap
        && has_subsequent_content
    {
        // Comments on separate line(s) before target, with FROM after.
        // Two non-overlapping edits.
        let first_comment_idx = gap_comment_indices[0];
        let (first_comment_start, _) = token_with_span_offsets(sql, &tokens[first_comment_idx])?;
        edits.push((
            gap_start,
            first_comment_start,
            format!(" {target_text}\n{target_indent}"),
        ));
        let nl = target_line_nl?;
        edits.push((nl, target_text_end, String::new()));
    } else if !gap_comment_indices.is_empty()
        && trailing_comment_indices.is_empty()
        && (two_edit_would_overlap || !has_subsequent_content)
    {
        // Comments and target on same line after SELECT, or no FROM clause.
        // Use surgical edits around comment protected ranges.
        let first_comment_idx = gap_comment_indices[0];
        let last_comment_idx = *gap_comment_indices.last().unwrap();
        let (first_comment_start, _) = token_with_span_offsets(sql, &tokens[first_comment_idx])?;
        let (_, last_comment_end) = token_with_span_offsets(sql, &tokens[last_comment_idx])?;

        if has_subsequent_content {
            // Place target before comments, comments on their own line.
            edits.push((
                gap_start,
                first_comment_start,
                format!(" {target_text}\n{target_indent}"),
            ));
            edits.push((last_comment_end, target_text_end, String::new()));
        } else {
            // No FROM — place target and comments on the same line.
            edits.push((gap_start, first_comment_start, format!(" {target_text} ")));
            edits.push((last_comment_end, target_text_end, String::new()));
        }
    } else if gap_comment_indices.is_empty() && !trailing_comment_indices.is_empty() {
        // Comments trail the target on the same line (e.g., `1-- comment`).
        // Edit 1: Collapse the gap to a space.
        edits.push((gap_start, gap_end, " ".to_string()));

        // Edit 2: Insert newline+indent before the trailing comment by
        // replacing the last byte of the target with itself + \n + indent.
        // This avoids a zero-width insert at the comment's protected range.
        if target_text_end > 0 {
            let anchor = target_text_end - 1;
            let anchor_char = &sql[anchor..target_text_end];
            edits.push((
                anchor,
                target_text_end,
                format!("{anchor_char}\n{target_indent}"),
            ));
        } else {
            return None;
        }
    } else if !gap_comment_indices.is_empty() && !trailing_comment_indices.is_empty() {
        // Comments both before and after target. Produce surgical edits that
        // avoid spanning any comment's protected range.
        let first_comment_idx = gap_comment_indices[0];
        let (first_comment_start, _) = token_with_span_offsets(sql, &tokens[first_comment_idx])?;

        // Edit 1: Replace gap before first comment with target.
        edits.push((
            gap_start,
            first_comment_start,
            format!(" {target_text}\n{target_indent}"),
        ));

        // Edit 2: Remove the target and whitespace between the last gap comment
        // and the first trailing comment. This avoids spanning the trailing
        // comment's protected range.
        let last_gap_comment_idx = *gap_comment_indices.last().unwrap();
        let (_, last_gap_comment_end) =
            token_with_span_offsets(sql, &tokens[last_gap_comment_idx])?;
        let first_trailing_idx = trailing_comment_indices[0];
        let (first_trailing_start, _) = token_with_span_offsets(sql, &tokens[first_trailing_idx])?;

        // Replace the region from after the last gap comment to the start of
        // the first trailing comment with just the indent (so the trailing
        // comment stays on its own line).
        edits.push((
            last_gap_comment_end,
            first_trailing_start,
            target_indent.to_string(),
        ));
    } else {
        return None;
    }

    Some(edits)
}

fn safe_from_newline_fix_span(
    sql: &str,
    tokens: &[TokenWithSpan],
    layout: &SelectClauseLayout,
) -> Option<Lt09AutofixEdits> {
    let from_idx = layout.from_idx?;
    if !only_from_shares_last_target_line_violation(layout, tokens) {
        return None;
    }

    let (_, last_target_end_idx) = *layout.target_ranges.last()?;
    if last_target_end_idx == 0 {
        return None;
    }
    let last_token_idx = last_target_end_idx - 1;

    let (_, gap_start) = token_with_span_offsets(sql, &tokens[last_token_idx])?;
    let (gap_end, _) = token_with_span_offsets(sql, &tokens[from_idx])?;
    if gap_start > gap_end || gap_end > sql.len() {
        return None;
    }

    let gap = &sql[gap_start..gap_end];
    if gap.chars().all(char::is_whitespace) && !gap.contains('\n') && !gap.contains('\r') {
        Some(vec![(gap_start, gap_end, "\n".to_string())])
    } else {
        None
    }
}

fn safe_multi_target_reflow_span(
    sql: &str,
    tokens: &[TokenWithSpan],
    layout: &SelectClauseLayout,
) -> Option<Lt09AutofixEdits> {
    if layout.target_ranges.len() < 2 {
        return None;
    }

    let from_idx = layout.from_idx?;

    let first_target_idx = layout
        .target_ranges
        .first()
        .map(|(start_idx, _)| *start_idx)?;
    let last_pre_target_idx = (layout.select_idx..first_target_idx)
        .rev()
        .find(|&idx| !is_ignorable_layout_token(&tokens[idx].token))?;

    if (last_pre_target_idx + 1..from_idx).any(|idx| comment_token_text(&tokens[idx]).is_some()) {
        return None;
    }

    let (_, replace_start) = token_with_span_offsets(sql, &tokens[last_pre_target_idx])?;
    let (replace_end, _) = token_with_span_offsets(sql, &tokens[from_idx])?;
    if replace_start > replace_end || replace_end > sql.len() {
        return None;
    }

    let (select_start, _) = token_with_span_offsets(sql, &tokens[layout.select_idx])?;
    let base_indent = detect_indent(sql, select_start);
    let target_indent = format!("{base_indent}    ");

    let mut replacement = String::from('\n');
    for (idx, (target_start_idx, target_end_idx)) in layout.target_ranges.iter().enumerate() {
        let (target_start, _) = token_with_span_offsets(sql, &tokens[*target_start_idx])?;
        let (_, target_end) = token_with_span_offsets(sql, &tokens[target_end_idx - 1])?;
        if target_start > target_end || target_end > sql.len() {
            return None;
        }

        replacement.push_str(&target_indent);
        replacement.push_str(&sql[target_start..target_end]);
        if idx + 1 < layout.target_ranges.len() {
            replacement.push(',');
        }
        replacement.push('\n');
    }
    replacement.push_str(&base_indent);

    Some(vec![(replace_start, replace_end, replacement)])
}

fn only_from_shares_last_target_line_violation(
    layout: &SelectClauseLayout,
    tokens: &[TokenWithSpan],
) -> bool {
    let Some(from_idx) = layout.from_idx else {
        return false;
    };
    let Some((last_start_idx, _)) = layout.target_ranges.last().copied() else {
        return false;
    };

    let last_target_line = tokens[last_start_idx].span.start.line;
    if tokens[from_idx].span.start.line != last_target_line {
        return false;
    }

    for (target_start, _) in &layout.target_ranges {
        let target_line = tokens[*target_start].span.start.line;
        if last_code_line_before(tokens, layout.select_idx, *target_start)
            .is_some_and(|prev_line| prev_line == target_line)
        {
            return false;
        }
    }

    true
}

/// Extract comment text from a token, if it is a comment.
/// Trailing newlines are stripped because the caller handles line breaks.
fn comment_token_text(token: &TokenWithSpan) -> Option<String> {
    use sqlparser::tokenizer::Whitespace;
    match &token.token {
        Token::Whitespace(Whitespace::SingleLineComment { prefix, comment }) => {
            let text = format!("{prefix}{comment}");
            Some(
                text.trim_end_matches('\n')
                    .trim_end_matches('\r')
                    .to_string(),
            )
        }
        Token::Whitespace(Whitespace::MultiLineComment(content)) => Some(format!("/*{content}*/")),
        _ => None,
    }
}

/// Detect the indentation prefix on the line where `offset` points.
fn detect_indent(sql: &str, offset: usize) -> String {
    // Walk backwards from offset to find the start of the line.
    let line_start = sql[..offset].rfind('\n').map(|pos| pos + 1).unwrap_or(0);
    // Collect leading whitespace from that line.
    sql[line_start..]
        .chars()
        .take_while(|ch| ch.is_whitespace() && *ch != '\n')
        .collect()
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

    fn run_with_rule(sql: &str, rule: &LayoutSelectTargets) -> Vec<Issue> {
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
        run_with_rule(sql, &LayoutSelectTargets::default())
    }

    fn run_with_wildcard_policy(sql: &str, policy: &str) -> Vec<Issue> {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.select_targets".to_string(),
                serde_json::json!({"wildcard_policy": policy}),
            )]),
        };
        let rule = LayoutSelectTargets::from_config(&config);
        run_with_rule(sql, &rule)
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
    fn flags_multiple_targets_on_same_select_line() {
        assert!(!run("SELECT a,b,c,d,e FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_single_target() {
        assert!(run("SELECT a FROM t").is_empty());
    }

    #[test]
    fn flags_each_select_line_with_multiple_targets() {
        let issues = run("SELECT a, b FROM t UNION ALL SELECT c, d FROM t");
        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.code == issue_codes::LINT_LT_009)
                .count(),
            2,
        );
    }

    #[test]
    fn does_not_flag_select_word_inside_single_quoted_string() {
        assert!(run("SELECT 'SELECT a, b' AS txt").is_empty());
    }

    #[test]
    fn multiple_wildcard_policy_flags_single_wildcard_target() {
        let issues = run_with_wildcard_policy("SELECT * FROM t", "multiple");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_009);
    }

    #[test]
    fn wildcard_policy_alias_allow_multiple_is_supported() {
        let issues = run_with_wildcard_policy("SELECT * FROM t", "allow_multiple");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn multiple_wildcard_policy_does_not_treat_multiplication_as_wildcard() {
        let issues = run_with_wildcard_policy("SELECT a * b FROM t", "multiple");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_single_target_on_new_line_after_select() {
        let sql = "SELECT\n  a\nFROM x";
        assert!(!run(sql).is_empty());
    }

    #[test]
    fn flags_single_target_when_select_followed_by_comment_line() {
        let sql = "SELECT -- some comment\na";
        assert!(!run(sql).is_empty());
    }

    #[test]
    fn does_not_flag_single_multiline_target() {
        let sql = "SELECT\n    SUM(\n        1 + 2\n    ) AS col\nFROM t";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn flags_last_multi_target_sharing_line_with_from() {
        let sql = "select\n  a,\n  b,\n  c from x";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "select\n    a,\n    b,\n    c\nfrom x");
    }

    #[test]
    fn dense_multi_target_layout_violation_autofixes_to_one_target_per_line() {
        let sql = "SELECT a,b,c,d,e FROM t";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "SELECT\n    a,\n    b,\n    c,\n    d,\n    e\nFROM t"
        );
    }

    #[test]
    fn wrapped_multi_target_layout_autofixes_when_from_is_on_next_line() {
        let sql = "SELECT a, b, c,\n  d\nFROM t";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT\n    a,\n    b,\n    c,\n    d\nFROM t");
    }

    #[test]
    fn flags_in_cte_single_target_newline_case() {
        let sql = "WITH cte1 AS (\n  SELECT\n    c1 AS c\n  FROM t\n)\nSELECT 1 FROM cte1";
        assert!(!run(sql).is_empty());
    }

    #[test]
    fn flags_in_create_view_single_target_newline_case() {
        let sql = "CREATE VIEW a AS\nSELECT\n    c\nFROM table1";
        assert!(!run(sql).is_empty());
    }

    #[test]
    fn multiple_wildcard_policy_flags_star_with_from_on_same_line() {
        let sql = "select\n    * from x";
        assert!(!run_with_wildcard_policy(sql, "multiple").is_empty());
    }

    #[test]
    fn multiple_wildcard_policy_allows_star_on_own_line() {
        let sql = "select\n    *\nfrom x";
        assert!(run_with_wildcard_policy(sql, "multiple").is_empty());
    }

    #[test]
    fn single_target_autofix_collapses_to_select_line() {
        let sql = "SELECT\n  a\nFROM x";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT a\nFROM x");
    }

    #[test]
    fn single_target_autofix_with_distinct() {
        let sql = "SELECT DISTINCT\n  a\nFROM x";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT DISTINCT a\nFROM x");
    }

    #[test]
    fn allows_leading_comma_layout_for_multiple_targets() {
        let sql = "select\n    a\n    , b\n    , c";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn single_target_with_comment_before_collapses() {
        let sql = "SELECT\n    -- This is the user's ID.\n    user_id\nFROM\n    safe_user";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "SELECT user_id\n    -- This is the user's ID.\nFROM\n    safe_user"
        );
    }

    #[test]
    fn single_target_with_block_comment_before_collapses_inline() {
        // No FROM clause — comment is placed inline after target.
        let sql = "SELECT\n /* test */  10000000";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT 10000000 /* test */");
    }

    #[test]
    fn single_target_with_trailing_inline_comment_collapses() {
        // Trailing comment after target — comment moves to its own indented line.
        let sql = "SELECT\n  1-- this is a comment\nFROM\n  my_table";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT 1\n  -- this is a comment\nFROM\n  my_table");
    }

    #[test]
    fn single_target_with_block_comment_before_on_same_line_collapses() {
        // Block comment before target on same line — comment moves below target.
        let sql = "SELECT\n  /* comment before */ 1\nFROM\n  my_table";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT 1\n  /* comment before */\nFROM\n  my_table");
    }

    #[test]
    fn does_not_flag_distinct_on_with_targets_on_own_lines() {
        // PostgreSQL DISTINCT ON (...) — targets each on own line = no violation.
        let sql = "SELECT DISTINCT ON (a.id)\n    a.id,\n    a.name\nFROM a";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn does_not_flag_distinct_on_single_target_inline() {
        // DISTINCT ON with a single target on the same line as SELECT.
        let sql = "SELECT DISTINCT ON (a.id) a.name FROM a";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn single_target_with_multiple_mixed_comments_collapses() {
        // Gap comment on separate line + trailing inline comment.
        let sql = "SELECT\n  -- previous comment\n  1 -- this is a comment\nFROM\n  my_table";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "SELECT 1\n  -- previous comment\n  -- this is a comment\nFROM\n  my_table"
        );
    }
}
