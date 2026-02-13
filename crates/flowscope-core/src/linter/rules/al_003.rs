//! LINT_AL_003: Implicit column alias.
//!
//! Computed expressions in SELECT without an explicit AS alias produce
//! implementation-dependent column names. Always give computed columns
//! an explicit alias for clarity and portability.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

pub struct ImplicitAlias {
    allow_scalar: bool,
}

impl ImplicitAlias {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            allow_scalar: config
                .rule_option_bool(issue_codes::LINT_AL_003, "allow_scalar")
                .unwrap_or(true),
        }
    }
}

impl Default for ImplicitAlias {
    fn default() -> Self {
        Self { allow_scalar: true }
    }
}

impl LintRule for ImplicitAlias {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_003
    }

    fn name(&self) -> &'static str {
        "Implicit alias"
    }

    fn description(&self) -> &'static str {
        "Computed expressions should have explicit AS aliases for clarity."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        check_statement(stmt, ctx, self.allow_scalar, &mut issues);
        issues
    }
}

fn check_statement(
    stmt: &Statement,
    ctx: &LintContext,
    allow_scalar: bool,
    issues: &mut Vec<Issue>,
) {
    match stmt {
        Statement::Query(q) => check_query(q, ctx, allow_scalar, issues),
        Statement::Insert(ins) => {
            if let Some(ref source) = ins.source {
                check_query(source, ctx, allow_scalar, issues);
            }
        }
        Statement::CreateView { query, .. } => check_query(query, ctx, allow_scalar, issues),
        Statement::CreateTable(create) => {
            if let Some(ref q) = create.query {
                check_query(q, ctx, allow_scalar, issues);
            }
        }
        _ => {}
    }
}

fn check_query(query: &Query, ctx: &LintContext, allow_scalar: bool, issues: &mut Vec<Issue>) {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            check_query(&cte.query, ctx, allow_scalar, issues);
        }
    }
    check_set_expr(&query.body, ctx, allow_scalar, issues, false);
}

fn check_set_expr(
    body: &SetExpr,
    ctx: &LintContext,
    allow_scalar: bool,
    issues: &mut Vec<Issue>,
    in_set_rhs: bool,
) {
    match body {
        SetExpr::Select(select) => {
            // In set-operation RHS branches, output column names come from the left side.
            // Requiring aliases here creates noisy false positives on common UNION patterns.
            if in_set_rhs {
                return;
            }

            for item in &select.projection {
                if let SelectItem::UnnamedExpr(expr) = item {
                    if is_computed(expr) || (!allow_scalar && is_scalar_literal(expr)) {
                        let expr_str = format!("{expr}");
                        issues.push(
                            Issue::info(
                                issue_codes::LINT_AL_003,
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
        SetExpr::Query(q) => check_query(q, ctx, allow_scalar, issues),
        SetExpr::SetOperation { left, right, .. } => {
            check_set_expr(left, ctx, allow_scalar, issues, false);
            check_set_expr(right, ctx, allow_scalar, issues, true);
        }
        SetExpr::Insert(stmt)
        | SetExpr::Update(stmt)
        | SetExpr::Delete(stmt)
        | SetExpr::Merge(stmt) => check_statement(stmt, ctx, allow_scalar, issues),
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

fn is_scalar_literal(expr: &Expr) -> bool {
    matches!(expr, Expr::Value(_))
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

    fn check_sql_with_rule(sql: &str, rule: ImplicitAlias) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
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

    fn check_sql(sql: &str) -> Vec<Issue> {
        check_sql_with_rule(sql, ImplicitAlias::default())
    }

    #[test]
    fn test_implicit_alias_detected() {
        let issues = check_sql("SELECT a + b FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_AL_003");
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
    fn test_union_rhs_expression_without_alias_ok() {
        let issues = check_sql("SELECT a + b AS total FROM t UNION ALL SELECT 0::INT FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_with_insert_select_expression_without_alias_detected() {
        let sql = "WITH params AS (SELECT 1) INSERT INTO t(a) SELECT COALESCE(x, 0) FROM src";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_AL_003");
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
        assert_eq!(issues[0].code, "LINT_AL_003");
    }

    #[test]
    fn test_allow_scalar_false_flags_literals() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.expression".to_string(),
                serde_json::json!({"allow_scalar": false}),
            )]),
        };
        let issues = check_sql_with_rule("SELECT 1 FROM t", ImplicitAlias::from_config(&config));
        assert_eq!(issues.len(), 1);
    }
}
