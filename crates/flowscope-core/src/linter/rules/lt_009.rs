//! LINT_LT_009: Layout select targets.
//!
//! SQLFluff LT09 parity: enforce select-target line layout for single-target
//! and multi-target SELECT clauses, with wildcard-policy behavior.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::linter::rules::semantic_helpers::visit_selects_in_statement;
use crate::types::{issue_codes, Dialect, Issue};
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
            "multiple" => Self::Multiple,
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

impl LintRule for LayoutSelectTargets {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_009
    }

    fn name(&self) -> &'static str {
        "Layout select targets"
    }

    fn description(&self) -> &'static str {
        "Select targets should be on a new line unless there is only one target."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        lt09_violation_spans(statement, ctx, self.wildcard_policy)
            .into_iter()
            .map(|(start, end)| {
                Issue::info(
                    issue_codes::LINT_LT_009,
                    "Select targets should be on a new line unless there is only one target.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
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

fn lt09_violation_spans(
    statement: &Statement,
    ctx: &LintContext,
    wildcard_policy: WildcardPolicy,
) -> Vec<(usize, usize)> {
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
        let Some(start) = line_col_to_offset(
            sql,
            token.span.start.line as usize,
            token.span.start.column as usize,
        ) else {
            continue;
        };
        let Some(end) = line_col_to_offset(
            sql,
            token.span.end.line as usize,
            token.span.end.column as usize,
        ) else {
            continue;
        };

        spans.push((start, end));
    }

    spans
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
    for (idx, token) in tokens.iter().enumerate().take(end).skip(start) {
        match &token.token {
            token if is_ignorable_layout_token(token) => {}
            Token::Word(word) if is_select_modifier_keyword(word.keyword) => {}
            _ => return Some(idx),
        }
    }
    None
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

    fn run_with_rule(sql: &str, rule: &LayoutSelectTargets) -> Vec<Issue> {
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
        assert!(!run(sql).is_empty());
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
    fn allows_leading_comma_layout_for_multiple_targets() {
        let sql = "select\n    a\n    , b\n    , c";
        assert!(run(sql).is_empty());
    }
}
