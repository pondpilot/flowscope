//! LINT_AL_002: Column alias style.
//!
//! SQLFluff parity: configurable column aliasing style (`explicit`/`implicit`).

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
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

impl BuiltinLintRule for AliasingColumnStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_002
    }

    fn name(&self) -> &'static str {
        "Column alias style"
    }

    fn description(&self) -> &'static str {
        "Implicit/explicit aliasing of columns."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        let tokens =
            tokenized_for_context(ctx).or_else(|| tokenized(ctx.statement_sql(), ctx.dialect()));

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

                let mut issue = Issue::info(issue_codes::LINT_AL_002, self.aliasing.message())
                    .with_statement(ctx.statement_index)
                    .with_span(ctx.span_from_statement_offset(occurrence.start, occurrence.end));
                if let Some(edits) = autofix_edits_for_occurrence(occurrence, self.aliasing) {
                    issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, edits);
                }
                issues.push(issue);
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
    as_span: Option<Span>,
    tsql_equals_assignment: bool,
}

fn autofix_edits_for_occurrence(
    occurrence: AliasOccurrence,
    aliasing: AliasingPreference,
) -> Option<Vec<IssuePatchEdit>> {
    match aliasing {
        AliasingPreference::Explicit if !occurrence.explicit_as => {
            let insert = Span::new(occurrence.start, occurrence.start);
            Some(vec![IssuePatchEdit::new(insert, "AS ")])
        }
        AliasingPreference::Implicit if occurrence.explicit_as => {
            let as_span = occurrence.as_span?;
            // Replace " AS " (leading whitespace + AS keyword + trailing whitespace)
            // with a single space to preserve separation between expression and alias.
            let delete_end = occurrence.start;
            Some(vec![IssuePatchEdit::new(
                Span::new(as_span.start, delete_end),
                " ",
            )])
        }
        _ => None,
    }
}

fn alias_occurrence_in_statement(
    alias: &Ident,
    item: &SelectItem,
    ctx: &RuleContext,
    tokens: Option<&[LocatedToken]>,
) -> Option<AliasOccurrence> {
    let tokens = tokens?;

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

    let (explicit_as, as_span) = explicit_as_before_alias_tokens(tokens, rel_start)?;
    let tsql_equals_assignment =
        tsql_assignment_after_alias_tokens(tokens, rel_end, rel_item_end).unwrap_or(false);
    Some(AliasOccurrence {
        start: rel_start,
        end: rel_end,
        explicit_as,
        as_span,
        tsql_equals_assignment,
    })
}

fn explicit_as_before_alias_tokens(
    tokens: &[LocatedToken],
    alias_start: usize,
) -> Option<(bool, Option<Span>)> {
    let token = tokens
        .iter()
        .rev()
        .find(|token| token.end <= alias_start && !is_trivia_token(&token.token))?;
    if is_as_token(&token.token) {
        // Include leading whitespace before AS in the span.
        let leading_ws_start = tokens
            .iter()
            .rev()
            .find(|t| t.end <= token.start && !is_trivia_token(&t.token))
            .map(|t| t.end)
            .unwrap_or(token.start);
        Some((true, Some(Span::new(leading_ws_start, token.end))))
    } else {
        Some((false, None))
    }
}

fn tsql_assignment_after_alias_tokens(
    tokens: &[LocatedToken],
    alias_end: usize,
    item_end: usize,
) -> Option<bool> {
    let token = tokens.iter().find(|token| {
        token.start >= alias_end && token.end <= item_end && !is_trivia_token(&token.token)
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

fn tokenized_for_context(ctx: &RuleContext) -> Option<Vec<LocatedToken>> {
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
        types::IssueAutofixApplicability,
        Dialect,
    };

    fn run_with_rule(sql: &str, rule: AliasingColumnStyle) -> Vec<Issue> {
        let stmts = parse_sql(sql).expect("parse");
        stmts
            .iter()
            .enumerate()
            .flat_map(|(index, stmt)| {
                rule.check_with_context(stmt, &RuleContext::new(sql, 0..sql.len(), index))
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
    fn explicit_mode_emits_safe_insert_as_autofix_patch() {
        let sql = "select a + 1 total from t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected AL002 core autofix");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        assert_eq!(autofix.edits[0].replacement, "AS ");
        assert_eq!(autofix.edits[0].span.start, autofix.edits[0].span.end);
    }

    #[test]
    fn implicit_mode_emits_safe_remove_as_autofix_patch() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.column".to_string(),
                serde_json::json!({"aliasing": "implicit"}),
            )]),
        };
        let rule = AliasingColumnStyle::from_config(&config);
        let sql = "select a + 1 as total from t";
        let issues = run_with_rule(sql, rule);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected AL002 core autofix in implicit mode");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        assert_eq!(autofix.edits[0].replacement, " ");
        // Span should cover " as " (leading whitespace + AS keyword + trailing whitespace).
        assert_eq!(
            &sql[autofix.edits[0].span.start..autofix.edits[0].span.end],
            " as "
        );
    }

    #[test]
    fn allows_tsql_assignment_style_alias() {
        let sql = "select alias1 = col1";
        let statements = parse_sql_with_dialect(sql, Dialect::Mssql).expect("parse");
        let issues = AliasingColumnStyle::default()
            .check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }
}
