//! LINT_ST_006: Structure column order.
//!
//! SQLFluff ST06 parity: prefer wildcards first, then simple column references
//! and casts, before complex expressions (aggregates, window functions, etc.)
//! in SELECT projection lists.
//!
//! The rule only applies to order-insensitive SELECTs: it skips INSERT, MERGE,
//! CREATE TABLE AS, and SELECTs participating in UNION/set operations (where
//! column position is semantically significant).

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::{
    CreateView, Expr, GroupByExpr, Query, Select, SelectItem, SetExpr, Statement, TableFactor,
    Update,
};
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};
use std::collections::{HashMap, HashSet};

pub struct StructureColumnOrder;

impl BuiltinLintRule for StructureColumnOrder {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_006
    }

    fn name(&self) -> &'static str {
        "Structure column order"
    }

    fn description(&self) -> &'static str {
        "Select wildcards then simple targets before calculations and aggregates."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut violations_info: Vec<ViolationInfo> = Vec::new();
        visit_order_safe_selects(statement, &mut |select| {
            if let Some(info) = check_select_order(select) {
                violations_info.push(info);
            }
        });

        let resolved = st006_resolve_violations(ctx, &violations_info);

        // Only emit violations that could be pinpointed in the token stream.
        // When a violation has no span, the detection may be unreliable and
        // SQLFluff would not emit in this case either.
        resolved
            .into_iter()
            .filter_map(|r| {
                let span = r.span?;
                let mut issue = Issue::info(
                    issue_codes::LINT_ST_006,
                    "Prefer simple columns before complex expressions in SELECT.",
                )
                .with_statement(ctx.statement_index)
                .with_span(span);
                if let Some(edits) = r.edits {
                    issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, edits);
                }
                Some(issue)
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Band-based item classification
// ---------------------------------------------------------------------------

/// SQLFluff ST06 classifies items into ordered bands:
///   Band 0: wildcards (`*`, `table.*`)
///   Band 1: simple columns, literals, standalone casts
///   Band 2: complex expressions (aggregates, window functions, arithmetic, etc.)
fn item_band(item: &SelectItem) -> u8 {
    match item {
        SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => 0,
        SelectItem::UnnamedExpr(expr) => expr_band(expr),
        SelectItem::ExprWithAlias { expr, .. } => expr_band(expr),
    }
}

fn expr_band(expr: &Expr) -> u8 {
    match expr {
        // Simple identifiers.
        Expr::Identifier(_) | Expr::CompoundIdentifier(_) => 1,
        // Literal values.
        Expr::Value(_) => 1,
        // Standalone CAST expressions: `CAST(x AS type)` or `x::type`.
        // SQLFluff classifies any top-level cast as band 1 regardless of
        // what is inside the cast (e.g. `EXTRACT(...)::integer` is simple).
        Expr::Cast { .. } => 1,
        // CAST as a function call: `cast(b)` parsed as Function.
        Expr::Function(f) if is_cast_function(f) => 1,
        // Nested/parenthesized expression — unwrap.
        Expr::Nested(inner) => expr_band(inner),
        // Everything else: functions, binary ops, window functions, etc.
        _ => 2,
    }
}

/// Check if a function call is a CAST function (e.g. `cast(b)` parsed as Function).
fn is_cast_function(f: &sqlparser::ast::Function) -> bool {
    use sqlparser::ast::ObjectNamePart;
    f.name
        .0
        .last()
        .and_then(ObjectNamePart::as_ident)
        .is_some_and(|ident| ident.value.eq_ignore_ascii_case("CAST"))
}

// ---------------------------------------------------------------------------
// Violation detection
// ---------------------------------------------------------------------------

struct ViolationInfo {
    /// The band assignments for each projection item.
    bands: Vec<u8>,
    /// Whether the SELECT has implicit column references (GROUP BY 1, 2).
    has_implicit_refs: bool,
    /// Rendered first projection item hint used to align AST violations with
    /// token-scanned projection segments.
    first_item_hint: String,
}

/// Check if a SELECT's projection ordering violates band order.
/// Returns `Some(ViolationInfo)` if there is a violation.
fn check_select_order(select: &Select) -> Option<ViolationInfo> {
    if select.projection.len() < 2 {
        return None;
    }

    let bands: Vec<u8> = select.projection.iter().map(item_band).collect();

    // A violation exists when any item has a lower band than a preceding item.
    let mut max_band = 0u8;
    let mut violated = false;
    for &band in &bands {
        if band < max_band {
            violated = true;
            break;
        }
        max_band = max_band.max(band);
    }

    if !violated {
        return None;
    }

    let has_implicit_refs = has_implicit_column_references(select);

    Some(ViolationInfo {
        bands,
        has_implicit_refs,
        first_item_hint: select
            .projection
            .first()
            .map(std::string::ToString::to_string)
            .unwrap_or_default(),
    })
}

/// Returns true if the SELECT uses positional (numeric) column references
/// in GROUP BY or ORDER BY (e.g. `GROUP BY 1, 2`).
fn has_implicit_column_references(select: &Select) -> bool {
    if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            if matches!(expr, Expr::Value(v) if matches!(v.value, sqlparser::ast::Value::Number(_, _)))
            {
                return true;
            }
        }
    }

    for sort in &select.sort_by {
        if matches!(&sort.expr, Expr::Value(v) if matches!(v.value, sqlparser::ast::Value::Number(_, _)))
        {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Context-aware SELECT visitor (skips order-sensitive contexts)
// ---------------------------------------------------------------------------

/// Visit only SELECTs where column order is NOT semantically significant.
/// Skips INSERT, MERGE, CREATE TABLE AS, and set operations (UNION, etc.).
fn visit_order_safe_selects<F: FnMut(&Select)>(statement: &Statement, visitor: &mut F) {
    match statement {
        Statement::Query(query) => visit_query_selects(query, visitor, false),
        // INSERT: column order matters — skip entirely.
        Statement::Insert(_) => {}
        // MERGE: column order matters — skip entirely.
        Statement::Merge(..) => {}
        // CREATE TABLE AS SELECT: column order matters — skip entirely.
        Statement::CreateTable(create) if create.query.is_some() => {
            // CREATE TABLE AS SELECT — skip.
        }
        // Regular CREATE TABLE without AS SELECT — nothing relevant to visit.
        Statement::CreateView(CreateView { query, .. }) => {
            visit_query_selects(query, visitor, false);
        }
        Statement::Update(Update {
            table,
            from,
            selection,
            ..
        }) => {
            // Visit subqueries in table/from/where but not the statement columns.
            visit_table_factor_selects(&table.relation, visitor);
            for join in &table.joins {
                visit_table_factor_selects(&join.relation, visitor);
            }
            if let Some(from) = from {
                match from {
                    sqlparser::ast::UpdateTableFromKind::BeforeSet(tables)
                    | sqlparser::ast::UpdateTableFromKind::AfterSet(tables) => {
                        for t in tables {
                            visit_table_factor_selects(&t.relation, visitor);
                            for j in &t.joins {
                                visit_table_factor_selects(&j.relation, visitor);
                            }
                        }
                    }
                }
            }
            if let Some(sel) = selection {
                visit_expr_selects(sel, visitor);
            }
        }
        _ => {}
    }
}

fn visit_query_selects<F: FnMut(&Select)>(query: &Query, visitor: &mut F, in_set_operation: bool) {
    let order_sensitive_ctes = order_sensitive_cte_names_for_query(query);

    // Visit CTE definitions — these may or may not be order-sensitive.
    // A CTE is order-sensitive if its output order can flow into a wildcard
    // set operation (`SELECT * ... UNION ...`), including transitive chains
    // such as `base -> union_cte -> final_select`.
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            let cte_name = cte.alias.name.value.to_ascii_uppercase();
            let cte_order_matters =
                in_set_operation || order_sensitive_ctes.contains(cte_name.as_str());
            visit_query_selects(&cte.query, visitor, cte_order_matters);
        }
    }

    visit_set_expr_selects(&query.body, visitor, in_set_operation);
}

fn visit_set_expr_selects<F: FnMut(&Select)>(
    set_expr: &SetExpr,
    visitor: &mut F,
    in_set_operation: bool,
) {
    match set_expr {
        SetExpr::Select(select) => {
            if in_set_operation {
                // In a set operation, column order is semantically significant.
                // Skip both this SELECT and its FROM subqueries, since subquery
                // column order feeds into the set operation result.
                return;
            }
            visitor(select);
            // Visit subqueries in FROM clause.
            for table in &select.from {
                visit_table_factor_selects(&table.relation, visitor);
                for join in &table.joins {
                    visit_table_factor_selects(&join.relation, visitor);
                }
            }
            // Visit subqueries in WHERE, etc.
            if let Some(sel) = &select.selection {
                visit_expr_selects(sel, visitor);
            }
        }
        SetExpr::Query(query) => visit_query_selects(query, visitor, in_set_operation),
        SetExpr::SetOperation { left, right, .. } => {
            // Both sides of a set operation are order-sensitive.
            visit_set_expr_selects(left, visitor, true);
            visit_set_expr_selects(right, visitor, true);
        }
        _ => {}
    }
}

fn visit_table_factor_selects<F: FnMut(&Select)>(table_factor: &TableFactor, visitor: &mut F) {
    match table_factor {
        TableFactor::Derived { subquery, .. } => {
            visit_query_selects(subquery, visitor, false);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            visit_table_factor_selects(&table_with_joins.relation, visitor);
            for join in &table_with_joins.joins {
                visit_table_factor_selects(&join.relation, visitor);
            }
        }
        _ => {}
    }
}

fn visit_expr_selects<F: FnMut(&Select)>(expr: &Expr, visitor: &mut F) {
    match expr {
        Expr::Subquery(query)
        | Expr::Exists {
            subquery: query, ..
        } => visit_query_selects(query, visitor, false),
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => {
            visit_expr_selects(inner, visitor);
            visit_query_selects(subquery, visitor, false);
        }
        Expr::BinaryOp { left, right, .. } => {
            visit_expr_selects(left, visitor);
            visit_expr_selects(right, visitor);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => visit_expr_selects(inner, visitor),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                visit_expr_selects(op, visitor);
            }
            for when in conditions {
                visit_expr_selects(&when.condition, visitor);
                visit_expr_selects(&when.result, visitor);
            }
            if let Some(e) = else_result {
                visit_expr_selects(e, visitor);
            }
        }
        _ => {}
    }
}

fn order_sensitive_cte_names_for_query(query: &Query) -> HashSet<String> {
    let Some(with) = &query.with else {
        return HashSet::new();
    };

    let cte_names: HashSet<String> = with
        .cte_tables
        .iter()
        .map(|cte| cte.alias.name.value.to_ascii_uppercase())
        .collect();
    if cte_names.is_empty() {
        return HashSet::new();
    }

    // Build direct CTE dependency edges per CTE.
    let mut deps_by_cte: HashMap<String, HashSet<String>> = HashMap::new();
    for cte in &with.cte_tables {
        let mut deps = HashSet::new();
        collect_cte_references_in_set_expr(&cte.query.body, &cte_names, &mut deps);
        deps_by_cte.insert(cte.alias.name.value.to_ascii_uppercase(), deps);
    }

    // Seed order-sensitive CTEs from wildcard set operations in this query
    // body and in CTE bodies that are themselves wildcard set operations.
    let mut sensitive = HashSet::new();
    if matches!(query.body.as_ref(), SetExpr::SetOperation { .. })
        && set_expr_has_wildcard_select(&query.body)
    {
        collect_cte_references_in_set_expr(&query.body, &cte_names, &mut sensitive);
    }
    for cte in &with.cte_tables {
        if matches!(cte.query.body.as_ref(), SetExpr::SetOperation { .. })
            && set_expr_has_wildcard_select(&cte.query.body)
        {
            collect_cte_references_in_set_expr(&cte.query.body, &cte_names, &mut sensitive);
        }
    }

    // Propagate sensitivity transitively through CTE dependency edges.
    let mut stack: Vec<String> = sensitive.iter().cloned().collect();
    while let Some(current) = stack.pop() {
        let Some(deps) = deps_by_cte.get(&current) else {
            continue;
        };
        for dep in deps {
            if sensitive.insert(dep.clone()) {
                stack.push(dep.clone());
            }
        }
    }

    sensitive
}

fn collect_cte_references_in_set_expr(
    set_expr: &SetExpr,
    cte_names: &HashSet<String>,
    out: &mut HashSet<String>,
) {
    match set_expr {
        SetExpr::Select(select) => collect_cte_references_in_select(select, cte_names, out),
        SetExpr::SetOperation { left, right, .. } => {
            collect_cte_references_in_set_expr(left, cte_names, out);
            collect_cte_references_in_set_expr(right, cte_names, out);
        }
        SetExpr::Query(query) => collect_cte_references_in_set_expr(&query.body, cte_names, out),
        _ => {}
    }
}

fn collect_cte_references_in_select(
    select: &Select,
    cte_names: &HashSet<String>,
    out: &mut HashSet<String>,
) {
    for table in &select.from {
        collect_cte_references_in_table_factor(&table.relation, cte_names, out);
        for join in &table.joins {
            collect_cte_references_in_table_factor(&join.relation, cte_names, out);
        }
    }
}

fn collect_cte_references_in_table_factor(
    table_factor: &TableFactor,
    cte_names: &HashSet<String>,
    out: &mut HashSet<String>,
) {
    match table_factor {
        TableFactor::Table { name, .. } => {
            if let Some(ident) = name.0.last().and_then(|part| part.as_ident()) {
                let upper = ident.value.to_ascii_uppercase();
                if cte_names.contains(upper.as_str()) {
                    out.insert(upper);
                }
            }
        }
        TableFactor::Derived { subquery, .. } => {
            collect_cte_references_in_set_expr(&subquery.body, cte_names, out);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_cte_references_in_table_factor(&table_with_joins.relation, cte_names, out);
            for join in &table_with_joins.joins {
                collect_cte_references_in_table_factor(&join.relation, cte_names, out);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_cte_references_in_table_factor(table, cte_names, out);
        }
        _ => {}
    }
}

/// Returns true if any leaf SELECT in a set expression uses a wildcard
/// (`SELECT *` or `SELECT table.*`). When a set operation uses wildcards,
/// the column order of referenced CTEs/subqueries is semantically significant.
fn set_expr_has_wildcard_select(set_expr: &SetExpr) -> bool {
    match set_expr {
        SetExpr::Select(select) => select.projection.iter().any(|item| {
            matches!(
                item,
                SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _)
            )
        }),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_has_wildcard_select(left) || set_expr_has_wildcard_select(right)
        }
        SetExpr::Query(query) => set_expr_has_wildcard_select(&query.body),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Autofix
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct PositionedToken {
    token: Token,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug)]
struct SelectProjectionSegment {
    items: Vec<SelectProjectionItem>,
}

#[derive(Clone, Debug)]
struct SelectProjectionItem {
    core_span: Span,
    leading_span: Span,
    trailing_span: Span,
}

#[derive(Clone, Debug)]
struct St006AutofixCandidate {
    span: Span,
    edits: Vec<IssuePatchEdit>,
}

/// A violation resolved to a span (location in token stream), with optional
/// autofix edits. Violations without a span are suppressed — the detection
/// may be unreliable when the token stream doesn't match the AST.
struct ResolvedViolation {
    span: Option<Span>,
    edits: Option<Vec<IssuePatchEdit>>,
}

/// Resolve each violation to a token-stream span and, when safe, autofix edits.
fn st006_resolve_violations(
    ctx: &RuleContext,
    violations: &[ViolationInfo],
) -> Vec<ResolvedViolation> {
    let candidates = st006_autofix_candidates_for_context(ctx, violations);

    // If candidates align 1:1 with violations, use them for spans and edits.
    if candidates.len() == violations.len() {
        return candidates
            .into_iter()
            .map(|c| ResolvedViolation {
                span: Some(c.span),
                edits: if c.edits.is_empty() {
                    None
                } else {
                    Some(c.edits)
                },
            })
            .collect();
    }

    // Candidates don't align — try to at least find spans by matching
    // segments to violations.
    let spans = st006_violation_spans(ctx, violations);
    if spans.len() == violations.len() {
        return spans
            .into_iter()
            .map(|span| ResolvedViolation {
                span: Some(span),
                edits: None,
            })
            .collect();
    }

    // Can't resolve any violations to spans.
    violations
        .iter()
        .map(|_| ResolvedViolation {
            span: None,
            edits: None,
        })
        .collect()
}

fn positioned_tokens_for_context(ctx: &RuleContext) -> Vec<PositionedToken> {
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
        tokens
    } else {
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
        positioned
    }
}

fn st006_autofix_candidates_for_context(
    ctx: &RuleContext,
    violations: &[ViolationInfo],
) -> Vec<St006AutofixCandidate> {
    let tokens = positioned_tokens_for_context(ctx);
    let segments = select_projection_segments(&tokens);

    // We need to match segments to violations. Since we skip some SELECTs
    // (set operations, INSERT, etc.), the segments from token scanning may
    // not 1:1 align with violations. Use positional matching.
    // If counts don't match, bail out.
    if segments.len() < violations.len() {
        return Vec::new();
    }

    // Find segments that have violations by scanning all segments and matching
    // against the violation bands. For each segment, compute its bands from
    // token-based item count and see if it matches a violation.
    let mut candidates = Vec::new();
    let mut violation_idx = 0;
    for segment in &segments {
        if violation_idx >= violations.len() {
            break;
        }
        let violation = &violations[violation_idx];
        if segment.items.len() != violation.bands.len() {
            continue;
        }
        if !segment_first_item_matches(ctx.sql, segment, &violation.first_item_hint) {
            continue;
        }

        // Skip autofix if implicit column references exist.
        if violation.has_implicit_refs {
            violation_idx += 1;
            continue;
        }

        if let Some(candidate) =
            projection_reorder_candidate_by_band(ctx.sql, &tokens, segment, &violation.bands)
        {
            candidates.push(candidate);
        }
        violation_idx += 1;
    }

    candidates
}

/// Resolve violations to spans without autofix, using the same segment-matching
/// logic as the autofix path but without requiring the reorder to succeed.
fn st006_violation_spans(ctx: &RuleContext, violations: &[ViolationInfo]) -> Vec<Span> {
    let tokens = positioned_tokens_for_context(ctx);
    let segments = select_projection_segments(&tokens);

    if segments.len() < violations.len() {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let mut violation_idx = 0;
    for segment in &segments {
        if violation_idx >= violations.len() {
            break;
        }
        let violation = &violations[violation_idx];
        if segment.items.len() != violation.bands.len() {
            continue;
        }
        if !segment_first_item_matches(ctx.sql, segment, &violation.first_item_hint) {
            continue;
        }

        // Use the first item span as the violation location.
        if let Some(first) = segment.items.first() {
            spans.push(first.core_span);
        }
        violation_idx += 1;
    }

    spans
}

fn select_projection_segments(tokens: &[PositionedToken]) -> Vec<SelectProjectionSegment> {
    let significant_positions: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| (!is_trivia(&token.token)).then_some(index))
        .collect();
    if significant_positions.is_empty() {
        return Vec::new();
    }

    let mut depths = vec![0usize; significant_positions.len()];
    let mut depth = 0usize;
    for (position, token_index) in significant_positions.iter().copied().enumerate() {
        depths[position] = depth;
        match tokens[token_index].token {
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            _ => {}
        }
    }

    let mut segments = Vec::new();
    for position in 0..significant_positions.len() {
        let token = &tokens[significant_positions[position]].token;
        if !token_word_equals(token, "SELECT") {
            continue;
        }

        let base_depth = depths[position];
        let Some(projection_start) = projection_start_after_select(
            tokens,
            &significant_positions,
            &depths,
            position + 1,
            base_depth,
        ) else {
            continue;
        };
        let Some(from_position) = from_position_for_select(
            tokens,
            &significant_positions,
            &depths,
            projection_start,
            base_depth,
        ) else {
            continue;
        };
        if from_position <= projection_start {
            continue;
        }

        let items = projection_items(
            tokens,
            &significant_positions,
            &depths,
            projection_start,
            from_position,
            base_depth,
        );
        if items.is_empty() {
            continue;
        }

        segments.push(SelectProjectionSegment { items });
    }

    segments
}

fn projection_start_after_select(
    tokens: &[PositionedToken],
    significant_positions: &[usize],
    depths: &[usize],
    mut position: usize,
    base_depth: usize,
) -> Option<usize> {
    while position < significant_positions.len() {
        if depths[position] != base_depth {
            return Some(position);
        }

        let token = &tokens[significant_positions[position]].token;
        if token_word_equals(token, "DISTINCT")
            || token_word_equals(token, "ALL")
            || token_word_equals(token, "DISTINCTROW")
        {
            position += 1;
            continue;
        }
        return Some(position);
    }

    None
}

fn from_position_for_select(
    tokens: &[PositionedToken],
    significant_positions: &[usize],
    depths: &[usize],
    start_position: usize,
    base_depth: usize,
) -> Option<usize> {
    (start_position..significant_positions.len()).find(|&position| {
        depths[position] == base_depth
            && token_word_equals(&tokens[significant_positions[position]].token, "FROM")
    })
}

fn projection_items(
    tokens: &[PositionedToken],
    significant_positions: &[usize],
    depths: &[usize],
    start_position: usize,
    from_position: usize,
    base_depth: usize,
) -> Vec<SelectProjectionItem> {
    if start_position >= from_position {
        return Vec::new();
    }

    let mut core_items: Vec<(usize, usize, Option<usize>)> = Vec::new();
    let mut item_start = start_position;

    for position in start_position..from_position {
        let token = &tokens[significant_positions[position]].token;
        if depths[position] == base_depth && matches!(token, Token::Comma) {
            if item_start < position {
                core_items.push((item_start, position - 1, Some(position)));
            }
            item_start = position + 1;
        }
    }

    if item_start < from_position {
        core_items.push((item_start, from_position - 1, None));
    }

    if core_items.is_empty() {
        return Vec::new();
    }

    let mut items = Vec::with_capacity(core_items.len());
    let mut previous_comma_end = 0usize;
    for (index, (core_start_position, core_end_position, comma_position)) in
        core_items.iter().copied().enumerate()
    {
        let Some(core_span) = span_from_positions(
            tokens,
            significant_positions,
            core_start_position,
            core_end_position,
        ) else {
            return Vec::new();
        };

        let leading_start = if index == 0 {
            core_span.start
        } else {
            previous_comma_end
        };
        let leading_end = core_span.start;

        let trailing_end = if let Some(comma_position) = comma_position {
            tokens[significant_positions[comma_position]].start
        } else {
            core_span.end
        };
        if trailing_end < core_span.end {
            return Vec::new();
        }

        let leading_span = Span::new(leading_start, leading_end);
        let trailing_span = Span::new(core_span.end, trailing_end);
        items.push(SelectProjectionItem {
            core_span,
            leading_span,
            trailing_span,
        });

        if let Some(comma_position) = comma_position {
            previous_comma_end = tokens[significant_positions[comma_position]].end;
        }
    }

    items
}

fn span_from_positions(
    tokens: &[PositionedToken],
    significant_positions: &[usize],
    start_position: usize,
    end_position: usize,
) -> Option<Span> {
    if end_position < start_position {
        return None;
    }

    let start = tokens[*significant_positions.get(start_position)?].start;
    let end = tokens[*significant_positions.get(end_position)?].end;
    (start < end).then_some(Span::new(start, end))
}

/// Reorder projection items by band, preserving relative order within each band.
fn projection_reorder_candidate_by_band(
    sql: &str,
    tokens: &[PositionedToken],
    segment: &SelectProjectionSegment,
    bands: &[u8],
) -> Option<St006AutofixCandidate> {
    if segment.items.len() != bands.len() {
        return None;
    }

    let replace_span = Span::new(
        segment.items.first()?.core_span.start,
        segment.items.last()?.core_span.end,
    );
    if replace_span.start >= replace_span.end || replace_span.end > sql.len() {
        return None;
    }

    let mut normalized_items = Vec::with_capacity(segment.items.len());
    for item in &segment.items {
        let core_span = item.core_span;
        if core_span.start >= core_span.end || core_span.end > sql.len() {
            return None;
        }
        if trailing_span_has_inline_comment(tokens, sql, core_span, item.trailing_span) {
            return None;
        }
        let text = sql[core_span.start..core_span.end].trim();
        let normalized = normalize_projection_item_text(text)?;

        let leading = if item.leading_span.start < item.leading_span.end
            && item.leading_span.end <= sql.len()
        {
            &sql[item.leading_span.start..item.leading_span.end]
        } else {
            ""
        };
        let trailing = if item.trailing_span.start < item.trailing_span.end
            && item.trailing_span.end <= sql.len()
        {
            &sql[item.trailing_span.start..item.trailing_span.end]
        } else {
            ""
        };
        normalized_items.push((normalized, leading, trailing));
    }

    // Stable sort by band — items with same band keep their relative order.
    let mut indexed: Vec<(usize, u8)> = normalized_items
        .iter()
        .enumerate()
        .zip(bands.iter())
        .map(|((i, _), &band)| (i, band))
        .collect();
    indexed.sort_by_key(|&(i, band)| (band, i));

    let original_segment = &sql[replace_span.start..replace_span.end];
    let replacement = if original_segment.contains('\n') || original_segment.contains('\r') {
        let indent = indent_prefix_for_offset(sql, segment.items.first()?.core_span.start);
        let default_separator = format!(",\n{indent}");
        let mut rewritten = String::new();
        for (position, &(item_index, _)) in indexed.iter().enumerate() {
            let (core_text, leading, trailing) = &normalized_items[item_index];
            if position > 0 {
                if leading.is_empty() {
                    rewritten.push_str(&default_separator);
                } else {
                    rewritten.push(',');
                    rewritten.push_str(leading);
                }
            }
            rewritten.push_str(core_text);
            rewritten.push_str(trailing);
        }
        rewritten
    } else {
        indexed
            .iter()
            .map(|(item_index, _)| normalized_items[*item_index].0.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    if replacement.is_empty() || replacement == original_segment {
        return None;
    }

    Some(St006AutofixCandidate {
        span: replace_span,
        edits: vec![IssuePatchEdit::new(replace_span, replacement)],
    })
}

fn trailing_span_has_inline_comment(
    tokens: &[PositionedToken],
    sql: &str,
    core_span: Span,
    trailing_span: Span,
) -> bool {
    if trailing_span.start >= trailing_span.end {
        return false;
    }
    tokens.iter().any(|token| {
        if token.start < trailing_span.start
            || token.end > trailing_span.end
            || !is_comment(&token.token)
        {
            return false;
        }
        if token.start <= core_span.end || token.start > sql.len() || core_span.end > sql.len() {
            return true;
        }
        let between = &sql[core_span.end..token.start];
        !between.contains('\n') && !between.contains('\r')
    })
}

fn normalize_projection_item_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn segment_first_item_matches(sql: &str, segment: &SelectProjectionSegment, hint: &str) -> bool {
    let Some(first_item) = segment.items.first() else {
        return false;
    };
    let span = first_item.core_span;
    if span.start >= span.end || span.end > sql.len() {
        return false;
    }
    normalize_item_hint(&sql[span.start..span.end]) == normalize_item_hint(hint)
}

fn normalize_item_hint(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(|ch| ch.to_uppercase())
        .collect()
}

fn indent_prefix_for_offset(sql: &str, offset: usize) -> String {
    let start = sql[..offset].rfind('\n').map_or(0, |idx| idx + 1);
    sql[start..offset]
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .collect()
}

// ---------------------------------------------------------------------------
// Token utilities
// ---------------------------------------------------------------------------

fn tokenize_with_spans(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
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
        let rule = StructureColumnOrder;
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

    // --- Pass cases from SQLFluff ST06 fixture ---

    #[test]
    fn pass_select_statement_order() {
        // a (simple), cast(b) (cast), c (simple) — all in band 1, no violation.
        let issues = run("SELECT a, cast(b as int) as b, c FROM x");
        assert!(issues.is_empty());
    }

    #[test]
    fn pass_union_statements_ignored() {
        let sql = "SELECT a + b as c, d FROM table_a UNION ALL SELECT c, d FROM table_b";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn pass_insert_statements_ignored() {
        let sql = "\
INSERT INTO example_schema.example_table
(id, example_column, rank_asc, rank_desc)
SELECT
    id,
    CASE WHEN col_a IN('a', 'b', 'c') THEN col_a END AS example_column,
    rank_asc,
    rank_desc
FROM another_schema.another_table";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn pass_insert_statement_with_cte_ignored() {
        let sql = "\
INSERT INTO my_table
WITH my_cte AS (SELECT * FROM t1)
SELECT MAX(field1), field2
FROM t1";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn with_cte_insert_into_still_checks_cte() {
        // WITH ... INSERT INTO ... is parsed by sqlparser as
        // Statement::Query { body: SetExpr::Insert(...) }.
        // SQLFluff still checks CTE SELECTs for ordering — only the
        // INSERT body's own SELECT is skipped.
        let sql = "\
WITH my_cte AS (
    SELECT MAX(field1) AS mx, field2 FROM t1
)
INSERT INTO my_table (col1, col2)
SELECT mx, field2 FROM my_cte";
        let issues = run(sql);
        // CTE has MAX(field1) (band 2) before field2 (band 1) → violation.
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn with_cte_insert_into_no_violation_when_ordered() {
        // When the CTE projection is already ordered, no violation.
        let sql = "\
WITH my_cte AS (
    SELECT field2, MAX(field1) AS mx FROM t1
)
INSERT INTO my_table (col1, col2)
SELECT mx, field2 FROM my_cte";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn pass_merge_statements_ignored() {
        let sql = "\
MERGE INTO t
USING
(
    SELECT
        DATE_TRUNC('DAY', end_time) AS time_day,
        b
    FROM u
) AS u ON (a = b)
WHEN MATCHED THEN
UPDATE SET a = b
WHEN NOT MATCHED THEN
INSERT (b) VALUES (c)";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn pass_merge_statement_with_cte_ignored() {
        let sql = "\
MERGE INTO t
USING
(
    WITH my_cte AS (SELECT * FROM t1)
    SELECT MAX(field1), field2
    FROM t1
) AS u ON (a = b)
WHEN MATCHED THEN
UPDATE SET a = b
WHEN NOT MATCHED THEN
INSERT (b) VALUES (c)";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn pass_create_table_as_select_with_cte_ignored() {
        let sql = "\
CREATE TABLE new_table AS (
  WITH my_cte AS (SELECT * FROM t1)
  SELECT MAX(field1), field2
  FROM t1
)";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn pass_cte_used_in_set() {
        let sql = "\
WITH T1 AS (
  SELECT
    'a'::varchar AS A,
    1::bigint AS B
),
T2 AS (
  SELECT
    CASE WHEN COL > 1 THEN 'x' ELSE 'y' END AS A,
    COL AS B
  FROM T
)
SELECT * FROM T1
UNION ALL
SELECT * FROM T2";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn fail_cte_used_in_set_with_explicit_columns() {
        // When the set operation uses explicit column lists (not SELECT *),
        // CTE projection order is irrelevant and ST06 should still apply.
        let sql = "\
WITH T1 AS (
  SELECT
    'a'::varchar AS A,
    1::bigint AS B
),
T2 AS (
  SELECT
    CASE WHEN COL > 1 THEN 'x' ELSE 'y' END AS A,
    COL AS B
  FROM T
)
SELECT A, B FROM T1
UNION ALL
SELECT A, B FROM T2";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_006);
    }

    #[test]
    fn pass_transitive_cte_dependency_into_wildcard_set_operation() {
        // base_a/base_b feed wildcard UNION CTE `combined`, so their projection
        // order is semantically significant and ST06 should not apply.
        let sql = "\
WITH base_a AS (
  SELECT MAX(a) AS mx, b FROM t
),
base_b AS (
  SELECT MAX(c) AS mx, d FROM t2
),
combined AS (
  SELECT * FROM base_a
  UNION ALL
  SELECT * FROM base_b
)
SELECT mx, b FROM combined";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn pass_subquery_used_in_set() {
        let sql = "\
SELECT * FROM (SELECT 'a'::varchar AS A, 1::bigint AS B)
UNION ALL
SELECT * FROM (SELECT CASE WHEN COL > 1 THEN 'x' ELSE 'y' END AS A, COL AS B FROM T)";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    // --- Fail cases from SQLFluff ST06 fixture ---

    #[test]
    fn fail_select_statement_order_1() {
        // a (band 1), row_number() over (...) (band 2), b (band 1) → violation.
        let sql = "SELECT a, row_number() over (partition by id order by date) as y, b FROM x";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_006);
    }

    #[test]
    fn fail_select_statement_order_2() {
        // row_number() (band 2), * (band 0), cast(b) (band 1) → violation.
        let sql = "SELECT row_number() over (partition by id order by date) as y, *, cast(b as int) as b_int FROM x";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn fail_select_statement_order_3() {
        // row_number() (band 2), cast(b) (band 1), * (band 0) → violation.
        let sql = "SELECT row_number() over (partition by id order by date) as y, cast(b as int) as b_int, * FROM x";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn fail_select_statement_order_4() {
        // row_number() (band 2), b::int (band 1), * (band 0) → violation.
        let sql = "SELECT row_number() over (partition by id order by date) as y, b::int, * FROM x";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn fail_select_statement_order_5() {
        // row_number() (band 2), * (band 0), 2::int + 4 (band 2), cast(b) (band 1) → violation.
        let sql = "SELECT row_number() over (partition by id order by date) as y, *, 2::int + 4 as sum, cast(b) as c FROM x";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
    }

    // --- Autofix tests ---

    #[test]
    fn autofix_reorder_simple_before_complex() {
        let sql = "SELECT a + 1, a FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0].autofix.as_ref().expect("expected ST006 autofix");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(fixed, "SELECT a, a + 1 FROM t");
    }

    #[test]
    fn autofix_reorder_wildcard_first() {
        let sql = "SELECT row_number() over (partition by id order by date) as y, *, cast(b as int) as b_int FROM x";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0].autofix.as_ref().expect("expected ST006 autofix");
        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(fixed, "SELECT *, cast(b as int) as b_int, row_number() over (partition by id order by date) as y FROM x");
    }

    #[test]
    fn autofix_reorder_with_casts() {
        let sql = "SELECT row_number() over (partition by id order by date) as y, b::int, * FROM x";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0].autofix.as_ref().expect("expected ST006 autofix");
        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(
            fixed,
            "SELECT *, b::int, row_number() over (partition by id order by date) as y FROM x"
        );
    }

    #[test]
    fn autofix_fail_order_5_complex() {
        // row_number() (2), * (0), 2::int + 4 (2), cast(b) (1)
        // Expected: * (0), cast(b) (1), row_number() (2), 2::int + 4 (2)
        let sql = "SELECT row_number() over (partition by id order by date) as y, *, 2::int + 4 as sum, cast(b) as c FROM x";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0].autofix.as_ref().expect("expected ST006 autofix");
        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(fixed, "SELECT *, cast(b) as c, row_number() over (partition by id order by date) as y, 2::int + 4 as sum FROM x");
    }

    #[test]
    fn no_autofix_with_implicit_column_references() {
        let sql =
            "SELECT DATE_TRUNC('DAY', end_time) AS time_day, b_field FROM table_name GROUP BY 1, 2";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0].autofix.is_none(),
            "should not autofix when implicit column references exist"
        );
    }

    #[test]
    fn autofix_explicit_column_references() {
        let sql = "SELECT DATE_TRUNC('DAY', end_time) AS time_day, b_field FROM table_name GROUP BY time_day, b_field";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0].autofix.as_ref().expect("expected ST006 autofix");
        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(fixed, "SELECT b_field, DATE_TRUNC('DAY', end_time) AS time_day FROM table_name GROUP BY time_day, b_field");
    }

    #[test]
    fn autofix_reorders_multiline_targets_without_quotes() {
        let sql = "SELECT\n    SUM(a) AS total,\n    a\nFROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0].autofix.as_ref().expect("expected ST006 autofix");
        let fixed = apply_edits(sql, &autofix.edits);
        assert!(
            fixed.contains("a,\n    SUM(a) AS total"),
            "expected reordered multiline projection, got: {fixed}"
        );

        parse_sql(&fixed).expect("fixed SQL should remain parseable");
    }

    #[test]
    fn autofix_reorders_multiline_targets_with_inter_item_comment() {
        let sql = "SELECT\n    -- total usage for period\n    SUM(a) AS total,\n    a\nFROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0].autofix.as_ref().expect("expected ST006 autofix");
        let fixed = apply_edits(sql, &autofix.edits);
        assert!(
            fixed.contains("-- total usage for period"),
            "expected inter-item comment to be preserved, got: {fixed}"
        );
        assert!(
            fixed.contains("a,\n    SUM(a) AS total"),
            "expected reordered projection, got: {fixed}"
        );

        parse_sql(&fixed).expect("fixed SQL should remain parseable");
    }

    #[test]
    fn autofix_reorders_trailing_simple_column_after_subquery_expressions() {
        let sql = "SELECT\n    a.table_full_name AS table_a,\n    b.table_full_name AS table_b,\n    (\n        SELECT count(*)\n        FROM unnest(a.columns) AS ac\n        WHERE ac = ANY(b.columns)\n    ) AS intersection_size,\n    a.column_count + b.column_count - (\n        SELECT count(*)\n        FROM unnest(a.columns) AS ac\n        WHERE ac = ANY(b.columns)\n    ) AS union_size,\n    a.connector_id\nFROM table_columns AS a\nINNER JOIN table_columns AS b\n    ON a.connector_id = b.connector_id";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected ST006 autofix metadata");
        let fixed = apply_edits(sql, &autofix.edits);
        assert!(
            fixed.contains("a.connector_id,\n    (\n        SELECT count(*)"),
            "expected simple trailing column to move before complex expressions, got: {fixed}"
        );
        parse_sql(&fixed).expect("fixed SQL should remain parseable");
    }

    #[test]
    fn fail_cte_used_in_select_not_set() {
        // CTE used in a regular SELECT (not UNION), so ST06 should apply to the CTE.
        let sql = "\
WITH T2 AS (
  SELECT
    CASE WHEN COL > 1 THEN 'x' ELSE 'y' END AS A,
    COL AS B
  FROM T
)
SELECT * FROM T2";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn comment_in_projection_blocks_safe_autofix_metadata() {
        let sql = "SELECT a + 1 /*keep*/, a FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0].autofix.is_none(),
            "comment-bearing projection should not receive ST006 safe patch metadata"
        );
    }

    // --- Existing tests ---

    #[test]
    fn does_not_flag_when_simple_target_starts_projection() {
        let issues = run("SELECT a, a + 1 FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_simple_target_after_complex() {
        // SQLFluff ST06 flags `b` (band 1) appearing after `a + 1` (band 2).
        let issues = run("SELECT a, a + 1, b FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn does_not_flag_when_alias_wraps_simple_identifier() {
        let issues = run("SELECT a AS first_a, b FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_in_nested_select_scopes() {
        let issues = run("SELECT * FROM (SELECT a + 1, a FROM t) AS sub");
        assert_eq!(issues.len(), 1);
    }
}
