//! LINT_CV_002: prefer COALESCE over IFNULL/NVL.
//!
//! SQLFluff CV02 parity: detect IFNULL/NVL function usage and recommend
//! COALESCE for portability and consistency.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::linter::visit;
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::{Expr, Statement};
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct CoalesceConvention;

impl BuiltinLintRule for CoalesceConvention {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_002
    }

    fn name(&self) -> &'static str {
        "COALESCE convention"
    }

    fn description(&self) -> &'static str {
        "Use 'COALESCE' instead of 'IFNULL' or 'NVL'."
    }

    fn check_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let function_name_spans =
            tokenized_for_context(ctx).or_else(|| tokenized(ctx.statement_sql(), ctx.dialect()));
        let function_name_spans = function_name_spans
            .as_deref()
            .map(collect_coalesce_function_name_spans)
            .unwrap_or_default();
        let mut span_index = 0usize;
        let mut issues = Vec::new();

        visit::visit_expressions(stmt, &mut |expr| {
            let Expr::Function(function) = expr else {
                return;
            };

            let function_name = function.name.to_string();
            let function_name_upper = function_name.to_ascii_uppercase();

            if function_name_upper != "IFNULL" && function_name_upper != "NVL" {
                return;
            }

            let mut issue = Issue::info(
                issue_codes::LINT_CV_002,
                format!("Use 'COALESCE' instead of '{}'.", function_name_upper),
            )
            .with_statement(ctx.statement_index);
            if let Some((start, end)) = function_name_spans.get(span_index).copied() {
                let span = ctx.span_from_statement_offset(start, end);
                issue = issue.with_span(span).with_autofix_edits(
                    IssueAutofixApplicability::Safe,
                    vec![IssuePatchEdit::new(span, "COALESCE")],
                );
            }
            span_index = span_index.saturating_add(1);
            issues.push(issue);
        });

        issues
    }
}

fn collect_coalesce_function_name_spans(tokens: &[LocatedToken]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0usize;
    while i < tokens.len() {
        let Token::Word(word) = &tokens[i].token else {
            i += 1;
            continue;
        };
        if !word.value.eq_ignore_ascii_case("IFNULL") && !word.value.eq_ignore_ascii_case("NVL") {
            i += 1;
            continue;
        }

        let mut j = i + 1;
        skip_trivia_tokens(tokens, &mut j);
        if j >= tokens.len() || !matches!(tokens[j].token, Token::LParen) {
            i += 1;
            continue;
        }

        spans.push((tokens[i].start, tokens[i].end));
        i = j + 1;
    }

    spans
}

#[derive(Debug, Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn tokenized_for_context(ctx: &RuleContext) -> Option<Vec<LocatedToken>> {
    let tokens = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        Some(
            tokens
                .iter()
                .filter_map(|token| {
                    token_with_span_offsets(ctx.sql, token).map(|(start, end)| LocatedToken {
                        token: token.token.clone(),
                        start,
                        end,
                    })
                })
                .collect::<Vec<_>>(),
        )
    });

    if let Some(tokens) = tokens {
        return Some(tokens);
    }

    tokenized(ctx.sql, ctx.dialect())
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

fn skip_trivia_tokens(tokens: &[LocatedToken], index: &mut usize) {
    while *index < tokens.len() {
        if !is_trivia_token(&tokens[*index].token) {
            break;
        }
        *index += 1;
    }
}

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CoalesceConvention;
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    // --- Edge cases adopted from sqlfluff CV02 ---

    #[test]
    fn passes_coalesce() {
        let issues = run("SELECT coalesce(foo, 0) AS bar FROM baz");
        assert!(issues.is_empty());
    }

    #[test]
    fn fails_ifnull() {
        let issues = run("SELECT ifnull(foo, 0) AS bar FROM baz");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_002);
        let fix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(fix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].replacement, "COALESCE");
    }

    #[test]
    fn fails_nvl() {
        let issues = run("SELECT nvl(foo, 0) AS bar FROM baz");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_002);
        let fix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(fix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].replacement, "COALESCE");
    }

    #[test]
    fn does_not_flag_case_when_null_pattern_anymore() {
        let issues = run("SELECT CASE WHEN x IS NULL THEN 'default' ELSE x END FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_nested_ifnull_calls() {
        let issues = run("SELECT SUM(IFNULL(amount, 0)) AS total FROM orders");
        assert_eq!(issues.len(), 1);
    }
}
