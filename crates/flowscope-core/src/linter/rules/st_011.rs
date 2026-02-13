//! LINT_ST_011: Unused joined source.
//!
//! Joined relations should be referenced outside of their own JOIN clause.

use std::collections::HashSet;

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{JoinOperator, Select, SelectItem, Statement};

use super::semantic_helpers::{
    collect_qualifier_prefixes_in_expr, count_reference_qualification_in_expr_excluding_aliases,
    join_on_expr, select_projection_alias_set, table_factor_reference_name,
    visit_selects_in_statement,
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
            violations += unused_join_count_for_select(select);
        });

        (0..violations)
            .map(|_| {
                Issue::warning(issue_codes::LINT_ST_011, "Joined source appears unused.")
                    .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn unused_join_count_for_select(select: &Select) -> usize {
    if select.from.is_empty() || select.from.len() > 1 {
        return 0;
    }

    let mut joined_sources = joined_sources(select);
    if joined_sources.len() <= 1 {
        return 0;
    }

    let referenced_in_join_clauses = referenced_tables_in_join_clauses(select);
    joined_sources.retain(|source| !referenced_in_join_clauses.contains(source));
    if joined_sources.is_empty() {
        return 0;
    }

    // SQLFluff ST11 parity: unqualified wildcard projection references
    // all available sources in the select scope.
    if select_has_unqualified_wildcard(select) {
        return 0;
    }

    let aliases = select_projection_alias_set(select);
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
        return 0;
    }

    collect_projection_wildcard_prefixes(select, &mut used_prefixes);

    joined_sources
        .iter()
        .filter(|source| !used_prefixes.contains(*source))
        .count()
}

fn select_has_unqualified_wildcard(select: &Select) -> bool {
    select
        .projection
        .iter()
        .any(|item| matches!(item, SelectItem::Wildcard(_)))
}

fn joined_sources(select: &Select) -> HashSet<String> {
    let mut joined_sources = HashSet::new();

    for table in &select.from {
        if let Some(name) = table_factor_reference_name(&table.relation) {
            joined_sources.insert(name);
        }

        for join in &table.joins {
            if !is_tracked_join(&join.join_operator) {
                continue;
            }

            if let Some(name) = table_factor_reference_name(&join.relation) {
                joined_sources.insert(name);
            }
        }
    }

    joined_sources
}

fn referenced_tables_in_join_clauses(select: &Select) -> HashSet<String> {
    let mut referenced = HashSet::new();

    for table in &select.from {
        for join in &table.joins {
            let self_ref = table_factor_reference_name(&join.relation);

            if let Some(on_expr) = join_on_expr(&join.join_operator) {
                let mut refs = HashSet::new();
                collect_qualifier_prefixes_in_expr(on_expr, &mut refs);
                for table_ref in refs {
                    if self_ref.as_deref() != Some(table_ref.as_str()) {
                        referenced.insert(table_ref);
                    }
                }
            }
        }
    }

    referenced
}

fn is_tracked_join(operator: &JoinOperator) -> bool {
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

fn collect_projection_wildcard_prefixes(select: &Select, prefixes: &mut HashSet<String>) {
    for item in &select.projection {
        if let SelectItem::QualifiedWildcard(object_name, _) = item {
            let name = object_name.to_string();
            if let Some(last) = name.rsplit('.').next() {
                prefixes.insert(
                    last.trim_matches(|ch| matches!(ch, '"' | '`' | '\'' | '[' | ']'))
                        .to_ascii_uppercase(),
                );
            }
        }
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
    fn allows_unqualified_wildcard_projection() {
        let issues = run("select * from a left join b on a.id = b.id");
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

    #[test]
    fn allows_outer_join_table_referenced_by_another_join_condition() {
        let issues =
            run("SELECT a.id FROM a LEFT JOIN b ON a.id = b.a_id LEFT JOIN c ON b.c_id = c.id");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_011);
    }
}
