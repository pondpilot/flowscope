//! LINT_ST_009: Reversed JOIN condition ordering.
//!
//! Detect predicates where the newly joined relation appears on the left side
//! and prior relation on the right side (e.g. `o.user_id = u.id`).

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::{BinaryOperator, Expr, Spanned, Statement, TableFactor};

use super::semantic_helpers::{
    join_on_expr, table_factor_reference_name, visit_selects_in_statement,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreferredFirstTableInJoinClause {
    Earlier,
    Later,
}

impl PreferredFirstTableInJoinClause {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(
                issue_codes::LINT_ST_009,
                "preferred_first_table_in_join_clause",
            )
            .unwrap_or("earlier")
            .to_ascii_lowercase()
            .as_str()
        {
            "later" => Self::Later,
            _ => Self::Earlier,
        }
    }

    fn left_source<'a>(self, current: &'a str, previous: &'a str) -> &'a str {
        match self {
            Self::Earlier => current,
            Self::Later => previous,
        }
    }

    fn right_source<'a>(self, current: &'a str, previous: &'a str) -> &'a str {
        match self {
            Self::Earlier => previous,
            Self::Later => current,
        }
    }
}

pub struct StructureJoinConditionOrder {
    preferred_first_table: PreferredFirstTableInJoinClause,
}

impl StructureJoinConditionOrder {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            preferred_first_table: PreferredFirstTableInJoinClause::from_config(config),
        }
    }
}

impl Default for StructureJoinConditionOrder {
    fn default() -> Self {
        Self {
            preferred_first_table: PreferredFirstTableInJoinClause::Earlier,
        }
    }
}

impl BuiltinLintRule for StructureJoinConditionOrder {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_009
    }

    fn name(&self) -> &'static str {
        "Structure join condition order"
    }

    fn description(&self) -> &'static str {
        "Joins should list the table referenced earlier/later first."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut issues = Vec::new();

        visit_selects_in_statement(statement, &mut |select| {
            // SQLFluff ST09 parity: report at most one reversed-join issue
            // per SELECT, matching SQLFluff's first-violation-only behaviour.
            let before = issues.len();
            'from_loop: for table in &select.from {
                let mut seen_sources: Vec<String> = Vec::new();
                check_table_factor_joins(
                    &table.relation,
                    &table.joins,
                    &mut seen_sources,
                    self.preferred_first_table,
                    ctx,
                    &mut issues,
                );
                if issues.len() > before {
                    break 'from_loop;
                }
            }
        });

        issues
    }
}

fn check_table_factor_joins(
    relation: &TableFactor,
    joins: &[sqlparser::ast::Join],
    seen_sources: &mut Vec<String>,
    preference: PreferredFirstTableInJoinClause,
    ctx: &RuleContext,
    issues: &mut Vec<Issue>,
) {
    let issues_before = issues.len();

    // For NestedJoin, recurse into the inner table_with_joins.
    if let TableFactor::NestedJoin {
        table_with_joins, ..
    } = relation
    {
        check_table_factor_joins(
            &table_with_joins.relation,
            &table_with_joins.joins,
            seen_sources,
            preference,
            ctx,
            issues,
        );
    } else if let Some(base) = table_factor_reference_name(relation) {
        seen_sources.push(base);
    }

    for (join_index, join) in joins.iter().enumerate() {
        // Recurse into nested join relations on the right side.
        if let TableFactor::NestedJoin {
            table_with_joins, ..
        } = &join.relation
        {
            check_table_factor_joins(
                &table_with_joins.relation,
                &table_with_joins.joins,
                seen_sources,
                preference,
                ctx,
                issues,
            );
        }

        let join_name = table_factor_reference_name(&join.relation);
        if let (Some(current), Some(on_expr)) =
            (join_name.as_ref(), join_on_expr(&join.join_operator))
        {
            // SQLFluff parity: only flag the first reversed join per FROM clause.
            if issues.len() == issues_before {
                let matching_previous = seen_sources
                    .iter()
                    .rev()
                    .find(|candidate| {
                        let left = preference.left_source(current, candidate.as_str());
                        let right = preference.right_source(current, candidate.as_str());
                        has_join_pair(on_expr, left, right)
                    })
                    .cloned();

                if matching_previous.is_some() {
                    let mut issue_span = expr_statement_offsets(ctx, on_expr)
                        .map(|(expr_start, expr_end)| {
                            ctx.span_from_statement_offset(expr_start, expr_end)
                        })
                        .unwrap_or_else(|| Span::new(0, 0));
                    let mut edits: Vec<IssuePatchEdit> = Vec::new();

                    if let Some((span, replacement)) =
                        join_condition_autofix_for_sources(ctx, on_expr, current, seen_sources)
                    {
                        issue_span = span;
                        edits.push(IssuePatchEdit::new(span, replacement));
                    }

                    let mut seen_for_following = seen_sources.clone();
                    if let Some(name) = join_name.as_ref() {
                        seen_for_following.push(name.clone());
                    }
                    edits.extend(collect_following_join_autofixes(
                        &joins[join_index + 1..],
                        &seen_for_following,
                        preference,
                        ctx,
                    ));

                    if issue_span.start != issue_span.end {
                        let mut issue = Issue::info(
                            issue_codes::LINT_ST_009,
                            "Join condition ordering appears inconsistent with configured preference.",
                        )
                        .with_statement(ctx.statement_index)
                        .with_span(issue_span);
                        if !edits.is_empty() {
                            issue =
                                issue.with_autofix_edits(IssueAutofixApplicability::Safe, edits);
                        }
                        issues.push(issue);
                    }
                }
            }
        }

        if let Some(name) = join_name {
            seen_sources.push(name);
        }
    }
}

fn collect_following_join_autofixes(
    joins: &[sqlparser::ast::Join],
    seen_sources: &[String],
    preference: PreferredFirstTableInJoinClause,
    ctx: &RuleContext,
) -> Vec<IssuePatchEdit> {
    let mut seen = seen_sources.to_vec();
    let mut edits = Vec::new();

    for join in joins {
        let join_name = table_factor_reference_name(&join.relation);
        if let (Some(current), Some(on_expr)) =
            (join_name.as_ref(), join_on_expr(&join.join_operator))
        {
            let matching_previous = seen
                .iter()
                .rev()
                .find(|candidate| {
                    let left = preference.left_source(current, candidate.as_str());
                    let right = preference.right_source(current, candidate.as_str());
                    has_join_pair(on_expr, left, right)
                })
                .cloned();
            if matching_previous.is_some() {
                if let Some((span, replacement)) =
                    join_condition_autofix_for_sources(ctx, on_expr, current, &seen)
                {
                    edits.push(IssuePatchEdit::new(span, replacement));
                }
            }
        }
        if let Some(name) = join_name {
            seen.push(name);
        }
    }

    edits
}

fn is_comparison_operator(op: &BinaryOperator) -> bool {
    matches!(
        op,
        BinaryOperator::Eq
            | BinaryOperator::NotEq
            | BinaryOperator::Lt
            | BinaryOperator::Gt
            | BinaryOperator::LtEq
            | BinaryOperator::GtEq
            | BinaryOperator::Spaceship
    )
}

fn has_join_pair(expr: &Expr, left_source_name: &str, right_source_name: &str) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            let direct = if is_comparison_operator(op) {
                if let (Some(left_prefix), Some(right_prefix)) =
                    (expr_qualified_prefix(left), expr_qualified_prefix(right))
                {
                    left_prefix == left_source_name && right_prefix == right_source_name
                } else {
                    false
                }
            } else {
                false
            };

            direct
                || has_join_pair(left, left_source_name, right_source_name)
                || has_join_pair(right, left_source_name, right_source_name)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            has_join_pair(inner, left_source_name, right_source_name)
        }
        Expr::InList { expr, list, .. } => {
            has_join_pair(expr, left_source_name, right_source_name)
                || list
                    .iter()
                    .any(|item| has_join_pair(item, left_source_name, right_source_name))
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            has_join_pair(expr, left_source_name, right_source_name)
                || has_join_pair(low, left_source_name, right_source_name)
                || has_join_pair(high, left_source_name, right_source_name)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand
                .as_ref()
                .is_some_and(|operand| has_join_pair(operand, left_source_name, right_source_name))
                || conditions.iter().any(|when| {
                    has_join_pair(&when.condition, left_source_name, right_source_name)
                        || has_join_pair(&when.result, left_source_name, right_source_name)
                })
                || else_result.as_ref().is_some_and(|otherwise| {
                    has_join_pair(otherwise, left_source_name, right_source_name)
                })
        }
        _ => false,
    }
}

/// Produce source-text-level edits that swap individual reversed comparison
/// pairs while preserving original formatting, quoting, and keyword casing.
fn join_condition_autofix_for_sources(
    ctx: &RuleContext,
    on_expr: &Expr,
    current_source: &str,
    previous_sources: &[String],
) -> Option<(Span, String)> {
    if previous_sources.is_empty() {
        return None;
    }
    let sql = ctx.statement_sql();
    let (expr_start, expr_end) = expr_statement_offsets(ctx, on_expr)?;
    if expr_start > expr_end || expr_end > sql.len() {
        return None;
    }
    let expr_span = ctx.span_from_statement_offset(expr_start, expr_end);
    let expr_source = &sql[expr_start..expr_end];

    // Collect text-level edits for each reversed pair.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for previous_source in previous_sources {
        collect_reversed_pair_edits(ctx, on_expr, current_source, previous_source, &mut edits);
    }

    if edits.is_empty() {
        return ast_join_condition_autofix_for_sources(
            on_expr,
            expr_span,
            expr_source,
            current_source,
            previous_sources,
        );
    }

    // Sort edits by position (ascending) for deterministic application.
    edits.sort_by_key(|(start, _, _)| *start);
    edits.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1 && left.2 == right.2);

    // Build replacement by applying text edits to the overall ON expression
    // source span.
    let mut result = String::with_capacity(expr_source.len());
    let mut cursor = expr_start;

    for (edit_start, edit_end, replacement) in &edits {
        if *edit_start < expr_start
            || *edit_end > expr_end
            || *edit_start > *edit_end
            || *edit_start < cursor
        {
            // Overlapping edits — bail out to avoid corruption.
            return ast_join_condition_autofix_for_sources(
                on_expr,
                expr_span,
                expr_source,
                current_source,
                previous_sources,
            );
        }
        result.push_str(&sql[cursor..*edit_start]);
        result.push_str(replacement);
        cursor = *edit_end;
    }
    if cursor > expr_end {
        return ast_join_condition_autofix_for_sources(
            on_expr,
            expr_span,
            expr_source,
            current_source,
            previous_sources,
        );
    }
    result.push_str(&sql[cursor..expr_end]);

    Some((expr_span, result))
}

fn ast_join_condition_autofix_for_sources(
    on_expr: &Expr,
    expr_span: Span,
    expr_source: &str,
    current_source: &str,
    previous_sources: &[String],
) -> Option<(Span, String)> {
    // AST rendering would drop comments; avoid this fallback on commented ON clauses.
    if expr_source.contains("--") || expr_source.contains("/*") {
        return None;
    }
    if previous_sources.is_empty() {
        return None;
    }

    let mut rewritten = on_expr.clone();
    let mut changed = false;
    for previous_source in previous_sources {
        changed |= swap_reversed_pairs_ast(&mut rewritten, current_source, previous_source);
    }
    if !changed {
        return None;
    }
    let replacement = rewritten.to_string();
    if replacement == expr_source {
        return None;
    }
    Some((expr_span, replacement))
}

fn swap_reversed_pairs_ast(expr: &mut Expr, current_source: &str, previous_source: &str) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            if is_comparison_operator(op) {
                let left_prefix = expr_qualified_prefix(left);
                let right_prefix = expr_qualified_prefix(right);
                if left_prefix.as_deref() == Some(current_source)
                    && right_prefix.as_deref() == Some(previous_source)
                {
                    std::mem::swap(left, right);
                    *op = flipped_comparison_operator(op);
                    return true;
                }
            }

            let left_changed = swap_reversed_pairs_ast(left, current_source, previous_source);
            let right_changed = swap_reversed_pairs_ast(right, current_source, previous_source);
            left_changed || right_changed
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::IsTrue(inner)
        | Expr::IsNotTrue(inner)
        | Expr::IsFalse(inner)
        | Expr::IsNotFalse(inner)
        | Expr::IsUnknown(inner)
        | Expr::IsNotUnknown(inner)
        | Expr::Cast { expr: inner, .. } => {
            swap_reversed_pairs_ast(inner, current_source, previous_source)
        }
        Expr::InList {
            expr: target, list, ..
        } => {
            let mut changed = swap_reversed_pairs_ast(target, current_source, previous_source);
            for item in list {
                changed |= swap_reversed_pairs_ast(item, current_source, previous_source);
            }
            changed
        }
        Expr::Between {
            expr: target,
            low,
            high,
            ..
        } => {
            swap_reversed_pairs_ast(target, current_source, previous_source)
                | swap_reversed_pairs_ast(low, current_source, previous_source)
                | swap_reversed_pairs_ast(high, current_source, previous_source)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let mut changed = false;
            if let Some(operand) = operand {
                changed |= swap_reversed_pairs_ast(operand, current_source, previous_source);
            }
            for case_when in conditions {
                changed |= swap_reversed_pairs_ast(
                    &mut case_when.condition,
                    current_source,
                    previous_source,
                );
                changed |=
                    swap_reversed_pairs_ast(&mut case_when.result, current_source, previous_source);
            }
            if let Some(else_result) = else_result {
                changed |= swap_reversed_pairs_ast(else_result, current_source, previous_source);
            }
            changed
        }
        _ => false,
    }
}

fn flipped_comparison_operator(op: &BinaryOperator) -> BinaryOperator {
    match op {
        BinaryOperator::Lt => BinaryOperator::Gt,
        BinaryOperator::Gt => BinaryOperator::Lt,
        BinaryOperator::LtEq => BinaryOperator::GtEq,
        BinaryOperator::GtEq => BinaryOperator::LtEq,
        _ => op.clone(),
    }
}

/// Walk the AST and collect source-text edits for each reversed comparison
/// pair. Each edit replaces `left op right` with `right flipped_op left`
/// using the original source text for both operands.
fn collect_reversed_pair_edits(
    ctx: &RuleContext,
    expr: &Expr,
    current_source: &str,
    previous_source: &str,
    edits: &mut Vec<(usize, usize, String)>,
) {
    let sql = ctx.statement_sql();

    match expr {
        Expr::BinaryOp { left, op, right } => {
            if is_comparison_operator(op) {
                let left_prefix = expr_qualified_prefix(left);
                let right_prefix = expr_qualified_prefix(right);
                if left_prefix.as_deref() == Some(current_source)
                    && right_prefix.as_deref() == Some(previous_source)
                {
                    // Extract source text for left and right operands.
                    if let (Some((l_start, l_end)), Some((r_start, r_end))) = (
                        expr_statement_offsets(ctx, left),
                        expr_statement_offsets(ctx, right),
                    ) {
                        if l_start <= l_end
                            && l_end <= r_start
                            && r_start <= r_end
                            && r_end <= sql.len()
                        {
                            let gap = &sql[l_end..r_start];
                            // Skip if the gap contains a comment — swapping could
                            // misplace or corrupt it.
                            if gap.contains("--") || gap.contains("/*") {
                                return;
                            }
                            let left_text = &sql[l_start..l_end];
                            let right_text = &sql[r_start..r_end];
                            let op_text = flip_operator_text(gap, op);

                            // Replace the entire `left op right` span with `right op left`.
                            let replacement = format!("{right_text}{op_text}{left_text}");
                            edits.push((l_start, r_end, replacement));
                            return; // Don't recurse into children we just handled.
                        }
                    }
                }
            }

            // Recurse into children for logical connectives (AND, OR).
            collect_reversed_pair_edits(ctx, left, current_source, previous_source, edits);
            collect_reversed_pair_edits(ctx, right, current_source, previous_source, edits);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::IsTrue(inner)
        | Expr::IsNotTrue(inner)
        | Expr::IsFalse(inner)
        | Expr::IsNotFalse(inner)
        | Expr::IsUnknown(inner)
        | Expr::IsNotUnknown(inner)
        | Expr::Cast { expr: inner, .. } => {
            collect_reversed_pair_edits(ctx, inner, current_source, previous_source, edits)
        }
        Expr::InList {
            expr: target, list, ..
        } => {
            collect_reversed_pair_edits(ctx, target, current_source, previous_source, edits);
            for item in list {
                collect_reversed_pair_edits(ctx, item, current_source, previous_source, edits);
            }
        }
        Expr::Between {
            expr: target,
            low,
            high,
            ..
        } => {
            collect_reversed_pair_edits(ctx, target, current_source, previous_source, edits);
            collect_reversed_pair_edits(ctx, low, current_source, previous_source, edits);
            collect_reversed_pair_edits(ctx, high, current_source, previous_source, edits);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand {
                collect_reversed_pair_edits(ctx, operand, current_source, previous_source, edits);
            }
            for case_when in conditions {
                collect_reversed_pair_edits(
                    ctx,
                    &case_when.condition,
                    current_source,
                    previous_source,
                    edits,
                );
                collect_reversed_pair_edits(
                    ctx,
                    &case_when.result,
                    current_source,
                    previous_source,
                    edits,
                );
            }
            if let Some(else_result) = else_result {
                collect_reversed_pair_edits(
                    ctx,
                    else_result,
                    current_source,
                    previous_source,
                    edits,
                );
            }
        }
        _ => {}
    }
}

/// Given the source text between the left and right operands (which contains
/// whitespace + operator + whitespace), return it with the operator flipped
/// for directional comparison operators.
fn flip_operator_text(gap: &str, op: &BinaryOperator) -> String {
    match op {
        // Symmetric operators — no change needed.
        BinaryOperator::Eq | BinaryOperator::NotEq | BinaryOperator::Spaceship => gap.to_string(),
        // Directional operators — flip the operator while preserving surrounding whitespace.
        BinaryOperator::Lt => gap.replacen('<', ">", 1),
        BinaryOperator::Gt => gap.replacen('>', "<", 1),
        BinaryOperator::LtEq => gap.replacen("<=", ">=", 1),
        BinaryOperator::GtEq => gap.replacen(">=", "<=", 1),
        _ => gap.to_string(),
    }
}

fn expr_statement_offsets(ctx: &RuleContext, expr: &Expr) -> Option<(usize, usize)> {
    // Statement ranges may intentionally trim leading comments/whitespace.
    // SQLParser span line/column coordinates are often absolute to the
    // original document, so prefer document-level offset conversion when the
    // statement does not start at byte 0.
    if ctx.statement_range.start > 0 {
        if let Some((start, end)) = expr_span_offsets(ctx.sql, expr) {
            if start >= ctx.statement_range.start && end <= ctx.statement_range.end {
                return Some((
                    start - ctx.statement_range.start,
                    end - ctx.statement_range.start,
                ));
            }
        }
    }

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

fn expr_span_offsets(sql: &str, expr: &Expr) -> Option<(usize, usize)> {
    let span = expr.span();
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

fn normalize_source_name(name: &str) -> String {
    name.trim_matches(|ch| matches!(ch, '"' | '`' | '\'' | '[' | ']'))
        .to_ascii_uppercase()
}

fn expr_qualified_prefix(expr: &Expr) -> Option<String> {
    match expr {
        Expr::CompoundIdentifier(parts) if parts.len() > 1 => parts
            .get(parts.len().saturating_sub(2))
            .map(|ident| normalize_source_name(&ident.value)),
        Expr::Nested(inner)
        | Expr::UnaryOp { expr: inner, .. }
        | Expr::Cast { expr: inner, .. } => expr_qualified_prefix(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::{IssueAutofixApplicability, IssuePatchEdit};

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = StructureJoinConditionOrder::default();
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

    // --- Edge cases adopted from sqlfluff ST09 ---

    #[test]
    fn allows_queries_without_joins() {
        let issues = run("select * from foo");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_expected_source_order_in_join_condition() {
        let issues = run("select foo.a, bar.b from foo left join bar on foo.a = bar.a");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_reversed_source_order_in_join_condition() {
        let issues = run("select foo.a, bar.b from foo left join bar on bar.a = foo.a");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_009);

        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected ST009 core autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        let fixed = apply_edits(
            "select foo.a, bar.b from foo left join bar on bar.a = foo.a",
            &autofix.edits,
        );
        assert_eq!(
            fixed,
            "select foo.a, bar.b from foo left join bar on foo.a = bar.a"
        );
    }

    #[test]
    fn allows_unqualified_reference_side() {
        let issues = run("select foo.a, bar.b from foo left join bar on bar.b = a");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_multiple_reversed_subconditions() {
        let issues = run(
            "select foo.a, foo.b, bar.c from foo left join bar on bar.a = foo.a and bar.b = foo.b",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn later_preference_flags_earlier_on_left_side() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "structure.join_condition_order".to_string(),
                serde_json::json!({"preferred_first_table_in_join_clause": "later"}),
            )]),
        };
        let rule = StructureJoinConditionOrder::from_config(&config);
        let sql = "select foo.a, bar.b from foo left join bar on foo.a = bar.a";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_009);
    }

    #[test]
    fn comment_in_join_condition_blocks_safe_autofix_metadata() {
        let sql = "select foo.a, bar.b from foo left join bar on bar.a /*keep*/ = foo.a";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0].autofix.is_none(),
            "comment-bearing join condition should not emit ST009 safe patch metadata"
        );
    }

    #[test]
    fn flags_reversed_non_equality_comparison_operators() {
        // SQLFluff: test_fail_later_table_first_multiple_comparison_operators
        let sql = "select foo.a, bar.b from foo left join bar on bar.a != foo.a and bar.b > foo.b and bar.c <= foo.c";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_009);
    }

    #[test]
    fn flags_reversed_join_inside_bracketed_from() {
        // SQLFluff: test_fail_later_table_first_brackets_after_from
        let sql = "select foo.a, bar.b from (foo left join bar on bar.a = foo.a)";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_009);
    }

    #[test]
    fn flags_spaceship_operator_reversed() {
        // SQLFluff: test_fail_sparksql_lt_eq_gt_operator
        let sql = "SELECT bt.test FROM base_table AS bt INNER JOIN second_table AS st ON st.test <=> bt.test";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_009);
    }

    #[test]
    fn autofix_preserves_parentheses_around_condition() {
        // SQLFluff: test_fail_later_table_first_brackets_after_on
        let sql = "select foo.a, bar.b from foo left join bar on (bar.a = foo.a)";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_edits(sql, &issues[0].autofix.as_ref().unwrap().edits);
        assert_eq!(
            fixed,
            "select foo.a, bar.b from foo left join bar on (foo.a = bar.a)"
        );
    }

    #[test]
    fn autofix_preserves_multiline_formatting_and_keyword_case() {
        // SQLFluff: test_fail_later_table_first_multiple_subconditions
        let sql = "select\n    foo.a,\n    foo.b,\n    bar.c\nfrom foo\nleft join bar\n    on bar.a = foo.a\n    and bar.b = foo.b\n";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_edits(sql, &issues[0].autofix.as_ref().unwrap().edits);
        assert_eq!(
            fixed,
            "select\n    foo.a,\n    foo.b,\n    bar.c\nfrom foo\nleft join bar\n    on foo.a = bar.a\n    and foo.b = bar.b\n"
        );
    }

    #[test]
    fn autofix_flips_directional_comparison_operators() {
        // SQLFluff: test_fail_later_table_first_multiple_comparison_operators (single join)
        let sql = "select foo.a, bar.b from foo left join bar on bar.a != foo.a and bar.b > foo.b and bar.c <= foo.c";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_edits(sql, &issues[0].autofix.as_ref().unwrap().edits);
        assert_eq!(
            fixed,
            "select foo.a, bar.b from foo left join bar on foo.a != bar.a and foo.b < bar.b and foo.c >= bar.c"
        );
    }

    #[test]
    fn autofix_preserves_quoted_identifiers() {
        // SQLFluff: test_fail_later_table_first_quoted_table_not_columns
        let sql = "select\n    \"foo\".\"a\",\n    \"bar\".\"b\"\nfrom foo\nleft join \"bar\"\n    on \"bar\".\"a\" = \"foo\".\"a\"\n    and \"bar\".\"b\" = foo.\"b\"\n";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_edits(sql, &issues[0].autofix.as_ref().unwrap().edits);
        assert_eq!(
            fixed,
            "select\n    \"foo\".\"a\",\n    \"bar\".\"b\"\nfrom foo\nleft join \"bar\"\n    on \"foo\".\"a\" = \"bar\".\"a\"\n    and foo.\"b\" = \"bar\".\"b\"\n"
        );
    }

    #[test]
    fn autofix_reorders_join_conditions_against_any_seen_source_in_chain() {
        // SQLFluff: ST09 flags only the first reversed join per SELECT.
        let sql = "select\n    foo.a,\n    bar.b,\n    baz.c\nfrom foo\nleft join bar\n    on bar.a != foo.a\n    and bar.b > foo.b\n    and bar.c <= foo.c\nleft join baz\n    on baz.a <> foo.a\n    and baz.b >= foo.b\n    and baz.c < foo.c\n";
        let issues = run(sql);
        // Only the first reversed join (bar) is reported per SELECT.
        assert_eq!(issues.len(), 1);
        let fixed = apply_edits(sql, &issues[0].autofix.as_ref().unwrap().edits);
        assert_eq!(
            fixed,
            "select\n    foo.a,\n    bar.b,\n    baz.c\nfrom foo\nleft join bar\n    on foo.a != bar.a\n    and foo.b < bar.b\n    and foo.c >= bar.c\nleft join baz\n    on foo.a <> baz.a\n    and foo.b <= baz.b\n    and foo.c > baz.c\n"
        );
    }

    #[test]
    fn autofix_handles_reversed_join_with_additional_same_table_filter() {
        let sql = "select wur.id from ledger.work_unit_run as wur inner join ledger.work_unit as wu on wu.id = wur.work_unit_id and wu.type = 'job'";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected ST009 autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert!(!autofix.edits.is_empty());

        let fixed = apply_edits(sql, &autofix.edits);
        assert!(
            fixed.contains("wur.work_unit_id = wu.id"),
            "expected reordered join predicate, got: {fixed}"
        );
        assert!(
            fixed.contains("wu.type = 'job'"),
            "expected same-table predicate to be preserved, got: {fixed}"
        );
    }

    #[test]
    fn emits_autofix_for_reversed_workspace_join_after_ordered_prior_join() {
        let sql = "select wur.id from ledger.work_unit_run as wur join ledger.work_unit as wu on wur.work_unit_id = wu.id and wu.type = 'job' inner join ledger.workspace as ws on ws.id = wu.workspace_id left join ledger.usage_line_item as uli on uli.job_run_id = wur.external_run_id";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected ST009 autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);

        let fixed = apply_edits(sql, &autofix.edits);
        assert!(
            fixed.contains("wu.workspace_id = ws.id"),
            "expected reordered workspace join predicate, got: {fixed}"
        );
    }

    #[test]
    fn emits_autofix_for_schema_qualified_reversed_join_condition() {
        let sql = "select ledger.work_unit_run.id from ledger.work_unit_run join ledger.work_unit on ledger.work_unit.id = ledger.work_unit_run.work_unit_id";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected ST009 autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert!(!autofix.edits.is_empty());

        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(
            fixed,
            "select ledger.work_unit_run.id from ledger.work_unit_run join ledger.work_unit on ledger.work_unit_run.work_unit_id = ledger.work_unit.id"
        );
    }

    #[test]
    fn emits_autofix_for_quoted_schema_qualified_reversed_join_condition() {
        let sql = "select \"ledger\".\"work_unit_run\".\"id\" from \"ledger\".\"work_unit_run\" join \"ledger\".\"work_unit\" on \"ledger\".\"work_unit\".\"id\" = \"ledger\".\"work_unit_run\".\"work_unit_id\"";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected ST009 autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert!(!autofix.edits.is_empty());

        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(
            fixed,
            "select \"ledger\".\"work_unit_run\".\"id\" from \"ledger\".\"work_unit_run\" join \"ledger\".\"work_unit\" on \"ledger\".\"work_unit_run\".\"work_unit_id\" = \"ledger\".\"work_unit\".\"id\""
        );
    }

    #[test]
    fn emits_autofix_for_reversed_join_inside_cte_with_additional_join() {
        let sql = "WITH active_jobs AS (\n  SELECT wu.id, wur.id\n  FROM raw.lakeflow_jobs AS j\n  INNER JOIN ledger.work_unit AS wu ON wu.external_id = j.job_id AND wu.type = 'job'\n  INNER JOIN ledger.work_unit_run AS wur ON wur.work_unit_id = wu.id\n)\nSELECT * FROM active_jobs";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected ST009 autofix metadata");
        let fixed = apply_edits(sql, &autofix.edits);
        assert!(
            fixed.contains("j.job_id = wu.external_id"),
            "expected first join to be reordered, got: {fixed}"
        );
        assert!(
            fixed.contains("wu.id = wur.work_unit_id"),
            "expected second join to be reordered in same patch, got: {fixed}"
        );
    }

    #[test]
    fn emits_autofix_for_workspace_join_after_inner_chain() {
        let sql = "SELECT wur.id\nFROM ledger.work_unit_run AS wur\nINNER JOIN ledger.work_unit AS wu ON wur.work_unit_id = wu.id AND wu.type = 'job'\nINNER JOIN ledger.workspace AS ws ON ws.id = wu.workspace_id";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected ST009 autofix metadata");
        let fixed = apply_edits(sql, &autofix.edits);
        assert!(
            fixed.contains("wu.workspace_id = ws.id"),
            "expected workspace join predicate reorder, got: {fixed}"
        );
    }

    #[test]
    fn emits_autofix_for_workspace_join_inside_cte_chain() {
        let sql = "WITH job_run_costs AS (\n    SELECT\n        wur.id AS work_unit_run_id,\n        wu.workspace_id\n    FROM ledger.work_unit_run AS wur\n    INNER\n    JOIN ledger.work_unit AS wu ON wur.work_unit_id = wu.id AND wu.type = 'job'\n    INNER JOIN ledger.workspace AS ws ON ws.id = wu.workspace_id\n)\nSELECT * FROM job_run_costs";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0].autofix.is_some(),
            "expected ST009 autofix metadata for CTE join chain"
        );
        let fixed = apply_edits(
            sql,
            &issues[0]
                .autofix
                .as_ref()
                .expect("expected ST009 autofix metadata")
                .edits,
        );
        assert!(
            fixed.contains("wu.workspace_id = ws.id"),
            "expected workspace join predicate reorder, got: {fixed}"
        );
    }

    #[test]
    fn autofix_handles_insert_select_with_on_conflict_join_chain() {
        let sql = "INSERT INTO metrics.page_performance_summary (\n    route,\n    period_start,\n    period_end,\n    nav_type,\n    mark\n)\nSELECT\n    o.route,\n    p.period_start,\n    p.period_end,\n    o.nav_type,\n    o.mark\nFROM overall AS o\nCROSS JOIN params AS p\nLEFT JOIN device_breakdown AS d\n    ON d.route = o.route AND d.nav_type = o.nav_type AND d.mark = o.mark\nLEFT JOIN network_breakdown AS n\n    ON n.route = o.route AND n.nav_type = o.nav_type AND n.mark = o.mark\nLEFT JOIN version_breakdown AS v\n    ON v.route = o.route AND v.nav_type = o.nav_type AND v.mark = o.mark\nON CONFLICT (route, period_start, nav_type, mark) DO UPDATE SET\n    period_end = excluded.period_end;\n";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected ST009 autofix metadata");
        let fixed = apply_edits(sql, &autofix.edits);
        assert!(
            fixed.contains("ON o.route = d.route AND o.nav_type = d.nav_type AND o.mark = d.mark"),
            "expected first join reordered, got: {fixed}"
        );
        assert!(
            fixed.contains("ON o.route = n.route AND o.nav_type = n.nav_type AND o.mark = n.mark"),
            "expected second join reordered, got: {fixed}"
        );
        assert!(
            fixed.contains("ON o.route = v.route AND o.nav_type = v.nav_type AND o.mark = v.mark"),
            "expected third join reordered, got: {fixed}"
        );
        assert!(
            fixed.contains("ON CONFLICT (route, period_start, nav_type, mark) DO UPDATE SET"),
            "expected ON CONFLICT clause preserved, got: {fixed}"
        );
    }
}
