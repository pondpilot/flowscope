//! LINT_AM_006: Ambiguous column references.
//!
//! SQLFluff AM06 parity: enforce consistent `GROUP BY` / `ORDER BY` reference
//! styles (implicit numeric position vs explicit expressions/identifiers).

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{
    Expr, FunctionArg, FunctionArgExpr, FunctionArguments, GroupByExpr, GroupByWithModifier,
    OrderByKind, Query, Select, SelectItem, SetExpr, Statement, TableFactor, Value, WindowType,
};

use super::semantic_helpers::join_on_expr;

pub struct AmbiguousColumnRefs;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReferenceStyle {
    Explicit,
    Implicit,
}

impl LintRule for AmbiguousColumnRefs {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_006
    }

    fn name(&self) -> &'static str {
        "Ambiguous column references"
    }

    fn description(&self) -> &'static str {
        "Inconsistent column references in GROUP BY/ORDER BY clauses."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut prior_style = None;
        check_statement(
            statement,
            ctx.statement_index,
            &mut prior_style,
            &mut issues,
        );
        issues
    }
}

fn check_statement(
    statement: &Statement,
    statement_index: usize,
    prior_style: &mut Option<ReferenceStyle>,
    issues: &mut Vec<Issue>,
) {
    match statement {
        Statement::Query(query) => check_query(query, statement_index, prior_style, issues),
        Statement::Insert(insert) => {
            if let Some(source) = &insert.source {
                check_query(source, statement_index, prior_style, issues);
            }
        }
        Statement::CreateView { query, .. } => {
            check_query(query, statement_index, prior_style, issues);
        }
        Statement::CreateTable(create) => {
            if let Some(query) = &create.query {
                check_query(query, statement_index, prior_style, issues);
            }
        }
        _ => {}
    }
}

fn check_query(
    query: &Query,
    statement_index: usize,
    prior_style: &mut Option<ReferenceStyle>,
    issues: &mut Vec<Issue>,
) {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            check_query(&cte.query, statement_index, prior_style, issues);
        }
    }

    check_set_expr(&query.body, statement_index, prior_style, issues);

    if let Some(order_by) = &query.order_by {
        let mut has_explicit = false;
        let mut has_implicit = false;

        if let OrderByKind::Expressions(order_exprs) = &order_by.kind {
            for order_expr in order_exprs {
                classify_reference_style_in_expr(
                    &order_expr.expr,
                    &mut has_explicit,
                    &mut has_implicit,
                );
            }
        }

        apply_consistent_style_policy(
            has_explicit,
            has_implicit,
            statement_index,
            prior_style,
            issues,
        );
    }
}

fn check_set_expr(
    set_expr: &SetExpr,
    statement_index: usize,
    prior_style: &mut Option<ReferenceStyle>,
    issues: &mut Vec<Issue>,
) {
    match set_expr {
        SetExpr::Select(select) => check_select(select, statement_index, prior_style, issues),
        SetExpr::Query(query) => check_query(query, statement_index, prior_style, issues),
        SetExpr::SetOperation { left, right, .. } => {
            check_set_expr(left, statement_index, prior_style, issues);
            check_set_expr(right, statement_index, prior_style, issues);
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => {
            check_statement(statement, statement_index, prior_style, issues);
        }
        _ => {}
    }
}

fn check_select(
    select: &Select,
    statement_index: usize,
    prior_style: &mut Option<ReferenceStyle>,
    issues: &mut Vec<Issue>,
) {
    // Traverse subqueries in approximate lexical order before evaluating this
    // SELECT's GROUP BY clause, matching SQLFluff's clause-order precedence.
    for item in &select.projection {
        if let SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } = item {
            visit_subqueries_in_expr(expr, statement_index, prior_style, issues);
        }
    }

    if let Some(prewhere) = &select.prewhere {
        visit_subqueries_in_expr(prewhere, statement_index, prior_style, issues);
    }

    for table in &select.from {
        check_table_factor(&table.relation, statement_index, prior_style, issues);
        for join in &table.joins {
            check_table_factor(&join.relation, statement_index, prior_style, issues);
            if let Some(on_expr) = join_on_expr(&join.join_operator) {
                visit_subqueries_in_expr(on_expr, statement_index, prior_style, issues);
            }
        }
    }

    if let Some(selection) = &select.selection {
        visit_subqueries_in_expr(selection, statement_index, prior_style, issues);
    }

    let mut has_explicit = false;
    let mut has_implicit = false;
    if let GroupByExpr::Expressions(exprs, modifiers) = &select.group_by {
        for expr in exprs {
            classify_reference_style_in_expr(expr, &mut has_explicit, &mut has_implicit);
        }

        for modifier in modifiers {
            classify_reference_style_in_group_modifier(
                modifier,
                &mut has_explicit,
                &mut has_implicit,
            );
        }
    }
    apply_consistent_style_policy(
        has_explicit,
        has_implicit,
        statement_index,
        prior_style,
        issues,
    );

    if let Some(having) = &select.having {
        visit_subqueries_in_expr(having, statement_index, prior_style, issues);
    }
    if let Some(qualify) = &select.qualify {
        visit_subqueries_in_expr(qualify, statement_index, prior_style, issues);
    }

    for sort_expr in &select.sort_by {
        visit_subqueries_in_expr(&sort_expr.expr, statement_index, prior_style, issues);
    }
}

fn check_table_factor(
    table_factor: &TableFactor,
    statement_index: usize,
    prior_style: &mut Option<ReferenceStyle>,
    issues: &mut Vec<Issue>,
) {
    match table_factor {
        TableFactor::Derived { subquery, .. } => {
            check_query(subquery, statement_index, prior_style, issues);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            check_table_factor(
                &table_with_joins.relation,
                statement_index,
                prior_style,
                issues,
            );
            for join in &table_with_joins.joins {
                check_table_factor(&join.relation, statement_index, prior_style, issues);
                if let Some(on_expr) = join_on_expr(&join.join_operator) {
                    visit_subqueries_in_expr(on_expr, statement_index, prior_style, issues);
                }
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            check_table_factor(table, statement_index, prior_style, issues)
        }
        _ => {}
    }
}

fn visit_subqueries_in_expr(
    expr: &Expr,
    statement_index: usize,
    prior_style: &mut Option<ReferenceStyle>,
    issues: &mut Vec<Issue>,
) {
    match expr {
        Expr::Subquery(query)
        | Expr::Exists {
            subquery: query, ..
        } => check_query(query, statement_index, prior_style, issues),
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => {
            visit_subqueries_in_expr(inner, statement_index, prior_style, issues);
            check_query(subquery, statement_index, prior_style, issues);
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. } => {
            visit_subqueries_in_expr(left, statement_index, prior_style, issues);
            visit_subqueries_in_expr(right, statement_index, prior_style, issues);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            visit_subqueries_in_expr(inner, statement_index, prior_style, issues);
        }
        Expr::InList { expr, list, .. } => {
            visit_subqueries_in_expr(expr, statement_index, prior_style, issues);
            for item in list {
                visit_subqueries_in_expr(item, statement_index, prior_style, issues);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            visit_subqueries_in_expr(expr, statement_index, prior_style, issues);
            visit_subqueries_in_expr(low, statement_index, prior_style, issues);
            visit_subqueries_in_expr(high, statement_index, prior_style, issues);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand {
                visit_subqueries_in_expr(operand, statement_index, prior_style, issues);
            }
            for when in conditions {
                visit_subqueries_in_expr(&when.condition, statement_index, prior_style, issues);
                visit_subqueries_in_expr(&when.result, statement_index, prior_style, issues);
            }
            if let Some(otherwise) = else_result {
                visit_subqueries_in_expr(otherwise, statement_index, prior_style, issues);
            }
        }
        Expr::Function(function) => {
            if let FunctionArguments::List(arguments) = &function.args {
                for arg in &arguments.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(expr),
                            ..
                        } => visit_subqueries_in_expr(expr, statement_index, prior_style, issues),
                        _ => {}
                    }
                }
            }

            if let Some(filter) = &function.filter {
                visit_subqueries_in_expr(filter, statement_index, prior_style, issues);
            }

            for order_expr in &function.within_group {
                visit_subqueries_in_expr(&order_expr.expr, statement_index, prior_style, issues);
            }

            if let Some(WindowType::WindowSpec(spec)) = &function.over {
                for expr in &spec.partition_by {
                    visit_subqueries_in_expr(expr, statement_index, prior_style, issues);
                }
                for order_expr in &spec.order_by {
                    visit_subqueries_in_expr(
                        &order_expr.expr,
                        statement_index,
                        prior_style,
                        issues,
                    );
                }
            }
        }
        _ => {}
    }
}

fn classify_reference_style_in_group_modifier(
    modifier: &GroupByWithModifier,
    has_explicit: &mut bool,
    has_implicit: &mut bool,
) {
    if let GroupByWithModifier::GroupingSets(expr) = modifier {
        classify_reference_style_in_expr(expr, has_explicit, has_implicit);
    }
}

fn classify_reference_style_in_expr(expr: &Expr, has_explicit: &mut bool, has_implicit: &mut bool) {
    match expr {
        Expr::Value(value) if matches!(value.value, Value::Number(_, _)) => *has_implicit = true,
        Expr::Rollup(sets) | Expr::Cube(sets) | Expr::GroupingSets(sets) => {
            for set in sets {
                for item in set {
                    classify_reference_style_in_expr(item, has_explicit, has_implicit);
                }
            }
        }
        _ => *has_explicit = true,
    }
}

fn apply_consistent_style_policy(
    has_explicit: bool,
    has_implicit: bool,
    statement_index: usize,
    prior_style: &mut Option<ReferenceStyle>,
    issues: &mut Vec<Issue>,
) {
    if !has_explicit && !has_implicit {
        return;
    }

    if has_explicit && has_implicit {
        issues.push(
            Issue::warning(
                issue_codes::LINT_AM_006,
                "Inconsistent column references in 'GROUP BY/ORDER BY' clauses.",
            )
            .with_statement(statement_index),
        );
        return;
    }

    let current_style = if has_implicit {
        ReferenceStyle::Implicit
    } else {
        ReferenceStyle::Explicit
    };

    if let Some(previous_style) = prior_style {
        if *previous_style != current_style {
            issues.push(
                Issue::warning(
                    issue_codes::LINT_AM_006,
                    "Inconsistent column references in 'GROUP BY/ORDER BY' clauses.",
                )
                .with_statement(statement_index),
            );
            return;
        }
    }

    *prior_style = Some(current_style);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AmbiguousColumnRefs;
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

    // --- Edge cases adopted from sqlfluff AM06 ---

    #[test]
    fn passes_explicit_group_by_default() {
        let issues =
            run("SELECT foo, bar, sum(baz) AS sum_value FROM fake_table GROUP BY foo, bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn passes_implicit_group_by_default() {
        let issues = run("SELECT foo, bar, sum(baz) AS sum_value FROM fake_table GROUP BY 1, 2");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_mixed_group_by_references() {
        let issues = run("SELECT foo, bar, sum(baz) AS sum_value FROM fake_table GROUP BY 1, bar");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_006);
    }

    #[test]
    fn flags_mixed_order_by_references() {
        let issues =
            run("SELECT foo, bar, sum(baz) AS sum_value FROM fake_table ORDER BY 1, power(bar, 2)");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_006);
    }

    #[test]
    fn flags_across_clause_style_switch() {
        let issues = run(
            "SELECT foo, bar, sum(baz) AS sum_value FROM fake_table GROUP BY 1, 2 ORDER BY foo, bar",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn passes_consistent_explicit_group_and_order() {
        let issues = run(
            "SELECT foo, bar, sum(baz) AS sum_value FROM fake_table GROUP BY foo, bar ORDER BY foo, bar",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn passes_consistent_implicit_group_and_order() {
        let issues = run(
            "SELECT foo, bar, sum(baz) AS sum_value FROM fake_table GROUP BY 1, 2 ORDER BY 1, 2",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn ignores_window_order_by() {
        let issues = run(
            "SELECT field_1, SUM(field_3) OVER (ORDER BY field_1) FROM table1 GROUP BY 1 ORDER BY 1",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn ignores_within_group_order_by() {
        let issues = run(
            "SELECT LISTAGG(x) WITHIN GROUP (ORDER BY list_order) AS my_list FROM main GROUP BY 1",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_two_violations_when_subquery_sets_explicit_precedent() {
        let issues = run(
            "SELECT foo, bar, sum(baz) AS sum_value FROM (SELECT foo, bar, sum(baz) AS baz FROM fake_table GROUP BY foo, bar) q GROUP BY 1, 2 ORDER BY 1, 2",
        );
        assert_eq!(issues.len(), 2);
        assert!(issues
            .iter()
            .all(|issue| issue.code == issue_codes::LINT_AM_006));
    }

    #[test]
    fn passes_rollup_when_consistent_with_prior_implicit_style() {
        let issues = run(
            "SELECT column1, column2 FROM table_name GROUP BY 1, 2 UNION ALL SELECT column1, column2 FROM table_name2 GROUP BY ROLLUP(1, 2)",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_rollup_when_inconsistent_with_prior_explicit_style() {
        let issues = run(
            "SELECT column1, column2 FROM table_name GROUP BY column1, column2 UNION ALL SELECT column1, column2 FROM table_name2 GROUP BY ROLLUP(1, 2)",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn ignores_statements_without_group_or_order_references() {
        let issues = run("SELECT a.id, name FROM a");
        assert!(issues.is_empty());
    }
}
