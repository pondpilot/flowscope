//! LINT_AL_008: Unique column alias.
//!
//! Column aliases should be unique in each SELECT projection.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{
    Expr, Query, Select, SelectItem, SetExpr, Statement, TableFactor, TableWithJoins,
};
use std::collections::HashSet;

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
            .rule_option_str(issue_codes::LINT_AL_008, "alias_case_check")
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
struct ProjectionAlias {
    name: String,
    quoted: bool,
}

pub struct AliasingUniqueColumn {
    alias_case_check: AliasCaseCheck,
}

impl AliasingUniqueColumn {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            alias_case_check: AliasCaseCheck::from_config(config),
        }
    }
}

impl Default for AliasingUniqueColumn {
    fn default() -> Self {
        Self {
            alias_case_check: AliasCaseCheck::Dialect,
        }
    }
}

impl LintRule for AliasingUniqueColumn {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_008
    }

    fn name(&self) -> &'static str {
        "Unique column alias"
    }

    fn description(&self) -> &'static str {
        "Column aliases should be unique in projection lists."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if first_duplicate_column_alias_in_statement(statement, self.alias_case_check).is_none() {
            return Vec::new();
        }

        vec![Issue::warning(
            issue_codes::LINT_AL_008,
            "Column aliases should be unique within SELECT projection.",
        )
        .with_statement(ctx.statement_index)]
    }
}

fn first_duplicate_column_alias_in_statement(
    statement: &Statement,
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    match statement {
        Statement::Query(query) => first_duplicate_column_alias_in_query(query, alias_case_check),
        Statement::Insert(insert) => insert
            .source
            .as_deref()
            .and_then(|query| first_duplicate_column_alias_in_query(query, alias_case_check)),
        Statement::CreateView { query, .. } => {
            first_duplicate_column_alias_in_query(query, alias_case_check)
        }
        Statement::CreateTable(create) => create
            .query
            .as_deref()
            .and_then(|query| first_duplicate_column_alias_in_query(query, alias_case_check)),
        _ => None,
    }
}

fn first_duplicate_column_alias_in_query(
    query: &Query,
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            if let Some(duplicate) =
                first_duplicate_column_alias_in_query(&cte.query, alias_case_check)
            {
                return Some(duplicate);
            }
        }
    }

    first_duplicate_column_alias_in_set_expr(&query.body, alias_case_check)
}

fn first_duplicate_column_alias_in_set_expr(
    set_expr: &SetExpr,
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    match set_expr {
        SetExpr::Select(select) => first_duplicate_column_alias_in_select(select, alias_case_check),
        SetExpr::Query(query) => first_duplicate_column_alias_in_query(query, alias_case_check),
        SetExpr::SetOperation { left, right, .. } => {
            first_duplicate_column_alias_in_set_expr(left, alias_case_check)
                .or_else(|| first_duplicate_column_alias_in_set_expr(right, alias_case_check))
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => {
            first_duplicate_column_alias_in_statement(statement, alias_case_check)
        }
        _ => None,
    }
}

fn first_duplicate_column_alias_in_select(
    select: &Select,
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    let mut aliases = Vec::new();
    for item in &select.projection {
        if let Some(alias) = projected_column_alias(item) {
            aliases.push(alias);
        }
    }

    if let Some(duplicate) = first_duplicate_alias(&aliases, alias_case_check) {
        return Some(duplicate);
    }

    for table_with_joins in &select.from {
        if let Some(duplicate) = first_duplicate_column_alias_in_table_with_joins_children(
            table_with_joins,
            alias_case_check,
        ) {
            return Some(duplicate);
        }
    }

    None
}

fn projected_column_alias(item: &SelectItem) -> Option<ProjectionAlias> {
    match item {
        SelectItem::ExprWithAlias { alias, .. } => Some(ProjectionAlias {
            name: alias.value.clone(),
            quoted: alias.quote_style.is_some(),
        }),
        SelectItem::UnnamedExpr(Expr::Identifier(identifier)) => Some(ProjectionAlias {
            name: identifier.value.clone(),
            quoted: identifier.quote_style.is_some(),
        }),
        SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) => {
            parts.last().map(|part| ProjectionAlias {
                name: part.value.clone(),
                quoted: part.quote_style.is_some(),
            })
        }
        _ => None,
    }
}

fn first_duplicate_column_alias_in_table_with_joins_children(
    table_with_joins: &TableWithJoins,
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    first_duplicate_column_alias_in_table_factor_children(
        &table_with_joins.relation,
        alias_case_check,
    )
    .or_else(|| {
        for join in &table_with_joins.joins {
            if let Some(duplicate) = first_duplicate_column_alias_in_table_factor_children(
                &join.relation,
                alias_case_check,
            ) {
                return Some(duplicate);
            }
        }
        None
    })
}

fn first_duplicate_column_alias_in_table_factor_children(
    table_factor: &TableFactor,
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    match table_factor {
        TableFactor::Derived { subquery, .. } => {
            first_duplicate_column_alias_in_query(subquery, alias_case_check)
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => first_duplicate_column_alias_in_nested_scope(table_with_joins, alias_case_check),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            first_duplicate_column_alias_in_table_factor_children(table, alias_case_check)
        }
        _ => None,
    }
}

fn first_duplicate_column_alias_in_nested_scope(
    table_with_joins: &TableWithJoins,
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    first_duplicate_column_alias_in_table_with_joins_children(table_with_joins, alias_case_check)
}

fn first_duplicate_alias(
    values: &[ProjectionAlias],
    alias_case_check: AliasCaseCheck,
) -> Option<String> {
    let mut seen: Vec<&ProjectionAlias> = Vec::new();
    let mut seen_case_insensitive = HashSet::new();

    for value in values {
        // Fast-path for the common case-insensitive mode.
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

fn aliases_match(
    left: &ProjectionAlias,
    right: &ProjectionAlias,
    alias_case_check: AliasCaseCheck,
) -> bool {
    match alias_case_check {
        AliasCaseCheck::CaseInsensitive => left.name.eq_ignore_ascii_case(&right.name),
        AliasCaseCheck::CaseSensitive => left.name == right.name,
        AliasCaseCheck::Dialect
        | AliasCaseCheck::QuotedCsNakedUpper
        | AliasCaseCheck::QuotedCsNakedLower => {
            if left.quoted || right.quoted {
                left.name == right.name
            } else {
                left.name.eq_ignore_ascii_case(&right.name)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingUniqueColumn::default();
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
    fn flags_duplicate_projection_alias() {
        let issues = run("select a as x, b as x from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_008);
    }

    #[test]
    fn allows_unique_projection_aliases() {
        let issues = run("select a as x, b as y from t");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_same_alias_in_different_cte_scopes() {
        let sql = "with a as (select col as x from t1), b as (select col as x from t2) select * from a join b on a.x = b.x";
        let issues = run(sql);
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_duplicate_alias_in_nested_subquery() {
        let sql = "select * from (select a as x, b as x from t) s";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_duplicate_unaliased_column_reference() {
        let issues = run("select foo, foo from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_008);
    }

    #[test]
    fn flags_alias_collision_with_unaliased_reference() {
        let issues = run("select foo, a as foo from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_008);
    }

    #[test]
    fn default_dialect_mode_does_not_flag_quoted_case_mismatch() {
        let issues = run("select \"A\", a from t");
        assert!(issues.is_empty());
    }

    #[test]
    fn alias_case_check_case_sensitive_allows_case_mismatch() {
        let sql = "select a, A from t";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingUniqueColumn::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unique.column".to_string(),
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
        let sql = "select a, a from t";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingUniqueColumn::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_AL_008".to_string(),
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
    }
}
