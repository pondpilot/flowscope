//! LINT_AM_005: Ambiguous JOIN style.
//!
//! Require explicit JOIN type keywords (`INNER`, `LEFT`, etc.) instead of bare
//! `JOIN` for clearer intent.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::{JoinOperator, Select, Statement};
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

use super::semantic_helpers::visit_selects_in_statement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FullyQualifyJoinTypes {
    Inner,
    Outer,
    Both,
}

impl FullyQualifyJoinTypes {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_AM_005, "fully_qualify_join_types")
            .unwrap_or("inner")
            .to_ascii_lowercase()
            .as_str()
        {
            "outer" => Self::Outer,
            "both" => Self::Both,
            _ => Self::Inner,
        }
    }
}

pub struct AmbiguousJoinStyle {
    qualify_mode: FullyQualifyJoinTypes,
}

impl AmbiguousJoinStyle {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            qualify_mode: FullyQualifyJoinTypes::from_config(config),
        }
    }
}

impl Default for AmbiguousJoinStyle {
    fn default() -> Self {
        Self {
            qualify_mode: FullyQualifyJoinTypes::Inner,
        }
    }
}

impl BuiltinLintRule for AmbiguousJoinStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_005
    }

    fn name(&self) -> &'static str {
        "Ambiguous join style"
    }

    fn description(&self) -> &'static str {
        "Join clauses should be fully qualified."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut plain_join_count = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            for table in &select.from {
                for join in &table.joins {
                    if matches!(join.join_operator, JoinOperator::Join(_)) {
                        plain_join_count += 1;
                    }
                }
            }
        });

        let outer_unqualified_count = count_unqualified_outer_joins(statement, ctx);
        let violation_count = match self.qualify_mode {
            FullyQualifyJoinTypes::Inner => plain_join_count,
            FullyQualifyJoinTypes::Outer => outer_unqualified_count,
            FullyQualifyJoinTypes::Both => plain_join_count + outer_unqualified_count,
        };
        let mut autofix_candidates = am005_autofix_candidates_for_context(ctx, self.qualify_mode);
        autofix_candidates.sort_by_key(|candidate| candidate.span.start);
        let candidates_align = autofix_candidates.len() == violation_count;

        (0..violation_count)
            .map(|index| {
                let mut issue = Issue::warning(
                    issue_codes::LINT_AM_005,
                    "Join clauses should be fully qualified.",
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
struct Am005AutofixCandidate {
    span: Span,
    edits: Vec<IssuePatchEdit>,
}

fn am005_autofix_candidates_for_context(
    ctx: &RuleContext,
    qualify_mode: FullyQualifyJoinTypes,
) -> Vec<Am005AutofixCandidate> {
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

    if let Some(positioned) = from_document_tokens {
        return am005_autofix_candidates_from_positioned_tokens(&positioned, qualify_mode);
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

    am005_autofix_candidates_from_positioned_tokens(&positioned, qualify_mode)
}

fn am005_autofix_candidates_from_positioned_tokens(
    tokens: &[PositionedToken],
    qualify_mode: FullyQualifyJoinTypes,
) -> Vec<Am005AutofixCandidate> {
    let significant_indexes: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| (!is_trivia(&token.token)).then_some(index))
        .collect();

    let mut candidates = Vec::new();

    for (position, token_index) in significant_indexes.iter().copied().enumerate() {
        if !token_word_equals(&tokens[token_index].token, "JOIN") {
            continue;
        }

        let previous = position
            .checked_sub(1)
            .and_then(|index| significant_indexes.get(index))
            .copied();
        let previous_previous = position
            .checked_sub(2)
            .and_then(|index| significant_indexes.get(index))
            .copied();

        let has_explicit_outer = previous.is_some_and(|index| {
            token_word_equals(&tokens[index].token, "OUTER")
                && previous_previous
                    .is_some_and(|inner| is_outer_join_side_keyword(&tokens[inner].token))
        });
        let requires_outer_keyword = !has_explicit_outer
            && previous.is_some_and(|index| is_outer_join_side_keyword(&tokens[index].token));
        let is_plain = is_plain_join_sequence(tokens, previous, previous_previous);

        let join_token = &tokens[token_index];
        let source_is_lower =
            token_word_value(&join_token.token).is_some_and(|v| v == v.to_ascii_lowercase());

        let needs_inner = match qualify_mode {
            FullyQualifyJoinTypes::Inner | FullyQualifyJoinTypes::Both => is_plain,
            FullyQualifyJoinTypes::Outer => false,
        };
        let needs_outer = match qualify_mode {
            FullyQualifyJoinTypes::Outer | FullyQualifyJoinTypes::Both => requires_outer_keyword,
            FullyQualifyJoinTypes::Inner => false,
        };

        if needs_inner {
            let replacement = if source_is_lower {
                "inner join"
            } else {
                "INNER JOIN"
            };
            let span = Span::new(join_token.start, join_token.end);
            candidates.push(Am005AutofixCandidate {
                span,
                edits: vec![IssuePatchEdit::new(span, replacement)],
            });
        } else if needs_outer {
            // Insert OUTER keyword before JOIN, preserving case.
            // Only replace the JOIN token span with "OUTER JOIN" to avoid
            // expanding the edit span over the side keyword (LEFT/RIGHT/FULL).
            let outer_kw = if source_is_lower { "outer" } else { "OUTER" };
            let join_kw = if source_is_lower { "join" } else { "JOIN" };
            let replacement = format!("{outer_kw} {join_kw}");
            let span = Span::new(join_token.start, join_token.end);
            candidates.push(Am005AutofixCandidate {
                span,
                edits: vec![IssuePatchEdit::new(span, &replacement)],
            });
        } else {
            continue;
        }
    }

    candidates
}

fn is_plain_join_sequence(
    tokens: &[PositionedToken],
    previous: Option<usize>,
    previous_previous: Option<usize>,
) -> bool {
    let Some(previous) = previous else {
        return false;
    };

    if token_word_equals(&tokens[previous].token, "OUTER")
        && previous_previous.is_some_and(|index| is_outer_join_side_keyword(&tokens[index].token))
    {
        return false;
    }

    if is_outer_join_side_keyword(&tokens[previous].token)
        || token_word_equals(&tokens[previous].token, "INNER")
        || token_word_equals(&tokens[previous].token, "CROSS")
        || token_word_equals(&tokens[previous].token, "SEMI")
        || token_word_equals(&tokens[previous].token, "ANTI")
        || token_word_equals(&tokens[previous].token, "ASOF")
        || token_word_equals(&tokens[previous].token, "OUTER")
        || token_word_equals(&tokens[previous].token, "APPLY")
        || token_word_equals(&tokens[previous].token, "STRAIGHT")
        || token_word_equals(&tokens[previous].token, "STRAIGHT_JOIN")
    {
        return false;
    }

    true
}

fn token_word_equals(token: &Token, expected_upper: &str) -> bool {
    matches!(token, Token::Word(word) if word.value.eq_ignore_ascii_case(expected_upper))
}

fn token_word_value(token: &Token) -> Option<&str> {
    match token {
        Token::Word(word) => Some(&word.value),
        _ => None,
    }
}

fn is_outer_join_side_keyword(token: &Token) -> bool {
    token_word_equals(token, "LEFT")
        || token_word_equals(token, "RIGHT")
        || token_word_equals(token, "FULL")
}

fn count_unqualified_outer_joins(statement: &Statement, ctx: &RuleContext) -> usize {
    count_unqualified_left_right_outer_joins(statement)
        + count_unqualified_full_outer_joins(statement, ctx)
}

fn count_unqualified_left_right_outer_joins(statement: &Statement) -> usize {
    let mut count = 0usize;

    visit_selects_in_statement(statement, &mut |select| {
        count += select_unqualified_left_right_outer_join_count(select);
    });

    count
}

fn select_unqualified_left_right_outer_join_count(select: &Select) -> usize {
    select
        .from
        .iter()
        .map(|table| {
            table
                .joins
                .iter()
                .filter(|join| {
                    matches!(
                        join.join_operator,
                        JoinOperator::Left(_) | JoinOperator::Right(_)
                    )
                })
                .count()
        })
        .sum()
}

fn count_unqualified_full_outer_joins(statement: &Statement, ctx: &RuleContext) -> usize {
    let full_outer_join_count = count_full_outer_joins(statement);
    if full_outer_join_count == 0 {
        return 0;
    }

    let explicit_full_outer_count = count_explicit_full_outer_joins_for_context(ctx);
    full_outer_join_count.saturating_sub(explicit_full_outer_count)
}

fn count_full_outer_joins(statement: &Statement) -> usize {
    let mut count = 0usize;
    visit_selects_in_statement(statement, &mut |select| {
        for table in &select.from {
            for join in &table.joins {
                if matches!(join.join_operator, JoinOperator::FullOuter(_)) {
                    count += 1;
                }
            }
        }
    });
    count
}

fn count_explicit_full_outer_joins(sql: &str, dialect: Dialect) -> usize {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return 0;
    };

    count_explicit_full_outer_joins_from_tokens(&tokens)
}

fn count_explicit_full_outer_joins_for_context(ctx: &RuleContext) -> usize {
    let from_document_tokens = ctx.with_document_tokens(|tokens| {
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
                    Some(token.token.clone())
                })
                .collect::<Vec<_>>(),
        )
    });

    if let Some(tokens) = from_document_tokens {
        return count_explicit_full_outer_joins_from_tokens(&tokens);
    }

    count_explicit_full_outer_joins(ctx.statement_sql(), ctx.dialect())
}

fn count_explicit_full_outer_joins_from_tokens(tokens: &[Token]) -> usize {
    let significant: Vec<&Token> = tokens.iter().filter(|token| !is_trivia(token)).collect();

    let mut count = 0usize;
    let mut idx = 0usize;
    while idx < significant.len() {
        let Token::Word(word) = significant[idx] else {
            idx += 1;
            continue;
        };

        if word.keyword != Keyword::FULL {
            idx += 1;
            continue;
        }

        let Some(next) = significant.get(idx + 1) else {
            break;
        };

        match next {
            Token::Word(next_word) if next_word.keyword == Keyword::OUTER => {
                if matches!(
                    significant.get(idx + 2),
                    Some(Token::Word(join_word)) if join_word.keyword == Keyword::JOIN
                ) {
                    count += 1;
                    idx += 3;
                } else {
                    idx += 2;
                }
            }
            _ => idx += 1,
        }
    }

    count
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AmbiguousJoinStyle::default();
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    // --- Edge cases adopted from sqlfluff AM05 ---

    #[test]
    fn flags_plain_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo JOIN bar");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_005);
    }

    #[test]
    fn flags_lowercase_plain_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo join bar");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_inner_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo INNER JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_left_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo LEFT JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_right_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo RIGHT JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_full_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo FULL JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_left_outer_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo LEFT OUTER JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_right_outer_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo RIGHT OUTER JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_full_outer_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo FULL OUTER JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_cross_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo CROSS JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_each_plain_join_in_chain() {
        let issues = run("SELECT * FROM a JOIN b ON a.id = b.id JOIN c ON b.id = c.id");
        assert_eq!(issues.len(), 2);
        assert!(issues
            .iter()
            .all(|issue| issue.code == issue_codes::LINT_AM_005));
    }

    #[test]
    fn outer_mode_flags_left_join_without_outer_keyword() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "ambiguous.join".to_string(),
                serde_json::json!({"fully_qualify_join_types": "outer"}),
            )]),
        };
        let rule = AmbiguousJoinStyle::from_config(&config);
        let sql = "SELECT foo.a, bar.b FROM foo LEFT JOIN bar ON foo.id = bar.id";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn outer_mode_allows_left_outer_join() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_AM_005".to_string(),
                serde_json::json!({"fully_qualify_join_types": "outer"}),
            )]),
        };
        let rule = AmbiguousJoinStyle::from_config(&config);
        let sql = "SELECT foo.a, bar.b FROM foo LEFT OUTER JOIN bar ON foo.id = bar.id";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn outer_mode_flags_right_join_without_outer_keyword() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "ambiguous.join".to_string(),
                serde_json::json!({"fully_qualify_join_types": "outer"}),
            )]),
        };
        let rule = AmbiguousJoinStyle::from_config(&config);
        let sql = "SELECT foo.a, bar.b FROM foo RIGHT JOIN bar ON foo.id = bar.id";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn outer_mode_allows_right_outer_join() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "ambiguous.join".to_string(),
                serde_json::json!({"fully_qualify_join_types": "outer"}),
            )]),
        };
        let rule = AmbiguousJoinStyle::from_config(&config);
        let sql = "SELECT foo.a, bar.b FROM foo RIGHT OUTER JOIN bar ON foo.id = bar.id";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn outer_mode_flags_full_join_without_outer_keyword() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "ambiguous.join".to_string(),
                serde_json::json!({"fully_qualify_join_types": "outer"}),
            )]),
        };
        let rule = AmbiguousJoinStyle::from_config(&config);
        let sql = "SELECT foo.a, bar.b FROM foo FULL JOIN bar ON foo.id = bar.id";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn outer_mode_allows_full_outer_join() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "ambiguous.join".to_string(),
                serde_json::json!({"fully_qualify_join_types": "outer"}),
            )]),
        };
        let rule = AmbiguousJoinStyle::from_config(&config);
        let sql = "SELECT foo.a, bar.b FROM foo FULL OUTER JOIN bar ON foo.id = bar.id";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn outer_mode_flags_only_unqualified_full_joins_in_mixed_chains() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "ambiguous.join".to_string(),
                serde_json::json!({"fully_qualify_join_types": "outer"}),
            )]),
        };
        let rule = AmbiguousJoinStyle::from_config(&config);
        let sql = "SELECT * FROM a FULL JOIN b ON a.id = b.id FULL OUTER JOIN c ON b.id = c.id";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_005);
    }

    #[test]
    fn inner_mode_plain_join_emits_safe_autofix_patch() {
        let sql = "SELECT a FROM t JOIN u ON t.id = u.id";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected AM005 core autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        assert_eq!(autofix.edits[0].replacement, "INNER JOIN");
        assert_eq!(
            &sql[autofix.edits[0].span.start..autofix.edits[0].span.end],
            "JOIN"
        );
    }

    #[test]
    fn outer_mode_full_join_emits_safe_outer_keyword_patch() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "ambiguous.join".to_string(),
                serde_json::json!({"fully_qualify_join_types": "outer"}),
            )]),
        };
        let rule = AmbiguousJoinStyle::from_config(&config);
        let sql = "SELECT a FROM t FULL JOIN u ON t.id = u.id";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected AM005 full join core autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        assert_eq!(autofix.edits[0].replacement, "OUTER JOIN");
        assert_eq!(
            &sql[autofix.edits[0].span.start..autofix.edits[0].span.end],
            "JOIN"
        );
    }

    #[test]
    fn inner_mode_lowercase_join_preserves_case() {
        let sql = "SELECT a FROM t join u ON t.id = u.id\n";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("expected AM005 autofix");
        assert_eq!(autofix.edits[0].replacement, "inner join");
    }
}
