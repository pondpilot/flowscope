//! LINT_CV_012: JOIN condition convention.
//!
//! Plain `JOIN` clauses without ON/USING should use explicit join predicates,
//! not implicit relationships hidden in WHERE.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::{
    BinaryOperator, Expr, JoinConstraint, JoinOperator, Select, Spanned, Statement, TableFactor,
    TableWithJoins,
};
use sqlparser::tokenizer::{Span as SqlParserSpan, Token, TokenWithSpan, Tokenizer};
use std::collections::HashSet;

use super::semantic_helpers::{table_factor_reference_name, visit_selects_in_statement};

pub struct ConventionJoinCondition;

impl BuiltinLintRule for ConventionJoinCondition {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_012
    }

    fn name(&self) -> &'static str {
        "Join condition convention"
    }

    fn description(&self) -> &'static str {
        "Use `JOIN ... ON ...` instead of `WHERE ...` for join conditions."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut found_violation = false;
        let mut autofix_edits: Vec<IssuePatchEdit> = Vec::new();

        visit_selects_in_statement(statement, &mut |select| {
            let fix_result = cv012_select_autofix_result(select, ctx);
            if fix_result.has_violation {
                found_violation = true;
                autofix_edits.extend(fix_result.edits);
            }
        });

        if !found_violation {
            return Vec::new();
        }

        sort_and_dedup_patch_edits(&mut autofix_edits);
        if patch_edits_overlap(&autofix_edits) {
            autofix_edits.clear();
        }

        let mut issue = Issue::warning(
            issue_codes::LINT_CV_012,
            "JOIN clause appears to lack a meaningful join condition.",
        )
        .with_statement(ctx.statement_index);
        if !autofix_edits.is_empty() {
            issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, autofix_edits);
        }
        vec![issue]
    }
}

#[derive(Default)]
struct Cv12SelectFixResult {
    has_violation: bool,
    edits: Vec<IssuePatchEdit>,
}

#[derive(Clone)]
struct Cv12JoinFixPlan {
    join_index: usize,
    predicates: Vec<Expr>,
}

fn cv012_select_autofix_result(select: &Select, ctx: &RuleContext) -> Cv12SelectFixResult {
    let Some(where_expr) = &select.selection else {
        return Cv12SelectFixResult::default();
    };

    let select_abs_start = sqlparser_span_abs_offsets(ctx, select.span())
        .map(|(start, _)| start)
        .unwrap_or(ctx.statement_range.start);

    let mut has_violation = false;
    let mut extracted_predicates: Vec<Expr> = Vec::new();
    let mut edits: Vec<IssuePatchEdit> = Vec::new();

    for table in &select.from {
        let Some(join_plans) = cv012_plan_join_chain(table, where_expr) else {
            continue;
        };
        has_violation = true;

        for plan in join_plans {
            if plan.predicates.is_empty() {
                continue;
            }

            let Some(join) = table.joins.get(plan.join_index) else {
                return Cv12SelectFixResult {
                    has_violation,
                    edits: Vec::new(),
                };
            };

            let Some((_, relation_end_abs)) = sqlparser_span_abs_offsets(ctx, join.relation.span())
            else {
                return Cv12SelectFixResult {
                    has_violation,
                    edits: Vec::new(),
                };
            };

            let Some(on_expr) = combine_predicates_with_and(&plan.predicates) else {
                continue;
            };

            edits.push(IssuePatchEdit::new(
                Span::new(relation_end_abs, relation_end_abs),
                format!(" ON {on_expr}"),
            ));
            extracted_predicates.extend(plan.predicates);
        }
    }

    if !has_violation {
        return Cv12SelectFixResult::default();
    }

    dedup_expressions(&mut extracted_predicates);
    if extracted_predicates.is_empty() {
        return Cv12SelectFixResult {
            has_violation,
            edits: Vec::new(),
        };
    }

    let Some((where_expr_start, where_expr_end)) =
        sqlparser_span_statement_offsets(ctx, where_expr.span())
    else {
        return Cv12SelectFixResult {
            has_violation,
            edits: Vec::new(),
        };
    };

    let where_expr_abs_start = ctx.statement_range.start + where_expr_start;
    let where_expr_abs_end =
        (ctx.statement_range.start + where_expr_end).min(ctx.statement_range.end);

    let Some(where_keyword_abs_start) =
        locate_where_keyword_abs_start(ctx, select_abs_start, where_expr_abs_start)
    else {
        return Cv12SelectFixResult {
            has_violation,
            edits: Vec::new(),
        };
    };

    if let Some(remaining_where) =
        cv012_remove_predicates(where_expr.clone(), &extracted_predicates)
    {
        edits.push(IssuePatchEdit::new(
            Span::new(where_keyword_abs_start, where_expr_abs_end),
            format!("WHERE {remaining_where}"),
        ));
    } else {
        edits.push(IssuePatchEdit::new(
            Span::new(where_keyword_abs_start, where_expr_abs_end),
            String::new(),
        ));
    }

    Cv12SelectFixResult {
        has_violation,
        edits,
    }
}

fn cv012_plan_join_chain(
    table: &TableWithJoins,
    where_expr: &Expr,
) -> Option<Vec<Cv12JoinFixPlan>> {
    let mut seen_sources = Vec::new();
    collect_table_factor_sources(&table.relation, &mut seen_sources);

    let mut pass_seen = seen_sources.clone();
    let mut bare_join_indexes: Vec<(usize, Vec<String>)> = Vec::new();

    for (idx, join) in table.joins.iter().enumerate() {
        let join_sources = collect_table_factor_all_sources(&join.relation);
        let Some(constraint) = join_constraint(&join.join_operator) else {
            pass_seen.extend(join_sources);
            continue;
        };

        let has_explicit_join_clause = matches!(
            constraint,
            JoinConstraint::On(_) | JoinConstraint::Using(_) | JoinConstraint::Natural
        );
        if has_explicit_join_clause {
            pass_seen.extend(join_sources);
            continue;
        }

        let matched_where_predicate = join_sources
            .iter()
            .any(|src| where_contains_join_predicate(where_expr, Some(src), &pass_seen))
            || (join_sources.is_empty()
                && where_contains_join_predicate(where_expr, None, &pass_seen));
        if !matched_where_predicate {
            return None;
        }

        bare_join_indexes.push((idx, join_sources.clone()));
        pass_seen.extend(join_sources);
    }

    if bare_join_indexes.is_empty() {
        return None;
    }

    let mut extraction_seen = seen_sources;
    let mut plans = Vec::new();
    for (idx, join_sources) in bare_join_indexes {
        let mut predicates = Vec::new();
        if join_sources.is_empty() {
            cv012_collect_extractable_eqs(where_expr, None, &extraction_seen, &mut predicates);
        } else {
            for source in &join_sources {
                cv012_collect_extractable_eqs(
                    where_expr,
                    Some(source),
                    &extraction_seen,
                    &mut predicates,
                );
            }
        }
        dedup_expressions(&mut predicates);
        plans.push(Cv12JoinFixPlan {
            join_index: idx,
            predicates,
        });
        extraction_seen.extend(join_sources);
    }

    Some(plans)
}

fn combine_predicates_with_and(predicates: &[Expr]) -> Option<Expr> {
    predicates
        .iter()
        .cloned()
        .reduce(|acc, pred| Expr::BinaryOp {
            left: Box::new(acc),
            op: BinaryOperator::And,
            right: Box::new(pred),
        })
}

fn dedup_expressions(exprs: &mut Vec<Expr>) {
    let mut seen = HashSet::new();
    exprs.retain(|expr| seen.insert(format!("{expr}")));
}

fn sort_and_dedup_patch_edits(edits: &mut Vec<IssuePatchEdit>) {
    edits.sort_by(|a, b| {
        a.span
            .start
            .cmp(&b.span.start)
            .then_with(|| a.span.end.cmp(&b.span.end))
            .then_with(|| a.replacement.cmp(&b.replacement))
    });
    edits.dedup_by(|a, b| a.span == b.span && a.replacement == b.replacement);
}

fn patch_edits_overlap(edits: &[IssuePatchEdit]) -> bool {
    edits
        .windows(2)
        .any(|pair| pair[0].span.end > pair[1].span.start)
}

/// Adds the reference name for a table factor to the list.
fn collect_table_factor_sources(table_factor: &TableFactor, sources: &mut Vec<String>) {
    if let Some(name) = table_factor_reference_name(table_factor) {
        sources.push(name);
    }
}

/// Collects all table source names from a table factor, including nested join
/// members. For a simple table reference this returns a single name. For a
/// `NestedJoin` it returns all inner table names.
fn collect_table_factor_all_sources(table_factor: &TableFactor) -> Vec<String> {
    let mut sources = Vec::new();
    match table_factor {
        TableFactor::NestedJoin {
            table_with_joins,
            alias,
            ..
        } => {
            if let Some(alias) = alias {
                sources.push(alias.name.value.to_ascii_uppercase());
            } else {
                collect_table_factor_sources(&table_with_joins.relation, &mut sources);
                for join in &table_with_joins.joins {
                    collect_table_factor_sources(&join.relation, &mut sources);
                }
            }
        }
        _ => {
            collect_table_factor_sources(table_factor, &mut sources);
        }
    }
    sources
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

fn where_contains_join_predicate(
    expr: &Expr,
    current_source: Option<&String>,
    seen_sources: &[String],
) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            let direct_match = matches!(
                op,
                BinaryOperator::Eq
                    | BinaryOperator::NotEq
                    | BinaryOperator::Lt
                    | BinaryOperator::Gt
                    | BinaryOperator::LtEq
                    | BinaryOperator::GtEq
            ) && is_column_reference(left)
                && is_column_reference(right)
                && references_joined_sources(left, right, current_source, seen_sources);

            direct_match
                || where_contains_join_predicate(left, current_source, seen_sources)
                || where_contains_join_predicate(right, current_source, seen_sources)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            where_contains_join_predicate(inner, current_source, seen_sources)
        }
        Expr::InList { expr, list, .. } => {
            where_contains_join_predicate(expr, current_source, seen_sources)
                || list
                    .iter()
                    .any(|item| where_contains_join_predicate(item, current_source, seen_sources))
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            where_contains_join_predicate(expr, current_source, seen_sources)
                || where_contains_join_predicate(low, current_source, seen_sources)
                || where_contains_join_predicate(high, current_source, seen_sources)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand.as_ref().is_some_and(|expr| {
                where_contains_join_predicate(expr, current_source, seen_sources)
            }) || conditions.iter().any(|when| {
                where_contains_join_predicate(&when.condition, current_source, seen_sources)
                    || where_contains_join_predicate(&when.result, current_source, seen_sources)
            }) || else_result.as_ref().is_some_and(|expr| {
                where_contains_join_predicate(expr, current_source, seen_sources)
            })
        }
        _ => false,
    }
}

fn is_column_reference(expr: &Expr) -> bool {
    matches!(expr, Expr::Identifier(_) | Expr::CompoundIdentifier(_))
}

fn references_joined_sources(
    left: &Expr,
    right: &Expr,
    current_source: Option<&String>,
    seen_sources: &[String],
) -> bool {
    let left_prefix = qualifier_prefix(left);
    let right_prefix = qualifier_prefix(right);

    match (left_prefix, right_prefix, current_source) {
        (Some(left), Some(right), Some(current)) => {
            (left == *current && seen_sources.contains(&right))
                || (right == *current && seen_sources.contains(&left))
        }
        // Unqualified `a = b` in a plain join WHERE is still ambiguous and should fail.
        (None, None, _) => true,
        _ => false,
    }
}

fn qualifier_prefix(expr: &Expr) -> Option<String> {
    match expr {
        Expr::CompoundIdentifier(parts) if parts.len() > 1 => {
            // For `table.col` (2 parts), the qualifier is the first part.
            // For `schema.table.col` (3+ parts), the qualifier is the
            // penultimate part (the table name) since that is what
            // `table_factor_reference_name` extracts as the source name.
            let qualifier_index = parts.len() - 2;
            Some(parts[qualifier_index].value.to_ascii_uppercase())
        }
        Expr::Nested(inner)
        | Expr::UnaryOp { expr: inner, .. }
        | Expr::Cast { expr: inner, .. } => qualifier_prefix(inner),
        _ => None,
    }
}

/// Collect top-level AND-chained equality predicates that join `current_source`
/// to one of `seen_sources`.
fn cv012_collect_extractable_eqs(
    expr: &Expr,
    current_source: Option<&str>,
    seen_sources: &[String],
    out: &mut Vec<Expr>,
) {
    match expr {
        Expr::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } => {
            cv012_collect_extractable_eqs(left, current_source, seen_sources, out);
            cv012_collect_extractable_eqs(right, current_source, seen_sources, out);
        }
        Expr::BinaryOp {
            left,
            op: BinaryOperator::Eq,
            right,
        } if cv012_is_extractable_eq(left, right, current_source, seen_sources) => {
            out.push(expr.clone());
        }
        Expr::Nested(inner) => {
            if let Expr::BinaryOp {
                left,
                op: BinaryOperator::Eq,
                right,
            } = inner.as_ref()
            {
                if cv012_is_extractable_eq(left, right, current_source, seen_sources) {
                    out.push(expr.clone());
                }
            }
        }
        _ => {}
    }
}

fn cv012_is_extractable_eq(
    left: &Expr,
    right: &Expr,
    current_source: Option<&str>,
    seen_sources: &[String],
) -> bool {
    let Some(current) = current_source else {
        return false;
    };
    let current_upper = current.to_ascii_uppercase();
    let left_qual = cv012_qualifier(left);
    let right_qual = cv012_qualifier(right);
    if let (Some(lq), Some(rq)) = (left_qual, right_qual) {
        return (lq == current_upper && seen_sources.iter().any(|s| s.to_ascii_uppercase() == rq))
            || (rq == current_upper && seen_sources.iter().any(|s| s.to_ascii_uppercase() == lq));
    }
    false
}

/// Extract the table qualifier from a column reference expression.
fn cv012_qualifier(expr: &Expr) -> Option<String> {
    match expr {
        Expr::CompoundIdentifier(parts) if parts.len() > 1 => {
            let qualifier_index = parts.len() - 2;
            parts
                .get(qualifier_index)
                .map(|part| part.value.to_ascii_uppercase())
        }
        _ => None,
    }
}

/// Remove specific predicates from an AND-chain expression.  Returns `None`
/// if the entire expression was consumed.
fn cv012_remove_predicates(expr: Expr, to_remove: &[Expr]) -> Option<Expr> {
    if to_remove.iter().any(|r| expr_eq(&expr, r)) {
        return None;
    }
    match expr {
        Expr::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } => {
            let left_remaining = cv012_remove_predicates(*left, to_remove);
            let right_remaining = cv012_remove_predicates(*right, to_remove);
            match (left_remaining, right_remaining) {
                (Some(l), Some(r)) => Some(Expr::BinaryOp {
                    left: Box::new(l),
                    op: BinaryOperator::And,
                    right: Box::new(r),
                }),
                (Some(l), None) => Some(l),
                (None, Some(r)) => Some(r),
                (None, None) => None,
            }
        }
        other => Some(other),
    }
}

/// Structural equality check for expressions (used by predicate removal).
fn expr_eq(a: &Expr, b: &Expr) -> bool {
    format!("{a}") == format!("{b}")
}

fn locate_where_keyword_abs_start(
    ctx: &RuleContext,
    select_abs_start: usize,
    where_expr_abs_start: usize,
) -> Option<usize> {
    let tokens = positioned_statement_tokens(ctx)?;
    tokens
        .iter()
        .filter(|token| {
            token.start >= select_abs_start
                && token.end <= where_expr_abs_start
                && token_word_equals(&token.token, "WHERE")
        })
        .map(|token| token.start)
        .max()
}

#[derive(Clone)]
struct PositionedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn positioned_statement_tokens(ctx: &RuleContext) -> Option<Vec<PositionedToken>> {
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
        return Some(tokens);
    }

    let dialect = ctx.dialect().to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), ctx.statement_sql());
    let tokens = tokenizer.tokenize_with_location().ok()?;
    let mut positioned = Vec::new();
    for token in tokens {
        let (start, end) = token_with_span_offsets(ctx.statement_sql(), &token)?;
        positioned.push(PositionedToken {
            token: token.token,
            start: ctx.statement_range.start + start,
            end: ctx.statement_range.start + end,
        });
    }
    Some(positioned)
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

fn sqlparser_span_statement_offsets(
    ctx: &RuleContext,
    span: SqlParserSpan,
) -> Option<(usize, usize)> {
    if let Some((start, end)) = sqlparser_span_offsets(ctx.statement_sql(), span) {
        return Some((start, end));
    }
    let (start, end) = sqlparser_span_offsets(ctx.sql, span)?;
    if start < ctx.statement_range.start || end > ctx.statement_range.end {
        return None;
    }
    Some((
        start - ctx.statement_range.start,
        end - ctx.statement_range.start,
    ))
}

fn sqlparser_span_abs_offsets(ctx: &RuleContext, span: SqlParserSpan) -> Option<(usize, usize)> {
    if let Some((start, end)) = sqlparser_span_offsets(ctx.statement_sql(), span) {
        return Some((
            ctx.statement_range.start + start,
            ctx.statement_range.start + end,
        ));
    }
    let (start, end) = sqlparser_span_offsets(ctx.sql, span)?;
    if start < ctx.statement_range.start || end > ctx.statement_range.end {
        return None;
    }
    Some((start, end))
}

fn sqlparser_span_offsets(sql: &str, span: SqlParserSpan) -> Option<(usize, usize)> {
    if span.start.line == 0 || span.start.column == 0 || span.end.line == 0 || span.end.column == 0
    {
        return None;
    }

    let start = line_col_to_offset(sql, span.start.line as usize, span.start.column as usize)?;
    let end = line_col_to_offset(sql, span.end.line as usize, span.end.column as usize)?;
    (end >= start).then_some((start, end))
}

fn line_col_to_offset(sql: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }

    let mut current_line = 1usize;
    let mut line_start = 0usize;
    for (idx, ch) in sql.char_indices() {
        if current_line == line {
            break;
        }
        if ch == '\n' {
            current_line += 1;
            line_start = idx + ch.len_utf8();
        }
    }
    if current_line != line {
        return None;
    }

    let mut current_column = 1usize;
    for (rel_idx, ch) in sql[line_start..].char_indices() {
        if current_column == column {
            return Some(line_start + rel_idx);
        }
        if ch == '\n' {
            return None;
        }
        current_column += 1;
    }
    if current_column == column {
        return Some(sql.len());
    }
    None
}

fn token_word_equals(token: &Token, word: &str) -> bool {
    matches!(token, Token::Word(w) if w.value.eq_ignore_ascii_case(word))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::{IssueAutofixApplicability, IssuePatchEdit};

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionJoinCondition;
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn apply_edits(sql: &str, edits: &[IssuePatchEdit]) -> String {
        let mut output = sql.to_string();
        let mut ordered = edits.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|edit| edit.span.start);
        for edit in ordered.into_iter().rev() {
            output.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        output
    }

    // --- Edge cases adopted from sqlfluff CV12 ---

    #[test]
    fn allows_plain_join_without_where_clause() {
        let issues = run("SELECT foo.a, bar.b FROM foo JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_plain_join_with_implicit_where_predicate() {
        let issues = run("SELECT foo.a, bar.b FROM foo JOIN bar WHERE foo.x = bar.y");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_012);
    }

    #[test]
    fn flags_plain_join_with_unqualified_where_predicate() {
        let issues = run("SELECT foo.a, bar.b FROM foo JOIN bar WHERE a = b");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_join_with_explicit_on_clause() {
        let issues = run("SELECT foo.a, bar.b FROM foo LEFT JOIN bar ON foo.x = bar.x");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_cross_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo CROSS JOIN bar WHERE bar.x > 3");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_inner_join_without_on_with_where_predicate() {
        let issues = run("SELECT foo.a, bar.b FROM foo INNER JOIN bar WHERE foo.x = bar.y");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_012);
    }

    #[test]
    fn does_not_flag_multi_join_chain_when_not_all_plain_joins_are_where_joined() {
        let sql = "select a.id from a join b join c where a.a = b.a and b.b > 1";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn flags_multi_join_chain_when_all_plain_joins_are_where_joined() {
        let sql = "select a.id from a join b join c where a.a = b.a and b.b = c.b";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_012);
    }

    #[test]
    fn flags_schema_qualified_where_join() {
        // SQLFluff: test_fail_missing_clause_and_stmt_qualified
        let sql = "SELECT foo.a, bar.b FROM schema.foo JOIN schema.bar WHERE schema.foo.x = schema.bar.y AND schema.foo.x = 3";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_012);
    }

    #[test]
    fn flags_bracketed_join_with_where_predicate() {
        // SQLFluff: test_fail_join_with_bracketed_join
        let sql =
            "SELECT * FROM bar JOIN (foo1 JOIN foo2 ON (foo1.id = foo2.id)) WHERE bar.id = foo1.id";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_012);
    }

    #[test]
    fn autofix_moves_where_join_predicate_into_on() {
        let sql = "SELECT foo.a, bar.b FROM foo JOIN bar WHERE foo.x = bar.y";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected CV12 core autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(
            fixed.trim_end(),
            "SELECT foo.a, bar.b FROM foo JOIN bar ON foo.x = bar.y"
        );
    }

    #[test]
    fn autofix_preserves_non_join_where_predicate() {
        let sql = "SELECT foo.a, bar.b FROM foo JOIN bar WHERE foo.x = bar.y AND foo.x = 3";
        let issues = run(sql);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected CV12 core autofix metadata");
        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(
            fixed,
            "SELECT foo.a, bar.b FROM foo JOIN bar ON foo.x = bar.y WHERE foo.x = 3"
        );
    }

    #[test]
    fn autofix_handles_bracketed_join_predicate() {
        let sql = "SELECT foo.a, bar.b FROM foo JOIN bar WHERE (foo.x = bar.y) AND foo.t = 3";
        let issues = run(sql);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected CV12 core autofix metadata");
        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(
            fixed,
            "SELECT foo.a, bar.b FROM foo JOIN bar ON (foo.x = bar.y) WHERE foo.t = 3"
        );
    }

    #[test]
    fn autofix_handles_two_bare_joins() {
        let sql = "SELECT foo.a, bar.b FROM foo JOIN bar JOIN baz WHERE foo.x = bar.y AND foo.x = baz.t AND foo.c = 3";
        let issues = run(sql);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected CV12 core autofix metadata");
        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(
            fixed,
            "SELECT foo.a, bar.b FROM foo JOIN bar ON foo.x = bar.y JOIN baz ON foo.x = baz.t WHERE foo.c = 3"
        );
    }

    #[test]
    fn autofix_handles_schema_qualified_references() {
        let sql = "SELECT foo.a, bar.b FROM schema.foo JOIN schema.bar WHERE schema.foo.x = schema.bar.y AND schema.foo.x = 3";
        let issues = run(sql);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected CV12 core autofix metadata");
        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(
            fixed,
            "SELECT foo.a, bar.b FROM schema.foo JOIN schema.bar ON schema.foo.x = schema.bar.y WHERE schema.foo.x = 3"
        );
    }
}
