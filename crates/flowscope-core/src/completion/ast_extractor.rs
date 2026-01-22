//! AST context extraction for hybrid SQL completion.
//!
//! This module extracts completion-relevant context from parsed SQL ASTs,
//! including CTEs, table aliases, and subquery aliases with their columns.

use sqlparser::ast::{
    Cte, Expr, Query, Select, SelectItem, SetExpr, Spanned, Statement, TableFactor, TableWithJoins,
};

use crate::analyzer::helpers::{infer_expr_type, line_col_to_offset};
use crate::types::{AstColumnInfo, AstContext, AstTableInfo, CteInfo, SubqueryInfo};

/// Information about a SELECT alias for lateral reference support.
///
/// Lateral column aliases allow referencing aliases defined earlier in the same
/// SELECT list. This is supported by dialects like DuckDB, BigQuery, and Snowflake.
///
/// # Scope Tracking
///
/// Each alias tracks the byte offset range of its containing SELECT's projection.
/// This is used to ensure aliases from CTEs or subqueries don't leak into outer
/// SELECT scopes.
#[derive(Debug, Clone)]
pub struct LateralAliasInfo {
    /// The alias name
    pub name: String,
    /// Byte offset where the alias definition ends
    pub definition_end: usize,
    /// Byte offset where the containing SELECT's projection starts (after SELECT keyword)
    pub projection_start: usize,
    /// Byte offset where the containing SELECT's projection ends (before FROM/WHERE/etc.)
    pub projection_end: usize,
}

/// Maximum recursion depth for AST traversal to prevent stack overflow.
/// This is a defensive limit - realistic SQL rarely exceeds 10-20 levels of nesting.
const MAX_EXTRACTION_DEPTH: usize = 50;

/// Maximum number of lateral aliases to extract.
/// This prevents memory exhaustion from malicious input with thousands of aliases.
/// 1000 aliases is far beyond any realistic SQL query.
const MAX_LATERAL_ALIASES: usize = 1000;

/// Extract AST context from parsed statements for completion enrichment.
///
/// Extracts:
/// - CTE definitions and their columns
/// - Table aliases and their resolved names
/// - Subquery aliases and their projected columns
pub(crate) fn extract_ast_context(statements: &[Statement]) -> AstContext {
    let mut ctx = AstContext::default();

    for stmt in statements {
        extract_from_statement(stmt, &mut ctx, 0);
    }

    ctx
}

/// Extract context from a single statement
fn extract_from_statement(stmt: &Statement, ctx: &mut AstContext, depth: usize) {
    if depth > MAX_EXTRACTION_DEPTH {
        return; // Silent truncation is acceptable for completion
    }

    match stmt {
        Statement::Query(query) => {
            extract_from_query(query, ctx, depth);
        }
        Statement::Insert(insert) => {
            // Extract from INSERT ... SELECT
            if let Some(source) = &insert.source {
                extract_from_query(source, ctx, depth);
            }
        }
        Statement::CreateTable(ct) => {
            // Extract from CREATE TABLE ... AS SELECT
            if let Some(query) = &ct.query {
                extract_from_query(query, ctx, depth);
            }
        }
        Statement::CreateView { query, .. } => {
            extract_from_query(query, ctx, depth);
        }
        _ => {}
    }
}

/// Extract context from a Query (SELECT, UNION, etc.)
fn extract_from_query(query: &Query, ctx: &mut AstContext, depth: usize) {
    if depth > MAX_EXTRACTION_DEPTH {
        return;
    }

    // Extract CTEs first (they're in scope for the body)
    if let Some(with) = &query.with {
        let is_recursive = with.recursive;
        for cte in &with.cte_tables {
            if let Some(info) = extract_cte_info(cte, is_recursive) {
                ctx.cte_definitions.insert(info.name.clone(), info);
            }
        }
    }

    // Extract from the query body
    extract_from_set_expr(&query.body, ctx, depth + 1);
}

/// Extract context from a SetExpr (SELECT, UNION, etc.)
fn extract_from_set_expr(set_expr: &SetExpr, ctx: &mut AstContext, depth: usize) {
    if depth > MAX_EXTRACTION_DEPTH {
        return;
    }

    match set_expr {
        SetExpr::Select(select) => {
            extract_from_select(select, ctx, depth);
        }
        SetExpr::Query(query) => {
            extract_from_query(query, ctx, depth);
        }
        SetExpr::SetOperation { left, right, .. } => {
            extract_from_set_expr(left, ctx, depth + 1);
            extract_from_set_expr(right, ctx, depth + 1);
        }
        SetExpr::Values(_) => {}
        SetExpr::Insert(_) => {}
        SetExpr::Update(_) => {}
        SetExpr::Table(_) => {}
        SetExpr::Delete(_) => {}
        SetExpr::Merge(_) => {}
    }
}

/// Extract context from a SELECT statement
fn extract_from_select(select: &Select, ctx: &mut AstContext, depth: usize) {
    if depth > MAX_EXTRACTION_DEPTH {
        return;
    }

    // Extract from FROM clause
    for table_with_joins in &select.from {
        extract_from_table_with_joins(table_with_joins, ctx, depth);
    }
}

/// Extract context from a table with joins
fn extract_from_table_with_joins(twj: &TableWithJoins, ctx: &mut AstContext, depth: usize) {
    if depth > MAX_EXTRACTION_DEPTH {
        return;
    }

    extract_from_table_factor(&twj.relation, ctx, depth);

    for join in &twj.joins {
        extract_from_table_factor(&join.relation, ctx, depth);
    }
}

/// Extract context from a table factor (table reference)
fn extract_from_table_factor(tf: &TableFactor, ctx: &mut AstContext, depth: usize) {
    if depth > MAX_EXTRACTION_DEPTH {
        return;
    }

    match tf {
        TableFactor::Table { name, alias, .. } => {
            let table_name = name.to_string();
            let alias_name = alias.as_ref().map(|a| a.name.value.clone());

            // Use alias if present, otherwise use table name
            let key = alias_name.clone().unwrap_or_else(|| {
                // Use just the table name part (last component)
                name.0
                    .last()
                    .map(|i| i.to_string())
                    .unwrap_or(table_name.clone())
            });

            ctx.table_aliases.insert(key, AstTableInfo);
        }
        TableFactor::Derived {
            subquery, alias, ..
        } => {
            // Extract subquery info
            if let Some(alias) = alias {
                let columns = extract_projected_columns_from_query(subquery);
                ctx.subquery_aliases.insert(
                    alias.name.value.clone(),
                    SubqueryInfo {
                        projected_columns: columns,
                    },
                );
            }

            // Recurse into subquery
            extract_from_query(subquery, ctx, depth + 1);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            extract_from_table_with_joins(table_with_joins, ctx, depth + 1);
        }
        TableFactor::TableFunction { .. } => {}
        TableFactor::UNNEST {
            alias: Some(alias), ..
        } => {
            ctx.table_aliases
                .insert(alias.name.value.clone(), AstTableInfo);
        }
        _ => {}
    }
}

/// Extract CTE info from a CTE definition
fn extract_cte_info(cte: &Cte, is_recursive: bool) -> Option<CteInfo> {
    let name = cte.alias.name.value.clone();

    // Get declared columns from alias
    let declared_columns: Vec<String> = cte
        .alias
        .columns
        .iter()
        .map(|c| c.name.value.clone())
        .collect();

    // Get projected columns from CTE body
    let projected_columns = if is_recursive {
        // For recursive CTEs, only use the base case (first SELECT in UNION)
        extract_base_case_columns(&cte.query)
    } else {
        extract_projected_columns_from_query(&cte.query)
    };

    Some(CteInfo {
        name,
        declared_columns,
        projected_columns,
    })
}

/// Extract columns from the base case of a recursive CTE
fn extract_base_case_columns(query: &Query) -> Vec<AstColumnInfo> {
    match &*query.body {
        SetExpr::SetOperation { left, .. } => {
            // In UNION, left is typically the base case
            if let SetExpr::Select(select) = &**left {
                extract_select_columns(select)
            } else {
                vec![]
            }
        }
        SetExpr::Select(select) => extract_select_columns(select),
        _ => vec![],
    }
}

/// Extract projected columns from a query
fn extract_projected_columns_from_query(query: &Query) -> Vec<AstColumnInfo> {
    match &*query.body {
        SetExpr::Select(select) => extract_select_columns(select),
        SetExpr::SetOperation { left, .. } => {
            // Use left side's columns for UNION
            if let SetExpr::Select(select) = &**left {
                extract_select_columns(select)
            } else {
                vec![]
            }
        }
        _ => vec![],
    }
}

/// Extract columns from a SELECT's projection
fn extract_select_columns(select: &Select) -> Vec<AstColumnInfo> {
    let mut columns = Vec::new();

    for (idx, item) in select.projection.iter().enumerate() {
        match item {
            SelectItem::ExprWithAlias { alias, expr } => {
                columns.push(AstColumnInfo {
                    name: alias.value.clone(),
                    data_type: infer_data_type(expr),
                });
            }
            SelectItem::UnnamedExpr(expr) => {
                columns.push(AstColumnInfo {
                    name: derive_column_name(expr, idx),
                    data_type: infer_data_type(expr),
                });
            }
            SelectItem::Wildcard(_) => {
                columns.push(AstColumnInfo {
                    name: "*".to_string(),
                    data_type: None,
                });
            }
            SelectItem::QualifiedWildcard(name, _) => {
                columns.push(AstColumnInfo {
                    name: format!("{}.*", name),
                    data_type: None,
                });
            }
        }
    }

    columns
}

/// Derive column name from expression
fn derive_column_name(expr: &Expr, index: usize) -> String {
    match expr {
        Expr::Identifier(ident) => ident.value.clone(),
        Expr::CompoundIdentifier(parts) => parts
            .last()
            .map(|i| i.value.clone())
            .unwrap_or_else(|| format!("col_{}", index)),
        Expr::Function(func) => func.name.to_string().to_lowercase(),
        Expr::Cast { .. } => format!("col_{}", index),
        Expr::Case { .. } => format!("case_{}", index),
        Expr::Subquery(_) => format!("subquery_{}", index),
        _ => format!("col_{}", index),
    }
}

/// Infer data type from expression using the analyzer's centralized type inference.
///
/// Returns the canonical type name in uppercase (e.g., "TEXT", "INTEGER", "FLOAT").
/// See [`crate::analyzer::helpers::infer_expr_type`] for supported expressions and behavior.
fn infer_data_type(expr: &Expr) -> Option<String> {
    infer_expr_type(expr).map(|canonical| canonical.as_uppercase_str().to_string())
}

/// Extracts SELECT aliases with their positions from parsed statements.
///
/// Returns aliases that appear in SELECT projections, along with the byte offset
/// where each alias definition ends. This is used for lateral alias completion
/// in dialects that support referencing earlier aliases in the same SELECT list.
///
/// # Arguments
///
/// * `statements` - Parsed SQL statements
/// * `sql` - Original SQL source text (needed for line:column to byte offset conversion)
///
/// # Returns
///
/// A vector of lateral alias info, ordered by position in the source.
/// Limited to `MAX_LATERAL_ALIASES` to prevent memory exhaustion.
pub(crate) fn extract_lateral_aliases(
    statements: &[Statement],
    sql: &str,
) -> Vec<LateralAliasInfo> {
    let mut aliases = Vec::with_capacity(64); // Reasonable starting capacity

    for stmt in statements {
        // Stop extraction if we've hit the limit
        if aliases.len() >= MAX_LATERAL_ALIASES {
            break;
        }

        if let Statement::Query(query) = stmt {
            // Extract from top-level CTEs first
            if let Some(with) = &query.with {
                for cte in &with.cte_tables {
                    if aliases.len() >= MAX_LATERAL_ALIASES {
                        break;
                    }
                    extract_lateral_aliases_from_set_expr(&cte.query.body, sql, &mut aliases, 0);
                }
            }
            // Then extract from the main query body
            if aliases.len() < MAX_LATERAL_ALIASES {
                extract_lateral_aliases_from_set_expr(&query.body, sql, &mut aliases, 0);
            }
        }
    }

    aliases
}

/// Extract lateral aliases from a SetExpr (handles SELECT, nested Query, and set operations).
///
/// Note: This intentionally extracts from all SELECT projections, including CTEs.
/// The filtering by cursor position happens in the consumer (context.rs) using
/// the projection_start/projection_end fields.
fn extract_lateral_aliases_from_set_expr(
    set_expr: &SetExpr,
    sql: &str,
    aliases: &mut Vec<LateralAliasInfo>,
    depth: usize,
) {
    if depth > MAX_EXTRACTION_DEPTH || aliases.len() >= MAX_LATERAL_ALIASES {
        return;
    }

    match set_expr {
        SetExpr::Select(select) => {
            extract_lateral_aliases_from_select(select, sql, aliases);
        }
        SetExpr::Query(query) => {
            // Handle CTEs within nested queries
            if let Some(with) = &query.with {
                for cte in &with.cte_tables {
                    if aliases.len() >= MAX_LATERAL_ALIASES {
                        break;
                    }
                    extract_lateral_aliases_from_set_expr(&cte.query.body, sql, aliases, depth + 1);
                }
            }
            if aliases.len() < MAX_LATERAL_ALIASES {
                extract_lateral_aliases_from_set_expr(&query.body, sql, aliases, depth + 1);
            }
        }
        SetExpr::SetOperation { left, right, .. } => {
            extract_lateral_aliases_from_set_expr(left, sql, aliases, depth + 1);
            if aliases.len() < MAX_LATERAL_ALIASES {
                extract_lateral_aliases_from_set_expr(right, sql, aliases, depth + 1);
            }
        }
        _ => {}
    }
}

/// Extract lateral aliases from a SELECT's projection.
///
/// Iterates over the projection items and records aliases with their positions,
/// including the span of the containing SELECT projection for scope filtering.
///
/// # Safety Notes
///
/// The function validates that computed byte offsets are:
/// 1. Within bounds of the SQL string
/// 2. At valid UTF-8 character boundaries
///
/// Aliases with invalid offsets (e.g., from parser bugs or multi-byte char issues)
/// are silently skipped to prevent panics.
fn extract_lateral_aliases_from_select(
    select: &Select,
    sql: &str,
    aliases: &mut Vec<LateralAliasInfo>,
) {
    // Early return if we've hit the extraction limit
    if aliases.len() >= MAX_LATERAL_ALIASES {
        return;
    }

    // Compute projection span from the first and last projection items
    // This is used to filter aliases to only those in the same SELECT scope as cursor
    let projection_span = compute_projection_span(select, sql);
    let (projection_start, projection_end) = match projection_span {
        Some((start, end)) => (start, end),
        None => return, // Can't determine span, skip this SELECT
    };

    for item in &select.projection {
        // Check limit on each iteration to stop early
        if aliases.len() >= MAX_LATERAL_ALIASES {
            break;
        }

        if let SelectItem::ExprWithAlias { alias, .. } = item {
            // Convert line:column to byte offset
            // sqlparser uses 1-indexed line/column numbers
            if let Some(end_offset) = line_col_to_offset(
                sql,
                alias.span.end.line as usize,
                alias.span.end.column as usize,
            ) {
                // Validate offset is within bounds and at a valid UTF-8 boundary
                // This prevents panics on multi-byte characters (emoji, unicode identifiers)
                if end_offset <= sql.len() && sql.is_char_boundary(end_offset) {
                    aliases.push(LateralAliasInfo {
                        name: alias.value.clone(),
                        definition_end: end_offset,
                        projection_start,
                        projection_end,
                    });
                }
            }
        }
    }
}

/// Compute the byte offset span of a SELECT's projection area.
///
/// The projection area starts at the first projection item and extends to:
/// - The FROM clause (if present)
/// - Or the end of the SELECT (if no FROM)
///
/// This allows cursor positions after the last alias (like `SELECT a AS x, |`)
/// to still be considered "within" the projection area for lateral alias purposes.
///
/// Returns (start, end) offsets, or None if the span cannot be determined.
fn compute_projection_span(select: &Select, sql: &str) -> Option<(usize, usize)> {
    if select.projection.is_empty() {
        return None;
    }

    // Find the first projection item that has a usable span. This skips leading
    // wildcards (plain `*`), which deliberately return None in select_item_span.
    // When every item lacks a span (e.g., `SELECT *`), fall back to the SELECT
    // keyword span so we still have a safe projection boundary.
    let first_span = select
        .projection
        .iter()
        .filter_map(select_item_span)
        .next()
        .or_else(|| {
            let span = select.span();
            if span.start.line > 0 && span.start.column > 0 {
                Some((span.start.line, span.start.column))
            } else {
                None
            }
        })?;
    let start = line_col_to_offset(sql, first_span.0 as usize, first_span.1 as usize)?;

    // Determine the end of the projection area
    // Prefer FROM clause position if available, otherwise use last item
    let end = if let Some(from_item) = select.from.first() {
        // Use the start of the FROM clause as the end of projection area
        compute_from_clause_start(from_item, sql).unwrap_or_else(|| {
            // Fallback to last projection item
            select
                .projection
                .last()
                .and_then(|item| {
                    let span = select_item_end_span(item)?;
                    line_col_to_offset(sql, span.0 as usize, span.1 as usize)
                })
                .unwrap_or(sql.len())
        })
    } else {
        // No FROM clause - use a large value to include trailing positions
        // This handles cases like `SELECT a AS x, |` (no FROM yet)
        sql.len()
    };

    // Validate spans
    if start <= sql.len() && end <= sql.len() && start <= end {
        Some((start, end))
    } else {
        None
    }
}

/// Get the byte offset where the FROM clause starts.
fn compute_from_clause_start(from_item: &TableWithJoins, sql: &str) -> Option<usize> {
    // Get the span of the first table in FROM
    let span = table_factor_span(&from_item.relation)?;
    let table_start = line_col_to_offset(sql, span.0 as usize, span.1 as usize)?;

    // Search backwards from table_start to find "FROM" keyword
    // This gives us the actual start of the FROM clause
    //
    // Safety: We need to find a valid UTF-8 char boundary before slicing.
    // saturating_sub(50) might land in the middle of a multi-byte character.
    let search_start = find_char_boundary_before(sql, table_start.saturating_sub(50));
    let search_area = &sql[search_start..table_start];

    // Find "FROM" (case insensitive) using ASCII-only comparison.
    // We avoid to_uppercase() because it can change string length for certain
    // Unicode characters (e.g., German ß -> SS), causing position misalignment.
    if let Some(pos) = rfind_ascii_case_insensitive(search_area, b"FROM") {
        Some(search_start + pos)
    } else {
        // Fallback to table start if FROM not found
        Some(table_start)
    }
}

/// Find a valid UTF-8 char boundary at or before the given position.
/// Returns 0 if no valid boundary is found before the position.
fn find_char_boundary_before(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    // Walk backwards from pos to find a valid char boundary
    (0..=pos)
        .rev()
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(0)
}

/// Case-insensitive reverse search for an ASCII pattern in a string slice.
/// Returns the byte offset of the last occurrence, or None if not found.
///
/// This is safe for UTF-8 strings because ASCII bytes (0x00-0x7F) never appear
/// as continuation bytes in multi-byte UTF-8 sequences.
fn rfind_ascii_case_insensitive(haystack: &str, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }

    let haystack_bytes = haystack.as_bytes();

    // Search from the end backwards
    for start in (0..=(haystack_bytes.len() - needle.len())).rev() {
        let mut matches = true;
        for (i, &needle_byte) in needle.iter().enumerate() {
            let hay_byte = haystack_bytes[start + i];
            // ASCII case-insensitive comparison
            if !hay_byte.eq_ignore_ascii_case(&needle_byte) {
                matches = false;
                break;
            }
        }
        if matches {
            return Some(start);
        }
    }
    None
}

/// Get the start position (line, column) of a TableFactor.
fn table_factor_span(tf: &TableFactor) -> Option<(u64, u64)> {
    match tf {
        TableFactor::Table { name, .. } => name.0.first().map(|i| {
            let span = i.span();
            (span.start.line, span.start.column)
        }),
        TableFactor::Derived { subquery, .. } => {
            // For derived tables, try to get the span of the subquery
            let span = subquery.body.span();
            if span.start.line > 0 {
                Some((span.start.line, span.start.column))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Get the start position (line, column) of a SelectItem.
fn select_item_span(item: &SelectItem) -> Option<(u64, u64)> {
    match item {
        SelectItem::ExprWithAlias { expr, .. } | SelectItem::UnnamedExpr(expr) => {
            expr_start_span(expr)
        }
        SelectItem::Wildcard(opts) => {
            // Wildcard span comes from the options if available
            if let Some(exclude) = &opts.opt_exclude {
                // Use first exclusion's span if available
                match exclude {
                    sqlparser::ast::ExcludeSelectItem::Single(ident) => {
                        Some((ident.span.start.line, ident.span.start.column))
                    }
                    sqlparser::ast::ExcludeSelectItem::Multiple(idents) => idents
                        .first()
                        .map(|i| (i.span.start.line, i.span.start.column)),
                }
            } else {
                None
            }
        }
        SelectItem::QualifiedWildcard(name, _) => {
            let span = name.span();
            Some((span.start.line, span.start.column))
        }
    }
}

/// Get the end position (line, column) of a SelectItem.
fn select_item_end_span(item: &SelectItem) -> Option<(u64, u64)> {
    match item {
        SelectItem::ExprWithAlias { alias, .. } => {
            Some((alias.span.end.line, alias.span.end.column))
        }
        SelectItem::UnnamedExpr(expr) => expr_end_span(expr),
        SelectItem::Wildcard(_) => None, // Wildcard doesn't have reliable end span
        SelectItem::QualifiedWildcard(name, _) => {
            let span = name.span();
            Some((span.end.line, span.end.column))
        }
    }
}

/// Get the start position (line, column) of an expression.
/// Uses the Spanned trait for comprehensive coverage of all expression types.
fn expr_start_span(expr: &Expr) -> Option<(u64, u64)> {
    let span = expr.span();
    // Check for valid span (non-zero positions indicate valid span)
    if span.start.line > 0 && span.start.column > 0 {
        Some((span.start.line, span.start.column))
    } else {
        None
    }
}

/// Get the end position (line, column) of an expression.
/// Uses the Spanned trait for comprehensive coverage of all expression types.
fn expr_end_span(expr: &Expr) -> Option<(u64, u64)> {
    let span = expr.span();
    // Check for valid span (non-zero positions indicate valid span)
    if span.end.line > 0 && span.end.column > 0 {
        Some((span.end.line, span.end.column))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::parser::Parser;

    fn parse_sql(sql: &str) -> Vec<Statement> {
        Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, sql).unwrap()
    }

    #[test]
    fn test_extract_cte() {
        let sql = "WITH cte AS (SELECT id, name FROM users) SELECT * FROM cte";
        let stmts = parse_sql(sql);
        let ctx = extract_ast_context(&stmts);

        assert!(ctx.cte_definitions.contains_key("cte"));
        let cte = &ctx.cte_definitions["cte"];
        assert_eq!(cte.name, "cte");
        assert_eq!(cte.projected_columns.len(), 2);
        assert_eq!(cte.projected_columns[0].name, "id");
        assert_eq!(cte.projected_columns[1].name, "name");
    }

    #[test]
    fn test_extract_cte_with_declared_columns() {
        let sql = "WITH cte(a, b) AS (SELECT id, name FROM users) SELECT * FROM cte";
        let stmts = parse_sql(sql);
        let ctx = extract_ast_context(&stmts);

        let cte = &ctx.cte_definitions["cte"];
        assert_eq!(cte.declared_columns, vec!["a", "b"]);
    }

    #[test]
    fn test_extract_table_alias() {
        let sql = "SELECT * FROM users u JOIN orders o ON u.id = o.user_id";
        let stmts = parse_sql(sql);
        let ctx = extract_ast_context(&stmts);

        // Aliases are stored as keys in the table_aliases map
        assert!(ctx.table_aliases.contains_key("u"));
        assert!(ctx.table_aliases.contains_key("o"));
    }

    #[test]
    fn test_extract_subquery_alias() {
        let sql = "SELECT * FROM (SELECT a, b FROM t) AS sub WHERE sub.a = 1";
        let stmts = parse_sql(sql);
        let ctx = extract_ast_context(&stmts);

        assert!(ctx.subquery_aliases.contains_key("sub"));
        let sub = &ctx.subquery_aliases["sub"];
        assert_eq!(sub.projected_columns.len(), 2);
        assert_eq!(sub.projected_columns[0].name, "a");
        assert_eq!(sub.projected_columns[1].name, "b");
    }

    #[test]
    fn test_extract_lateral_subquery() {
        let sql = "SELECT * FROM users u, LATERAL (SELECT * FROM orders WHERE user_id = u.id) AS o";
        let stmts = parse_sql(sql);
        let ctx = extract_ast_context(&stmts);

        // Lateral subqueries are extracted just like regular derived tables
        assert!(ctx.subquery_aliases.contains_key("o"));
    }

    #[test]
    fn test_extract_column_with_alias() {
        let sql =
            "WITH cte AS (SELECT id AS user_id, name AS user_name FROM users) SELECT * FROM cte";
        let stmts = parse_sql(sql);
        let ctx = extract_ast_context(&stmts);

        let cte = &ctx.cte_definitions["cte"];
        assert_eq!(cte.projected_columns[0].name, "user_id");
        assert_eq!(cte.projected_columns[1].name, "user_name");
    }

    #[test]
    fn test_extract_function_column_name() {
        let sql = "WITH cte AS (SELECT COUNT(*), SUM(amount) FROM orders) SELECT * FROM cte";
        let stmts = parse_sql(sql);
        let ctx = extract_ast_context(&stmts);

        let cte = &ctx.cte_definitions["cte"];
        assert!(cte.projected_columns[0]
            .name
            .to_lowercase()
            .contains("count"));
    }

    #[test]
    fn test_extract_wildcard() {
        let sql = "WITH cte AS (SELECT * FROM users) SELECT * FROM cte";
        let stmts = parse_sql(sql);
        let ctx = extract_ast_context(&stmts);

        let cte = &ctx.cte_definitions["cte"];
        assert_eq!(cte.projected_columns[0].name, "*");
    }

    #[test]
    fn test_extract_recursive_cte() {
        let sql = r#"
            WITH RECURSIVE cte AS (
                SELECT 1 AS n
                UNION ALL
                SELECT n + 1 FROM cte WHERE n < 10
            )
            SELECT * FROM cte
        "#;
        let stmts = parse_sql(sql);
        let ctx = extract_ast_context(&stmts);

        let cte = &ctx.cte_definitions["cte"];
        // Should have column from base case
        assert_eq!(cte.projected_columns.len(), 1);
        assert_eq!(cte.projected_columns[0].name, "n");
    }

    #[test]
    fn test_has_enrichment() {
        let sql = "SELECT * FROM users";
        let stmts = parse_sql(sql);
        let ctx = extract_ast_context(&stmts);

        assert!(ctx.has_enrichment()); // Has table alias
    }

    #[test]
    fn test_empty_context() {
        let ctx = AstContext::default();
        assert!(!ctx.has_enrichment());
    }

    // Lateral alias extraction tests

    #[test]
    fn test_extract_lateral_aliases_single() {
        let sql = "SELECT price * qty AS total FROM orders";
        let stmts = parse_sql(sql);
        let aliases = extract_lateral_aliases(&stmts, sql);

        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].name, "total");
        // The alias ends after "total" (position should be after the alias)
        assert!(aliases[0].definition_end > 0);
        assert!(aliases[0].definition_end <= sql.len());
    }

    #[test]
    fn test_extract_lateral_aliases_with_leading_wildcard() {
        let sql = "SELECT *, price * qty AS total, discount AS disc FROM orders";
        let stmts = parse_sql(sql);
        let aliases = extract_lateral_aliases(&stmts, sql);

        let names: Vec<_> = aliases.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["total", "disc"]);
    }

    #[test]
    fn test_extract_lateral_aliases_multiple() {
        let sql = "SELECT a AS x, b AS y, c AS z FROM t";
        let stmts = parse_sql(sql);
        let aliases = extract_lateral_aliases(&stmts, sql);

        assert_eq!(aliases.len(), 3);
        assert_eq!(aliases[0].name, "x");
        assert_eq!(aliases[1].name, "y");
        assert_eq!(aliases[2].name, "z");
        // Aliases should be ordered by position
        assert!(aliases[0].definition_end < aliases[1].definition_end);
        assert!(aliases[1].definition_end < aliases[2].definition_end);
    }

    #[test]
    fn test_extract_lateral_aliases_with_expression() {
        let sql = "SELECT price * qty AS total, total * 0.1 AS tax FROM orders";
        let stmts = parse_sql(sql);
        let aliases = extract_lateral_aliases(&stmts, sql);

        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases[0].name, "total");
        assert_eq!(aliases[1].name, "tax");
    }

    #[test]
    fn test_extract_lateral_aliases_no_aliases() {
        let sql = "SELECT price, qty FROM orders";
        let stmts = parse_sql(sql);
        let aliases = extract_lateral_aliases(&stmts, sql);

        assert!(aliases.is_empty());
    }

    #[test]
    fn test_extract_lateral_aliases_mixed() {
        // Mix of aliased and non-aliased columns
        let sql = "SELECT a, b AS alias_b, c FROM t";
        let stmts = parse_sql(sql);
        let aliases = extract_lateral_aliases(&stmts, sql);

        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].name, "alias_b");
    }

    #[test]
    fn test_extract_lateral_aliases_quoted() {
        let sql = r#"SELECT a AS "My Total", b AS "Tax Amount" FROM t"#;
        let stmts = parse_sql(sql);
        let aliases = extract_lateral_aliases(&stmts, sql);

        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases[0].name, "My Total");
        assert_eq!(aliases[1].name, "Tax Amount");
    }

    #[test]
    fn test_extract_lateral_aliases_subquery_in_from() {
        // Aliases in subqueries in FROM clause should NOT be extracted,
        // because lateral aliases are only visible within the same SELECT list
        let sql = "SELECT * FROM (SELECT a AS x, b AS y FROM t) sub";
        let stmts = parse_sql(sql);
        let aliases = extract_lateral_aliases(&stmts, sql);

        // The outer SELECT has SELECT * which has no aliases
        assert_eq!(aliases.len(), 0);
    }

    #[test]
    fn test_extract_lateral_aliases_outer_select_with_alias() {
        // If the outer SELECT has aliases, those should be extracted
        let sql = "SELECT sub.x AS outer_x FROM (SELECT a AS x FROM t) sub";
        let stmts = parse_sql(sql);
        let aliases = extract_lateral_aliases(&stmts, sql);

        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].name, "outer_x");
    }

    #[test]
    fn test_extract_lateral_aliases_with_unicode() {
        // Unicode characters in SQL should not cause panics
        // The parser may report byte offsets that don't align with char boundaries
        let sql = "SELECT '日本語' AS label, value AS val FROM t";
        let stmts = parse_sql(sql);
        let aliases = extract_lateral_aliases(&stmts, sql);

        // Should successfully extract aliases even with unicode in the SQL
        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases[0].name, "label");
        assert_eq!(aliases[1].name, "val");
    }

    #[test]
    fn test_extract_lateral_aliases_cte_scope_isolation() {
        // Aliases from CTE's SELECT should have different projection spans
        // than aliases from the outer SELECT
        let sql =
            "WITH cte AS (SELECT a AS inner_alias FROM t) SELECT cte.a AS outer_alias FROM cte";
        let stmts = parse_sql(sql);
        let aliases = extract_lateral_aliases(&stmts, sql);

        // Should extract both aliases, but they should have different projection spans
        assert_eq!(aliases.len(), 2);

        let inner = aliases.iter().find(|a| a.name == "inner_alias").unwrap();
        let outer = aliases.iter().find(|a| a.name == "outer_alias").unwrap();

        // Inner alias projection should be before outer alias projection
        assert!(
            inner.projection_start < outer.projection_start,
            "CTE projection should start before outer SELECT projection"
        );

        // Projection spans should not overlap significantly
        assert!(
            inner.projection_end < outer.projection_start
                || outer.projection_end < inner.projection_start
                || inner.projection_start != outer.projection_start,
            "CTE and outer SELECT projections should have different spans"
        );
    }

    #[test]
    fn test_extract_lateral_aliases_projection_span_validity() {
        // Verify that projection spans are valid and contain the alias
        let sql = "SELECT a AS x, b AS y FROM t";
        let stmts = parse_sql(sql);
        let aliases = extract_lateral_aliases(&stmts, sql);

        assert_eq!(aliases.len(), 2);

        for alias in &aliases {
            // Projection span should contain the alias definition
            assert!(
                alias.definition_end <= alias.projection_end,
                "Alias definition should be within projection span"
            );
            assert!(
                alias.projection_start < alias.definition_end,
                "Projection should start before alias definition ends"
            );
        }
    }
}
