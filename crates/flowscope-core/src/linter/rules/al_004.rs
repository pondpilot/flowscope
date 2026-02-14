//! LINT_AL_004: Unique table alias.
//!
//! Table aliases should be unique within a query scope.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{
    Expr, FunctionArg, FunctionArgExpr, FunctionArguments, Query, Select, SetExpr, Statement,
    TableFactor, TableWithJoins, WindowType,
};
use std::collections::HashSet;

use super::semantic_helpers::visit_select_expressions;

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
            .rule_option_str(issue_codes::LINT_AL_004, "alias_case_check")
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct AliasRef {
    name: String,
    quoted: bool,
}

pub struct AliasingUniqueTable {
    alias_case_check: AliasCaseCheck,
}

impl AliasingUniqueTable {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            alias_case_check: AliasCaseCheck::from_config(config),
        }
    }
}

impl Default for AliasingUniqueTable {
    fn default() -> Self {
        Self {
            alias_case_check: AliasCaseCheck::Dialect,
        }
    }
}

impl LintRule for AliasingUniqueTable {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_004
    }

    fn name(&self) -> &'static str {
        "Unique table alias"
    }

    fn description(&self) -> &'static str {
        "Table aliases should be unique within a statement."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if first_duplicate_table_alias_in_statement(statement, self.alias_case_check).is_none() {
            return Vec::new();
        }

        vec![Issue::warning(
            issue_codes::LINT_AL_004,
            "Table aliases should be unique within a statement.",
        )
        .with_statement(ctx.statement_index)]
    }
}

fn first_duplicate_table_alias_in_statement(
    statement: &Statement,
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    match statement {
        Statement::Query(query) => {
            first_duplicate_table_alias_in_query_with_parent(query, &[], alias_case_check)
        }
        Statement::Insert(insert) => insert.source.as_deref().and_then(|query| {
            first_duplicate_table_alias_in_query_with_parent(query, &[], alias_case_check)
        }),
        Statement::CreateView { query, .. } => {
            first_duplicate_table_alias_in_query_with_parent(query, &[], alias_case_check)
        }
        Statement::CreateTable(create) => create.query.as_deref().and_then(|query| {
            first_duplicate_table_alias_in_query_with_parent(query, &[], alias_case_check)
        }),
        _ => None,
    }
}

fn first_duplicate_table_alias_in_query_with_parent(
    query: &Query,
    parent_aliases: &[AliasRef],
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            if let Some(duplicate) =
                first_duplicate_table_alias_in_query_with_parent(&cte.query, &[], alias_case_check)
            {
                return Some(duplicate);
            }
        }
    }

    first_duplicate_table_alias_in_set_expr_with_parent(
        &query.body,
        parent_aliases,
        alias_case_check,
    )
}

fn first_duplicate_table_alias_in_set_expr_with_parent(
    set_expr: &SetExpr,
    parent_aliases: &[AliasRef],
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    match set_expr {
        SetExpr::Select(select) => first_duplicate_table_alias_in_select_with_parent(
            select,
            parent_aliases,
            alias_case_check,
        ),
        SetExpr::Query(query) => first_duplicate_table_alias_in_query_with_parent(
            query,
            parent_aliases,
            alias_case_check,
        ),
        SetExpr::SetOperation { left, right, .. } => {
            first_duplicate_table_alias_in_set_expr_with_parent(
                left,
                parent_aliases,
                alias_case_check,
            )
            .or_else(|| {
                first_duplicate_table_alias_in_set_expr_with_parent(
                    right,
                    parent_aliases,
                    alias_case_check,
                )
            })
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => {
            first_duplicate_table_alias_in_statement(statement, alias_case_check)
        }
        _ => None,
    }
}

fn first_duplicate_table_alias_in_select_with_parent(
    select: &Select,
    parent_aliases: &[AliasRef],
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    let mut aliases = Vec::new();
    for table_with_joins in &select.from {
        collect_scope_table_aliases(table_with_joins, &mut aliases);
    }

    let mut aliases_with_parent = parent_aliases.to_vec();
    aliases_with_parent.extend(aliases);

    if let Some(duplicate) = first_duplicate_alias(&aliases_with_parent, alias_case_check) {
        return Some(duplicate);
    }

    if let Some(duplicate) = first_duplicate_table_alias_in_select_expression_subqueries(
        select,
        &aliases_with_parent,
        alias_case_check,
    ) {
        return Some(duplicate);
    }

    for table_with_joins in &select.from {
        if let Some(duplicate) = first_duplicate_table_alias_in_table_with_joins_children(
            table_with_joins,
            &aliases_with_parent,
            alias_case_check,
        ) {
            return Some(duplicate);
        }
    }

    None
}

fn first_duplicate_table_alias_in_select_expression_subqueries(
    select: &Select,
    parent_aliases: &[AliasRef],
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    let mut duplicate = None;
    visit_select_expressions(select, &mut |expr| {
        if duplicate.is_none() {
            duplicate = first_duplicate_table_alias_in_expr_with_parent(
                expr,
                parent_aliases,
                alias_case_check,
            );
        }
    });
    duplicate
}

fn first_duplicate_table_alias_in_expr_with_parent(
    expr: &Expr,
    parent_aliases: &[AliasRef],
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    match expr {
        Expr::Subquery(query)
        | Expr::Exists {
            subquery: query, ..
        } => first_duplicate_table_alias_in_query_with_parent(
            query,
            parent_aliases,
            alias_case_check,
        ),
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => {
            first_duplicate_table_alias_in_expr_with_parent(inner, parent_aliases, alias_case_check)
                .or_else(|| {
                    first_duplicate_table_alias_in_query_with_parent(
                        subquery,
                        parent_aliases,
                        alias_case_check,
                    )
                })
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. } => {
            first_duplicate_table_alias_in_expr_with_parent(left, parent_aliases, alias_case_check)
                .or_else(|| {
                    first_duplicate_table_alias_in_expr_with_parent(
                        right,
                        parent_aliases,
                        alias_case_check,
                    )
                })
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            first_duplicate_table_alias_in_expr_with_parent(inner, parent_aliases, alias_case_check)
        }
        Expr::InList { expr, list, .. } => {
            first_duplicate_table_alias_in_expr_with_parent(expr, parent_aliases, alias_case_check)
                .or_else(|| {
                    for item in list {
                        if let Some(duplicate) = first_duplicate_table_alias_in_expr_with_parent(
                            item,
                            parent_aliases,
                            alias_case_check,
                        ) {
                            return Some(duplicate);
                        }
                    }
                    None
                })
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            first_duplicate_table_alias_in_expr_with_parent(expr, parent_aliases, alias_case_check)
                .or_else(|| {
                    first_duplicate_table_alias_in_expr_with_parent(
                        low,
                        parent_aliases,
                        alias_case_check,
                    )
                })
                .or_else(|| {
                    first_duplicate_table_alias_in_expr_with_parent(
                        high,
                        parent_aliases,
                        alias_case_check,
                    )
                })
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand {
                if let Some(duplicate) = first_duplicate_table_alias_in_expr_with_parent(
                    operand,
                    parent_aliases,
                    alias_case_check,
                ) {
                    return Some(duplicate);
                }
            }
            for when in conditions {
                if let Some(duplicate) = first_duplicate_table_alias_in_expr_with_parent(
                    &when.condition,
                    parent_aliases,
                    alias_case_check,
                ) {
                    return Some(duplicate);
                }
                if let Some(duplicate) = first_duplicate_table_alias_in_expr_with_parent(
                    &when.result,
                    parent_aliases,
                    alias_case_check,
                ) {
                    return Some(duplicate);
                }
            }
            if let Some(otherwise) = else_result {
                return first_duplicate_table_alias_in_expr_with_parent(
                    otherwise,
                    parent_aliases,
                    alias_case_check,
                );
            }
            None
        }
        Expr::Function(function) => {
            if let FunctionArguments::List(arguments) = &function.args {
                for arg in &arguments.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(expr),
                            ..
                        } => {
                            if let Some(duplicate) = first_duplicate_table_alias_in_expr_with_parent(
                                expr,
                                parent_aliases,
                                alias_case_check,
                            ) {
                                return Some(duplicate);
                            }
                        }
                        _ => {}
                    }
                }
            }

            if let Some(filter) = &function.filter {
                if let Some(duplicate) = first_duplicate_table_alias_in_expr_with_parent(
                    filter,
                    parent_aliases,
                    alias_case_check,
                ) {
                    return Some(duplicate);
                }
            }

            for order_expr in &function.within_group {
                if let Some(duplicate) = first_duplicate_table_alias_in_expr_with_parent(
                    &order_expr.expr,
                    parent_aliases,
                    alias_case_check,
                ) {
                    return Some(duplicate);
                }
            }

            if let Some(WindowType::WindowSpec(spec)) = &function.over {
                for expr in &spec.partition_by {
                    if let Some(duplicate) = first_duplicate_table_alias_in_expr_with_parent(
                        expr,
                        parent_aliases,
                        alias_case_check,
                    ) {
                        return Some(duplicate);
                    }
                }
                for order_expr in &spec.order_by {
                    if let Some(duplicate) = first_duplicate_table_alias_in_expr_with_parent(
                        &order_expr.expr,
                        parent_aliases,
                        alias_case_check,
                    ) {
                        return Some(duplicate);
                    }
                }
            }

            None
        }
        _ => None,
    }
}

fn collect_scope_table_aliases(table_with_joins: &TableWithJoins, aliases: &mut Vec<AliasRef>) {
    collect_scope_table_aliases_from_factor(&table_with_joins.relation, aliases);
    for join in &table_with_joins.joins {
        collect_scope_table_aliases_from_factor(&join.relation, aliases);
    }
}

fn collect_scope_table_aliases_from_factor(
    table_factor: &TableFactor,
    aliases: &mut Vec<AliasRef>,
) {
    if let Some(alias) = inferred_alias_name(table_factor) {
        aliases.push(alias);
    }

    match table_factor {
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => collect_scope_table_aliases(table_with_joins, aliases),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_scope_table_aliases_from_factor(table, aliases)
        }
        _ => {}
    }
}

fn inferred_alias_name(table_factor: &TableFactor) -> Option<AliasRef> {
    if let Some(alias) = explicit_alias_name(table_factor) {
        return Some(alias);
    }

    match table_factor {
        TableFactor::Table { name, .. } => name.0.last().map(|part| {
            if let Some(ident) = part.as_ident() {
                AliasRef {
                    name: ident.value.clone(),
                    quoted: ident.quote_style.is_some(),
                }
            } else {
                AliasRef {
                    name: part.to_string(),
                    quoted: false,
                }
            }
        }),
        _ => None,
    }
}

fn explicit_alias_name(table_factor: &TableFactor) -> Option<AliasRef> {
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

    Some(AliasRef {
        name: alias.name.value.clone(),
        quoted: alias.name.quote_style.is_some(),
    })
}

fn first_duplicate_table_alias_in_table_with_joins_children(
    table_with_joins: &TableWithJoins,
    parent_aliases: &[AliasRef],
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    first_duplicate_table_alias_in_table_factor_children(
        &table_with_joins.relation,
        parent_aliases,
        alias_case_check,
    )
    .or_else(|| {
        for join in &table_with_joins.joins {
            if let Some(duplicate) = first_duplicate_table_alias_in_table_factor_children(
                &join.relation,
                parent_aliases,
                alias_case_check,
            ) {
                return Some(duplicate);
            }
        }
        None
    })
}

fn child_parent_aliases(
    parent_aliases: &[AliasRef],
    table_factor: &TableFactor,
    alias_case_check: AliasCaseCheck,
) -> Vec<AliasRef> {
    let mut next = parent_aliases.to_vec();
    if let Some(alias) = inferred_alias_name(table_factor) {
        if let Some(index) = next
            .iter()
            .position(|existing| aliases_match(existing, &alias, alias_case_check))
        {
            next.remove(index);
        }
    }
    next
}

fn first_duplicate_table_alias_in_table_factor_children(
    table_factor: &TableFactor,
    parent_aliases: &[AliasRef],
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    let child_parent_aliases = child_parent_aliases(parent_aliases, table_factor, alias_case_check);

    match table_factor {
        TableFactor::Derived { subquery, .. } => first_duplicate_table_alias_in_query_with_parent(
            subquery,
            &child_parent_aliases,
            alias_case_check,
        ),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => first_duplicate_table_alias_in_nested_scope(
            table_with_joins,
            &child_parent_aliases,
            alias_case_check,
        ),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            first_duplicate_table_alias_in_table_factor_children(
                table,
                &child_parent_aliases,
                alias_case_check,
            )
        }
        _ => None,
    }
}

fn first_duplicate_table_alias_in_nested_scope(
    table_with_joins: &TableWithJoins,
    parent_aliases: &[AliasRef],
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    let mut aliases = Vec::new();
    collect_scope_table_aliases(table_with_joins, &mut aliases);
    let mut aliases_with_parent = parent_aliases.to_vec();
    aliases_with_parent.extend(aliases);

    if let Some(duplicate) = first_duplicate_alias(&aliases_with_parent, alias_case_check) {
        return Some(duplicate);
    }

    first_duplicate_table_alias_in_table_with_joins_children(
        table_with_joins,
        &aliases_with_parent,
        alias_case_check,
    )
}

fn first_duplicate_alias(values: &[AliasRef], alias_case_check: AliasCaseCheck) -> Option<String> {
    let mut seen_case_insensitive = HashSet::new();
    let mut seen: Vec<&AliasRef> = Vec::new();

    for value in values {
        if matches!(alias_case_check, AliasCaseCheck::CaseInsensitive) {
            let key = value.name.to_ascii_uppercase();
            if !seen_case_insensitive.insert(key) {
                return Some(value.name.clone());
            }
            continue;
        }

        let is_duplicate = seen
            .iter()
            .any(|existing| aliases_match(existing, value, alias_case_check));
        if is_duplicate {
            return Some(value.name.clone());
        }
        seen.push(value);
    }

    None
}

fn aliases_match(left: &AliasRef, right: &AliasRef, alias_case_check: AliasCaseCheck) -> bool {
    match alias_case_check {
        AliasCaseCheck::CaseInsensitive => left.name.eq_ignore_ascii_case(&right.name),
        AliasCaseCheck::CaseSensitive => left.name == right.name,
        AliasCaseCheck::Dialect => {
            if left.quoted || right.quoted {
                left.name == right.name
            } else {
                left.name.eq_ignore_ascii_case(&right.name)
            }
        }
        AliasCaseCheck::QuotedCsNakedUpper | AliasCaseCheck::QuotedCsNakedLower => {
            normalize_alias_for_mode(left, alias_case_check)
                == normalize_alias_for_mode(right, alias_case_check)
        }
    }
}

fn normalize_alias_for_mode(alias: &AliasRef, mode: AliasCaseCheck) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingUniqueTable::default();
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

    #[test]
    fn flags_duplicate_alias_in_same_scope() {
        let issues = run("select * from users u join orders u on u.id = u.user_id");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_004);
    }

    #[test]
    fn allows_unique_aliases() {
        let issues = run("select * from users u join orders o on u.id = o.user_id");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_same_alias_in_separate_cte_scopes() {
        let sql = "with a as (select * from users u), b as (select * from orders u) select * from a join b on a.id = b.id";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_duplicate_alias_in_nested_subquery() {
        let sql = "select * from (select * from users u join orders u on u.id = u.user_id) t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_duplicate_implicit_table_name_aliases() {
        let sql =
            "select * from analytics.foo join reporting.foo on analytics.foo.id = reporting.foo.id";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_004);
    }

    #[test]
    fn flags_duplicate_alias_between_parent_and_subquery_scope() {
        let sql = "select * from (select * from users a) s join orders a on s.id = a.user_id";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_004);
    }

    #[test]
    fn flags_duplicate_alias_between_parent_and_where_subquery_scope() {
        let sql = "select * from tbl as t where t.val in (select t.val from tbl2 as t)";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_004);
    }

    #[test]
    fn flags_implicit_table_name_alias_collision_in_where_subquery() {
        let sql = "select * from tbl where val in (select tbl.val from tbl2 as tbl)";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_004);
    }

    #[test]
    fn flags_implicit_table_name_collision_in_where_subquery() {
        let sql = "select * from tbl where val in (select tbl.val from tbl)";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_004);
    }

    #[test]
    fn does_not_treat_subquery_alias_as_parent_alias_conflict() {
        let sql = "select * from (select * from users s) s";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn default_dialect_mode_does_not_flag_quoted_case_mismatch() {
        let sql = "select * from users \"A\" join orders a on \"A\".id = a.user_id";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn alias_case_check_case_sensitive_allows_case_mismatch() {
        let sql = "select * from users a join orders A on a.id = A.user_id";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingUniqueTable::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unique.table".to_string(),
                serde_json::json!({"alias_case_check": "case_sensitive"}),
            )]),
        });
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
    fn alias_case_check_case_sensitive_flags_exact_duplicates() {
        let sql = "select * from users a join orders a on a.id = a.user_id";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingUniqueTable::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_AL_004".to_string(),
                serde_json::json!({"alias_case_check": "case_sensitive"}),
            )]),
        });
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_004);
    }

    #[test]
    fn alias_case_check_quoted_cs_naked_upper_flags_upper_fold_match() {
        let sql = "select * from users \"FOO\" join orders foo on \"FOO\".id = foo.user_id";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingUniqueTable::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unique.table".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_upper"}),
            )]),
        });
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_004);
    }

    #[test]
    fn alias_case_check_quoted_cs_naked_upper_allows_nonmatching_quoted_case() {
        let sql = "select * from users \"foo\" join orders foo on \"foo\".id = foo.user_id";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingUniqueTable::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unique.table".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_upper"}),
            )]),
        });
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
    fn alias_case_check_quoted_cs_naked_lower_flags_lower_fold_match() {
        let sql = "select * from users \"foo\" join orders FOO on \"foo\".id = FOO.user_id";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingUniqueTable::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unique.table".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_lower"}),
            )]),
        });
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_004);
    }

    #[test]
    fn alias_case_check_quoted_cs_naked_lower_allows_nonmatching_quoted_case() {
        let sql = "select * from users \"FOO\" join orders FOO on \"FOO\".id = FOO.user_id";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingUniqueTable::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unique.table".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_lower"}),
            )]),
        });
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
}
