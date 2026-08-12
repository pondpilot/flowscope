//! LINT_CV_004: Prefer COUNT(*) over COUNT(1).
//!
//! `COUNT(1)` and `COUNT(*)` are semantically identical in all major databases,
//! but `COUNT(*)` is the standard convention and more clearly expresses intent.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::linter::visit;
use crate::types::{issue_codes, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::{Spanned, *};
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CountPreference {
    Star,
    One,
    Zero,
}

impl CountPreference {
    fn from_config(config: &LintConfig) -> Self {
        let prefer_one = config
            .rule_option_bool(issue_codes::LINT_CV_004, "prefer_count_1")
            .unwrap_or(false);
        let prefer_zero = config
            .rule_option_bool(issue_codes::LINT_CV_004, "prefer_count_0")
            .unwrap_or(false);

        if prefer_one {
            Self::One
        } else if prefer_zero {
            Self::Zero
        } else {
            Self::Star
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Star => "Use COUNT(*) for row counts.",
            Self::One => "Use COUNT(1) for row counts.",
            Self::Zero => "Use COUNT(0) for row counts.",
        }
    }

    fn violates(self, kind: CountArgKind) -> bool {
        match self {
            Self::Star => matches!(kind, CountArgKind::One | CountArgKind::Zero),
            Self::One => matches!(kind, CountArgKind::Star | CountArgKind::Zero),
            Self::Zero => matches!(kind, CountArgKind::Star | CountArgKind::One),
        }
    }

    fn replacement(self) -> &'static str {
        match self {
            Self::Star => "*",
            Self::One => "1",
            Self::Zero => "0",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CountArgKind {
    Star,
    One,
    Zero,
    Other,
}

pub struct CountStyle {
    preference: CountPreference,
}

impl CountStyle {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            preference: CountPreference::from_config(config),
        }
    }
}

impl Default for CountStyle {
    fn default() -> Self {
        Self {
            preference: CountPreference::Star,
        }
    }
}

impl BuiltinLintRule for CountStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_004
    }

    fn name(&self) -> &'static str {
        "COUNT style"
    }

    fn description(&self) -> &'static str {
        "Use consistent syntax to express \"count number of rows\"."
    }

    fn check_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let tokens =
            tokenized_for_context(ctx).or_else(|| tokenized(ctx.statement_sql(), ctx.dialect()));
        let wildcard_spans = tokens
            .as_deref()
            .map(collect_count_wildcard_spans)
            .unwrap_or_default();
        let numeric_spans = tokens
            .as_deref()
            .map(collect_count_numeric_spans)
            .unwrap_or_default();
        let mut wildcard_index = 0usize;
        let mut numeric_index = 0usize;

        let mut issues = Vec::new();
        visit::visit_expressions(stmt, &mut |expr| {
            let Expr::Function(func) = expr else {
                return;
            };
            if !func.name.to_string().eq_ignore_ascii_case("COUNT") {
                return;
            }

            let kind = count_argument_kind(&func.args);
            let argument_span = match kind {
                CountArgKind::Star => {
                    let span = wildcard_spans.get(wildcard_index).copied();
                    wildcard_index = wildcard_index.saturating_add(1);
                    span
                }
                CountArgKind::One | CountArgKind::Zero => {
                    let span = numeric_spans.get(numeric_index).copied();
                    numeric_index = numeric_index.saturating_add(1);
                    span.or_else(|| count_numeric_argument_span(ctx, func))
                }
                CountArgKind::Other => None,
            };

            if self.preference.violates(kind) {
                let mut issue = Issue::info(issue_codes::LINT_CV_004, self.preference.message())
                    .with_statement(ctx.statement_index);
                if let Some((start, end)) = argument_span {
                    let span = ctx.span_from_statement_offset(start, end);
                    issue = issue.with_span(span).with_autofix_edits(
                        IssueAutofixApplicability::Safe,
                        vec![IssuePatchEdit::new(span, self.preference.replacement())],
                    );
                }
                issues.push(issue);
            }
        });
        issues
    }
}

fn count_argument_kind(args: &FunctionArguments) -> CountArgKind {
    let arg_list = match args {
        FunctionArguments::List(list) => list,
        _ => return CountArgKind::Other,
    };

    if arg_list.args.len() != 1 {
        return CountArgKind::Other;
    }

    match &arg_list.args[0] {
        FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => CountArgKind::Star,
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(ValueWithSpan {
            value: Value::Number(n, _),
            ..
        }))) if numeric_literal_matches(n, 1) => CountArgKind::One,
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(ValueWithSpan {
            value: Value::Number(n, _),
            ..
        }))) if numeric_literal_matches(n, 0) => CountArgKind::Zero,
        _ => CountArgKind::Other,
    }
}

fn numeric_literal_matches(raw: &str, expected: u8) -> bool {
    raw.trim()
        .parse::<u64>()
        .ok()
        .is_some_and(|value| value == expected as u64)
}

fn count_numeric_argument_span(ctx: &RuleContext, func: &Function) -> Option<(usize, usize)> {
    let FunctionArguments::List(arg_list) = &func.args else {
        return None;
    };
    if arg_list.args.len() != 1 {
        return None;
    }

    let FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) = &arg_list.args[0] else {
        return None;
    };

    if let Some((start, end)) = expr_span_offsets(ctx.statement_sql(), expr) {
        return Some((start, end));
    }

    let (start, end) = expr_span_offsets(ctx.sql, expr)?;
    if start < ctx.statement_range.start || end > ctx.statement_range.end {
        return None;
    }

    Some((
        start - ctx.statement_range.start,
        end - ctx.statement_range.start,
    ))
}

fn collect_count_wildcard_spans(tokens: &[LocatedToken]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0usize;

    while i < tokens.len() {
        if !is_count_word(&tokens[i].token) {
            i += 1;
            continue;
        }

        let mut j = i + 1;
        skip_trivia_tokens(tokens, &mut j);
        if j >= tokens.len() || !matches!(tokens[j].token, Token::LParen) {
            i += 1;
            continue;
        }

        j += 1;
        skip_trivia_tokens(tokens, &mut j);
        if j >= tokens.len() {
            break;
        }

        if let Token::Word(word) = &tokens[j].token {
            if word.value.eq_ignore_ascii_case("ALL") || word.value.eq_ignore_ascii_case("DISTINCT")
            {
                j += 1;
                skip_trivia_tokens(tokens, &mut j);
            }
        }

        if j >= tokens.len() || !matches!(tokens[j].token, Token::Mul) {
            i += 1;
            continue;
        }

        let star_start = tokens[j].start;
        let star_end = tokens[j].end;
        j += 1;
        skip_trivia_tokens(tokens, &mut j);
        if j < tokens.len() && matches!(tokens[j].token, Token::RParen) {
            spans.push((star_start, star_end));
            i = j + 1;
        } else {
            i += 1;
        }
    }

    spans
}

fn collect_count_numeric_spans(tokens: &[LocatedToken]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0usize;

    while i < tokens.len() {
        if !is_count_word(&tokens[i].token) {
            i += 1;
            continue;
        }

        let mut j = i + 1;
        skip_trivia_tokens(tokens, &mut j);
        if j >= tokens.len() || !matches!(tokens[j].token, Token::LParen) {
            i += 1;
            continue;
        }

        j += 1;
        skip_trivia_tokens(tokens, &mut j);
        if j >= tokens.len() {
            break;
        }

        if let Token::Word(word) = &tokens[j].token {
            if word.value.eq_ignore_ascii_case("ALL") || word.value.eq_ignore_ascii_case("DISTINCT")
            {
                j += 1;
                skip_trivia_tokens(tokens, &mut j);
            }
        }

        if j >= tokens.len() {
            break;
        }

        let Some(raw_number) = token_numeric_literal(&tokens[j].token) else {
            i += 1;
            continue;
        };
        if !numeric_literal_matches(raw_number, 0) && !numeric_literal_matches(raw_number, 1) {
            i += 1;
            continue;
        }

        let number_start = tokens[j].start;
        let number_end = tokens[j].end;
        j += 1;
        skip_trivia_tokens(tokens, &mut j);
        if j < tokens.len() && matches!(tokens[j].token, Token::RParen) {
            spans.push((number_start, number_end));
            i = j + 1;
        } else {
            i += 1;
        }
    }

    spans
}

fn skip_trivia_tokens(tokens: &[LocatedToken], index: &mut usize) {
    while *index < tokens.len() && is_trivia_token(&tokens[*index].token) {
        *index += 1;
    }
}

fn is_count_word(token: &Token) -> bool {
    matches!(token, Token::Word(word) if word.value.eq_ignore_ascii_case("COUNT"))
}

fn token_numeric_literal(token: &Token) -> Option<&str> {
    match token {
        Token::Number(raw, _) => Some(raw.as_str()),
        _ => None,
    }
}

fn expr_span_offsets(sql: &str, expr: &Expr) -> Option<(usize, usize)> {
    let span = expr.span();
    if span.start.line == 0 || span.start.column == 0 || span.end.line == 0 || span.end.column == 0
    {
        return None;
    }
    let start = line_col_to_offset(sql, span.start.line as usize, span.start.column as usize)?;
    let end = line_col_to_offset(sql, span.end.line as usize, span.end.column as usize)?;
    if end < start {
        return None;
    }
    Some((start, end))
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn tokenized(sql: &str, dialect: crate::types::Dialect) -> Option<Vec<LocatedToken>> {
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

fn tokenized_for_context(ctx: &RuleContext) -> Option<Vec<LocatedToken>> {
    let statement_start = ctx.statement_range.start;
    let from_document = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        Some(
            tokens
                .iter()
                .filter_map(|token| {
                    let (start, end) = token_with_span_offsets(ctx.sql, token)?;
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
        Some(sql.len())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = CountStyle::default();
        let ctx = RuleContext::new(sql, 0..sql.len(), 0);
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check_with_context(stmt, &ctx));
        }
        issues
    }

    fn assert_single_safe_edit(
        issue: &Issue,
        expected_start: usize,
        expected_end: usize,
        expected_replacement: &str,
    ) {
        let span = issue.span.expect("issue span");
        assert_eq!(span.start, expected_start);
        assert_eq!(span.end, expected_end);

        let autofix = issue.autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        assert_eq!(autofix.edits[0].span.start, expected_start);
        assert_eq!(autofix.edits[0].span.end, expected_end);
        assert_eq!(autofix.edits[0].replacement, expected_replacement);
    }

    #[test]
    fn test_count_one_detected() {
        let sql = "SELECT COUNT(1) FROM t";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_CV_004");

        let one_start = sql.find('1').expect("count literal");
        assert_single_safe_edit(&issues[0], one_start, one_start + 1, "*");
    }

    #[test]
    fn test_count_leading_zero_numeric_literals_are_detected() {
        let sql = "SELECT COUNT(01), COUNT(00) FROM t";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 2);

        let first_start = sql.find("01").expect("first literal");
        let second_start = sql.find("00").expect("second literal");
        assert_single_safe_edit(&issues[0], first_start, first_start + 2, "*");
        assert_single_safe_edit(&issues[1], second_start, second_start + 2, "*");
    }

    #[test]
    fn test_count_star_ok() {
        let issues = check_sql("SELECT COUNT(*) FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_count_column_ok() {
        let issues = check_sql("SELECT COUNT(id) FROM t");
        assert!(issues.is_empty());
    }

    // --- Edge cases ---

    #[test]
    fn test_count_zero_detected_with_default_star_preference() {
        let issues = check_sql("SELECT COUNT(0) FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_count_one_in_having() {
        let issues = check_sql("SELECT col FROM t GROUP BY col HAVING COUNT(1) > 5");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_count_one_in_subquery() {
        let issues =
            check_sql("SELECT * FROM t WHERE id IN (SELECT COUNT(1) FROM t2 GROUP BY col)");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_multiple_count_one() {
        let issues = check_sql("SELECT COUNT(1), COUNT(1) FROM t");
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_count_distinct_ok() {
        let issues = check_sql("SELECT COUNT(DISTINCT id) FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_count_one_in_cte() {
        let issues = check_sql("WITH cte AS (SELECT COUNT(1) AS cnt FROM t) SELECT * FROM cte");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_count_one_in_qualify() {
        let issues = check_sql("SELECT a FROM t QUALIFY COUNT(1) > 0");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_prefer_count_one_flags_count_star() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.count_rows".to_string(),
                serde_json::json!({"prefer_count_1": true}),
            )]),
        };
        let rule = CountStyle::from_config(&config);
        let sql = "SELECT COUNT(*) FROM t";
        let stmts = parse_sql(sql).unwrap();
        let issues = rule.check_with_context(&stmts[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);

        let star_start = sql.find('*').expect("star argument");
        assert_single_safe_edit(&issues[0], star_start, star_start + 1, "1");
    }

    #[test]
    fn test_prefer_count_zero_flags_count_one() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_004".to_string(),
                serde_json::json!({"prefer_count_0": true}),
            )]),
        };
        let rule = CountStyle::from_config(&config);
        let sql = "SELECT COUNT(1) FROM t";
        let stmts = parse_sql(sql).unwrap();
        let issues = rule.check_with_context(&stmts[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);

        let one_start = sql.find('1').expect("count literal");
        assert_single_safe_edit(&issues[0], one_start, one_start + 1, "0");
    }
}
