//! LINT_AL_002: Unused table alias.
//!
//! A table is aliased in a FROM/JOIN clause but the alias is never referenced
//! anywhere in the query. This may indicate dead code or a copy-paste error.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;
use std::collections::{HashMap, HashSet};

pub struct UnusedTableAlias;

impl LintRule for UnusedTableAlias {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_002
    }

    fn name(&self) -> &'static str {
        "Unused table alias"
    }

    fn description(&self) -> &'static str {
        "Table alias defined but never referenced in the query."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        match stmt {
            Statement::Query(q) => check_query(q, ctx, &mut issues),
            Statement::Insert(ins) => {
                if let Some(ref source) = ins.source {
                    check_query(source, ctx, &mut issues);
                }
            }
            Statement::CreateView { query, .. } => check_query(query, ctx, &mut issues),
            Statement::CreateTable(create) => {
                if let Some(ref q) = create.query {
                    check_query(q, ctx, &mut issues);
                }
            }
            _ => {}
        }
        issues
    }
}

fn check_query(query: &Query, ctx: &LintContext, issues: &mut Vec<Issue>) {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            check_query(&cte.query, ctx, issues);
        }
    }
    match query.body.as_ref() {
        SetExpr::Select(select) => check_select(select, query.order_by.as_ref(), ctx, issues),
        _ => check_set_expr(&query.body, ctx, issues),
    }
}

fn check_set_expr(body: &SetExpr, ctx: &LintContext, issues: &mut Vec<Issue>) {
    match body {
        SetExpr::Select(select) => {
            check_select(select, None, ctx, issues);
        }
        SetExpr::Query(q) => check_query(q, ctx, issues),
        SetExpr::SetOperation { left, right, .. } => {
            check_set_expr(left, ctx, issues);
            check_set_expr(right, ctx, issues);
        }
        _ => {}
    }
}

fn check_select(
    select: &Select,
    order_by: Option<&OrderBy>,
    ctx: &LintContext,
    issues: &mut Vec<Issue>,
) {
    // Only flag when there are multiple tables (JOINs) — with a single table,
    // aliases are less important
    let table_count: usize = select.from.iter().map(|f| 1 + f.joins.len()).sum();
    if table_count < 2 {
        return;
    }

    // Collect aliases -> table names
    let mut aliases: HashMap<String, String> = HashMap::new();
    for from_item in &select.from {
        collect_aliases(&from_item.relation, &mut aliases);
        for join in &from_item.joins {
            collect_aliases(&join.relation, &mut aliases);
        }
    }

    if aliases.is_empty() {
        return;
    }

    let mut used_prefixes: HashSet<String> = HashSet::new();
    collect_identifier_prefixes_from_select(select, order_by, &mut used_prefixes);

    for alias in aliases.keys() {
        if !used_prefixes.contains(&alias.to_uppercase()) {
            issues.push(
                Issue::warning(
                    issue_codes::LINT_AL_002,
                    format!("Table alias '{alias}' is defined but never referenced."),
                )
                .with_statement(ctx.statement_index),
            );
        }
    }
}

fn collect_identifier_prefixes_from_order_by(order_by: &OrderBy, prefixes: &mut HashSet<String>) {
    if let OrderByKind::Expressions(order_by_exprs) = &order_by.kind {
        for order_expr in order_by_exprs {
            collect_identifier_prefixes(&order_expr.expr, prefixes);
        }
    }
}

fn collect_identifier_prefixes_from_query(query: &Query, prefixes: &mut HashSet<String>) {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            collect_identifier_prefixes_from_query(&cte.query, prefixes);
        }
    }

    match query.body.as_ref() {
        SetExpr::Select(select) => {
            collect_identifier_prefixes_from_select(select, query.order_by.as_ref(), prefixes);
        }
        SetExpr::Query(q) => collect_identifier_prefixes_from_query(q, prefixes),
        SetExpr::SetOperation { left, right, .. } => {
            collect_identifier_prefixes_from_set_expr(left, prefixes);
            collect_identifier_prefixes_from_set_expr(right, prefixes);
        }
        _ => {}
    }
}

fn collect_identifier_prefixes_from_set_expr(body: &SetExpr, prefixes: &mut HashSet<String>) {
    match body {
        SetExpr::Select(select) => collect_identifier_prefixes_from_select(select, None, prefixes),
        SetExpr::Query(q) => collect_identifier_prefixes_from_query(q, prefixes),
        SetExpr::SetOperation { left, right, .. } => {
            collect_identifier_prefixes_from_set_expr(left, prefixes);
            collect_identifier_prefixes_from_set_expr(right, prefixes);
        }
        _ => {}
    }
}

fn collect_identifier_prefixes_from_select(
    select: &Select,
    order_by: Option<&OrderBy>,
    prefixes: &mut HashSet<String>,
) {
    for item in &select.projection {
        collect_identifier_prefixes_from_select_item(item, prefixes);
    }
    if let Some(ref selection) = select.selection {
        collect_identifier_prefixes(selection, prefixes);
    }
    if let Some(ref having) = select.having {
        collect_identifier_prefixes(having, prefixes);
    }
    for from_item in &select.from {
        for join in &from_item.joins {
            if let Some(constraint) = join_constraint(&join.join_operator) {
                collect_identifier_prefixes(constraint, prefixes);
            }
        }
    }
    if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            collect_identifier_prefixes(expr, prefixes);
        }
    }
    if let Some(order_by) = order_by {
        collect_identifier_prefixes_from_order_by(order_by, prefixes);
    }
}

fn collect_aliases(relation: &TableFactor, aliases: &mut HashMap<String, String>) {
    if let TableFactor::Table {
        name,
        alias: Some(alias),
        ..
    } = relation
    {
        let table_name = name.to_string();
        let alias_name = alias.name.value.clone();
        // Only count as alias if it differs from the table name
        if alias_name.to_uppercase() != table_name.to_uppercase() {
            aliases.insert(alias_name, table_name);
        }
    }
}

fn collect_identifier_prefixes_from_select_item(item: &SelectItem, prefixes: &mut HashSet<String>) {
    match item {
        SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
            collect_identifier_prefixes(expr, prefixes);
        }
        SelectItem::QualifiedWildcard(name, _) => {
            let name_str = name.to_string();
            if let Some(prefix) = name_str.split('.').next() {
                prefixes.insert(prefix.to_uppercase());
            }
        }
        _ => {}
    }
}

fn collect_identifier_prefixes(expr: &Expr, prefixes: &mut HashSet<String>) {
    match expr {
        Expr::CompoundIdentifier(parts) => {
            if parts.len() >= 2 {
                prefixes.insert(parts[0].value.to_uppercase());
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_identifier_prefixes(left, prefixes);
            collect_identifier_prefixes(right, prefixes);
        }
        Expr::UnaryOp { expr: inner, .. } => collect_identifier_prefixes(inner, prefixes),
        Expr::Nested(inner) => collect_identifier_prefixes(inner, prefixes),
        Expr::Function(func) => {
            if let FunctionArguments::List(arg_list) = &func.args {
                for arg in &arg_list.args {
                    if let FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) = arg {
                        collect_identifier_prefixes(e, prefixes);
                    }
                }
            }
        }
        Expr::IsNull(inner) | Expr::IsNotNull(inner) | Expr::Cast { expr: inner, .. } => {
            collect_identifier_prefixes(inner, prefixes);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                collect_identifier_prefixes(op, prefixes);
            }
            for case_when in conditions {
                collect_identifier_prefixes(&case_when.condition, prefixes);
                collect_identifier_prefixes(&case_when.result, prefixes);
            }
            if let Some(el) = else_result {
                collect_identifier_prefixes(el, prefixes);
            }
        }
        Expr::InList { expr, list, .. } => {
            collect_identifier_prefixes(expr, prefixes);
            for item in list {
                collect_identifier_prefixes(item, prefixes);
            }
        }
        Expr::InSubquery { expr, subquery, .. } => {
            collect_identifier_prefixes(expr, prefixes);
            collect_identifier_prefixes_from_query(subquery, prefixes);
        }
        Expr::Subquery(subquery) | Expr::Exists { subquery, .. } => {
            collect_identifier_prefixes_from_query(subquery, prefixes);
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            collect_identifier_prefixes(expr, prefixes);
            collect_identifier_prefixes(low, prefixes);
            collect_identifier_prefixes(high, prefixes);
        }
        _ => {}
    }
}

fn join_constraint(op: &JoinOperator) -> Option<&Expr> {
    let constraint = match op {
        JoinOperator::Join(c)
        | JoinOperator::Inner(c)
        | JoinOperator::LeftOuter(c)
        | JoinOperator::RightOuter(c)
        | JoinOperator::FullOuter(c)
        | JoinOperator::LeftSemi(c)
        | JoinOperator::RightSemi(c)
        | JoinOperator::LeftAnti(c)
        | JoinOperator::RightAnti(c) => c,
        _ => return None,
    };
    match constraint {
        JoinConstraint::On(expr) => Some(expr),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = UnusedTableAlias;
        let ctx = LintContext {
            sql,
            statement_range: 0..sql.len(),
            statement_index: 0,
        };
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check(stmt, &ctx));
        }
        issues
    }

    #[test]
    fn test_unused_alias_detected() {
        let issues = check_sql("SELECT * FROM users u JOIN orders o ON users.id = orders.user_id");
        // Both aliases u and o are unused (full table names used instead)
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].code, "LINT_AL_002");
    }

    #[test]
    fn test_used_alias_ok() {
        let issues = check_sql("SELECT u.name FROM users u JOIN orders o ON u.id = o.user_id");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_single_table_no_check() {
        // With a single table, we don't flag unused aliases
        let issues = check_sql("SELECT * FROM users u");
        assert!(issues.is_empty());
    }

    // --- Edge cases adopted from sqlfluff aliasing rules ---

    #[test]
    fn test_alias_used_in_where() {
        let issues = check_sql(
            "SELECT u.name FROM users u JOIN orders o ON u.id = o.user_id WHERE u.active = true",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_in_group_by() {
        let issues = check_sql(
            "SELECT u.name, COUNT(*) FROM users u JOIN orders o ON u.id = o.user_id GROUP BY u.name",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_in_having() {
        let issues = check_sql(
            "SELECT u.name, COUNT(o.id) FROM users u JOIN orders o ON u.id = o.user_id \
             GROUP BY u.name HAVING COUNT(o.id) > 5",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_in_qualified_wildcard() {
        // u is used via u.*, o is used in JOIN ON condition
        let issues = check_sql("SELECT u.* FROM users u JOIN orders o ON u.id = o.user_id");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_unused_despite_qualified_wildcard() {
        // u is used via u.*, but o is never referenced (join uses full table name)
        let issues = check_sql("SELECT u.* FROM users u JOIN orders o ON u.id = orders.user_id");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("o"));
    }

    #[test]
    fn test_partial_alias_usage() {
        // Only one of two aliases is used
        let issues = check_sql("SELECT u.name FROM users u JOIN orders o ON u.id = orders.user_id");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("o"));
    }

    #[test]
    fn test_three_tables_one_unused() {
        let issues = check_sql(
            "SELECT a.name, b.total \
             FROM users a \
             JOIN orders b ON a.id = b.user_id \
             JOIN products c ON b.product_id = products.id",
        );
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("c"));
    }

    #[test]
    fn test_no_aliases_ok() {
        let issues =
            check_sql("SELECT users.name FROM users JOIN orders ON users.id = orders.user_id");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_self_join_with_aliases() {
        let issues =
            check_sql("SELECT a.name, b.name FROM users a JOIN users b ON a.manager_id = b.id");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_in_case_expression() {
        let issues = check_sql(
            "SELECT CASE WHEN u.active THEN 'yes' ELSE 'no' END \
             FROM users u JOIN orders o ON u.id = o.user_id",
        );
        // u is used in CASE, o is used in JOIN ON
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_in_order_by() {
        let issues = check_sql(
            "SELECT u.name \
             FROM users u \
             JOIN orders o ON users.id = orders.user_id \
             ORDER BY o.created_at",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_only_in_correlated_exists_subquery() {
        let issues = check_sql(
            "SELECT 1 \
             FROM users u \
             JOIN orders o ON 1 = 1 \
             WHERE EXISTS (SELECT 1 WHERE u.id = o.user_id)",
        );
        assert!(issues.is_empty());
    }
}
