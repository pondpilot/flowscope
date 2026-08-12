//! LINT_AM_008: Ambiguous JOIN condition.
//!
//! SQLFluff AM08 parity: detect implicit cross joins where JOIN-like operators
//! omit ON/USING/NATURAL conditions.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::{JoinConstraint, JoinOperator, Select, Statement, TableFactor};
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

use super::semantic_helpers::visit_selects_in_statement;

pub struct AmbiguousJoinCondition;

impl BuiltinLintRule for AmbiguousJoinCondition {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_008
    }

    fn name(&self) -> &'static str {
        "Ambiguous join condition"
    }

    fn description(&self) -> &'static str {
        "Implicit cross join detected."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut violations = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            violations += count_implicit_cross_join_violations(select);
        });

        // Subtract join types the AST doesn't distinguish but the token
        // scanner can identify (e.g., DuckDB POSITIONAL JOIN).
        let positional_joins = count_positional_joins_in_context(ctx);
        violations = violations.saturating_sub(positional_joins);

        let mut autofix_candidates = am008_autofix_candidates_for_context(ctx);
        autofix_candidates.sort_by_key(|candidate| candidate.span.start);
        let candidates_align = autofix_candidates.len() == violations;

        (0..violations)
            .map(|index| {
                let mut issue =
                    Issue::warning(issue_codes::LINT_AM_008, "Implicit cross join detected.")
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

fn count_implicit_cross_join_violations(select: &Select) -> usize {
    // SQLFluff AM08 defers JOIN+WHERE patterns to CV12.
    if select.selection.is_some() {
        return 0;
    }

    let mut violations = 0usize;

    for table in &select.from {
        for join in &table.joins {
            if !operator_requires_join_condition(&join.join_operator) {
                continue;
            }

            if join_constraint_is_explicit(&join.join_operator) {
                continue;
            }

            if is_unnest_join_target(&join.relation) {
                continue;
            }

            violations += 1;
        }
    }

    violations
}

fn operator_requires_join_condition(join_operator: &JoinOperator) -> bool {
    matches!(
        join_operator,
        JoinOperator::Join(_)
            | JoinOperator::Inner(_)
            | JoinOperator::Left(_)
            | JoinOperator::LeftOuter(_)
            | JoinOperator::Right(_)
            | JoinOperator::RightOuter(_)
            | JoinOperator::FullOuter(_)
            | JoinOperator::StraightJoin(_)
    )
}

fn join_constraint_is_explicit(join_operator: &JoinOperator) -> bool {
    let Some(constraint) = join_constraint(join_operator) else {
        return false;
    };

    matches!(
        constraint,
        JoinConstraint::On(_) | JoinConstraint::Using(_) | JoinConstraint::Natural
    )
}

fn join_constraint(join_operator: &JoinOperator) -> Option<&JoinConstraint> {
    match join_operator {
        JoinOperator::Join(constraint)
        | JoinOperator::Inner(constraint)
        | JoinOperator::Left(constraint)
        | JoinOperator::LeftOuter(constraint)
        | JoinOperator::Right(constraint)
        | JoinOperator::RightOuter(constraint)
        | JoinOperator::FullOuter(constraint)
        | JoinOperator::CrossJoin(constraint)
        | JoinOperator::Semi(constraint)
        | JoinOperator::LeftSemi(constraint)
        | JoinOperator::RightSemi(constraint)
        | JoinOperator::Anti(constraint)
        | JoinOperator::LeftAnti(constraint)
        | JoinOperator::RightAnti(constraint)
        | JoinOperator::StraightJoin(constraint) => Some(constraint),
        JoinOperator::AsOf { constraint, .. } => Some(constraint),
        JoinOperator::CrossApply | JoinOperator::OuterApply => None,
    }
}

fn is_unnest_join_target(table_factor: &TableFactor) -> bool {
    matches!(table_factor, TableFactor::UNNEST { .. })
}

#[derive(Clone, Debug)]
struct PositionedToken {
    token: Token,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug)]
struct Am008AutofixCandidate {
    span: Span,
    edits: Vec<IssuePatchEdit>,
}

#[derive(Clone, Copy, Debug)]
struct JoinOperatorTokenSpan {
    start_position: usize,
    end_position: usize,
}

/// Count `POSITIONAL JOIN` occurrences in the statement's token stream.
///
/// sqlparser doesn't model POSITIONAL JOIN as a distinct operator — it parses
/// `POSITIONAL` as a table alias and `JOIN` as a bare join. This function
/// detects the pattern at the token level so the AST violation count can be
/// corrected.
fn count_positional_joins_in_context(ctx: &RuleContext) -> usize {
    let sql = ctx.statement_sql();
    // Quick textual check to avoid tokenization when not needed.
    if !sql.to_ascii_uppercase().contains("POSITIONAL") {
        return 0;
    }

    let Some(tokens) = tokenize_with_spans(sql, ctx.dialect()) else {
        return 0;
    };
    let mut count = 0;
    let mut prev_word: Option<String> = None;
    for token in &tokens {
        match &token.token {
            Token::Word(w) => {
                let upper = w.value.to_ascii_uppercase();
                if upper == "JOIN" && prev_word.as_deref() == Some("POSITIONAL") {
                    count += 1;
                }
                prev_word = Some(upper);
            }
            t if is_trivia(t) => {}
            _ => {
                prev_word = None;
            }
        }
    }
    count
}

fn am008_autofix_candidates_for_context(ctx: &RuleContext) -> Vec<Am008AutofixCandidate> {
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
        return am008_autofix_candidates_from_positioned_tokens(&tokens);
    }

    let Some(tokens) = tokenize_with_spans(ctx.statement_sql(), ctx.dialect()) else {
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

    am008_autofix_candidates_from_positioned_tokens(&positioned)
}

fn am008_autofix_candidates_from_positioned_tokens(
    tokens: &[PositionedToken],
) -> Vec<Am008AutofixCandidate> {
    if tokens.is_empty() {
        return Vec::new();
    }

    let significant_indexes: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| (!is_trivia(&token.token)).then_some(index))
        .collect();
    if significant_indexes.is_empty() {
        return Vec::new();
    }

    let max_end = tokens.last().map_or(0, |token| token.end);
    let mut candidates = Vec::new();
    for position in 0..significant_indexes.len() {
        let Some(operator_span) =
            join_operator_token_span_at(tokens, &significant_indexes, position)
        else {
            continue;
        };

        if has_explicit_join_constraint(tokens, &significant_indexes, operator_span) {
            continue;
        }
        if relation_starts_with_unnest(tokens, &significant_indexes, operator_span.end_position) {
            continue;
        }

        let start_index = significant_indexes[operator_span.start_position];
        let end_index = significant_indexes[operator_span.end_position];
        let span = Span::new(tokens[start_index].start, tokens[end_index].end);
        if span.start >= span.end || span.end > max_end {
            continue;
        }
        if operator_span_contains_comment(tokens, span) {
            continue;
        }

        candidates.push(Am008AutofixCandidate {
            span,
            edits: vec![IssuePatchEdit::new(span, "CROSS JOIN")],
        });
    }

    candidates
}

fn join_operator_token_span_at(
    tokens: &[PositionedToken],
    significant_indexes: &[usize],
    position: usize,
) -> Option<JoinOperatorTokenSpan> {
    let token = &tokens[*significant_indexes.get(position)?].token;

    if token_word_equals(token, "STRAIGHT_JOIN") {
        return Some(JoinOperatorTokenSpan {
            start_position: position,
            end_position: position,
        });
    }

    if token_word_equals(token, "STRAIGHT")
        && token_is_word_at(tokens, significant_indexes, position + 1, "JOIN")
    {
        return Some(JoinOperatorTokenSpan {
            start_position: position,
            end_position: position + 1,
        });
    }

    if token_word_equals(token, "INNER")
        && token_is_word_at(tokens, significant_indexes, position + 1, "JOIN")
    {
        return Some(JoinOperatorTokenSpan {
            start_position: position,
            end_position: position + 1,
        });
    }

    if is_outer_join_side_keyword(token) {
        if token_is_word_at(tokens, significant_indexes, position + 1, "OUTER")
            && token_is_word_at(tokens, significant_indexes, position + 2, "JOIN")
        {
            return Some(JoinOperatorTokenSpan {
                start_position: position,
                end_position: position + 2,
            });
        }

        if token_is_word_at(tokens, significant_indexes, position + 1, "JOIN") {
            return Some(JoinOperatorTokenSpan {
                start_position: position,
                end_position: position + 1,
            });
        }
    }

    if token_word_equals(token, "JOIN")
        && !previous_token_is_join_modifier(tokens, significant_indexes, position)
    {
        return Some(JoinOperatorTokenSpan {
            start_position: position,
            end_position: position,
        });
    }

    None
}

fn token_is_word_at(
    tokens: &[PositionedToken],
    significant_indexes: &[usize],
    position: usize,
    expected_upper: &str,
) -> bool {
    significant_indexes
        .get(position)
        .and_then(|index| tokens.get(*index))
        .is_some_and(|token| token_word_equals(&token.token, expected_upper))
}

fn previous_token_is_join_modifier(
    tokens: &[PositionedToken],
    significant_indexes: &[usize],
    position: usize,
) -> bool {
    if position == 0 {
        return false;
    }

    let previous = &tokens[significant_indexes[position - 1]].token;
    token_word_equals(previous, "INNER")
        || token_word_equals(previous, "LEFT")
        || token_word_equals(previous, "RIGHT")
        || token_word_equals(previous, "FULL")
        || token_word_equals(previous, "CROSS")
        || token_word_equals(previous, "NATURAL")
        || token_word_equals(previous, "OUTER")
        || token_word_equals(previous, "SEMI")
        || token_word_equals(previous, "ANTI")
        || token_word_equals(previous, "ASOF")
        || token_word_equals(previous, "STRAIGHT")
        || token_word_equals(previous, "STRAIGHT_JOIN")
        || token_word_equals(previous, "POSITIONAL")
}

fn has_explicit_join_constraint(
    tokens: &[PositionedToken],
    significant_indexes: &[usize],
    operator_span: JoinOperatorTokenSpan,
) -> bool {
    if operator_span.start_position > 0
        && token_is_word_at(
            tokens,
            significant_indexes,
            operator_span.start_position - 1,
            "NATURAL",
        )
    {
        return true;
    }

    // DuckDB POSITIONAL JOIN matches rows by position; no ON/USING needed.
    if operator_span.start_position > 0
        && token_is_word_at(
            tokens,
            significant_indexes,
            operator_span.start_position - 1,
            "POSITIONAL",
        )
    {
        return true;
    }

    let mut depth = 0usize;
    for position in (operator_span.end_position + 1)..significant_indexes.len() {
        let token = &tokens[significant_indexes[position]].token;

        match token {
            Token::LParen => {
                depth += 1;
                continue;
            }
            Token::RParen => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                continue;
            }
            _ => {}
        }

        if depth > 0 {
            continue;
        }

        if token_word_equals(token, "ON") || token_word_equals(token, "USING") {
            return true;
        }

        if token_word_equals(token, "NATURAL")
            || is_clause_boundary_token(token)
            || matches!(token, Token::Comma | Token::SemiColon)
            || join_operator_token_span_at(tokens, significant_indexes, position).is_some()
        {
            break;
        }
    }

    false
}

fn relation_starts_with_unnest(
    tokens: &[PositionedToken],
    significant_indexes: &[usize],
    operator_end_position: usize,
) -> bool {
    for position in (operator_end_position + 1)..significant_indexes.len() {
        let token = &tokens[significant_indexes[position]].token;
        if token_word_equals(token, "LATERAL") {
            continue;
        }
        return token_word_equals(token, "UNNEST");
    }

    false
}

fn is_clause_boundary_token(token: &Token) -> bool {
    token_word_equals(token, "WHERE")
        || token_word_equals(token, "GROUP")
        || token_word_equals(token, "HAVING")
        || token_word_equals(token, "QUALIFY")
        || token_word_equals(token, "ORDER")
        || token_word_equals(token, "LIMIT")
        || token_word_equals(token, "FETCH")
        || token_word_equals(token, "OFFSET")
        || token_word_equals(token, "UNION")
        || token_word_equals(token, "EXCEPT")
        || token_word_equals(token, "INTERSECT")
        || token_word_equals(token, "WINDOW")
        || token_word_equals(token, "RETURNING")
        || token_word_equals(token, "CONNECT")
        || token_word_equals(token, "START")
        || token_word_equals(token, "MODEL")
}

fn operator_span_contains_comment(tokens: &[PositionedToken], span: Span) -> bool {
    tokens
        .iter()
        .any(|token| token.start >= span.start && token.end <= span.end && is_comment(&token.token))
}

fn tokenize_with_spans(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn token_word_equals(token: &Token, expected_upper: &str) -> bool {
    matches!(token, Token::Word(word) if word.value.eq_ignore_ascii_case(expected_upper))
}

fn is_outer_join_side_keyword(token: &Token) -> bool {
    token_word_equals(token, "LEFT")
        || token_word_equals(token, "RIGHT")
        || token_word_equals(token, "FULL")
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

fn is_comment(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::SingleLineComment { .. } | Whitespace::MultiLineComment(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AmbiguousJoinCondition;
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    // --- Edge cases adopted from sqlfluff AM08 ---

    #[test]
    fn flags_missing_on_clause_for_inner_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo INNER JOIN bar");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_008);
    }

    #[test]
    fn flags_missing_on_clause_for_left_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo left join bar");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_each_missing_join_condition_in_join_chain() {
        let issues =
            run("SELECT foo.a, bar.b FROM foo left join bar left join baz on foo.x = bar.y");
        assert_eq!(issues.len(), 1);

        let issues =
            run("SELECT foo.a, bar.b FROM foo left join bar on foo.x = bar.y left join baz");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn does_not_flag_join_without_on_when_where_clause_exists() {
        let issues = run("SELECT foo.a, bar.b FROM foo left join bar where foo.x = bar.y");
        assert!(issues.is_empty());

        let issues = run("SELECT foo.a, bar.b FROM foo JOIN bar WHERE foo.a = bar.a OR foo.x = 3");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_explicit_join_conditions() {
        let issues = run("SELECT foo.a, bar.b FROM foo INNER JOIN bar ON 1=1");
        assert!(issues.is_empty());

        let issues = run("SELECT foo.id, bar.id FROM foo LEFT JOIN bar USING (id)");
        assert!(issues.is_empty());

        let issues = run("SELECT foo.x FROM foo NATURAL JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_explicit_cross_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo CROSS JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn ignores_unnest_joins() {
        let issues = run("SELECT t.id FROM t INNER JOIN UNNEST(t.items) AS item");
        assert!(issues.is_empty());
    }

    #[test]
    fn inner_join_without_condition_emits_safe_autofix_patch() {
        let sql = "SELECT foo.a, bar.b FROM foo INNER JOIN bar";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected AM008 core autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        assert_eq!(autofix.edits[0].replacement, "CROSS JOIN");
        assert_eq!(
            &sql[autofix.edits[0].span.start..autofix.edits[0].span.end],
            "INNER JOIN"
        );
    }

    #[test]
    fn bare_join_without_condition_emits_safe_autofix_patch() {
        let sql = "SELECT foo.a, bar.b FROM foo JOIN bar";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected AM008 core autofix metadata for bare JOIN");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        assert_eq!(autofix.edits[0].replacement, "CROSS JOIN");
        assert_eq!(
            &sql[autofix.edits[0].span.start..autofix.edits[0].span.end],
            "JOIN"
        );
    }

    #[test]
    fn join_operator_comment_blocks_safe_autofix_metadata() {
        let sql = "SELECT foo.a, bar.b FROM foo LEFT /*keep*/ JOIN bar";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0].autofix.is_none(),
            "comment-bearing join operator span should remain unpatched"
        );
    }

    #[test]
    fn allows_positional_join_duckdb() {
        let issues = run("SELECT foo.a, bar.b FROM foo POSITIONAL JOIN bar");
        assert!(issues.is_empty());
    }
}
