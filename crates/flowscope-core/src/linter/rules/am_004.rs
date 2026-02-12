//! LINT_AM_004: Set operation column count mismatch.
//!
//! For set operations (e.g., UNION/INTERSECT/EXCEPT), each branch should expose
//! the same number of result columns.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

pub struct SetOperationColumnCount;

impl LintRule for SetOperationColumnCount {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_004
    }

    fn name(&self) -> &'static str {
        "Set operation column count"
    }

    fn description(&self) -> &'static str {
        "Set operation branches should return the same number of columns."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        check_statement(stmt, ctx, &mut issues);
        issues
    }
}

fn check_statement(stmt: &Statement, ctx: &LintContext, issues: &mut Vec<Issue>) {
    match stmt {
        Statement::Query(q) => {
            check_query(q, ctx, issues);
        }
        Statement::Insert(ins) => {
            if let Some(ref source) = ins.source {
                check_query(source, ctx, issues);
            }
        }
        Statement::CreateView { query, .. } => {
            check_query(query, ctx, issues);
        }
        Statement::CreateTable(create) => {
            if let Some(ref q) = create.query {
                check_query(q, ctx, issues);
            }
        }
        _ => {}
    }
}

fn check_query(query: &Query, ctx: &LintContext, issues: &mut Vec<Issue>) -> Option<usize> {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            check_query(&cte.query, ctx, issues);
        }
    }
    check_set_expr(&query.body, ctx, issues)
}

fn check_set_expr(body: &SetExpr, ctx: &LintContext, issues: &mut Vec<Issue>) -> Option<usize> {
    match body {
        SetExpr::Select(select) => {
            for from_item in &select.from {
                check_table_factor(&from_item.relation, ctx, issues);
                for join in &from_item.joins {
                    check_table_factor(&join.relation, ctx, issues);
                }
            }
            select_projection_count(select)
        }
        SetExpr::Query(q) => check_query(q, ctx, issues),
        SetExpr::SetOperation { left, right, .. } => {
            let left_count = check_set_expr(left, ctx, issues);
            let right_count = check_set_expr(right, ctx, issues);

            match (left_count, right_count) {
                (Some(l), Some(r)) if l != r => {
                    issues.push(
                        Issue::warning(
                            issue_codes::LINT_AM_004,
                            format!(
                                "Set operation has mismatched column counts: left has {}, right has {}.",
                                l, r
                            ),
                        )
                        .with_statement(ctx.statement_index),
                    );
                    None
                }
                (Some(l), Some(r)) => Some(l.min(r)),
                _ => None,
            }
        }
        SetExpr::Values(values) => values.rows.first().map(std::vec::Vec::len),
        _ => None,
    }
}

fn check_table_factor(relation: &TableFactor, ctx: &LintContext, issues: &mut Vec<Issue>) {
    match relation {
        TableFactor::Derived { subquery, .. } => {
            check_query(subquery, ctx, issues);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            check_table_factor(&table_with_joins.relation, ctx, issues);
            for join in &table_with_joins.joins {
                check_table_factor(&join.relation, ctx, issues);
            }
        }
        _ => {}
    }
}

fn select_projection_count(select: &Select) -> Option<usize> {
    let mut count = 0usize;
    for item in &select.projection {
        match item {
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => return None,
            _ => {
                count += 1;
            }
        }
    }
    Some(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = SetOperationColumnCount;
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
    fn test_mismatch_detected() {
        let issues = check_sql("SELECT a FROM t UNION SELECT a, b FROM t2");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_AM_004");
    }

    #[test]
    fn test_matching_counts_ok() {
        let issues = check_sql("SELECT a FROM t UNION SELECT b FROM t2");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_wildcard_skips_check() {
        let issues = check_sql("SELECT * FROM t UNION SELECT a, b FROM t2");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_nested_set_operation_in_cte_detected() {
        let issues =
            check_sql("WITH cte AS (SELECT a FROM t UNION SELECT a, b FROM t2) SELECT * FROM cte");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_AM_004");
    }
}
