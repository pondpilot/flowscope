//! LINT_AL_001: Implicit column alias.
//!
//! Computed expressions in SELECT without an explicit AS alias produce
//! implementation-dependent column names. Always give computed columns
//! an explicit alias for clarity and portability.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

pub struct ImplicitAlias;

impl LintRule for ImplicitAlias {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_001
    }

    fn name(&self) -> &'static str {
        "Implicit alias"
    }

    fn description(&self) -> &'static str {
        "Computed expressions should have explicit AS aliases for clarity."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        check_statement(stmt, ctx, &mut issues);
        issues
    }
}

fn check_statement(stmt: &Statement, ctx: &LintContext, issues: &mut Vec<Issue>) {
    match stmt {
        Statement::Query(q) => check_query(q, ctx, issues),
        Statement::Insert(ins) => {
            if let Some(ref source) = ins.source {
                check_query(source, ctx, issues);
            }
        }
        Statement::CreateView { query, .. } => check_query(query, ctx, issues),
        Statement::CreateTable(create) => {
            if let Some(ref q) = create.query {
                check_query(q, ctx, issues);
            }
        }
        _ => {}
    }
}

fn check_query(query: &Query, ctx: &LintContext, issues: &mut Vec<Issue>) {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            check_query(&cte.query, ctx, issues);
        }
    }
    check_set_expr(&query.body, ctx, issues);
}

fn check_set_expr(body: &SetExpr, ctx: &LintContext, issues: &mut Vec<Issue>) {
    match body {
        SetExpr::Select(select) => {
            for item in &select.projection {
                if let SelectItem::UnnamedExpr(expr) = item {
                    if is_computed(expr) {
                        let expr_str = format!("{expr}");
                        issues.push(
                            Issue::info(
                                issue_codes::LINT_AL_001,
                                format!(
                                    "Expression '{}' has no explicit alias. Add AS <name>.",
                                    truncate(&expr_str, 60)
                                ),
                            )
                            .with_statement(ctx.statement_index),
                        );
                    }
                }
            }
        }
        SetExpr::Query(q) => check_query(q, ctx, issues),
        SetExpr::SetOperation { left, right, .. } => {
            check_set_expr(left, ctx, issues);
            check_set_expr(right, ctx, issues);
        }
        _ => {}
    }
}

/// Returns true if the expression is "computed" (not a simple column reference or literal).
fn is_computed(expr: &Expr) -> bool {
    !matches!(
        expr,
        Expr::Identifier(_) | Expr::CompoundIdentifier(_) | Expr::Value(_)
    )
}

fn truncate(s: &str, max_len: usize) -> &str {
    match s.char_indices().nth(max_len) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = ImplicitAlias;
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
    fn test_implicit_alias_detected() {
        let issues = check_sql("SELECT a + b FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_AL_001");
    }

    #[test]
    fn test_explicit_alias_ok() {
        let issues = check_sql("SELECT a + b AS total FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_simple_column_ok() {
        let issues = check_sql("SELECT a, b FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_function_without_alias() {
        let issues = check_sql("SELECT COUNT(*) FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_function_with_alias_ok() {
        let issues = check_sql("SELECT COUNT(*) AS cnt FROM t");
        assert!(issues.is_empty());
    }

    // --- Edge cases adopted from sqlfluff AL03 (aliasing.expression) ---

    #[test]
    fn test_cast_without_alias() {
        let issues = check_sql("SELECT CAST(x AS INT) FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_cast_with_alias_ok() {
        let issues = check_sql("SELECT CAST(x AS INT) AS x_int FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_star_ok() {
        let issues = check_sql("SELECT * FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_qualified_star_ok() {
        let issues = check_sql("SELECT t.* FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_literal_ok() {
        let issues = check_sql("SELECT 1 FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_string_literal_ok() {
        let issues = check_sql("SELECT 'hello' FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_upper_function_without_alias() {
        let issues = check_sql("SELECT UPPER(name) FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_upper_function_with_alias_ok() {
        let issues = check_sql("SELECT UPPER(name) AS upper_name FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_arithmetic_without_alias() {
        let issues = check_sql("SELECT price * quantity FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_multiple_expressions_mixed() {
        // One has alias, one doesn't
        let issues = check_sql("SELECT a + b AS total, c * d FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_case_expression_without_alias() {
        let issues = check_sql("SELECT CASE WHEN x > 0 THEN 'yes' ELSE 'no' END FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_case_expression_with_alias_ok() {
        let issues = check_sql("SELECT CASE WHEN x > 0 THEN 'yes' ELSE 'no' END AS flag FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_expression_in_cte() {
        let issues = check_sql("WITH cte AS (SELECT a + b FROM t) SELECT * FROM cte");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_qualified_column_ok() {
        let issues = check_sql("SELECT t.a, t.b FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_non_ascii_expression_truncation_is_utf8_safe() {
        let sql = format!("SELECT \"{}é\" + 1 FROM t", "a".repeat(58));
        let issues = check_sql(&sql);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_AL_001");
    }
}
