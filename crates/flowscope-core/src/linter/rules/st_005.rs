//! LINT_ST_005: Structure subquery.
//!
//! SQLFluff ST05 parity: avoid subqueries in FROM/JOIN clauses; prefer CTEs.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::parser::parse_sql_with_dialect;
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::{Query, Select, SetExpr, Statement, TableFactor};
use std::collections::HashSet;

use super::semantic_helpers::{
    collect_qualifier_prefixes_in_expr, visit_select_expressions, visit_selects_in_statement,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ForbidSubqueryIn {
    Both,
    Join,
    From,
}

impl ForbidSubqueryIn {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_ST_005, "forbid_subquery_in")
            .unwrap_or("join")
            .to_ascii_lowercase()
            .as_str()
        {
            "join" => Self::Join,
            "from" => Self::From,
            _ => Self::Both,
        }
    }

    fn forbid_from(self) -> bool {
        matches!(self, Self::Both | Self::From)
    }

    fn forbid_join(self) -> bool {
        matches!(self, Self::Both | Self::Join)
    }
}

pub struct StructureSubquery {
    forbid_subquery_in: ForbidSubqueryIn,
}

impl StructureSubquery {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            forbid_subquery_in: ForbidSubqueryIn::from_config(config),
        }
    }
}

impl Default for StructureSubquery {
    fn default() -> Self {
        Self {
            forbid_subquery_in: ForbidSubqueryIn::Join,
        }
    }
}

impl BuiltinLintRule for StructureSubquery {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_005
    }

    fn name(&self) -> &'static str {
        "Structure subquery"
    }

    fn description(&self) -> &'static str {
        "Join/From clauses should not contain subqueries. Use CTEs instead."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut violations = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            let outer_source_names = source_names_in_select(select);
            for table in &select.from {
                if self.forbid_subquery_in.forbid_from()
                    && table_factor_contains_derived(&table.relation, &outer_source_names)
                {
                    violations += 1;
                }
                if self.forbid_subquery_in.forbid_join() {
                    for join in &table.joins {
                        if table_factor_contains_derived(&join.relation, &outer_source_names) {
                            violations += 1;
                        }
                    }
                }
            }
        });

        if violations == 0 {
            return Vec::new();
        }

        let autofix_edits = st005_subquery_to_cte_rewrite(
            ctx.statement_sql(),
            statement,
            self.forbid_subquery_in,
            ctx.dialect(),
        )
        .filter(|rewritten| rewritten != ctx.statement_sql())
        .map(|rewritten| {
            vec![IssuePatchEdit::new(
                ctx.span_from_statement_offset(0, ctx.statement_sql().len()),
                rewritten,
            )]
        })
        .unwrap_or_default();

        (0..violations)
            .map(|index| {
                let mut issue = Issue::info(
                    issue_codes::LINT_ST_005,
                    "Join/From clauses should not contain subqueries. Use CTEs instead.",
                )
                .with_statement(ctx.statement_index);
                if index == 0 && !autofix_edits.is_empty() {
                    issue = issue.with_autofix_edits(
                        IssueAutofixApplicability::Unsafe,
                        autofix_edits.clone(),
                    );
                }
                issue
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Comprehensive text-preserving subquery-to-CTE rewriter
// ---------------------------------------------------------------------------

/// A subquery found in a FROM/JOIN clause that should be extracted to a CTE.
#[derive(Debug, Clone)]
struct SubqueryExtraction {
    /// Byte offset of the open parenthesis.
    open_paren: usize,
    /// Byte offset of the close parenthesis.
    close_paren: usize,
    /// Alias name (explicit or auto-generated).
    alias: String,
    /// Byte offset past the end of the alias region.
    alias_region_end: usize,
}

/// Rewrite the SQL statement by extracting all subqueries in FROM/JOIN clauses
/// to CTEs. Returns the rewritten SQL, or None if no rewrite is possible.
fn st005_subquery_to_cte_rewrite(
    sql: &str,
    stmt: &Statement,
    forbid_subquery_in: ForbidSubqueryIn,
    dialect: Dialect,
) -> Option<String> {
    const MAX_REWRITE_PASSES: usize = 8;

    let mut current_sql = sql.to_string();
    let mut current_stmt = stmt.clone();
    let mut changed = false;

    for _ in 0..MAX_REWRITE_PASSES {
        // Collect all non-correlated subqueries from the current AST.
        let mut subquery_aliases: Vec<(String, bool)> = Vec::new();
        collect_extractable_subqueries(&current_stmt, forbid_subquery_in, &mut subquery_aliases);
        if subquery_aliases.is_empty() {
            break;
        }

        // Find subquery positions in the current SQL text.
        let extractions =
            find_subquery_positions(&current_sql, forbid_subquery_in, &subquery_aliases);
        if extractions.is_empty() {
            break;
        }

        let Some(rewritten) = apply_cte_extractions(&current_sql, &extractions, dialect) else {
            break;
        };
        if rewritten == current_sql {
            break;
        }

        changed = true;
        current_sql = rewritten;

        // Re-parse the rewritten SQL so later passes can extract newly-exposed
        // nested subqueries (e.g. inside extracted CTE bodies).
        let Ok(mut reparsed) = parse_sql_with_dialect(&current_sql, dialect) else {
            break;
        };
        let Some(next_stmt) = (reparsed.len() == 1).then(|| reparsed.remove(0)) else {
            break;
        };
        current_stmt = next_stmt;
    }

    changed.then_some(current_sql)
}

/// Walk the AST to collect info about each extractable (non-correlated) subquery.
/// Collects (alias_name, is_correlated) in document order.
fn collect_extractable_subqueries(
    stmt: &Statement,
    forbid_in: ForbidSubqueryIn,
    out: &mut Vec<(String, bool)>,
) {
    visit_selects_in_statement(stmt, &mut |select| {
        let outer_source_names = source_names_in_select(select);
        for table in &select.from {
            if forbid_in.forbid_from() {
                collect_from_table_factor(&table.relation, &outer_source_names, out);
            }
            if forbid_in.forbid_join() {
                for join in &table.joins {
                    collect_from_table_factor(&join.relation, &outer_source_names, out);
                }
            }
        }
    });
}

/// Recursively collect extractable subqueries from a table factor.
fn collect_from_table_factor(
    tf: &TableFactor,
    outer_names: &HashSet<String>,
    out: &mut Vec<(String, bool)>,
) {
    match tf {
        TableFactor::Derived {
            subquery, alias, ..
        } => {
            let is_correlated = query_references_outer_sources(subquery, outer_names);
            if !is_correlated {
                let alias_name = alias
                    .as_ref()
                    .map(|a| a.name.value.clone())
                    .unwrap_or_default();
                out.push((alias_name, is_correlated));
            }
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_from_table_factor(&table_with_joins.relation, outer_names, out);
            for join in &table_with_joins.joins {
                collect_from_table_factor(&join.relation, outer_names, out);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_from_table_factor(table, outer_names, out);
        }
        _ => {}
    }
}

/// Scan the SQL text to locate subquery parenthesized expressions in FROM/JOIN
/// clauses. Returns extractions sorted by position (for correct processing order).
fn find_subquery_positions(
    sql: &str,
    forbid_in: ForbidSubqueryIn,
    ast_aliases: &[(String, bool)],
) -> Vec<SubqueryExtraction> {
    let bytes = sql.as_bytes();
    let mut extractions = Vec::new();
    let mut ast_idx = 0usize;
    let mut auto_name_counter = 0usize;
    // Collect all names to avoid clashes.
    let mut existing_cte_names: HashSet<String> = HashSet::new();
    collect_existing_cte_names(sql, &mut existing_cte_names);

    // Names reserved for generated prep_N CTEs.
    let mut used_names: HashSet<String> = existing_cte_names.clone();
    for (alias, _) in ast_aliases {
        if !alias.is_empty() {
            used_names.insert(alias.to_ascii_uppercase());
        }
    }

    // Names already claimed by explicit/auto extractions in this pass.
    let mut claimed_names: HashSet<String> = existing_cte_names;

    let mut pos = 0usize;
    while pos < bytes.len() {
        // Skip quoted regions.
        if let Some(end) = skip_quoted_region(bytes, pos) {
            pos = end;
            continue;
        }
        // Skip line comments.
        if bytes[pos] == b'-' && bytes.get(pos + 1) == Some(&b'-') {
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }
        // Skip block comments.
        if bytes[pos] == b'/' && bytes.get(pos + 1) == Some(&b'*') {
            pos += 2;
            while pos + 1 < bytes.len() {
                if bytes[pos] == b'*' && bytes[pos + 1] == b'/' {
                    pos += 2;
                    break;
                }
                pos += 1;
            }
            continue;
        }

        // Look for FROM or JOIN keywords followed by a parenthesized subquery.
        let is_from =
            forbid_in.forbid_from() && match_ascii_keyword_at(bytes, pos, b"FROM").is_some();
        let is_join = forbid_in.forbid_join()
            && (match_ascii_keyword_at(bytes, pos, b"JOIN").is_some()
                || match_join_keyword_sequence(bytes, pos).is_some());

        if is_from || is_join {
            let keyword_end = if is_from {
                match_ascii_keyword_at(bytes, pos, b"FROM").unwrap()
            } else if let Some(end) = match_join_keyword_sequence(bytes, pos) {
                end
            } else {
                match_ascii_keyword_at(bytes, pos, b"JOIN").unwrap()
            };

            let after_keyword = skip_ascii_whitespace(bytes, keyword_end);

            // Check for open parenthesis (could be `FROM(` or `FROM (` or `JOIN\n(`).
            if after_keyword < bytes.len() && bytes[after_keyword] == b'(' {
                if let Some(close) = find_matching_parenthesis_outside_quotes(sql, after_keyword) {
                    let inner = sql[after_keyword + 1..close].trim();
                    let inner_lower = inner.to_ascii_lowercase();
                    // Only extract if inner content starts with SELECT or WITH,
                    // and we still have AST aliases to consume.
                    if (inner_lower.starts_with("select") || inner_lower.starts_with("with"))
                        && ast_idx < ast_aliases.len()
                    {
                        let (ref ast_alias, _) = ast_aliases[ast_idx];
                        ast_idx += 1;

                        let alias = if ast_alias.is_empty() {
                            let name = generate_prep_name(&mut auto_name_counter, &used_names);
                            let name_key = name.to_ascii_uppercase();
                            used_names.insert(name_key.clone());
                            claimed_names.insert(name_key);
                            name
                        } else {
                            let alias_key = ast_alias.to_ascii_uppercase();
                            // If the alias would clash with an existing/previous CTE
                            // name, leave this subquery in place (SQLFluff parity).
                            if claimed_names.contains(&alias_key) {
                                pos = close + 1;
                                continue;
                            }
                            claimed_names.insert(alias_key.clone());
                            used_names.insert(alias_key);
                            ast_alias.clone()
                        };

                        // Parse alias region after close paren.
                        let (_alias_start, alias_end) =
                            parse_alias_region_after_close_paren(bytes, close);

                        extractions.push(SubqueryExtraction {
                            open_paren: after_keyword,
                            close_paren: close,
                            alias: alias.clone(),
                            alias_region_end: alias_end,
                        });

                        // Skip past the subquery.
                        pos = alias_end;
                        continue;
                    }
                }
            }
        }

        pos += 1;
    }

    extractions
}

/// Generate a unique prep_N name that doesn't clash with used_names.
fn generate_prep_name(counter: &mut usize, used_names: &HashSet<String>) -> String {
    loop {
        *counter += 1;
        let name = format!("prep_{counter}");
        if !used_names.contains(&name.to_ascii_uppercase()) {
            return name;
        }
    }
}

/// Collect CTE names from existing WITH clause in the SQL text.
fn collect_existing_cte_names(sql: &str, names: &mut HashSet<String>) {
    let bytes = sql.as_bytes();
    let mut pos = skip_ascii_whitespace(bytes, 0);

    // Check for INSERT ... WITH or CREATE TABLE ... AS WITH patterns.
    // Skip past INSERT INTO ... or CREATE TABLE ... AS to find WITH.
    if let Some(end) = match_ascii_keyword_at(bytes, pos, b"INSERT") {
        pos = skip_to_with_or_select(bytes, end);
    } else if let Some(end) = match_ascii_keyword_at(bytes, pos, b"CREATE") {
        pos = skip_to_with_or_select(bytes, end);
    }

    if match_ascii_keyword_at(bytes, pos, b"WITH").is_none() {
        return;
    }

    let with_end = match_ascii_keyword_at(bytes, pos, b"WITH").unwrap();
    pos = skip_ascii_whitespace(bytes, with_end);

    // Skip RECURSIVE keyword if present.
    if let Some(end) = match_ascii_keyword_at(bytes, pos, b"RECURSIVE") {
        pos = skip_ascii_whitespace(bytes, end);
    }

    // Parse CTE names: name AS (...), name AS (...), ...
    loop {
        // Parse CTE name.
        let name_start = pos;
        if let Some(quoted_end) = consume_quoted_identifier(bytes, pos) {
            let raw = &sql[name_start..quoted_end];
            let unquoted = raw.trim_matches(|c| c == '"' || c == '`' || c == '[' || c == ']');
            names.insert(unquoted.to_ascii_uppercase());
            pos = skip_ascii_whitespace(bytes, quoted_end);
        } else if let Some(name_end) = consume_ascii_identifier(bytes, pos) {
            names.insert(sql[name_start..name_end].to_ascii_uppercase());
            pos = skip_ascii_whitespace(bytes, name_end);
        } else {
            break;
        }

        // Expect AS keyword.
        if let Some(as_end) = match_ascii_keyword_at(bytes, pos, b"AS") {
            pos = skip_ascii_whitespace(bytes, as_end);
        } else {
            break;
        }

        // Skip the CTE body parenthesized expression.
        if pos < bytes.len() && bytes[pos] == b'(' {
            if let Some(close) = find_matching_parenthesis_outside_quotes(sql, pos) {
                pos = skip_ascii_whitespace(bytes, close + 1);
            } else {
                break;
            }
        } else {
            break;
        }

        // Check for comma (more CTEs follow).
        if pos < bytes.len() && bytes[pos] == b',' {
            pos += 1;
            pos = skip_ascii_whitespace(bytes, pos);
        } else {
            break;
        }
    }
}

/// Skip forward in bytes to find the position of WITH or SELECT keyword.
fn skip_to_with_or_select(bytes: &[u8], mut pos: usize) -> usize {
    while pos < bytes.len() {
        let ws = skip_ascii_whitespace(bytes, pos);
        if ws > pos {
            pos = ws;
        }
        if match_ascii_keyword_at(bytes, pos, b"WITH").is_some() {
            return pos;
        }
        if match_ascii_keyword_at(bytes, pos, b"SELECT").is_some() {
            return pos;
        }
        pos += 1;
    }
    pos
}

/// Parse the alias region (optional `AS` + identifier) after a close parenthesis.
/// Returns (region_start, region_end) where region_start is close_paren + 1.
fn parse_alias_region_after_close_paren(bytes: &[u8], close_paren: usize) -> (usize, usize) {
    let start = close_paren + 1;
    let mut pos = start;
    let ws_pos = skip_ascii_whitespace(bytes, pos);

    // Check for AS keyword.
    if let Some(as_end) = match_ascii_keyword_at(bytes, ws_pos, b"AS") {
        let after_as = skip_ascii_whitespace(bytes, as_end);
        if let Some(quoted_end) = consume_quoted_identifier(bytes, after_as) {
            return (start, quoted_end);
        }
        if let Some(ident_end) = consume_ascii_identifier(bytes, after_as) {
            return (start, ident_end);
        }
    }

    // No AS keyword; check for bare identifier alias.
    // An identifier here is an alias only if it's not a SQL keyword that would
    // indicate the start of the next clause (ON, USING, WHERE, JOIN, etc.).
    if let Some(quoted_end) = consume_quoted_identifier(bytes, ws_pos) {
        return (start, quoted_end);
    }
    if let Some(ident_end) = consume_ascii_identifier(bytes, ws_pos) {
        let word = &bytes[ws_pos..ident_end];
        if !is_clause_keyword(word) {
            pos = ident_end;
            return (start, pos);
        }
    }

    (start, start)
}

/// Check if a word is a SQL clause keyword that should not be treated as an alias.
fn is_clause_keyword(word: &[u8]) -> bool {
    let upper: Vec<u8> = word.iter().map(|b| b.to_ascii_uppercase()).collect();
    matches!(
        upper.as_slice(),
        b"ON"
            | b"USING"
            | b"WHERE"
            | b"JOIN"
            | b"INNER"
            | b"LEFT"
            | b"RIGHT"
            | b"FULL"
            | b"OUTER"
            | b"CROSS"
            | b"NATURAL"
            | b"GROUP"
            | b"ORDER"
            | b"HAVING"
            | b"LIMIT"
            | b"UNION"
            | b"INTERSECT"
            | b"EXCEPT"
            | b"MINUS"
            | b"FROM"
            | b"SELECT"
            | b"INSERT"
            | b"UPDATE"
            | b"DELETE"
            | b"SET"
            | b"INTO"
            | b"VALUES"
            | b"WITH"
    )
}

/// Apply the subquery extractions: build CTE definitions, replace subqueries
/// with alias references, and insert the WITH clause.
fn apply_cte_extractions(
    sql: &str,
    extractions: &[SubqueryExtraction],
    dialect: Dialect,
) -> Option<String> {
    if extractions.is_empty() {
        return None;
    }

    let case_pref = detect_case_preference(sql);

    // Find if there's an existing WITH clause and where each existing CTE lives.
    let existing_ctes = parse_existing_cte_ranges(sql);

    // For each extraction, determine if it's inside an existing CTE body.
    // Build (cte_def, insert_before_cte_index) pairs.
    struct CteInsertion {
        definition: String,
        /// None = append at end / prepend for new WITH. Some(i) = insert before existing CTE i.
        insert_before: Option<usize>,
    }

    let mut insertions: Vec<CteInsertion> = Vec::new();
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    for ext in extractions {
        let subquery_text = &sql[ext.open_paren + 1..ext.close_paren];
        let as_kw = if case_pref == CasePref::Upper {
            "AS"
        } else {
            "as"
        };
        let cte_def = format!("{} {} ({})", ext.alias, as_kw, subquery_text);

        // Check if this extraction is inside an existing CTE body.
        let containing_cte = existing_ctes
            .iter()
            .position(|cte| ext.open_paren >= cte.body_start && ext.close_paren <= cte.body_end);

        insertions.push(CteInsertion {
            definition: cte_def,
            insert_before: containing_cte,
        });

        let mut replacement = ext.alias.clone();
        if ext.open_paren > 0 {
            let prev = sql.as_bytes()[ext.open_paren - 1];
            if !prev.is_ascii_whitespace() {
                replacement.insert(0, ' ');
            }
        }

        replacements.push((ext.open_paren, ext.alias_region_end, replacement));
    }

    // Apply text replacements in reverse order to preserve positions.
    let mut result = sql.to_string();
    for (start, end, replacement) in replacements.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }

    // Now insert CTEs. Separate into two groups:
    // 1. CTEs that need to be inserted before an existing CTE (dependency ordering)
    // 2. CTEs that are new top-level (no existing WITH, or appended)
    let mut before_insertions: Vec<(usize, String)> = Vec::new(); // (cte_index, definition)
    let mut top_level_defs: Vec<String> = Vec::new();

    for insertion in insertions {
        match insertion.insert_before {
            Some(cte_idx) => before_insertions.push((cte_idx, insertion.definition)),
            None => top_level_defs.push(insertion.definition),
        }
    }

    if !before_insertions.is_empty() && !existing_ctes.is_empty() {
        // We need to rebuild the WITH clause with reordered CTEs.
        result = rebuild_with_clause_with_insertions(
            &result,
            sql,
            &existing_ctes,
            &before_insertions,
            &top_level_defs,
            case_pref,
        );
        return Some(result);
    }

    // Simple case: just insert/append new CTEs.
    insert_cte_clause(&result, &top_level_defs, case_pref, dialect)
}

/// Range info for an existing CTE in the WITH clause.
#[derive(Debug, Clone)]
struct ExistingCteRange {
    /// Byte offset of the CTE body open paren.
    body_start: usize,
    /// Byte offset of the CTE body close paren.
    body_end: usize,
}

/// Parse the existing CTE definitions in a WITH clause.
fn parse_existing_cte_ranges(sql: &str) -> Vec<ExistingCteRange> {
    let bytes = sql.as_bytes();
    let mut pos = skip_ascii_whitespace(bytes, 0);
    let mut ranges = Vec::new();

    // Skip INSERT/CREATE prefix.
    if match_ascii_keyword_at(bytes, pos, b"INSERT").is_some()
        || match_ascii_keyword_at(bytes, pos, b"CREATE").is_some()
    {
        pos = skip_to_with_or_select(bytes, pos + 6);
    }

    let with_end = match match_ascii_keyword_at(bytes, pos, b"WITH") {
        Some(end) => end,
        None => return ranges,
    };
    pos = skip_ascii_whitespace(bytes, with_end);

    // Skip RECURSIVE.
    if let Some(end) = match_ascii_keyword_at(bytes, pos, b"RECURSIVE") {
        pos = skip_ascii_whitespace(bytes, end);
    }

    loop {
        // CTE name.
        if let Some(quoted_end) = consume_quoted_identifier(bytes, pos) {
            pos = skip_ascii_whitespace(bytes, quoted_end);
        } else if let Some(name_end) = consume_ascii_identifier(bytes, pos) {
            pos = skip_ascii_whitespace(bytes, name_end);
        } else {
            break;
        }

        // AS keyword.
        if let Some(as_end) = match_ascii_keyword_at(bytes, pos, b"AS") {
            pos = skip_ascii_whitespace(bytes, as_end);
        } else {
            break;
        }

        // CTE body paren.
        if pos < bytes.len() && bytes[pos] == b'(' {
            if let Some(close) = find_matching_parenthesis_outside_quotes(sql, pos) {
                ranges.push(ExistingCteRange {
                    body_start: pos,
                    body_end: close,
                });
                pos = skip_ascii_whitespace(bytes, close + 1);
            } else {
                break;
            }
        } else {
            break;
        }

        // Comma.
        if pos < bytes.len() && bytes[pos] == b',' {
            pos += 1;
            pos = skip_ascii_whitespace(bytes, pos);
        } else {
            break;
        }
    }

    ranges
}

/// Rebuild the WITH clause with new CTEs inserted before their containing CTEs.
fn rebuild_with_clause_with_insertions(
    modified_sql: &str,
    _original_sql: &str,
    _existing_ctes: &[ExistingCteRange],
    before_insertions: &[(usize, String)],
    top_level_defs: &[String],
    case_pref: CasePref,
) -> String {
    // The modified_sql has already had subquery text replaced with alias names.
    // We need to reconstruct the WITH clause with CTEs in dependency order.
    //
    // Strategy: find the WITH clause region in modified_sql, extract each CTE text,
    // then rebuild with new CTEs inserted at the right positions.

    let bytes = modified_sql.as_bytes();
    let mut pos = skip_ascii_whitespace(bytes, 0);

    // Skip INSERT/CREATE prefix.
    if match_ascii_keyword_at(bytes, pos, b"INSERT").is_some()
        || match_ascii_keyword_at(bytes, pos, b"CREATE").is_some()
    {
        pos = skip_to_with_or_select(bytes, pos + 6);
    }

    let with_kw_start = pos;
    let with_end = match match_ascii_keyword_at(bytes, pos, b"WITH") {
        Some(end) => end,
        None => return modified_sql.to_string(),
    };
    pos = skip_ascii_whitespace(bytes, with_end);

    // Skip RECURSIVE.
    if let Some(end) = match_ascii_keyword_at(bytes, pos, b"RECURSIVE") {
        pos = skip_ascii_whitespace(bytes, end);
    }

    // Parse CTE texts from modified SQL.
    let mut cte_texts: Vec<String> = Vec::new();
    let mut last_cte_end = pos;

    loop {
        let cte_start = pos;

        if let Some(quoted_end) = consume_quoted_identifier(bytes, pos) {
            pos = skip_ascii_whitespace(bytes, quoted_end);
        } else if let Some(name_end) = consume_ascii_identifier(bytes, pos) {
            pos = skip_ascii_whitespace(bytes, name_end);
        } else {
            break;
        }

        if let Some(as_end) = match_ascii_keyword_at(bytes, pos, b"AS") {
            pos = skip_ascii_whitespace(bytes, as_end);
        } else {
            break;
        }

        if pos < bytes.len() && bytes[pos] == b'(' {
            if let Some(close) = find_matching_parenthesis_outside_quotes(modified_sql, pos) {
                let cte_text = modified_sql[cte_start..close + 1].to_string();
                cte_texts.push(cte_text);
                last_cte_end = close + 1;
                pos = skip_ascii_whitespace(bytes, close + 1);
            } else {
                break;
            }
        } else {
            break;
        }

        if pos < bytes.len() && bytes[pos] == b',' {
            pos += 1;
            pos = skip_ascii_whitespace(bytes, pos);
        } else {
            break;
        }
    }

    // Build new CTE list with insertions at the right positions.
    let mut new_cte_list: Vec<String> = Vec::new();
    for (i, cte_text) in cte_texts.iter().enumerate() {
        // Insert any new CTEs that should go before this existing CTE.
        for (before_idx, def) in before_insertions {
            if *before_idx == i {
                new_cte_list.push(def.clone());
            }
        }
        new_cte_list.push(cte_text.clone());
    }

    // Append top-level defs at end.
    for def in top_level_defs {
        new_cte_list.push(def.clone());
    }

    // Rebuild the SQL.
    let with_kw = if case_pref == CasePref::Upper {
        "WITH"
    } else {
        "with"
    };
    let remainder = &modified_sql[last_cte_end..];

    let mut result = String::with_capacity(modified_sql.len() + 200);
    result.push_str(&modified_sql[..with_kw_start]);
    result.push_str(with_kw);
    result.push(' ');
    for (i, cte) in new_cte_list.iter().enumerate() {
        if i > 0 {
            result.push_str(",\n");
        }
        result.push_str(cte);
    }
    result.push_str(remainder);

    result
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CasePref {
    Upper,
    Lower,
}

/// Detect whether the SQL uses uppercase or lowercase keywords.
fn detect_case_preference(sql: &str) -> CasePref {
    let bytes = sql.as_bytes();
    let pos = skip_ascii_whitespace(bytes, 0);
    // Check the first keyword.
    for kw in &[b"WITH" as &[u8], b"SELECT", b"INSERT", b"CREATE"] {
        if pos + kw.len() <= bytes.len() {
            let word = &bytes[pos..pos + kw.len()];
            if word
                .iter()
                .zip(kw.iter())
                .all(|(a, b)| a.to_ascii_uppercase() == *b)
                && is_word_boundary_for_keyword(bytes, pos + kw.len())
            {
                return if word[0].is_ascii_uppercase() {
                    CasePref::Upper
                } else {
                    CasePref::Lower
                };
            }
        }
    }
    CasePref::Upper
}

/// Insert CTE definitions into the SQL, handling existing WITH clauses,
/// INSERT...SELECT, and CTAS patterns.
fn insert_cte_clause(
    sql: &str,
    cte_defs: &[String],
    case_pref: CasePref,
    dialect: Dialect,
) -> Option<String> {
    let bytes = sql.as_bytes();
    let with_kw = if case_pref == CasePref::Upper {
        "WITH"
    } else {
        "with"
    };

    // Check for INSERT...SELECT or CREATE TABLE...AS patterns.
    let scan_pos = skip_ascii_whitespace(bytes, 0);

    let is_insert = match_ascii_keyword_at(bytes, scan_pos, b"INSERT").is_some();
    let is_create = match_ascii_keyword_at(bytes, scan_pos, b"CREATE").is_some();
    let is_tsql_insert = is_insert && dialect == Dialect::Mssql;

    if is_tsql_insert {
        // T-SQL: WITH goes before INSERT.
        let insert_pos = skip_ascii_whitespace(bytes, 0);
        return Some(insert_with_before_position(
            sql, insert_pos, cte_defs, with_kw,
        ));
    }

    if is_create {
        if let Some(body_pos) = find_create_as_body_position(sql) {
            return insert_with_at_select(sql, body_pos, cte_defs, with_kw);
        }
        // Fallback for unusual CREATE syntaxes.
        if let Some(pos) = find_main_select_position(sql) {
            return insert_with_at_select(sql, pos, cte_defs, with_kw);
        }
        return None;
    }

    if is_insert {
        // For non-TSQL INSERT: find where SELECT/WITH starts and insert there.
        let select_pos = find_main_select_position(sql);
        if let Some(pos) = select_pos {
            return insert_with_at_select(sql, pos, cte_defs, with_kw);
        }
        return None;
    }

    // Look for existing WITH clause.
    if let Some(with_info) = find_existing_with_clause(sql) {
        // Append new CTEs to existing WITH clause.
        return Some(append_to_existing_with(sql, &with_info, cte_defs));
    }

    // No existing WITH: prepend.
    let insert_pos = skip_ascii_whitespace(bytes, 0);
    Some(insert_with_before_position(
        sql, insert_pos, cte_defs, with_kw,
    ))
}

/// Find the start of a CREATE ... AS body (typically SELECT or WITH).
fn find_create_as_body_position(sql: &str) -> Option<usize> {
    let bytes = sql.as_bytes();
    let mut pos = skip_ascii_whitespace(bytes, 0);
    let create_end = match_ascii_keyword_at(bytes, pos, b"CREATE")?;
    pos = create_end;

    let mut depth = 0usize;
    while pos < bytes.len() {
        if let Some(end) = skip_quoted_region(bytes, pos) {
            pos = end;
            continue;
        }
        if bytes[pos] == b'-' && bytes.get(pos + 1) == Some(&b'-') {
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }
        if bytes[pos] == b'/' && bytes.get(pos + 1) == Some(&b'*') {
            pos += 2;
            while pos + 1 < bytes.len() {
                if bytes[pos] == b'*' && bytes[pos + 1] == b'/' {
                    pos += 2;
                    break;
                }
                pos += 1;
            }
            continue;
        }

        if bytes[pos] == b'(' {
            depth += 1;
            pos += 1;
            continue;
        }
        if bytes[pos] == b')' {
            depth = depth.saturating_sub(1);
            pos += 1;
            continue;
        }

        if depth == 0 {
            if let Some(as_end) = match_ascii_keyword_at(bytes, pos, b"AS") {
                return Some(skip_ascii_whitespace(bytes, as_end));
            }
        }

        pos += 1;
    }

    None
}

struct ExistingWithInfo {
    /// Byte position just after the last CTE definition's closing paren.
    last_cte_end: usize,
}

/// Find the existing WITH clause and return info about where to append.
fn find_existing_with_clause(sql: &str) -> Option<ExistingWithInfo> {
    let bytes = sql.as_bytes();
    let mut pos = skip_ascii_whitespace(bytes, 0);

    // Skip INSERT/CREATE prefix.
    if match_ascii_keyword_at(bytes, pos, b"INSERT").is_some()
        || match_ascii_keyword_at(bytes, pos, b"CREATE").is_some()
    {
        pos = skip_to_with_or_select(bytes, pos + 6);
    }

    let _with_end = match_ascii_keyword_at(bytes, pos, b"WITH")?;
    let mut cursor = skip_ascii_whitespace(bytes, _with_end);

    // Skip RECURSIVE.
    if let Some(end) = match_ascii_keyword_at(bytes, cursor, b"RECURSIVE") {
        cursor = skip_ascii_whitespace(bytes, end);
    }

    // Walk through CTE definitions to find the last one.
    let mut last_cte_end = cursor;
    loop {
        // Skip CTE name.
        if let Some(quoted_end) = consume_quoted_identifier(bytes, cursor) {
            cursor = skip_ascii_whitespace(bytes, quoted_end);
        } else if let Some(name_end) = consume_ascii_identifier(bytes, cursor) {
            cursor = skip_ascii_whitespace(bytes, name_end);
        } else {
            break;
        }

        // AS keyword.
        if let Some(as_end) = match_ascii_keyword_at(bytes, cursor, b"AS") {
            cursor = skip_ascii_whitespace(bytes, as_end);
        } else {
            break;
        }

        // CTE body.
        if cursor < bytes.len() && bytes[cursor] == b'(' {
            if let Some(close) = find_matching_parenthesis_outside_quotes(sql, cursor) {
                last_cte_end = close + 1;
                cursor = skip_ascii_whitespace(bytes, close + 1);
            } else {
                break;
            }
        } else {
            break;
        }

        // Comma means more CTEs.
        if cursor < bytes.len() && bytes[cursor] == b',' {
            cursor += 1;
            cursor = skip_ascii_whitespace(bytes, cursor);
        } else {
            break;
        }
    }

    Some(ExistingWithInfo { last_cte_end })
}

/// Append new CTE definitions after the last existing CTE.
fn append_to_existing_with(sql: &str, with_info: &ExistingWithInfo, cte_defs: &[String]) -> String {
    let insert_pos = with_info.last_cte_end;
    let mut result =
        String::with_capacity(sql.len() + cte_defs.iter().map(|d| d.len() + 4).sum::<usize>());
    result.push_str(&sql[..insert_pos]);
    for def in cte_defs {
        result.push_str(",\n");
        result.push_str(def);
    }
    result.push_str(&sql[insert_pos..]);
    result
}

/// Insert WITH clause before a given position.
fn insert_with_before_position(
    sql: &str,
    pos: usize,
    cte_defs: &[String],
    with_kw: &str,
) -> String {
    let mut result = String::with_capacity(sql.len() + 100);
    result.push_str(&sql[..pos]);
    result.push_str(with_kw);
    result.push(' ');
    for (i, def) in cte_defs.iter().enumerate() {
        if i > 0 {
            result.push_str(",\n");
        }
        result.push_str(def);
    }
    result.push('\n');
    result.push_str(&sql[pos..]);
    result
}

/// Insert WITH clause before a SELECT that is preceded by INSERT/CREATE.
fn insert_with_at_select(
    sql: &str,
    select_pos: usize,
    cte_defs: &[String],
    with_kw: &str,
) -> Option<String> {
    // Check if there's already a WITH clause at this position.
    let bytes = sql.as_bytes();
    if match_ascii_keyword_at(bytes, select_pos, b"WITH").is_some() {
        // Existing WITH at select position — append to it.
        if let Some(with_info) = find_existing_with_clause_at(sql, select_pos) {
            return Some(append_to_existing_with(sql, &with_info, cte_defs));
        }
    }

    Some(insert_with_before_position(
        sql, select_pos, cte_defs, with_kw,
    ))
}

/// Find existing WITH clause starting at a specific position.
fn find_existing_with_clause_at(sql: &str, start: usize) -> Option<ExistingWithInfo> {
    let bytes = sql.as_bytes();
    let _with_end = match_ascii_keyword_at(bytes, start, b"WITH")?;
    let mut cursor = skip_ascii_whitespace(bytes, _with_end);

    // Skip RECURSIVE.
    if let Some(end) = match_ascii_keyword_at(bytes, cursor, b"RECURSIVE") {
        cursor = skip_ascii_whitespace(bytes, end);
    }

    let mut last_cte_end = cursor;
    loop {
        if let Some(quoted_end) = consume_quoted_identifier(bytes, cursor) {
            cursor = skip_ascii_whitespace(bytes, quoted_end);
        } else if let Some(name_end) = consume_ascii_identifier(bytes, cursor) {
            cursor = skip_ascii_whitespace(bytes, name_end);
        } else {
            break;
        }

        if let Some(as_end) = match_ascii_keyword_at(bytes, cursor, b"AS") {
            cursor = skip_ascii_whitespace(bytes, as_end);
        } else {
            break;
        }

        if cursor < bytes.len() && bytes[cursor] == b'(' {
            if let Some(close) = find_matching_parenthesis_outside_quotes(sql, cursor) {
                last_cte_end = close + 1;
                cursor = skip_ascii_whitespace(bytes, close + 1);
            } else {
                break;
            }
        } else {
            break;
        }

        if cursor < bytes.len() && bytes[cursor] == b',' {
            cursor += 1;
            cursor = skip_ascii_whitespace(bytes, cursor);
        } else {
            break;
        }
    }

    Some(ExistingWithInfo { last_cte_end })
}

/// Find the position of the main SELECT keyword in an INSERT or CREATE statement.
fn find_main_select_position(sql: &str) -> Option<usize> {
    let bytes = sql.as_bytes();
    let mut pos = 0usize;
    let mut depth = 0usize;

    while pos < bytes.len() {
        if let Some(end) = skip_quoted_region(bytes, pos) {
            pos = end;
            continue;
        }
        if bytes[pos] == b'-' && bytes.get(pos + 1) == Some(&b'-') {
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }
        if bytes[pos] == b'/' && bytes.get(pos + 1) == Some(&b'*') {
            pos += 2;
            while pos + 1 < bytes.len() {
                if bytes[pos] == b'*' && bytes[pos + 1] == b'/' {
                    pos += 2;
                    break;
                }
                pos += 1;
            }
            continue;
        }

        if bytes[pos] == b'(' {
            depth += 1;
            pos += 1;
            continue;
        }
        if bytes[pos] == b')' {
            depth = depth.saturating_sub(1);
            pos += 1;
            continue;
        }

        // Only at depth 0, look for SELECT or WITH keyword.
        if depth == 0 {
            if match_ascii_keyword_at(bytes, pos, b"WITH").is_some() {
                return Some(pos);
            }
            if match_ascii_keyword_at(bytes, pos, b"SELECT").is_some() {
                return Some(pos);
            }
        }

        pos += 1;
    }
    None
}

/// Skip a quoted region (single quote, double quote, backtick, bracket).
/// Returns the position after the closing quote, or None if not in a quoted region.
fn skip_quoted_region(bytes: &[u8], pos: usize) -> Option<usize> {
    let b = bytes[pos];
    if b == b'\'' {
        return Some(skip_to_close_quote(bytes, pos + 1, b'\''));
    }
    if b == b'"' {
        return Some(skip_to_close_quote(bytes, pos + 1, b'"'));
    }
    if b == b'`' {
        return Some(skip_to_close_quote(bytes, pos + 1, b'`'));
    }
    if b == b'[' {
        return Some(skip_to_close_quote(bytes, pos + 1, b']'));
    }
    None
}

fn skip_to_close_quote(bytes: &[u8], mut pos: usize, close: u8) -> usize {
    while pos < bytes.len() {
        if bytes[pos] == close {
            if bytes.get(pos + 1) == Some(&close) {
                pos += 2; // Escaped quote.
            } else {
                return pos + 1;
            }
        } else {
            pos += 1;
        }
    }
    pos
}

/// Consume a quoted identifier (double-quoted, backtick-quoted, or bracket-quoted).
fn consume_quoted_identifier(bytes: &[u8], pos: usize) -> Option<usize> {
    if pos >= bytes.len() {
        return None;
    }
    match bytes[pos] {
        b'"' => Some(skip_to_close_quote(bytes, pos + 1, b'"')),
        b'`' => Some(skip_to_close_quote(bytes, pos + 1, b'`')),
        b'[' => Some(skip_to_close_quote(bytes, pos + 1, b']')),
        _ => None,
    }
}

/// Match a multi-word JOIN keyword sequence like INNER JOIN, LEFT JOIN, etc.
/// Returns the byte position after the final JOIN keyword.
fn match_join_keyword_sequence(bytes: &[u8], pos: usize) -> Option<usize> {
    // Check for: INNER JOIN, LEFT [OUTER] JOIN, RIGHT [OUTER] JOIN,
    // FULL [OUTER] JOIN, CROSS JOIN, LEFT OUTER JOIN, etc.
    let prefixes: &[&[u8]] = &[b"INNER", b"LEFT", b"RIGHT", b"FULL", b"CROSS", b"NATURAL"];

    for prefix in prefixes {
        if let Some(prefix_end) = match_ascii_keyword_at(bytes, pos, prefix) {
            let mut cursor = skip_ascii_whitespace(bytes, prefix_end);

            // Optional OUTER keyword.
            if let Some(outer_end) = match_ascii_keyword_at(bytes, cursor, b"OUTER") {
                cursor = skip_ascii_whitespace(bytes, outer_end);
            }

            if let Some(join_end) = match_ascii_keyword_at(bytes, cursor, b"JOIN") {
                return Some(join_end);
            }
        }
    }
    None
}

fn find_matching_parenthesis_outside_quotes(sql: &str, open_paren_index: usize) -> Option<usize> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Outside,
        SingleQuote,
        DoubleQuote,
        BacktickQuote,
        BracketQuote,
    }

    let bytes = sql.as_bytes();
    if open_paren_index >= bytes.len() || bytes[open_paren_index] != b'(' {
        return None;
    }

    let mut depth = 0usize;
    let mut mode = Mode::Outside;
    let mut index = open_paren_index;

    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();

        match mode {
            Mode::Outside => {
                if byte == b'\'' {
                    mode = Mode::SingleQuote;
                    index += 1;
                    continue;
                }
                if byte == b'"' {
                    mode = Mode::DoubleQuote;
                    index += 1;
                    continue;
                }
                if byte == b'`' {
                    mode = Mode::BacktickQuote;
                    index += 1;
                    continue;
                }
                if byte == b'[' {
                    mode = Mode::BracketQuote;
                    index += 1;
                    continue;
                }
                if byte == b'(' {
                    depth += 1;
                    index += 1;
                    continue;
                }
                if byte == b')' {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(index);
                    }
                }
                index += 1;
            }
            Mode::SingleQuote => {
                if byte == b'\'' {
                    if next == Some(b'\'') {
                        index += 2;
                    } else {
                        mode = Mode::Outside;
                        index += 1;
                    }
                } else {
                    index += 1;
                }
            }
            Mode::DoubleQuote => {
                if byte == b'"' {
                    if next == Some(b'"') {
                        index += 2;
                    } else {
                        mode = Mode::Outside;
                        index += 1;
                    }
                } else {
                    index += 1;
                }
            }
            Mode::BacktickQuote => {
                if byte == b'`' {
                    if next == Some(b'`') {
                        index += 2;
                    } else {
                        mode = Mode::Outside;
                        index += 1;
                    }
                } else {
                    index += 1;
                }
            }
            Mode::BracketQuote => {
                if byte == b']' {
                    if next == Some(b']') {
                        index += 2;
                    } else {
                        mode = Mode::Outside;
                        index += 1;
                    }
                } else {
                    index += 1;
                }
            }
        }
    }

    None
}

fn is_ascii_whitespace_byte(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t' | 0x0b | 0x0c)
}

fn is_ascii_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ascii_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn skip_ascii_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && is_ascii_whitespace_byte(bytes[index]) {
        index += 1;
    }
    index
}

fn consume_ascii_identifier(bytes: &[u8], start: usize) -> Option<usize> {
    if start >= bytes.len() || !is_ascii_ident_start(bytes[start]) {
        return None;
    }
    let mut index = start + 1;
    while index < bytes.len() && is_ascii_ident_continue(bytes[index]) {
        index += 1;
    }
    Some(index)
}

fn is_word_boundary_for_keyword(bytes: &[u8], index: usize) -> bool {
    index == 0 || index >= bytes.len() || !is_ascii_ident_continue(bytes[index])
}

fn match_ascii_keyword_at(bytes: &[u8], start: usize, keyword_upper: &[u8]) -> Option<usize> {
    let end = start.checked_add(keyword_upper.len())?;
    if end > bytes.len() {
        return None;
    }
    if !is_word_boundary_for_keyword(bytes, start.saturating_sub(1))
        || !is_word_boundary_for_keyword(bytes, end)
    {
        return None;
    }
    let matches = bytes[start..end]
        .iter()
        .zip(keyword_upper.iter())
        .all(|(actual, expected)| actual.to_ascii_uppercase() == *expected);
    if matches {
        Some(end)
    } else {
        None
    }
}

fn table_factor_contains_derived(
    table_factor: &TableFactor,
    outer_source_names: &HashSet<String>,
) -> bool {
    match table_factor {
        TableFactor::Derived { subquery, .. } => {
            !query_references_outer_sources(subquery, outer_source_names)
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            table_factor_contains_derived(&table_with_joins.relation, outer_source_names)
                || table_with_joins
                    .joins
                    .iter()
                    .any(|join| table_factor_contains_derived(&join.relation, outer_source_names))
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            table_factor_contains_derived(table, outer_source_names)
        }
        _ => false,
    }
}

fn query_references_outer_sources(query: &Query, outer_source_names: &HashSet<String>) -> bool {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            if query_references_outer_sources(&cte.query, outer_source_names) {
                return true;
            }
        }
    }

    set_expr_references_outer_sources(&query.body, outer_source_names)
}

fn set_expr_references_outer_sources(
    set_expr: &SetExpr,
    outer_source_names: &HashSet<String>,
) -> bool {
    match set_expr {
        SetExpr::Select(select) => select_references_outer_sources(select, outer_source_names),
        SetExpr::Query(query) => query_references_outer_sources(query, outer_source_names),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_references_outer_sources(left, outer_source_names)
                || set_expr_references_outer_sources(right, outer_source_names)
        }
        _ => false,
    }
}

fn select_references_outer_sources(select: &Select, outer_source_names: &HashSet<String>) -> bool {
    let mut qualifier_prefixes = HashSet::new();
    visit_select_expressions(select, &mut |expr| {
        collect_qualifier_prefixes_in_expr(expr, &mut qualifier_prefixes);
    });

    let local_source_names = source_names_in_select(select);
    if qualifier_prefixes
        .iter()
        .any(|name| outer_source_names.contains(name) && !local_source_names.contains(name))
    {
        return true;
    }

    for table in &select.from {
        if table_factor_references_outer_sources(&table.relation, outer_source_names) {
            return true;
        }
        for join in &table.joins {
            if table_factor_references_outer_sources(&join.relation, outer_source_names) {
                return true;
            }
        }
    }
    false
}

fn table_factor_references_outer_sources(
    table_factor: &TableFactor,
    outer_source_names: &HashSet<String>,
) -> bool {
    match table_factor {
        TableFactor::Derived { subquery, .. } => {
            query_references_outer_sources(subquery, outer_source_names)
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            table_factor_references_outer_sources(&table_with_joins.relation, outer_source_names)
                || table_with_joins.joins.iter().any(|join| {
                    table_factor_references_outer_sources(&join.relation, outer_source_names)
                })
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            table_factor_references_outer_sources(table, outer_source_names)
        }
        _ => false,
    }
}

fn source_names_in_select(select: &Select) -> HashSet<String> {
    let mut names = HashSet::new();
    for table in &select.from {
        collect_source_names_from_table_factor(&table.relation, &mut names);
        for join in &table.joins {
            collect_source_names_from_table_factor(&join.relation, &mut names);
        }
    }
    names
}

fn collect_source_names_from_table_factor(table_factor: &TableFactor, names: &mut HashSet<String>) {
    match table_factor {
        TableFactor::Table { name, alias, .. } => {
            if let Some(last) = name.0.last().and_then(|part| part.as_ident()) {
                names.insert(last.value.to_ascii_uppercase());
            }
            if let Some(alias) = alias {
                names.insert(alias.name.value.to_ascii_uppercase());
            }
        }
        TableFactor::Derived {
            alias, subquery, ..
        } => {
            if let Some(alias) = alias {
                names.insert(alias.name.value.to_ascii_uppercase());
            }
            if let Some(with) = &subquery.with {
                for cte in &with.cte_tables {
                    names.insert(cte.alias.name.value.to_ascii_uppercase());
                }
            }
        }
        TableFactor::TableFunction { alias, .. }
        | TableFactor::Function { alias, .. }
        | TableFactor::UNNEST { alias, .. }
        | TableFactor::JsonTable { alias, .. }
        | TableFactor::OpenJsonTable { alias, .. } => {
            if let Some(alias) = alias {
                names.insert(alias.name.value.to_ascii_uppercase());
            }
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_source_names_from_table_factor(&table_with_joins.relation, names);
            for join in &table_with_joins.joins {
                collect_source_names_from_table_factor(&join.relation, names);
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_source_names_from_table_factor(table, names);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::{config::LintConfig, rule::RuleContext, Linter};
    use crate::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse sql");
        let linter = Linter::new(LintConfig::default());
        let stmt = &statements[0];
        let ctx = RuleContext::new(sql, 0..sql.len(), 0);
        linter.check_statement(stmt, &ctx)
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

    #[test]
    fn default_does_not_flag_subquery_in_from() {
        let issues = run("SELECT * FROM (SELECT * FROM t) sub");
        assert!(!issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn default_flags_subquery_in_join() {
        let issues = run("SELECT * FROM t JOIN (SELECT * FROM u) sub ON t.id = sub.id");
        assert!(issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn default_allows_correlated_subquery_join_without_alias() {
        let issues = run("SELECT pd.* \
             FROM person_dates \
             JOIN (SELECT * FROM events WHERE events.name = person_dates.name)");
        assert!(!issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn default_allows_correlated_subquery_join_with_alias_reference() {
        let issues = run("SELECT pd.* \
             FROM person_dates AS pd \
             JOIN (SELECT * FROM events AS ce WHERE ce.name = pd.name)");
        assert!(!issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn default_allows_correlated_subquery_join_with_outer_table_name_reference() {
        let issues = run("SELECT pd.* \
             FROM person_dates AS pd \
             JOIN (SELECT * FROM events AS ce WHERE ce.name = person_dates.name)");
        assert!(!issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn does_not_flag_cte_usage() {
        let issues = run("WITH sub AS (SELECT * FROM t) SELECT * FROM sub");
        assert!(!issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn does_not_flag_scalar_subquery_in_where() {
        let issues = run("SELECT * FROM t WHERE id IN (SELECT id FROM u)");
        assert!(!issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_ST_005));
    }

    #[test]
    fn forbid_subquery_in_join_does_not_flag_from_subquery() {
        let sql = "SELECT * FROM (SELECT * FROM t) sub";
        let statements = parse_sql(sql).expect("parse sql");
        let rule = StructureSubquery::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "structure.subquery".to_string(),
                serde_json::json!({"forbid_subquery_in": "join"}),
            )]),
        });
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn forbid_subquery_in_from_emits_unsafe_cte_autofix_for_simple_case() {
        let sql = "SELECT * FROM (SELECT 1) sub";
        let statements = parse_sql(sql).expect("parse sql");
        let rule = StructureSubquery::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_ST_005".to_string(),
                serde_json::json!({"forbid_subquery_in": "from"}),
            )]),
        });
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Unsafe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "WITH sub AS (SELECT 1)\nSELECT * FROM sub");
    }

    #[test]
    fn forbid_subquery_in_from_does_not_flag_join_subquery() {
        let sql = "SELECT * FROM t JOIN (SELECT * FROM u) sub ON t.id = sub.id";
        let statements = parse_sql(sql).expect("parse sql");
        let rule = StructureSubquery::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_ST_005".to_string(),
                serde_json::json!({"forbid_subquery_in": "from"}),
            )]),
        });
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn forbid_both_flags_subquery_inside_cte_body() {
        let sql = "WITH b AS (SELECT x, z FROM (SELECT x, z FROM p_cte)) SELECT b.z FROM b";
        let statements = parse_sql(sql).expect("parse sql");
        let rule = StructureSubquery::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "structure.subquery".to_string(),
                serde_json::json!({"forbid_subquery_in": "both"}),
            )]),
        });
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn forbid_both_flags_subqueries_in_set_operation_second_branch() {
        let sql = "SELECT 1 AS value_name UNION SELECT value FROM (SELECT 2 AS value_name) CROSS JOIN (SELECT 1 AS v2)";
        let statements = parse_sql(sql).expect("parse sql");
        let rule = StructureSubquery::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "structure.subquery".to_string(),
                serde_json::json!({"forbid_subquery_in": "both"}),
            )]),
        });
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 2);
    }

    // --- Fixture-based rewriter tests ---

    fn run_fix(sql: &str, forbid_in: &str) -> Option<String> {
        let statements = parse_sql(sql).expect("parse sql");
        let rule = StructureSubquery::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "structure.subquery".to_string(),
                serde_json::json!({"forbid_subquery_in": forbid_in}),
            )]),
        });
        let ctx = RuleContext::new(sql, 0..sql.len(), 0);
        let issues = rule.check_with_context(&statements[0], &ctx);
        if issues.is_empty() {
            return None;
        }
        let st05_issue = issues
            .iter()
            .find(|i| i.code == issue_codes::LINT_ST_005 && i.autofix.is_some())?;
        apply_issue_autofix(sql, st05_issue)
    }

    fn assert_fix_whitespace_eq(actual: &str, expected: &str) {
        let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(
            norm(actual),
            norm(expected),
            "\n--- actual ---\n{actual}\n--- expected ---\n{expected}\n"
        );
    }

    #[test]
    fn fixture_select_fail() {
        let sql = "select\n    a.x, a.y, b.z\nfrom a\njoin (\n    select x, z from b\n) as b on (a.x = b.x)\n";
        let expected = "with b as (\n    select x, z from b\n)\nselect\n    a.x, a.y, b.z\nfrom a\njoin b on (a.x = b.x)\n";
        let fixed = run_fix(sql, "join").expect("should produce fix");
        assert_fix_whitespace_eq(&fixed, expected);
    }

    #[test]
    fn fixture_cte_select_fail() {
        let sql = "with prep as (\n  select 1 as x, 2 as z\n)\nselect\n    a.x, a.y, b.z\nfrom a\njoin (\n    select x, z from b\n) as b on (a.x = b.x)\n";
        let expected = "with prep as (\n  select 1 as x, 2 as z\n),\nb as (\n    select x, z from b\n)\nselect\n    a.x, a.y, b.z\nfrom a\njoin b on (a.x = b.x)\n";
        let fixed = run_fix(sql, "join").expect("should produce fix");
        assert_fix_whitespace_eq(&fixed, expected);
    }

    #[test]
    fn fixture_from_clause_fail() {
        let sql = "select\n    a.x, a.y\nfrom (\n    select * from b\n) as a\n";
        let expected = "with a as (\n    select * from b\n)\nselect\n    a.x, a.y\nfrom a\n";
        let fixed = run_fix(sql, "from").expect("should produce fix");
        assert_fix_whitespace_eq(&fixed, expected);
    }

    #[test]
    fn fixture_both_clause_fail() {
        let sql = "select\n    a.x, a.y\nfrom (\n    select * from b\n) as a\n";
        let expected = "with a as (\n    select * from b\n)\nselect\n    a.x, a.y\nfrom a\n";
        let fixed = run_fix(sql, "both").expect("should produce fix");
        assert_fix_whitespace_eq(&fixed, expected);
    }

    #[test]
    fn fixture_cte_with_clashing_name_generates_prep() {
        let sql = "with prep_1 as (\n  select 1 as x, 2 as z\n)\nselect\n    a.x, a.y, z\nfrom a\njoin (\n    select x, z from b\n) on a.x = z\n";
        let fixed = run_fix(sql, "join").expect("should produce fix");
        // Should generate prep_2 since prep_1 exists.
        assert!(
            fixed.contains("prep_2"),
            "expected prep_2 in output: {fixed}"
        );
    }

    #[test]
    fn fixture_set_subquery_in_second_query() {
        let sql = "SELECT 1 AS value_name\nUNION\nSELECT value\nFROM (SELECT 2 AS value_name);\n";
        let expected = "WITH prep_1 AS (SELECT 2 AS value_name)\nSELECT 1 AS value_name\nUNION\nSELECT value\nFROM prep_1;\n";
        let fixed = run_fix(sql, "both").expect("should produce fix");
        assert_fix_whitespace_eq(&fixed, expected);
    }

    #[test]
    fn fixture_set_subquery_in_second_query_join() {
        let sql = "SELECT 1 AS value_name\nUNION\nSELECT value\nFROM (SELECT 2 AS value_name)\nCROSS JOIN (SELECT 1 as v2);\n";
        let expected = "WITH prep_1 AS (SELECT 2 AS value_name),\nprep_2 AS (SELECT 1 as v2)\nSELECT 1 AS value_name\nUNION\nSELECT value\nFROM prep_1\nCROSS JOIN prep_2;\n";
        let fixed = run_fix(sql, "both").expect("should produce fix");
        assert_fix_whitespace_eq(&fixed, expected);
    }

    #[test]
    fn fixture_with_fail_generates_prep_for_unnamed_subquery() {
        let sql = "select\n    a.x, a.y, b.z\nfrom a\njoin (\n    with d as (\n        select x, z from b\n    )\n    select * from d\n) using (x)\n";
        let fixed = run_fix(sql, "join").expect("should produce fix");
        assert!(
            fixed.contains("prep_1"),
            "expected prep_1 in output: {fixed}"
        );
    }

    #[test]
    fn fixture_set_fail() {
        let sql = "SELECT\n    a.x, a.y, b.z\nFROM a\nJOIN (\n    select x, z from b\n    union\n    select x, z from d\n) USING (x)\n";
        let fixed = run_fix(sql, "join").expect("should produce fix");
        assert!(
            fixed.contains("prep_1"),
            "expected prep_1 in output: {fixed}"
        );
    }

    #[test]
    fn fixture_subquery_in_cte_both() {
        let sql = "with b as (\n  select x, z from (\n    select x, z from p_cte\n  )\n)\nselect b.z\nfrom b\n";
        let expected = "with prep_1 as (\n    select x, z from p_cte\n  ),\nb as (\n  select x, z from prep_1\n)\nselect b.z\nfrom b\n";
        let fixed = run_fix(sql, "both").expect("should produce fix");
        assert_fix_whitespace_eq(&fixed, expected);
    }

    #[test]
    fn fixture_issue_3598_avoid_looping_1() {
        let sql = "WITH cte1 AS (\n    SELECT a\n    FROM (SELECT a)\n)\nSELECT a FROM cte1\n";
        let expected = "WITH prep_1 AS (SELECT a),\ncte1 AS (\n    SELECT a\n    FROM prep_1\n)\nSELECT a FROM cte1\n";
        let fixed = run_fix(sql, "both").expect("should produce fix");
        assert_fix_whitespace_eq(&fixed, expected);
    }

    #[test]
    fn fixture_issue_3598_avoid_looping_2() {
        let sql = "WITH cte1 AS (\n    SELECT *\n    FROM (SELECT * FROM mongo.temp)\n)\nSELECT * FROM cte1\n";
        let expected = "WITH prep_1 AS (SELECT * FROM mongo.temp),\ncte1 AS (\n    SELECT *\n    FROM prep_1\n)\nSELECT * FROM cte1\n";
        let fixed = run_fix(sql, "both").expect("should produce fix");
        assert_fix_whitespace_eq(&fixed, expected);
    }

    #[test]
    fn fixture_multijoin_both() {
        let sql = "select\n    a.x, d.x as foo, a.y, b.z\nfrom (select a, x from foo) a\njoin d using(x)\njoin (\n    select x, z from b\n) as b using (x)\n";
        let fixed = run_fix(sql, "both").expect("should produce fix");
        // Should extract both subqueries.
        assert!(
            fixed.to_ascii_lowercase().contains("with"),
            "expected WITH in output: {fixed}"
        );
    }
}
