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
        let stmt_sql = ctx.statement_sql();
        let mut union_search_start = 0;
        match stmt {
            Statement::Query(query) => {
                check_query(query, stmt_sql, &mut union_search_start, ctx, &mut issues)
            }
            Statement::Insert(insert) => {
                if let Some(ref source) = insert.source {
                    check_query(source, stmt_sql, &mut union_search_start, ctx, &mut issues);
                }
            }
            Statement::CreateView { query, .. } => {
                check_query(query, stmt_sql, &mut union_search_start, ctx, &mut issues)
            }
            Statement::CreateTable(create) => {
                if let Some(ref query) = create.query {
                    check_query(query, stmt_sql, &mut union_search_start, ctx, &mut issues);
                }
            }
            _ => {}
        }
        issues
    }
}

fn check_query(
    query: &Query,
    stmt_sql: &str,
    union_search_start: &mut usize,
    ctx: &LintContext,
    issues: &mut Vec<Issue>,
) {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            check_query(&cte.query, stmt_sql, union_search_start, ctx, issues);
        }
    }
    check_query_body(&query.body, stmt_sql, union_search_start, ctx, issues);
}

fn check_query_body(
    body: &SetExpr,
    stmt_sql: &str,
    union_search_start: &mut usize,
    ctx: &LintContext,
    issues: &mut Vec<Issue>,
) {
    match body {
        SetExpr::SetOperation {
            op: SetOperator::Union,
            set_quantifier,
            left,
            right,
        } => {
            check_query_body(left, stmt_sql, union_search_start, ctx, issues);
            let union_span = find_next_union_keyword(stmt_sql, ctx, *union_search_start);
            if let Some((_, next_search_start)) = union_span {
                *union_search_start = next_search_start;
            }

            if matches!(set_quantifier, SetQuantifier::None | SetQuantifier::ByName) {
                let mut issue = Issue::warning(
                    issue_codes::LINT_AM_001,
                    "Use UNION ALL instead of UNION to avoid implicit deduplication.",
                )
                .with_statement(ctx.statement_index);
                if let Some((s, _)) = union_span {
                    issue = issue.with_span(s);
                }
                issues.push(issue);
            }
            check_query_body(right, stmt_sql, union_search_start, ctx, issues);
        }
        SetExpr::SetOperation { left, right, .. } => {
            check_query_body(left, stmt_sql, union_search_start, ctx, issues);
            check_query_body(right, stmt_sql, union_search_start, ctx, issues);
        }
        SetExpr::Select(_) => {}
        SetExpr::Query(q) => {
            check_query(q, stmt_sql, union_search_start, ctx, issues);
        }
        _ => {}
    }
}

/// Finds the next `UNION` keyword byte span after `search_start`.
fn find_next_union_keyword(
    stmt_sql: &str,
    ctx: &LintContext,
    search_start: usize,
) -> Option<(crate::types::Span, usize)> {
    let upper = stmt_sql.to_ascii_uppercase();
    let mut search_pos = search_start;
    while let Some(pos) = upper[search_pos..].find("UNION") {
        let abs_pos = search_pos + pos;
        let after = abs_pos + 5;
        // Check word boundaries
        let before_ok = abs_pos == 0 || !is_sql_identifier_char(stmt_sql.as_bytes()[abs_pos - 1]);
        let after_ok =
            after >= stmt_sql.len() || !is_sql_identifier_char(stmt_sql.as_bytes()[after]);
        if before_ok && after_ok {
            return Some((ctx.span_from_statement_offset(abs_pos, after), after));
        }
        search_pos = after;
    }
    None
}

fn is_sql_identifier_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
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
    fn test_multiple_bare_unions_have_distinct_spans() {
        let issues = check_sql("SELECT 1 UNION SELECT 2 UNION SELECT 3");
        assert_eq!(issues.len(), 2);
        let first_span = issues[0].span.expect("first UNION should have span");
        let second_span = issues[1].span.expect("second UNION should have span");
        assert!(first_span.start < second_span.start);
    }

    #[test]
    fn test_except_and_intersect_ok() {
        let issues = check_sql("SELECT 1 EXCEPT SELECT 2");
        assert!(issues.is_empty());
        let issues = check_sql("SELECT 1 INTERSECT SELECT 2");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_union_identifier_with_underscore_does_not_steal_span() {
        let sql = "SELECT union_col FROM t UNION SELECT 2";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        let span = issues[0].span.expect("UNION issue should include a span");
        let union_pos = sql.find("UNION").expect("query should contain UNION");
        assert_eq!(span.start, union_pos);
    }
}
