//! LINT_AL_005: Unused table alias.
//!
//! A table is aliased in a FROM/JOIN clause but the alias is never referenced
//! anywhere in the query. This may indicate dead code or a copy-paste error.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::*;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};
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
    relation_key: Option<String>,
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

impl BuiltinLintRule for UnusedTableAlias {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_005
    }

    fn name(&self) -> &'static str {
        "Unused table alias"
    }

    fn description(&self) -> &'static str {
        "Tables should not be aliased if that alias is not used."
    }

    fn check_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        match stmt {
            Statement::Query(q) => check_query(q, self.alias_case_check, ctx, &mut issues),
            Statement::Insert(ins) => {
                if let Some(ref source) = ins.source {
                    check_query(source, self.alias_case_check, ctx, &mut issues);
                }
            }
            Statement::CreateView(CreateView { query, .. }) => {
                check_query(query, self.alias_case_check, ctx, &mut issues)
            }
            Statement::CreateTable(create) => {
                if let Some(ref q) = create.query {
                    check_query(q, self.alias_case_check, ctx, &mut issues);
                }
            }
            Statement::Delete(delete) => {
                check_delete(delete, self.alias_case_check, ctx, &mut issues);
            }
            _ => {}
        }

        if let Some(first_issue) = issues.first_mut() {
            let autofix_edits: Vec<IssuePatchEdit> = al005_legacy_autofix_edits(
                ctx.statement_sql(),
                ctx.dialect(),
                self.alias_case_check,
            )
            .into_iter()
            .map(|(start, end)| IssuePatchEdit::new(ctx.span_from_statement_offset(start, end), ""))
            .collect();
            if !autofix_edits.is_empty() {
                *first_issue = first_issue
                    .clone()
                    .with_autofix_edits(IssueAutofixApplicability::Safe, autofix_edits);
            }
        }

        issues
    }
}

fn check_query(
    query: &Query,
    alias_case_check: AliasCaseCheck,
    ctx: &RuleContext,
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
    ctx: &RuleContext,
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
    ctx: &RuleContext,
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

    if matches!(ctx.dialect(), Dialect::Redshift) {
        if let Some(qualify) = &select.qualify {
            if include_qualify_alias_references(ctx.dialect(), select) {
                for alias in aliases.values() {
                    if redshift_qualify_uses_alias_prefixed_identifier(qualify, &alias.name) {
                        used_prefixes.insert(QualifierRef {
                            name: alias.name.clone(),
                            quoted: alias.quoted,
                        });
                    }
                }
            }
        }
    }

    emit_unused_alias_issues(
        &aliases,
        &used_prefixes,
        alias_case_check,
        ctx.dialect(),
        ctx.statement_index,
        issues,
    );
}

fn check_delete(
    delete: &Delete,
    alias_case_check: AliasCaseCheck,
    ctx: &RuleContext,
    issues: &mut Vec<Issue>,
) {
    let mut aliases: HashMap<String, AliasRef> = HashMap::new();
    let mut used_prefixes: HashSet<QualifierRef> = HashSet::new();

    for table in delete_source_tables(delete) {
        check_table_factor_subqueries(&table.relation, alias_case_check, ctx, issues);
        collect_aliases(&table.relation, ctx.dialect(), &mut aliases);
        collect_identifier_prefixes_from_table_factor(
            &table.relation,
            ctx.dialect(),
            &mut used_prefixes,
        );

        for join in &table.joins {
            check_table_factor_subqueries(&join.relation, alias_case_check, ctx, issues);
            collect_aliases(&join.relation, ctx.dialect(), &mut aliases);
            collect_identifier_prefixes_from_table_factor(
                &join.relation,
                ctx.dialect(),
                &mut used_prefixes,
            );
            if let Some(constraint) = join_constraint(&join.join_operator) {
                collect_identifier_prefixes(constraint, ctx.dialect(), &mut used_prefixes);
            }
        }
    }

    if let Some(selection) = &delete.selection {
        collect_identifier_prefixes(selection, ctx.dialect(), &mut used_prefixes);
    }
    if let Some(returning) = &delete.returning {
        for item in returning {
            collect_identifier_prefixes_from_select_item(item, ctx.dialect(), &mut used_prefixes);
        }
    }
    for order_expr in &delete.order_by {
        collect_identifier_prefixes(&order_expr.expr, ctx.dialect(), &mut used_prefixes);
    }
    if let Some(limit) = &delete.limit {
        collect_identifier_prefixes(limit, ctx.dialect(), &mut used_prefixes);
    }

    emit_unused_alias_issues(
        &aliases,
        &used_prefixes,
        alias_case_check,
        ctx.dialect(),
        ctx.statement_index,
        issues,
    );
}

fn delete_source_tables(delete: &Delete) -> Vec<&TableWithJoins> {
    let mut tables = Vec::new();

    match &delete.from {
        FromTable::WithFromKeyword(from) | FromTable::WithoutKeyword(from) => {
            tables.extend(from.iter());
        }
    }

    if let Some(using_tables) = &delete.using {
        tables.extend(using_tables.iter());
    }

    tables
}

fn emit_unused_alias_issues(
    aliases: &HashMap<String, AliasRef>,
    used_prefixes: &HashSet<QualifierRef>,
    alias_case_check: AliasCaseCheck,
    dialect: Dialect,
    statement_index: usize,
    issues: &mut Vec<Issue>,
) {
    if aliases.is_empty() {
        return;
    }

    let mut used_alias_names: HashSet<String> = HashSet::new();
    for alias in aliases.values() {
        let used = used_prefixes
            .iter()
            .any(|prefix| qualifier_matches_alias(prefix, alias, alias_case_check, dialect));
        if used {
            used_alias_names.insert(alias.name.clone());
        }
    }

    let mut relation_alias_counts: HashMap<String, usize> = HashMap::new();
    let mut relations_with_used_alias: HashSet<String> = HashSet::new();
    for alias in aliases.values() {
        let Some(relation_key) = &alias.relation_key else {
            continue;
        };
        *relation_alias_counts
            .entry(relation_key.clone())
            .or_insert(0) += 1;
        if used_alias_names.contains(&alias.name) {
            relations_with_used_alias.insert(relation_key.clone());
        }
    }

    for alias in aliases.values() {
        if used_alias_names.contains(&alias.name) {
            continue;
        }

        let repeated_self_join_alias_exempt = alias.relation_key.as_ref().is_some_and(|key| {
            relation_alias_counts.get(key).copied().unwrap_or_default() > 1
                && relations_with_used_alias.contains(key)
        });

        if repeated_self_join_alias_exempt {
            continue;
        }

        issues.push(
            Issue::warning(
                issue_codes::LINT_AL_005,
                format!(
                    "Table alias '{}' is defined but never referenced.",
                    alias.name
                ),
            )
            .with_statement(statement_index),
        );
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
    for connect_by_kind in &select.connect_by {
        match connect_by_kind {
            ConnectByKind::ConnectBy { relationships, .. } => {
                for relationship in relationships {
                    collect_identifier_prefixes(relationship, dialect, prefixes);
                }
            }
            ConnectByKind::StartWith { condition, .. } => {
                collect_identifier_prefixes(condition, dialect, prefixes);
            }
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

fn collect_aliases(
    relation: &TableFactor,
    dialect: Dialect,
    aliases: &mut HashMap<String, AliasRef>,
) {
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
                        relation_key: Some(table_name.to_ascii_uppercase()),
                    },
                );
            }
        }
        TableFactor::Derived {
            subquery,
            alias: Some(alias),
            ..
        } if derived_values_alias_can_be_unused(dialect, subquery) => {
            aliases.insert(
                alias.name.value.clone(),
                AliasRef {
                    name: alias.name.value.clone(),
                    quoted: alias.name.quote_style.is_some(),
                    relation_key: None,
                },
            );
        }
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
                    relation_key: None,
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
        Expr::CompoundIdentifier(parts) if parts.len() >= 2 => {
            prefixes.insert(QualifierRef {
                name: parts[0].value.clone(),
                quoted: parts[0].quote_style.is_some(),
            });
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
        Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. }
        | Expr::JsonAccess { value: inner, .. } => {
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

fn derived_values_alias_can_be_unused(dialect: Dialect, subquery: &Query) -> bool {
    // SQLFluff AL05 parity: this currently applies to SparkSQL fixtures
    // (mapped to Databricks). Other dialect fixtures treat VALUES aliases as
    // valid/required and should not be flagged.
    matches!(dialect, Dialect::Databricks) && matches!(subquery.body.as_ref(), SetExpr::Values(_))
}

fn redshift_qualify_uses_alias_prefixed_identifier(expr: &Expr, alias: &str) -> bool {
    match expr {
        Expr::Identifier(identifier) => {
            let value = identifier.value.as_str();
            value
                .strip_prefix(alias)
                .is_some_and(|suffix| suffix.starts_with('_'))
                || value
                    .to_ascii_uppercase()
                    .strip_prefix(&alias.to_ascii_uppercase())
                    .is_some_and(|suffix| suffix.starts_with('_'))
        }
        Expr::CompoundIdentifier(_) => false,
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. } => {
            redshift_qualify_uses_alias_prefixed_identifier(left, alias)
                || redshift_qualify_uses_alias_prefixed_identifier(right, alias)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            redshift_qualify_uses_alias_prefixed_identifier(inner, alias)
        }
        Expr::InList { expr, list, .. } => {
            redshift_qualify_uses_alias_prefixed_identifier(expr, alias)
                || list
                    .iter()
                    .any(|item| redshift_qualify_uses_alias_prefixed_identifier(item, alias))
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            redshift_qualify_uses_alias_prefixed_identifier(expr, alias)
                || redshift_qualify_uses_alias_prefixed_identifier(low, alias)
                || redshift_qualify_uses_alias_prefixed_identifier(high, alias)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand
                .as_ref()
                .is_some_and(|inner| redshift_qualify_uses_alias_prefixed_identifier(inner, alias))
                || conditions.iter().any(|when| {
                    redshift_qualify_uses_alias_prefixed_identifier(&when.condition, alias)
                        || redshift_qualify_uses_alias_prefixed_identifier(&when.result, alias)
                })
                || else_result.as_ref().is_some_and(|inner| {
                    redshift_qualify_uses_alias_prefixed_identifier(inner, alias)
                })
        }
        Expr::Function(function) => {
            let args_match = if let FunctionArguments::List(arguments) = &function.args {
                arguments.args.iter().any(|arg| match arg {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                    | FunctionArg::Named {
                        arg: FunctionArgExpr::Expr(expr),
                        ..
                    } => redshift_qualify_uses_alias_prefixed_identifier(expr, alias),
                    _ => false,
                })
            } else {
                false
            };
            let filter_match = function.filter.as_ref().is_some_and(|filter| {
                redshift_qualify_uses_alias_prefixed_identifier(filter, alias)
            });
            let within_group_match = function.within_group.iter().any(|order_expr| {
                redshift_qualify_uses_alias_prefixed_identifier(&order_expr.expr, alias)
            });
            let over_match = match &function.over {
                Some(WindowType::WindowSpec(spec)) => {
                    spec.partition_by
                        .iter()
                        .any(|expr| redshift_qualify_uses_alias_prefixed_identifier(expr, alias))
                        || spec.order_by.iter().any(|order_expr| {
                            redshift_qualify_uses_alias_prefixed_identifier(&order_expr.expr, alias)
                        })
                }
                _ => false,
            };
            args_match || filter_match || within_group_match || over_match
        }
        _ => false,
    }
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
    dialect: Dialect,
) -> bool {
    match alias_case_check {
        AliasCaseCheck::CaseInsensitive => qualifier.name.eq_ignore_ascii_case(&alias.name),
        AliasCaseCheck::CaseSensitive => qualifier.name == alias.name,
        AliasCaseCheck::Dialect => {
            normalize_identifier_for_dialect(&qualifier.name, qualifier.quoted, dialect)
                == normalize_identifier_for_dialect(&alias.name, alias.quoted, dialect)
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

fn normalize_identifier_for_dialect(identifier: &str, quoted: bool, dialect: Dialect) -> String {
    if quoted && !quoted_identifiers_case_insensitive_for_dialect(dialect) {
        identifier.to_string()
    } else {
        normalize_naked_identifier_for_dialect(identifier, dialect)
    }
}

fn normalize_naked_identifier_for_dialect(identifier: &str, dialect: Dialect) -> String {
    if matches!(
        dialect,
        Dialect::Postgres
            | Dialect::Redshift
            | Dialect::Mysql
            | Dialect::Sqlite
            | Dialect::Mssql
            | Dialect::Clickhouse
    ) {
        identifier.to_ascii_lowercase()
    } else {
        identifier.to_ascii_uppercase()
    }
}

fn quoted_identifiers_case_insensitive_for_dialect(dialect: Dialect) -> bool {
    matches!(
        dialect,
        Dialect::Duckdb | Dialect::Hive | Dialect::Sqlite | Dialect::Databricks
    )
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
    ctx: &RuleContext,
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

#[derive(Debug, Clone)]
struct LegacySimpleTableAliasDecl {
    table_end: usize,
    alias_end: usize,
    alias: String,
    quoted: bool,
}

#[derive(Clone)]
struct LegacyLocatedToken {
    token: Token,
    end: usize,
}

fn al005_legacy_autofix_edits(
    sql: &str,
    dialect: Dialect,
    alias_case_check: AliasCaseCheck,
) -> Vec<(usize, usize)> {
    let Some(decls) = legacy_collect_simple_table_alias_declarations(sql, dialect) else {
        return Vec::new();
    };
    if decls.is_empty() {
        return Vec::new();
    }

    let mut seen_aliases = HashSet::new();
    let mut removals = Vec::new();
    for decl in &decls {
        let alias_key = decl.alias.to_ascii_lowercase();
        if !seen_aliases.insert(alias_key.clone()) {
            continue;
        }
        if legacy_is_sql_keyword(&decl.alias) || legacy_is_generated_alias_identifier(&decl.alias) {
            continue;
        }
        if legacy_contains_alias_qualifier_dialect(
            sql,
            &decl.alias,
            decl.quoted,
            dialect,
            alias_case_check,
        ) {
            continue;
        }

        removals.extend(
            decls
                .iter()
                .filter(|candidate| candidate.alias.eq_ignore_ascii_case(&alias_key))
                .map(|candidate| (candidate.table_end, candidate.alias_end)),
        );
    }

    removals.sort_unstable();
    removals.dedup();
    removals.retain(|(start, end)| start < end);
    removals
}

fn legacy_collect_simple_table_alias_declarations(
    sql: &str,
    dialect: Dialect,
) -> Option<Vec<LegacySimpleTableAliasDecl>> {
    let tokens = legacy_tokenize_with_offsets(sql, dialect)?;
    let mut out = Vec::new();
    let mut index = 0usize;

    while index < tokens.len() {
        if !legacy_token_matches_keyword(&tokens[index].token, "FROM")
            && !legacy_token_matches_keyword(&tokens[index].token, "JOIN")
        {
            index += 1;
            continue;
        }

        // Parse first table item after FROM/JOIN.
        let Some(next) = legacy_next_non_trivia_token(&tokens, index + 1) else {
            index += 1;
            continue;
        };
        index = legacy_try_parse_table_item(&tokens, next, dialect, &mut out);

        // Handle comma-separated table items (FROM t1, t2, LATERAL f(...) AS x).
        while let Some(comma_index) = legacy_next_non_trivia_token(&tokens, index) {
            if !matches!(tokens[comma_index].token, Token::Comma) {
                break;
            }
            let Some(next_item) = legacy_next_non_trivia_token(&tokens, comma_index + 1) else {
                index = comma_index + 1;
                break;
            };
            index = legacy_try_parse_table_item(&tokens, next_item, dialect, &mut out);
        }
    }

    Some(out)
}

/// Try to parse a single table item (table reference or LATERAL function call) starting
/// at token `start`. Returns the next token index to continue scanning from.
fn legacy_try_parse_table_item(
    tokens: &[LegacyLocatedToken],
    start: usize,
    dialect: Dialect,
    out: &mut Vec<LegacySimpleTableAliasDecl>,
) -> usize {
    if start >= tokens.len() {
        return start;
    }

    // Handle LATERAL function(...) AS alias pattern.
    if legacy_token_matches_keyword(&tokens[start].token, "LATERAL") {
        if let Some(func_end) = legacy_skip_lateral_function_call(tokens, start + 1) {
            let Some(mut alias_index) = legacy_next_non_trivia_token(tokens, func_end) else {
                return func_end;
            };
            if legacy_token_matches_keyword(&tokens[alias_index].token, "AS") {
                let Some(next_index) = legacy_next_non_trivia_token(tokens, alias_index + 1) else {
                    return alias_index + 1;
                };
                alias_index = next_index;
            }
            if let Some((alias_value, alias_quoted)) =
                legacy_token_any_identifier(&tokens[alias_index].token)
            {
                out.push(LegacySimpleTableAliasDecl {
                    table_end: tokens[func_end - 1].end,
                    alias_end: tokens[alias_index].end,
                    alias: alias_value.to_string(),
                    quoted: alias_quoted,
                });
                return alias_index + 1;
            }
            return func_end;
        }
        return start + 1;
    }

    // Handle parenthesized VALUES relation:
    //   (VALUES (...), (...)) AS t(c1, c2)
    if matches!(tokens[start].token, Token::LParen)
        && matches!(dialect, Dialect::Databricks)
        && legacy_parenthesized_relation_starts_with_values(tokens, start)
    {
        let Some(paren_end) = legacy_skip_parenthesized(tokens, start) else {
            return start + 1;
        };
        let table_end = tokens[paren_end - 1].end;

        let Some(mut alias_index) = legacy_next_non_trivia_token(tokens, paren_end) else {
            return paren_end;
        };
        if legacy_token_matches_keyword(&tokens[alias_index].token, "AS") {
            let Some(next_index) = legacy_next_non_trivia_token(tokens, alias_index + 1) else {
                return alias_index + 1;
            };
            alias_index = next_index;
        }
        let Some((alias_value, alias_quoted)) =
            legacy_token_any_identifier(&tokens[alias_index].token)
        else {
            return paren_end;
        };

        let mut alias_end = tokens[alias_index].end;
        let mut next_cursor = alias_index + 1;
        if let Some(cols_start) = legacy_next_non_trivia_token(tokens, alias_index + 1) {
            if matches!(tokens[cols_start].token, Token::LParen) {
                if let Some(cols_end) = legacy_skip_parenthesized(tokens, cols_start) {
                    alias_end = tokens[cols_end - 1].end;
                    next_cursor = cols_end;
                }
            }
        }

        out.push(LegacySimpleTableAliasDecl {
            table_end,
            alias_end,
            alias: alias_value.to_string(),
            quoted: alias_quoted,
        });
        return next_cursor;
    }

    // Table name: identifier(.identifier)*
    if legacy_token_any_identifier(&tokens[start].token).is_none() {
        return start + 1;
    }

    let mut table_end = tokens[start].end;
    let mut cursor = start + 1;

    while let Some(dot_index) = legacy_next_non_trivia_token(tokens, cursor) {
        if !matches!(tokens[dot_index].token, Token::Period) {
            break;
        }
        let Some(next_index) = legacy_next_non_trivia_token(tokens, dot_index + 1) else {
            break;
        };
        if legacy_token_any_identifier(&tokens[next_index].token).is_none() {
            break;
        }
        table_end = tokens[next_index].end;
        cursor = next_index + 1;
    }

    let Some(mut alias_index) = legacy_next_non_trivia_token(tokens, cursor) else {
        return cursor;
    };
    if legacy_token_matches_keyword(&tokens[alias_index].token, "AS") {
        let Some(next_index) = legacy_next_non_trivia_token(tokens, alias_index + 1) else {
            return alias_index + 1;
        };
        alias_index = next_index;
    }

    let Some((alias_value, alias_quoted)) = legacy_token_any_identifier(&tokens[alias_index].token)
    else {
        return cursor;
    };

    out.push(LegacySimpleTableAliasDecl {
        table_end,
        alias_end: tokens[alias_index].end,
        alias: alias_value.to_string(),
        quoted: alias_quoted,
    });
    alias_index + 1
}

fn legacy_tokenize_with_offsets(sql: &str, dialect: Dialect) -> Option<Vec<LegacyLocatedToken>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        let (_, end) = legacy_token_with_span_offsets(sql, &token)?;
        out.push(LegacyLocatedToken {
            token: token.token,
            end,
        });
    }
    Some(out)
}

fn legacy_token_with_span_offsets(sql: &str, token: &TokenWithSpan) -> Option<(usize, usize)> {
    let start = legacy_line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )?;
    let end = legacy_line_col_to_offset(
        sql,
        token.span.end.line as usize,
        token.span.end.column as usize,
    )?;
    Some((start, end))
}

fn legacy_line_col_to_offset(sql: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }

    let mut current_line = 1usize;
    let mut current_col = 1usize;

    for (offset, ch) in sql.char_indices() {
        if current_line == line && current_col == column {
            return Some(offset);
        }

        if ch == '\n' {
            current_line += 1;
            current_col = 1;
        } else {
            current_col += 1;
        }
    }

    if current_line == line && current_col == column {
        return Some(sql.len());
    }

    None
}

fn legacy_next_non_trivia_token(tokens: &[LegacyLocatedToken], mut start: usize) -> Option<usize> {
    while start < tokens.len() {
        if !legacy_is_trivia_token(&tokens[start].token) {
            return Some(start);
        }
        start += 1;
    }
    None
}

/// Skip past `FUNCTION_NAME(...)` after LATERAL keyword.
/// Returns the token index right after the closing `)`, or None if not found.
fn legacy_skip_lateral_function_call(tokens: &[LegacyLocatedToken], start: usize) -> Option<usize> {
    // Expect: function_name ( ... )
    let func_index = legacy_next_non_trivia_token(tokens, start)?;
    legacy_token_any_identifier(&tokens[func_index].token)?;
    let lparen_index = legacy_next_non_trivia_token(tokens, func_index + 1)?;
    if !matches!(tokens[lparen_index].token, Token::LParen) {
        return None;
    }
    // Find matching closing paren, handling nesting.
    let mut depth = 1u32;
    let mut cursor = lparen_index + 1;
    while cursor < tokens.len() && depth > 0 {
        match &tokens[cursor].token {
            Token::LParen => depth += 1,
            Token::RParen => depth -= 1,
            _ => {}
        }
        cursor += 1;
    }
    if depth == 0 {
        Some(cursor)
    } else {
        None
    }
}

fn legacy_parenthesized_relation_starts_with_values(
    tokens: &[LegacyLocatedToken],
    lparen_index: usize,
) -> bool {
    let Some(first_inner) = legacy_next_non_trivia_token(tokens, lparen_index + 1) else {
        return false;
    };
    legacy_token_matches_keyword(&tokens[first_inner].token, "VALUES")
}

fn legacy_skip_parenthesized(tokens: &[LegacyLocatedToken], lparen_index: usize) -> Option<usize> {
    if !matches!(tokens.get(lparen_index)?.token, Token::LParen) {
        return None;
    }
    let mut depth = 1u32;
    let mut cursor = lparen_index + 1;
    while cursor < tokens.len() && depth > 0 {
        match &tokens[cursor].token {
            Token::LParen => depth += 1,
            Token::RParen => depth -= 1,
            _ => {}
        }
        cursor += 1;
    }
    if depth == 0 {
        Some(cursor)
    } else {
        None
    }
}

fn legacy_is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(
            Whitespace::Space
                | Whitespace::Newline
                | Whitespace::Tab
                | Whitespace::SingleLineComment { .. }
                | Whitespace::MultiLineComment(_)
        )
    )
}

fn legacy_token_matches_keyword(token: &Token, keyword: &str) -> bool {
    matches!(token, Token::Word(word) if word.value.eq_ignore_ascii_case(keyword))
}

/// Extract identifier value from any token type (unquoted, single-quoted,
/// double-quoted, backtick-quoted). Returns (value, is_quoted).
fn legacy_token_any_identifier(token: &Token) -> Option<(&str, bool)> {
    match token {
        Token::Word(word) if legacy_is_simple_identifier(&word.value) => {
            if word.quote_style.is_some() {
                Some((&word.value, true))
            } else {
                Some((&word.value, false))
            }
        }
        Token::SingleQuotedString(s) => Some((s.as_str(), true)),
        _ => None,
    }
}

fn legacy_contains_alias_qualifier_dialect(
    sql: &str,
    alias: &str,
    alias_quoted: bool,
    dialect: Dialect,
    alias_case_check: AliasCaseCheck,
) -> bool {
    if matches!(dialect, Dialect::Redshift)
        && legacy_redshift_qualify_uses_alias_prefixed_identifier(sql, alias)
    {
        return true;
    }

    let alias_bytes = alias.as_bytes();
    if alias_bytes.is_empty() {
        return false;
    }

    // Build the normalized alias for comparison, respecting the dialect/config.
    let normalized_alias = match alias_case_check {
        AliasCaseCheck::Dialect => normalize_identifier_for_dialect(alias, alias_quoted, dialect),
        AliasCaseCheck::CaseInsensitive => alias.to_ascii_lowercase(),
        AliasCaseCheck::CaseSensitive => alias.to_string(),
        AliasCaseCheck::QuotedCsNakedUpper => {
            if alias_quoted {
                alias.to_string()
            } else {
                alias.to_ascii_uppercase()
            }
        }
        AliasCaseCheck::QuotedCsNakedLower => {
            if alias_quoted {
                alias.to_string()
            } else {
                alias.to_ascii_lowercase()
            }
        }
    };

    let bytes = sql.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        // Skip past quote characters to handle quoted qualifiers like "A".col
        let (ref_name, ref_quoted, ref_end) = if index < bytes.len()
            && (bytes[index] == b'"' || bytes[index] == b'`' || bytes[index] == b'[')
        {
            let close_char = match bytes[index] {
                b'"' => b'"',
                b'`' => b'`',
                b'[' => b']',
                _ => unreachable!(),
            };
            let start = index + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] != close_char {
                end += 1;
            }
            if end >= bytes.len() {
                index += 1;
                continue;
            }
            let name = &sql[start..end];
            // end points to the closing quote; advance past it
            (name.to_string(), true, end + 1)
        } else if index < bytes.len() && legacy_is_ascii_ident_start(bytes[index]) {
            let start = index;
            let mut end = start;
            while end < bytes.len() && legacy_is_ascii_ident_continue(bytes[end]) {
                end += 1;
            }
            let name = &sql[start..end];
            (name.to_string(), false, end)
        } else {
            index += 1;
            continue;
        };

        // Check if followed by '.'
        if ref_end < bytes.len() && bytes[ref_end] == b'.' {
            let normalized_ref = match alias_case_check {
                AliasCaseCheck::Dialect => {
                    normalize_identifier_for_dialect(&ref_name, ref_quoted, dialect)
                }
                AliasCaseCheck::CaseInsensitive => ref_name.to_ascii_lowercase(),
                AliasCaseCheck::CaseSensitive => ref_name.clone(),
                AliasCaseCheck::QuotedCsNakedUpper => {
                    if ref_quoted {
                        ref_name.clone()
                    } else {
                        ref_name.to_ascii_uppercase()
                    }
                }
                AliasCaseCheck::QuotedCsNakedLower => {
                    if ref_quoted {
                        ref_name.clone()
                    } else {
                        ref_name.to_ascii_lowercase()
                    }
                }
            };
            if normalized_ref == normalized_alias {
                return true;
            }
        }

        index = if ref_end > index { ref_end } else { index + 1 };
    }

    false
}

fn legacy_redshift_qualify_uses_alias_prefixed_identifier(sql: &str, alias: &str) -> bool {
    let Some(tokens) = legacy_tokenize_with_offsets(sql, Dialect::Redshift) else {
        return false;
    };
    let Some(qualify_index) = tokens
        .iter()
        .position(|token| legacy_token_matches_keyword(&token.token, "QUALIFY"))
    else {
        return false;
    };

    // SQLFluff parity: for Redshift AL05, QUALIFY references only count when
    // QUALIFY follows FROM/JOIN directly (i.e. no WHERE before QUALIFY).
    if tokens[..qualify_index]
        .iter()
        .any(|token| legacy_token_matches_keyword(&token.token, "WHERE"))
    {
        return false;
    }

    tokens[qualify_index + 1..]
        .iter()
        .filter_map(|token| legacy_token_reference_identifier(&token.token))
        .any(|identifier| legacy_alias_prefixed_identifier(identifier, alias))
}

fn legacy_token_reference_identifier(token: &Token) -> Option<&str> {
    match token {
        Token::Word(word) => Some(word.value.as_str()),
        _ => None,
    }
}

fn legacy_alias_prefixed_identifier(identifier: &str, alias: &str) -> bool {
    if identifier.is_empty() || alias.is_empty() {
        return false;
    }
    identifier
        .to_ascii_uppercase()
        .strip_prefix(&alias.to_ascii_uppercase())
        .is_some_and(|suffix| suffix.starts_with('_'))
}

fn legacy_is_generated_alias_identifier(alias: &str) -> bool {
    let mut chars = alias.chars();
    match chars.next() {
        Some('t') => {}
        _ => return false,
    }
    let mut saw_digit = false;
    for ch in chars {
        if !ch.is_ascii_digit() {
            return false;
        }
        saw_digit = true;
    }
    saw_digit
}

fn legacy_is_sql_keyword(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "ALL"
            | "ALTER"
            | "AND"
            | "ANY"
            | "AS"
            | "ASC"
            | "BEGIN"
            | "BETWEEN"
            | "BOOLEAN"
            | "BY"
            | "CASE"
            | "CAST"
            | "CHECK"
            | "COLUMN"
            | "CONSTRAINT"
            | "CREATE"
            | "CROSS"
            | "DEFAULT"
            | "DELETE"
            | "DESC"
            | "DISTINCT"
            | "DROP"
            | "ELSE"
            | "END"
            | "EXCEPT"
            | "EXISTS"
            | "FALSE"
            | "FETCH"
            | "FOR"
            | "FOREIGN"
            | "FROM"
            | "FULL"
            | "GROUP"
            | "HAVING"
            | "IF"
            | "IN"
            | "INDEX"
            | "INNER"
            | "INSERT"
            | "INT"
            | "INTEGER"
            | "INTERSECT"
            | "INTO"
            | "IS"
            | "JOIN"
            | "KEY"
            | "LEFT"
            | "LIKE"
            | "LIMIT"
            | "NOT"
            | "NULL"
            | "OFFSET"
            | "ON"
            | "OR"
            | "ORDER"
            | "OUTER"
            | "OVER"
            | "PARTITION"
            | "PRIMARY"
            | "REFERENCES"
            | "RIGHT"
            | "SELECT"
            | "SET"
            | "TABLE"
            | "TEXT"
            | "THEN"
            | "TRUE"
            | "UNION"
            | "UNIQUE"
            | "UPDATE"
            | "USING"
            | "VALUES"
            | "VARCHAR"
            | "VIEW"
            | "WHEN"
            | "WHERE"
            | "WINDOW"
            | "WITH"
    )
}

fn legacy_is_simple_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !legacy_is_ascii_ident_start(bytes[0]) {
        return false;
    }
    bytes[1..]
        .iter()
        .copied()
        .all(legacy_is_ascii_ident_continue)
}

fn legacy_is_ascii_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'#'
}

fn legacy_is_ascii_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::{parse_sql, parse_sql_with_dialect};
    use crate::types::{Dialect, IssueAutofixApplicability};

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = UnusedTableAlias::default();
        let ctx = RuleContext::new(sql, 0..sql.len(), 0);
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check_with_context(stmt, &ctx));
        }
        issues
    }

    fn apply_issue_autofix(sql: &str, issue: &Issue) -> Option<String> {
        let autofix = issue.autofix.as_ref()?;
        let mut edits = autofix.edits.clone();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        let mut rewritten = sql.to_string();
        for edit in edits.into_iter().rev() {
            rewritten.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(rewritten)
    }

    fn check_sql_in_dialect(sql: &str, dialect: Dialect) -> Vec<Issue> {
        let stmts = parse_sql_with_dialect(sql, dialect).unwrap();
        let rule = UnusedTableAlias::default();
        let ctx = RuleContext::new(sql, 0..sql.len(), 0).with_dialect(dialect);
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check_with_context(stmt, &ctx));
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
    fn test_unused_alias_emits_safe_autofix_patch() {
        let sql = "SELECT users.name FROM users AS u JOIN orders AS o ON users.id = orders.user_id";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 2);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "SELECT users.name FROM users JOIN orders ON users.id = orders.user_id"
        );
    }

    #[test]
    fn test_generated_alias_does_not_emit_autofix() {
        let sql = "SELECT * FROM users AS t1";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0].autofix.is_none(),
            "legacy AL005 parity skips generated aliases like t1"
        );
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
        let issues = rule.check_with_context(&stmts[0], &RuleContext::new(sql, 0..sql.len(), 0));
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
        let issues = rule.check_with_context(&stmts[0], &RuleContext::new(sql, 0..sql.len(), 0));
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
        let issues = rule.check_with_context(&stmts[0], &RuleContext::new(sql, 0..sql.len(), 0));
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
        let issues = rule.check_with_context(&stmts[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn dialect_mode_generic_allows_quoted_unquoted_fold_match() {
        let issues = check_sql("SELECT a.col1 FROM tab1 AS \"A\"");
        assert!(issues.is_empty());
    }

    #[test]
    fn dialect_mode_generic_allows_quoted_prefix_against_unquoted_alias() {
        let issues = check_sql("SELECT \"A\".col1 FROM tab1 AS a");
        assert!(issues.is_empty());
    }

    #[test]
    fn dialect_mode_generic_flags_single_quoted_alias_case_mismatch() {
        let issues = check_sql("SELECT a.col1 FROM tab1 AS 'a'");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("a"));
    }

    #[test]
    fn dialect_mode_postgres_allows_lower_fold_for_quoted_alias() {
        let issues =
            check_sql_in_dialect("SELECT A.col_1 FROM table_a AS \"a\"", Dialect::Postgres);
        assert!(issues.is_empty());
    }

    #[test]
    fn dialect_mode_snowflake_flags_mixed_quoted_case_mismatch() {
        let issues =
            check_sql_in_dialect("SELECT a.col_1 FROM table_a AS \"a\"", Dialect::Snowflake);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("a"));
    }

    #[test]
    fn dialect_mode_bigquery_allows_backtick_quoted_alias_fold_match() {
        let issues = check_sql_in_dialect("SELECT a.col1 FROM tab1 AS `A`", Dialect::Bigquery);
        assert!(issues.is_empty());
    }

    #[test]
    fn dialect_mode_redshift_allows_lower_fold_for_quoted_alias() {
        let issues =
            check_sql_in_dialect("SELECT A.col_1 FROM table_a AS \"a\"", Dialect::Redshift);
        assert!(issues.is_empty());
    }

    #[test]
    fn dialect_mode_redshift_flags_mixed_quoted_case_mismatch() {
        let issues =
            check_sql_in_dialect("SELECT a.col_1 FROM table_a AS \"A\"", Dialect::Redshift);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("A"));
    }

    #[test]
    fn dialect_mode_mysql_allows_backtick_qualified_reference_against_unquoted_alias() {
        let issues = check_sql_in_dialect(
            "SELECT `nih`.`userID` FROM `flight_notification_item_history` AS nih",
            Dialect::Mysql,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn dialect_mode_duckdb_allows_case_insensitive_quoted_reference() {
        let issues = check_sql_in_dialect("SELECT \"a\".col_1 FROM table_a AS A", Dialect::Duckdb);
        assert!(issues.is_empty());
    }

    #[test]
    fn dialect_mode_hive_allows_case_insensitive_quoted_reference() {
        let issues = check_sql_in_dialect("SELECT `a`.col1 FROM tab1 AS A", Dialect::Hive);
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
    fn flags_unused_alias_inside_snowflake_delete_using_cte() {
        let issues = check_sql_in_dialect(
            "DELETE FROM MYTABLE1 \
             USING ( \
                 WITH MYCTE AS (SELECT COLUMN2 FROM MYTABLE3 AS MT3) \
                 SELECT COLUMN3 FROM MYTABLE3 \
             ) X \
             WHERE COLUMN1 = X.COLUMN3",
            Dialect::Snowflake,
        );
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("MT3"));
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
    fn redshift_qualify_unqualified_alias_prefixed_identifier_counts_alias_usage() {
        let issues = check_sql_in_dialect(
            "SELECT * \
             FROM #store_sales AS ss \
             QUALIFY row_number() OVER (PARTITION BY ss_sold_date ORDER BY ss_sales_price DESC) <= 2",
            Dialect::Redshift,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn redshift_qualify_after_from_autofix_keeps_used_join_alias() {
        let sql = "SELECT *\n\
FROM #store as s\n\
INNER JOIN #store_sales AS ss\n\
QUALIFY row_number() OVER (PARTITION BY ss_sold_date ORDER BY ss_sales_price DESC) <= 2";
        let issues = check_sql_in_dialect(sql, Dialect::Redshift);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "SELECT *\n\
FROM #store\n\
INNER JOIN #store_sales AS ss\n\
QUALIFY row_number() OVER (PARTITION BY ss_sold_date ORDER BY ss_sales_price DESC) <= 2"
        );
    }

    #[test]
    fn redshift_qualify_after_where_autofix_removes_both_unused_aliases() {
        let sql = "SELECT *\n\
FROM #store as s\n\
INNER JOIN #store_sales AS ss\n\
WHERE col = 1\n\
QUALIFY row_number() OVER (PARTITION BY ss_sold_date ORDER BY ss_sales_price DESC) <= 2";
        let issues = check_sql_in_dialect(sql, Dialect::Redshift);
        assert_eq!(issues.len(), 2);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "SELECT *\n\
FROM #store\n\
INNER JOIN #store_sales\n\
WHERE col = 1\n\
QUALIFY row_number() OVER (PARTITION BY ss_sold_date ORDER BY ss_sales_price DESC) <= 2"
        );
    }

    #[test]
    fn sparksql_values_derived_alias_is_detected_and_autofixed() {
        let sql = "SELECT *\n\
FROM (\n\
    VALUES (1, 2), (3, 4)\n\
) AS t(c1, c2)";
        let issues = check_sql_in_dialect(sql, Dialect::Databricks);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "SELECT *\n\
FROM (\n\
    VALUES (1, 2), (3, 4)\n\
)"
        );
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

    #[test]
    fn allows_repeat_referenced_table_aliases() {
        let issues = check_sql(
            "SELECT ROW_NUMBER() OVER(PARTITION BY a.object_id ORDER BY a.object_id) \
             FROM sys.objects a \
             CROSS JOIN sys.objects b \
             CROSS JOIN sys.objects c",
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn dialect_mode_databricks_allows_backtick_case_insensitive_reference() {
        let issues =
            check_sql_in_dialect("SELECT `a`.col_1 FROM table_a AS A", Dialect::Databricks);
        assert!(issues.is_empty());
    }

    #[test]
    fn snowflake_json_access_counts_as_alias_usage() {
        let issues = check_sql_in_dialect(
            "SELECT r.rec:foo::string FROM foo.bar AS r",
            Dialect::Snowflake,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn snowflake_lateral_flatten_unused_alias_detected_and_fixable() {
        let sql = "SELECT r.rec:foo::string, value:bar::string \
                   FROM foo.bar AS r, LATERAL FLATTEN(input => rec:result) AS x";
        let issues = check_sql_in_dialect(sql, Dialect::Snowflake);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("x"));
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert!(
            !fixed.contains("AS x"),
            "autofix should remove the unused LATERAL alias"
        );
    }

    #[test]
    fn autofix_removes_double_quoted_alias_in_dialect_mode() {
        let sql = "SELECT a.col_1\nFROM table_a AS \"A\"\n";
        let issues = check_sql_in_dialect(sql, Dialect::Postgres);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("A"));
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT a.col_1\nFROM table_a\n");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
    }

    #[test]
    fn autofix_removes_single_quoted_alias() {
        let sql = "SELECT a.col1\nFROM tab1 as 'a'\n";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT a.col1\nFROM tab1\n");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
    }
}
