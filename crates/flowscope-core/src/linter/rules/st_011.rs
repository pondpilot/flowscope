//! LINT_ST_011: Unused joined source.
//!
//! Joined relations should be referenced outside of their own JOIN clause.

use std::collections::HashSet;

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{
    Expr, FunctionArg, FunctionArgExpr, JoinOperator, NamedWindowExpr, OrderByKind, Query, Select,
    SelectItem, SetExpr, Statement, TableFactor,
};

use super::semantic_helpers::{
    collect_qualifier_prefixes_in_expr, count_reference_qualification_in_expr_excluding_aliases,
    join_on_expr, select_projection_alias_set, table_factor_reference_name,
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
        let violations = unused_join_count_for_statement(statement);

        (0..violations)
            .map(|_| {
                Issue::warning(issue_codes::LINT_ST_011, "Joined source appears unused.")
                    .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn unused_join_count_for_statement(statement: &Statement) -> usize {
    match statement {
        Statement::Query(query) => unused_join_count_for_query(query),
        Statement::Insert(insert) => insert
            .source
            .as_ref()
            .map_or(0, |query| unused_join_count_for_query(query)),
        Statement::CreateView { query, .. } => unused_join_count_for_query(query),
        Statement::CreateTable(create) => create
            .query
            .as_ref()
            .map_or(0, |query| unused_join_count_for_query(query)),
        _ => 0,
    }
}

fn unused_join_count_for_query(query: &Query) -> usize {
    let mut total = 0usize;

    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            total += unused_join_count_for_query(&cte.query);
        }
    }

    let query_order_by_exprs = query_order_by_exprs(query);
    total + unused_join_count_for_set_expr(&query.body, &query_order_by_exprs)
}

fn unused_join_count_for_set_expr(set_expr: &SetExpr, query_order_by_exprs: &[&Expr]) -> usize {
    match set_expr {
        SetExpr::Select(select) => {
            let mut total = unused_join_count_for_select(select, query_order_by_exprs);

            for table in &select.from {
                total += unused_join_count_for_table_factor(&table.relation);
                for join in &table.joins {
                    total += unused_join_count_for_table_factor(&join.relation);
                }
            }

            visit_non_join_select_expressions(select, &mut |expr| {
                total += unused_join_count_for_expr_subqueries(expr);
            });
            visit_named_window_expressions(select, &mut |expr| {
                total += unused_join_count_for_expr_subqueries(expr);
            });

            total
        }
        SetExpr::Query(query) => unused_join_count_for_query(query),
        SetExpr::SetOperation { left, right, .. } => {
            unused_join_count_for_set_expr(left, &[]) + unused_join_count_for_set_expr(right, &[])
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => unused_join_count_for_statement(statement),
        _ => 0,
    }
}

fn query_order_by_exprs(query: &Query) -> Vec<&Expr> {
    let Some(order_by) = &query.order_by else {
        return Vec::new();
    };

    match &order_by.kind {
        OrderByKind::Expressions(order_exprs) => {
            order_exprs.iter().map(|item| &item.expr).collect()
        }
        _ => Vec::new(),
    }
}

fn unused_join_count_for_select(select: &Select, query_order_by_exprs: &[&Expr]) -> usize {
    if select.from.is_empty() {
        return 0;
    }

    let mut joined_sources = joined_sources(select);
    if joined_sources.is_empty() {
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
    visit_named_window_expressions(select, &mut |expr| {
        collect_qualifier_prefixes_in_expr(expr, &mut used_prefixes);
        let (_, unqualified) =
            count_reference_qualification_in_expr_excluding_aliases(expr, &aliases);
        unqualified_references += unqualified;
    });

    for expr in query_order_by_exprs {
        collect_qualifier_prefixes_in_expr(expr, &mut used_prefixes);
        let (_, unqualified) =
            count_reference_qualification_in_expr_excluding_aliases(expr, &aliases);
        unqualified_references += unqualified;
    }

    // Match SQLFluff ST11 behavior: if unqualified references exist,
    // this rule defers until reference qualification issues are resolved.
    if unqualified_references > 0 {
        return 0;
    }

    collect_projection_wildcard_prefixes(select, &mut used_prefixes);
    collect_join_relation_reference_prefixes(select, &mut used_prefixes);

    joined_sources
        .iter()
        .filter(|source| !used_prefixes.contains(*source))
        .count()
}

fn unused_join_count_for_table_factor(table_factor: &TableFactor) -> usize {
    match table_factor {
        TableFactor::Derived { subquery, .. } => unused_join_count_for_query(subquery),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            let mut total = unused_join_count_for_table_factor(&table_with_joins.relation);
            for join in &table_with_joins.joins {
                total += unused_join_count_for_table_factor(&join.relation);
                if let Some(on_expr) = join_on_expr(&join.join_operator) {
                    total += unused_join_count_for_expr_subqueries(on_expr);
                }
            }
            total
        }
        TableFactor::Pivot {
            table,
            aggregate_functions,
            value_column,
            default_on_null,
            ..
        } => {
            let mut total = unused_join_count_for_table_factor(table);
            for expr_with_alias in aggregate_functions {
                total += unused_join_count_for_expr_subqueries(&expr_with_alias.expr);
            }
            for expr in value_column {
                total += unused_join_count_for_expr_subqueries(expr);
            }
            if let Some(expr) = default_on_null {
                total += unused_join_count_for_expr_subqueries(expr);
            }
            total
        }
        TableFactor::Unpivot {
            table,
            value,
            columns,
            ..
        } => {
            let mut total = unused_join_count_for_table_factor(table);
            total += unused_join_count_for_expr_subqueries(value);
            for expr_with_alias in columns {
                total += unused_join_count_for_expr_subqueries(&expr_with_alias.expr);
            }
            total
        }
        TableFactor::MatchRecognize {
            table,
            partition_by,
            order_by,
            measures,
            ..
        } => {
            let mut total = unused_join_count_for_table_factor(table);
            for expr in partition_by {
                total += unused_join_count_for_expr_subqueries(expr);
            }
            for order in order_by {
                total += unused_join_count_for_expr_subqueries(&order.expr);
            }
            for measure in measures {
                total += unused_join_count_for_expr_subqueries(&measure.expr);
            }
            total
        }
        TableFactor::TableFunction { expr, .. } => unused_join_count_for_expr_subqueries(expr),
        TableFactor::Function { args, .. } => args
            .iter()
            .map(unused_join_count_for_function_arg)
            .sum::<usize>(),
        TableFactor::UNNEST { array_exprs, .. } => array_exprs
            .iter()
            .map(unused_join_count_for_expr_subqueries)
            .sum::<usize>(),
        TableFactor::JsonTable { json_expr, .. } | TableFactor::OpenJsonTable { json_expr, .. } => {
            unused_join_count_for_expr_subqueries(json_expr)
        }
        TableFactor::XmlTable { row_expression, .. } => {
            unused_join_count_for_expr_subqueries(row_expression)
        }
        TableFactor::Table { .. } | TableFactor::SemanticView { .. } => 0,
    }
}

fn unused_join_count_for_function_arg(arg: &FunctionArg) -> usize {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
        | FunctionArg::Named {
            arg: FunctionArgExpr::Expr(expr),
            ..
        } => unused_join_count_for_expr_subqueries(expr),
        _ => 0,
    }
}

fn unused_join_count_for_expr_subqueries(expr: &Expr) -> usize {
    match expr {
        Expr::Subquery(query)
        | Expr::Exists {
            subquery: query, ..
        } => unused_join_count_for_query(query),
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => unused_join_count_for_expr_subqueries(inner) + unused_join_count_for_query(subquery),
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. } => {
            unused_join_count_for_expr_subqueries(left)
                + unused_join_count_for_expr_subqueries(right)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => unused_join_count_for_expr_subqueries(inner),
        Expr::InList { expr, list, .. } => {
            unused_join_count_for_expr_subqueries(expr)
                + list
                    .iter()
                    .map(unused_join_count_for_expr_subqueries)
                    .sum::<usize>()
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            unused_join_count_for_expr_subqueries(expr)
                + unused_join_count_for_expr_subqueries(low)
                + unused_join_count_for_expr_subqueries(high)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let operand_count = operand
                .as_ref()
                .map_or(0, |expr| unused_join_count_for_expr_subqueries(expr));
            let condition_count = conditions
                .iter()
                .map(|when| {
                    unused_join_count_for_expr_subqueries(&when.condition)
                        + unused_join_count_for_expr_subqueries(&when.result)
                })
                .sum::<usize>();
            let else_count = else_result
                .as_ref()
                .map_or(0, |expr| unused_join_count_for_expr_subqueries(expr));
            operand_count + condition_count + else_count
        }
        Expr::Function(function) => {
            let args_count =
                if let sqlparser::ast::FunctionArguments::List(arguments) = &function.args {
                    arguments
                        .args
                        .iter()
                        .map(unused_join_count_for_function_arg)
                        .sum::<usize>()
                } else {
                    0
                };
            let filter_count = function
                .filter
                .as_ref()
                .map_or(0, |expr| unused_join_count_for_expr_subqueries(expr));
            let within_group_count = function
                .within_group
                .iter()
                .map(|order| unused_join_count_for_expr_subqueries(&order.expr))
                .sum::<usize>();
            let window_count = match &function.over {
                Some(sqlparser::ast::WindowType::WindowSpec(spec)) => {
                    spec.partition_by
                        .iter()
                        .map(unused_join_count_for_expr_subqueries)
                        .sum::<usize>()
                        + spec
                            .order_by
                            .iter()
                            .map(|order| unused_join_count_for_expr_subqueries(&order.expr))
                            .sum::<usize>()
                }
                _ => 0,
            };
            args_count + filter_count + within_group_count + window_count
        }
        _ => 0,
    }
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

fn visit_named_window_expressions<F: FnMut(&sqlparser::ast::Expr)>(
    select: &Select,
    visitor: &mut F,
) {
    for named_window in &select.named_window {
        if let NamedWindowExpr::WindowSpec(spec) = &named_window.1 {
            for expr in &spec.partition_by {
                visitor(expr);
            }
            for order in &spec.order_by {
                visitor(&order.expr);
            }
        }
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

fn collect_join_relation_reference_prefixes(select: &Select, prefixes: &mut HashSet<String>) {
    for table in &select.from {
        for join in &table.joins {
            collect_table_factor_reference_prefixes(&join.relation, prefixes);
        }
    }
}

fn collect_table_factor_reference_prefixes(
    table_factor: &TableFactor,
    prefixes: &mut HashSet<String>,
) {
    match table_factor {
        TableFactor::Table { .. } => {}
        TableFactor::Derived { .. } => {}
        TableFactor::TableFunction { expr, .. } => {
            collect_qualifier_prefixes_in_expr(expr, prefixes);
        }
        TableFactor::Function { args, .. } => {
            for arg in args {
                collect_function_arg_prefixes(arg, prefixes);
            }
        }
        TableFactor::UNNEST { array_exprs, .. } => {
            for expr in array_exprs {
                collect_qualifier_prefixes_in_expr(expr, prefixes);
            }
        }
        TableFactor::JsonTable { json_expr, .. } | TableFactor::OpenJsonTable { json_expr, .. } => {
            collect_qualifier_prefixes_in_expr(json_expr, prefixes);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_table_factor_reference_prefixes(&table_with_joins.relation, prefixes);
            for join in &table_with_joins.joins {
                collect_table_factor_reference_prefixes(&join.relation, prefixes);
            }
        }
        TableFactor::Pivot {
            table,
            aggregate_functions,
            value_column,
            default_on_null,
            ..
        } => {
            collect_table_factor_reference_prefixes(table, prefixes);
            for expr_with_alias in aggregate_functions {
                collect_qualifier_prefixes_in_expr(&expr_with_alias.expr, prefixes);
            }
            for expr in value_column {
                collect_qualifier_prefixes_in_expr(expr, prefixes);
            }
            if let Some(expr) = default_on_null {
                collect_qualifier_prefixes_in_expr(expr, prefixes);
            }
        }
        TableFactor::Unpivot {
            table,
            value,
            columns,
            ..
        } => {
            collect_table_factor_reference_prefixes(table, prefixes);
            collect_qualifier_prefixes_in_expr(value, prefixes);
            for expr_with_alias in columns {
                collect_qualifier_prefixes_in_expr(&expr_with_alias.expr, prefixes);
            }
        }
        TableFactor::MatchRecognize {
            table,
            partition_by,
            order_by,
            measures,
            ..
        } => {
            collect_table_factor_reference_prefixes(table, prefixes);
            for expr in partition_by {
                collect_qualifier_prefixes_in_expr(expr, prefixes);
            }
            for order in order_by {
                collect_qualifier_prefixes_in_expr(&order.expr, prefixes);
            }
            for measure in measures {
                collect_qualifier_prefixes_in_expr(&measure.expr, prefixes);
            }
        }
        TableFactor::XmlTable { row_expression, .. } => {
            collect_qualifier_prefixes_in_expr(row_expression, prefixes);
        }
        TableFactor::SemanticView { .. } => {}
    }
}

fn collect_function_arg_prefixes(arg: &FunctionArg, prefixes: &mut HashSet<String>) {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
        | FunctionArg::Named {
            arg: FunctionArgExpr::Expr(expr),
            ..
        } => collect_qualifier_prefixes_in_expr(expr, prefixes),
        _ => {}
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
    fn allows_outer_join_when_joined_source_only_referenced_in_query_order_by() {
        let issues = run("select a.id from a left join b on a.id = b.id order by b.id");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_base_from_source_as_unused_with_using_join() {
        let issues = run("select c.id from b left join c using(id)");
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

    #[test]
    fn flags_unused_outer_join_in_multi_root_from_clause() {
        let issues = run("SELECT a.id FROM a, b LEFT JOIN c ON b.id = c.id");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_011);
    }

    #[test]
    fn allows_used_outer_join_in_multi_root_from_clause() {
        let issues = run("SELECT c.id FROM a, b LEFT JOIN c ON b.id = c.id");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_outer_join_source_referenced_by_later_unnest_join_relation() {
        let issues = run(
            "SELECT ft.id, n.generic_field FROM fact_table AS ft LEFT JOIN UNNEST(ft.generic_array) AS g LEFT JOIN UNNEST(g.nested_array) AS n",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_outer_join_source_referenced_in_named_window_clause() {
        let issues = run(
            "SELECT sum(a.value) OVER w FROM a LEFT JOIN b ON a.id = b.id WINDOW w AS (PARTITION BY b.group_key)",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn defers_when_named_window_clause_has_unqualified_reference() {
        let issues = run(
            "SELECT sum(a.value) OVER w FROM a LEFT JOIN b ON a.id = b.id WINDOW w AS (PARTITION BY group_key)",
        );
        assert!(issues.is_empty());
    }
}
