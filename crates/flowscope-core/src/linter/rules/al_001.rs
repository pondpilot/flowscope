//! LINT_AL_001: Table alias style.
//!
//! SQLFluff parity: configurable table aliasing style (`explicit`/`implicit`).

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Ident, Query, SetExpr, Statement, TableFactor, TableWithJoins};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AliasingPreference {
    Explicit,
    Implicit,
}

impl AliasingPreference {
    fn from_config(config: &LintConfig, rule_code: &str) -> Self {
        match config
            .rule_option_str(rule_code, "aliasing")
            .unwrap_or("explicit")
            .to_ascii_lowercase()
            .as_str()
        {
            "implicit" => Self::Implicit,
            _ => Self::Explicit,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Explicit => "Use explicit AS when aliasing tables.",
            Self::Implicit => "Use implicit aliasing when aliasing tables (omit AS).",
        }
    }

    fn violation(self, explicit_as: bool) -> bool {
        match self {
            Self::Explicit => !explicit_as,
            Self::Implicit => explicit_as,
        }
    }
}

pub struct AliasingTableStyle {
    aliasing: AliasingPreference,
}

impl AliasingTableStyle {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            aliasing: AliasingPreference::from_config(config, issue_codes::LINT_AL_001),
        }
    }
}

impl Default for AliasingTableStyle {
    fn default() -> Self {
        Self {
            aliasing: AliasingPreference::Explicit,
        }
    }
}

impl LintRule for AliasingTableStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_001
    }

    fn name(&self) -> &'static str {
        "Table alias style"
    }

    fn description(&self) -> &'static str {
        "Implicit/explicit aliasing of table."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();

        collect_table_aliases_in_statement(statement, &mut |alias| {
            let Some(occurrence) = alias_occurrence_in_statement(alias, ctx) else {
                return;
            };

            if !self.aliasing.violation(occurrence.explicit_as) {
                return;
            }

            issues.push(
                Issue::warning(issue_codes::LINT_AL_001, self.aliasing.message())
                    .with_statement(ctx.statement_index)
                    .with_span(ctx.span_from_statement_offset(occurrence.start, occurrence.end)),
            );
        });

        issues
    }
}

#[derive(Clone, Copy)]
struct AliasOccurrence {
    start: usize,
    end: usize,
    explicit_as: bool,
}

fn alias_occurrence_in_statement(alias: &Ident, ctx: &LintContext) -> Option<AliasOccurrence> {
    let abs_start = line_col_to_offset(
        ctx.sql,
        alias.span.start.line as usize,
        alias.span.start.column as usize,
    )?;
    let abs_end = line_col_to_offset(
        ctx.sql,
        alias.span.end.line as usize,
        alias.span.end.column as usize,
    )?;

    if abs_start < ctx.statement_range.start || abs_end > ctx.statement_range.end {
        return None;
    }

    let rel_start = abs_start - ctx.statement_range.start;
    let rel_end = abs_end - ctx.statement_range.start;
    let statement_sql = ctx.statement_sql();
    let explicit_as = explicit_as_before_alias(statement_sql, rel_start);
    Some(AliasOccurrence {
        start: rel_start,
        end: rel_end,
        explicit_as,
    })
}

fn collect_table_aliases_in_statement<F: FnMut(&Ident)>(statement: &Statement, visitor: &mut F) {
    match statement {
        Statement::Query(query) => collect_table_aliases_in_query(query, visitor),
        Statement::Insert(insert) => {
            if let Some(source) = &insert.source {
                collect_table_aliases_in_query(source, visitor);
            }
        }
        Statement::CreateView { query, .. } => collect_table_aliases_in_query(query, visitor),
        Statement::CreateTable(create) => {
            if let Some(query) = &create.query {
                collect_table_aliases_in_query(query, visitor);
            }
        }
        Statement::Merge { table, source, .. } => {
            collect_table_aliases_in_table_factor(table, visitor);
            collect_table_aliases_in_table_factor(source, visitor);
        }
        _ => {}
    }
}

fn collect_table_aliases_in_query<F: FnMut(&Ident)>(query: &Query, visitor: &mut F) {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            collect_table_aliases_in_query(&cte.query, visitor);
        }
    }

    collect_table_aliases_in_set_expr(&query.body, visitor);
}

fn collect_table_aliases_in_set_expr<F: FnMut(&Ident)>(set_expr: &SetExpr, visitor: &mut F) {
    match set_expr {
        SetExpr::Select(select) => {
            for table in &select.from {
                collect_table_aliases_in_table_with_joins(table, visitor);
            }
        }
        SetExpr::Query(query) => collect_table_aliases_in_query(query, visitor),
        SetExpr::SetOperation { left, right, .. } => {
            collect_table_aliases_in_set_expr(left, visitor);
            collect_table_aliases_in_set_expr(right, visitor);
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => collect_table_aliases_in_statement(statement, visitor),
        _ => {}
    }
}

fn collect_table_aliases_in_table_with_joins<F: FnMut(&Ident)>(
    table_with_joins: &TableWithJoins,
    visitor: &mut F,
) {
    collect_table_aliases_in_table_factor(&table_with_joins.relation, visitor);
    for join in &table_with_joins.joins {
        collect_table_aliases_in_table_factor(&join.relation, visitor);
    }
}

fn collect_table_aliases_in_table_factor<F: FnMut(&Ident)>(
    table_factor: &TableFactor,
    visitor: &mut F,
) {
    if let Some(alias) = table_factor_alias_ident(table_factor) {
        visitor(alias);
    }

    match table_factor {
        TableFactor::Derived { subquery, .. } => collect_table_aliases_in_query(subquery, visitor),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => collect_table_aliases_in_table_with_joins(table_with_joins, visitor),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_table_aliases_in_table_factor(table, visitor)
        }
        _ => {}
    }
}

fn table_factor_alias_ident(table_factor: &TableFactor) -> Option<&Ident> {
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

    Some(&alias.name)
}

fn explicit_as_before_alias(statement_sql: &str, alias_start: usize) -> bool {
    if alias_start > statement_sql.len() {
        return false;
    }
    let prefix = &statement_sql[..alias_start];
    let trimmed = trim_trailing_trivia(prefix);
    trailing_word(trimmed)
        .map(|word| word.eq_ignore_ascii_case("as"))
        .unwrap_or(false)
}

fn trim_trailing_trivia(mut input: &str) -> &str {
    loop {
        let trimmed = input.trim_end_matches(char::is_whitespace);
        if trimmed.len() != input.len() {
            input = trimmed;
            continue;
        }

        if let Some(stripped) = strip_trailing_line_comment(input) {
            input = stripped;
            continue;
        }

        if let Some(stripped) = strip_trailing_block_comment(input) {
            input = stripped;
            continue;
        }

        return input;
    }
}

fn strip_trailing_line_comment(input: &str) -> Option<&str> {
    let line_start = input.rfind('\n').map_or(0, |idx| idx + 1);
    let tail = &input[line_start..];
    let comment_start = tail.rfind("--")?;
    Some(&input[..line_start + comment_start])
}

fn strip_trailing_block_comment(input: &str) -> Option<&str> {
    if !input.ends_with("*/") {
        return None;
    }
    let start = input.rfind("/*")?;
    Some(&input[..start])
}

fn trailing_word(input: &str) -> Option<&str> {
    let mut end = input.len();
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }

    let mut start = end;
    while start > 0 {
        let ch = input[..start].chars().next_back()?;
        if ch.is_ascii_alphanumeric() || ch == '_' {
            start -= ch.len_utf8();
        } else {
            break;
        }
    }

    (start < end).then_some(&input[start..end])
}

fn line_col_to_offset(sql: &str, line: usize, column: usize) -> Option<usize> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        parser::{parse_sql, parse_sql_with_dialect},
        Dialect,
    };

    fn run_with_rule(sql: &str, rule: AliasingTableStyle) -> Vec<Issue> {
        let stmts = parse_sql(sql).expect("parse");
        stmts
            .iter()
            .enumerate()
            .flat_map(|(index, stmt)| {
                rule.check(
                    stmt,
                    &LintContext {
                        sql,
                        statement_range: 0..sql.len(),
                        statement_index: index,
                    },
                )
            })
            .collect()
    }

    fn run(sql: &str) -> Vec<Issue> {
        run_with_rule(sql, AliasingTableStyle::default())
    }

    #[test]
    fn flags_implicit_table_aliases() {
        let issues = run("select * from users u join orders o on u.id = o.user_id");
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|i| i.code == issue_codes::LINT_AL_001));
    }

    #[test]
    fn allows_explicit_as_table_aliases() {
        let issues = run("select * from users as u join orders as o on u.id = o.user_id");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_explicit_aliases_when_implicit_policy_requested() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_AL_001".to_string(),
                serde_json::json!({"aliasing": "implicit"}),
            )]),
        };
        let issues = run_with_rule(
            "select * from users as u join orders as o on u.id = o.user_id",
            AliasingTableStyle::from_config(&config),
        );
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn flags_implicit_derived_table_alias() {
        let issues = run("select * from (select 1) d");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_001);
    }

    #[test]
    fn flags_implicit_merge_aliases_in_bigquery() {
        let sql = "MERGE dataset.inventory t USING dataset.newarrivals s ON t.product = s.product WHEN MATCHED THEN UPDATE SET quantity = t.quantity + s.quantity";
        let statements = parse_sql_with_dialect(sql, Dialect::Bigquery).expect("parse");
        let issues = statements
            .iter()
            .enumerate()
            .flat_map(|(index, stmt)| {
                AliasingTableStyle::default().check(
                    stmt,
                    &LintContext {
                        sql,
                        statement_range: 0..sql.len(),
                        statement_index: index,
                    },
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|i| i.code == issue_codes::LINT_AL_001));
    }
}
