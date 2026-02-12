//! LINT_ST_011: Unused joined source.
//!
//! Outer-joined relations should be referenced outside of their own JOIN clause.

use std::collections::HashSet;

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{JoinOperator, Select, Statement};

use super::semantic_helpers::{
    collect_qualifier_prefixes_in_expr, count_reference_qualification_in_expr_excluding_aliases,
    table_factor_reference_name, visit_selects_in_statement,
};

pub struct StructureUnusedJoin;

impl LintRule for StructureUnusedJoin {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_011
    }

    fn name(&self) -> &'static str {
        "Structure unused join"
    }

    fn description(&self) -> &'static str {
        "Joined sources should be referenced meaningfully."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violations = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            let joined_sources = outer_join_sources(select);
            if joined_sources.is_empty() {
                return;
            }

            let aliases = super::semantic_helpers::select_projection_alias_set(select);
            let mut used_prefixes = HashSet::new();
            let mut unqualified_references = 0usize;

            visit_non_join_select_expressions(select, &mut |expr| {
                collect_qualifier_prefixes_in_expr(expr, &mut used_prefixes);
                let (_, unqualified) =
                    count_reference_qualification_in_expr_excluding_aliases(expr, &aliases);
                unqualified_references += unqualified;
            });

            // Match SQLFluff ST11 behavior: if unqualified references exist,
            // this rule defers until reference qualification issues are resolved.
            if unqualified_references > 0 {
                return;
            }

            if joined_sources
                .iter()
                .any(|source| !used_prefixes.contains(source))
            {
                violations += 1;
            }
        });

        (0..violations)
            .map(|_| {
                Issue::warning(issue_codes::LINT_ST_011, "Joined source appears unused.")
                    .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn outer_join_sources(select: &Select) -> HashSet<String> {
    let mut joined_sources = HashSet::new();

    for table in &select.from {
        for join in &table.joins {
            if !is_outer_join(&join.join_operator) {
                continue;
            }

            if let Some(name) = table_factor_reference_name(&join.relation) {
                joined_sources.insert(name);
            }
        }
    }

    joined_sources
}

fn is_outer_join(operator: &JoinOperator) -> bool {
    matches!(
        operator,
        JoinOperator::Left(_)
            | JoinOperator::LeftOuter(_)
            | JoinOperator::Right(_)
            | JoinOperator::RightOuter(_)
            | JoinOperator::FullOuter(_)
    )
}

fn visit_non_join_select_expressions<F: FnMut(&sqlparser::ast::Expr)>(
    select: &Select,
    visitor: &mut F,
) {
    for item in &select.projection {
        if let sqlparser::ast::SelectItem::UnnamedExpr(expr)
        | sqlparser::ast::SelectItem::ExprWithAlias { expr, .. } = item
        {
            visitor(expr);
        }
    }

    if let Some(prewhere) = &select.prewhere {
        visitor(prewhere);
    }

    if let Some(selection) = &select.selection {
        visitor(selection);
    }

    if let sqlparser::ast::GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            visitor(expr);
        }
    }

    if let Some(having) = &select.having {
        visitor(having);
    }

    if let Some(qualify) = &select.qualify {
        visitor(qualify);
    }

    for sort in &select.sort_by {
        visitor(&sort.expr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = StructureUnusedJoin;
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check(
                    statement,
                    &LintContext {
                        sql,
                        statement_range: 0..sql.len(),
                        statement_index: index,
                    },
                )
            })
            .collect()
    }

    // --- Edge cases adopted from sqlfluff ST11 ---

    #[test]
    fn flags_unused_outer_joined_source() {
        let issues = run("select 1 from b left join c on b.x = c.x");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_011);
    }

    #[test]
    fn allows_single_table_statement() {
        let issues = run("select 1 from foo");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_inner_join_when_joined_source_unreferenced() {
        let issues = run("select a.* from a inner join b using(x)");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_implicit_inner_join_when_joined_source_unreferenced() {
        let issues = run("select a.* from a join b using(x)");
        assert!(issues.is_empty());
    }

    #[test]
    fn defers_when_unqualified_references_exist() {
        let issues = run("select a from b left join c using(d)");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_outer_join_when_joined_source_is_referenced() {
        let issues = run("select widget.id, inventor.id from widget left join inventor on widget.inventor_id = inventor.id");
        assert!(issues.is_empty());
    }

    #[test]
    fn detects_unused_join_in_subquery_scope() {
        let issues = run(
            "SELECT a.col1 FROM a LEFT JOIN b ON a.id = b.a_id WHERE a.some_column IN (SELECT c.some_column FROM c WHERE c.other_column = a.col)",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_join_reference_inside_subquery() {
        let issues = run(
            "SELECT a.col1 FROM a LEFT JOIN b ON a.id = b.a_id WHERE a.some_column IN (SELECT c.some_column FROM c WHERE c.other_column = b.col)",
        );
        assert!(issues.is_empty());
    }
}
