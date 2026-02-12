//! SQL lint auto-fix helpers.
//!
//! Auto-fix support is intentionally conservative and currently targets rules
//! with deterministic rewrites:
//! - LINT_AM_003: remove DISTINCT when GROUP BY is present
//! - LINT_CV_001: CASE WHEN x IS NULL THEN y ELSE x END -> COALESCE(x, y)
//! - LINT_CV_002: COUNT(1) -> COUNT(*)
//! - LINT_ST_002: remove redundant ELSE NULL in CASE expressions

use flowscope_core::{parse_sql_with_dialect, Dialect, ParseError};
use sqlparser::ast::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FixCounts {
    pub distinct_with_group_by: usize,
    pub coalesce_over_case: usize,
    pub count_style: usize,
    pub unnecessary_else_null: usize,
}

impl FixCounts {
    pub fn total(self) -> usize {
        self.distinct_with_group_by
            + self.coalesce_over_case
            + self.count_style
            + self.unnecessary_else_null
    }
}

#[derive(Debug, Clone)]
pub struct FixOutcome {
    pub sql: String,
    pub counts: FixCounts,
    pub changed: bool,
    pub skipped_due_to_comments: bool,
}

/// Apply deterministic lint fixes to a SQL document.
///
/// Notes:
/// - If comment markers are detected, auto-fix is skipped to avoid losing
///   comments when rendering SQL from AST.
/// - Parse errors are returned so callers can decide whether to continue linting.
pub fn apply_lint_fixes(sql: &str, dialect: Dialect) -> Result<FixOutcome, ParseError> {
    if contains_comment_markers(sql, dialect) {
        return Ok(FixOutcome {
            sql: sql.to_string(),
            counts: FixCounts::default(),
            changed: false,
            skipped_due_to_comments: true,
        });
    }

    let mut statements = parse_sql_with_dialect(sql, dialect)?;
    let mut counts = FixCounts::default();

    for stmt in &mut statements {
        fix_statement(stmt, &mut counts);
    }

    if counts.total() == 0 {
        return Ok(FixOutcome {
            sql: sql.to_string(),
            counts,
            changed: false,
            skipped_due_to_comments: false,
        });
    }

    let fixed_sql = render_statements(&statements, sql);
    let changed = fixed_sql != sql;

    Ok(FixOutcome {
        sql: fixed_sql,
        counts,
        changed,
        skipped_due_to_comments: false,
    })
}

fn contains_comment_markers(sql: &str, dialect: Dialect) -> bool {
    sql.contains("--")
        || sql.contains("/*")
        || (matches!(dialect, Dialect::Mysql) && sql.contains('#'))
}

fn render_statements(statements: &[Statement], original: &str) -> String {
    let mut rendered = statements
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(";\n");

    if original.trim_end().ends_with(';') {
        rendered.push(';');
    }

    rendered
}

fn fix_statement(stmt: &mut Statement, counts: &mut FixCounts) {
    match stmt {
        Statement::Query(query) => fix_query(query, counts),
        Statement::Insert(insert) => {
            if let Some(source) = insert.source.as_mut() {
                fix_query(source, counts);
            }
        }
        Statement::CreateView { query, .. } => fix_query(query, counts),
        Statement::CreateTable(create) => {
            if let Some(query) = create.query.as_mut() {
                fix_query(query, counts);
            }
        }
        _ => {}
    }
}

fn fix_query(query: &mut Query, counts: &mut FixCounts) {
    if let Some(with) = query.with.as_mut() {
        for cte in &mut with.cte_tables {
            fix_query(&mut cte.query, counts);
        }
    }

    fix_set_expr(query.body.as_mut(), counts);

    if let Some(order_by) = query.order_by.as_mut() {
        fix_order_by(order_by, counts);
    }

    if let Some(limit_clause) = query.limit_clause.as_mut() {
        fix_limit_clause(limit_clause, counts);
    }

    if let Some(fetch) = query.fetch.as_mut() {
        if let Some(quantity) = fetch.quantity.as_mut() {
            fix_expr(quantity, counts);
        }
    }
}

fn fix_set_expr(body: &mut SetExpr, counts: &mut FixCounts) {
    match body {
        SetExpr::Select(select) => fix_select(select, counts),
        SetExpr::Query(query) => fix_query(query, counts),
        SetExpr::SetOperation { left, right, .. } => {
            fix_set_expr(left, counts);
            fix_set_expr(right, counts);
        }
        SetExpr::Values(values) => {
            for row in &mut values.rows {
                for expr in row {
                    fix_expr(expr, counts);
                }
            }
        }
        SetExpr::Insert(stmt)
        | SetExpr::Update(stmt)
        | SetExpr::Delete(stmt)
        | SetExpr::Merge(stmt) => fix_statement(stmt, counts),
        _ => {}
    }
}

fn fix_select(select: &mut Select, counts: &mut FixCounts) {
    if has_distinct_and_group_by(select) {
        select.distinct = None;
        counts.distinct_with_group_by += 1;
    }

    for item in &mut select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                fix_expr(expr, counts);
            }
            _ => {}
        }
    }

    for table_with_joins in &mut select.from {
        fix_table_factor(&mut table_with_joins.relation, counts);
        for join in &mut table_with_joins.joins {
            fix_table_factor(&mut join.relation, counts);
            fix_join_operator(&mut join.join_operator, counts);
        }
    }

    if let Some(prewhere) = select.prewhere.as_mut() {
        fix_expr(prewhere, counts);
    }

    if let Some(selection) = select.selection.as_mut() {
        fix_expr(selection, counts);
    }

    if let Some(having) = select.having.as_mut() {
        fix_expr(having, counts);
    }

    if let Some(qualify) = select.qualify.as_mut() {
        fix_expr(qualify, counts);
    }

    if let GroupByExpr::Expressions(exprs, _) = &mut select.group_by {
        for expr in exprs {
            fix_expr(expr, counts);
        }
    }

    for expr in &mut select.cluster_by {
        fix_expr(expr, counts);
    }

    for expr in &mut select.distribute_by {
        fix_expr(expr, counts);
    }

    for expr in &mut select.sort_by {
        fix_expr(&mut expr.expr, counts);
    }

    for lateral_view in &mut select.lateral_views {
        fix_expr(&mut lateral_view.lateral_view, counts);
    }

    if let Some(connect_by) = select.connect_by.as_mut() {
        fix_expr(&mut connect_by.condition, counts);
        for relationship in &mut connect_by.relationships {
            fix_expr(relationship, counts);
        }
    }
}

fn has_distinct_and_group_by(select: &Select) -> bool {
    let has_distinct = matches!(
        select.distinct,
        Some(Distinct::Distinct) | Some(Distinct::On(_))
    );
    let has_group_by = match &select.group_by {
        GroupByExpr::All(_) => true,
        GroupByExpr::Expressions(exprs, _) => !exprs.is_empty(),
    };
    has_distinct && has_group_by
}

fn fix_table_factor(relation: &mut TableFactor, counts: &mut FixCounts) {
    match relation {
        TableFactor::Table {
            args, with_hints, ..
        } => {
            if let Some(args) = args {
                for arg in &mut args.args {
                    fix_function_arg(arg, counts);
                }
            }
            for hint in with_hints {
                fix_expr(hint, counts);
            }
        }
        TableFactor::Derived { subquery, .. } => fix_query(subquery, counts),
        TableFactor::TableFunction { expr, .. } => fix_expr(expr, counts),
        TableFactor::Function { args, .. } => {
            for arg in args {
                fix_function_arg(arg, counts);
            }
        }
        TableFactor::UNNEST { array_exprs, .. } => {
            for expr in array_exprs {
                fix_expr(expr, counts);
            }
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            fix_table_factor(&mut table_with_joins.relation, counts);
            for join in &mut table_with_joins.joins {
                fix_table_factor(&mut join.relation, counts);
                fix_join_operator(&mut join.join_operator, counts);
            }
        }
        TableFactor::Pivot {
            table,
            aggregate_functions,
            value_column,
            default_on_null,
            ..
        } => {
            fix_table_factor(table, counts);
            for func in aggregate_functions {
                fix_expr(&mut func.expr, counts);
            }
            for expr in value_column {
                fix_expr(expr, counts);
            }
            if let Some(expr) = default_on_null {
                fix_expr(expr, counts);
            }
        }
        TableFactor::Unpivot {
            table,
            value,
            columns,
            ..
        } => {
            fix_table_factor(table, counts);
            fix_expr(value, counts);
            for column in columns {
                fix_expr(&mut column.expr, counts);
            }
        }
        TableFactor::JsonTable { json_expr, .. } => fix_expr(json_expr, counts),
        TableFactor::OpenJsonTable { json_expr, .. } => fix_expr(json_expr, counts),
        _ => {}
    }
}

fn fix_join_operator(op: &mut JoinOperator, counts: &mut FixCounts) {
    match op {
        JoinOperator::Join(constraint)
        | JoinOperator::Inner(constraint)
        | JoinOperator::Left(constraint)
        | JoinOperator::LeftOuter(constraint)
        | JoinOperator::Right(constraint)
        | JoinOperator::RightOuter(constraint)
        | JoinOperator::FullOuter(constraint)
        | JoinOperator::CrossJoin(constraint)
        | JoinOperator::Semi(constraint)
        | JoinOperator::LeftSemi(constraint)
        | JoinOperator::RightSemi(constraint)
        | JoinOperator::Anti(constraint)
        | JoinOperator::LeftAnti(constraint)
        | JoinOperator::RightAnti(constraint)
        | JoinOperator::StraightJoin(constraint) => fix_join_constraint(constraint, counts),
        JoinOperator::AsOf {
            match_condition,
            constraint,
        } => {
            fix_expr(match_condition, counts);
            fix_join_constraint(constraint, counts);
        }
        JoinOperator::CrossApply | JoinOperator::OuterApply => {}
    }
}

fn fix_join_constraint(constraint: &mut JoinConstraint, counts: &mut FixCounts) {
    if let JoinConstraint::On(expr) = constraint {
        fix_expr(expr, counts);
    }
}

fn fix_order_by(order_by: &mut OrderBy, counts: &mut FixCounts) {
    if let OrderByKind::Expressions(exprs) = &mut order_by.kind {
        for order_expr in exprs {
            fix_expr(&mut order_expr.expr, counts);
        }
    }

    if let Some(interpolate) = order_by.interpolate.as_mut() {
        if let Some(exprs) = interpolate.exprs.as_mut() {
            for expr in exprs {
                if let Some(inner) = expr.expr.as_mut() {
                    fix_expr(inner, counts);
                }
            }
        }
    }
}

fn fix_limit_clause(limit_clause: &mut LimitClause, counts: &mut FixCounts) {
    match limit_clause {
        LimitClause::LimitOffset {
            limit,
            offset,
            limit_by,
        } => {
            if let Some(limit) = limit {
                fix_expr(limit, counts);
            }
            if let Some(offset) = offset {
                fix_expr(&mut offset.value, counts);
            }
            for expr in limit_by {
                fix_expr(expr, counts);
            }
        }
        LimitClause::OffsetCommaLimit { offset, limit } => {
            fix_expr(offset, counts);
            fix_expr(limit, counts);
        }
    }
}

fn fix_expr(expr: &mut Expr, counts: &mut FixCounts) {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            fix_expr(left, counts);
            fix_expr(right, counts);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::IsTrue(inner)
        | Expr::IsNotTrue(inner)
        | Expr::IsFalse(inner)
        | Expr::IsNotFalse(inner)
        | Expr::IsUnknown(inner)
        | Expr::IsNotUnknown(inner) => fix_expr(inner, counts),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand.as_mut() {
                fix_expr(operand, counts);
            }
            for case_when in conditions {
                fix_expr(&mut case_when.condition, counts);
                fix_expr(&mut case_when.result, counts);
            }
            if let Some(else_result) = else_result.as_mut() {
                fix_expr(else_result, counts);
            }
        }
        Expr::Function(func) => fix_function(func, counts),
        Expr::Cast { expr: inner, .. } => fix_expr(inner, counts),
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => {
            fix_expr(inner, counts);
            fix_query(subquery, counts);
        }
        Expr::Subquery(subquery) | Expr::Exists { subquery, .. } => fix_query(subquery, counts),
        Expr::Between {
            expr: target,
            low,
            high,
            ..
        } => {
            fix_expr(target, counts);
            fix_expr(low, counts);
            fix_expr(high, counts);
        }
        Expr::InList {
            expr: target, list, ..
        } => {
            fix_expr(target, counts);
            for item in list {
                fix_expr(item, counts);
            }
        }
        Expr::Tuple(items) => {
            for item in items {
                fix_expr(item, counts);
            }
        }
        _ => {}
    }

    if let Some((check_expr, fallback_expr)) = coalesce_replacement(expr) {
        *expr = build_coalesce_expr(check_expr, fallback_expr);
        counts.coalesce_over_case += 1;
        return;
    }

    if let Expr::Case {
        else_result: Some(else_result),
        ..
    } = expr
    {
        if is_null_expr(else_result) {
            if let Expr::Case { else_result, .. } = expr {
                *else_result = None;
                counts.unnecessary_else_null += 1;
            }
        }
    }
}

fn fix_function(func: &mut Function, counts: &mut FixCounts) {
    if let FunctionArguments::List(arg_list) = &mut func.args {
        for arg in &mut arg_list.args {
            fix_function_arg(arg, counts);
        }
        for clause in &mut arg_list.clauses {
            match clause {
                FunctionArgumentClause::OrderBy(order_by_exprs) => {
                    for order_by_expr in order_by_exprs {
                        fix_expr(&mut order_by_expr.expr, counts);
                    }
                }
                FunctionArgumentClause::Limit(expr) => fix_expr(expr, counts),
                _ => {}
            }
        }
    }

    if let Some(filter) = func.filter.as_mut() {
        fix_expr(filter, counts);
    }

    for order_expr in &mut func.within_group {
        fix_expr(&mut order_expr.expr, counts);
    }

    if is_count_one(func) {
        if let FunctionArguments::List(arg_list) = &mut func.args {
            arg_list.args[0] = FunctionArg::Unnamed(FunctionArgExpr::Wildcard);
            counts.count_style += 1;
        }
    }
}

fn fix_function_arg(arg: &mut FunctionArg, counts: &mut FixCounts) {
    match arg {
        FunctionArg::Named { arg, .. }
        | FunctionArg::ExprNamed { arg, .. }
        | FunctionArg::Unnamed(arg) => {
            if let FunctionArgExpr::Expr(expr) = arg {
                fix_expr(expr, counts);
            }
        }
    }
}

fn is_count_one(func: &Function) -> bool {
    if !func.name.to_string().eq_ignore_ascii_case("COUNT") {
        return false;
    }

    let FunctionArguments::List(arg_list) = &func.args else {
        return false;
    };

    if arg_list.duplicate_treatment.is_some() || !arg_list.clauses.is_empty() {
        return false;
    }

    if arg_list.args.len() != 1 {
        return false;
    }

    matches!(
        &arg_list.args[0],
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(ValueWithSpan {
            value: Value::Number(n, _),
            ..
        }))) if n == "1"
    )
}

fn coalesce_replacement(expr: &Expr) -> Option<(Expr, Expr)> {
    if let Expr::Case {
        operand: None,
        conditions,
        else_result: Some(else_expr),
        ..
    } = expr
    {
        if conditions.len() == 1 {
            let case_when = &conditions[0];
            if let Expr::IsNull(check_expr) = &case_when.condition {
                if exprs_equal(check_expr, else_expr) {
                    return Some(((*check_expr.clone()), case_when.result.clone()));
                }
            }
        }
    }

    None
}

fn build_coalesce_expr(check_expr: Expr, fallback_expr: Expr) -> Expr {
    Expr::Function(Function {
        name: vec![Ident::new("COALESCE")].into(),
        uses_odbc_syntax: false,
        parameters: FunctionArguments::None,
        args: FunctionArguments::List(FunctionArgumentList {
            duplicate_treatment: None,
            args: vec![
                FunctionArg::Unnamed(FunctionArgExpr::Expr(check_expr)),
                FunctionArg::Unnamed(FunctionArgExpr::Expr(fallback_expr)),
            ],
            clauses: vec![],
        }),
        filter: None,
        null_treatment: None,
        over: None,
        within_group: vec![],
    })
}

fn exprs_equal(a: &Expr, b: &Expr) -> bool {
    format!("{a}") == format!("{b}")
}

fn is_null_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Value(ValueWithSpan {
            value: Value::Null,
            ..
        })
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use flowscope_core::{analyze, issue_codes, AnalysisOptions, AnalyzeRequest, LintConfig};

    fn lint_rule_count(sql: &str, code: &str) -> usize {
        let request = AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: Some(AnalysisOptions {
                lint: Some(LintConfig {
                    enabled: true,
                    disabled_rules: vec![],
                }),
                ..Default::default()
            }),
            schema: None,
            #[cfg(feature = "templating")]
            template_config: None,
        };

        analyze(&request)
            .issues
            .iter()
            .filter(|issue| issue.code == code)
            .count()
    }

    fn fix_count_for_code(counts: FixCounts, code: &str) -> usize {
        match code {
            issue_codes::LINT_AM_003 => counts.distinct_with_group_by,
            issue_codes::LINT_CV_001 => counts.coalesce_over_case,
            issue_codes::LINT_CV_002 => counts.count_style,
            issue_codes::LINT_ST_002 => counts.unnecessary_else_null,
            _ => 0,
        }
    }

    fn assert_rule_case(
        sql: &str,
        code: &str,
        expected_before: usize,
        expected_after: usize,
        expected_fix_count: usize,
    ) {
        let before = lint_rule_count(sql, code);
        assert_eq!(
            before, expected_before,
            "unexpected initial lint count for {code} in SQL: {sql}"
        );

        let out = apply_lint_fixes(sql, Dialect::Generic).expect("fix result");
        assert!(
            !out.skipped_due_to_comments,
            "test SQL should not be skipped"
        );
        assert_eq!(
            fix_count_for_code(out.counts, code),
            expected_fix_count,
            "unexpected fix count for {code} in SQL: {sql}"
        );

        if expected_fix_count > 0 {
            assert!(out.changed, "expected SQL to change for {code}: {sql}");
        } else {
            assert!(!out.changed, "expected SQL to remain unchanged: {sql}");
        }

        let after = lint_rule_count(&out.sql, code);
        assert_eq!(
            after, expected_after,
            "unexpected lint count after fix for {code}. SQL: {}",
            out.sql
        );

        let second_pass = apply_lint_fixes(&out.sql, Dialect::Generic).expect("second pass");
        assert_eq!(
            fix_count_for_code(second_pass.counts, code),
            0,
            "expected idempotent second pass for {code}"
        );
        assert!(!second_pass.changed, "expected no second-pass changes");
    }

    #[test]
    fn sqlfluff_am003_cases_are_fixed() {
        let cases = [
            ("SELECT DISTINCT col FROM t GROUP BY col", 1, 0, 1),
            (
                "SELECT * FROM (SELECT DISTINCT a FROM t GROUP BY a) AS sub",
                1,
                0,
                1,
            ),
            (
                "WITH cte AS (SELECT DISTINCT a FROM t GROUP BY a) SELECT * FROM cte",
                1,
                0,
                1,
            ),
            (
                "CREATE VIEW v AS SELECT DISTINCT a FROM t GROUP BY a",
                1,
                0,
                1,
            ),
            (
                "INSERT INTO target SELECT DISTINCT a FROM t GROUP BY a",
                1,
                0,
                1,
            ),
            (
                "SELECT a FROM t UNION ALL SELECT DISTINCT b FROM t2 GROUP BY b",
                1,
                0,
                1,
            ),
            ("SELECT a, b FROM t", 0, 0, 0),
        ];

        for (sql, before, after, fix_count) in cases {
            assert_rule_case(sql, issue_codes::LINT_AM_003, before, after, fix_count);
        }
    }

    #[test]
    fn sqlfluff_cv001_cases_are_fixed_or_unchanged() {
        let cases = [
            (
                "SELECT CASE WHEN x IS NULL THEN 'default' ELSE x END FROM t",
                1,
                0,
                1,
            ),
            (
                "SELECT * FROM t WHERE (CASE WHEN x IS NULL THEN 0 ELSE x END) > 5",
                1,
                0,
                1,
            ),
            (
                "WITH cte AS (SELECT CASE WHEN x IS NULL THEN 0 ELSE x END AS val FROM t) SELECT * FROM cte",
                1,
                0,
                1,
            ),
            (
                "SELECT CASE WHEN x IS NULL THEN 'a' WHEN y IS NULL THEN 'b' ELSE x END FROM t",
                0,
                0,
                0,
            ),
            ("SELECT COALESCE(x, 'default') FROM t", 0, 0, 0),
            (
                "SELECT CASE WHEN x IS NOT NULL THEN x ELSE 'default' END FROM t",
                0,
                0,
                0,
            ),
            ("SELECT CASE x WHEN 1 THEN 'a' ELSE 'b' END FROM t", 0, 0, 0),
        ];

        for (sql, before, after, fix_count) in cases {
            assert_rule_case(sql, issue_codes::LINT_CV_001, before, after, fix_count);
        }
    }

    #[test]
    fn sqlfluff_st002_cases_are_fixed_or_unchanged() {
        let cases = [
            ("SELECT CASE WHEN x > 1 THEN 'a' ELSE NULL END FROM t", 1, 0, 1),
            (
                "SELECT CASE name WHEN 'cat' THEN 'meow' WHEN 'dog' THEN 'woof' ELSE NULL END FROM t",
                1,
                0,
                1,
            ),
            (
                "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' WHEN x = 3 THEN 'c' ELSE NULL END FROM t",
                1,
                0,
                1,
            ),
            (
                "SELECT CASE WHEN x > 0 THEN CASE WHEN y > 0 THEN 'pos' ELSE NULL END ELSE NULL END FROM t",
                2,
                0,
                2,
            ),
            (
                "SELECT * FROM t WHERE (CASE WHEN x > 0 THEN 1 ELSE NULL END) IS NOT NULL",
                1,
                0,
                1,
            ),
            (
                "WITH cte AS (SELECT CASE WHEN x > 0 THEN 'yes' ELSE NULL END AS flag FROM t) SELECT * FROM cte",
                1,
                0,
                1,
            ),
            ("SELECT CASE WHEN x > 1 THEN 'a' END FROM t", 0, 0, 0),
            (
                "SELECT CASE name WHEN 'cat' THEN 'meow' ELSE UPPER(name) END FROM t",
                0,
                0,
                0,
            ),
            ("SELECT CASE WHEN x > 1 THEN 'a' ELSE 'b' END FROM t", 0, 0, 0),
        ];

        for (sql, before, after, fix_count) in cases {
            assert_rule_case(sql, issue_codes::LINT_ST_002, before, after, fix_count);
        }
    }

    #[test]
    fn count_style_cases_are_fixed_or_unchanged() {
        let cases = [
            ("SELECT COUNT(1) FROM t", 1, 0, 1),
            (
                "SELECT col FROM t GROUP BY col HAVING COUNT(1) > 5",
                1,
                0,
                1,
            ),
            (
                "SELECT * FROM t WHERE id IN (SELECT COUNT(1) FROM t2 GROUP BY col)",
                1,
                0,
                1,
            ),
            ("SELECT COUNT(1), COUNT(1) FROM t", 2, 0, 2),
            (
                "WITH cte AS (SELECT COUNT(1) AS cnt FROM t) SELECT * FROM cte",
                1,
                0,
                1,
            ),
            ("SELECT COUNT(*) FROM t", 0, 0, 0),
            ("SELECT COUNT(id) FROM t", 0, 0, 0),
            ("SELECT COUNT(0) FROM t", 0, 0, 0),
            ("SELECT COUNT(DISTINCT id) FROM t", 0, 0, 0),
        ];

        for (sql, before, after, fix_count) in cases {
            assert_rule_case(sql, issue_codes::LINT_CV_002, before, after, fix_count);
        }
    }

    #[test]
    fn skips_files_with_comments() {
        let sql = "-- keep this comment\nSELECT COUNT(1) FROM t";
        let out = apply_lint_fixes(sql, Dialect::Generic).expect("fix result");
        assert!(!out.changed);
        assert!(out.skipped_due_to_comments);
        assert_eq!(out.sql, sql);
    }

    #[test]
    fn skips_files_with_mysql_hash_comments() {
        let sql = "# keep this comment\nSELECT COUNT(1) FROM t";
        let out = apply_lint_fixes(sql, Dialect::Mysql).expect("fix result");
        assert!(!out.changed);
        assert!(out.skipped_due_to_comments);
        assert_eq!(out.sql, sql);
    }
}
