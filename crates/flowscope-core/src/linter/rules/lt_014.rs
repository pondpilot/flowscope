//! LINT_LT_014: Layout keyword newline.
//!
//! SQLFluff LT14 parity: detect clause keywords that violate
//! `keyword_line_position` policy (`leading`, `alone`, `trailing`, `none`).
//! Configs are per-clause-type via `layout.type.<clause_type>.keyword_line_position`.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Location, Span, Token, TokenWithSpan, Tokenizer, Whitespace};

// ---------------------------------------------------------------------------
// Config types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeywordLinePosition {
    Leading,
    Alone,
    Trailing,
    None,
}

impl KeywordLinePosition {
    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "leading" => Some(Self::Leading),
            "alone" => Some(Self::Alone),
            "trailing" => Some(Self::Trailing),
            "none" => Some(Self::None),
            _ => Option::None,
        }
    }
}

/// Holds per-clause-type keyword_line_position configs.
#[derive(Clone, Debug, Default)]
struct ClauseConfigs {
    from_clause: Option<KeywordLinePosition>,
    where_clause: Option<KeywordLinePosition>,
    join_clause: Option<KeywordLinePosition>,
    join_on_condition: Option<KeywordLinePosition>,
    orderby_clause: Option<KeywordLinePosition>,
    orderby_exclusions: Vec<String>,
    groupby_clause: Option<KeywordLinePosition>,
    partitionby_clause: Option<KeywordLinePosition>,
    qualify_clause: Option<KeywordLinePosition>,
    select_clause: Option<KeywordLinePosition>,
    data_type: Option<KeywordLinePosition>,
    having_clause: Option<KeywordLinePosition>,
    limit_clause: Option<KeywordLinePosition>,
}

impl ClauseConfigs {
    fn from_lint_config(config: &LintConfig) -> Self {
        let mut out = Self::default();

        let obj = config.rule_config_object(issue_codes::LINT_LT_014);
        let Some(obj) = obj else {
            return out;
        };

        fn read_clause(
            obj: &serde_json::Map<String, serde_json::Value>,
            key: &str,
        ) -> Option<KeywordLinePosition> {
            let clause_obj = obj.get(key)?.as_object()?;
            let pos_str = clause_obj.get("keyword_line_position")?.as_str()?;
            KeywordLinePosition::parse(pos_str)
        }

        fn read_exclusions(
            obj: &serde_json::Map<String, serde_json::Value>,
            key: &str,
        ) -> Vec<String> {
            let Some(clause_obj) = obj.get(key).and_then(|v| v.as_object()) else {
                return Vec::new();
            };
            let Some(excl) = clause_obj.get("keyword_line_position_exclusions") else {
                return Vec::new();
            };
            // Can be a string (single value) or null/None (meaning "no exclusions,
            // apply everywhere").
            if excl.is_null() {
                return vec!["__none__".to_string()];
            }
            if let Some(s) = excl.as_str() {
                if s.eq_ignore_ascii_case("None") {
                    return vec!["__none__".to_string()];
                }
                return s
                    .split(',')
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            Vec::new()
        }

        out.from_clause = read_clause(obj, "from_clause");
        out.where_clause = read_clause(obj, "where_clause");
        out.join_clause = read_clause(obj, "join_clause");
        out.join_on_condition = read_clause(obj, "join_on_condition");
        out.orderby_clause = read_clause(obj, "orderby_clause");
        out.orderby_exclusions = read_exclusions(obj, "orderby_clause");
        out.groupby_clause = read_clause(obj, "groupby_clause");
        out.partitionby_clause = read_clause(obj, "partitionby_clause");
        out.qualify_clause = read_clause(obj, "qualify_clause");
        out.select_clause = read_clause(obj, "select_clause");
        out.data_type = read_clause(obj, "data_type");
        out.having_clause = read_clause(obj, "having_clause");
        out.limit_clause = read_clause(obj, "limit_clause");

        out
    }

    /// Returns true if any clause type has a configured position.
    fn has_any_config(&self) -> bool {
        self.from_clause.is_some()
            || self.where_clause.is_some()
            || self.join_clause.is_some()
            || self.join_on_condition.is_some()
            || self.orderby_clause.is_some()
            || self.groupby_clause.is_some()
            || self.partitionby_clause.is_some()
            || self.qualify_clause.is_some()
            || self.select_clause.is_some()
            || self.data_type.is_some()
            || self.having_clause.is_some()
            || self.limit_clause.is_some()
    }
}

// ---------------------------------------------------------------------------
// Rule struct
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct LayoutKeywordNewline {
    clause_configs: ClauseConfigs,
}

impl LayoutKeywordNewline {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            clause_configs: ClauseConfigs::from_lint_config(config),
        }
    }
}

impl BuiltinLintRule for LayoutKeywordNewline {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_014
    }

    fn name(&self) -> &'static str {
        "Layout keyword newline"
    }

    fn description(&self) -> &'static str {
        "Keyword clauses should follow a standard for being before/after newlines."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let tokens = tokenized_for_context(ctx);
        let sql = ctx.statement_sql();

        if self.clause_configs.has_any_config() {
            return check_with_configs(sql, ctx, &self.clause_configs, tokens.as_deref());
        }

        // Fallback: legacy behaviour for no-config mode.
        let Some((keyword_start, keyword_end)) =
            keyword_newline_violation_span(sql, ctx.dialect(), tokens.as_deref())
        else {
            return Vec::new();
        };

        let keyword_span = ctx.span_from_statement_offset(keyword_start, keyword_end);
        let ws_start = sql[..keyword_start].trim_end().len();
        let replace_span = ctx.span_from_statement_offset(ws_start, keyword_start);
        vec![Issue::info(
            issue_codes::LINT_LT_014,
            "Major clauses should be consistently line-broken.",
        )
        .with_statement(ctx.statement_index)
        .with_span(keyword_span)
        .with_autofix_edits(
            IssueAutofixApplicability::Safe,
            vec![IssuePatchEdit::new(replace_span, "\n")],
        )]
    }
}

// ---------------------------------------------------------------------------
// Config-driven detection
// ---------------------------------------------------------------------------

/// A detected keyword occurrence with its clause type and byte range.
#[derive(Clone, Debug)]
struct KeywordOccurrence {
    /// Clause type (e.g., "from_clause", "join_clause").
    clause_type: &'static str,
    /// Byte offset of keyword start in the statement SQL.
    start: usize,
    /// Byte offset of keyword end in the statement SQL.
    end: usize,
    /// Whether there is content before the keyword on the same line.
    has_content_before: bool,
    /// Whether there is content after the keyword on the same line (before next newline).
    has_content_after: bool,
    /// Whether the keyword is the first token on its line (ignoring whitespace).
    is_first_on_line: bool,
    /// Whether this occurrence is inside a window function / aggregate context.
    in_window: bool,
    /// Whether this occurrence is inside an aggregate ORDER BY context.
    in_aggregate_order_by: bool,
}

fn check_with_configs(
    sql: &str,
    ctx: &RuleContext,
    configs: &ClauseConfigs,
    tokens: Option<&[TokenWithSpan]>,
) -> Vec<Issue> {
    let owned_tokens;
    let tokens = if let Some(tokens) = tokens {
        tokens
    } else {
        owned_tokens = match tokenized(sql, ctx.dialect()) {
            Some(t) => t,
            std::option::Option::None => return Vec::new(),
        };
        &owned_tokens
    };

    let occurrences = find_clause_keyword_occurrences(sql, tokens);

    let mut issues = Vec::new();

    for occ in &occurrences {
        let Some(position) = config_for_clause(configs, occ) else {
            continue;
        };

        if position == KeywordLinePosition::None {
            continue;
        }

        // Check for exclusions (ORDER BY in window functions, etc.)
        if occ.clause_type == "orderby_clause" && !configs.orderby_exclusions.is_empty() {
            let has_none_exclusion = configs.orderby_exclusions.iter().any(|e| e == "__none__");
            if !has_none_exclusion {
                // If exclusions are set, skip this occurrence if it's in an excluded context.
                let in_excluded = configs.orderby_exclusions.iter().any(|e| {
                    (e == "window_specification" && occ.in_window)
                        || (e == "aggregate_order_by" && occ.in_aggregate_order_by)
                });
                if in_excluded {
                    continue;
                }
            }
            // __none__ means "no exclusions" — apply the rule everywhere.
        }

        let violation = match position {
            KeywordLinePosition::Leading => {
                // Keyword should be the first non-whitespace on its line.
                !occ.is_first_on_line
            }
            KeywordLinePosition::Alone => {
                // Keyword should be on its own line: first on line AND no content after.
                !occ.is_first_on_line || occ.has_content_after
            }
            KeywordLinePosition::Trailing => {
                // Keyword should be at the end of a line (content before, newline after).
                !occ.has_content_before || occ.has_content_after
            }
            KeywordLinePosition::None => false,
        };

        if !violation {
            continue;
        }

        let keyword_span = ctx.span_from_statement_offset(occ.start, occ.end);
        let edits = build_autofix_edits(sql, ctx, occ, position);

        issues.push(
            Issue::info(
                issue_codes::LINT_LT_014,
                "Keyword clauses should follow a standard for being before/after newlines.",
            )
            .with_statement(ctx.statement_index)
            .with_span(keyword_span)
            .with_autofix_edits(IssueAutofixApplicability::Safe, edits),
        );
    }

    issues
}

fn config_for_clause(
    configs: &ClauseConfigs,
    occ: &KeywordOccurrence,
) -> Option<KeywordLinePosition> {
    match occ.clause_type {
        "from_clause" => configs.from_clause,
        "where_clause" => configs.where_clause,
        "join_clause" => configs.join_clause,
        "join_on_condition" => configs.join_on_condition,
        "orderby_clause" => configs.orderby_clause,
        "groupby_clause" => configs.groupby_clause,
        "partitionby_clause" => configs.partitionby_clause,
        "qualify_clause" => configs.qualify_clause,
        "select_clause" => configs.select_clause,
        "data_type" => configs.data_type,
        "having_clause" => configs.having_clause,
        "limit_clause" => configs.limit_clause,
        _ => Option::None,
    }
}

fn build_autofix_edits(
    sql: &str,
    ctx: &RuleContext,
    occ: &KeywordOccurrence,
    position: KeywordLinePosition,
) -> Vec<IssuePatchEdit> {
    match position {
        KeywordLinePosition::Leading => {
            // Insert newline before keyword (replace preceding whitespace).
            let ws_start = sql[..occ.start].trim_end().len();
            let replace_span = ctx.span_from_statement_offset(ws_start, occ.start);
            vec![IssuePatchEdit::new(replace_span, "\n")]
        }
        KeywordLinePosition::Alone => {
            let mut edits = Vec::new();

            // If keyword is not first on line, add newline before.
            if !occ.is_first_on_line {
                let ws_start = sql[..occ.start].trim_end().len();
                let replace_span = ctx.span_from_statement_offset(ws_start, occ.start);
                edits.push(IssuePatchEdit::new(replace_span, "\n"));
            }

            // If there is content after the keyword on the same line, add newline after.
            if occ.has_content_after {
                let after_keyword = &sql[occ.end..];
                let content_offset = after_keyword
                    .find(|c: char| c != ' ' && c != '\t')
                    .unwrap_or(0);
                let replace_start = occ.end;
                let replace_end = occ.end + content_offset;
                let replace_span = ctx.span_from_statement_offset(replace_start, replace_end);
                edits.push(IssuePatchEdit::new(replace_span, "\n"));
            }

            edits
        }
        KeywordLinePosition::Trailing => {
            // Insert newline before keyword content, so keyword ends up at end of previous line.
            // This is complex and rare; for now emit a newline after the keyword.
            let mut edits = Vec::new();

            // Put keyword at end of previous content (remove whitespace before, add space).
            if !occ.has_content_before {
                let ws_start = sql[..occ.start].trim_end().len();
                let replace_span = ctx.span_from_statement_offset(ws_start, occ.start);
                edits.push(IssuePatchEdit::new(replace_span, " "));
            }

            // Add newline after keyword.
            if occ.has_content_after {
                let after_keyword = &sql[occ.end..];
                let content_offset = after_keyword
                    .find(|c: char| c != ' ' && c != '\t')
                    .unwrap_or(0);
                let replace_start = occ.end;
                let replace_end = occ.end + content_offset;
                let replace_span = ctx.span_from_statement_offset(replace_start, replace_end);
                edits.push(IssuePatchEdit::new(replace_span, "\n"));
            }

            edits
        }
        KeywordLinePosition::None => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Keyword occurrence finding
// ---------------------------------------------------------------------------

fn find_clause_keyword_occurrences(sql: &str, tokens: &[TokenWithSpan]) -> Vec<KeywordOccurrence> {
    let significant: Vec<(usize, &TokenWithSpan)> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| !is_trivia_token(&t.token))
        .collect();

    let mut out = Vec::new();
    let mut paren_depth: i32 = 0;
    // Track window function context: depth at which OVER ( opened.
    let mut window_paren_depth: Option<i32> = Option::None;
    // Track whether we're inside a function call (for aggregate ORDER BY detection).
    let mut function_call_depths: Vec<i32> = Vec::new();

    let mut sig_idx = 0;
    while sig_idx < significant.len() {
        let (_, token) = significant[sig_idx];

        // Track parentheses depth.
        match &token.token {
            Token::LParen => {
                paren_depth += 1;
            }
            Token::RParen => {
                if window_paren_depth == Some(paren_depth) {
                    window_paren_depth = Option::None;
                }
                if function_call_depths.last() == Some(&paren_depth) {
                    function_call_depths.pop();
                }
                paren_depth -= 1;
            }
            _ => {}
        }

        let Token::Word(word) = &token.token else {
            sig_idx += 1;
            continue;
        };

        match word.keyword {
            Keyword::OVER => {
                // Look ahead for `(` to detect window function context.
                if let Some((_, next)) = significant.get(sig_idx + 1) {
                    if matches!(&next.token, Token::LParen) {
                        window_paren_depth = Some(paren_depth + 1);
                    }
                }
                sig_idx += 1;
            }
            Keyword::FROM => {
                if let Some(occ) = single_keyword_occurrence(
                    sql,
                    token,
                    "from_clause",
                    window_paren_depth.is_some(),
                    false,
                ) {
                    out.push(occ);
                }
                sig_idx += 1;
            }
            Keyword::WHERE => {
                if let Some(occ) = single_keyword_occurrence(
                    sql,
                    token,
                    "where_clause",
                    window_paren_depth.is_some(),
                    false,
                ) {
                    out.push(occ);
                }
                sig_idx += 1;
            }
            Keyword::ON => {
                // ON after JOIN — look back to see if it's a join_on_condition.
                let is_join_on = significant[..sig_idx].iter().rev().any(|(_, t)| {
                    if let Token::Word(w) = &t.token {
                        matches!(
                            w.keyword,
                            Keyword::JOIN
                                | Keyword::INNER
                                | Keyword::LEFT
                                | Keyword::RIGHT
                                | Keyword::FULL
                                | Keyword::CROSS
                                | Keyword::OUTER
                        )
                    } else {
                        false
                    }
                });
                if is_join_on {
                    if let Some(occ) = single_keyword_occurrence(
                        sql,
                        token,
                        "join_on_condition",
                        window_paren_depth.is_some(),
                        false,
                    ) {
                        out.push(occ);
                    }
                }
                sig_idx += 1;
            }
            Keyword::QUALIFY => {
                if let Some(occ) = single_keyword_occurrence(
                    sql,
                    token,
                    "qualify_clause",
                    window_paren_depth.is_some(),
                    false,
                ) {
                    out.push(occ);
                }
                sig_idx += 1;
            }
            Keyword::SELECT => {
                if let Some(occ) =
                    single_keyword_occurrence(sql, token, "select_clause", false, false)
                {
                    out.push(occ);
                }
                sig_idx += 1;
            }
            Keyword::HAVING => {
                if let Some(occ) =
                    single_keyword_occurrence(sql, token, "having_clause", false, false)
                {
                    out.push(occ);
                }
                sig_idx += 1;
            }
            Keyword::LIMIT => {
                if let Some(occ) =
                    single_keyword_occurrence(sql, token, "limit_clause", false, false)
                {
                    out.push(occ);
                }
                sig_idx += 1;
            }
            Keyword::JOIN => {
                // Standalone JOIN — check if there's a preceding modifier on the same line
                // (LEFT, RIGHT, INNER, etc.). If so, the join occurrence starts from the modifier.
                let join_start = find_join_keyword_start(sql, &significant, sig_idx);
                if let Some(join_end) = token_end_offset(sql, token) {
                    if let Some(occ) = make_keyword_occurrence(
                        sql,
                        join_start,
                        join_end,
                        "join_clause",
                        window_paren_depth.is_some(),
                        false,
                    ) {
                        out.push(occ);
                    }
                }

                // Track function call depth for aggregate ORDER BY.
                if let Some((_, next)) = significant.get(sig_idx + 1) {
                    if matches!(&next.token, Token::LParen) {
                        function_call_depths.push(paren_depth + 1);
                    }
                }

                sig_idx += 1;
            }
            Keyword::ORDER | Keyword::GROUP => {
                let Some((_, next)) = significant.get(sig_idx + 1) else {
                    sig_idx += 1;
                    continue;
                };
                let is_by = matches!(&next.token, Token::Word(w) if w.keyword == Keyword::BY);
                if !is_by {
                    sig_idx += 1;
                    continue;
                }

                if let (Some(kw_start), Some(kw_end)) =
                    (token_start_offset(sql, token), token_end_offset(sql, next))
                {
                    let clause_type = if word.keyword == Keyword::ORDER {
                        "orderby_clause"
                    } else {
                        "groupby_clause"
                    };

                    let in_window = window_paren_depth.is_some();
                    let in_aggregate =
                        !function_call_depths.is_empty() && word.keyword == Keyword::ORDER;

                    if let Some(occ) = make_keyword_occurrence(
                        sql,
                        kw_start,
                        kw_end,
                        clause_type,
                        in_window,
                        in_aggregate,
                    ) {
                        out.push(occ);
                    }
                }

                sig_idx += 2;
            }
            Keyword::PARTITION => {
                let Some((_, next)) = significant.get(sig_idx + 1) else {
                    sig_idx += 1;
                    continue;
                };
                let is_by = matches!(&next.token, Token::Word(w) if w.keyword == Keyword::BY);
                if !is_by {
                    sig_idx += 1;
                    continue;
                }

                if let (Some(kw_start), Some(kw_end)) =
                    (token_start_offset(sql, token), token_end_offset(sql, next))
                {
                    if let Some(occ) = make_keyword_occurrence(
                        sql,
                        kw_start,
                        kw_end,
                        "partitionby_clause",
                        window_paren_depth.is_some(),
                        false,
                    ) {
                        out.push(occ);
                    }
                }

                sig_idx += 2;
            }
            Keyword::DOUBLE | Keyword::NOT => {
                // Data type keywords: DOUBLE PRECISION, NOT NULL.
                // Only relevant when data_type config is set.
                if let Some((_, next)) = significant.get(sig_idx + 1) {
                    let is_data_type_compound = match word.keyword {
                        Keyword::DOUBLE => {
                            matches!(&next.token, Token::Word(w) if w.keyword == Keyword::PRECISION)
                        }
                        Keyword::NOT => {
                            matches!(&next.token, Token::Word(w) if w.keyword == Keyword::NULL)
                        }
                        _ => false,
                    };
                    if is_data_type_compound {
                        if let (Some(kw_start), Some(kw_end)) =
                            (token_start_offset(sql, token), token_end_offset(sql, next))
                        {
                            if let Some(occ) = make_keyword_occurrence(
                                sql,
                                kw_start,
                                kw_end,
                                "data_type",
                                false,
                                false,
                            ) {
                                out.push(occ);
                            }
                        }

                        sig_idx += 2;
                        continue;
                    }
                }
                sig_idx += 1;
            }
            _ => {
                // Track function calls for aggregate ORDER BY detection.
                // This includes both keyword-functions (like ROW_NUMBER) and
                // identifier-functions (like STRING_AGG).
                if let Some((_, next)) = significant.get(sig_idx + 1) {
                    if matches!(&next.token, Token::LParen) {
                        function_call_depths.push(paren_depth + 1);
                    }
                }
                sig_idx += 1;
            }
        }
    }

    out
}

fn single_keyword_occurrence(
    sql: &str,
    token: &TokenWithSpan,
    clause_type: &'static str,
    in_window: bool,
    in_aggregate: bool,
) -> Option<KeywordOccurrence> {
    let start = token_start_offset(sql, token)?;
    let end = token_end_offset(sql, token)?;
    make_keyword_occurrence(sql, start, end, clause_type, in_window, in_aggregate)
}

fn make_keyword_occurrence(
    sql: &str,
    start: usize,
    end: usize,
    clause_type: &'static str,
    in_window: bool,
    in_aggregate: bool,
) -> Option<KeywordOccurrence> {
    let line_start = sql[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = sql[end..].find('\n').map_or(sql.len(), |i| end + i);

    let before_on_line = &sql[line_start..start];
    let after_on_line = &sql[end..line_end];

    let has_content_before = before_on_line.chars().any(|c| !c.is_ascii_whitespace());
    // For "content after" check, ignore closing parens/brackets that immediately follow
    // the keyword — they are structural delimiters, not clause content.
    let after_trimmed = after_on_line.trim_start();
    let has_content_after = !after_trimmed.is_empty()
        && !after_trimmed
            .chars()
            .all(|c| c == ')' || c == ']' || c.is_ascii_whitespace());
    let is_first_on_line = !has_content_before;

    Some(KeywordOccurrence {
        clause_type,
        start,
        end,
        has_content_before,
        has_content_after,
        is_first_on_line,
        in_window,
        in_aggregate_order_by: in_aggregate,
    })
}

/// Find the start of a JOIN keyword, including any preceding modifiers
/// (LEFT, RIGHT, INNER, FULL, CROSS, OUTER) on the same line.
fn find_join_keyword_start(
    sql: &str,
    significant: &[(usize, &TokenWithSpan)],
    sig_idx: usize,
) -> usize {
    let (_, token) = significant[sig_idx];
    let join_start = match token_start_offset(sql, token) {
        Some(s) => s,
        std::option::Option::None => return 0,
    };

    // Look backwards through significant tokens for JOIN modifiers.
    let join_line_start = sql[..join_start].rfind('\n').map_or(0, |i| i + 1);

    let mut earliest_start = join_start;
    let mut look_back = sig_idx;
    while look_back > 0 {
        look_back -= 1;
        let (_, prev) = significant[look_back];
        let Token::Word(w) = &prev.token else {
            break;
        };
        if !matches!(
            w.keyword,
            Keyword::LEFT
                | Keyword::RIGHT
                | Keyword::INNER
                | Keyword::FULL
                | Keyword::CROSS
                | Keyword::OUTER
        ) {
            break;
        }
        let prev_start = match token_start_offset(sql, prev) {
            Some(s) => s,
            std::option::Option::None => break,
        };
        // Must be on the same line.
        if prev_start < join_line_start {
            break;
        }
        earliest_start = prev_start;
    }

    earliest_start
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

// ---------------------------------------------------------------------------
// Legacy detection (no-config fallback)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct ClauseOccurrence {
    line: u64,
    start: usize,
    end: usize,
}

fn keyword_newline_violation_span(
    sql: &str,
    dialect: Dialect,
    tokens: Option<&[TokenWithSpan]>,
) -> Option<(usize, usize)> {
    let owned_tokens;
    let tokens = if let Some(tokens) = tokens {
        tokens
    } else {
        owned_tokens = tokenized(sql, dialect)?;
        &owned_tokens
    };

    // Only consider top-level SELECTs (paren_depth == 0). Subquery SELECTs
    // inside EXISTS or similar constructs have compact single-line layouts
    // that are intentional and should not trigger inconsistency warnings.
    let select_line = {
        let mut depth = 0i32;
        let mut found = None;
        for token in tokens {
            match &token.token {
                Token::LParen => depth += 1,
                Token::RParen => depth -= 1,
                Token::Word(word) if word.keyword == Keyword::SELECT && depth == 0 => {
                    let select_start = line_col_to_offset(
                        sql,
                        token.span.start.line as usize,
                        token.span.start.column as usize,
                    );
                    if let Some(start) = select_start {
                        let line_start = sql[..start].rfind('\n').map_or(0, |idx| idx + 1);
                        if sql[line_start..start].trim().is_empty() {
                            found = Some(token.span.start.line);
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        found
    }?;

    let clauses = major_clause_occurrences(sql, tokens)?;

    let mut clauses_on_select_line = clauses.iter().filter(|clause| clause.line == select_line);
    let first_clause_on_select_line = clauses_on_select_line.next()?;

    let has_second_clause_on_select_line = clauses_on_select_line.next().is_some();
    let has_major_clause_on_later_line = clauses.iter().any(|clause| clause.line > select_line);

    if !has_second_clause_on_select_line && !has_major_clause_on_later_line {
        return None;
    }

    Some((
        first_clause_on_select_line.start,
        first_clause_on_select_line.end,
    ))
}

fn major_clause_occurrences(sql: &str, tokens: &[TokenWithSpan]) -> Option<Vec<ClauseOccurrence>> {
    // Build significant-token list with paren depth so we can skip subqueries.
    let mut depth = 0i32;
    let mut significant: Vec<(&TokenWithSpan, i32)> = Vec::new();
    for token in tokens {
        match &token.token {
            Token::LParen => {
                significant.push((token, depth));
                depth += 1;
            }
            Token::RParen => {
                depth -= 1;
                significant.push((token, depth));
            }
            t if is_trivia_token(t) => {}
            _ => significant.push((token, depth)),
        }
    }

    let mut out = Vec::new();
    let mut index = 0usize;

    while index < significant.len() {
        let (token, token_depth) = significant[index];
        // Only collect top-level clause keywords.
        if token_depth != 0 {
            index += 1;
            continue;
        }
        let Token::Word(word) = &token.token else {
            index += 1;
            continue;
        };

        match word.keyword {
            Keyword::FROM | Keyword::WHERE => {
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
                out.push(ClauseOccurrence {
                    line: token.span.start.line,
                    start,
                    end,
                });
                index += 1;
            }
            Keyword::GROUP | Keyword::ORDER => {
                let Some((next, _)) = significant.get(index + 1) else {
                    index += 1;
                    continue;
                };

                let is_by = matches!(&next.token, Token::Word(next_word) if next_word.keyword == Keyword::BY);
                if !is_by {
                    index += 1;
                    continue;
                }

                let start = line_col_to_offset(
                    sql,
                    token.span.start.line as usize,
                    token.span.start.column as usize,
                )?;
                let end = line_col_to_offset(
                    sql,
                    next.span.end.line as usize,
                    next.span.end.column as usize,
                )?;
                out.push(ClauseOccurrence {
                    line: token.span.start.line,
                    start,
                    end,
                });
                index += 2;
            }
            _ => index += 1,
        }
    }

    Some(out)
}

// ---------------------------------------------------------------------------
// Tokenizer & utility helpers
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
    use crate::types::IssueAutofixApplicability;
    use std::collections::BTreeMap;

    fn run(sql: &str) -> Vec<Issue> {
        let rule = LayoutKeywordNewline::default();
        run_with_rule(sql, &rule)
    }

    fn run_with_config(sql: &str, config: &LintConfig) -> Vec<Issue> {
        let rule = LayoutKeywordNewline::from_config(config);
        run_with_rule(sql, &rule)
    }

    fn run_with_rule(sql: &str, rule: &LayoutKeywordNewline) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn apply_all_autofixes(sql: &str, issues: &[Issue]) -> String {
        let mut edits: Vec<IssuePatchEdit> = issues
            .iter()
            .filter_map(|i| i.autofix.as_ref())
            .flat_map(|a| a.edits.iter().cloned())
            .collect();
        edits.sort_by(|a, b| b.span.start.cmp(&a.span.start));
        let mut out = sql.to_string();
        for edit in edits {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        out
    }

    fn make_config(clause_configs: serde_json::Value) -> LintConfig {
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: BTreeMap::from([("layout.keyword_newline".to_string(), clause_configs)]),
        }
    }

    // --- Legacy mode tests ---

    #[test]
    fn flags_inconsistent_major_clause_placement() {
        assert!(!run("SELECT a FROM t WHERE a = 1").is_empty());
        assert!(!run("SELECT a FROM t\nWHERE a = 1").is_empty());
    }

    #[test]
    fn does_not_flag_consistent_layout() {
        assert!(run("SELECT a FROM t").is_empty());
        assert!(run("SELECT a\nFROM t\nWHERE a = 1").is_empty());
    }

    #[test]
    fn does_not_flag_clause_words_in_string_literal() {
        assert!(run("SELECT 'FROM t WHERE x = 1' AS txt").is_empty());
    }

    #[test]
    fn emits_safe_autofix_patch_for_first_clause_on_select_line() {
        let sql = "SELECT a FROM t\nWHERE a = 1";
        let issues = run(sql);
        let issue = &issues[0];
        let autofix = issue.autofix.as_ref().expect("autofix metadata");

        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);

        let fixed = apply_all_autofixes(sql, &issues);
        assert!(
            fixed.contains("\nFROM t"),
            "expected FROM to move to new line: {fixed}"
        );
    }

    #[test]
    fn does_not_flag_exists_subquery_select_from_inline() {
        // EXISTS subqueries with compact SELECT 1 FROM t are intentional and
        // should not trigger the legacy inconsistency check.
        let sql = "UPDATE t SET x = 1\nWHERE EXISTS (\n  SELECT 1 FROM s\n  WHERE s.id = t.id\n)";
        assert!(run(sql).is_empty());
    }

    // --- Config-driven tests (SQLFluff parity) ---

    #[test]
    fn pass_leading_from_clause() {
        let config = make_config(serde_json::json!({
            "from_clause": {"keyword_line_position": "leading"}
        }));
        assert!(run_with_config("SELECT foo\nFROM bar\n", &config).is_empty());
    }

    #[test]
    fn pass_alone_from_clause() {
        let config = make_config(serde_json::json!({
            "from_clause": {"keyword_line_position": "alone"}
        }));
        assert!(run_with_config("SELECT foo\nFROM\n  bar\n", &config).is_empty());
    }

    #[test]
    fn fail_leading_from_clause() {
        let config = make_config(serde_json::json!({
            "from_clause": {"keyword_line_position": "leading"}
        }));
        let sql = "SELECT foo FROM bar\n";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty(), "should flag FROM not on new line");
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT foo\nFROM bar\n");
    }

    #[test]
    fn fail_alone_from_clause() {
        let config = make_config(serde_json::json!({
            "from_clause": {"keyword_line_position": "alone"}
        }));
        let sql = "SELECT foo FROM bar\n";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty(), "should flag FROM not alone");
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(fixed, "SELECT foo\nFROM\nbar\n");
    }

    #[test]
    fn pass_leading_join_clause() {
        let config = make_config(serde_json::json!({
            "join_clause": {"keyword_line_position": "leading"}
        }));
        let sql = "SELECT foo\nFROM bar a\nJOIN baz b\n  ON a.id = b.id\nINNER JOIN qux c\n  ON a.id = c.id\nLEFT OUTER JOIN quux d\n  ON a.id = d.id\n";
        assert!(run_with_config(sql, &config).is_empty());
    }

    #[test]
    fn fail_leading_join_clause() {
        let config = make_config(serde_json::json!({
            "join_clause": {"keyword_line_position": "leading"}
        }));
        let sql = "SELECT foo\nFROM bar a JOIN baz b\nON a.id = b.id INNER JOIN qux c\nON a.id = c.id LEFT OUTER JOIN quux d\nON a.id = d.id\n";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty(), "should flag JOINs not on new line");
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(
            fixed,
            "SELECT foo\nFROM bar a\nJOIN baz b\nON a.id = b.id\nINNER JOIN qux c\nON a.id = c.id\nLEFT OUTER JOIN quux d\nON a.id = d.id\n"
        );
    }

    #[test]
    fn fail_alone_join_clause() {
        let config = make_config(serde_json::json!({
            "join_clause": {"keyword_line_position": "alone"}
        }));
        let sql = "SELECT foo\nFROM bar a JOIN baz b\nON a.id = b.id INNER JOIN qux c\nON a.id = c.id LEFT OUTER JOIN quux d\nON a.id = d.id\n";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty(), "should flag JOINs not alone");
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(
            fixed,
            "SELECT foo\nFROM bar a\nJOIN\nbaz b\nON a.id = b.id\nINNER JOIN\nqux c\nON a.id = c.id\nLEFT OUTER JOIN\nquux d\nON a.id = d.id\n"
        );
    }

    #[test]
    fn pass_none_where_clause() {
        let config = make_config(serde_json::json!({
            "where_clause": {"keyword_line_position": "none"}
        }));
        assert!(run_with_config("SELECT a, b FROM tabx WHERE b = 2;\n", &config).is_empty());
    }

    #[test]
    fn fail_leading_on_condition() {
        let config = make_config(serde_json::json!({
            "join_clause": {"keyword_line_position": "leading"},
            "join_on_condition": {"keyword_line_position": "leading"}
        }));
        let sql = "SELECT foo\nFROM bar a JOIN baz b ON a.id = b.id\n";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty());
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(
            fixed,
            "SELECT foo\nFROM bar a\nJOIN baz b\nON a.id = b.id\n"
        );
    }

    #[test]
    fn fail_trailing_on_condition() {
        let config = make_config(serde_json::json!({
            "join_clause": {"keyword_line_position": "leading"},
            "join_on_condition": {"keyword_line_position": "trailing"}
        }));
        let sql = "SELECT foo\nFROM bar a JOIN baz b ON a.id = b.id\n";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty());
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(
            fixed,
            "SELECT foo\nFROM bar a\nJOIN baz b ON\na.id = b.id\n"
        );
    }

    #[test]
    fn pass_leading_orderby_with_window_exclusion() {
        let config = make_config(serde_json::json!({
            "orderby_clause": {
                "keyword_line_position": "leading",
                "keyword_line_position_exclusions": "window_specification"
            }
        }));
        let sql = "SELECT\na,\nb,\nROW_NUMBER() OVER (PARTITION BY c ORDER BY d) AS e\nFROM f\nJOIN g\nON g.h = f.h\n";
        assert!(run_with_config(sql, &config).is_empty());
    }

    #[test]
    fn fail_leading_orderby_except_window_outer_orderby() {
        let config = make_config(serde_json::json!({
            "orderby_clause": {
                "keyword_line_position": "leading",
                "keyword_line_position_exclusions": "window_specification"
            }
        }));
        let sql = "SELECT\na,\nb,\nROW_NUMBER() OVER (PARTITION BY c ORDER BY d) AS e\nFROM f\nJOIN g\nON g.h = f.h ORDER BY a\n";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty(), "should flag outer ORDER BY");
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(
            fixed,
            "SELECT\na,\nb,\nROW_NUMBER() OVER (PARTITION BY c ORDER BY d) AS e\nFROM f\nJOIN g\nON g.h = f.h\nORDER BY a\n"
        );
    }

    #[test]
    fn fail_alone_window_function_partitionby_orderby() {
        let config = make_config(serde_json::json!({
            "partitionby_clause": {"keyword_line_position": "alone"},
            "orderby_clause": {
                "keyword_line_position": "alone",
                "keyword_line_position_exclusions": null
            }
        }));
        let sql = "SELECT\na,\nb,\nROW_NUMBER() OVER (PARTITION BY c ORDER BY d) AS e\nFROM f\nJOIN g\nON g.h = f.h\n";
        let issues = run_with_config(sql, &config);
        assert!(
            !issues.is_empty(),
            "should flag PARTITION BY and ORDER BY in window"
        );
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(
            fixed,
            "SELECT\na,\nb,\nROW_NUMBER() OVER (\nPARTITION BY\nc\nORDER BY\nd) AS e\nFROM f\nJOIN g\nON g.h = f.h\n"
        );
    }

    #[test]
    fn fail_select_clause_alone() {
        let config = make_config(serde_json::json!({
            "select_clause": {"keyword_line_position": "alone"}
        }));
        let sql = "WITH some_cte AS (SELECT\n    column1,\n    column2\n    FROM some_table\n) SELECT\n  column1,\n  column2\nFROM some_cte\n";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty(), "should flag SELECT not alone");
    }

    #[test]
    fn fail_data_type_alone() {
        let config = make_config(serde_json::json!({
            "data_type": {"keyword_line_position": "alone"}
        }));
        let sql = "CREATE TABLE t (c1 DOUBLE PRECISION NOT NULL)\n";
        let issues = run_with_config(sql, &config);
        assert!(
            !issues.is_empty(),
            "should flag DOUBLE PRECISION and NOT NULL"
        );
        let fixed = apply_all_autofixes(sql, &issues);
        assert_eq!(fixed, "CREATE TABLE t (c1\nDOUBLE PRECISION\nNOT NULL)\n");
    }

    #[test]
    fn pass_leading_orderby_except_window_and_aggregate() {
        let config = make_config(serde_json::json!({
            "orderby_clause": {
                "keyword_line_position": "leading",
                "keyword_line_position_exclusions": "window_specification, aggregate_order_by"
            }
        }));
        let sql = "SELECT\na,\nb,\nROW_NUMBER() OVER (PARTITION BY c ORDER BY d) AS e,\nSTRING_AGG(a ORDER BY b, c)\nFROM f\nJOIN g\nON g.h = f.h\n";
        assert!(run_with_config(sql, &config).is_empty());
    }
}
