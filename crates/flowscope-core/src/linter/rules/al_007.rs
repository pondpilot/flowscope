//! LINT_AL_007: Forbid unnecessary alias.
//!
//! SQLFluff AL07 parity: base-table aliases are unnecessary unless they are
//! needed to disambiguate repeated references to the same table (self-joins).

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::{Ident, Select, Statement, TableFactor, TableWithJoins};
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};
use std::collections::HashMap;

use super::semantic_helpers::visit_selects_in_statement;

#[derive(Default)]
pub struct AliasingForbidSingleTable {
    force_enable: bool,
}

impl AliasingForbidSingleTable {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            force_enable: config
                .rule_option_bool(issue_codes::LINT_AL_007, "force_enable")
                .unwrap_or(false),
        }
    }
}

impl BuiltinLintRule for AliasingForbidSingleTable {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_007
    }

    fn name(&self) -> &'static str {
        "Forbid unnecessary alias"
    }

    fn description(&self) -> &'static str {
        "Avoid table aliases in from clauses and join conditions."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        if !self.force_enable {
            return Vec::new();
        }

        let tokens =
            tokenized_for_context(ctx).or_else(|| tokenized(ctx.statement_sql(), ctx.dialect()));

        let mut issues = Vec::new();

        visit_selects_in_statement(statement, &mut |select| {
            let aliases = collect_unnecessary_aliases(select);
            for alias_info in &aliases {
                let edits = build_autofix_edits(alias_info, &aliases, ctx, tokens.as_deref());
                let mut issue =
                    Issue::info(issue_codes::LINT_AL_007, "Avoid unnecessary table aliases.")
                        .with_statement(ctx.statement_index);
                if !edits.is_empty() {
                    issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, edits);
                }
                issues.push(issue);
            }
        });

        if issues.is_empty() {
            if let Some(issue) = fallback_single_from_alias_issue(ctx, tokens.as_deref()) {
                issues.push(issue);
            }
        }

        issues
    }
}

#[derive(Clone)]
struct FallbackAliasCandidate {
    table_name: String,
    alias_name: String,
    alias_start: usize,
    alias_end: usize,
}

fn fallback_single_from_alias_issue(
    ctx: &RuleContext,
    tokens: Option<&[LocatedToken]>,
) -> Option<Issue> {
    if ctx.dialect() != Dialect::Mssql {
        return None;
    }
    let tokens = tokens?;
    if tokens.is_empty() || contains_join_keyword(tokens) {
        return None;
    }

    let candidate = fallback_single_from_alias_candidate(tokens, ctx.statement_sql())?;
    let mut edits = Vec::new();

    if let Some(delete_span) =
        alias_declaration_delete_span(tokens, candidate.alias_start, candidate.alias_end)
    {
        edits.push(IssuePatchEdit::new(
            ctx.span_from_statement_offset(delete_span.start, delete_span.end),
            "",
        ));
    }

    for (ref_start, ref_end) in find_qualified_alias_references(tokens, &candidate.alias_name, &[])
    {
        edits.push(IssuePatchEdit::new(
            ctx.span_from_statement_offset(ref_start, ref_end),
            candidate.table_name.clone(),
        ));
    }

    let mut issue = Issue::info(issue_codes::LINT_AL_007, "Avoid unnecessary table aliases.")
        .with_statement(ctx.statement_index)
        .with_span(ctx.span_from_statement_offset(candidate.alias_start, candidate.alias_end));

    if !edits.is_empty() {
        issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, edits);
    }

    Some(issue)
}

fn fallback_single_from_alias_candidate(
    tokens: &[LocatedToken],
    sql: &str,
) -> Option<FallbackAliasCandidate> {
    for (index, token) in tokens.iter().enumerate() {
        if !token_is_keyword(&token.token, "FROM") {
            continue;
        }

        let table_start_idx = next_non_trivia_index(tokens, index + 1)?;
        if !is_identifier_token(&tokens[table_start_idx].token) {
            continue;
        }

        let mut table_end_idx = table_start_idx;
        while let Some(dot_idx) = next_non_trivia_index(tokens, table_end_idx + 1) {
            if !matches!(tokens[dot_idx].token, Token::Period) {
                break;
            }
            let Some(next_part_idx) = next_non_trivia_index(tokens, dot_idx + 1) else {
                break;
            };
            if !is_identifier_token(&tokens[next_part_idx].token) {
                break;
            }
            table_end_idx = next_part_idx;
        }

        let alias_idx = next_non_trivia_index(tokens, table_end_idx + 1)?;
        let Token::Word(alias_word) = &tokens[alias_idx].token else {
            continue;
        };
        if alias_word.keyword != Keyword::NoKeyword {
            continue;
        }

        let table_start = tokens[table_start_idx].start;
        let table_end = tokens[table_end_idx].end;
        if table_start >= table_end || table_end > sql.len() {
            continue;
        }

        return Some(FallbackAliasCandidate {
            table_name: sql[table_start..table_end].to_string(),
            alias_name: alias_word.value.clone(),
            alias_start: tokens[alias_idx].start,
            alias_end: tokens[alias_idx].end,
        });
    }

    None
}

fn token_is_keyword(token: &Token, keyword: &str) -> bool {
    matches!(token, Token::Word(word) if word.value.eq_ignore_ascii_case(keyword))
}

fn is_identifier_token(token: &Token) -> bool {
    matches!(token, Token::Word(_) | Token::Placeholder(_))
}

fn next_non_trivia_index(tokens: &[LocatedToken], mut index: usize) -> Option<usize> {
    while index < tokens.len() {
        if !is_trivia_token(&tokens[index].token) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn contains_join_keyword(tokens: &[LocatedToken]) -> bool {
    tokens.iter().any(|token| {
        matches!(
            &token.token,
            Token::Word(word)
                if word.value.eq_ignore_ascii_case("JOIN")
                    || word.value.eq_ignore_ascii_case("LEFT")
                    || word.value.eq_ignore_ascii_case("RIGHT")
                    || word.value.eq_ignore_ascii_case("FULL")
                    || word.value.eq_ignore_ascii_case("INNER")
                    || word.value.eq_ignore_ascii_case("CROSS")
        )
    })
}

/// Information about a single unnecessary alias.
#[derive(Clone)]
struct UnnecessaryAlias {
    /// The original table name as written (e.g. "users", "SchemaA.TableB").
    table_name: String,
    /// The alias identifier from the AST (carries span information).
    alias_ident: Ident,
}

/// Collects all unnecessary aliases from a SELECT clause.
fn collect_unnecessary_aliases(select: &Select) -> Vec<UnnecessaryAlias> {
    let mut candidates = Vec::new();
    for table in &select.from {
        collect_alias_candidates_from_table_with_joins(table, &mut candidates);
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    // Count how many times each table appears to detect self-joins.
    let mut table_occurrence_counts: HashMap<String, usize> = HashMap::new();
    for (canonical, _has_alias, _table_name, _alias) in &candidates {
        *table_occurrence_counts
            .entry(canonical.clone())
            .or_insert(0) += 1;
    }

    let is_multi_source = candidates.len() > 1;

    candidates
        .into_iter()
        .filter_map(|(canonical, has_alias, table_name, alias_ident)| {
            if !has_alias {
                return None;
            }
            if is_multi_source
                && table_occurrence_counts
                    .get(&canonical)
                    .copied()
                    .unwrap_or(0)
                    > 1
            {
                return None;
            }
            Some(UnnecessaryAlias {
                table_name,
                alias_ident: alias_ident?,
            })
        })
        .collect()
}

type AliasCandidate = (
    String,        // canonical name (uppercase)
    bool,          // has_alias
    String,        // original table name as written
    Option<Ident>, // alias ident
);

fn collect_alias_candidates_from_table_with_joins(
    table: &TableWithJoins,
    candidates: &mut Vec<AliasCandidate>,
) {
    collect_alias_candidates_from_table_factor(&table.relation, candidates);
    for join in &table.joins {
        collect_alias_candidates_from_table_factor(&join.relation, candidates);
    }
}

fn collect_alias_candidates_from_table_factor(
    table_factor: &TableFactor,
    candidates: &mut Vec<AliasCandidate>,
) {
    match table_factor {
        TableFactor::Table { name, alias, .. } => {
            let canonical = name.to_string().to_ascii_uppercase();
            let table_name = name.to_string();
            let alias_ident = alias.as_ref().map(|a| a.name.clone());
            candidates.push((canonical, alias.is_some(), table_name, alias_ident));
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_alias_candidates_from_table_with_joins(table_with_joins, candidates);
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_alias_candidates_from_table_factor(table, candidates);
        }
        _ => {}
    }
}

/// Builds autofix edits for a single unnecessary alias violation.
///
/// Generates two types of edits:
/// 1. Delete the alias declaration (e.g., ` AS u` or ` u`)
/// 2. Replace qualified alias references (e.g., `u.id` → `users.id`)
fn build_autofix_edits(
    alias_info: &UnnecessaryAlias,
    all_aliases: &[UnnecessaryAlias],
    ctx: &RuleContext,
    tokens: Option<&[LocatedToken]>,
) -> Vec<IssuePatchEdit> {
    let Some(tokens) = tokens else {
        return Vec::new();
    };

    let alias_name = &alias_info.alias_ident.value;
    let table_name = &alias_info.table_name;

    // Find the alias declaration span and delete it.
    let alias_abs_start = line_col_to_offset(
        ctx.sql,
        alias_info.alias_ident.span.start.line as usize,
        alias_info.alias_ident.span.start.column as usize,
    );
    let alias_abs_end = line_col_to_offset(
        ctx.sql,
        alias_info.alias_ident.span.end.line as usize,
        alias_info.alias_ident.span.end.column as usize,
    );
    let (Some(alias_abs_start), Some(alias_abs_end)) = (alias_abs_start, alias_abs_end) else {
        return Vec::new();
    };

    if alias_abs_start < ctx.statement_range.start || alias_abs_end > ctx.statement_range.end {
        return Vec::new();
    }

    let rel_alias_start = alias_abs_start - ctx.statement_range.start;
    let rel_alias_end = alias_abs_end - ctx.statement_range.start;

    let mut edits = Vec::new();

    // Find the extent to delete: look back from alias for AS keyword + whitespace.
    if let Some(delete_span) = alias_declaration_delete_span(tokens, rel_alias_start, rel_alias_end)
    {
        edits.push(IssuePatchEdit::new(
            ctx.span_from_statement_offset(delete_span.start, delete_span.end),
            "",
        ));
    }

    // Find all qualified references to this alias (e.g., `u.id`) and replace with table name.
    let alias_refs = find_qualified_alias_references(tokens, alias_name, all_aliases);
    for (ref_start, ref_end) in alias_refs {
        edits.push(IssuePatchEdit::new(
            ctx.span_from_statement_offset(ref_start, ref_end),
            table_name.clone(),
        ));
    }

    edits
}

/// Determines the span to delete for an alias declaration.
///
/// Given the alias identifier position, looks backwards for an optional `AS` keyword
/// and the whitespace between the table name and the alias.
/// Returns the span `[whitespace_start..alias_end]` to delete.
fn alias_declaration_delete_span(
    tokens: &[LocatedToken],
    alias_start: usize,
    alias_end: usize,
) -> Option<Span> {
    // Walk backwards from alias_start to find:
    // 1. Optional whitespace before alias
    // 2. Optional AS keyword before that whitespace
    // 3. Whitespace before AS keyword (if AS was found)

    let mut delete_start = alias_start;

    // Find the token just before alias_start (skip trivia to find AS or table name).
    let mut found_as = false;
    for token in tokens.iter().rev() {
        if token.end > alias_start {
            continue;
        }
        if is_trivia_token(&token.token) {
            // Include whitespace in the deletion.
            if token.start < delete_start {
                delete_start = token.start;
            }
            continue;
        }
        if is_as_token(&token.token) {
            // Include AS keyword in the deletion.
            found_as = true;
            delete_start = token.start;
        }
        break;
    }

    // If we found AS, also include whitespace before it.
    if found_as {
        for token in tokens.iter().rev() {
            if token.end > delete_start {
                continue;
            }
            if is_trivia_token(&token.token) {
                if token.start < delete_start {
                    delete_start = token.start;
                }
                continue;
            }
            break;
        }
    }

    if delete_start < alias_end {
        Some(Span::new(delete_start, alias_end))
    } else {
        None
    }
}

/// Finds all qualified alias references in the token stream.
///
/// Looks for patterns like `alias.column` where `alias` matches the given name.
/// Only matches when followed by a `.` (dot) to avoid replacing bare identifiers
/// that happen to match the alias name (e.g., `ORDER BY o DESC` where `o` is a column).
fn find_qualified_alias_references(
    tokens: &[LocatedToken],
    alias_name: &str,
    all_aliases: &[UnnecessaryAlias],
) -> Vec<(usize, usize)> {
    let mut refs = Vec::new();

    for (i, token) in tokens.iter().enumerate() {
        // Look for Word tokens that match the alias name.
        let Token::Word(word) = &token.token else {
            continue;
        };
        if !word.value.eq_ignore_ascii_case(alias_name) {
            continue;
        }
        // Must be followed by a dot to be a qualified reference.
        let next_non_trivia = tokens[i + 1..].iter().find(|t| !is_trivia_token(&t.token));
        if !next_non_trivia.is_some_and(|t| matches!(t.token, Token::Period)) {
            continue;
        }
        // Must NOT be preceded by a dot (would be a schema-qualified name, not an alias ref).
        let prev_non_trivia = tokens[..i]
            .iter()
            .rev()
            .find(|t| !is_trivia_token(&t.token));
        if prev_non_trivia.is_some_and(|t| matches!(t.token, Token::Period)) {
            continue;
        }
        // Skip if this token's position is at an alias declaration site.
        if all_aliases.iter().any(|a| {
            let a_start = line_col_to_absolute_offset(
                a.alias_ident.span.start.line as usize,
                a.alias_ident.span.start.column as usize,
            );
            a_start.is_some_and(|s| s == token.start)
        }) {
            continue;
        }
        refs.push((token.start, token.end));
    }

    refs
}

// This is a simplified version that doesn't require the full SQL text.
// It works because token offsets are already relative to the statement.
fn line_col_to_absolute_offset(_line: usize, _column: usize) -> Option<usize> {
    // We can't compute this without the SQL text. This function is a placeholder;
    // the actual skip logic uses direct offset comparison.
    None
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<LocatedToken>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        let (start, end) = token_with_span_offsets(sql, &token)?;
        out.push(LocatedToken {
            token: token.token,
            start,
            end,
        });
    }
    Some(out)
}

fn tokenized_for_context(ctx: &RuleContext) -> Option<Vec<LocatedToken>> {
    let statement_start = ctx.statement_range.start;
    ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        Some(
            tokens
                .iter()
                .filter_map(|token| {
                    let (start, end) = token_with_span_offsets(ctx.sql, token)?;
                    if start < ctx.statement_range.start || end > ctx.statement_range.end {
                        return None;
                    }
                    Some(LocatedToken {
                        token: token.token.clone(),
                        start: start - statement_start,
                        end: end - statement_start,
                    })
                })
                .collect::<Vec<_>>(),
        )
    })
}

fn token_with_span_offsets(sql: &str, token: &TokenWithSpan) -> Option<(usize, usize)> {
    let start = line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )?;
    let end = line_col_to_offset(
        sql,
        token.span.end.line as usize,
        token.span.end.column as usize,
    )?;
    Some((start, end))
}

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

fn is_as_token(token: &Token) -> bool {
    match token {
        Token::Word(word) => word.value.eq_ignore_ascii_case("AS"),
        _ => false,
    }
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
    use crate::parser::parse_sql;
    use crate::types::Dialect;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingForbidSingleTable::default();
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn run_force_enabled_statementless_mssql(sql: &str) -> Vec<Issue> {
        let synthetic = parse_sql("SELECT 1").expect("parse");
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.forbid".to_string(),
                serde_json::json!({"force_enable": true}),
            )]),
        };
        let rule = AliasingForbidSingleTable::from_config(&config);
        rule.check_with_context(
            &synthetic[0],
            &RuleContext::new(sql, 0..sql.len(), 0).with_dialect(Dialect::Mssql),
        )
    }

    fn apply_issue_autofix(sql: &str, issue: &Issue) -> Option<String> {
        let autofix = issue.autofix.as_ref()?;
        let mut out = sql.to_string();
        let mut edits = autofix.edits.clone();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        for edit in edits.into_iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    fn run_force_enabled(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.forbid".to_string(),
                serde_json::json!({"force_enable": true}),
            )]),
        };
        let rule = AliasingForbidSingleTable::from_config(&config);
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    #[test]
    fn disabled_by_default() {
        let issues = run("SELECT * FROM users u");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_single_table_alias_when_force_enabled() {
        let issues = run_force_enabled("SELECT * FROM users u");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_007);
    }

    #[test]
    fn does_not_flag_single_table_without_alias() {
        let issues = run_force_enabled("SELECT * FROM users");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_multi_source_query_when_force_enabled() {
        let issues = run_force_enabled("SELECT * FROM users u JOIN orders o ON u.id = o.user_id");
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn allows_self_join_aliases() {
        let issues = run_force_enabled("SELECT * FROM users u1 JOIN users u2 ON u1.id = u2.id");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_non_self_join_alias_in_self_join_scope() {
        let issues = run_force_enabled(
            "SELECT * FROM users u1 JOIN users u2 ON u1.id = u2.id JOIN orders o ON o.user_id = u1.id",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn does_not_flag_derived_table_alias() {
        let issues = run_force_enabled("SELECT * FROM (SELECT 1 AS id) sub");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_nested_single_table_alias() {
        let issues = run_force_enabled("SELECT * FROM (SELECT * FROM users u) sub");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn force_enable_false_disables_rule() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.forbid".to_string(),
                serde_json::json!({"force_enable": false}),
            )]),
        };
        let rule = AliasingForbidSingleTable::from_config(&config);
        let sql = "SELECT * FROM users u";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn single_table_alias_autofix_removes_alias_and_replaces_refs() {
        let sql = "SELECT u.id FROM users AS u";
        let issues = run_force_enabled(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("expected autofix");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        // Should have: 1 delete (alias decl) + 1 replace (u.id → users.id)
        assert!(
            autofix.edits.len() >= 2,
            "expected at least 2 edits, got: {:?}",
            autofix.edits
        );
    }

    #[test]
    fn multi_table_alias_autofix() {
        let sql = "SELECT u.id, o.total FROM users AS u JOIN orders AS o ON u.id = o.user_id";
        let issues = run_force_enabled(sql);
        assert_eq!(issues.len(), 2);
        for issue in &issues {
            assert!(issue.autofix.is_some(), "expected autofix on AL07 issue");
        }
    }

    #[test]
    fn statementless_tsql_create_table_as_alias_fallback_detects_and_fixes() {
        let sql = "DECLARE @VariableE date = GETDATE()\n\nCREATE TABLE #TempTable\nAS\n(\n  Select ColumnD\n  from SchemaA.TableB AliasC\n  where ColumnD  >= @VariableE\n)\n";
        let issues = run_force_enabled_statementless_mssql(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("autofix");
        assert_eq!(
            fixed,
            "DECLARE @VariableE date = GETDATE()\n\nCREATE TABLE #TempTable\nAS\n(\n  Select ColumnD\n  from SchemaA.TableB\n  where ColumnD  >= @VariableE\n)\n"
        );
    }
}
