//! LINT_RF_002: References qualification.
//!
//! In multi-table queries, require qualified column references.

use std::collections::HashSet;

use regex::Regex;

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::parser::parse_sql_with_dialect;
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::{
    Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, Ident, ObjectNamePart,
    OrderByKind, Query, Select, SelectItem, SelectItemQualifiedWildcardKind, SetExpr, Statement,
    TableFactor, TableWithJoins,
};

use super::semantic_helpers::{
    join_on_expr, select_projection_alias_set, visit_select_expressions,
};

pub struct ReferencesQualification {
    force_enable: bool,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
    subqueries_ignore_external_references: bool,
}

impl ReferencesQualification {
    pub fn from_config(config: &LintConfig) -> Self {
        let mut ignore_words: HashSet<String> = HashSet::new();
        if let Some(values) =
            config.rule_option_string_list(issue_codes::LINT_RF_002, "ignore_words")
        {
            for value in values {
                ignore_words.extend(split_ignore_words_csv(&value));
            }
        }

        if let Some(csv) = config.rule_option_str(issue_codes::LINT_RF_002, "ignore_words") {
            ignore_words.extend(split_ignore_words_csv(csv));
        }

        let ignore_words_regex = config
            .rule_option_str(issue_codes::LINT_RF_002, "ignore_words_regex")
            .and_then(|pattern| Regex::new(pattern).ok());

        Self {
            force_enable: config
                .rule_option_bool(issue_codes::LINT_RF_002, "force_enable")
                .unwrap_or(true),
            ignore_words,
            ignore_words_regex,
            subqueries_ignore_external_references: config
                .rule_option_bool(
                    issue_codes::LINT_RF_002,
                    "subqueries_ignore_external_references",
                )
                .unwrap_or(false),
        }
    }
}

impl Default for ReferencesQualification {
    fn default() -> Self {
        Self {
            force_enable: true,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
            subqueries_ignore_external_references: false,
        }
    }
}

impl LintRule for ReferencesQualification {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_002
    }

    fn name(&self) -> &'static str {
        "References qualification"
    }

    fn description(&self) -> &'static str {
        "Use qualification consistently in multi-table queries."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if !self.force_enable {
            return Vec::new();
        }

        let declared_variables =
            declared_variables_before_statement(ctx.sql, ctx.dialect(), ctx.statement_index);
        let context = QualificationContext {
            dialect: ctx.dialect(),
            ignore_words: &self.ignore_words,
            ignore_words_regex: self.ignore_words_regex.as_ref(),
            declared_variables: &declared_variables,
            subqueries_ignore_external_references: self.subqueries_ignore_external_references,
        };

        let unqualified_count = violations_in_statement(statement, &context, 0);

        (0..unqualified_count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_RF_002,
                    "Use qualified references in multi-table queries.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

struct QualificationContext<'a> {
    dialect: Dialect,
    ignore_words: &'a HashSet<String>,
    ignore_words_regex: Option<&'a Regex>,
    declared_variables: &'a HashSet<String>,
    subqueries_ignore_external_references: bool,
}

#[derive(Default)]
struct SourceInfo {
    normal_source_count: usize,
    value_table_aliases: HashSet<String>,
}

fn split_ignore_words_csv(raw: &str) -> impl Iterator<Item = String> + '_ {
    raw.split(',')
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .map(|word| word.to_ascii_uppercase())
}

fn declared_variables_before_statement(
    sql: &str,
    dialect: Dialect,
    statement_index: usize,
) -> HashSet<String> {
    if !matches!(dialect, Dialect::Bigquery) {
        return HashSet::new();
    }

    let Ok(statements) = parse_sql_with_dialect(sql, dialect) else {
        return HashSet::new();
    };

    let mut names = HashSet::new();
    for statement in statements.into_iter().take(statement_index) {
        if let Statement::Declare { stmts } = statement {
            for declare in stmts {
                for name in declare.names {
                    names.insert(name.value.to_ascii_uppercase());
                }
            }
        }
    }

    names
}

fn violations_in_statement(
    statement: &Statement,
    ctx: &QualificationContext,
    external_sources: usize,
) -> usize {
    match statement {
        Statement::Query(query) => violations_in_query(query, ctx, external_sources),
        Statement::Insert(insert) => insert
            .source
            .as_ref()
            .map_or(0, |query| violations_in_query(query, ctx, external_sources)),
        Statement::CreateView { query, .. } => violations_in_query(query, ctx, external_sources),
        Statement::CreateTable(create) => create
            .query
            .as_ref()
            .map_or(0, |query| violations_in_query(query, ctx, external_sources)),
        _ => 0,
    }
}

fn violations_in_query(
    query: &Query,
    ctx: &QualificationContext,
    external_sources: usize,
) -> usize {
    let mut count = 0usize;

    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            // CTE bodies do not inherit parent query scope.
            count += violations_in_query(&cte.query, ctx, 0);
        }
    }

    count += violations_in_set_expr(&query.body, ctx, external_sources);

    if let Some(order_by) = &query.order_by {
        if let OrderByKind::Expressions(order_exprs) = &order_by.kind {
            let child_external = child_external_sources(external_sources, ctx);
            for order_expr in order_exprs {
                count += nested_subquery_violations_in_expr(&order_expr.expr, ctx, child_external);
            }
        }
    }

    count
}

fn violations_in_set_expr(
    set_expr: &SetExpr,
    ctx: &QualificationContext,
    external_sources: usize,
) -> usize {
    match set_expr {
        SetExpr::Select(select) => violations_in_select(select, ctx, external_sources),
        SetExpr::Query(query) => violations_in_query(query, ctx, external_sources),
        SetExpr::SetOperation { left, right, .. } => {
            violations_in_set_expr(left, ctx, external_sources)
                + violations_in_set_expr(right, ctx, external_sources)
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => violations_in_statement(statement, ctx, external_sources),
        _ => 0,
    }
}

fn violations_in_select(
    select: &Select,
    ctx: &QualificationContext,
    external_sources: usize,
) -> usize {
    let source_info = collect_source_info(select, ctx.dialect);

    let effective_external = if ctx.subqueries_ignore_external_references {
        0
    } else {
        external_sources
    };
    let effective_source_count = source_info.normal_source_count + effective_external;

    let aliases = select_projection_alias_set(select);

    let mut count = 0usize;
    if effective_source_count > 1 {
        count += unqualified_references_in_select_scope(
            select,
            &aliases,
            &source_info.value_table_aliases,
            ctx,
        );
    }

    let child_external = child_external_sources(effective_source_count, ctx);
    count += nested_subquery_violations_in_select(select, ctx, child_external);

    count
}

fn child_external_sources(current_effective_sources: usize, ctx: &QualificationContext) -> usize {
    if ctx.subqueries_ignore_external_references {
        0
    } else {
        current_effective_sources
    }
}

fn collect_source_info(select: &Select, dialect: Dialect) -> SourceInfo {
    let mut info = SourceInfo::default();

    for from_item in &select.from {
        collect_source_info_from_table_factor(&from_item.relation, dialect, &mut info);
        for join in &from_item.joins {
            collect_source_info_from_table_factor(&join.relation, dialect, &mut info);
        }
    }

    info
}

fn collect_source_info_from_table_factor(
    table_factor: &TableFactor,
    dialect: Dialect,
    info: &mut SourceInfo,
) {
    if is_value_table_function(table_factor, dialect) {
        if let Some(alias) = table_factor_value_table_alias(table_factor) {
            info.value_table_aliases.insert(alias);
        }
        if let TableFactor::UNNEST {
            with_offset_alias: Some(alias),
            ..
        } = table_factor
        {
            info.value_table_aliases
                .insert(alias.value.to_ascii_uppercase());
        }
        return;
    }

    match table_factor {
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_source_info_from_table_factor(&table_with_joins.relation, dialect, info);
            for join in &table_with_joins.joins {
                collect_source_info_from_table_factor(&join.relation, dialect, info);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_source_info_from_table_factor(table, dialect, info);
        }
        _ => {
            info.normal_source_count += 1;
        }
    }
}

fn table_factor_value_table_alias(table_factor: &TableFactor) -> Option<String> {
    let alias = match table_factor {
        TableFactor::Table { alias, .. }
        | TableFactor::Derived { alias, .. }
        | TableFactor::TableFunction { alias, .. }
        | TableFactor::Function { alias, .. }
        | TableFactor::UNNEST { alias, .. }
        | TableFactor::JsonTable { alias, .. }
        | TableFactor::OpenJsonTable { alias, .. }
        | TableFactor::NestedJoin { alias, .. }
        | TableFactor::Pivot { alias, .. }
        | TableFactor::Unpivot { alias, .. }
        | TableFactor::MatchRecognize { alias, .. }
        | TableFactor::XmlTable { alias, .. }
        | TableFactor::SemanticView { alias, .. } => alias.as_ref(),
    }?;

    Some(alias.name.value.to_ascii_uppercase())
}

fn is_value_table_function(table_factor: &TableFactor, dialect: Dialect) -> bool {
    matches!(dialect, Dialect::Bigquery) && matches!(table_factor, TableFactor::UNNEST { .. })
}

fn unqualified_references_in_select_scope(
    select: &Select,
    aliases: &HashSet<String>,
    value_table_aliases: &HashSet<String>,
    ctx: &QualificationContext,
) -> usize {
    let projection_unqualified_full =
        projection_unqualified_count_with_aliases(select, aliases, value_table_aliases, ctx);
    let projection_unqualified_sequential =
        projection_unqualified_count_sequential(select, value_table_aliases, ctx);

    let mut unqualified_in_select = 0usize;
    visit_select_expressions(select, &mut |expr| {
        unqualified_in_select += count_unqualified_references_in_expr_no_subqueries(
            expr,
            aliases,
            value_table_aliases,
            ctx,
            &HashSet::new(),
        );
    });

    unqualified_in_select.saturating_sub(projection_unqualified_full)
        + projection_unqualified_sequential
}

fn projection_unqualified_count_with_aliases(
    select: &Select,
    aliases: &HashSet<String>,
    value_table_aliases: &HashSet<String>,
    ctx: &QualificationContext,
) -> usize {
    select
        .projection
        .iter()
        .map(|item| match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                count_unqualified_references_in_expr_no_subqueries(
                    expr,
                    aliases,
                    value_table_aliases,
                    ctx,
                    &HashSet::new(),
                )
            }
            SelectItem::QualifiedWildcard(kind, _) => match kind {
                SelectItemQualifiedWildcardKind::Expr(expr) => {
                    count_unqualified_references_in_expr_no_subqueries(
                        expr,
                        aliases,
                        value_table_aliases,
                        ctx,
                        &HashSet::new(),
                    )
                }
                _ => 0,
            },
            _ => 0,
        })
        .sum()
}

fn projection_unqualified_count_sequential(
    select: &Select,
    value_table_aliases: &HashSet<String>,
    ctx: &QualificationContext,
) -> usize {
    let mut aliases_before = HashSet::new();
    let mut unqualified = 0usize;

    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) => {
                unqualified += count_unqualified_references_in_expr_no_subqueries(
                    expr,
                    &aliases_before,
                    value_table_aliases,
                    ctx,
                    &HashSet::new(),
                );
            }
            SelectItem::ExprWithAlias { expr, alias } => {
                unqualified += count_unqualified_references_in_expr_no_subqueries(
                    expr,
                    &aliases_before,
                    value_table_aliases,
                    ctx,
                    &HashSet::new(),
                );
                aliases_before.insert(alias.value.to_ascii_uppercase());
            }
            _ => {}
        }
    }

    unqualified
}

fn count_unqualified_references_in_expr_no_subqueries(
    expr: &Expr,
    aliases: &HashSet<String>,
    value_table_aliases: &HashSet<String>,
    ctx: &QualificationContext,
    lambda_params: &HashSet<String>,
) -> usize {
    match expr {
        Expr::Identifier(identifier) => identifier_is_unqualified_reference(
            identifier,
            aliases,
            value_table_aliases,
            ctx,
            lambda_params,
        )
        .into(),
        Expr::CompoundIdentifier(_) => 0,
        Expr::CompoundFieldAccess { root, .. } => {
            count_unqualified_references_in_expr_no_subqueries(
                root,
                aliases,
                value_table_aliases,
                ctx,
                lambda_params,
            )
        }
        Expr::JsonAccess { value, .. } => count_unqualified_references_in_expr_no_subqueries(
            value,
            aliases,
            value_table_aliases,
            ctx,
            lambda_params,
        ),
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. }
        | Expr::IsDistinctFrom(left, right)
        | Expr::IsNotDistinctFrom(left, right) => {
            count_unqualified_references_in_expr_no_subqueries(
                left,
                aliases,
                value_table_aliases,
                ctx,
                lambda_params,
            ) + count_unqualified_references_in_expr_no_subqueries(
                right,
                aliases,
                value_table_aliases,
                ctx,
                lambda_params,
            )
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
        | Expr::IsNotUnknown(inner)
        | Expr::Cast { expr: inner, .. }
        | Expr::AtTimeZone {
            timestamp: inner, ..
        }
        | Expr::Extract { expr: inner, .. }
        | Expr::Ceil { expr: inner, .. }
        | Expr::Floor { expr: inner, .. }
        | Expr::Position { expr: inner, .. }
        | Expr::Substring { expr: inner, .. }
        | Expr::Trim { expr: inner, .. } => count_unqualified_references_in_expr_no_subqueries(
            inner,
            aliases,
            value_table_aliases,
            ctx,
            lambda_params,
        ),
        Expr::InList { expr, list, .. } => {
            count_unqualified_references_in_expr_no_subqueries(
                expr,
                aliases,
                value_table_aliases,
                ctx,
                lambda_params,
            ) + list
                .iter()
                .map(|item| {
                    count_unqualified_references_in_expr_no_subqueries(
                        item,
                        aliases,
                        value_table_aliases,
                        ctx,
                        lambda_params,
                    )
                })
                .sum::<usize>()
        }
        Expr::InSubquery { expr, .. } => count_unqualified_references_in_expr_no_subqueries(
            expr,
            aliases,
            value_table_aliases,
            ctx,
            lambda_params,
        ),
        Expr::InUnnest {
            expr, array_expr, ..
        } => {
            count_unqualified_references_in_expr_no_subqueries(
                expr,
                aliases,
                value_table_aliases,
                ctx,
                lambda_params,
            ) + count_unqualified_references_in_expr_no_subqueries(
                array_expr,
                aliases,
                value_table_aliases,
                ctx,
                lambda_params,
            )
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            count_unqualified_references_in_expr_no_subqueries(
                expr,
                aliases,
                value_table_aliases,
                ctx,
                lambda_params,
            ) + count_unqualified_references_in_expr_no_subqueries(
                low,
                aliases,
                value_table_aliases,
                ctx,
                lambda_params,
            ) + count_unqualified_references_in_expr_no_subqueries(
                high,
                aliases,
                value_table_aliases,
                ctx,
                lambda_params,
            )
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let mut count = 0usize;
            if let Some(operand) = operand {
                count += count_unqualified_references_in_expr_no_subqueries(
                    operand,
                    aliases,
                    value_table_aliases,
                    ctx,
                    lambda_params,
                );
            }
            for when in conditions {
                count += count_unqualified_references_in_expr_no_subqueries(
                    &when.condition,
                    aliases,
                    value_table_aliases,
                    ctx,
                    lambda_params,
                );
                count += count_unqualified_references_in_expr_no_subqueries(
                    &when.result,
                    aliases,
                    value_table_aliases,
                    ctx,
                    lambda_params,
                );
            }
            if let Some(otherwise) = else_result {
                count += count_unqualified_references_in_expr_no_subqueries(
                    otherwise,
                    aliases,
                    value_table_aliases,
                    ctx,
                    lambda_params,
                );
            }
            count
        }
        Expr::Function(function) => count_unqualified_references_in_function_no_subqueries(
            function,
            aliases,
            value_table_aliases,
            ctx,
            lambda_params,
        ),
        Expr::Lambda(lambda) => {
            let mut next_lambda_params = lambda_params.clone();
            for param in &lambda.params {
                next_lambda_params.insert(param.value.to_ascii_uppercase());
            }
            count_unqualified_references_in_expr_no_subqueries(
                &lambda.body,
                aliases,
                value_table_aliases,
                ctx,
                &next_lambda_params,
            )
        }
        Expr::Subquery(_) | Expr::Exists { .. } => 0,
        _ => 0,
    }
}

fn count_unqualified_references_in_function_no_subqueries(
    function: &Function,
    aliases: &HashSet<String>,
    value_table_aliases: &HashSet<String>,
    ctx: &QualificationContext,
    lambda_params: &HashSet<String>,
) -> usize {
    let mut count = 0usize;

    if let FunctionArguments::List(arguments) = &function.args {
        for (index, arg) in arguments.args.iter().enumerate() {
            count += match arg {
                FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                | FunctionArg::Named {
                    arg: FunctionArgExpr::Expr(expr),
                    ..
                }
                | FunctionArg::ExprNamed {
                    arg: FunctionArgExpr::Expr(expr),
                    ..
                } => {
                    if should_skip_identifier_reference_for_function_arg(function, index, expr) {
                        0
                    } else {
                        count_unqualified_references_in_expr_no_subqueries(
                            expr,
                            aliases,
                            value_table_aliases,
                            ctx,
                            lambda_params,
                        )
                    }
                }
                _ => 0,
            };
        }
    }

    if let Some(filter) = &function.filter {
        count += count_unqualified_references_in_expr_no_subqueries(
            filter,
            aliases,
            value_table_aliases,
            ctx,
            lambda_params,
        );
    }

    for order_expr in &function.within_group {
        count += count_unqualified_references_in_expr_no_subqueries(
            &order_expr.expr,
            aliases,
            value_table_aliases,
            ctx,
            lambda_params,
        );
    }

    if let Some(sqlparser::ast::WindowType::WindowSpec(spec)) = &function.over {
        for expr in &spec.partition_by {
            count += count_unqualified_references_in_expr_no_subqueries(
                expr,
                aliases,
                value_table_aliases,
                ctx,
                lambda_params,
            );
        }
        for order_expr in &spec.order_by {
            count += count_unqualified_references_in_expr_no_subqueries(
                &order_expr.expr,
                aliases,
                value_table_aliases,
                ctx,
                lambda_params,
            );
        }
    }

    count
}

fn nested_subquery_violations_in_select(
    select: &Select,
    ctx: &QualificationContext,
    child_external_sources: usize,
) -> usize {
    let mut count = 0usize;

    visit_select_expressions(select, &mut |expr| {
        count += nested_subquery_violations_in_expr(expr, ctx, child_external_sources);
    });

    for from_item in &select.from {
        count += nested_subquery_violations_in_table_factor(
            &from_item.relation,
            ctx,
            child_external_sources,
        );
        for join in &from_item.joins {
            count += nested_subquery_violations_in_table_factor(
                &join.relation,
                ctx,
                child_external_sources,
            );
            if let Some(on_expr) = join_on_expr(&join.join_operator) {
                count += nested_subquery_violations_in_expr(on_expr, ctx, child_external_sources);
            }
        }
    }

    count
}

fn nested_subquery_violations_in_expr(
    expr: &Expr,
    ctx: &QualificationContext,
    child_external_sources: usize,
) -> usize {
    match expr {
        Expr::InSubquery { expr, subquery, .. } => {
            nested_subquery_violations_in_expr(expr, ctx, child_external_sources)
                + violations_in_query(subquery, ctx, child_external_sources)
        }
        Expr::Subquery(subquery) | Expr::Exists { subquery, .. } => {
            violations_in_query(subquery, ctx, child_external_sources)
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. }
        | Expr::IsDistinctFrom(left, right)
        | Expr::IsNotDistinctFrom(left, right) => {
            nested_subquery_violations_in_expr(left, ctx, child_external_sources)
                + nested_subquery_violations_in_expr(right, ctx, child_external_sources)
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
        | Expr::IsNotUnknown(inner)
        | Expr::Cast { expr: inner, .. }
        | Expr::AtTimeZone {
            timestamp: inner, ..
        }
        | Expr::Extract { expr: inner, .. }
        | Expr::Ceil { expr: inner, .. }
        | Expr::Floor { expr: inner, .. }
        | Expr::Position { expr: inner, .. }
        | Expr::Substring { expr: inner, .. }
        | Expr::Trim { expr: inner, .. }
        | Expr::JsonAccess { value: inner, .. }
        | Expr::CompoundFieldAccess { root: inner, .. } => {
            nested_subquery_violations_in_expr(inner, ctx, child_external_sources)
        }
        Expr::InList { expr, list, .. } => {
            nested_subquery_violations_in_expr(expr, ctx, child_external_sources)
                + list
                    .iter()
                    .map(|item| {
                        nested_subquery_violations_in_expr(item, ctx, child_external_sources)
                    })
                    .sum::<usize>()
        }
        Expr::InUnnest {
            expr, array_expr, ..
        } => {
            nested_subquery_violations_in_expr(expr, ctx, child_external_sources)
                + nested_subquery_violations_in_expr(array_expr, ctx, child_external_sources)
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            nested_subquery_violations_in_expr(expr, ctx, child_external_sources)
                + nested_subquery_violations_in_expr(low, ctx, child_external_sources)
                + nested_subquery_violations_in_expr(high, ctx, child_external_sources)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let mut count = operand.as_ref().map_or(0, |expr| {
                nested_subquery_violations_in_expr(expr, ctx, child_external_sources)
            });
            for when in conditions {
                count += nested_subquery_violations_in_expr(
                    &when.condition,
                    ctx,
                    child_external_sources,
                );
                count +=
                    nested_subquery_violations_in_expr(&when.result, ctx, child_external_sources);
            }
            if let Some(otherwise) = else_result {
                count += nested_subquery_violations_in_expr(otherwise, ctx, child_external_sources);
            }
            count
        }
        Expr::Function(function) => {
            let mut count = 0usize;
            if let FunctionArguments::List(arguments) = &function.args {
                for arg in &arguments.args {
                    count += match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(expr),
                            ..
                        }
                        | FunctionArg::ExprNamed {
                            arg: FunctionArgExpr::Expr(expr),
                            ..
                        } => nested_subquery_violations_in_expr(expr, ctx, child_external_sources),
                        _ => 0,
                    };
                }
            }
            if let Some(filter) = &function.filter {
                count += nested_subquery_violations_in_expr(filter, ctx, child_external_sources);
            }
            for order_expr in &function.within_group {
                count += nested_subquery_violations_in_expr(
                    &order_expr.expr,
                    ctx,
                    child_external_sources,
                );
            }
            if let Some(sqlparser::ast::WindowType::WindowSpec(spec)) = &function.over {
                for expr in &spec.partition_by {
                    count += nested_subquery_violations_in_expr(expr, ctx, child_external_sources);
                }
                for order_expr in &spec.order_by {
                    count += nested_subquery_violations_in_expr(
                        &order_expr.expr,
                        ctx,
                        child_external_sources,
                    );
                }
            }
            count
        }
        Expr::Lambda(lambda) => {
            nested_subquery_violations_in_expr(&lambda.body, ctx, child_external_sources)
        }
        _ => 0,
    }
}

fn nested_subquery_violations_in_table_factor(
    table_factor: &TableFactor,
    ctx: &QualificationContext,
    child_external_sources: usize,
) -> usize {
    match table_factor {
        TableFactor::Derived {
            lateral, subquery, ..
        } => {
            let external = if *lateral { child_external_sources } else { 0 };
            violations_in_query(subquery, ctx, external)
        }
        TableFactor::TableFunction { expr, .. } => {
            nested_subquery_violations_in_expr(expr, ctx, child_external_sources)
        }
        TableFactor::Function { args, .. } => args
            .iter()
            .map(|arg| match arg {
                FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                | FunctionArg::Named {
                    arg: FunctionArgExpr::Expr(expr),
                    ..
                }
                | FunctionArg::ExprNamed {
                    arg: FunctionArgExpr::Expr(expr),
                    ..
                } => nested_subquery_violations_in_expr(expr, ctx, child_external_sources),
                _ => 0,
            })
            .sum(),
        TableFactor::UNNEST { array_exprs, .. } => array_exprs
            .iter()
            .map(|expr| nested_subquery_violations_in_expr(expr, ctx, child_external_sources))
            .sum(),
        TableFactor::JsonTable { json_expr, .. } | TableFactor::OpenJsonTable { json_expr, .. } => {
            nested_subquery_violations_in_expr(json_expr, ctx, child_external_sources)
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => nested_subquery_violations_in_table_with_joins(
            table_with_joins,
            ctx,
            child_external_sources,
        ),
        TableFactor::Pivot {
            table,
            aggregate_functions,
            value_column,
            default_on_null,
            ..
        } => {
            let mut count =
                nested_subquery_violations_in_table_factor(table, ctx, child_external_sources);
            for expr_with_alias in aggregate_functions {
                count += nested_subquery_violations_in_expr(
                    &expr_with_alias.expr,
                    ctx,
                    child_external_sources,
                );
            }
            for expr in value_column {
                count += nested_subquery_violations_in_expr(expr, ctx, child_external_sources);
            }
            if let Some(expr) = default_on_null {
                count += nested_subquery_violations_in_expr(expr, ctx, child_external_sources);
            }
            count
        }
        TableFactor::Unpivot {
            table,
            value,
            columns,
            ..
        } => {
            let mut count =
                nested_subquery_violations_in_table_factor(table, ctx, child_external_sources);
            count += nested_subquery_violations_in_expr(value, ctx, child_external_sources);
            for expr_with_alias in columns {
                count += nested_subquery_violations_in_expr(
                    &expr_with_alias.expr,
                    ctx,
                    child_external_sources,
                );
            }
            count
        }
        TableFactor::MatchRecognize {
            table,
            partition_by,
            order_by,
            measures,
            symbols,
            ..
        } => {
            let mut count =
                nested_subquery_violations_in_table_factor(table, ctx, child_external_sources);
            for expr in partition_by {
                count += nested_subquery_violations_in_expr(expr, ctx, child_external_sources);
            }
            for order in order_by {
                count +=
                    nested_subquery_violations_in_expr(&order.expr, ctx, child_external_sources);
            }
            for measure in measures {
                count +=
                    nested_subquery_violations_in_expr(&measure.expr, ctx, child_external_sources);
            }
            for symbol in symbols {
                count += nested_subquery_violations_in_expr(
                    &symbol.definition,
                    ctx,
                    child_external_sources,
                );
            }
            count
        }
        TableFactor::XmlTable { row_expression, .. } => {
            nested_subquery_violations_in_expr(row_expression, ctx, child_external_sources)
        }
        _ => 0,
    }
}

fn nested_subquery_violations_in_table_with_joins(
    table_with_joins: &TableWithJoins,
    ctx: &QualificationContext,
    child_external_sources: usize,
) -> usize {
    let mut count = nested_subquery_violations_in_table_factor(
        &table_with_joins.relation,
        ctx,
        child_external_sources,
    );

    for join in &table_with_joins.joins {
        count +=
            nested_subquery_violations_in_table_factor(&join.relation, ctx, child_external_sources);
        if let Some(on_expr) = join_on_expr(&join.join_operator) {
            count += nested_subquery_violations_in_expr(on_expr, ctx, child_external_sources);
        }
    }

    count
}

fn identifier_is_unqualified_reference(
    identifier: &Ident,
    aliases: &HashSet<String>,
    value_table_aliases: &HashSet<String>,
    ctx: &QualificationContext,
    lambda_params: &HashSet<String>,
) -> bool {
    let name = identifier.value.as_str();
    let normalized = name.to_ascii_uppercase();

    if aliases.contains(&normalized)
        || value_table_aliases.contains(&normalized)
        || lambda_params.contains(&normalized)
        || ctx.declared_variables.contains(&normalized)
    {
        return false;
    }

    if name.starts_with('@') {
        return false;
    }

    if ctx.ignore_words.contains(&normalized) {
        return false;
    }

    if let Some(regex) = ctx.ignore_words_regex {
        if regex.is_match(name) {
            return false;
        }
    }

    true
}

fn should_skip_identifier_reference_for_function_arg(
    function: &Function,
    arg_index: usize,
    expr: &Expr,
) -> bool {
    let Expr::Identifier(ident) = expr else {
        return false;
    };
    if ident.quote_style.is_some() || !is_date_part_identifier(&ident.value) {
        return false;
    }

    let Some(function_name) = function_name_upper(function) else {
        return false;
    };
    if !is_datepart_function_name(&function_name) {
        return false;
    }

    // Dialect-specific datepart-position differences exist; to avoid false
    // positives across common dialects we allow datepart keywords in the first
    // two argument positions of known datepart functions.
    arg_index <= 1
}

fn function_name_upper(function: &Function) -> Option<String> {
    function
        .name
        .0
        .last()
        .and_then(ObjectNamePart::as_ident)
        .map(|ident| ident.value.to_ascii_uppercase())
}

fn is_datepart_function_name(name: &str) -> bool {
    matches!(
        name,
        "DATEDIFF"
            | "DATE_DIFF"
            | "DATEADD"
            | "DATE_ADD"
            | "DATE_PART"
            | "DATETIME_TRUNC"
            | "TIME_TRUNC"
            | "TIMESTAMP_TRUNC"
            | "TIMESTAMP_DIFF"
            | "TIMESTAMPDIFF"
    )
}

fn is_date_part_identifier(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "YEAR"
            | "QUARTER"
            | "MONTH"
            | "WEEK"
            | "DAY"
            | "DOW"
            | "DOY"
            | "HOUR"
            | "MINUTE"
            | "SECOND"
            | "MILLISECOND"
            | "MICROSECOND"
            | "NANOSECOND"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::rule::with_active_dialect;
    use crate::parser::{parse_sql, parse_sql_with_dialect};

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesQualification::default();
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

    fn run_in_dialect(sql: &str, dialect: Dialect) -> Vec<Issue> {
        let statements = parse_sql_with_dialect(sql, dialect).expect("parse");
        let rule = ReferencesQualification::default();
        let mut issues = Vec::new();
        with_active_dialect(dialect, || {
            for (index, statement) in statements.iter().enumerate() {
                issues.extend(rule.check(
                    statement,
                    &LintContext {
                        sql,
                        statement_range: 0..sql.len(),
                        statement_index: index,
                    },
                ));
            }
        });
        issues
    }

    fn run_with_config(sql: &str, dialect: Dialect, config_json: serde_json::Value) -> Vec<Issue> {
        let statements = parse_sql_with_dialect(sql, dialect).expect("parse");
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.qualification".to_string(),
                config_json,
            )]),
        };
        let rule = ReferencesQualification::from_config(&config);
        let mut issues = Vec::new();
        with_active_dialect(dialect, || {
            for (index, statement) in statements.iter().enumerate() {
                issues.extend(rule.check(
                    statement,
                    &LintContext {
                        sql,
                        statement_range: 0..sql.len(),
                        statement_index: index,
                    },
                ));
            }
        });
        issues
    }

    // --- Edge cases adopted from sqlfluff RF02 ---

    #[test]
    fn allows_fully_qualified_multi_table_query() {
        let issues = run("SELECT foo.a, vee.b FROM foo LEFT JOIN vee ON vee.a = foo.a");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_unqualified_multi_table_query() {
        let issues = run("SELECT a, b FROM foo LEFT JOIN vee ON vee.a = foo.a");
        assert!(!issues.is_empty());
        assert!(issues
            .iter()
            .all(|issue| issue.code == issue_codes::LINT_RF_002));
    }

    #[test]
    fn allows_qualified_multi_table_query_inside_subquery() {
        let issues =
            run("SELECT a FROM (SELECT foo.a, vee.b FROM foo LEFT JOIN vee ON vee.a = foo.a)");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_unqualified_multi_table_query_inside_subquery() {
        let issues = run("SELECT a FROM (SELECT a, b FROM foo LEFT JOIN vee ON vee.a = foo.a)");
        assert!(!issues.is_empty());
    }

    #[test]
    fn force_enable_false_disables_rule() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_RF_002".to_string(),
                serde_json::json!({"force_enable": false}),
            )]),
        };
        let rule = ReferencesQualification::from_config(&config);
        let sql = "SELECT a, b FROM foo LEFT JOIN vee ON vee.a = foo.a";
        let statements = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_projection_self_alias_in_multi_source_query() {
        let issues = run("SELECT foo AS foo FROM a LEFT JOIN b ON a.id = b.id");
        assert!(!issues.is_empty());
        assert!(issues
            .iter()
            .all(|issue| issue.code == issue_codes::LINT_RF_002));
    }

    #[test]
    fn allows_later_projection_reference_to_previous_alias() {
        let issues = run("SELECT a.bar AS baz, baz FROM a LEFT JOIN b ON a.id = b.id");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_bigquery_value_table_function_in_source_count() {
        let sql = "select unqualified_reference_from_table_a, _t_start from a left join unnest(generate_timestamp_array('2020-01-01','2020-01-30', interval 1 day)) as _t_start on true";
        let issues = run_in_dialect(sql, Dialect::Bigquery);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn allows_bigquery_unqualified_value_table_alias_with_other_tables() {
        let sql = "select a.*, b.*, _t_start from a left join b on true left join unnest(generate_timestamp_array('2020-01-01','2020-01-30', interval 1 day)) as _t_start on true";
        let issues = run_in_dialect(sql, Dialect::Bigquery);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn allows_mysql_session_variable_reference() {
        let sql = "SET @someVar = 1; SELECT Table1.Col1, Table2.Col2 FROM Table1 LEFT JOIN Table2 ON Table1.Join1 = Table2.Join1 WHERE Table1.FilterCol = @someVar;";
        let issues = run_in_dialect(sql, Dialect::Mysql);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn flags_snowflake_table_plus_flatten_unqualified_value_reference() {
        let sql = "SELECT r.rec:foo::string AS foo, value:bar::string AS bar FROM foo.bar AS r, LATERAL FLATTEN(input => r.rec:result) AS x";
        let issues = run_in_dialect(sql, Dialect::Snowflake);
        assert!(!issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn ignore_words_config_skips_named_unqualified_refs() {
        let sql = "SELECT test1, test2 FROM t_table1 LEFT JOIN t_table_2 ON TRUE";
        let issues = run_with_config(
            sql,
            Dialect::Generic,
            serde_json::json!({"ignore_words":"test1,test2"}),
        );
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn ignore_words_regex_config_skips_named_unqualified_refs() {
        let sql = "SELECT _test1, _test2 FROM t_table1 LEFT JOIN t_table_2 ON TRUE";
        let issues = run_with_config(
            sql,
            Dialect::Generic,
            serde_json::json!({"ignore_words_regex":"^_"}),
        );
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn declared_bigquery_variables_are_exempt() {
        let sql = "DECLARE run_time TIMESTAMP DEFAULT '2020-01-01 00:00:00'; SELECT table_a.age FROM table_a INNER JOIN table_b ON table_a.id = table_b.id WHERE table_a.start_date <= run_time;";
        let issues = run_in_dialect(sql, Dialect::Bigquery);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn flags_unqualified_subquery_reference_when_outer_scope_exists() {
        let sql = "SELECT a FROM foo WHERE a IN (SELECT a FROM bar)";
        let issues = run(sql);
        assert!(!issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn subqueries_ignore_external_references_allows_unqualified_subquery_reference() {
        let sql = "SELECT a FROM foo WHERE a IN (SELECT a FROM bar)";
        let issues = run_with_config(
            sql,
            Dialect::Generic,
            serde_json::json!({"subqueries_ignore_external_references": true}),
        );
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn flags_scalar_subquery_unqualified_reference() {
        let sql = "SELECT (SELECT max(id) FROM foo2) AS f1 FROM bar";
        let issues = run(sql);
        assert!(!issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn flags_exists_subquery_unqualified_reference() {
        let sql = "SELECT id FROM bar WHERE EXISTS (SELECT 1 FROM foo2 WHERE bar.id = id)";
        let issues = run(sql);
        assert!(!issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn flags_nested_correlated_subquery_inside_from_clause() {
        let sql = "SELECT a.id AS a_id, b.id AS b_id FROM (SELECT id FROM foo WHERE id IN (SELECT id FROM baz)) AS a INNER JOIN bar AS b ON a.id = b.id";
        let issues = run(sql);
        assert!(!issues.is_empty(), "{issues:?}");
    }
}
