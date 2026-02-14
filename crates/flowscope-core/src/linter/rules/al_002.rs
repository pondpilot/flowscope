//! LINT_AL_002: Column alias style.
//!
//! SQLFluff parity: configurable column aliasing style (`explicit`/`implicit`).

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::{Ident, SelectItem, Spanned, Statement};
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

use super::semantic_helpers::visit_selects_in_statement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AliasingPreference {
    Explicit,
    Implicit,
}

impl AliasingPreference {
    fn from_config(config: &LintConfig, rule_code: &str) -> Self {
        match config
            .rule_option_str(rule_code, "aliasing")
            .unwrap_or("explicit")
            .to_ascii_lowercase()
            .as_str()
        {
            "implicit" => Self::Implicit,
            _ => Self::Explicit,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Explicit => "Use explicit AS when aliasing columns.",
            Self::Implicit => "Use implicit aliasing when aliasing columns (omit AS).",
        }
    }

    fn violation(self, explicit_as: bool) -> bool {
        match self {
            Self::Explicit => !explicit_as,
            Self::Implicit => explicit_as,
        }
    }
}

pub struct AliasingColumnStyle {
    aliasing: AliasingPreference,
}

impl AliasingColumnStyle {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            aliasing: AliasingPreference::from_config(config, issue_codes::LINT_AL_002),
        }
    }
}

impl Default for AliasingColumnStyle {
    fn default() -> Self {
        Self {
            aliasing: AliasingPreference::Explicit,
        }
    }
}

impl LintRule for AliasingColumnStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_002
    }

    fn name(&self) -> &'static str {
        "Column alias style"
    }

    fn description(&self) -> &'static str {
        "Implicit/explicit aliasing of columns."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        let tokens = tokenized_for_context(ctx).or_else(|| tokenized(ctx.statement_sql(), ctx.dialect()));

        visit_selects_in_statement(statement, &mut |select| {
            for item in &select.projection {
                let SelectItem::ExprWithAlias { alias, .. } = item else {
                    continue;
                };

                let Some(occurrence) =
                    alias_occurrence_in_statement(alias, item, ctx, tokens.as_deref())
                else {
                    continue;
                };

                if occurrence.tsql_equals_assignment {
                    // TSQL supports `SELECT alias = expr`, which SQLFluff excludes from AL02.
                    continue;
                }

                if !self.aliasing.violation(occurrence.explicit_as) {
                    continue;
                }

                issues.push(
                    Issue::info(issue_codes::LINT_AL_002, self.aliasing.message())
                        .with_statement(ctx.statement_index)
                        .with_span(
                            ctx.span_from_statement_offset(occurrence.start, occurrence.end),
                        ),
                );
            }
        });

        issues
    }
}

#[derive(Clone, Copy)]
struct AliasOccurrence {
    start: usize,
    end: usize,
    explicit_as: bool,
    tsql_equals_assignment: bool,
}

fn alias_occurrence_in_statement(
    alias: &Ident,
    item: &SelectItem,
    ctx: &LintContext,
    tokens: Option<&[LocatedToken]>,
) -> Option<AliasOccurrence> {
    let abs_start = line_col_to_offset(
        ctx.sql,
        alias.span.start.line as usize,
        alias.span.start.column as usize,
    )?;
    let abs_end = line_col_to_offset(
        ctx.sql,
        alias.span.end.line as usize,
        alias.span.end.column as usize,
    )?;

    if abs_start < ctx.statement_range.start || abs_end > ctx.statement_range.end {
        return None;
    }

    let rel_start = abs_start - ctx.statement_range.start;
    let rel_end = abs_end - ctx.statement_range.start;
    let item_span = item.span();
    let abs_item_end = line_col_to_offset(
        ctx.sql,
        item_span.end.line as usize,
        item_span.end.column as usize,
    )?;
    if abs_item_end < abs_end || abs_item_end > ctx.statement_range.end {
        return None;
    }
    let rel_item_end = abs_item_end - ctx.statement_range.start;

    let statement_sql = ctx.statement_sql();
    let explicit_as = tokens
        .and_then(|tokens| explicit_as_before_alias_tokens(tokens, rel_start))
        .unwrap_or_else(|| explicit_as_before_alias(statement_sql, rel_start));
    let tsql_equals_assignment = tokens
        .and_then(|tokens| tsql_assignment_after_alias_tokens(tokens, rel_end, rel_item_end))
        .unwrap_or_else(|| tsql_assignment_after_alias(statement_sql, rel_end, rel_item_end));
    Some(AliasOccurrence {
        start: rel_start,
        end: rel_end,
        explicit_as,
        tsql_equals_assignment,
    })
}

fn explicit_as_before_alias(statement_sql: &str, alias_start: usize) -> bool {
    if alias_start > statement_sql.len() {
        return false;
    }
    let prefix = &statement_sql[..alias_start];
    let trimmed = trim_trailing_trivia(prefix);
    trailing_word(trimmed)
        .map(|word| word.eq_ignore_ascii_case("as"))
        .unwrap_or(false)
}

fn tsql_assignment_after_alias(statement_sql: &str, alias_end: usize, item_end: usize) -> bool {
    if alias_end > item_end || item_end > statement_sql.len() {
        return false;
    }
    let suffix = &statement_sql[alias_end..item_end];
    strip_leading_trivia(suffix).starts_with('=')
}

fn trim_trailing_trivia(mut input: &str) -> &str {
    loop {
        let trimmed = input.trim_end_matches(char::is_whitespace);
        if trimmed.len() != input.len() {
            input = trimmed;
            continue;
        }

        if let Some(stripped) = strip_trailing_line_comment(input) {
            input = stripped;
            continue;
        }

        if let Some(stripped) = strip_trailing_block_comment(input) {
            input = stripped;
            continue;
        }

        return input;
    }
}

fn strip_trailing_line_comment(input: &str) -> Option<&str> {
    let line_start = input.rfind('\n').map_or(0, |idx| idx + 1);
    let tail = &input[line_start..];
    let comment_start = tail.rfind("--")?;
    Some(&input[..line_start + comment_start])
}

fn strip_trailing_block_comment(input: &str) -> Option<&str> {
    if !input.ends_with("*/") {
        return None;
    }
    let start = input.rfind("/*")?;
    Some(&input[..start])
}

fn trailing_word(input: &str) -> Option<&str> {
    let mut end = input.len();
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }

    let mut start = end;
    while start > 0 {
        let ch = input[..start].chars().next_back()?;
        if ch.is_ascii_alphanumeric() || ch == '_' {
            start -= ch.len_utf8();
        } else {
            break;
        }
    }

    (start < end).then_some(&input[start..end])
}

fn strip_leading_trivia(mut input: &str) -> &str {
    loop {
        let trimmed = input.trim_start_matches(char::is_whitespace);
        if trimmed.len() != input.len() {
            input = trimmed;
            continue;
        }
        if let Some(stripped) = strip_leading_line_comment(input) {
            input = stripped;
            continue;
        }
        if let Some(stripped) = strip_leading_block_comment(input) {
            input = stripped;
            continue;
        }
        return input;
    }
}

fn strip_leading_line_comment(input: &str) -> Option<&str> {
    let suffix = input.strip_prefix("--")?;
    let next_line = suffix
        .find('\n')
        .map_or(suffix.len(), |idx| idx + 1);
    Some(&suffix[next_line..])
}

fn strip_leading_block_comment(input: &str) -> Option<&str> {
    let suffix = input.strip_prefix("/*")?;
    let end = suffix.find("*/")?;
    Some(&suffix[end + 2..])
}

fn explicit_as_before_alias_tokens(tokens: &[LocatedToken], alias_start: usize) -> Option<bool> {
    let token = tokens
        .iter()
        .rev()
        .find(|token| token.end <= alias_start && !is_trivia_token(&token.token))?;
    Some(is_as_token(&token.token))
}

fn tsql_assignment_after_alias_tokens(
    tokens: &[LocatedToken],
    alias_end: usize,
    item_end: usize,
) -> Option<bool> {
    let token = tokens
        .iter()
        .find(|token| {
            token.start >= alias_end
                && token.end <= item_end
                && !is_trivia_token(&token.token)
        })?;
    Some(matches!(token.token, Token::Eq | Token::Assignment))
}

fn is_as_token(token: &Token) -> bool {
    match token {
        Token::Word(word) => word.value.eq_ignore_ascii_case("AS"),
        _ => false,
    }
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
        let (start, end) = token_with_span_offsets(sql, &token)?;
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
    ctx.with_document_tokens(|tokens| {
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
    })
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
        return Some(sql.len());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        parser::{parse_sql, parse_sql_with_dialect},
        Dialect,
    };

    fn run_with_rule(sql: &str, rule: AliasingColumnStyle) -> Vec<Issue> {
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

    fn run(sql: &str) -> Vec<Issue> {
        run_with_rule(sql, AliasingColumnStyle::default())
    }

    #[test]
    fn flags_implicit_column_alias() {
        let issues = run("select a + 1 total from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_002);
    }

    #[test]
    fn allows_explicit_column_alias() {
        let issues = run("select a + 1 as total from t");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_explicit_aliases_when_implicit_policy_requested() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.column".to_string(),
                serde_json::json!({"aliasing": "implicit"}),
            )]),
        };
        let issues = run_with_rule(
            "select a + 1 as total, b + 1 value from t",
            AliasingColumnStyle::from_config(&config),
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_002);
    }

    #[test]
    fn does_not_flag_alias_text_in_string_literal() {
        let issues = run("select 'a as label' as value from t");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_tsql_assignment_style_alias() {
        let sql = "select alias1 = col1";
        let statements = parse_sql_with_dialect(sql, Dialect::Mssql).expect("parse");
        let issues = AliasingColumnStyle::default().check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert!(issues.is_empty());
    }
}
