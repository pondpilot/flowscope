//! LINT_ST_012: Structure consecutive semicolons.
//!
//! SQLFluff ST12 parity (current scope): detect consecutive semicolons in the
//! document text.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct StructureConsecutiveSemicolons;

impl BuiltinLintRule for StructureConsecutiveSemicolons {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_012
    }

    fn name(&self) -> &'static str {
        "Structure consecutive semicolons"
    }

    fn description(&self) -> &'static str {
        "Consecutive semicolons detected."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        if ctx.statement_index > 0 {
            Vec::new()
        } else {
            let tokens = tokenize_with_offsets_for_context(ctx);
            let Some(fix) = consecutive_semicolon_fix(ctx.sql, ctx.dialect(), tokens.as_deref())
            else {
                return Vec::new();
            };

            let edits = fix
                .remove_spans
                .into_iter()
                .map(|(start, end)| IssuePatchEdit::new(Span::new(start, end), ""))
                .collect();

            vec![
                Issue::warning(issue_codes::LINT_ST_012, "Consecutive semicolons detected.")
                    .with_statement(ctx.statement_index)
                    .with_span(Span::new(fix.issue_start, fix.issue_end))
                    .with_autofix_edits(IssueAutofixApplicability::Safe, edits),
            ]
        }
    }
}

#[derive(Debug)]
struct ConsecutiveSemicolonFix {
    issue_start: usize,
    issue_end: usize,
    remove_spans: Vec<(usize, usize)>,
}

fn consecutive_semicolon_fix(
    sql: &str,
    dialect: Dialect,
    tokens: Option<&[LocatedToken]>,
) -> Option<ConsecutiveSemicolonFix> {
    let owned_tokens;
    let tokens = if let Some(tokens) = tokens {
        tokens
    } else {
        owned_tokens = tokenize_with_offsets(sql, dialect)?;
        &owned_tokens
    };

    let mut previous_semicolon_seen = false;
    let mut remove_spans = Vec::new();

    for token in tokens {
        if is_trivia_token(&token.token) {
            continue;
        }

        if matches!(token.token, Token::SemiColon) {
            if previous_semicolon_seen {
                remove_spans.push((token.start, token.end));
            } else {
                previous_semicolon_seen = true;
            }
        } else {
            previous_semicolon_seen = false;
        }
    }

    let (issue_start, issue_end) = remove_spans.first().copied()?;
    Some(ConsecutiveSemicolonFix {
        issue_start,
        issue_end,
        remove_spans,
    })
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn tokenize_with_offsets(sql: &str, dialect: Dialect) -> Option<Vec<LocatedToken>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
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
        out.push(LocatedToken {
            token: token.token,
            start,
            end,
        });
    }

    Some(out)
}

fn tokenize_with_offsets_for_context(ctx: &RuleContext) -> Option<Vec<LocatedToken>> {
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

    tokenize_with_offsets(ctx.sql, ctx.dialect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_sql, parse_sql_with_dialect};
    use crate::types::{Dialect, IssueAutofixApplicability};

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = StructureConsecutiveSemicolons;
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn run_in_dialect(sql: &str, dialect: Dialect) -> Vec<Issue> {
        let statements = parse_sql_with_dialect(sql, dialect).expect("parse");
        let rule = StructureConsecutiveSemicolons;
        let mut issues = Vec::new();

        for (index, statement) in statements.iter().enumerate() {
            issues.extend(rule.check_with_context(
                statement,
                &RuleContext::new(sql, 0..sql.len(), index).with_dialect(dialect),
            ));
        }

        issues
    }

    fn apply_issue_autofix(sql: &str, issue: &Issue) -> Option<String> {
        let autofix = issue.autofix.as_ref()?;
        let mut out = sql.to_string();
        let mut edits = autofix.edits.clone();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        for edit in edits.iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    #[test]
    fn flags_consecutive_semicolons() {
        let issues = run("SELECT 1;;");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_012);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix("SELECT 1;;", &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT 1;");
    }

    #[test]
    fn does_not_flag_single_semicolon() {
        let issues = run("SELECT 1;");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_semicolons_inside_string_literal() {
        let issues = run("SELECT 'a;;b';");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_semicolons_inside_comments() {
        let issues = run("SELECT 1 /* ;; */;");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_consecutive_semicolons_separated_by_comment() {
        let sql = "SELECT 1; /* keep */ ;";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_012);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert!(
            fixed.contains("/* keep */"),
            "comment should be preserved after ST012 autofix: {fixed}"
        );
        assert_eq!(fixed.matches(';').count(), 1);
    }

    #[test]
    fn does_not_flag_normal_statement_separator() {
        let issues = run("SELECT 1; SELECT 2;");
        assert!(issues.is_empty());
    }

    #[test]
    fn mysql_hash_comment_is_treated_as_trivia() {
        let sql = "SELECT 1; # dialect-specific comment\n;";
        assert!(consecutive_semicolon_fix(sql, Dialect::Generic, None).is_none());
        assert!(consecutive_semicolon_fix(sql, Dialect::Mysql, None).is_some());

        let issues = run_in_dialect(sql, Dialect::Mysql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_012);
    }
}
