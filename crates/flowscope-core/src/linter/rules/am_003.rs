//! LINT_AM_003: DISTINCT with GROUP BY.
//!
//! Using DISTINCT with GROUP BY is redundant because GROUP BY already
//! collapses duplicate rows. The DISTINCT can be safely removed.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

pub struct DistinctWithGroupBy;

impl LintRule for DistinctWithGroupBy {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_003
    }

    fn name(&self) -> &'static str {
        "DISTINCT with GROUP BY"
    }

    fn description(&self) -> &'static str {
        "DISTINCT is redundant when GROUP BY is used."
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
            let has_distinct = matches!(
                select.distinct,
                Some(Distinct::Distinct) | Some(Distinct::On(_))
            );
            let has_group_by = match &select.group_by {
                GroupByExpr::All(_) => true,
                GroupByExpr::Expressions(exprs, _) => !exprs.is_empty(),
            };

            if has_distinct && has_group_by {
                issues.push(
                    Issue::warning(
                        issue_codes::LINT_AM_003,
                        "DISTINCT is redundant when GROUP BY is present.",
                    )
                    .with_statement(ctx.statement_index),
                );
            }

            // Recurse into derived tables (subqueries in FROM)
            for from_item in &select.from {
                check_table_factor(&from_item.relation, ctx, issues);
                for join in &from_item.joins {
                    check_table_factor(&join.relation, ctx, issues);
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

fn check_table_factor(relation: &TableFactor, ctx: &LintContext, issues: &mut Vec<Issue>) {
    match relation {
        TableFactor::Derived { subquery, .. } => check_query(subquery, ctx, issues),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = DistinctWithGroupBy;
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
    fn test_distinct_with_group_by_detected() {
        let issues = check_sql("SELECT DISTINCT col FROM t GROUP BY col");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_AM_003");
    }

    #[test]
    fn test_distinct_without_group_by_ok() {
        let issues = check_sql("SELECT DISTINCT col FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_group_by_without_distinct_ok() {
        let issues = check_sql("SELECT col FROM t GROUP BY col");
        assert!(issues.is_empty());
    }

    // --- Edge cases adopted from sqlfluff AM01 (ambiguous.distinct) ---

    #[test]
    fn test_distinct_group_by_in_subquery() {
        let issues = check_sql("SELECT * FROM (SELECT DISTINCT a FROM t GROUP BY a) AS sub");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_distinct_group_by_in_cte() {
        let issues =
            check_sql("WITH cte AS (SELECT DISTINCT a FROM t GROUP BY a) SELECT * FROM cte");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_distinct_group_by_in_create_view() {
        let issues = check_sql("CREATE VIEW v AS SELECT DISTINCT a FROM t GROUP BY a");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_distinct_group_by_in_insert() {
        let issues = check_sql("INSERT INTO target SELECT DISTINCT a FROM t GROUP BY a");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_no_distinct_no_group_by_ok() {
        let issues = check_sql("SELECT a, b FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_distinct_group_by_in_union_branch() {
        let issues = check_sql("SELECT a FROM t UNION ALL SELECT DISTINCT b FROM t2 GROUP BY b");
        assert_eq!(issues.len(), 1);
    }
}
