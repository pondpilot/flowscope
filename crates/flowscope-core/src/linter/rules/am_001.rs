//! LINT_AM_001: Bare UNION (without ALL).
//!
//! `UNION` without `ALL` triggers implicit deduplication which is often unintended
//! and has significant performance cost. Use `UNION ALL` when duplicates are acceptable.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

pub struct BareUnion;

impl LintRule for BareUnion {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_001
    }

    fn name(&self) -> &'static str {
        "Bare UNION"
    }

    fn description(&self) -> &'static str {
        "UNION without ALL triggers implicit deduplication. Use UNION ALL if duplicates are acceptable."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        match stmt {
            Statement::Query(query) => check_query(query, ctx, &mut issues),
            Statement::Insert(insert) => {
                if let Some(ref source) = insert.source {
                    check_query(source, ctx, &mut issues);
                }
            }
            Statement::CreateView { query, .. } => check_query(query, ctx, &mut issues),
            Statement::CreateTable(create) => {
                if let Some(ref query) = create.query {
                    check_query(query, ctx, &mut issues);
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
    check_query_body(&query.body, ctx, issues);
}

fn check_query_body(body: &SetExpr, ctx: &LintContext, issues: &mut Vec<Issue>) {
    match body {
        SetExpr::SetOperation {
            op: SetOperator::Union,
            set_quantifier,
            left,
            right,
        } => {
            if matches!(set_quantifier, SetQuantifier::None | SetQuantifier::ByName) {
                let stmt_sql = ctx.statement_sql();
                let span = find_union_keyword(stmt_sql, ctx);
                let mut issue = Issue::warning(
                    issue_codes::LINT_AM_001,
                    "Use UNION ALL instead of UNION to avoid implicit deduplication.",
                )
                .with_statement(ctx.statement_index);
                if let Some(s) = span {
                    issue = issue.with_span(s);
                }
                issues.push(issue);
            }
            check_query_body(left, ctx, issues);
            check_query_body(right, ctx, issues);
        }
        SetExpr::SetOperation { left, right, .. } => {
            check_query_body(left, ctx, issues);
            check_query_body(right, ctx, issues);
        }
        SetExpr::Select(_) => {}
        SetExpr::Query(q) => {
            check_query_body(&q.body, ctx, issues);
        }
        _ => {}
    }
}

/// Finds the byte span of the `UNION` keyword (not followed by `ALL`) in statement SQL.
fn find_union_keyword(stmt_sql: &str, ctx: &LintContext) -> Option<crate::types::Span> {
    let upper = stmt_sql.to_ascii_uppercase();
    let mut search_pos = 0;
    while let Some(pos) = upper[search_pos..].find("UNION") {
        let abs_pos = search_pos + pos;
        let after = abs_pos + 5;
        // Check word boundaries
        let before_ok = abs_pos == 0 || !stmt_sql.as_bytes()[abs_pos - 1].is_ascii_alphanumeric();
        let after_ok =
            after >= stmt_sql.len() || !stmt_sql.as_bytes()[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            // Check it's not followed by ALL
            let rest = upper[after..].trim_start();
            if !rest.starts_with("ALL") {
                return Some(ctx.span_from_statement_offset(abs_pos, after));
            }
        }
        search_pos = after;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = BareUnion;
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
    fn test_bare_union_detected() {
        let issues = check_sql("SELECT 1 UNION SELECT 2");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_AM_001");
    }

    #[test]
    fn test_union_all_ok() {
        let issues = check_sql("SELECT 1 UNION ALL SELECT 2");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_multiple_bare_unions() {
        let issues = check_sql("SELECT 1 UNION SELECT 2 UNION SELECT 3");
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_mixed_union() {
        let issues = check_sql("SELECT 1 UNION ALL SELECT 2 UNION SELECT 3");
        assert_eq!(issues.len(), 1);
    }

    // --- Edge cases adopted from sqlfluff AM02 ---

    #[test]
    fn test_union_distinct_ok() {
        let issues = check_sql("SELECT a, b FROM t1 UNION DISTINCT SELECT a, b FROM t2");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_bare_union_in_insert() {
        let issues = check_sql("INSERT INTO target SELECT 1 UNION SELECT 2");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_bare_union_in_create_view() {
        let issues = check_sql("CREATE VIEW v AS SELECT 1 UNION SELECT 2");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_bare_union_in_cte() {
        let issues = check_sql("WITH cte AS (SELECT 1 UNION SELECT 2) SELECT * FROM cte");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_union_all_in_cte_ok() {
        let issues = check_sql("WITH cte AS (SELECT 1 UNION ALL SELECT 2) SELECT * FROM cte");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_triple_bare_union() {
        let issues = check_sql("SELECT 1 UNION SELECT 2 UNION SELECT 3");
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_except_and_intersect_ok() {
        let issues = check_sql("SELECT 1 EXCEPT SELECT 2");
        assert!(issues.is_empty());
        let issues = check_sql("SELECT 1 INTERSECT SELECT 2");
        assert!(issues.is_empty());
    }
}
