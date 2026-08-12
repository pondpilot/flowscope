//! LINT_RF_001: References from.
//!
//! Qualified column prefixes should resolve to known FROM/JOIN sources.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{
    AlterPolicy, AlterPolicyOperation, Assignment, AssignmentTarget, ConditionalStatements,
    CreatePolicy, CreateView, Expr, FromTable, FunctionArg, FunctionArgExpr, FunctionArguments,
    Ident, Merge, MergeAction, MergeInsertKind, MergeUpdateExpr, ObjectName, OrderByKind, Query,
    Select, SelectItem, SelectItemQualifiedWildcardKind, SetExpr, Statement, TableFactor,
    TableWithJoins, Update, UpdateTableFromKind,
};

use super::semantic_helpers::{join_on_expr, table_factor_alias_name, visit_select_expressions};

pub struct ReferencesFrom {
    force_enable: bool,
    force_enable_configured: bool,
}

impl ReferencesFrom {
    pub fn from_config(config: &LintConfig) -> Self {
        let force_enable = config.rule_option_bool(issue_codes::LINT_RF_001, "force_enable");
        Self {
            force_enable: force_enable.unwrap_or(true),
            force_enable_configured: force_enable.is_some(),
        }
    }
}

impl Default for ReferencesFrom {
    fn default() -> Self {
        Self {
            force_enable: true,
            force_enable_configured: false,
        }
    }
}

impl BuiltinLintRule for ReferencesFrom {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_001
    }

    fn name(&self) -> &'static str {
        "References from"
    }

    fn description(&self) -> &'static str {
        "References cannot reference objects not present in 'FROM' clause."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let effective_force_enable = if self.force_enable_configured {
            self.force_enable
        } else {
            // SQLFluff keeps RF01 disabled by default for SparkSQL because
            // dotted struct-field access is ambiguous with table qualification.
            !matches!(ctx.dialect(), crate::types::Dialect::Databricks)
        };

        if !effective_force_enable {
            return Vec::new();
        }

        let unresolved_count = unresolved_references_in_statement(
            statement,
            &SourceRegistry::with_force_enable_explicit(self.force_enable_configured),
            ctx.dialect(),
            false,
        );

        (0..unresolved_count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_RF_001,
                    "Reference prefix appears unresolved from FROM/JOIN sources.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

#[derive(Clone, Default)]
struct SourceRegistry {
    exact: HashSet<String>,
    unqualified: HashSet<String>,
    force_enable_explicit: bool,
}

impl SourceRegistry {
    fn with_force_enable_explicit(force_enable_explicit: bool) -> Self {
        Self {
            force_enable_explicit,
            ..Self::default()
        }
    }

    fn isolated(&self) -> Self {
        Self::with_force_enable_explicit(self.force_enable_explicit)
    }

    fn register_alias(&mut self, alias: &str) {
        let clean = clean_identifier_component(alias);
        if clean.is_empty() {
            return;
        }
        self.exact.insert(clean.clone());
        self.unqualified.insert(clean);
    }

    fn register_object_name(&mut self, name: &ObjectName) {
        let parts = object_name_parts(name);
        if parts.is_empty() {
            return;
        }

        let full = parts.join(".");
        self.exact.insert(full);

        if let Some(last) = parts.last() {
            // Qualified table references are commonly referred to by their trailing
            // relation name (e.g. schema.table -> table).
            self.exact.insert(last.clone());
        }

        if parts.len() == 1 {
            self.unqualified.insert(parts[0].clone());
        }
    }

    fn register_pseudo_sources(&mut self, in_trigger: bool) {
        for pseudo in ["EXCLUDED", "INSERTED", "DELETED"] {
            self.register_alias(pseudo);
        }
        if in_trigger {
            self.register_alias("NEW");
            self.register_alias("OLD");
        }
    }

    fn matches_qualifier(&self, qualifier_parts: &[String]) -> bool {
        if qualifier_parts.is_empty() {
            return true;
        }

        let full = qualifier_parts.join(".");
        if self.exact.contains(&full) {
            return true;
        }

        if qualifier_parts.len() > 1 {
            if let Some(last) = qualifier_parts.last() {
                return self.unqualified.contains(last);
            }
        }

        false
    }

    fn is_empty(&self) -> bool {
        self.exact.is_empty()
    }
}

fn unresolved_references_in_statement(
    statement: &Statement,
    inherited_sources: &SourceRegistry,
    dialect: crate::types::Dialect,
    in_trigger: bool,
) -> usize {
    match statement {
        Statement::Query(query) => {
            unresolved_references_in_query(query, inherited_sources, dialect, in_trigger)
        }
        Statement::Insert(insert) => insert.source.as_ref().map_or(0, |query| {
            unresolved_references_in_query(query, inherited_sources, dialect, in_trigger)
        }),
        Statement::CreateView(CreateView { query, .. }) => {
            unresolved_references_in_query(query, inherited_sources, dialect, in_trigger)
        }
        Statement::CreateTable(create) => create.query.as_ref().map_or(0, |query| {
            unresolved_references_in_query(query, inherited_sources, dialect, in_trigger)
        }),
        Statement::Update(Update {
            table,
            assignments,
            from,
            selection,
            returning,
            ..
        }) => {
            let mut scope_sources = inherited_sources.clone();
            register_table_with_joins_sources(table, &mut scope_sources);
            if let Some(from_tables) = from {
                let tables = match from_tables {
                    UpdateTableFromKind::BeforeSet(tables)
                    | UpdateTableFromKind::AfterSet(tables) => tables,
                };
                for table in tables {
                    register_table_with_joins_sources(table, &mut scope_sources);
                }
            }
            register_assignment_target_sources(assignments, &mut scope_sources);
            scope_sources.register_pseudo_sources(in_trigger);

            let mut count = 0usize;
            for assignment in assignments {
                count += unresolved_references_in_expr(
                    &assignment.value,
                    &scope_sources,
                    dialect,
                    in_trigger,
                );
            }
            if let Some(selection) = selection {
                count +=
                    unresolved_references_in_expr(selection, &scope_sources, dialect, in_trigger);
            }
            if let Some(returning) = returning {
                for item in returning {
                    count += unresolved_references_in_select_item(
                        item,
                        &scope_sources,
                        dialect,
                        in_trigger,
                    );
                }
            }
            count
        }
        Statement::Delete(delete) => {
            let mut scope_sources = inherited_sources.clone();
            let delete_from = match &delete.from {
                FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => tables,
            };
            for table in delete_from {
                register_table_with_joins_sources(table, &mut scope_sources);
            }
            if let Some(using) = &delete.using {
                for table in using {
                    register_table_with_joins_sources(table, &mut scope_sources);
                }
            }
            scope_sources.register_pseudo_sources(in_trigger);

            let mut count = 0usize;
            if let Some(selection) = &delete.selection {
                count +=
                    unresolved_references_in_expr(selection, &scope_sources, dialect, in_trigger);
            }
            if let Some(returning) = &delete.returning {
                for item in returning {
                    count += unresolved_references_in_select_item(
                        item,
                        &scope_sources,
                        dialect,
                        in_trigger,
                    );
                }
            }
            for order_by in &delete.order_by {
                count += unresolved_references_in_expr(
                    &order_by.expr,
                    &scope_sources,
                    dialect,
                    in_trigger,
                );
            }
            if let Some(limit) = &delete.limit {
                count += unresolved_references_in_expr(limit, &scope_sources, dialect, in_trigger);
            }
            count
        }
        Statement::Merge(Merge {
            table,
            source,
            on,
            clauses,
            ..
        }) => {
            let mut scope_sources = inherited_sources.clone();
            register_table_factor_sources(table, &mut scope_sources);
            register_table_factor_sources(source, &mut scope_sources);
            scope_sources.register_pseudo_sources(in_trigger);

            let mut count = unresolved_references_in_expr(on, &scope_sources, dialect, in_trigger);
            count +=
                unresolved_references_in_table_factor(table, &scope_sources, dialect, in_trigger);
            count +=
                unresolved_references_in_table_factor(source, &scope_sources, dialect, in_trigger);

            for clause in clauses {
                if let Some(predicate) = &clause.predicate {
                    count += unresolved_references_in_expr(
                        predicate,
                        &scope_sources,
                        dialect,
                        in_trigger,
                    );
                }
                match &clause.action {
                    MergeAction::Update(MergeUpdateExpr { assignments, .. }) => {
                        for assignment in assignments {
                            count += unresolved_references_in_expr(
                                &assignment.value,
                                &scope_sources,
                                dialect,
                                in_trigger,
                            );
                        }
                    }
                    MergeAction::Insert(insert) => {
                        if let MergeInsertKind::Values(values) = &insert.kind {
                            for row in &values.rows {
                                for expr in row {
                                    count += unresolved_references_in_expr(
                                        expr,
                                        &scope_sources,
                                        dialect,
                                        in_trigger,
                                    );
                                }
                            }
                        }
                    }
                    MergeAction::Delete { .. } => {}
                }
            }

            count
        }
        Statement::CreatePolicy(CreatePolicy {
            table_name,
            using,
            with_check,
            ..
        }) => {
            let mut scope_sources = inherited_sources.clone();
            scope_sources.register_object_name(table_name);
            scope_sources.register_pseudo_sources(in_trigger);

            let mut count = 0usize;
            if let Some(using) = using {
                count += unresolved_references_in_expr(using, &scope_sources, dialect, in_trigger);
            }
            if let Some(with_check) = with_check {
                count +=
                    unresolved_references_in_expr(with_check, &scope_sources, dialect, in_trigger);
            }
            count
        }
        Statement::AlterPolicy(AlterPolicy {
            table_name,
            operation,
            ..
        }) => {
            let mut scope_sources = inherited_sources.clone();
            scope_sources.register_object_name(table_name);
            scope_sources.register_pseudo_sources(in_trigger);

            match operation {
                AlterPolicyOperation::Apply {
                    using, with_check, ..
                } => {
                    let mut count = 0usize;
                    if let Some(using) = using {
                        count += unresolved_references_in_expr(
                            using,
                            &scope_sources,
                            dialect,
                            in_trigger,
                        );
                    }
                    if let Some(with_check) = with_check {
                        count += unresolved_references_in_expr(
                            with_check,
                            &scope_sources,
                            dialect,
                            in_trigger,
                        );
                    }
                    count
                }
                AlterPolicyOperation::Rename { .. } => 0,
            }
        }
        Statement::CreateTrigger(trigger) => {
            let mut scope_sources = inherited_sources.clone();
            scope_sources.register_object_name(&trigger.table_name);
            scope_sources.register_pseudo_sources(true);

            let mut count = 0usize;
            if let Some(condition) = &trigger.condition {
                count += unresolved_references_in_expr(condition, &scope_sources, dialect, true);
            }
            if let Some(statements) = &trigger.statements {
                count += unresolved_references_in_conditional_statements(
                    statements,
                    &scope_sources,
                    dialect,
                    true,
                );
            }
            count
        }
        _ => 0,
    }
}

fn unresolved_references_in_conditional_statements(
    statements: &ConditionalStatements,
    inherited_sources: &SourceRegistry,
    dialect: crate::types::Dialect,
    in_trigger: bool,
) -> usize {
    statements
        .statements()
        .iter()
        .map(|statement| {
            unresolved_references_in_statement(statement, inherited_sources, dialect, in_trigger)
        })
        .sum()
}

fn unresolved_references_in_query(
    query: &Query,
    inherited_sources: &SourceRegistry,
    dialect: crate::types::Dialect,
    in_trigger: bool,
) -> usize {
    let mut count = 0usize;

    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            count +=
                unresolved_references_in_query(&cte.query, inherited_sources, dialect, in_trigger);
        }
    }

    count += unresolved_references_in_set_expr(&query.body, inherited_sources, dialect, in_trigger);

    // ORDER BY lives at the Query level but references columns from the
    // body SELECT's FROM/JOIN scope. Build that scope for validation.
    if let Some(order_by) = &query.order_by {
        if let OrderByKind::Expressions(order_exprs) = &order_by.kind {
            let order_scope = order_by_scope_from_body(&query.body, inherited_sources);
            for order_expr in order_exprs {
                count += unresolved_references_in_expr(
                    &order_expr.expr,
                    &order_scope,
                    dialect,
                    in_trigger,
                );
            }
        }
    }

    count
}

/// Build a source registry for ORDER BY by extracting FROM/JOIN sources
/// from the body SELECT (or both sides of a set operation).
fn order_by_scope_from_body(body: &SetExpr, inherited: &SourceRegistry) -> SourceRegistry {
    let mut scope = inherited.clone();
    match body {
        SetExpr::Select(select) => {
            for from_item in &select.from {
                register_table_with_joins_sources(from_item, &mut scope);
            }
        }
        SetExpr::Query(query) => {
            return order_by_scope_from_body(&query.body, inherited);
        }
        SetExpr::SetOperation { left, .. } => {
            // For UNION/INTERSECT/EXCEPT, ORDER BY references come from the left branch.
            return order_by_scope_from_body(left, inherited);
        }
        _ => {}
    }
    scope
}

fn unresolved_references_in_set_expr(
    set_expr: &SetExpr,
    inherited_sources: &SourceRegistry,
    dialect: crate::types::Dialect,
    in_trigger: bool,
) -> usize {
    match set_expr {
        SetExpr::Select(select) => {
            unresolved_references_in_select(select, inherited_sources, dialect, in_trigger)
        }
        SetExpr::Query(query) => {
            unresolved_references_in_query(query, inherited_sources, dialect, in_trigger)
        }
        SetExpr::SetOperation { left, right, .. } => {
            unresolved_references_in_set_expr(left, inherited_sources, dialect, in_trigger)
                + unresolved_references_in_set_expr(right, inherited_sources, dialect, in_trigger)
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => {
            unresolved_references_in_statement(statement, inherited_sources, dialect, in_trigger)
        }
        _ => 0,
    }
}

fn unresolved_references_in_select(
    select: &Select,
    inherited_sources: &SourceRegistry,
    dialect: crate::types::Dialect,
    in_trigger: bool,
) -> usize {
    let mut scope_sources = inherited_sources.clone();
    for from_item in &select.from {
        register_table_with_joins_sources(from_item, &mut scope_sources);
    }
    scope_sources.register_pseudo_sources(in_trigger);

    let mut count = 0usize;

    if scope_sources.is_empty() {
        return 0;
    }

    for item in &select.projection {
        if let SelectItem::QualifiedWildcard(kind, _) = item {
            count += match kind {
                SelectItemQualifiedWildcardKind::ObjectName(name) => {
                    unresolved_references_in_qualifier_parts(
                        &object_name_parts(name),
                        &scope_sources,
                        dialect,
                    )
                }
                SelectItemQualifiedWildcardKind::Expr(expr) => {
                    unresolved_references_in_expr(expr, &scope_sources, dialect, in_trigger)
                }
            };
        }
    }

    visit_select_expressions(select, &mut |expr| {
        count += unresolved_references_in_expr(expr, &scope_sources, dialect, in_trigger);
    });

    for from_item in &select.from {
        count += unresolved_references_in_table_with_joins(
            from_item,
            &scope_sources,
            dialect,
            in_trigger,
        );
    }

    count
}

fn unresolved_references_in_select_item(
    item: &SelectItem,
    scope_sources: &SourceRegistry,
    dialect: crate::types::Dialect,
    in_trigger: bool,
) -> usize {
    match item {
        SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
            unresolved_references_in_expr(expr, scope_sources, dialect, in_trigger)
        }
        SelectItem::QualifiedWildcard(kind, _) => match kind {
            SelectItemQualifiedWildcardKind::ObjectName(name) => {
                unresolved_references_in_qualifier_parts(
                    &object_name_parts(name),
                    scope_sources,
                    dialect,
                )
            }
            SelectItemQualifiedWildcardKind::Expr(expr) => {
                unresolved_references_in_expr(expr, scope_sources, dialect, in_trigger)
            }
        },
        _ => 0,
    }
}

fn unresolved_references_in_table_with_joins(
    table_with_joins: &TableWithJoins,
    scope_sources: &SourceRegistry,
    dialect: crate::types::Dialect,
    in_trigger: bool,
) -> usize {
    let mut count = unresolved_references_in_table_factor(
        &table_with_joins.relation,
        scope_sources,
        dialect,
        in_trigger,
    );

    for join in &table_with_joins.joins {
        count += unresolved_references_in_table_factor(
            &join.relation,
            scope_sources,
            dialect,
            in_trigger,
        );
        if let Some(on_expr) = join_on_expr(&join.join_operator) {
            count += unresolved_references_in_expr(on_expr, scope_sources, dialect, in_trigger);
        }
    }

    count
}

fn unresolved_references_in_table_factor(
    table_factor: &TableFactor,
    scope_sources: &SourceRegistry,
    dialect: crate::types::Dialect,
    in_trigger: bool,
) -> usize {
    match table_factor {
        TableFactor::Derived {
            lateral, subquery, ..
        } => {
            if *lateral {
                unresolved_references_in_query(subquery, scope_sources, dialect, in_trigger)
            } else {
                unresolved_references_in_query(
                    subquery,
                    &scope_sources.isolated(),
                    dialect,
                    in_trigger,
                )
            }
        }
        TableFactor::TableFunction { expr, .. } => {
            unresolved_references_in_expr(expr, scope_sources, dialect, in_trigger)
        }
        TableFactor::Function { args, .. } => args
            .iter()
            .map(|arg| {
                unresolved_references_in_function_arg(arg, scope_sources, dialect, in_trigger)
            })
            .sum(),
        TableFactor::UNNEST { array_exprs, .. } => array_exprs
            .iter()
            .map(|expr| unresolved_references_in_expr(expr, scope_sources, dialect, in_trigger))
            .sum(),
        TableFactor::JsonTable { json_expr, .. } | TableFactor::OpenJsonTable { json_expr, .. } => {
            unresolved_references_in_expr(json_expr, scope_sources, dialect, in_trigger)
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => unresolved_references_in_table_with_joins(
            table_with_joins,
            scope_sources,
            dialect,
            in_trigger,
        ),
        TableFactor::Pivot {
            table,
            aggregate_functions,
            value_column,
            default_on_null,
            ..
        } => {
            let mut count =
                unresolved_references_in_table_factor(table, scope_sources, dialect, in_trigger);
            for expr_with_alias in aggregate_functions {
                count += unresolved_references_in_expr(
                    &expr_with_alias.expr,
                    scope_sources,
                    dialect,
                    in_trigger,
                );
            }
            for expr in value_column {
                count += unresolved_references_in_expr(expr, scope_sources, dialect, in_trigger);
            }
            if let Some(expr) = default_on_null {
                count += unresolved_references_in_expr(expr, scope_sources, dialect, in_trigger);
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
                unresolved_references_in_table_factor(table, scope_sources, dialect, in_trigger);
            count += unresolved_references_in_expr(value, scope_sources, dialect, in_trigger);
            for expr_with_alias in columns {
                count += unresolved_references_in_expr(
                    &expr_with_alias.expr,
                    scope_sources,
                    dialect,
                    in_trigger,
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
                unresolved_references_in_table_factor(table, scope_sources, dialect, in_trigger);
            for expr in partition_by {
                count += unresolved_references_in_expr(expr, scope_sources, dialect, in_trigger);
            }
            for order in order_by {
                count +=
                    unresolved_references_in_expr(&order.expr, scope_sources, dialect, in_trigger);
            }
            for measure in measures {
                count += unresolved_references_in_expr(
                    &measure.expr,
                    scope_sources,
                    dialect,
                    in_trigger,
                );
            }
            for symbol in symbols {
                count += unresolved_references_in_expr(
                    &symbol.definition,
                    scope_sources,
                    dialect,
                    in_trigger,
                );
            }
            count
        }
        TableFactor::XmlTable { row_expression, .. } => {
            unresolved_references_in_expr(row_expression, scope_sources, dialect, in_trigger)
        }
        _ => 0,
    }
}

fn unresolved_references_in_function_arg(
    arg: &FunctionArg,
    scope_sources: &SourceRegistry,
    dialect: crate::types::Dialect,
    in_trigger: bool,
) -> usize {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
        | FunctionArg::Named {
            arg: FunctionArgExpr::Expr(expr),
            ..
        }
        | FunctionArg::ExprNamed {
            arg: FunctionArgExpr::Expr(expr),
            ..
        } => unresolved_references_in_expr(expr, scope_sources, dialect, in_trigger),
        FunctionArg::Unnamed(FunctionArgExpr::QualifiedWildcard(name))
        | FunctionArg::Named {
            arg: FunctionArgExpr::QualifiedWildcard(name),
            ..
        }
        | FunctionArg::ExprNamed {
            arg: FunctionArgExpr::QualifiedWildcard(name),
            ..
        } => unresolved_references_in_qualifier_parts(
            &object_name_parts(name),
            scope_sources,
            dialect,
        ),
        _ => 0,
    }
}

fn unresolved_references_in_expr(
    expr: &Expr,
    scope_sources: &SourceRegistry,
    dialect: crate::types::Dialect,
    in_trigger: bool,
) -> usize {
    match expr {
        Expr::CompoundIdentifier(parts) if parts.len() > 1 => {
            unresolved_references_in_qualifier_parts(
                &qualifier_parts_from_compound_identifier(parts),
                scope_sources,
                dialect,
            )
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. } => {
            unresolved_references_in_expr(left, scope_sources, dialect, in_trigger)
                + unresolved_references_in_expr(right, scope_sources, dialect, in_trigger)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            unresolved_references_in_expr(inner, scope_sources, dialect, in_trigger)
        }
        Expr::InList { expr, list, .. } => {
            unresolved_references_in_expr(expr, scope_sources, dialect, in_trigger)
                + list
                    .iter()
                    .map(|item| {
                        unresolved_references_in_expr(item, scope_sources, dialect, in_trigger)
                    })
                    .sum::<usize>()
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            unresolved_references_in_expr(expr, scope_sources, dialect, in_trigger)
                + unresolved_references_in_expr(low, scope_sources, dialect, in_trigger)
                + unresolved_references_in_expr(high, scope_sources, dialect, in_trigger)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let mut count = 0usize;
            if let Some(operand) = operand {
                count += unresolved_references_in_expr(operand, scope_sources, dialect, in_trigger);
            }
            for when in conditions {
                count += unresolved_references_in_expr(
                    &when.condition,
                    scope_sources,
                    dialect,
                    in_trigger,
                );
                count +=
                    unresolved_references_in_expr(&when.result, scope_sources, dialect, in_trigger);
            }
            if let Some(otherwise) = else_result {
                count +=
                    unresolved_references_in_expr(otherwise, scope_sources, dialect, in_trigger);
            }
            count
        }
        Expr::Function(function) => {
            let mut count = 0usize;
            if let FunctionArguments::List(arguments) = &function.args {
                for arg in &arguments.args {
                    count += unresolved_references_in_function_arg(
                        arg,
                        scope_sources,
                        dialect,
                        in_trigger,
                    );
                }
            }
            if let Some(filter) = &function.filter {
                count += unresolved_references_in_expr(filter, scope_sources, dialect, in_trigger);
            }
            for order_expr in &function.within_group {
                count += unresolved_references_in_expr(
                    &order_expr.expr,
                    scope_sources,
                    dialect,
                    in_trigger,
                );
            }
            if let Some(sqlparser::ast::WindowType::WindowSpec(spec)) = &function.over {
                for expr in &spec.partition_by {
                    count +=
                        unresolved_references_in_expr(expr, scope_sources, dialect, in_trigger);
                }
                for order_expr in &spec.order_by {
                    count += unresolved_references_in_expr(
                        &order_expr.expr,
                        scope_sources,
                        dialect,
                        in_trigger,
                    );
                }
            }
            count
        }
        Expr::InSubquery { expr, subquery, .. } => {
            unresolved_references_in_expr(expr, scope_sources, dialect, in_trigger)
                + unresolved_references_in_query(subquery, scope_sources, dialect, in_trigger)
        }
        Expr::Subquery(subquery) | Expr::Exists { subquery, .. } => {
            unresolved_references_in_query(subquery, scope_sources, dialect, in_trigger)
        }
        _ => 0,
    }
}

fn unresolved_references_in_qualifier_parts(
    qualifier_parts: &[String],
    scope_sources: &SourceRegistry,
    dialect: crate::types::Dialect,
) -> usize {
    if qualifier_parts.is_empty() {
        return 0;
    }

    if should_resolve_nested_field_reference_from_known_prefix(
        dialect,
        qualifier_parts,
        scope_sources,
    ) {
        return 0;
    }

    if should_defer_struct_field_reference(dialect, qualifier_parts, scope_sources) {
        return 0;
    }

    usize::from(!scope_sources.matches_qualifier(qualifier_parts))
}

fn should_resolve_nested_field_reference_from_known_prefix(
    dialect: crate::types::Dialect,
    qualifier_parts: &[String],
    scope_sources: &SourceRegistry,
) -> bool {
    if !matches!(
        dialect,
        crate::types::Dialect::Bigquery
            | crate::types::Dialect::Duckdb
            | crate::types::Dialect::Hive
            | crate::types::Dialect::Redshift
    ) {
        return false;
    }

    if qualifier_parts.len() < 2 {
        return false;
    }

    scope_sources.matches_qualifier(&[qualifier_parts[0].clone()])
}

fn should_defer_struct_field_reference(
    dialect: crate::types::Dialect,
    qualifier_parts: &[String],
    scope_sources: &SourceRegistry,
) -> bool {
    if scope_sources.force_enable_explicit {
        return false;
    }

    if !matches!(
        dialect,
        crate::types::Dialect::Bigquery
            | crate::types::Dialect::Hive
            | crate::types::Dialect::Redshift
    ) {
        return false;
    }

    qualifier_parts.len() == 1
        && !scope_sources.is_empty()
        && !scope_sources.matches_qualifier(qualifier_parts)
}

fn register_table_with_joins_sources(
    table_with_joins: &TableWithJoins,
    scope_sources: &mut SourceRegistry,
) {
    register_table_factor_sources(&table_with_joins.relation, scope_sources);
    for join in &table_with_joins.joins {
        register_table_factor_sources(&join.relation, scope_sources);
    }
}

fn register_assignment_target_sources(
    assignments: &[Assignment],
    scope_sources: &mut SourceRegistry,
) {
    for assignment in assignments {
        match &assignment.target {
            AssignmentTarget::ColumnName(name) => {
                register_assignment_target_name_prefixes(name, scope_sources);
            }
            AssignmentTarget::Tuple(columns) => {
                for name in columns {
                    register_assignment_target_name_prefixes(name, scope_sources);
                }
            }
        }
    }
}

fn register_assignment_target_name_prefixes(name: &ObjectName, scope_sources: &mut SourceRegistry) {
    let parts = object_name_parts(name);
    if parts.len() < 2 {
        return;
    }

    if let Some(first) = parts.first() {
        scope_sources.register_alias(first);
    }

    let full_prefix = parts[..parts.len() - 1].join(".");
    if !full_prefix.is_empty() {
        scope_sources.exact.insert(full_prefix);
    }
}

fn register_table_factor_sources(table_factor: &TableFactor, scope_sources: &mut SourceRegistry) {
    if let Some(alias) = table_factor_alias_name(table_factor) {
        scope_sources.register_alias(alias);
    }

    match table_factor {
        TableFactor::Table { name, .. } => scope_sources.register_object_name(name),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => register_table_with_joins_sources(table_with_joins, scope_sources),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            register_table_factor_sources(table, scope_sources)
        }
        _ => {}
    }
}

fn object_name_parts(name: &ObjectName) -> Vec<String> {
    let mut parts = Vec::new();
    for part in &name.0 {
        if let Some(ident) = part.as_ident() {
            append_identifier_segments(&ident.value, &mut parts);
        } else {
            append_identifier_segments(&part.to_string(), &mut parts);
        }
    }
    parts
}

fn qualifier_parts_from_compound_identifier(parts: &[Ident]) -> Vec<String> {
    let mut qualifier_parts = Vec::new();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        append_identifier_segments(&part.value, &mut qualifier_parts);
    }
    qualifier_parts
}

fn append_identifier_segments(raw: &str, out: &mut Vec<String>) {
    for segment in raw.split('.') {
        let clean = clean_identifier_component(segment);
        if !clean.is_empty() {
            out.push(clean);
        }
    }
}

fn clean_identifier_component(raw: &str) -> String {
    raw.trim()
        .trim_matches(|ch| matches!(ch, '"' | '`' | '\'' | '[' | ']'))
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;
    use crate::parser::parse_sql_with_dialect;
    use crate::types::Dialect;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesFrom::default();
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn run_in_dialect(sql: &str, dialect: Dialect) -> Vec<Issue> {
        let statements = parse_sql_with_dialect(sql, dialect).expect("parse");
        let rule = ReferencesFrom::default();
        let mut issues = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            issues.extend(rule.check_with_context(
                statement,
                &RuleContext::new(sql, 0..sql.len(), index).with_dialect(dialect),
            ));
        }
        issues
    }

    // --- Edge cases adopted from sqlfluff RF01 ---

    #[test]
    fn flags_unknown_qualifier() {
        let issues = run("SELECT * FROM my_tbl WHERE foo.bar > 0");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_001);
    }

    #[test]
    fn allows_known_table_qualifier() {
        let issues = run("SELECT users.id FROM users");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_nested_subquery_references_that_resolve_locally() {
        let issues = run("SELECT * FROM db.sc.tbl2 WHERE a NOT IN (SELECT a FROM db.sc.tbl1)");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_correlated_subquery_reference_to_outer_source() {
        let issues =
            run("SELECT * FROM tbl2 WHERE a NOT IN (SELECT a FROM tbl1 WHERE tbl2.a = tbl1.a)");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_unresolved_two_part_reference() {
        let issues = run("select * from schema1.agent1 where schema2.agent1.agent_code <> 'abc'");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_simple_delete_statement() {
        let issues = run("delete from table1 where 1 = 1");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_three_part_reference_when_source_is_unqualified() {
        let issues = run("SELECT * FROM agent1 WHERE public.agent1.agent_code <> 'abc'");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_unresolved_reference_in_update_statement() {
        let issues = run("UPDATE my_table SET amount = 1 WHERE my_tableeee.id = my_table.id");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_old_new_outside_sqlite_trigger_context() {
        let issues = run_in_dialect("SELECT old.xyz, new.abc FROM foo", Dialect::Sqlite);
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn allows_bigquery_quoted_qualified_table_reference() {
        let issues = run_in_dialect("SELECT bar.user_id FROM `foo.far.bar`", Dialect::Bigquery);
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_struct_field_style_reference_in_bigquery() {
        let issues = run_in_dialect("SELECT col1.field FROM foo", Dialect::Bigquery);
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_unresolved_reference_in_postgres_create_policy() {
        let issues = run_in_dialect(
            "CREATE POLICY p ON my_table USING (my_tableeee.id = my_table.id)",
            Dialect::Postgres,
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn sparksql_default_mode_skips_explode_nested_field_check() {
        let issues = run_in_dialect(
            "SELECT tbl.a AS a_new, EXPLODE(tbl.b.c) AS a_b_new FROM test AS tbl",
            Dialect::Databricks,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn sparksql_force_enable_flags_explode_nested_field_reference() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.from".to_string(),
                serde_json::json!({"force_enable": true}),
            )]),
        };
        let rule = ReferencesFrom::from_config(&config);
        let sql = "SELECT tbl.a AS a_new, EXPLODE(tbl.b.c) AS a_b_new FROM test AS tbl";
        let statements = parse_sql_with_dialect(sql, Dialect::Databricks).expect("parse");
        let mut issues = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            issues.extend(rule.check_with_context(
                statement,
                &RuleContext::new(sql, 0..sql.len(), index).with_dialect(Dialect::Databricks),
            ));
        }
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_qualified_order_by_from_select_scope() {
        let issues = run("SELECT t.a FROM my_table AS t ORDER BY t.a DESC");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_qualified_order_by_in_cte_query() {
        let issues = run("\
WITH cte AS (SELECT t.a FROM my_table AS t)
SELECT cte.a FROM cte ORDER BY cte.a");
        assert!(issues.is_empty());
    }

    #[test]
    fn force_enable_false_disables_rule() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.from".to_string(),
                serde_json::json!({"force_enable": false}),
            )]),
        };
        let rule = ReferencesFrom::from_config(&config);
        let sql = "SELECT * FROM my_tbl WHERE foo.bar > 0";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }
}
