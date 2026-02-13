//! LINT_AL_005: Unused table alias.
//!
//! A table is aliased in a FROM/JOIN clause but the alias is never referenced
//! anywhere in the query. This may indicate dead code or a copy-paste error.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AliasCaseCheck {
    Dialect,
    CaseInsensitive,
    QuotedCsNakedUpper,
    QuotedCsNakedLower,
    CaseSensitive,
}

impl AliasCaseCheck {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_AL_005, "alias_case_check")
            .unwrap_or("dialect")
            .to_ascii_lowercase()
            .as_str()
        {
            "case_insensitive" => Self::CaseInsensitive,
            "quoted_cs_naked_upper" => Self::QuotedCsNakedUpper,
            "quoted_cs_naked_lower" => Self::QuotedCsNakedLower,
            "case_sensitive" => Self::CaseSensitive,
            _ => Self::Dialect,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct AliasRef {
    name: String,
    quoted: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct QualifierRef {
    name: String,
    quoted: bool,
}

pub struct UnusedTableAlias {
    alias_case_check: AliasCaseCheck,
}

impl UnusedTableAlias {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            alias_case_check: AliasCaseCheck::from_config(config),
        }
    }
}

impl Default for UnusedTableAlias {
    fn default() -> Self {
        Self {
            alias_case_check: AliasCaseCheck::Dialect,
        }
    }
}

impl LintRule for UnusedTableAlias {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_005
    }

    fn name(&self) -> &'static str {
        "Unused table alias"
    }

    fn description(&self) -> &'static str {
        "Table alias defined but never referenced in the query."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        match stmt {
            Statement::Query(q) => check_query(q, self.alias_case_check, ctx, &mut issues),
            Statement::Insert(ins) => {
                if let Some(ref source) = ins.source {
                    check_query(source, self.alias_case_check, ctx, &mut issues);
                }
            }
            Statement::CreateView { query, .. } => {
                check_query(query, self.alias_case_check, ctx, &mut issues)
            }
            Statement::CreateTable(create) => {
                if let Some(ref q) = create.query {
                    check_query(q, self.alias_case_check, ctx, &mut issues);
                }
            }
            _ => {}
        }
        issues
    }
}

fn check_query(
    query: &Query,
    alias_case_check: AliasCaseCheck,
    ctx: &LintContext,
    issues: &mut Vec<Issue>,
) {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            check_query(&cte.query, alias_case_check, ctx, issues);
        }
    }
    match query.body.as_ref() {
        SetExpr::Select(select) => check_select(
            select,
            query.order_by.as_ref(),
            alias_case_check,
            ctx,
            issues,
        ),
        _ => check_set_expr(&query.body, alias_case_check, ctx, issues),
    }
}

fn check_set_expr(
    body: &SetExpr,
    alias_case_check: AliasCaseCheck,
    ctx: &LintContext,
    issues: &mut Vec<Issue>,
) {
    match body {
        SetExpr::Select(select) => {
            check_select(select, None, alias_case_check, ctx, issues);
        }
        SetExpr::Query(q) => check_query(q, alias_case_check, ctx, issues),
        SetExpr::SetOperation { left, right, .. } => {
            check_set_expr(left, alias_case_check, ctx, issues);
            check_set_expr(right, alias_case_check, ctx, issues);
        }
        _ => {}
    }
}

fn check_select(
    select: &Select,
    order_by: Option<&OrderBy>,
    alias_case_check: AliasCaseCheck,
    ctx: &LintContext,
    issues: &mut Vec<Issue>,
) {
    // Only flag when there are multiple tables (JOINs) — with a single table,
    // aliases are less important
    let table_count: usize = select.from.iter().map(|f| 1 + f.joins.len()).sum();
    if table_count < 2 {
        return;
    }

    // Collect aliases -> table names
    let mut aliases: HashMap<String, AliasRef> = HashMap::new();
    for from_item in &select.from {
        collect_aliases(&from_item.relation, &mut aliases);
        for join in &from_item.joins {
            collect_aliases(&join.relation, &mut aliases);
        }
    }

    if aliases.is_empty() {
        return;
    }

    let mut used_prefixes: HashSet<QualifierRef> = HashSet::new();
    collect_identifier_prefixes_from_select(select, order_by, &mut used_prefixes);

    for alias in aliases.values() {
        let used = used_prefixes
            .iter()
            .any(|prefix| qualifier_matches_alias(prefix, alias, alias_case_check));
        if !used {
            issues.push(
                Issue::warning(
                    issue_codes::LINT_AL_005,
                    format!(
                        "Table alias '{}' is defined but never referenced.",
                        alias.name
                    ),
                )
                .with_statement(ctx.statement_index),
            );
        }
    }
}

fn collect_identifier_prefixes_from_order_by(
    order_by: &OrderBy,
    prefixes: &mut HashSet<QualifierRef>,
) {
    if let OrderByKind::Expressions(order_by_exprs) = &order_by.kind {
        for order_expr in order_by_exprs {
            collect_identifier_prefixes(&order_expr.expr, prefixes);
        }
    }
}

fn collect_identifier_prefixes_from_query(query: &Query, prefixes: &mut HashSet<QualifierRef>) {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            collect_identifier_prefixes_from_query(&cte.query, prefixes);
        }
    }

    match query.body.as_ref() {
        SetExpr::Select(select) => {
            collect_identifier_prefixes_from_select(select, query.order_by.as_ref(), prefixes);
        }
        SetExpr::Query(q) => collect_identifier_prefixes_from_query(q, prefixes),
        SetExpr::SetOperation { left, right, .. } => {
            collect_identifier_prefixes_from_set_expr(left, prefixes);
            collect_identifier_prefixes_from_set_expr(right, prefixes);
        }
        _ => {}
    }
}

fn collect_identifier_prefixes_from_set_expr(body: &SetExpr, prefixes: &mut HashSet<QualifierRef>) {
    match body {
        SetExpr::Select(select) => collect_identifier_prefixes_from_select(select, None, prefixes),
        SetExpr::Query(q) => collect_identifier_prefixes_from_query(q, prefixes),
        SetExpr::SetOperation { left, right, .. } => {
            collect_identifier_prefixes_from_set_expr(left, prefixes);
            collect_identifier_prefixes_from_set_expr(right, prefixes);
        }
        _ => {}
    }
}

fn collect_identifier_prefixes_from_select(
    select: &Select,
    order_by: Option<&OrderBy>,
    prefixes: &mut HashSet<QualifierRef>,
) {
    for item in &select.projection {
        collect_identifier_prefixes_from_select_item(item, prefixes);
    }
    if let Some(ref prewhere) = select.prewhere {
        collect_identifier_prefixes(prewhere, prefixes);
    }
    if let Some(ref selection) = select.selection {
        collect_identifier_prefixes(selection, prefixes);
    }
    if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            collect_identifier_prefixes(expr, prefixes);
        }
    }
    for expr in &select.cluster_by {
        collect_identifier_prefixes(expr, prefixes);
    }
    for expr in &select.distribute_by {
        collect_identifier_prefixes(expr, prefixes);
    }
    for sort_expr in &select.sort_by {
        collect_identifier_prefixes(&sort_expr.expr, prefixes);
    }
    if let Some(ref having) = select.having {
        collect_identifier_prefixes(having, prefixes);
    }
    if let Some(ref qualify) = select.qualify {
        collect_identifier_prefixes(qualify, prefixes);
    }
    if let Some(Distinct::On(exprs)) = &select.distinct {
        for expr in exprs {
            collect_identifier_prefixes(expr, prefixes);
        }
    }
    for named_window in &select.named_window {
        if let NamedWindowExpr::WindowSpec(spec) = &named_window.1 {
            for expr in &spec.partition_by {
                collect_identifier_prefixes(expr, prefixes);
            }
            for order_expr in &spec.order_by {
                collect_identifier_prefixes(&order_expr.expr, prefixes);
            }
        }
    }
    for lateral_view in &select.lateral_views {
        collect_identifier_prefixes(&lateral_view.lateral_view, prefixes);
    }
    if let Some(connect_by) = &select.connect_by {
        collect_identifier_prefixes(&connect_by.condition, prefixes);
        for relationship in &connect_by.relationships {
            collect_identifier_prefixes(relationship, prefixes);
        }
    }
    for from_item in &select.from {
        for join in &from_item.joins {
            if let Some(constraint) = join_constraint(&join.join_operator) {
                collect_identifier_prefixes(constraint, prefixes);
            }
        }
    }
    if let Some(order_by) = order_by {
        collect_identifier_prefixes_from_order_by(order_by, prefixes);
    }
}

fn collect_aliases(relation: &TableFactor, aliases: &mut HashMap<String, AliasRef>) {
    match relation {
        TableFactor::Table {
            name,
            alias: Some(alias),
            ..
        } => {
            let table_name = name.to_string();
            let alias_name = alias.name.value.clone();
            // Only count as alias if it differs from the table name.
            if alias_name.to_uppercase() != table_name.to_uppercase() {
                aliases.insert(
                    alias_name.clone(),
                    AliasRef {
                        name: alias_name,
                        quoted: alias.name.quote_style.is_some(),
                    },
                );
            }
        }
        TableFactor::Derived {
            lateral,
            subquery,
            alias: Some(alias),
            ..
        } => {
            // SQLFluff AL05 compatibility:
            // - Do not enforce usage for LATERAL aliases.
            // - Do not enforce usage for VALUES-derived aliases.
            let is_values = matches!(subquery.body.as_ref(), SetExpr::Values(_));
            if !*lateral && !is_values {
                aliases.insert(
                    alias.name.value.clone(),
                    AliasRef {
                        name: alias.name.value.clone(),
                        quoted: alias.name.quote_style.is_some(),
                    },
                );
            }
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_aliases(&table_with_joins.relation, aliases);
            for join in &table_with_joins.joins {
                collect_aliases(&join.relation, aliases);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => collect_aliases(table, aliases),
        _ => {}
    }
}

fn collect_identifier_prefixes_from_select_item(
    item: &SelectItem,
    prefixes: &mut HashSet<QualifierRef>,
) {
    match item {
        SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
            collect_identifier_prefixes(expr, prefixes);
        }
        SelectItem::QualifiedWildcard(name, _) => {
            let name_str = name.to_string();
            if let Some(prefix) = name_str.split('.').next() {
                prefixes.insert(QualifierRef {
                    name: prefix
                        .trim_matches(|ch| matches!(ch, '"' | '`' | '\'' | '[' | ']'))
                        .to_string(),
                    quoted: prefix.starts_with('"')
                        || prefix.starts_with('`')
                        || prefix.starts_with('['),
                });
            }
        }
        _ => {}
    }
}

fn collect_identifier_prefixes(expr: &Expr, prefixes: &mut HashSet<QualifierRef>) {
    match expr {
        Expr::CompoundIdentifier(parts) => {
            if parts.len() >= 2 {
                prefixes.insert(QualifierRef {
                    name: parts[0].value.clone(),
                    quoted: parts[0].quote_style.is_some(),
                });
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_identifier_prefixes(left, prefixes);
            collect_identifier_prefixes(right, prefixes);
        }
        Expr::UnaryOp { expr: inner, .. } => collect_identifier_prefixes(inner, prefixes),
        Expr::Nested(inner) => collect_identifier_prefixes(inner, prefixes),
        Expr::Function(func) => {
            if let FunctionArguments::List(arg_list) = &func.args {
                for arg in &arg_list.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(e),
                            ..
                        } => collect_identifier_prefixes(e, prefixes),
                        _ => {}
                    }
                }
            }
            if let Some(filter) = &func.filter {
                collect_identifier_prefixes(filter, prefixes);
            }
            for order_expr in &func.within_group {
                collect_identifier_prefixes(&order_expr.expr, prefixes);
            }
            if let Some(WindowType::WindowSpec(spec)) = &func.over {
                for expr in &spec.partition_by {
                    collect_identifier_prefixes(expr, prefixes);
                }
                for order_expr in &spec.order_by {
                    collect_identifier_prefixes(&order_expr.expr, prefixes);
                }
            }
        }
        Expr::IsNull(inner) | Expr::IsNotNull(inner) | Expr::Cast { expr: inner, .. } => {
            collect_identifier_prefixes(inner, prefixes);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                collect_identifier_prefixes(op, prefixes);
            }
            for case_when in conditions {
                collect_identifier_prefixes(&case_when.condition, prefixes);
                collect_identifier_prefixes(&case_when.result, prefixes);
            }
            if let Some(el) = else_result {
                collect_identifier_prefixes(el, prefixes);
            }
        }
        Expr::InList { expr, list, .. } => {
            collect_identifier_prefixes(expr, prefixes);
            for item in list {
                collect_identifier_prefixes(item, prefixes);
            }
        }
        Expr::InSubquery { expr, subquery, .. } => {
            collect_identifier_prefixes(expr, prefixes);
            collect_identifier_prefixes_from_query(subquery, prefixes);
        }
        Expr::AnyOp { left, right, .. } | Expr::AllOp { left, right, .. } => {
            collect_identifier_prefixes(left, prefixes);
            collect_identifier_prefixes(right, prefixes);
        }
        Expr::Subquery(subquery) | Expr::Exists { subquery, .. } => {
            collect_identifier_prefixes_from_query(subquery, prefixes);
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            collect_identifier_prefixes(expr, prefixes);
            collect_identifier_prefixes(low, prefixes);
            collect_identifier_prefixes(high, prefixes);
        }
        _ => {}
    }
}

fn qualifier_matches_alias(
    qualifier: &QualifierRef,
    alias: &AliasRef,
    alias_case_check: AliasCaseCheck,
) -> bool {
    match alias_case_check {
        AliasCaseCheck::CaseInsensitive => qualifier.name.eq_ignore_ascii_case(&alias.name),
        AliasCaseCheck::CaseSensitive => qualifier.name == alias.name,
        AliasCaseCheck::Dialect => {
            if qualifier.quoted || alias.quoted {
                qualifier.name == alias.name
            } else {
                qualifier.name.eq_ignore_ascii_case(&alias.name)
            }
        }
        AliasCaseCheck::QuotedCsNakedUpper => {
            normalize_case_for_mode(qualifier, alias_case_check)
                == normalize_case_for_mode_alias(alias, alias_case_check)
        }
        AliasCaseCheck::QuotedCsNakedLower => {
            normalize_case_for_mode(qualifier, alias_case_check)
                == normalize_case_for_mode_alias(alias, alias_case_check)
        }
    }
}

fn normalize_case_for_mode(reference: &QualifierRef, mode: AliasCaseCheck) -> String {
    match mode {
        AliasCaseCheck::QuotedCsNakedUpper => {
            if reference.quoted {
                reference.name.clone()
            } else {
                reference.name.to_ascii_uppercase()
            }
        }
        AliasCaseCheck::QuotedCsNakedLower => {
            if reference.quoted {
                reference.name.clone()
            } else {
                reference.name.to_ascii_lowercase()
            }
        }
        _ => reference.name.clone(),
    }
}

fn normalize_case_for_mode_alias(alias: &AliasRef, mode: AliasCaseCheck) -> String {
    match mode {
        AliasCaseCheck::QuotedCsNakedUpper => {
            if alias.quoted {
                alias.name.clone()
            } else {
                alias.name.to_ascii_uppercase()
            }
        }
        AliasCaseCheck::QuotedCsNakedLower => {
            if alias.quoted {
                alias.name.clone()
            } else {
                alias.name.to_ascii_lowercase()
            }
        }
        _ => alias.name.clone(),
    }
}

fn join_constraint(op: &JoinOperator) -> Option<&Expr> {
    let constraint = match op {
        JoinOperator::Join(c)
        | JoinOperator::Left(c)
        | JoinOperator::Inner(c)
        | JoinOperator::Right(c)
        | JoinOperator::LeftOuter(c)
        | JoinOperator::RightOuter(c)
        | JoinOperator::FullOuter(c)
        | JoinOperator::LeftSemi(c)
        | JoinOperator::RightSemi(c)
        | JoinOperator::LeftAnti(c)
        | JoinOperator::RightAnti(c) => c,
        _ => return None,
    };
    match constraint {
        JoinConstraint::On(expr) => Some(expr),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = UnusedTableAlias::default();
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
    fn test_unused_alias_detected() {
        let issues = check_sql("SELECT * FROM users u JOIN orders o ON users.id = orders.user_id");
        // Both aliases u and o are unused (full table names used instead)
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].code, "LINT_AL_005");
    }

    #[test]
    fn test_used_alias_ok() {
        let issues = check_sql("SELECT u.name FROM users u JOIN orders o ON u.id = o.user_id");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_single_table_no_check() {
        // With a single table, we don't flag unused aliases
        let issues = check_sql("SELECT * FROM users u");
        assert!(issues.is_empty());
    }

    // --- Edge cases adopted from sqlfluff aliasing rules ---

    #[test]
    fn test_alias_used_in_where() {
        let issues = check_sql(
            "SELECT u.name FROM users u JOIN orders o ON u.id = o.user_id WHERE u.active = true",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_in_group_by() {
        let issues = check_sql(
            "SELECT u.name, COUNT(*) FROM users u JOIN orders o ON u.id = o.user_id GROUP BY u.name",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_in_having() {
        let issues = check_sql(
            "SELECT u.name, COUNT(o.id) FROM users u JOIN orders o ON u.id = o.user_id \
             GROUP BY u.name HAVING COUNT(o.id) > 5",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_in_qualified_wildcard() {
        // u is used via u.*, o is used in JOIN ON condition
        let issues = check_sql("SELECT u.* FROM users u JOIN orders o ON u.id = o.user_id");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_unused_despite_qualified_wildcard() {
        // u is used via u.*, but o is never referenced (join uses full table name)
        let issues = check_sql("SELECT u.* FROM users u JOIN orders o ON u.id = orders.user_id");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("o"));
    }

    #[test]
    fn test_partial_alias_usage() {
        // Only one of two aliases is used
        let issues = check_sql("SELECT u.name FROM users u JOIN orders o ON u.id = orders.user_id");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("o"));
    }

    #[test]
    fn test_three_tables_one_unused() {
        let issues = check_sql(
            "SELECT a.name, b.total \
             FROM users a \
             JOIN orders b ON a.id = b.user_id \
             JOIN products c ON b.product_id = products.id",
        );
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("c"));
    }

    #[test]
    fn test_no_aliases_ok() {
        let issues =
            check_sql("SELECT users.name FROM users JOIN orders ON users.id = orders.user_id");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_self_join_with_aliases() {
        let issues =
            check_sql("SELECT a.name, b.name FROM users a JOIN users b ON a.manager_id = b.id");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_in_case_expression() {
        let issues = check_sql(
            "SELECT CASE WHEN u.active THEN 'yes' ELSE 'no' END \
             FROM users u JOIN orders o ON u.id = o.user_id",
        );
        // u is used in CASE, o is used in JOIN ON
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_in_order_by() {
        let issues = check_sql(
            "SELECT u.name \
             FROM users u \
             JOIN orders o ON users.id = orders.user_id \
             ORDER BY o.created_at",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_left_join_alias_used_in_on_clause() {
        let issues = check_sql("SELECT u.name FROM users u LEFT JOIN orders o ON u.id = o.user_id");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_only_in_correlated_exists_subquery() {
        let issues = check_sql(
            "SELECT 1 \
             FROM users u \
             JOIN orders o ON 1 = 1 \
             WHERE EXISTS (SELECT 1 WHERE u.id = o.user_id)",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_in_qualify_clause() {
        let issues = check_sql(
            "SELECT u.id \
             FROM users u \
             JOIN orders o ON users.id = orders.user_id \
             QUALIFY ROW_NUMBER() OVER (PARTITION BY o.user_id ORDER BY o.user_id) = 1",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_in_named_window_clause() {
        let issues = check_sql(
            "SELECT SUM(u.id) OVER w \
             FROM users u \
             JOIN orders o ON users.id = orders.user_id \
             WINDOW w AS (PARTITION BY o.user_id)",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_unused_derived_alias_detected() {
        let issues = check_sql(
            "SELECT u.id \
             FROM users u \
             JOIN (SELECT id FROM orders) o2 ON u.id = u.id",
        );
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("o2"));
    }

    #[test]
    fn test_lateral_alias_is_ignored() {
        let issues = check_sql("SELECT u.id FROM users u JOIN LATERAL (SELECT 1) lx ON TRUE");
        assert!(issues.is_empty());
    }

    #[test]
    fn alias_case_check_case_sensitive_flags_case_mismatch() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unused".to_string(),
                serde_json::json!({"alias_case_check": "case_sensitive"}),
            )]),
        };
        let rule = UnusedTableAlias::from_config(&config);
        let sql = "SELECT zoo.id, b.id FROM users AS \"Zoo\" JOIN books b ON zoo.id = b.user_id";
        let stmts = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &stmts[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("Zoo"));
    }

    #[test]
    fn alias_case_check_case_insensitive_allows_case_mismatch() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_AL_005".to_string(),
                serde_json::json!({"alias_case_check": "case_insensitive"}),
            )]),
        };
        let rule = UnusedTableAlias::from_config(&config);
        let sql = "SELECT zoo.id, b.id FROM users AS \"Zoo\" JOIN books b ON zoo.id = b.user_id";
        let stmts = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &stmts[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn alias_case_check_quoted_cs_naked_upper_allows_unquoted_upper_fold_for_quoted_alias() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unused".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_upper"}),
            )]),
        };
        let rule = UnusedTableAlias::from_config(&config);
        let sql = "SELECT foo.id, b.id FROM users AS \"FOO\" JOIN books b ON foo.id = b.user_id";
        let stmts = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &stmts[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn alias_case_check_quoted_cs_naked_lower_allows_unquoted_lower_fold_for_quoted_alias() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unused".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_lower"}),
            )]),
        };
        let rule = UnusedTableAlias::from_config(&config);
        let sql = "SELECT FOO.id, b.id FROM users AS \"foo\" JOIN books b ON FOO.id = b.user_id";
        let stmts = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &stmts[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert!(issues.is_empty());
    }
}
