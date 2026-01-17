//! AST context extraction for hybrid SQL completion.
//!
//! This module extracts completion-relevant context from parsed SQL ASTs,
//! including CTEs, table aliases, and subquery aliases with their columns.

use sqlparser::ast::{
    Cte, Expr, Query, Select, SelectItem, SetExpr, Statement, TableFactor, TableWithJoins,
};

use crate::types::{AstColumnInfo, AstContext, AstTableInfo, CteInfo, SubqueryInfo};

/// Maximum recursion depth for AST traversal to prevent stack overflow.
/// This is a defensive limit - realistic SQL rarely exceeds 10-20 levels of nesting.
const MAX_EXTRACTION_DEPTH: usize = 50;

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

/// Basic type inference from expression (very limited)
fn infer_data_type(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Value(v) => match &v.value {
            sqlparser::ast::Value::Number(_, _) => Some("number".to_string()),
            sqlparser::ast::Value::SingleQuotedString(_)
            | sqlparser::ast::Value::DoubleQuotedString(_) => Some("string".to_string()),
            sqlparser::ast::Value::Boolean(_) => Some("boolean".to_string()),
            sqlparser::ast::Value::Null => Some("null".to_string()),
            _ => None,
        },
        Expr::Cast { data_type, .. } => Some(data_type.to_string()),
        _ => None,
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
}
