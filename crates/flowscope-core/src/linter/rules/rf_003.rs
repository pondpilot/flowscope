//! LINT_RF_003: References consistency.
//!
//! In single-source queries, avoid mixing qualified and unqualified references.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::{
    Expr, FunctionArg, FunctionArgExpr, FunctionArguments, Select, SelectItem, Spanned, Statement,
    TableFactor, WindowType,
};
use std::collections::HashSet;

use super::semantic_helpers::{
    select_projection_alias_set, select_source_count, visit_select_expressions,
    visit_selects_in_statement,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SingleTableReferencesMode {
    Consistent,
    Qualified,
    Unqualified,
}

impl SingleTableReferencesMode {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_RF_003, "single_table_references")
            .unwrap_or("consistent")
            .to_ascii_lowercase()
            .as_str()
        {
            "qualified" => Self::Qualified,
            "unqualified" => Self::Unqualified,
            _ => Self::Consistent,
        }
    }

    fn violation(self, qualified: usize, unqualified: usize) -> bool {
        match self {
            Self::Consistent => qualified > 0 && unqualified > 0,
            Self::Qualified => unqualified > 0,
            Self::Unqualified => qualified > 0,
        }
    }
}

pub struct ReferencesConsistent {
    single_table_references: SingleTableReferencesMode,
    force_enable: bool,
}

#[derive(Clone)]
struct Rf003SelectScope {
    start: usize,
    end: usize,
    sources: HashSet<String>,
}

impl ReferencesConsistent {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            single_table_references: SingleTableReferencesMode::from_config(config),
            force_enable: config
                .rule_option_bool(issue_codes::LINT_RF_003, "force_enable")
                .unwrap_or(true),
        }
    }
}

impl Default for ReferencesConsistent {
    fn default() -> Self {
        Self {
            single_table_references: SingleTableReferencesMode::Consistent,
            force_enable: true,
        }
    }
}

impl BuiltinLintRule for ReferencesConsistent {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_003
    }

    fn name(&self) -> &'static str {
        "References consistent"
    }

    fn description(&self) -> &'static str {
        "Column references should be qualified consistently in single table statements."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        if !self.force_enable {
            return Vec::new();
        }

        let mut select_scopes = Vec::new();
        visit_selects_in_statement(statement, &mut |select| {
            if let Some((start, end)) = select_span_offsets(ctx.sql, select) {
                select_scopes.push(Rf003SelectScope {
                    start,
                    end,
                    sources: select_source_names(select),
                });
            }
        });

        let mut mixed_count = 0usize;
        let mut consistency_transition_count = 0usize;
        let mut autofix_edits_raw: Vec<Rf003AutofixEdit> = Vec::new();

        visit_selects_in_statement(statement, &mut |select| {
            if select_source_count(select) != 1 {
                return;
            }
            if select_contains_pivot(select) || select_contains_table_variable_source(select) {
                return;
            }

            let aliases = select_projection_alias_set(select);
            let source_names = select_source_names(select);
            let ancestor_sources = ancestor_source_names_for_select(ctx, select, &select_scopes);
            let (mut qualified, mut unqualified, has_outer_references) =
                count_reference_qualification_for_select(
                    select,
                    &aliases,
                    &source_names,
                    &ancestor_sources,
                    ctx.dialect(),
                );
            let (projection_qualified, projection_unqualified) =
                projection_wildcard_qualification_counts(select);
            qualified += projection_qualified;
            unqualified += projection_unqualified;

            // SQLFluff RF03 parity: correlated subqueries in unqualified mode
            // should not be forced into local-table qualification checks.
            if has_outer_references
                && self.single_table_references == SingleTableReferencesMode::Unqualified
            {
                return;
            }

            if self
                .single_table_references
                .violation(qualified, unqualified)
            {
                mixed_count += 1;
                if self.single_table_references == SingleTableReferencesMode::Consistent
                    && qualified > 0
                    && unqualified > 0
                {
                    // SQLFluff emits an additional "inconsistent with previous
                    // references" issue per mixed single-source SELECT.
                    consistency_transition_count += 1;
                }

                let target_style = match self.single_table_references {
                    SingleTableReferencesMode::Consistent
                    | SingleTableReferencesMode::Qualified => {
                        Some(Rf003AutofixTargetStyle::Qualify)
                    }
                    SingleTableReferencesMode::Unqualified => {
                        Some(Rf003AutofixTargetStyle::Unqualify)
                    }
                };

                if let Some(target_style) = target_style {
                    autofix_edits_raw.extend(rf003_autofix_edits_for_select(
                        select,
                        ctx,
                        target_style,
                        &aliases,
                        &source_names,
                        &ancestor_sources,
                    ));
                }
            }
        });

        if mixed_count == 0 {
            return Vec::new();
        }

        if autofix_edits_raw.is_empty()
            && self.single_table_references == SingleTableReferencesMode::Unqualified
        {
            let sql = ctx.statement_sql();
            if let Some((table_name, alias)) = extract_from_table_and_alias(sql) {
                let prefix = if alias.is_empty() {
                    table_name.rsplit('.').next().unwrap_or(&table_name)
                } else {
                    alias.as_str()
                };
                if !prefix.is_empty() {
                    let rewritten = unqualify_prefix_in_sql_slice(sql, prefix);
                    if rewritten != sql {
                        autofix_edits_raw.push(Rf003AutofixEdit {
                            start: 0,
                            end: sql.len(),
                            replacement: rewritten,
                        });
                    }
                }
            }
        }
        if autofix_edits_raw.is_empty() {
            // Keep legacy text fallback for simple cases not covered by AST spans.
            autofix_edits_raw.extend(mixed_reference_autofix_edits(ctx.statement_sql()));
        }
        autofix_edits_raw.sort_by_key(|edit| (edit.start, edit.end));
        autofix_edits_raw.dedup_by_key(|edit| (edit.start, edit.end));

        let autofix_edits: Vec<IssuePatchEdit> = autofix_edits_raw
            .into_iter()
            .map(|edit| {
                IssuePatchEdit::new(
                    ctx.span_from_statement_offset(edit.start, edit.end),
                    edit.replacement,
                )
            })
            .collect();

        // Emit one issue per violating reference at its specific position
        // (SQLFluff reports per-reference, not per-SELECT).
        if !autofix_edits.is_empty() || consistency_transition_count > 0 {
            let mut issues: Vec<Issue> = autofix_edits
                .into_iter()
                .map(|edit| {
                    let span = Span::new(edit.span.start, edit.span.end);
                    Issue::info(
                        issue_codes::LINT_RF_003,
                        "Avoid mixing qualified and unqualified references.",
                    )
                    .with_statement(ctx.statement_index)
                    .with_span(span)
                    .with_autofix_edits(IssueAutofixApplicability::Safe, vec![edit])
                })
                .collect();
            issues.extend((0..consistency_transition_count).map(|_| {
                Issue::info(
                    issue_codes::LINT_RF_003,
                    "Avoid mixing qualified and unqualified references.",
                )
                .with_statement(ctx.statement_index)
            }));
            return issues;
        }

        // Fallback: no per-reference edits available, emit per-SELECT issues.
        (0..mixed_count)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_RF_003,
                    "Avoid mixing qualified and unqualified references.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn ancestor_source_names_for_select(
    ctx: &RuleContext,
    select: &Select,
    scopes: &[Rf003SelectScope],
) -> HashSet<String> {
    let Some((start, end)) = select_span_offsets(ctx.sql, select) else {
        return HashSet::new();
    };

    let mut out = HashSet::new();
    for scope in scopes {
        let strictly_contains =
            scope.start <= start && scope.end >= end && (scope.start != start || scope.end != end);
        if strictly_contains {
            out.extend(scope.sources.iter().cloned());
        }
    }
    out
}

struct Rf003AutofixEdit {
    start: usize,
    end: usize,
    replacement: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Rf003AutofixTargetStyle {
    Qualify,
    Unqualify,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Rf003ReferenceClass {
    Unqualified,
    LocalQualified,
    ObjectPath,
    Ignore,
}

fn rf003_autofix_edits_for_select(
    select: &Select,
    ctx: &RuleContext,
    target_style: Rf003AutofixTargetStyle,
    aliases: &HashSet<String>,
    local_sources: &HashSet<String>,
    statement_sources: &HashSet<String>,
) -> Vec<Rf003AutofixEdit> {
    let Some(prefix) = preferred_qualification_prefix(select) else {
        return Vec::new();
    };
    if prefix.is_empty() {
        return Vec::new();
    }

    let statement_sql = ctx.statement_sql();
    let mut edits = Vec::new();
    visit_select_expressions(select, &mut |expr| {
        collect_rf003_autofix_edits_in_expr(
            expr,
            ctx,
            statement_sql,
            target_style,
            &prefix,
            aliases,
            local_sources,
            statement_sources,
            ctx.dialect(),
            &mut edits,
        );
    });

    if edits.is_empty() && target_style == Rf003AutofixTargetStyle::Unqualify {
        if let Some((start, end)) = select_statement_offsets(ctx, select) {
            if start < end && end <= statement_sql.len() {
                let original = &statement_sql[start..end];
                let rewritten = unqualify_prefix_in_sql_slice(original, &prefix);
                if rewritten != original {
                    edits.push(Rf003AutofixEdit {
                        start,
                        end,
                        replacement: rewritten,
                    });
                }
            }
        }
    }

    edits
}

fn preferred_qualification_prefix(select: &Select) -> Option<String> {
    let table = select.from.first()?;
    match &table.relation {
        TableFactor::Table { name, alias, .. } => {
            if let Some(alias) = alias {
                return Some(alias.name.value.clone());
            }
            let table_name = name.to_string();
            let last = table_name.rsplit('.').next().unwrap_or(&table_name).trim();
            (!last.is_empty()).then_some(last.to_string())
        }
        TableFactor::Derived { alias, .. }
        | TableFactor::TableFunction { alias, .. }
        | TableFactor::Function { alias, .. }
        | TableFactor::UNNEST { alias, .. }
        | TableFactor::JsonTable { alias, .. }
        | TableFactor::OpenJsonTable { alias, .. }
        | TableFactor::NestedJoin { alias, .. }
        | TableFactor::Pivot { alias, .. }
        | TableFactor::Unpivot { alias, .. }
        | TableFactor::MatchRecognize { alias, .. } => alias.as_ref().map(|a| a.name.value.clone()),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_rf003_autofix_edits_in_expr(
    expr: &Expr,
    ctx: &RuleContext,
    statement_sql: &str,
    target_style: Rf003AutofixTargetStyle,
    prefix: &str,
    aliases: &HashSet<String>,
    local_sources: &HashSet<String>,
    statement_sources: &HashSet<String>,
    dialect: Dialect,
    edits: &mut Vec<Rf003AutofixEdit>,
) {
    match expr {
        Expr::Identifier(_) | Expr::CompoundIdentifier(_) => {
            let class =
                classify_rf003_reference(expr, aliases, local_sources, statement_sources, dialect);
            let Some((start, end)) = expr_statement_offsets(ctx, expr) else {
                return;
            };
            if start >= end || end > statement_sql.len() {
                return;
            }
            let original = &statement_sql[start..end];

            let replacement = match (target_style, class) {
                (Rf003AutofixTargetStyle::Qualify, Rf003ReferenceClass::Unqualified)
                | (Rf003AutofixTargetStyle::Qualify, Rf003ReferenceClass::ObjectPath) => {
                    Some(format!("{prefix}.{original}"))
                }
                (Rf003AutofixTargetStyle::Unqualify, Rf003ReferenceClass::LocalQualified) => {
                    original
                        .find('.')
                        .map(|dot| original[dot + 1..].to_string())
                        .filter(|rest| !rest.is_empty())
                }
                _ => None,
            };

            if let Some(replacement) = replacement {
                if replacement != original {
                    edits.push(Rf003AutofixEdit {
                        start,
                        end,
                        replacement,
                    });
                }
            }
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. } => {
            collect_rf003_autofix_edits_in_expr(
                left,
                ctx,
                statement_sql,
                target_style,
                prefix,
                aliases,
                local_sources,
                statement_sources,
                dialect,
                edits,
            );
            collect_rf003_autofix_edits_in_expr(
                right,
                ctx,
                statement_sql,
                target_style,
                prefix,
                aliases,
                local_sources,
                statement_sources,
                dialect,
                edits,
            );
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => collect_rf003_autofix_edits_in_expr(
            inner,
            ctx,
            statement_sql,
            target_style,
            prefix,
            aliases,
            local_sources,
            statement_sources,
            dialect,
            edits,
        ),
        Expr::InList { expr, list, .. } => {
            collect_rf003_autofix_edits_in_expr(
                expr,
                ctx,
                statement_sql,
                target_style,
                prefix,
                aliases,
                local_sources,
                statement_sources,
                dialect,
                edits,
            );
            for item in list {
                collect_rf003_autofix_edits_in_expr(
                    item,
                    ctx,
                    statement_sql,
                    target_style,
                    prefix,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    edits,
                );
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            collect_rf003_autofix_edits_in_expr(
                expr,
                ctx,
                statement_sql,
                target_style,
                prefix,
                aliases,
                local_sources,
                statement_sources,
                dialect,
                edits,
            );
            collect_rf003_autofix_edits_in_expr(
                low,
                ctx,
                statement_sql,
                target_style,
                prefix,
                aliases,
                local_sources,
                statement_sources,
                dialect,
                edits,
            );
            collect_rf003_autofix_edits_in_expr(
                high,
                ctx,
                statement_sql,
                target_style,
                prefix,
                aliases,
                local_sources,
                statement_sources,
                dialect,
                edits,
            );
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand {
                collect_rf003_autofix_edits_in_expr(
                    operand,
                    ctx,
                    statement_sql,
                    target_style,
                    prefix,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    edits,
                );
            }
            for when in conditions {
                collect_rf003_autofix_edits_in_expr(
                    &when.condition,
                    ctx,
                    statement_sql,
                    target_style,
                    prefix,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    edits,
                );
                collect_rf003_autofix_edits_in_expr(
                    &when.result,
                    ctx,
                    statement_sql,
                    target_style,
                    prefix,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    edits,
                );
            }
            if let Some(otherwise) = else_result {
                collect_rf003_autofix_edits_in_expr(
                    otherwise,
                    ctx,
                    statement_sql,
                    target_style,
                    prefix,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    edits,
                );
            }
        }
        Expr::Function(function) => {
            if let FunctionArguments::List(arguments) = &function.args {
                for (index, arg) in arguments.args.iter().enumerate() {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(expr),
                            ..
                        } => {
                            if should_skip_identifier_reference_for_function_arg(
                                function, index, expr,
                            ) {
                                continue;
                            }
                            collect_rf003_autofix_edits_in_expr(
                                expr,
                                ctx,
                                statement_sql,
                                target_style,
                                prefix,
                                aliases,
                                local_sources,
                                statement_sources,
                                dialect,
                                edits,
                            );
                        }
                        _ => {}
                    }
                }
            }
            if let Some(filter) = &function.filter {
                collect_rf003_autofix_edits_in_expr(
                    filter,
                    ctx,
                    statement_sql,
                    target_style,
                    prefix,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    edits,
                );
            }
            for order_expr in &function.within_group {
                collect_rf003_autofix_edits_in_expr(
                    &order_expr.expr,
                    ctx,
                    statement_sql,
                    target_style,
                    prefix,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    edits,
                );
            }
            if let Some(WindowType::WindowSpec(spec)) = &function.over {
                for expr in &spec.partition_by {
                    collect_rf003_autofix_edits_in_expr(
                        expr,
                        ctx,
                        statement_sql,
                        target_style,
                        prefix,
                        aliases,
                        local_sources,
                        statement_sources,
                        dialect,
                        edits,
                    );
                }
                for order_expr in &spec.order_by {
                    collect_rf003_autofix_edits_in_expr(
                        &order_expr.expr,
                        ctx,
                        statement_sql,
                        target_style,
                        prefix,
                        aliases,
                        local_sources,
                        statement_sources,
                        dialect,
                        edits,
                    );
                }
            }
        }
        Expr::InSubquery { expr, .. } => collect_rf003_autofix_edits_in_expr(
            expr,
            ctx,
            statement_sql,
            target_style,
            prefix,
            aliases,
            local_sources,
            statement_sources,
            dialect,
            edits,
        ),
        Expr::Exists { .. } | Expr::Subquery(_) => {}
        _ => {}
    }
}

fn classify_rf003_reference(
    expr: &Expr,
    aliases: &HashSet<String>,
    local_sources: &HashSet<String>,
    statement_sources: &HashSet<String>,
    dialect: Dialect,
) -> Rf003ReferenceClass {
    match expr {
        Expr::Identifier(identifier) => {
            let name = identifier.value.to_ascii_uppercase();
            if aliases.contains(&name) || identifier.value.starts_with('@') {
                Rf003ReferenceClass::Ignore
            } else {
                Rf003ReferenceClass::Unqualified
            }
        }
        Expr::CompoundIdentifier(parts) => {
            if parts.is_empty() {
                return Rf003ReferenceClass::Ignore;
            }
            let first = parts[0].value.to_ascii_uppercase();
            if first.starts_with('@') {
                return Rf003ReferenceClass::Ignore;
            }
            if parts.len() == 1 {
                if aliases.contains(&first) {
                    Rf003ReferenceClass::Ignore
                } else {
                    Rf003ReferenceClass::Unqualified
                }
            } else if local_sources.contains(&first) {
                Rf003ReferenceClass::LocalQualified
            } else if statement_sources.contains(&first) {
                Rf003ReferenceClass::Ignore
            } else if is_object_reference_dialect(dialect) {
                Rf003ReferenceClass::ObjectPath
            } else {
                Rf003ReferenceClass::LocalQualified
            }
        }
        _ => Rf003ReferenceClass::Ignore,
    }
}

fn expr_statement_offsets(ctx: &RuleContext, expr: &Expr) -> Option<(usize, usize)> {
    // Statement ranges may intentionally trim leading comments/whitespace.
    // SQLParser spans are often absolute to the full document, so prefer
    // document-level conversion when the statement does not start at byte 0.
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

fn select_statement_offsets(ctx: &RuleContext, select: &Select) -> Option<(usize, usize)> {
    // Statement ranges may intentionally trim leading comments/whitespace.
    // SQLParser spans are often absolute to the full document, so prefer
    // document-level conversion when the statement does not start at byte 0.
    if ctx.statement_range.start > 0 {
        if let Some((start, end)) = select_span_offsets(ctx.sql, select) {
            if start >= ctx.statement_range.start && end <= ctx.statement_range.end {
                return Some((
                    start - ctx.statement_range.start,
                    end - ctx.statement_range.start,
                ));
            }
        }
    }

    if let Some((start, end)) = select_span_offsets(ctx.statement_sql(), select) {
        return Some((start, end));
    }

    let (start, end) = select_span_offsets(ctx.sql, select)?;
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

fn select_span_offsets(sql: &str, select: &Select) -> Option<(usize, usize)> {
    let span = select.span();
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

fn unqualify_prefix_in_sql_slice(sql: &str, prefix: &str) -> String {
    let bytes = sql.as_bytes();
    let prefix_bytes = prefix.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0usize;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Outside,
        SingleQuote,
        DoubleQuote,
        BacktickQuote,
        BracketQuote,
        LineComment,
        BlockComment,
    }

    let mut mode = Mode::Outside;

    while i < bytes.len() {
        let b = bytes[i];
        let next = bytes.get(i + 1).copied();

        match mode {
            Mode::Outside => {
                if b == b'-' && next == Some(b'-') {
                    out.push('-');
                    out.push('-');
                    i += 2;
                    mode = Mode::LineComment;
                    continue;
                }
                if b == b'/' && next == Some(b'*') {
                    out.push('/');
                    out.push('*');
                    i += 2;
                    mode = Mode::BlockComment;
                    continue;
                }
                if b == b'\'' {
                    out.push('\'');
                    i += 1;
                    mode = Mode::SingleQuote;
                    continue;
                }
                if b == b'"' {
                    out.push('"');
                    i += 1;
                    mode = Mode::DoubleQuote;
                    continue;
                }
                if b == b'`' {
                    out.push('`');
                    i += 1;
                    mode = Mode::BacktickQuote;
                    continue;
                }
                if b == b'[' {
                    out.push('[');
                    i += 1;
                    mode = Mode::BracketQuote;
                    continue;
                }

                if i + prefix_bytes.len() + 1 < bytes.len()
                    && bytes[i..i + prefix_bytes.len()]
                        .iter()
                        .zip(prefix_bytes.iter())
                        .all(|(actual, expected)| actual.eq_ignore_ascii_case(expected))
                    && (i == 0
                        || !bytes[i - 1].is_ascii_alphanumeric()
                            && bytes[i - 1] != b'_'
                            && bytes[i - 1] != b'$')
                    && bytes[i + prefix_bytes.len()] == b'.'
                {
                    i += prefix_bytes.len() + 1;
                    continue;
                }

                out.push(char::from(b));
                i += 1;
            }
            Mode::SingleQuote => {
                out.push(char::from(b));
                i += 1;
                if b == b'\'' {
                    if next == Some(b'\'') {
                        out.push('\'');
                        i += 1;
                    } else {
                        mode = Mode::Outside;
                    }
                }
            }
            Mode::DoubleQuote => {
                out.push(char::from(b));
                i += 1;
                if b == b'"' {
                    if next == Some(b'"') {
                        out.push('"');
                        i += 1;
                    } else {
                        mode = Mode::Outside;
                    }
                }
            }
            Mode::BacktickQuote => {
                out.push(char::from(b));
                i += 1;
                if b == b'`' {
                    if next == Some(b'`') {
                        out.push('`');
                        i += 1;
                    } else {
                        mode = Mode::Outside;
                    }
                }
            }
            Mode::BracketQuote => {
                out.push(char::from(b));
                i += 1;
                if b == b']' {
                    if next == Some(b']') {
                        out.push(']');
                        i += 1;
                    } else {
                        mode = Mode::Outside;
                    }
                }
            }
            Mode::LineComment => {
                out.push(char::from(b));
                i += 1;
                if b == b'\n' {
                    mode = Mode::Outside;
                }
            }
            Mode::BlockComment => {
                out.push(char::from(b));
                i += 1;
                if b == b'*' && next == Some(b'/') {
                    out.push('/');
                    i += 1;
                    mode = Mode::Outside;
                }
            }
        }
    }

    out
}

fn mixed_reference_autofix_edits(sql: &str) -> Vec<Rf003AutofixEdit> {
    let bytes = sql.as_bytes();
    let Some(select_start) = find_ascii_keyword(bytes, b"SELECT", 0) else {
        return Vec::new();
    };
    let select_end = select_start + b"SELECT".len();
    let Some(from_start) = find_ascii_keyword(bytes, b"FROM", select_end) else {
        return Vec::new();
    };

    let Some((table_name, alias)) = extract_from_table_and_alias(sql) else {
        return Vec::new();
    };
    let prefix = if alias.is_empty() {
        table_name.rsplit('.').next().unwrap_or(&table_name)
    } else {
        alias.as_str()
    };
    if prefix.is_empty() {
        return Vec::new();
    }

    let select_clause = &sql[select_end..from_start];
    let projection_items = split_projection_items(select_clause);
    if projection_items.is_empty() {
        return Vec::new();
    }

    let has_qualified = projection_items
        .iter()
        .any(|(value, _, _)| is_simple_qualified_identifier(value));
    let has_unqualified = projection_items
        .iter()
        .any(|(value, _, _)| is_simple_identifier(value));
    if !(has_qualified && has_unqualified) {
        return Vec::new();
    }

    projection_items
        .into_iter()
        .filter_map(|(value, start, end)| {
            if !is_simple_identifier(&value) {
                return None;
            }
            Some(Rf003AutofixEdit {
                start: select_end + start,
                end: select_end + end,
                replacement: format!("{prefix}.{value}"),
            })
        })
        .collect()
}

fn split_projection_items(select_clause: &str) -> Vec<(String, usize, usize)> {
    let bytes = select_clause.as_bytes();
    let mut out = Vec::new();
    let mut segment_start = 0usize;
    let mut index = 0usize;

    while index <= bytes.len() {
        if index == bytes.len() || bytes[index] == b',' {
            let segment = &select_clause[segment_start..index];
            let leading_trim = segment
                .char_indices()
                .find(|(_, ch)| !ch.is_ascii_whitespace())
                .map(|(idx, _)| idx)
                .unwrap_or(segment.len());
            let trailing_trim = segment
                .char_indices()
                .rfind(|(_, ch)| !ch.is_ascii_whitespace())
                .map(|(idx, ch)| idx + ch.len_utf8())
                .unwrap_or(leading_trim);

            if leading_trim < trailing_trim {
                let value = segment[leading_trim..trailing_trim].to_string();
                out.push((
                    value,
                    segment_start + leading_trim,
                    segment_start + trailing_trim,
                ));
            }
            segment_start = index + 1;
        }
        index += 1;
    }

    out
}

fn extract_from_table_and_alias(sql: &str) -> Option<(String, String)> {
    let bytes = sql.as_bytes();
    let from_start = find_ascii_keyword(bytes, b"FROM", 0)?;
    let mut index = skip_ascii_whitespace(bytes, from_start + b"FROM".len());
    let table_start = index;
    index = consume_ascii_identifier(bytes, index)?;
    while index < bytes.len() && bytes[index] == b'.' {
        let next = consume_ascii_identifier(bytes, index + 1)?;
        index = next;
    }
    let table_name = sql[table_start..index].to_string();

    let mut alias = String::new();
    let after_table = skip_ascii_whitespace(bytes, index);
    if after_table > index {
        if let Some(as_end) = match_ascii_keyword_at(bytes, after_table, b"AS") {
            let alias_start = skip_ascii_whitespace(bytes, as_end);
            if alias_start > as_end {
                if let Some(alias_end) = consume_ascii_identifier(bytes, alias_start) {
                    alias = sql[alias_start..alias_end].to_string();
                }
            }
        } else if let Some(alias_end) = consume_ascii_identifier(bytes, after_table) {
            alias = sql[after_table..alias_end].to_string();
        }
    }

    Some((table_name, alias))
}

fn is_ascii_whitespace_byte(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t' | 0x0b | 0x0c)
}

fn is_ascii_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ascii_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn skip_ascii_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && is_ascii_whitespace_byte(bytes[index]) {
        index += 1;
    }
    index
}

fn consume_ascii_identifier(bytes: &[u8], start: usize) -> Option<usize> {
    if start >= bytes.len() || !is_ascii_ident_start(bytes[start]) {
        return None;
    }
    let mut index = start + 1;
    while index < bytes.len() && is_ascii_ident_continue(bytes[index]) {
        index += 1;
    }
    Some(index)
}

fn is_word_boundary_for_keyword(bytes: &[u8], index: usize) -> bool {
    index == 0 || index >= bytes.len() || !is_ascii_ident_continue(bytes[index])
}

fn match_ascii_keyword_at(bytes: &[u8], start: usize, keyword_upper: &[u8]) -> Option<usize> {
    let end = start.checked_add(keyword_upper.len())?;
    if end > bytes.len() {
        return None;
    }
    if !is_word_boundary_for_keyword(bytes, start.saturating_sub(1))
        || !is_word_boundary_for_keyword(bytes, end)
    {
        return None;
    }
    let matches = bytes[start..end]
        .iter()
        .zip(keyword_upper.iter())
        .all(|(actual, expected)| actual.to_ascii_uppercase() == *expected);
    if matches {
        Some(end)
    } else {
        None
    }
}

fn find_ascii_keyword(bytes: &[u8], keyword_upper: &[u8], from: usize) -> Option<usize> {
    let mut index = from;
    while index + keyword_upper.len() <= bytes.len() {
        if match_ascii_keyword_at(bytes, index, keyword_upper).is_some() {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn is_simple_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !is_ascii_ident_start(bytes[0]) {
        return false;
    }
    bytes[1..].iter().copied().all(is_ascii_ident_continue)
}

fn is_simple_qualified_identifier(value: &str) -> bool {
    let mut parts = value.split('.');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(left), Some(right), None) => {
            is_simple_identifier(left) && is_simple_identifier(right)
        }
        _ => false,
    }
}

fn projection_wildcard_qualification_counts(select: &Select) -> (usize, usize) {
    let mut qualified = 0usize;

    for item in &select.projection {
        match item {
            // SQLFluff RF03 parity: treat qualified wildcards as qualified references.
            SelectItem::QualifiedWildcard(_, _) => qualified += 1,
            // Keep unqualified wildcard neutral to avoid forcing `SELECT *` style choices.
            SelectItem::Wildcard(_) => {}
            _ => {}
        }
    }

    (qualified, 0)
}

fn select_source_names(select: &Select) -> HashSet<String> {
    let mut names = HashSet::new();
    for table in &select.from {
        collect_source_names_from_table_factor(&table.relation, &mut names);
        for join in &table.joins {
            collect_source_names_from_table_factor(&join.relation, &mut names);
        }
    }
    names
}

fn collect_source_names_from_table_factor(table_factor: &TableFactor, names: &mut HashSet<String>) {
    match table_factor {
        TableFactor::Table { name, alias, .. } => {
            if let Some(alias) = alias {
                names.insert(alias.name.value.to_ascii_uppercase());
            }
            let table_name = name.to_string();
            if !table_name.is_empty() {
                let last = table_name
                    .rsplit('.')
                    .next()
                    .unwrap_or(&table_name)
                    .trim_matches(|ch| matches!(ch, '"' | '`' | '[' | ']'))
                    .to_ascii_uppercase();
                if !last.is_empty() {
                    names.insert(last);
                }
            }
        }
        TableFactor::Derived {
            alias: Some(alias), ..
        } => {
            names.insert(alias.name.value.to_ascii_uppercase());
        }
        TableFactor::Derived { alias: None, .. } => {}
        TableFactor::TableFunction { alias, .. }
        | TableFactor::Function { alias, .. }
        | TableFactor::UNNEST { alias, .. }
        | TableFactor::JsonTable { alias, .. }
        | TableFactor::OpenJsonTable { alias, .. } => {
            if let Some(alias) = alias {
                names.insert(alias.name.value.to_ascii_uppercase());
            }
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_source_names_from_table_factor(&table_with_joins.relation, names);
            for join in &table_with_joins.joins {
                collect_source_names_from_table_factor(&join.relation, names);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_source_names_from_table_factor(table, names);
        }
        _ => {}
    }
}

fn select_contains_pivot(select: &Select) -> bool {
    select.from.iter().any(|table| {
        table_factor_contains_pivot(&table.relation)
            || table
                .joins
                .iter()
                .any(|join| table_factor_contains_pivot(&join.relation))
    })
}

fn table_factor_contains_pivot(table_factor: &TableFactor) -> bool {
    match table_factor {
        TableFactor::Pivot { .. } => true,
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            table_factor_contains_pivot(&table_with_joins.relation)
                || table_with_joins
                    .joins
                    .iter()
                    .any(|join| table_factor_contains_pivot(&join.relation))
        }
        TableFactor::Unpivot { table, .. } | TableFactor::MatchRecognize { table, .. } => {
            table_factor_contains_pivot(table)
        }
        _ => false,
    }
}

fn select_contains_table_variable_source(select: &Select) -> bool {
    select.from.iter().any(|table| {
        table_factor_contains_table_variable(&table.relation)
            || table
                .joins
                .iter()
                .any(|join| table_factor_contains_table_variable(&join.relation))
    })
}

fn table_factor_contains_table_variable(table_factor: &TableFactor) -> bool {
    match table_factor {
        TableFactor::Table { name, .. } => name.to_string().trim_start().starts_with('@'),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            table_factor_contains_table_variable(&table_with_joins.relation)
                || table_with_joins
                    .joins
                    .iter()
                    .any(|join| table_factor_contains_table_variable(&join.relation))
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => table_factor_contains_table_variable(table),
        _ => false,
    }
}

fn count_reference_qualification_for_select(
    select: &Select,
    aliases: &HashSet<String>,
    local_sources: &HashSet<String>,
    statement_sources: &HashSet<String>,
    dialect: Dialect,
) -> (usize, usize, bool) {
    let mut qualified = 0usize;
    let mut unqualified = 0usize;
    let mut has_outer_references = false;

    visit_select_expressions(select, &mut |expr| {
        let (q, u) = count_reference_qualification_in_expr_rf03(
            expr,
            aliases,
            local_sources,
            statement_sources,
            dialect,
            &mut has_outer_references,
        );
        qualified += q;
        unqualified += u;
    });

    (qualified, unqualified, has_outer_references)
}

fn count_reference_qualification_in_expr_rf03(
    expr: &Expr,
    aliases: &HashSet<String>,
    local_sources: &HashSet<String>,
    statement_sources: &HashSet<String>,
    dialect: Dialect,
    has_outer_references: &mut bool,
) -> (usize, usize) {
    match expr {
        Expr::Identifier(identifier) => {
            let name = identifier.value.to_ascii_uppercase();
            if aliases.contains(&name) || identifier.value.starts_with('@') {
                (0, 0)
            } else {
                (0, 1)
            }
        }
        Expr::CompoundIdentifier(parts) => {
            if parts.is_empty() {
                return (0, 0);
            }

            let first = parts[0].value.to_ascii_uppercase();
            if first.starts_with('@') {
                return (0, 0);
            }

            if parts.len() == 1 {
                if aliases.contains(&first) {
                    return (0, 0);
                }
                return (0, 1);
            }

            if local_sources.contains(&first) {
                (1, 0)
            } else if statement_sources.contains(&first) {
                *has_outer_references = true;
                (0, 0)
            } else if is_object_reference_dialect(dialect) {
                // BigQuery/Hive/Redshift object-style refs (e.g. a.bar) should
                // behave like unqualified refs unless the prefix is a known source.
                (0, 1)
            } else {
                (1, 0)
            }
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. } => {
            let (lq, lu) = count_reference_qualification_in_expr_rf03(
                left,
                aliases,
                local_sources,
                statement_sources,
                dialect,
                has_outer_references,
            );
            let (rq, ru) = count_reference_qualification_in_expr_rf03(
                right,
                aliases,
                local_sources,
                statement_sources,
                dialect,
                has_outer_references,
            );
            (lq + rq, lu + ru)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => count_reference_qualification_in_expr_rf03(
            inner,
            aliases,
            local_sources,
            statement_sources,
            dialect,
            has_outer_references,
        ),
        Expr::InList { expr, list, .. } => {
            let (mut q, mut u) = count_reference_qualification_in_expr_rf03(
                expr,
                aliases,
                local_sources,
                statement_sources,
                dialect,
                has_outer_references,
            );
            for item in list {
                let (iq, iu) = count_reference_qualification_in_expr_rf03(
                    item,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    has_outer_references,
                );
                q += iq;
                u += iu;
            }
            (q, u)
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            let (eq, eu) = count_reference_qualification_in_expr_rf03(
                expr,
                aliases,
                local_sources,
                statement_sources,
                dialect,
                has_outer_references,
            );
            let (lq, lu) = count_reference_qualification_in_expr_rf03(
                low,
                aliases,
                local_sources,
                statement_sources,
                dialect,
                has_outer_references,
            );
            let (hq, hu) = count_reference_qualification_in_expr_rf03(
                high,
                aliases,
                local_sources,
                statement_sources,
                dialect,
                has_outer_references,
            );
            (eq + lq + hq, eu + lu + hu)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let mut q = 0usize;
            let mut u = 0usize;
            if let Some(operand) = operand {
                let (oq, ou) = count_reference_qualification_in_expr_rf03(
                    operand,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    has_outer_references,
                );
                q += oq;
                u += ou;
            }
            for when in conditions {
                let (cq, cu) = count_reference_qualification_in_expr_rf03(
                    &when.condition,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    has_outer_references,
                );
                let (rq, ru) = count_reference_qualification_in_expr_rf03(
                    &when.result,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    has_outer_references,
                );
                q += cq + rq;
                u += cu + ru;
            }
            if let Some(otherwise) = else_result {
                let (oq, ou) = count_reference_qualification_in_expr_rf03(
                    otherwise,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    has_outer_references,
                );
                q += oq;
                u += ou;
            }
            (q, u)
        }
        Expr::Function(function) => {
            let mut q = 0usize;
            let mut u = 0usize;

            if let FunctionArguments::List(arguments) = &function.args {
                for (index, arg) in arguments.args.iter().enumerate() {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(expr),
                            ..
                        } => {
                            if should_skip_identifier_reference_for_function_arg(
                                function, index, expr,
                            ) {
                                continue;
                            }
                            let (aq, au) = count_reference_qualification_in_expr_rf03(
                                expr,
                                aliases,
                                local_sources,
                                statement_sources,
                                dialect,
                                has_outer_references,
                            );
                            q += aq;
                            u += au;
                        }
                        _ => {}
                    }
                }
            }

            if let Some(filter) = &function.filter {
                let (fq, fu) = count_reference_qualification_in_expr_rf03(
                    filter,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    has_outer_references,
                );
                q += fq;
                u += fu;
            }

            for order_expr in &function.within_group {
                let (oq, ou) = count_reference_qualification_in_expr_rf03(
                    &order_expr.expr,
                    aliases,
                    local_sources,
                    statement_sources,
                    dialect,
                    has_outer_references,
                );
                q += oq;
                u += ou;
            }

            if let Some(WindowType::WindowSpec(spec)) = &function.over {
                for expr in &spec.partition_by {
                    let (pq, pu) = count_reference_qualification_in_expr_rf03(
                        expr,
                        aliases,
                        local_sources,
                        statement_sources,
                        dialect,
                        has_outer_references,
                    );
                    q += pq;
                    u += pu;
                }
                for order_expr in &spec.order_by {
                    let (oq, ou) = count_reference_qualification_in_expr_rf03(
                        &order_expr.expr,
                        aliases,
                        local_sources,
                        statement_sources,
                        dialect,
                        has_outer_references,
                    );
                    q += oq;
                    u += ou;
                }
            }

            (q, u)
        }
        Expr::InSubquery { expr, .. } => count_reference_qualification_in_expr_rf03(
            expr,
            aliases,
            local_sources,
            statement_sources,
            dialect,
            has_outer_references,
        ),
        Expr::Exists { .. } | Expr::Subquery(_) => (0, 0),
        _ => (0, 0),
    }
}

fn is_object_reference_dialect(dialect: Dialect) -> bool {
    matches!(
        dialect,
        Dialect::Bigquery | Dialect::Hive | Dialect::Redshift
    )
}

fn should_skip_identifier_reference_for_function_arg(
    function: &sqlparser::ast::Function,
    arg_index: usize,
    expr: &Expr,
) -> bool {
    let Expr::Identifier(ident) = expr else {
        return false;
    };
    if ident.quote_style.is_some() || !is_date_part_identifier(&ident.value) {
        return false;
    }

    let Some(function_name) = function_name_upper(function) else {
        return false;
    };
    if !is_datepart_function_name(&function_name) {
        return false;
    }

    arg_index <= 1
}

fn function_name_upper(function: &sqlparser::ast::Function) -> Option<String> {
    function
        .name
        .0
        .last()
        .and_then(sqlparser::ast::ObjectNamePart::as_ident)
        .map(|ident| ident.value.to_ascii_uppercase())
}

fn is_datepart_function_name(name: &str) -> bool {
    matches!(
        name,
        "DATEDIFF"
            | "DATE_DIFF"
            | "DATEADD"
            | "DATE_ADD"
            | "DATE_PART"
            | "DATETIME_TRUNC"
            | "TIME_TRUNC"
            | "TIMESTAMP_TRUNC"
            | "TIMESTAMP_DIFF"
            | "TIMESTAMPDIFF"
    )
}

fn is_date_part_identifier(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "YEAR"
            | "QUARTER"
            | "MONTH"
            | "WEEK"
            | "DAY"
            | "DOW"
            | "DOY"
            | "HOUR"
            | "MINUTE"
            | "SECOND"
            | "MILLISECOND"
            | "MICROSECOND"
            | "NANOSECOND"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesConsistent::default();
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn apply_issue_autofix(sql: &str, issue: &Issue) -> Option<String> {
        let autofix = issue.autofix.as_ref()?;
        let mut out = sql.to_string();
        let mut edits = autofix.edits.clone();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        for edit in edits.into_iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    fn apply_all_autofixes(sql: &str, issues: &[Issue]) -> String {
        let mut edits: Vec<_> = issues
            .iter()
            .filter_map(|issue| issue.autofix.as_ref())
            .flat_map(|autofix| autofix.edits.clone())
            .collect();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        edits.dedup_by(|left, right| {
            left.span.start == right.span.start
                && left.span.end == right.span.end
                && left.replacement == right.replacement
        });

        let mut out = sql.to_string();
        for edit in edits.into_iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        out
    }

    // --- Edge cases adopted from sqlfluff RF03 ---

    #[test]
    fn flags_mixed_qualification_single_table() {
        let sql = "SELECT my_tbl.bar, baz FROM my_tbl";
        let issues = run(sql);
        assert_eq!(issues.len(), 2);
        assert!(issues
            .iter()
            .all(|issue| issue.code == issue_codes::LINT_RF_003));
        let issue_with_fix = issues
            .iter()
            .find(|issue| issue.autofix.is_some())
            .expect("issue with autofix");
        let autofix = issue_with_fix.autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, issue_with_fix).expect("apply autofix");
        assert_eq!(fixed, "SELECT my_tbl.bar, my_tbl.baz FROM my_tbl");
    }

    #[test]
    fn allows_consistently_unqualified_references() {
        let issues = run("SELECT bar FROM my_tbl");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_consistently_qualified_references() {
        let issues = run("SELECT my_tbl.bar FROM my_tbl");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_mixed_qualification_in_subquery() {
        let issues = run("SELECT * FROM (SELECT my_tbl.bar, baz FROM my_tbl)");
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn allows_consistent_references_in_subquery() {
        let issues = run("SELECT * FROM (SELECT my_tbl.bar FROM my_tbl)");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_mixed_qualification_with_qualified_wildcard() {
        let issues = run("SELECT my_tbl.*, bar FROM my_tbl");
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn allows_consistent_qualified_wildcard_and_columns() {
        let issues = run("SELECT my_tbl.*, my_tbl.bar FROM my_tbl");
        assert!(issues.is_empty());
    }

    #[test]
    fn qualified_mode_flags_unqualified_references() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.consistent".to_string(),
                serde_json::json!({"single_table_references": "qualified"}),
            )]),
        };
        let rule = ReferencesConsistent::from_config(&config);
        let sql = "SELECT bar FROM my_tbl";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn force_enable_false_disables_rule() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_RF_003".to_string(),
                serde_json::json!({"force_enable": false}),
            )]),
        };
        let rule = ReferencesConsistent::from_config(&config);
        let sql = "SELECT my_tbl.bar, baz FROM my_tbl";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn autofix_uses_document_spans_for_trimmed_statement_ranges() {
        let sql = "-- c1\n-- c2\n-- c3\n-- c4\n-- c5\n-- c6\n-- c7\n-- c8\n-- c9\n-- c10\n-- c11\n-- c12\nSELECT\n    t.a,\n    b,\n    c,\n    d,\n    e,\n    f,\n    g,\n    h,\n    i,\n    j,\n    k,\n    l,\n    m,\n    n\nFROM foo AS t";
        let statements = parse_sql(sql).expect("parse");
        let statement_start = sql.find("SELECT").expect("statement start");
        let statement_end = sql.len();
        let rule = ReferencesConsistent::default();

        let issues = rule.check_with_context(
            &statements[0],
            &RuleContext::new(sql, statement_start..statement_end, 0),
        );

        let fixed = apply_all_autofixes(sql, &issues);
        assert!(fixed.contains("    t.b,"));
        assert!(fixed.contains("    t.n"));
        assert!(fixed.contains("FROM foo AS t"));
        assert!(!fixed.contains("\n    b,"));
    }
}
