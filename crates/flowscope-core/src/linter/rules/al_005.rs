//! LINT_AL_005: Unused table alias.
//!
//! A table is aliased in a FROM/JOIN clause but the alias is never referenced
//! anywhere in the query. This may indicate dead code or a copy-paste error.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
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
    for from_item in &select.from {
        check_table_factor_subqueries(&from_item.relation, alias_case_check, ctx, issues);
        for join in &from_item.joins {
            check_table_factor_subqueries(&join.relation, alias_case_check, ctx, issues);
        }
    }

    // Collect aliases -> table names
    let mut aliases: HashMap<String, AliasRef> = HashMap::new();
    for from_item in &select.from {
        collect_aliases(&from_item.relation, ctx.dialect(), &mut aliases);
        for join in &from_item.joins {
            collect_aliases(&join.relation, ctx.dialect(), &mut aliases);
        }
    }

    if aliases.is_empty() {
        return;
    }

    let mut used_prefixes: HashSet<QualifierRef> = HashSet::new();
    collect_identifier_prefixes_from_select(select, order_by, ctx.dialect(), &mut used_prefixes);

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
    dialect: Dialect,
    prefixes: &mut HashSet<QualifierRef>,
) {
    if let OrderByKind::Expressions(order_by_exprs) = &order_by.kind {
        for order_expr in order_by_exprs {
            collect_identifier_prefixes(&order_expr.expr, dialect, prefixes);
        }
    }
}

fn collect_identifier_prefixes_from_query(
    query: &Query,
    dialect: Dialect,
    prefixes: &mut HashSet<QualifierRef>,
) {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            collect_identifier_prefixes_from_query(&cte.query, dialect, prefixes);
        }
    }

    match query.body.as_ref() {
        SetExpr::Select(select) => {
            collect_identifier_prefixes_from_select(
                select,
                query.order_by.as_ref(),
                dialect,
                prefixes,
            );
        }
        SetExpr::Query(q) => collect_identifier_prefixes_from_query(q, dialect, prefixes),
        SetExpr::SetOperation { left, right, .. } => {
            collect_identifier_prefixes_from_set_expr(left, dialect, prefixes);
            collect_identifier_prefixes_from_set_expr(right, dialect, prefixes);
        }
        _ => {}
    }
}

fn collect_identifier_prefixes_from_set_expr(
    body: &SetExpr,
    dialect: Dialect,
    prefixes: &mut HashSet<QualifierRef>,
) {
    match body {
        SetExpr::Select(select) => {
            collect_identifier_prefixes_from_select(select, None, dialect, prefixes)
        }
        SetExpr::Query(q) => collect_identifier_prefixes_from_query(q, dialect, prefixes),
        SetExpr::SetOperation { left, right, .. } => {
            collect_identifier_prefixes_from_set_expr(left, dialect, prefixes);
            collect_identifier_prefixes_from_set_expr(right, dialect, prefixes);
        }
        _ => {}
    }
}

fn collect_identifier_prefixes_from_select(
    select: &Select,
    order_by: Option<&OrderBy>,
    dialect: Dialect,
    prefixes: &mut HashSet<QualifierRef>,
) {
    for item in &select.projection {
        collect_identifier_prefixes_from_select_item(item, dialect, prefixes);
    }
    if let Some(ref prewhere) = select.prewhere {
        collect_identifier_prefixes(prewhere, dialect, prefixes);
    }
    if let Some(ref selection) = select.selection {
        collect_identifier_prefixes(selection, dialect, prefixes);
    }
    if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            collect_identifier_prefixes(expr, dialect, prefixes);
        }
    }
    for expr in &select.cluster_by {
        collect_identifier_prefixes(expr, dialect, prefixes);
    }
    for expr in &select.distribute_by {
        collect_identifier_prefixes(expr, dialect, prefixes);
    }
    for sort_expr in &select.sort_by {
        collect_identifier_prefixes(&sort_expr.expr, dialect, prefixes);
    }
    if let Some(ref having) = select.having {
        collect_identifier_prefixes(having, dialect, prefixes);
    }
    if let Some(ref qualify) = select.qualify {
        if include_qualify_alias_references(dialect, select) {
            collect_identifier_prefixes(qualify, dialect, prefixes);
        }
    }
    if let Some(Distinct::On(exprs)) = &select.distinct {
        for expr in exprs {
            collect_identifier_prefixes(expr, dialect, prefixes);
        }
    }
    for named_window in &select.named_window {
        if let NamedWindowExpr::WindowSpec(spec) = &named_window.1 {
            for expr in &spec.partition_by {
                collect_identifier_prefixes(expr, dialect, prefixes);
            }
            for order_expr in &spec.order_by {
                collect_identifier_prefixes(&order_expr.expr, dialect, prefixes);
            }
        }
    }
    for lateral_view in &select.lateral_views {
        collect_identifier_prefixes(&lateral_view.lateral_view, dialect, prefixes);
    }
    if let Some(connect_by) = &select.connect_by {
        collect_identifier_prefixes(&connect_by.condition, dialect, prefixes);
        for relationship in &connect_by.relationships {
            collect_identifier_prefixes(relationship, dialect, prefixes);
        }
    }
    for from_item in &select.from {
        collect_identifier_prefixes_from_table_factor(&from_item.relation, dialect, prefixes);
        for join in &from_item.joins {
            collect_identifier_prefixes_from_table_factor(&join.relation, dialect, prefixes);
            if let Some(constraint) = join_constraint(&join.join_operator) {
                collect_identifier_prefixes(constraint, dialect, prefixes);
            }
        }
    }
    if let Some(order_by) = order_by {
        collect_identifier_prefixes_from_order_by(order_by, dialect, prefixes);
    }
}

fn collect_aliases(relation: &TableFactor, dialect: Dialect, aliases: &mut HashMap<String, AliasRef>) {
    match relation {
        TableFactor::Table {
            name,
            alias: Some(alias),
            args,
            ..
        } => {
            if args.is_some() {
                return;
            }
            if is_implicit_array_relation_alias(dialect, name, aliases) {
                return;
            }
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
        TableFactor::Derived { alias: Some(_), .. } => {}
        TableFactor::Function {
            lateral: true,
            alias: Some(alias),
            ..
        } => {
            aliases.insert(
                alias.name.value.clone(),
                AliasRef {
                    name: alias.name.value.clone(),
                    quoted: alias.name.quote_style.is_some(),
                },
            );
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_aliases(&table_with_joins.relation, dialect, aliases);
            for join in &table_with_joins.joins {
                collect_aliases(&join.relation, dialect, aliases);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => collect_aliases(table, dialect, aliases),
        _ => {}
    }
}

fn collect_identifier_prefixes_from_select_item(
    item: &SelectItem,
    dialect: Dialect,
    prefixes: &mut HashSet<QualifierRef>,
) {
    match item {
        SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
            collect_identifier_prefixes(expr, dialect, prefixes);
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

fn collect_identifier_prefixes(
    expr: &Expr,
    dialect: Dialect,
    prefixes: &mut HashSet<QualifierRef>,
) {
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
            collect_identifier_prefixes(left, dialect, prefixes);
            collect_identifier_prefixes(right, dialect, prefixes);
        }
        Expr::UnaryOp { expr: inner, .. } => collect_identifier_prefixes(inner, dialect, prefixes),
        Expr::Nested(inner) => collect_identifier_prefixes(inner, dialect, prefixes),
        Expr::Function(func) => {
            let function_name = function_name(func);
            if let FunctionArguments::List(arg_list) = &func.args {
                for (index, arg) in arg_list.args.iter().enumerate() {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(e),
                            ..
                        } => {
                            collect_identifier_prefixes(e, dialect, prefixes);
                            if function_arg_is_table_alias_reference(
                                dialect,
                                function_name.as_str(),
                                index,
                            ) {
                                if let Expr::Identifier(ident) = e {
                                    prefixes.insert(QualifierRef {
                                        name: ident.value.clone(),
                                        quoted: ident.quote_style.is_some(),
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            if let Some(filter) = &func.filter {
                collect_identifier_prefixes(filter, dialect, prefixes);
            }
            for order_expr in &func.within_group {
                collect_identifier_prefixes(&order_expr.expr, dialect, prefixes);
            }
            if let Some(WindowType::WindowSpec(spec)) = &func.over {
                for expr in &spec.partition_by {
                    collect_identifier_prefixes(expr, dialect, prefixes);
                }
                for order_expr in &spec.order_by {
                    collect_identifier_prefixes(&order_expr.expr, dialect, prefixes);
                }
            }
        }
        Expr::IsNull(inner) | Expr::IsNotNull(inner) | Expr::Cast { expr: inner, .. } => {
            collect_identifier_prefixes(inner, dialect, prefixes);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                collect_identifier_prefixes(op, dialect, prefixes);
            }
            for case_when in conditions {
                collect_identifier_prefixes(&case_when.condition, dialect, prefixes);
                collect_identifier_prefixes(&case_when.result, dialect, prefixes);
            }
            if let Some(el) = else_result {
                collect_identifier_prefixes(el, dialect, prefixes);
            }
        }
        Expr::InList { expr, list, .. } => {
            collect_identifier_prefixes(expr, dialect, prefixes);
            for item in list {
                collect_identifier_prefixes(item, dialect, prefixes);
            }
        }
        Expr::InSubquery { expr, subquery, .. } => {
            collect_identifier_prefixes(expr, dialect, prefixes);
            collect_identifier_prefixes_from_query(subquery, dialect, prefixes);
        }
        Expr::AnyOp { left, right, .. } | Expr::AllOp { left, right, .. } => {
            collect_identifier_prefixes(left, dialect, prefixes);
            collect_identifier_prefixes(right, dialect, prefixes);
        }
        Expr::Subquery(subquery) | Expr::Exists { subquery, .. } => {
            collect_identifier_prefixes_from_query(subquery, dialect, prefixes);
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            collect_identifier_prefixes(expr, dialect, prefixes);
            collect_identifier_prefixes(low, dialect, prefixes);
            collect_identifier_prefixes(high, dialect, prefixes);
        }
        _ => {}
    }
}

fn collect_identifier_prefixes_from_table_factor(
    table_factor: &TableFactor,
    dialect: Dialect,
    prefixes: &mut HashSet<QualifierRef>,
) {
    match table_factor {
        TableFactor::Table { name, .. } => {
            if let Some(prefix) = implicit_array_relation_prefix(dialect, name) {
                prefixes.insert(prefix);
            }
        }
        TableFactor::Derived {
            lateral: true,
            subquery,
            ..
        } => collect_identifier_prefixes_from_query(subquery, dialect, prefixes),
        TableFactor::TableFunction { expr, .. } => {
            collect_identifier_prefixes(expr, dialect, prefixes);
        }
        TableFactor::Function { args, .. } => {
            for arg in args {
                collect_identifier_prefixes_from_function_arg(arg, dialect, prefixes);
            }
        }
        TableFactor::UNNEST { array_exprs, .. } => {
            for expr in array_exprs {
                collect_identifier_prefixes(expr, dialect, prefixes);
            }
        }
        TableFactor::JsonTable { json_expr, .. } | TableFactor::OpenJsonTable { json_expr, .. } => {
            collect_identifier_prefixes(json_expr, dialect, prefixes);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_identifier_prefixes_from_table_factor(
                &table_with_joins.relation,
                dialect,
                prefixes,
            );
            for join in &table_with_joins.joins {
                collect_identifier_prefixes_from_table_factor(&join.relation, dialect, prefixes);
                if let Some(constraint) = join_constraint(&join.join_operator) {
                    collect_identifier_prefixes(constraint, dialect, prefixes);
                }
            }
        }
        TableFactor::Pivot {
            table,
            aggregate_functions,
            value_column,
            default_on_null,
            ..
        } => {
            collect_identifier_prefixes_from_table_factor(table, dialect, prefixes);
            for expr_with_alias in aggregate_functions {
                collect_identifier_prefixes(&expr_with_alias.expr, dialect, prefixes);
            }
            for expr in value_column {
                collect_identifier_prefixes(expr, dialect, prefixes);
            }
            if let Some(expr) = default_on_null {
                collect_identifier_prefixes(expr, dialect, prefixes);
            }
        }
        TableFactor::Unpivot {
            table,
            value,
            columns,
            ..
        } => {
            collect_identifier_prefixes_from_table_factor(table, dialect, prefixes);
            collect_identifier_prefixes(value, dialect, prefixes);
            for expr_with_alias in columns {
                collect_identifier_prefixes(&expr_with_alias.expr, dialect, prefixes);
            }
        }
        TableFactor::MatchRecognize {
            table,
            partition_by,
            order_by,
            measures,
            ..
        } => {
            collect_identifier_prefixes_from_table_factor(table, dialect, prefixes);
            for expr in partition_by {
                collect_identifier_prefixes(expr, dialect, prefixes);
            }
            for order in order_by {
                collect_identifier_prefixes(&order.expr, dialect, prefixes);
            }
            for measure in measures {
                collect_identifier_prefixes(&measure.expr, dialect, prefixes);
            }
        }
        TableFactor::XmlTable { row_expression, .. } => {
            collect_identifier_prefixes(row_expression, dialect, prefixes);
        }
        _ => {}
    }
}

fn collect_identifier_prefixes_from_function_arg(
    arg: &FunctionArg,
    dialect: Dialect,
    prefixes: &mut HashSet<QualifierRef>,
) {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
        | FunctionArg::Named {
            arg: FunctionArgExpr::Expr(expr),
            ..
        } => collect_identifier_prefixes(expr, dialect, prefixes),
        _ => {}
    }
}

fn function_name(function: &Function) -> String {
    function
        .name
        .0
        .last()
        .and_then(ObjectNamePart::as_ident)
        .map(|ident| ident.value.to_ascii_uppercase())
        .unwrap_or_default()
}

fn function_arg_is_table_alias_reference(
    dialect: Dialect,
    function_name: &str,
    arg_index: usize,
) -> bool {
    matches!(dialect, Dialect::Bigquery) && arg_index == 0 && function_name == "TO_JSON_STRING"
}

fn include_qualify_alias_references(dialect: Dialect, select: &Select) -> bool {
    // SQLFluff AL05 Redshift parity: QUALIFY references only count for alias usage
    // when QUALIFY immediately follows the FROM/JOIN section (no WHERE clause).
    !matches!(dialect, Dialect::Redshift) || select.selection.is_none()
}

fn implicit_array_relation_prefix(dialect: Dialect, name: &ObjectName) -> Option<QualifierRef> {
    if !matches!(dialect, Dialect::Bigquery | Dialect::Redshift) {
        return None;
    }
    if name.0.len() != 2 {
        return None;
    }
    let first = name.0.first()?.as_ident()?;
    Some(QualifierRef {
        name: first.value.clone(),
        quoted: first.quote_style.is_some(),
    })
}

fn is_implicit_array_relation_alias(
    dialect: Dialect,
    name: &ObjectName,
    aliases: &HashMap<String, AliasRef>,
) -> bool {
    let Some(prefix) = implicit_array_relation_prefix(dialect, name) else {
        return false;
    };
    aliases
        .values()
        .any(|alias| alias.name.eq_ignore_ascii_case(&prefix.name))
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

fn check_table_factor_subqueries(
    relation: &TableFactor,
    alias_case_check: AliasCaseCheck,
    ctx: &LintContext,
    issues: &mut Vec<Issue>,
) {
    match relation {
        TableFactor::Derived { subquery, .. } => {
            check_query(subquery, alias_case_check, ctx, issues);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            check_table_factor_subqueries(
                &table_with_joins.relation,
                alias_case_check,
                ctx,
                issues,
            );
            for join in &table_with_joins.joins {
                check_table_factor_subqueries(&join.relation, alias_case_check, ctx, issues);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            check_table_factor_subqueries(table, alias_case_check, ctx, issues);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::linter::rule::with_active_dialect;
    use crate::parser::{parse_sql, parse_sql_with_dialect};
    use crate::types::Dialect;

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

    fn check_sql_in_dialect(sql: &str, dialect: Dialect) -> Vec<Issue> {
        let stmts = parse_sql_with_dialect(sql, dialect).unwrap();
        let rule = UnusedTableAlias::default();
        let ctx = LintContext {
            sql,
            statement_range: 0..sql.len(),
            statement_index: 0,
        };
        let mut issues = Vec::new();
        with_active_dialect(dialect, || {
            for stmt in &stmts {
                issues.extend(rule.check(stmt, &ctx));
            }
        });
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
    fn test_single_table_unused_alias_detected() {
        let issues = check_sql("SELECT * FROM users u");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("u"));
    }

    #[test]
    fn test_single_table_alias_used_ok() {
        let issues = check_sql("SELECT u.id FROM users u");
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
        assert!(issues.is_empty());
    }

    #[test]
    fn test_lateral_alias_is_ignored() {
        let issues = check_sql("SELECT u.id FROM users u JOIN LATERAL (SELECT 1) lx ON TRUE");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_only_in_lateral_subquery_relation() {
        let issues = check_sql(
            "SELECT 1 \
             FROM users u \
             JOIN LATERAL (SELECT u.id) lx ON TRUE",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_alias_used_only_in_unnest_join_relation() {
        let issues = check_sql(
            "SELECT 1 \
             FROM users u \
             LEFT JOIN UNNEST(u.tags) tag ON TRUE",
        );
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

    #[test]
    fn flags_inner_subquery_unused_alias() {
        let issues = check_sql("SELECT * FROM (SELECT * FROM my_tbl AS foo)");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("foo"));
    }

    #[test]
    fn allows_unreferenced_subquery_alias() {
        let issues = check_sql("SELECT * FROM (SELECT 1 AS a) subquery");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_postgres_generate_series_alias() {
        let issues = check_sql_in_dialect(
            "SELECT date_trunc('day', dd)::timestamp FROM generate_series('2022-02-01'::timestamp, NOW()::timestamp, '1 day'::interval) dd",
            Dialect::Postgres,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_unused_snowflake_lateral_flatten_alias() {
        let issues = check_sql_in_dialect(
            "SELECT a.test1, a.test2, b.test3 \
             FROM table1 AS a, \
             LATERAL flatten(input => some_field) AS b, \
             LATERAL flatten(input => b.value) AS c, \
             LATERAL flatten(input => c.value) AS d, \
             LATERAL flatten(input => d.value) AS e, \
             LATERAL flatten(input => e.value) AS f",
            Dialect::Snowflake,
        );
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("f"));
    }

    #[test]
    fn allows_bigquery_to_json_string_table_alias_argument() {
        let issues = check_sql_in_dialect(
            "SELECT TO_JSON_STRING(t) FROM my_table AS t",
            Dialect::Bigquery,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_ansi_to_json_string_table_alias_argument() {
        let issues =
            check_sql_in_dialect("SELECT TO_JSON_STRING(t) FROM my_table AS t", Dialect::Ansi);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("t"));
    }

    #[test]
    fn redshift_qualify_after_from_counts_alias_usage() {
        let issues = check_sql_in_dialect(
            "SELECT * \
             FROM store AS s \
             INNER JOIN store_sales AS ss \
             QUALIFY ROW_NUMBER() OVER (PARTITION BY ss.sold_date ORDER BY ss.sales_price DESC) <= 2",
            Dialect::Redshift,
        );
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("s"));
    }

    #[test]
    fn redshift_qualify_after_where_does_not_count_alias_usage() {
        let issues = check_sql_in_dialect(
            "SELECT * \
             FROM store AS s \
             INNER JOIN store_sales AS ss \
             WHERE col = 1 \
             QUALIFY ROW_NUMBER() OVER (PARTITION BY ss.sold_date ORDER BY ss.sales_price DESC) <= 2",
            Dialect::Redshift,
        );
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().any(|issue| issue.message.contains("s")));
        assert!(issues.iter().any(|issue| issue.message.contains("ss")));
    }

    #[test]
    fn allows_bigquery_implicit_array_table_reference() {
        let issues = check_sql_in_dialect(
            "WITH table_arr AS (SELECT [1,2,4,2] AS arr) \
             SELECT arr \
             FROM table_arr AS t, t.arr",
            Dialect::Bigquery,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_redshift_super_array_relation_reference() {
        let issues = check_sql_in_dialect(
            "SELECT my_column, my_array_value \
             FROM my_schema.my_table AS t, t.super_array AS my_array_value",
            Dialect::Redshift,
        );
        assert!(issues.is_empty());
    }
}
