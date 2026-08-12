//! LINT_ST_001: Unnecessary ELSE NULL in CASE expressions.
//!
//! `CASE ... ELSE NULL END` is redundant because CASE already returns NULL
//! when no branch matches. The ELSE NULL can be removed.

use crate::linter::helpers;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::linter::visit;
use crate::types::{issue_codes, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::*;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct UnnecessaryElseNull;

impl BuiltinLintRule for UnnecessaryElseNull {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_001
    }

    fn name(&self) -> &'static str {
        "Unnecessary ELSE NULL"
    }

    fn description(&self) -> &'static str {
        "Do not specify 'else null' in a case when statement (redundant)."
    }

    fn check_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut violation_count = 0usize;
        visit::visit_expressions(stmt, &mut |expr| {
            if let Expr::Case {
                else_result: Some(else_expr),
                ..
            } = expr
            {
                if helpers::is_null_expr(else_expr) {
                    violation_count += 1;
                }
            }
        });
        let mut autofix_candidates = st001_else_null_candidates_for_context(ctx);
        autofix_candidates.sort_by_key(|candidate| candidate.span.start);
        let candidates_align = autofix_candidates.len() == violation_count;

        (0..violation_count)
            .map(|index| {
                let mut issue = Issue::info(
                    issue_codes::LINT_ST_001,
                    "ELSE NULL is redundant in CASE expressions; it can be removed.",
                )
                .with_statement(ctx.statement_index);

                if candidates_align {
                    let candidate = &autofix_candidates[index];
                    issue = issue.with_span(candidate.span).with_autofix_edits(
                        IssueAutofixApplicability::Safe,
                        candidate.edits.clone(),
                    );
                }

                issue
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
struct PositionedToken {
    token: Token,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug)]
struct St001AutofixCandidate {
    span: Span,
    edits: Vec<IssuePatchEdit>,
}

#[derive(Clone, Copy, Debug)]
struct CaseFrame {
    else_sig_pos: Option<usize>,
}

fn st001_else_null_candidates_for_context(ctx: &RuleContext) -> Vec<St001AutofixCandidate> {
    let tokens = statement_positioned_tokens(ctx);
    if tokens.is_empty() {
        return Vec::new();
    }

    st001_else_null_candidates_from_tokens(&tokens)
}

fn statement_positioned_tokens(ctx: &RuleContext) -> Vec<PositionedToken> {
    let from_document_tokens = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut positioned = Vec::new();
        for token in tokens {
            let (start, end) = token_with_span_offsets(ctx.sql, token)?;
            if start < ctx.statement_range.start || end > ctx.statement_range.end {
                continue;
            }

            positioned.push(PositionedToken {
                token: token.token.clone(),
                start,
                end,
            });
        }

        Some(positioned)
    });

    if let Some(tokens) = from_document_tokens {
        return tokens;
    }

    let dialect = ctx.dialect().to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), ctx.statement_sql());
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return Vec::new();
    };

    let mut positioned = Vec::new();
    for token in &tokens {
        let Some((start, end)) = token_with_span_offsets(ctx.statement_sql(), token) else {
            continue;
        };
        positioned.push(PositionedToken {
            token: token.token.clone(),
            start: ctx.statement_range.start + start,
            end: ctx.statement_range.start + end,
        });
    }

    positioned
}

fn st001_else_null_candidates_from_tokens(
    tokens: &[PositionedToken],
) -> Vec<St001AutofixCandidate> {
    let significant_positions: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| (!is_trivia(&token.token)).then_some(index))
        .collect();

    let mut candidates = Vec::new();
    let mut case_stack: Vec<CaseFrame> = Vec::new();

    for (sig_pos, token_index) in significant_positions.iter().copied().enumerate() {
        if token_word_equals(&tokens[token_index].token, "CASE") {
            case_stack.push(CaseFrame { else_sig_pos: None });
            continue;
        }

        if token_word_equals(&tokens[token_index].token, "ELSE") {
            if let Some(frame) = case_stack.last_mut() {
                frame.else_sig_pos = Some(sig_pos);
            }
            continue;
        }

        if !token_word_equals(&tokens[token_index].token, "END") {
            continue;
        }

        let Some(frame) = case_stack.pop() else {
            continue;
        };
        let Some(else_sig_pos) = frame.else_sig_pos else {
            continue;
        };
        if else_sig_pos + 1 >= sig_pos {
            continue;
        }

        let null_token_index = significant_positions[else_sig_pos + 1];
        if else_sig_pos + 2 != sig_pos
            || !token_word_equals(&tokens[null_token_index].token, "NULL")
        {
            continue;
        }

        if else_sig_pos == 0 {
            continue;
        }

        let else_token_index = significant_positions[else_sig_pos];
        let previous_sig_token_index = significant_positions[else_sig_pos - 1];
        let removal_start_token_index = previous_sig_token_index.saturating_add(1);

        if removal_start_token_index > else_token_index
            || removal_start_token_index >= tokens.len()
            || trivia_contains_comment(tokens, removal_start_token_index, null_token_index + 1)
        {
            continue;
        }

        let removal_span = Span::new(
            tokens[removal_start_token_index].start,
            tokens[null_token_index].end,
        );
        candidates.push(St001AutofixCandidate {
            span: removal_span,
            edits: vec![IssuePatchEdit::new(removal_span, "")],
        });
    }

    candidates
}

fn token_word_equals(token: &Token, expected_upper: &str) -> bool {
    matches!(token, Token::Word(word) if word.value.eq_ignore_ascii_case(expected_upper))
}

fn is_trivia(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(
            Whitespace::Space
                | Whitespace::Newline
                | Whitespace::Tab
                | Whitespace::SingleLineComment { .. }
                | Whitespace::MultiLineComment(_)
        )
    )
}

fn trivia_contains_comment(tokens: &[PositionedToken], start: usize, end: usize) -> bool {
    if start >= end {
        return false;
    }

    tokens[start..end].iter().any(|token| {
        matches!(
            token.token,
            Token::Whitespace(
                Whitespace::SingleLineComment { .. } | Whitespace::MultiLineComment(_)
            )
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
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = UnnecessaryElseNull;
        let ctx = RuleContext::new(sql, 0..sql.len(), 0);
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check_with_context(stmt, &ctx));
        }
        issues
    }

    #[test]
    fn test_else_null_detected() {
        let issues = check_sql("SELECT CASE WHEN x > 1 THEN 'a' ELSE NULL END FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_ST_001");
    }

    #[test]
    fn test_no_else_ok() {
        let issues = check_sql("SELECT CASE WHEN x > 1 THEN 'a' END FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_else_value_ok() {
        let issues = check_sql("SELECT CASE WHEN x > 1 THEN 'a' ELSE 'b' END FROM t");
        assert!(issues.is_empty());
    }

    // --- Edge cases adopted from sqlfluff ST01 (structure.else_null) ---

    #[test]
    fn test_simple_case_else_null() {
        // CASE x WHEN ... ELSE NULL END
        let issues = check_sql(
            "SELECT CASE name WHEN 'cat' THEN 'meow' WHEN 'dog' THEN 'woof' ELSE NULL END FROM t",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_else_with_complex_expression_ok() {
        let issues =
            check_sql("SELECT CASE name WHEN 'cat' THEN 'meow' ELSE UPPER(name) END FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_multiple_when_branches_else_null() {
        let issues = check_sql(
            "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' WHEN x = 3 THEN 'c' ELSE NULL END FROM t",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_nested_case_else_null() {
        // Both the inner and outer CASE have ELSE NULL
        let issues = check_sql(
            "SELECT CASE WHEN x > 0 THEN CASE WHEN y > 0 THEN 'pos' ELSE NULL END ELSE NULL END FROM t",
        );
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_else_null_in_where_clause() {
        let issues =
            check_sql("SELECT * FROM t WHERE (CASE WHEN x > 0 THEN 1 ELSE NULL END) IS NOT NULL");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_else_null_in_cte() {
        let issues = check_sql(
            "WITH cte AS (SELECT CASE WHEN x > 0 THEN 'yes' ELSE NULL END AS flag FROM t) SELECT * FROM cte",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_else_null_emits_safe_autofix_patch() {
        let sql = "SELECT CASE WHEN x > 1 THEN 'a' ELSE NULL END FROM t";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected ST001 core autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);

        let edit = &autofix.edits[0];
        let rewritten = format!(
            "{}{}{}",
            &sql[..edit.span.start],
            edit.replacement,
            &sql[edit.span.end..]
        );
        assert_eq!(rewritten, "SELECT CASE WHEN x > 1 THEN 'a' END FROM t");
    }
}
