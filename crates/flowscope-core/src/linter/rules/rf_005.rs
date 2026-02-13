//! LINT_RF_005: References special chars.
//!
//! SQLFluff RF05 parity (current scope): flag identifiers containing disallowed
//! special characters with SQLFluff-style identifier policy/config controls.

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
            .rule_option_str(issue_codes::LINT_RF_005, "unquoted_identifiers_policy")
            .unwrap_or("all")
            .to_ascii_lowercase()
            .as_str()
        {
            "none" => Self::None,
            "aliases" => Self::Aliases,
            "column_aliases" => Self::ColumnAliases,
            "table_aliases" => Self::TableAliases,
            _ => Self::All,
        }
    }

    fn from_quoted_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_RF_005, "quoted_identifiers_policy")
            .unwrap_or("all")
            .to_ascii_lowercase()
            .as_str()
        {
            "none" => Self::None,
            "aliases" => Self::Aliases,
            "column_aliases" => Self::ColumnAliases,
            "table_aliases" => Self::TableAliases,
            _ => Self::All,
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

pub struct ReferencesSpecialChars {
    quoted_policy: IdentifierPolicy,
    unquoted_policy: IdentifierPolicy,
    additional_allowed_characters: HashSet<char>,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
}

impl ReferencesSpecialChars {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            quoted_policy: IdentifierPolicy::from_quoted_config(config),
            unquoted_policy: IdentifierPolicy::from_unquoted_config(config),
            additional_allowed_characters: configured_additional_allowed_characters(config),
            ignore_words: configured_ignore_words(config)
                .into_iter()
                .map(|word| normalize_token(&word))
                .collect(),
            ignore_words_regex: config
                .rule_option_str(issue_codes::LINT_RF_005, "ignore_words_regex")
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

impl Default for ReferencesSpecialChars {
    fn default() -> Self {
        Self {
            quoted_policy: IdentifierPolicy::All,
            unquoted_policy: IdentifierPolicy::All,
            additional_allowed_characters: HashSet::new(),
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
        }
    }
}

impl LintRule for ReferencesSpecialChars {
    fn code(&self) -> &'static str {
        issue_codes::LINT_RF_005
    }

    fn name(&self) -> &'static str {
        "References special chars"
    }

    fn description(&self) -> &'static str {
        "Avoid unsupported special characters in identifiers."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_special_chars = collect_identifier_candidates(statement)
            .into_iter()
            .any(|candidate| candidate_triggers_rule(&candidate, self));

        if has_special_chars {
            vec![Issue::warning(
                issue_codes::LINT_RF_005,
                "Identifier contains unsupported special characters.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn candidate_triggers_rule(candidate: &IdentifierCandidate, rule: &ReferencesSpecialChars) -> bool {
    if is_ignored_token(&candidate.value, rule) {
        return false;
    }

    let policy = if candidate.quoted {
        rule.quoted_policy
    } else {
        rule.unquoted_policy
    };
    if !policy.allows(candidate.kind) {
        return false;
    }

    contains_disallowed_identifier_chars(&candidate.value, &rule.additional_allowed_characters)
}

fn contains_disallowed_identifier_chars(ident: &str, additional_allowed: &HashSet<char>) -> bool {
    ident
        .chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || additional_allowed.contains(&ch)))
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

fn configured_additional_allowed_characters(config: &LintConfig) -> HashSet<char> {
    if let Some(values) =
        config.rule_option_string_list(issue_codes::LINT_RF_005, "additional_allowed_characters")
    {
        let mut chars = HashSet::new();
        for value in values {
            chars.extend(value.chars());
        }
        return chars;
    }

    config
        .rule_option_str(issue_codes::LINT_RF_005, "additional_allowed_characters")
        .map(|value| {
            value
                .split(',')
                .flat_map(|item| item.trim().chars())
                .collect()
        })
        .unwrap_or_default()
}

fn configured_ignore_words(config: &LintConfig) -> Vec<String> {
    if let Some(words) = config.rule_option_string_list(issue_codes::LINT_RF_005, "ignore_words") {
        return words;
    }

    config
        .rule_option_str(issue_codes::LINT_RF_005, "ignore_words")
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

fn is_ignored_token(token: &str, rule: &ReferencesSpecialChars) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        run_with_config(sql, LintConfig::default())
    }

    fn run_with_config(sql: &str, config: LintConfig) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ReferencesSpecialChars::from_config(&config);
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
    fn flags_quoted_identifier_with_hyphen() {
        let issues = run("SELECT \"bad-name\" FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_RF_005);
    }

    #[test]
    fn does_not_flag_quoted_identifier_with_underscore() {
        let issues = run("SELECT \"good_name\" FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_double_quotes_inside_string_literal() {
        let issues = run("SELECT '\"bad-name\"' AS note FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn additional_allowed_characters_permit_hyphen() {
        let issues = run_with_config(
            "SELECT \"bad-name\" FROM t",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "references.special_chars".to_string(),
                    serde_json::json!({"additional_allowed_characters": "-"}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn quoted_policy_none_skips_quoted_identifier_checks() {
        let issues = run_with_config(
            "SELECT \"bad-name\" FROM t",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "LINT_RF_005".to_string(),
                    serde_json::json!({"quoted_identifiers_policy": "none"}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_words_suppresses_configured_identifier() {
        let issues = run_with_config(
            "SELECT \"bad-name\" FROM t",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "references.special_chars".to_string(),
                    serde_json::json!({"ignore_words": ["bad-name"]}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn ignore_words_regex_suppresses_configured_identifier() {
        let issues = run_with_config(
            "SELECT \"bad-name\" FROM t",
            LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "LINT_RF_005".to_string(),
                    serde_json::json!({"ignore_words_regex": "^BAD-"}),
                )]),
            },
        );
        assert!(issues.is_empty());
    }
}
