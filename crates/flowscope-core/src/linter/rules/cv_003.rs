//! LINT_CV_003: Select trailing comma.
//!
//! Avoid trailing comma before FROM.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::linter::rules::semantic_helpers::visit_selects_in_statement;
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::{GroupByExpr, Select, SelectItem, Spanned, Statement};
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectClauseTrailingCommaPolicy {
    Forbid,
    Require,
}

impl SelectClauseTrailingCommaPolicy {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_CV_003, "select_clause_trailing_comma")
            .unwrap_or("forbid")
            .to_ascii_lowercase()
            .as_str()
        {
            "require" => Self::Require,
            _ => Self::Forbid,
        }
    }

    fn violated(self, trailing_comma_present: bool) -> bool {
        match self {
            Self::Forbid => trailing_comma_present,
            Self::Require => !trailing_comma_present,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Forbid => "Avoid trailing comma before FROM in SELECT clause.",
            Self::Require => "Use trailing comma before FROM in SELECT clause.",
        }
    }
}

pub struct ConventionSelectTrailingComma {
    policy: SelectClauseTrailingCommaPolicy,
}

impl ConventionSelectTrailingComma {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            policy: SelectClauseTrailingCommaPolicy::from_config(config),
        }
    }
}

impl Default for ConventionSelectTrailingComma {
    fn default() -> Self {
        Self {
            policy: SelectClauseTrailingCommaPolicy::Forbid,
        }
    }
}

impl LintRule for ConventionSelectTrailingComma {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_003
    }

    fn name(&self) -> &'static str {
        "Select trailing comma"
    }

    fn description(&self) -> &'static str {
        "Trailing commas within select clause."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let tokens =
            tokenized_for_context(ctx).or_else(|| tokenized(ctx.statement_sql(), ctx.dialect()));
        if has_select_trailing_comma_violation(
            statement,
            ctx.statement_sql(),
            self.policy,
            tokens.as_deref(),
        ) {
            vec![
                Issue::warning(issue_codes::LINT_CV_003, self.policy.message())
                    .with_statement(ctx.statement_index),
            ]
        } else {
            Vec::new()
        }
    }
}

fn has_select_trailing_comma_violation(
    statement: &Statement,
    sql: &str,
    policy: SelectClauseTrailingCommaPolicy,
    tokens: Option<&[LocatedToken]>,
) -> bool {
    let mut violation = false;
    visit_selects_in_statement(statement, &mut |select| {
        if violation {
            return;
        }
        if select_clause_violates_policy(select, sql, policy, tokens) {
            violation = true;
        }
    });
    violation
}

fn select_clause_violates_policy(
    select: &Select,
    sql: &str,
    policy: SelectClauseTrailingCommaPolicy,
    tokens: Option<&[LocatedToken]>,
) -> bool {
    let Some(last_projection_end) = select_last_projection_end(select) else {
        return false;
    };
    let Some(last_projection_end_offset) = line_col_to_offset(
        sql,
        last_projection_end.0 as usize,
        last_projection_end.1 as usize,
    ) else {
        return false;
    };

    let boundary = select_clause_boundary(select).or_else(|| span_end(select.span()));
    let Some(boundary) = boundary else {
        return false;
    };
    let Some(boundary_offset) = line_col_to_offset(sql, boundary.0 as usize, boundary.1 as usize)
    else {
        return false;
    };
    if boundary_offset < last_projection_end_offset {
        return false;
    }

    let has_trailing_comma =
        first_significant_token_is_comma(tokens, sql, last_projection_end_offset, boundary_offset);
    policy.violated(has_trailing_comma)
}

fn select_last_projection_end(select: &Select) -> Option<(u64, u64)> {
    let item = select.projection.last()?;
    match item {
        SelectItem::ExprWithAlias { alias, .. } => span_end(alias.span),
        SelectItem::UnnamedExpr(expr) => span_end(expr.span()),
        SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => span_end(item.span()),
    }
}

fn select_clause_boundary(select: &Select) -> Option<(u64, u64)> {
    let mut candidates = Vec::new();

    if let Some(first_from) = select.from.first() {
        if let Some(start) = span_start(first_from.relation.span()) {
            candidates.push(start);
        }
    }

    if let Some(into) = &select.into {
        if let Some(start) = span_start(into.span()) {
            candidates.push(start);
        }
    }

    if let Some(expr) = &select.prewhere {
        if let Some(start) = span_start(expr.span()) {
            candidates.push(start);
        }
    }
    if let Some(expr) = &select.selection {
        if let Some(start) = span_start(expr.span()) {
            candidates.push(start);
        }
    }
    if let Some(expr) = &select.having {
        if let Some(start) = span_start(expr.span()) {
            candidates.push(start);
        }
    }
    if let Some(expr) = &select.qualify {
        if let Some(start) = span_start(expr.span()) {
            candidates.push(start);
        }
    }
    if let Some(connect_by) = &select.connect_by {
        if let Some(start) = span_start(connect_by.span()) {
            candidates.push(start);
        }
    }

    if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
        if let Some(start) = exprs.first().and_then(|expr| span_start(expr.span())) {
            candidates.push(start);
        }
    }
    if let Some(start) = select
        .cluster_by
        .first()
        .and_then(|expr| span_start(expr.span()))
    {
        candidates.push(start);
    }
    if let Some(start) = select
        .distribute_by
        .first()
        .and_then(|expr| span_start(expr.span()))
    {
        candidates.push(start);
    }
    if let Some(start) = select
        .sort_by
        .first()
        .and_then(|expr| span_start(expr.expr.span()))
    {
        candidates.push(start);
    }
    if let Some(start) = select
        .named_window
        .first()
        .and_then(|window| span_start(window.span()))
    {
        candidates.push(start);
    }

    candidates.into_iter().min()
}

fn span_start(span: sqlparser::tokenizer::Span) -> Option<(u64, u64)> {
    if span.start.line == 0 || span.start.column == 0 {
        None
    } else {
        Some((span.start.line, span.start.column))
    }
}

fn span_end(span: sqlparser::tokenizer::Span) -> Option<(u64, u64)> {
    if span.end.line == 0 || span.end.column == 0 {
        None
    } else {
        Some((span.end.line, span.end.column))
    }
}

fn first_significant_token_is_comma(
    tokens: Option<&[LocatedToken]>,
    sql: &str,
    start: usize,
    end: usize,
) -> bool {
    let Some(tokens) = tokens else {
        let clause_suffix = &sql[start..end];
        return first_significant_char_is_comma(clause_suffix);
    };

    for token in tokens {
        if token.end <= start || token.start >= end {
            continue;
        }
        if is_trivia_token(&token.token) {
            continue;
        }
        return matches!(token.token, Token::Comma);
    }
    false
}

fn first_significant_char_is_comma(slice: &str) -> bool {
    let bytes = slice.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b' ' | b'\t' | b'\n' | b'\r' => {
                index += 1;
            }
            b'-' if index + 1 < bytes.len() && bytes[index + 1] == b'-' => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'*' => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                if index + 1 < bytes.len() {
                    index += 2;
                } else {
                    index = bytes.len();
                }
            }
            byte => return byte == b',',
        }
    }
    false
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<LocatedToken>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        let Some((start, end)) = token_with_span_offsets(sql, &token) else {
            continue;
        };
        out.push(LocatedToken {
            token: token.token,
            start,
            end,
        });
    }
    Some(out)
}

fn tokenized_for_context(ctx: &LintContext) -> Option<Vec<LocatedToken>> {
    let statement_start = ctx.statement_range.start;
    let from_document = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        Some(
            tokens
                .iter()
                .filter_map(|token| {
                    let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                        return None;
                    };
                    if start < ctx.statement_range.start || end > ctx.statement_range.end {
                        return None;
                    }

                    Some(LocatedToken {
                        token: token.token.clone(),
                        start: start - statement_start,
                        end: end - statement_start,
                    })
                })
                .collect::<Vec<_>>(),
        )
    });

    if let Some(tokens) = from_document {
        return Some(tokens);
    }

    tokenized(ctx.statement_sql(), ctx.dialect())
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

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
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
    for (idx, ch) in sql.char_indices() {
        if current_line == line && current_col == column {
            return Some(idx);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        run_with_rule(sql, ConventionSelectTrailingComma::default())
    }

    fn run_with_rule(sql: &str, rule: ConventionSelectTrailingComma) -> Vec<Issue> {
        let stmts = parse_sql(sql).expect("parse");
        stmts
            .iter()
            .enumerate()
            .flat_map(|(index, stmt)| {
                rule.check(
                    stmt,
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
    fn flags_trailing_comma_before_from() {
        let issues = run("select a, from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_003);
    }

    #[test]
    fn wildcard_select_without_trailing_comma_is_allowed() {
        let issues = run("SELECT * FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn wildcard_select_with_trailing_comma_is_flagged() {
        let issues = run("SELECT *, FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_003);
    }

    #[test]
    fn allows_no_trailing_comma() {
        let issues = run("select a from t");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_nested_select_trailing_comma() {
        let issues = run("SELECT (SELECT a, FROM t) AS x FROM outer_t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_003);
    }

    #[test]
    fn does_not_flag_comma_in_string_or_comment() {
        let issues = run("SELECT 'a, from t' AS txt -- select a, from t\nFROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn require_policy_flags_missing_trailing_comma() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.select_trailing_comma".to_string(),
                serde_json::json!({"select_clause_trailing_comma": "require"}),
            )]),
        };
        let rule = ConventionSelectTrailingComma::from_config(&config);
        let issues = run_with_rule("SELECT a FROM t", rule);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn require_policy_allows_trailing_comma() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_003".to_string(),
                serde_json::json!({"select_clause_trailing_comma": "require"}),
            )]),
        };
        let rule = ConventionSelectTrailingComma::from_config(&config);
        let issues = run_with_rule("SELECT a, FROM t", rule);
        assert!(issues.is_empty());
    }

    #[test]
    fn require_policy_flags_select_without_from() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.select_trailing_comma".to_string(),
                serde_json::json!({"select_clause_trailing_comma": "require"}),
            )]),
        };
        let rule = ConventionSelectTrailingComma::from_config(&config);
        let issues = run_with_rule("SELECT 1", rule);
        assert_eq!(issues.len(), 1);
    }
}
