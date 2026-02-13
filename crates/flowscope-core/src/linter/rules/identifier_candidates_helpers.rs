//! Shared helpers for AST identifier candidate collection and identifier-policy
//! filtering used by reference/capitalisation rules.

use crate::linter::config::LintConfig;
use crate::linter::visit::visit_expressions;
use sqlparser::ast::{
    Assignment, AssignmentTarget, Expr, Ident, ObjectName, Query, SelectItem, SetExpr, Statement,
    TableAlias, TableFactor,
};

use super::semantic_helpers::visit_selects_in_statement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IdentifierKind {
    TableAlias,
    ColumnAlias,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IdentifierPolicy {
    None,
    All,
    Aliases,
    ColumnAliases,
    TableAliases,
}

impl IdentifierPolicy {
    pub(crate) fn from_config(config: &LintConfig, code: &str, key: &str, default: &str) -> Self {
        config
            .rule_option_str(code, key)
            .and_then(Self::parse_raw)
            .or_else(|| Self::parse_raw(default))
            .unwrap_or(Self::All)
    }

    pub(crate) fn allows(self, kind: IdentifierKind) -> bool {
        match self {
            Self::None => false,
            Self::All => true,
            Self::Aliases => matches!(
                kind,
                IdentifierKind::TableAlias | IdentifierKind::ColumnAlias
            ),
            Self::ColumnAliases => kind == IdentifierKind::ColumnAlias,
            Self::TableAliases => kind == IdentifierKind::TableAlias,
        }
    }

    fn parse_raw(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "all" => Some(Self::All),
            "aliases" => Some(Self::Aliases),
            "column_aliases" => Some(Self::ColumnAliases),
            "table_aliases" => Some(Self::TableAliases),
            _ => None,
        }
    }
}

pub(crate) struct IdentifierCandidate {
    pub(crate) value: String,
    pub(crate) quoted: bool,
    pub(crate) kind: IdentifierKind,
}

pub(crate) fn collect_identifier_candidates(statement: &Statement) -> Vec<IdentifierCandidate> {
    let mut candidates = Vec::new();

    visit_expressions(statement, &mut |expr| match expr {
        Expr::Identifier(ident) => {
            push_ident_candidate(ident, IdentifierKind::Other, &mut candidates);
        }
        Expr::CompoundIdentifier(parts) => {
            for part in parts {
                push_ident_candidate(part, IdentifierKind::Other, &mut candidates);
            }
        }
        Expr::Function(function) => {
            if let sqlparser::ast::FunctionArguments::List(arguments) = &function.args {
                for arg in &arguments.args {
                    if let sqlparser::ast::FunctionArg::Named { name, .. } = arg {
                        push_ident_candidate(name, IdentifierKind::Other, &mut candidates);
                    }
                }
            }
        }
        _ => {}
    });

    visit_selects_in_statement(statement, &mut |select| {
        for item in &select.projection {
            if let SelectItem::ExprWithAlias { alias, .. } = item {
                push_ident_candidate(alias, IdentifierKind::ColumnAlias, &mut candidates);
            }
        }

        for table in &select.from {
            collect_table_factor_identifiers(&table.relation, &mut candidates);
            for join in &table.joins {
                collect_table_factor_identifiers(&join.relation, &mut candidates);
            }
        }
    });

    collect_cte_identifiers_in_statement(statement, &mut candidates);
    collect_show_statement_identifiers(statement, &mut candidates);
    collect_assignment_target_identifiers(statement, &mut candidates);
    candidates
}

fn collect_table_factor_identifiers(
    table_factor: &TableFactor,
    candidates: &mut Vec<IdentifierCandidate>,
) {
    if let Some(alias) = table_factor_alias(table_factor) {
        push_ident_candidate(&alias.name, IdentifierKind::TableAlias, candidates);
        for column in &alias.columns {
            push_ident_candidate(&column.name, IdentifierKind::ColumnAlias, candidates);
        }
    }

    match table_factor {
        TableFactor::Table { name, .. } => {
            for part in &name.0 {
                if let Some(ident) = part.as_ident() {
                    push_ident_candidate(ident, IdentifierKind::Other, candidates);
                }
            }
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_table_factor_identifiers(&table_with_joins.relation, candidates);
            for join in &table_with_joins.joins {
                collect_table_factor_identifiers(&join.relation, candidates);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_table_factor_identifiers(table, candidates);
        }
        _ => {}
    }
}

fn collect_cte_identifiers_in_statement(
    statement: &Statement,
    candidates: &mut Vec<IdentifierCandidate>,
) {
    match statement {
        Statement::Query(query) => collect_cte_identifiers_in_query(query, candidates),
        Statement::Insert(insert) => {
            if let Some(source) = &insert.source {
                collect_cte_identifiers_in_query(source, candidates);
            }
        }
        Statement::CreateView { query, .. } => collect_cte_identifiers_in_query(query, candidates),
        Statement::CreateTable(create) => {
            if let Some(query) = &create.query {
                collect_cte_identifiers_in_query(query, candidates);
            }
        }
        _ => {}
    }
}

fn collect_show_statement_identifiers(
    statement: &Statement,
    candidates: &mut Vec<IdentifierCandidate>,
) {
    let Statement::ShowVariable { variable } = statement else {
        return;
    };

    // Databricks/SparkSQL `SHOW TBLPROPERTIES <table> (<property.path>)` is
    // represented as a flat identifier list: `TBLPROPERTIES`, `<table>`,
    // `<property>`, ...
    let Some(first) = variable.first() else {
        return;
    };
    if !first.value.eq_ignore_ascii_case("TBLPROPERTIES") {
        return;
    }

    for ident in variable.iter().skip(1) {
        push_ident_candidate(ident, IdentifierKind::Other, candidates);
    }
}

fn collect_assignment_target_identifiers(
    statement: &Statement,
    candidates: &mut Vec<IdentifierCandidate>,
) {
    match statement {
        Statement::Insert(insert) => collect_assignment_targets(&insert.assignments, candidates),
        Statement::Update { assignments, .. } => collect_assignment_targets(assignments, candidates),
        Statement::Merge { clauses, .. } => {
            for clause in clauses {
                if let sqlparser::ast::MergeAction::Update { assignments } = &clause.action {
                    collect_assignment_targets(assignments, candidates);
                }
            }
        }
        _ => {}
    }
}

fn collect_assignment_targets(assignments: &[Assignment], candidates: &mut Vec<IdentifierCandidate>) {
    for assignment in assignments {
        match &assignment.target {
            AssignmentTarget::ColumnName(name) => {
                collect_object_name_idents(name, IdentifierKind::Other, candidates)
            }
            AssignmentTarget::Tuple(names) => {
                for name in names {
                    collect_object_name_idents(name, IdentifierKind::Other, candidates);
                }
            }
        }
    }
}

fn collect_object_name_idents(
    name: &ObjectName,
    kind: IdentifierKind,
    candidates: &mut Vec<IdentifierCandidate>,
) {
    for part in &name.0 {
        if let Some(ident) = part.as_ident() {
            push_ident_candidate(ident, kind, candidates);
        }
    }
}

fn collect_cte_identifiers_in_query(query: &Query, candidates: &mut Vec<IdentifierCandidate>) {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            push_ident_candidate(&cte.alias.name, IdentifierKind::TableAlias, candidates);
            for column in &cte.alias.columns {
                push_ident_candidate(&column.name, IdentifierKind::ColumnAlias, candidates);
            }
            collect_cte_identifiers_in_query(&cte.query, candidates);
        }
    }

    collect_cte_identifiers_in_set_expr(&query.body, candidates);
}

fn collect_cte_identifiers_in_set_expr(
    set_expr: &SetExpr,
    candidates: &mut Vec<IdentifierCandidate>,
) {
    match set_expr {
        SetExpr::Query(query) => collect_cte_identifiers_in_query(query, candidates),
        SetExpr::SetOperation { left, right, .. } => {
            collect_cte_identifiers_in_set_expr(left, candidates);
            collect_cte_identifiers_in_set_expr(right, candidates);
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => collect_cte_identifiers_in_statement(statement, candidates),
        _ => {}
    }
}

fn push_ident_candidate(
    ident: &Ident,
    kind: IdentifierKind,
    candidates: &mut Vec<IdentifierCandidate>,
) {
    candidates.push(IdentifierCandidate {
        value: ident.value.clone(),
        quoted: ident.quote_style.is_some(),
        kind,
    });
}

fn table_factor_alias(table_factor: &TableFactor) -> Option<&TableAlias> {
    match table_factor {
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
    }
}
