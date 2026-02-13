//! LINT_RF_004: References keywords.
//!
//! SQLFluff RF04 parity (current scope): avoid keyword-looking identifiers with
//! SQLFluff-style quoted/unquoted identifier-policy controls.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit::visit_expressions;
use crate::types::{issue_codes, Issue};
use regex::{Regex, RegexBuilder};
use sqlparser::ast::{Expr, Ident, Query, SelectItem, SetExpr, Statement, TableAlias, TableFactor};

use super::semantic_helpers::visit_selects_in_statement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IdentifierKind {
    TableAlias,
    ColumnAlias,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IdentifierPolicy {
    None,
    All,
    Aliases,
    ColumnAliases,
    TableAliases,
}

impl IdentifierPolicy {
    fn from_unquoted_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_RF_004, "unquoted_identifiers_policy")
            .unwrap_or("aliases")
            .to_ascii_lowercase()
            .as_str()
        {
            "all" => Self::All,
            "column_aliases" => Self::ColumnAliases,
            "table_aliases" => Self::TableAliases,
            _ => Self::Aliases,
        }
    }

    fn from_quoted_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_RF_004, "quoted_identifiers_policy")
            .unwrap_or("none")
            .to_ascii_lowercase()
            .as_str()
        {
            "all" => Self::All,
            "aliases" => Self::Aliases,
            "column_aliases" => Self::ColumnAliases,
            "table_aliases" => Self::TableAliases,
            _ => Self::None,
        }
    }

    fn allows(self, kind: IdentifierKind) -> bool {
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
}

struct IdentifierCandidate {
    value: String,
    quoted: bool,
    kind: IdentifierKind,
}

pub struct ReferencesKeywords {
    quoted_policy: IdentifierPolicy,
    unquoted_policy: IdentifierPolicy,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
}

impl ReferencesKeywords {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            quoted_policy: IdentifierPolicy::from_quoted_config(config),
            unquoted_policy: IdentifierPolicy::from_unquoted_config(config),
            ignore_words: configured_ignore_words(config)
                .into_iter()
                .map(|word| normalize_token(&word))
                .collect(),
            ignore_words_regex: config
                .rule_option_str(issue_codes::LINT_RF_004, "ignore_words_regex")
                .filter(|pattern| !pattern.trim().is_empty())
                .and_then(|pattern| {
                    RegexBuilder::new(pattern)
                        .case_insensitive(true)
                        .build()
                        .ok()
                }),
        }
    }
}

impl Default for ReferencesKeywords {
    fn default() -> Self {
        Self {
            quoted_policy: IdentifierPolicy::None,
            unquoted_policy: IdentifierPolicy::Aliases,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
        }
    }
}

impl LintRule for ReferencesKeywords {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_004
    }

    fn name(&self) -> &'static str {
        "References keywords"
    }

    fn description(&self) -> &'static str {
        "Avoid keywords as identifiers."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if statement_contains_keyword_identifier(statement, self) {
            vec![
                Issue::info(issue_codes::LINT_RF_004, "Keyword used as identifier.")
                    .with_statement(ctx.statement_index),
            ]
        } else {
            Vec::new()
        }
    }
}

fn statement_contains_keyword_identifier(statement: &Statement, rule: &ReferencesKeywords) -> bool {
    collect_identifier_candidates(statement)
        .into_iter()
        .any(|candidate| candidate_triggers_rule(&candidate, rule))
}

fn candidate_triggers_rule(candidate: &IdentifierCandidate, rule: &ReferencesKeywords) -> bool {
    if is_ignored_token(&candidate.value, rule) || !is_keyword(&candidate.value) {
        return false;
    }

    if candidate.quoted {
        rule.quoted_policy.allows(candidate.kind)
    } else {
        rule.unquoted_policy.allows(candidate.kind)
    }
}

fn collect_identifier_candidates(statement: &Statement) -> Vec<IdentifierCandidate> {
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

fn configured_ignore_words(config: &LintConfig) -> Vec<String> {
    if let Some(words) = config.rule_option_string_list(issue_codes::LINT_RF_004, "ignore_words") {
        return words;
    }

    config
        .rule_option_str(issue_codes::LINT_RF_004, "ignore_words")
        .map(|words| {
            words
                .split(',')
                .map(str::trim)
                .filter(|word| !word.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn is_ignored_token(token: &str, rule: &ReferencesKeywords) -> bool {
    let normalized = normalize_token(token);
    rule.ignore_words.contains(&normalized)
        || rule
            .ignore_words_regex
            .as_ref()
            .is_some_and(|regex| regex.is_match(&normalized))
}

fn normalize_token(token: &str) -> String {
    token
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '`' | '\'' | '[' | ']'))
        .to_ascii_uppercase()
}

fn is_keyword(token: &str) -> bool {
    matches!(
        token.trim().to_ascii_uppercase().as_str(),
        "ALL"
            | "AS"
            | "BY"
            | "CASE"
            | "CROSS"
            | "DISTINCT"
            | "ELSE"
            | "END"
            | "FROM"
            | "FULL"
            | "GROUP"
            | "HAVING"
            | "INNER"
            | "JOIN"
            | "LEFT"
            | "LIMIT"
            | "OFFSET"
            | "ON"
            | "ORDER"
            | "OUTER"
            | "RECURSIVE"
            | "RIGHT"
            | "SELECT"
            | "SUM"
            | "THEN"
            | "UNION"
            | "USING"
            | "WHEN"
            | "WHERE"
            | "WITH"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        run_with_config(sql, LintConfig::default())
    }

    fn run_with_config(sql: &str, config: LintConfig) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesKeywords::from_config(&config);
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
    fn flags_unquoted_keyword_table_alias() {
        let issues = run("SELECT sum.id FROM users AS sum");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn flags_unquoted_keyword_projection_alias() {
        let issues = run("SELECT amount AS sum FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn flags_unquoted_keyword_cte_alias() {
        let issues = run("WITH sum AS (SELECT 1 AS value) SELECT value FROM sum");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn does_not_flag_quoted_keyword_alias_by_default() {
        assert!(run("SELECT \"select\".id FROM users AS \"select\"").is_empty());
    }

    #[test]
    fn does_not_flag_non_keyword_alias() {
        let issues = run("SELECT u.id FROM users AS u");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_sql_like_string_literal() {
        let issues = run("SELECT 'FROM users AS date' AS snippet");
        assert!(issues.is_empty());
    }

    #[test]
    fn quoted_identifiers_policy_all_flags_quoted_keyword_alias() {
        let issues = run_with_config(
            "SELECT \"select\".id FROM users AS \"select\"",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "references.keywords".to_string(),
                    serde_json::json!({"quoted_identifiers_policy": "all"}),
                )]),
            },
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_004);
    }

    #[test]
    fn unquoted_column_alias_policy_does_not_flag_table_alias() {
        let issues = run_with_config(
            "SELECT sum.id FROM users AS sum",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "LINT_RF_004".to_string(),
                    serde_json::json!({"unquoted_identifiers_policy": "column_aliases"}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_words_suppresses_keyword_identifier() {
        let issues = run_with_config(
            "SELECT amount AS sum FROM t",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "references.keywords".to_string(),
                    serde_json::json!({"ignore_words": ["sum"]}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_words_regex_suppresses_keyword_identifier() {
        let issues = run_with_config(
            "SELECT amount AS sum FROM t",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "LINT_RF_004".to_string(),
                    serde_json::json!({"ignore_words_regex": "^s.*"}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }
}
