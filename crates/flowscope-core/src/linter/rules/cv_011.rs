//! LINT_CV_011: Casting style.
//!
//! SQLFluff CV11 parity: detect mixed use of `::`, `CAST()`, and `CONVERT()`
//! within the same statement and emit autofix edits to normalise to the
//! preferred style.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::linter::visit::visit_expressions;
use crate::types::{issue_codes, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::{CastKind, DataType, Expr, Spanned, Statement};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreferredTypeCastingStyle {
    Consistent,
    Shorthand,
    Cast,
    Convert,
}

impl PreferredTypeCastingStyle {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_CV_011, "preferred_type_casting_style")
            .unwrap_or("consistent")
            .to_ascii_lowercase()
            .as_str()
        {
            "shorthand" => Self::Shorthand,
            "cast" => Self::Cast,
            "convert" => Self::Convert,
            _ => Self::Consistent,
        }
    }
}

// ---------------------------------------------------------------------------
// Cast-expression descriptor
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CastStyle {
    FunctionCast,
    DoubleColon,
    Convert,
}

/// A single cast expression found in the statement.
struct CastInstance {
    style: CastStyle,
    /// Byte range of the whole expression in the full SQL text.
    start: usize,
    end: usize,
    /// Whether the cast contains embedded comments and should not be auto-fixed.
    has_comments: bool,
    /// For CONVERT: true if it has 3+ arguments (style argument) — can't be converted.
    is_3arg_convert: bool,
}

// ---------------------------------------------------------------------------
// Rule struct
// ---------------------------------------------------------------------------

pub struct ConventionCastingStyle {
    preferred_style: PreferredTypeCastingStyle,
}

impl ConventionCastingStyle {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            preferred_style: PreferredTypeCastingStyle::from_config(config),
        }
    }
}

impl Default for ConventionCastingStyle {
    fn default() -> Self {
        Self {
            preferred_style: PreferredTypeCastingStyle::Consistent,
        }
    }
}

impl BuiltinLintRule for ConventionCastingStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_011
    }

    fn name(&self) -> &'static str {
        "Casting style"
    }

    fn description(&self) -> &'static str {
        "Enforce consistent type casting style."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let sql = ctx.sql;
        let casts = collect_cast_instances(statement, sql);

        if casts.is_empty() {
            return Vec::new();
        }

        // Determine the target style.
        let target = match self.preferred_style {
            PreferredTypeCastingStyle::Consistent => casts[0].style,
            PreferredTypeCastingStyle::Shorthand => CastStyle::DoubleColon,
            PreferredTypeCastingStyle::Cast => CastStyle::FunctionCast,
            PreferredTypeCastingStyle::Convert => CastStyle::Convert,
        };

        // Check if there is a violation at all.
        let has_violation = casts.iter().any(|c| c.style != target);
        if !has_violation {
            return Vec::new();
        }

        let message = match self.preferred_style {
            PreferredTypeCastingStyle::Consistent => {
                "Use consistent casting style (avoid mixing CAST styles)."
            }
            PreferredTypeCastingStyle::Shorthand => "Use `::` shorthand casting style.",
            PreferredTypeCastingStyle::Cast => "Use `CAST(...)` style casts.",
            PreferredTypeCastingStyle::Convert => "Use `CONVERT(...)` style casts.",
        };

        // Emit one issue per non-conforming cast so that partially fixable
        // statements (e.g. 3-arg CONVERT that can't be converted) still show
        // improvement in the violation count after autofix.
        let mut issues = Vec::new();
        for cast in &casts {
            if cast.style == target {
                continue;
            }

            let mut issue =
                Issue::info(issue_codes::LINT_CV_011, message).with_statement(ctx.statement_index);

            if !cast.is_3arg_convert && !cast.has_comments {
                let cast_text = &sql[cast.start..cast.end];
                if let Some(replacement) = convert_cast(cast_text, cast.style, target) {
                    issue = issue.with_autofix_edits(
                        IssueAutofixApplicability::Unsafe,
                        vec![IssuePatchEdit::new(
                            Span::new(cast.start, cast.end),
                            replacement,
                        )],
                    );
                }
            }

            issues.push(issue);
        }

        issues
    }
}

// ---------------------------------------------------------------------------
// Collect all cast expressions with their source positions
// ---------------------------------------------------------------------------

fn collect_cast_instances(statement: &Statement, sql: &str) -> Vec<CastInstance> {
    let mut casts = Vec::new();

    visit_expressions(statement, &mut |expr| {
        match expr {
            Expr::Cast {
                kind,
                expr: inner,
                data_type,
                ..
            } => {
                let style = match kind {
                    CastKind::DoubleColon => CastStyle::DoubleColon,
                    CastKind::Cast | CastKind::TryCast | CastKind::SafeCast => {
                        CastStyle::FunctionCast
                    }
                };

                // For chained :: (inner is also ::), skip the inner — we handle
                // the entire chain as one entry via the outermost.
                let is_inner_chain = matches!(
                    inner.as_ref(),
                    Expr::Cast {
                        kind: CastKind::DoubleColon,
                        ..
                    }
                );

                // Get the inner expression's byte range.
                let inner_span = find_cast_span(sql, inner, kind.clone(), data_type);
                if let Some((start, end)) = inner_span {
                    let text = &sql[start..end];
                    let has_comments = text.contains("--") || text.contains("/*");

                    if style == CastStyle::DoubleColon && is_inner_chain {
                        // Outermost chained :: — remove previously collected inner.
                        casts.retain(|c: &CastInstance| c.start < start || c.end > end);
                    }

                    casts.push(CastInstance {
                        style,
                        start,
                        end,
                        has_comments,
                        is_3arg_convert: false,
                    });
                }
            }
            Expr::Function(function)
                if function.name.to_string().eq_ignore_ascii_case("CONVERT") =>
            {
                if let Some((start, mut end)) = expr_span_offsets(sql, expr) {
                    // Function::span() may not include the closing paren.
                    // Scan forward to include it.
                    if end < sql.len() && sql.as_bytes().get(end) == Some(&b')') {
                        end += 1;
                    } else {
                        // Try to find the closing paren after the span end.
                        if let Some(close) = find_matching_close_paren(&sql[end..]) {
                            end += close + 1;
                        }
                    }

                    let text = &sql[start..end];
                    let has_comments = text.contains("--") || text.contains("/*");

                    let arg_count = match &function.args {
                        sqlparser::ast::FunctionArguments::List(list) => list.args.len(),
                        _ => 0,
                    };

                    casts.push(CastInstance {
                        style: CastStyle::Convert,
                        start,
                        end,
                        has_comments,
                        is_3arg_convert: arg_count > 2,
                    });
                }
            }
            _ => {}
        }
    });

    // Parser span extraction can miss parenthesized shorthand casts in some
    // Snowflake semi-structured forms. Add a lightweight lexical fallback.
    for (start, end) in scan_parenthesized_shorthand_cast_spans(sql) {
        if casts.iter().any(|cast| {
            cast.start == start && cast.end == end && cast.style == CastStyle::DoubleColon
        }) {
            continue;
        }
        let text = &sql[start..end];
        casts.push(CastInstance {
            style: CastStyle::DoubleColon,
            start,
            end,
            has_comments: text.contains("--") || text.contains("/*"),
            is_3arg_convert: false,
        });
    }

    // Sort by position so first-seen logic works correctly.
    casts.sort_by_key(|c| c.start);

    // Deduplicate: remove entries whose ranges are fully contained within
    // another entry's range (handles chained :: where both outer and inner
    // are collected by the visitor). For overlapping shorthand casts, keep
    // the wider range so we don't emit conflicting nested edits.
    let mut deduped: Vec<CastInstance> = Vec::with_capacity(casts.len());
    for cast in casts {
        let mut dominated = false;
        let mut replace_index = None;

        for (index, other) in deduped.iter().enumerate() {
            if other.start <= cast.start && other.end >= cast.end {
                dominated = true;
                break;
            }
            if cast.start <= other.start && cast.end >= other.end {
                replace_index = Some(index);
                break;
            }
            if cast.style == other.style
                && spans_overlap(cast.start, cast.end, other.start, other.end)
            {
                let cast_len = cast.end.saturating_sub(cast.start);
                let other_len = other.end.saturating_sub(other.start);
                if cast_len > other_len {
                    replace_index = Some(index);
                } else {
                    dominated = true;
                }
                break;
            }
        }

        if dominated {
            continue;
        }

        if let Some(index) = replace_index {
            deduped[index] = cast;
        } else {
            deduped.push(cast);
        }
    }

    deduped.sort_by_key(|cast| (cast.start, cast.end, cast.style as u8));
    deduped.dedup_by(|left, right| left.start == right.start && left.end == right.end);
    deduped
}

fn spans_overlap(left_start: usize, left_end: usize, right_start: usize, right_end: usize) -> bool {
    left_start < right_end && right_start < left_end
}

fn scan_parenthesized_shorthand_cast_spans(sql: &str) -> Vec<(usize, usize)> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut index = 0usize;

    while index + 1 < bytes.len() {
        if bytes[index] != b':' || bytes[index + 1] != b':' {
            index += 1;
            continue;
        }

        let mut lhs_end = index;
        while lhs_end > 0 && bytes[lhs_end - 1].is_ascii_whitespace() {
            lhs_end -= 1;
        }
        if lhs_end == 0 || bytes[lhs_end - 1] != b')' {
            index += 2;
            continue;
        }
        let close_paren = lhs_end - 1;
        let Some(open_paren) = find_matching_open_paren(bytes, close_paren) else {
            index += 2;
            continue;
        };

        let Some(type_end) = scan_parenthesized_shorthand_type_end(bytes, index + 2) else {
            index += 2;
            continue;
        };

        out.push((open_paren, type_end));
        index = type_end;
    }

    out
}

fn scan_parenthesized_shorthand_type_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    let mut depth = 0i32;
    let mut saw_any = false;

    while index < bytes.len() {
        match bytes[index] {
            b'(' => {
                depth += 1;
                saw_any = true;
                index += 1;
            }
            b')' if depth > 0 => {
                depth -= 1;
                index += 1;
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'.' => {
                saw_any = true;
                index += 1;
            }
            b',' if depth > 0 => index += 1,
            b' ' | b'\t' | b'\n' | b'\r' if depth > 0 => index += 1,
            _ => break,
        }
    }

    if saw_any {
        Some(index)
    } else {
        None
    }
}

fn find_matching_open_paren(bytes: &[u8], close_paren: usize) -> Option<usize> {
    if bytes.get(close_paren).copied() != Some(b')') {
        return None;
    }
    let mut depth = 1i32;
    let mut cursor = close_paren;
    while cursor > 0 {
        cursor -= 1;
        match bytes[cursor] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(cursor);
                }
            }
            _ => {}
        }
    }
    None
}

/// Find the full source span of a CAST/:: expression.
///
/// sqlparser's `Expr::Cast.span()` only returns the inner expression's span,
/// so we compute the full span by:
/// - For CAST/TRY_CAST/SAFE_CAST: scan backwards from inner expr to find the
///   keyword, then forwards to find the closing paren.
/// - For `::`: find the deepest base expression, use its span as start, then
///   scan forwards through all `::type` segments to find the outermost end.
fn find_cast_span(
    sql: &str,
    inner: &Expr,
    kind: CastKind,
    data_type: &DataType,
) -> Option<(usize, usize)> {
    match kind {
        CastKind::Cast | CastKind::TryCast | CastKind::SafeCast => {
            let (inner_start, inner_end) = expr_span_offsets(sql, inner)?;

            // Scan backwards from inner_start to find `CAST(`, `TRY_CAST(`, or `SAFE_CAST(`.
            let before = &sql[..inner_start];
            let paren_pos = before.rfind('(')?;
            let before_paren = before[..paren_pos].trim_end();
            let kw = match kind {
                CastKind::TryCast => "TRY_CAST",
                CastKind::SafeCast => "SAFE_CAST",
                _ => "CAST",
            };
            let kw_len = kw.len();
            if before_paren.len() < kw_len {
                return None;
            }
            let kw_candidate = &before_paren[before_paren.len() - kw_len..];
            if !kw_candidate.eq_ignore_ascii_case(kw) {
                return None;
            }
            let start = before_paren.len() - kw_len;

            // Scan forwards from inner_end to find closing paren.
            let after = &sql[inner_end..];
            let close = find_matching_close_paren(after)?;
            let end = inner_end + close + 1;

            Some((start, end))
        }
        CastKind::DoubleColon => {
            // Find the deepest non-:: base expression to get the real start.
            let base = deepest_base_expr(inner);
            let (base_start, base_end) = expr_span_offsets(sql, base)?;

            // Scan forward from base_end through all `::type` segments.
            let type_str = data_type.to_string();
            let mut pos = base_end;
            loop {
                let after = &sql[pos..];
                let dc_pos = match after.find("::") {
                    Some(p) => p,
                    None => break,
                };
                let type_start = pos + dc_pos + 2;
                let type_len = source_type_len(sql, type_start, &type_str);
                if type_len == 0 {
                    break;
                }
                pos = type_start + type_len;
                // Check if this type matches the outermost data_type.
                let this_type = &sql[type_start..pos];
                if this_type.eq_ignore_ascii_case(&type_str) {
                    break;
                }
            }

            Some((base_start, pos))
        }
    }
}

/// Walk down the `Expr::Cast { kind: DoubleColon }` chain to find the
/// deepest non-Cast base expression.
fn deepest_base_expr(expr: &Expr) -> &Expr {
    match expr {
        Expr::Cast {
            kind: CastKind::DoubleColon,
            expr: inner,
            ..
        } => deepest_base_expr(inner),
        _ => expr,
    }
}

/// Find the position of the matching closing paren in `text`, accounting for
/// nesting. Returns offset relative to `text` start.
fn find_matching_close_paren(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Determine the length of a type name in the source SQL starting at `pos`.
/// The type in source may use different casing or spacing than `DataType::to_string()`.
fn source_type_len(sql: &str, pos: usize, type_display: &str) -> usize {
    // The type ends at the first character that can't be part of a type name.
    // Type names consist of alphanumeric chars, `_`, `(`, `)`, `,`, spaces
    // (for compound types like `CHARACTER VARYING(10)`).
    // We use the Display length as a guide but match against the actual source.
    let remaining = &sql[pos..];
    let display_len = type_display.len();

    // Try exact match first (common case).
    if remaining.len() >= display_len && remaining[..display_len].eq_ignore_ascii_case(type_display)
    {
        return display_len;
    }

    // Fallback: scan forward through identifier-like characters.
    let mut len = 0;
    let mut depth = 0i32;
    for &b in remaining.as_bytes() {
        match b {
            b'(' => {
                depth += 1;
                len += 1;
            }
            b')' if depth > 0 => {
                depth -= 1;
                len += 1;
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' => len += 1,
            b' ' | b'\t' | b'\n' | b',' if depth > 0 => len += 1,
            _ => break,
        }
    }
    len
}

// ---------------------------------------------------------------------------
// Convert a cast expression to the target style
// ---------------------------------------------------------------------------

fn convert_cast(cast_text: &str, from_style: CastStyle, to_style: CastStyle) -> Option<String> {
    match (from_style, to_style) {
        (CastStyle::FunctionCast, CastStyle::DoubleColon) => cast_to_shorthand(cast_text),
        (CastStyle::FunctionCast, CastStyle::Convert) => cast_to_convert(cast_text),
        (CastStyle::DoubleColon, CastStyle::FunctionCast) => shorthand_to_cast(cast_text),
        (CastStyle::DoubleColon, CastStyle::Convert) => shorthand_to_convert(cast_text),
        (CastStyle::Convert, CastStyle::FunctionCast) => convert_to_cast(cast_text),
        (CastStyle::Convert, CastStyle::DoubleColon) => convert_to_shorthand(cast_text),
        _ => None,
    }
}

/// Parse the interior of `CAST(expr AS type)` from raw text.
/// Returns `(expr_text, type_text)`.
fn parse_cast_interior(cast_text: &str) -> Option<(&str, &str)> {
    let open = cast_text.find('(')?;
    let close = cast_text.rfind(')')?;
    let inner = cast_text[open + 1..close].trim();

    let as_pos = find_top_level_as(inner)?;
    let expr_part = inner[..as_pos].trim();
    // The `AS` keyword is typically 2 chars, but ` AS ` starts with a space.
    // `as_pos` points to the space/newline before `AS`.
    let type_part = inner[as_pos + 1..].trim();
    // Strip the leading `AS` keyword.
    let type_part = type_part
        .strip_prefix("AS")
        .or_else(|| type_part.strip_prefix("as"))
        .or_else(|| type_part.strip_prefix("As"))
        .or_else(|| type_part.strip_prefix("aS"))
        .unwrap_or(type_part)
        .trim();
    Some((expr_part, type_part))
}

/// Find the position of top-level whitespace-AS-whitespace in CAST interior.
fn find_top_level_as(inner: &str) -> Option<usize> {
    let bytes = inner.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ if depth == 0
                && is_whitespace_byte(bytes[i])
                && i + 3 < bytes.len()
                && bytes[i + 1].eq_ignore_ascii_case(&b'A')
                && bytes[i + 2].eq_ignore_ascii_case(&b'S')
                && is_whitespace_byte(bytes[i + 3]) =>
            {
                return Some(i);
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn is_whitespace_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// `CAST(expr AS type)` → `expr::type` or `(expr)::type`.
fn cast_to_shorthand(cast_text: &str) -> Option<String> {
    let (expr, type_text) = parse_cast_interior(cast_text)?;
    let needs_parens = expr_is_complex(expr);
    if needs_parens {
        Some(format!("({expr})::{type_text}"))
    } else {
        Some(format!("{expr}::{type_text}"))
    }
}

/// `CAST(expr AS type)` → `convert(type, expr)`.
fn cast_to_convert(cast_text: &str) -> Option<String> {
    let (expr, type_text) = parse_cast_interior(cast_text)?;
    Some(format!("convert({type_text}, {expr})"))
}

/// `CONVERT(type, expr)` → `cast(expr as type)`.
fn convert_to_cast(convert_text: &str) -> Option<String> {
    let (type_text, expr) = parse_convert_interior(convert_text)?;
    Some(format!("cast({expr} as {type_text})"))
}

/// `CONVERT(type, expr)` → `expr::type` or `(expr)::type`.
fn convert_to_shorthand(convert_text: &str) -> Option<String> {
    let (type_text, expr) = parse_convert_interior(convert_text)?;
    let needs_parens = expr_is_complex(expr);
    if needs_parens {
        Some(format!("({expr})::{type_text}"))
    } else {
        Some(format!("{expr}::{type_text}"))
    }
}

/// `expr::type` → `cast(expr as type)`.
/// Handles chained casts: `expr::t1::t2` → `cast(cast(expr as t1) as t2)`.
fn shorthand_to_cast(shorthand_text: &str) -> Option<String> {
    let parts = split_shorthand_chain(shorthand_text)?;
    if parts.len() < 2 {
        return None;
    }
    let mut result = rewrite_nested_simple_shorthand_to_cast(parts[0]);
    for type_part in &parts[1..] {
        result = format!("cast({result} as {type_part})");
    }
    Some(result)
}

/// `expr::type` → `convert(type, expr)`.
/// Handles chained casts: `expr::t1::t2` → `convert(t2, convert(t1, expr))`.
fn shorthand_to_convert(shorthand_text: &str) -> Option<String> {
    let parts = split_shorthand_chain(shorthand_text)?;
    if parts.len() < 2 {
        return None;
    }
    let mut result = parts[0].to_string();
    for type_part in &parts[1..] {
        result = format!("convert({type_part}, {result})");
    }
    Some(result)
}

/// Split a `::` chain like `100::int::text` into `["100", "int", "text"]`.
fn split_shorthand_chain(text: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let bytes = text.as_bytes();
    let mut last_split = 0;

    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b':' if depth == 0 && i + 1 < bytes.len() && bytes[i + 1] == b':' => {
                parts.push(&text[last_split..i]);
                i += 2;
                last_split = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(&text[last_split..]);

    if parts.len() >= 2 {
        Some(parts)
    } else {
        None
    }
}

/// Rewrites simple nested shorthand fragments in an expression, e.g.
/// `value:Longitude::varchar` -> `cast(value:Longitude as varchar)`.
/// This is intentionally conservative: it only rewrites contiguous identifier
/// chains and leaves complex nested expressions to the outer conversion pass.
fn rewrite_nested_simple_shorthand_to_cast(expr: &str) -> String {
    let bytes = expr.as_bytes();
    let mut index = 0usize;
    let mut out = String::with_capacity(expr.len() + 16);

    while index < bytes.len() {
        let Some(rel_dc) = expr[index..].find("::") else {
            out.push_str(&expr[index..]);
            break;
        };
        let dc = index + rel_dc;

        let mut lhs_start = dc;
        while lhs_start > 0 && is_simple_shorthand_lhs_char(bytes[lhs_start - 1]) {
            lhs_start -= 1;
        }
        if lhs_start == dc {
            out.push_str(&expr[index..dc + 2]);
            index = dc + 2;
            continue;
        }

        let mut rhs_end = dc + 2;
        while rhs_end < bytes.len() && is_simple_type_char(bytes[rhs_end]) {
            rhs_end += 1;
        }
        if rhs_end == dc + 2 {
            out.push_str(&expr[index..dc + 2]);
            index = dc + 2;
            continue;
        }

        out.push_str(&expr[index..lhs_start]);
        out.push_str("cast(");
        out.push_str(&expr[lhs_start..dc]);
        out.push_str(" as ");
        out.push_str(&expr[dc + 2..rhs_end]);
        out.push(')');
        index = rhs_end;
    }

    out
}

fn is_simple_shorthand_lhs_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'_' | b'.' | b':' | b'$' | b'@' | b'"' | b'`' | b'[' | b']'
        )
}

fn is_simple_type_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'_' | b' ' | b'\t' | b'\n' | b'\r' | b'(' | b')' | b','
        )
}

/// Parse interior of `CONVERT(type, expr)`. Returns `(type_text, expr_text)`.
fn parse_convert_interior(convert_text: &str) -> Option<(&str, &str)> {
    let open = convert_text.find('(')?;
    let close = convert_text.rfind(')')?;
    let inner = convert_text[open + 1..close].trim();
    let comma = find_top_level_comma(inner)?;
    let type_part = inner[..comma].trim();
    let expr_part = inner[comma + 1..].trim();
    Some((type_part, expr_part))
}

/// Find position of the first top-level comma.
fn find_top_level_comma(inner: &str) -> Option<usize> {
    let bytes = inner.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b',' if depth == 0 => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Returns true if the expression text is "complex" and needs parenthesization
/// when used in shorthand `::` form.
fn expr_is_complex(expr: &str) -> bool {
    let trimmed = expr.trim();
    let bytes = trimmed.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'\'' | b'"' => return false, // string literal — not complex
            b'|' | b'+' | b'-' | b'*' | b'/' | b'%' if depth == 0 => {
                if b == b'-' && i == 0 {
                    continue;
                }
                return true;
            }
            b' ' | b'\t' | b'\n' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Span helpers
// ---------------------------------------------------------------------------

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

    let mut col = 1usize;
    for (idx, _ch) in sql[line_start..].char_indices() {
        if col == column {
            return Some(line_start + idx);
        }
        col += 1;
    }
    if col == column {
        return Some(sql.len());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionCastingStyle::default();
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn run_with_config(sql: &str, config: &LintConfig) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionCastingStyle::from_config(config);
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn apply_edits(sql: &str, edits: &[IssuePatchEdit]) -> String {
        let mut sorted: Vec<_> = edits.iter().collect();
        sorted.sort_by_key(|e| std::cmp::Reverse(e.span.start));
        let mut result = sql.to_string();
        for edit in sorted {
            result.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        result
    }

    fn collect_all_edits(issues: &[Issue]) -> Vec<&IssuePatchEdit> {
        issues
            .iter()
            .filter_map(|i| i.autofix.as_ref())
            .flat_map(|a| a.edits.iter())
            .collect()
    }

    fn apply_all_fixes(sql: &str, issues: &[Issue]) -> String {
        let edits = collect_all_edits(issues);
        let owned: Vec<IssuePatchEdit> = edits.into_iter().cloned().collect();
        apply_edits(sql, &owned)
    }

    #[test]
    fn flags_mixed_casting_styles() {
        let issues = run("SELECT CAST(amount AS INT)::TEXT FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_011);
    }

    #[test]
    fn does_not_flag_single_casting_style() {
        assert!(run("SELECT amount::INT FROM t").is_empty());
        assert!(run("SELECT CAST(amount AS INT) FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_cast_like_tokens_inside_string_literal() {
        assert!(run("SELECT 'value::TEXT and CAST(value AS INT)' AS note").is_empty());
    }

    #[test]
    fn flags_mixed_try_cast_and_double_colon_styles() {
        let issues = run("SELECT TRY_CAST(amount AS INT)::TEXT FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_011);
    }

    #[test]
    fn shorthand_preference_flags_cast_function_style() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.casting_style".to_string(),
                serde_json::json!({"preferred_type_casting_style": "shorthand"}),
            )]),
        };
        let rule = ConventionCastingStyle::from_config(&config);
        let sql = "SELECT CAST(amount AS INT) FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn cast_preference_flags_shorthand_style() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_011".to_string(),
                serde_json::json!({"preferred_type_casting_style": "cast"}),
            )]),
        };
        let rule = ConventionCastingStyle::from_config(&config);
        let sql = "SELECT amount::INT FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Autofix tests — SQLFluff CV11 fixture parity
    // -----------------------------------------------------------------------

    #[test]
    fn autofix_consistent_prior_convert() {
        let sql = "select\n    convert(int, 1) as bar,\n    100::int::text,\n    cast(10\n    as text) as coo\nfrom foo;";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    convert(int, 1) as bar,\n    convert(text, convert(int, 100)),\n    convert(text, 10) as coo\nfrom foo;"
        );
    }

    #[test]
    fn autofix_consistent_prior_cast() {
        let sql = "select\n    cast(10 as text) as coo,\n    convert(int, 1) as bar,\n    100::int::text,\nfrom foo;";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    cast(10 as text) as coo,\n    cast(1 as int) as bar,\n    cast(cast(100 as int) as text),\nfrom foo;"
        );
    }

    #[test]
    fn autofix_consistent_prior_shorthand() {
        let sql = "select\n    100::int::text,\n    cast(10 as text) as coo,\n    convert(int, 1) as bar\nfrom foo;";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    100::int::text,\n    10::text as coo,\n    1::int as bar\nfrom foo;"
        );
    }

    #[test]
    fn autofix_config_cast() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.casting_style".to_string(),
                serde_json::json!({"preferred_type_casting_style": "cast"}),
            )]),
        };
        let sql = "select\n    convert(int, 1) as bar,\n    100::int::text,\n    cast(10 as text) as coo\nfrom foo;";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    cast(1 as int) as bar,\n    cast(cast(100 as int) as text),\n    cast(10 as text) as coo\nfrom foo;"
        );
    }

    #[test]
    fn autofix_config_convert() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.casting_style".to_string(),
                serde_json::json!({"preferred_type_casting_style": "convert"}),
            )]),
        };
        let sql = "select\n    convert(int, 1) as bar,\n    100::int::text,\n    cast(10 as text) as coo\nfrom foo;";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    convert(int, 1) as bar,\n    convert(text, convert(int, 100)),\n    convert(text, 10) as coo\nfrom foo;"
        );
    }

    #[test]
    fn autofix_config_shorthand() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.casting_style".to_string(),
                serde_json::json!({"preferred_type_casting_style": "shorthand"}),
            )]),
        };
        let sql = "select\n    convert(int, 1) as bar,\n    100::int::text,\n    cast(10 as text) as coo\nfrom foo;";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    1::int as bar,\n    100::int::text,\n    10::text as coo\nfrom foo;"
        );
    }

    #[test]
    fn autofix_3arg_convert_skipped_config_cast() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.casting_style".to_string(),
                serde_json::json!({"preferred_type_casting_style": "cast"}),
            )]),
        };
        let sql = "select\n    convert(int, 1, 126) as bar,\n    100::int::text,\n    cast(10 as text) as coo\nfrom foo;";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    convert(int, 1, 126) as bar,\n    cast(cast(100 as int) as text),\n    cast(10 as text) as coo\nfrom foo;"
        );
    }

    #[test]
    fn autofix_3arg_convert_skipped_config_shorthand() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.casting_style".to_string(),
                serde_json::json!({"preferred_type_casting_style": "shorthand"}),
            )]),
        };
        let sql = "select\n    convert(int, 1, 126) as bar,\n    100::int::text,\n    cast(10 as text) as coo\nfrom foo;";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    convert(int, 1, 126) as bar,\n    100::int::text,\n    10::text as coo\nfrom foo;"
        );
    }

    #[test]
    fn autofix_parenthesize_complex_expr_shorthand_from_cast() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.casting_style".to_string(),
                serde_json::json!({"preferred_type_casting_style": "shorthand"}),
            )]),
        };
        let sql = "select\n    id::int,\n    cast(calendar_date||' 11:00:00' as timestamp) as calendar_datetime\nfrom foo;";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    id::int,\n    (calendar_date||' 11:00:00')::timestamp as calendar_datetime\nfrom foo;"
        );
    }

    #[test]
    fn autofix_parenthesize_complex_expr_shorthand_from_convert() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.casting_style".to_string(),
                serde_json::json!({"preferred_type_casting_style": "shorthand"}),
            )]),
        };
        let sql = "select\n    id::int,\n    convert(timestamp, calendar_date||' 11:00:00') as calendar_datetime\nfrom foo;";
        let issues = run_with_config(sql, &config);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    id::int,\n    (calendar_date||' 11:00:00')::timestamp as calendar_datetime\nfrom foo;"
        );
    }

    #[test]
    fn autofix_comment_cast_skipped() {
        let sql = "select\n    cast(10 as text) as coo,\n    convert( -- Convert the value\n        int, /*\n              to an integer\n            */ 1) as bar,\n    100::int::text\nfrom foo;";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    cast(10 as text) as coo,\n    convert( -- Convert the value\n        int, /*\n              to an integer\n            */ 1) as bar,\n    cast(cast(100 as int) as text)\nfrom foo;"
        );
    }

    #[test]
    fn autofix_3arg_convert_consistent_prior_cast() {
        let sql = "select\n    cast(10 as text) as coo,\n    convert(int, 1, 126) as bar,\n    100::int::text\nfrom foo;";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    cast(10 as text) as coo,\n    convert(int, 1, 126) as bar,\n    cast(cast(100 as int) as text)\nfrom foo;"
        );
    }

    #[test]
    fn autofix_comment_prior_convert_shorthand_fixed() {
        let sql = "select\n    convert(int, 126) as bar,\n    cast(\n    1 /* cast the value\n        to an integer\n      */ as int) as coo,\n    100::int::text\nfrom foo;";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    convert(int, 126) as bar,\n    cast(\n    1 /* cast the value\n        to an integer\n      */ as int) as coo,\n    convert(text, convert(int, 100))\nfrom foo;"
        );
    }

    #[test]
    fn autofix_comment_prior_shorthand_convert_fixed() {
        let sql = "select\n    100::int::text,\n    convert(int, 126) as bar,\n    cast(\n    1 /* cast the value\n        to an integer\n      */ as int) as coo\nfrom foo;";
        let issues = run(sql);
        assert!(!issues.is_empty());
        let fixed = apply_all_fixes(sql, &issues);
        assert_eq!(
            fixed,
            "select\n    100::int::text,\n    126::int as bar,\n    cast(\n    1 /* cast the value\n        to an integer\n      */ as int) as coo\nfrom foo;"
        );
    }

    #[test]
    fn shorthand_to_cast_rewrites_nested_snowflake_path_casts() {
        let fixed = shorthand_to_cast("(trim(value:Longitude::varchar))::double").expect("rewrite");
        assert_eq!(
            fixed,
            "cast((trim(cast(value:Longitude as varchar))) as double)"
        );
        assert_eq!(
            shorthand_to_cast("col:a.b:c::varchar").expect("rewrite"),
            "cast(col:a.b:c as varchar)"
        );
    }
}
