use super::*;

pub(super) fn clause_category_order(clause: CompletionClause) -> &'static [CompletionItemCategory] {
    use CompletionItemCategory as Category;
    match clause {
        CompletionClause::Select => &[
            Category::Column,
            Category::Function,
            Category::Aggregate,
            Category::Table,
            Category::Keyword,
            Category::Operator,
            Category::Snippet,
            Category::SchemaTable,
        ],
        CompletionClause::From | CompletionClause::Join => &[
            Category::Table,
            Category::SchemaTable,
            Category::Keyword,
            Category::Column,
            Category::Function,
            Category::Operator,
            Category::Aggregate,
            Category::Snippet,
        ],
        CompletionClause::On
        | CompletionClause::Where
        | CompletionClause::Having
        | CompletionClause::Qualify => &[
            Category::Column,
            Category::Operator,
            Category::Function,
            Category::Aggregate,
            Category::Keyword,
            Category::Table,
            Category::SchemaTable,
            Category::Snippet,
        ],
        CompletionClause::GroupBy | CompletionClause::OrderBy => &[
            Category::Column,
            Category::Function,
            Category::Aggregate,
            Category::Keyword,
            Category::Table,
            Category::SchemaTable,
            Category::Operator,
            Category::Snippet,
        ],
        CompletionClause::Limit => &[
            Category::Keyword,
            Category::Column,
            Category::Function,
            Category::Aggregate,
            Category::Table,
            Category::SchemaTable,
            Category::Operator,
            Category::Snippet,
        ],
        CompletionClause::Window => &[
            Category::Function,
            Category::Column,
            Category::Keyword,
            Category::Aggregate,
            Category::Table,
            Category::SchemaTable,
            Category::Operator,
            Category::Snippet,
        ],
        CompletionClause::Insert | CompletionClause::Update => &[
            Category::Table,
            Category::SchemaTable,
            Category::Column,
            Category::Keyword,
            Category::Function,
            Category::Operator,
            Category::Aggregate,
            Category::Snippet,
        ],
        CompletionClause::Delete => &[
            Category::Table,
            Category::SchemaTable,
            Category::Keyword,
            Category::Column,
            Category::Function,
            Category::Operator,
            Category::Aggregate,
            Category::Snippet,
        ],
        CompletionClause::With => &[
            Category::Keyword,
            Category::Table,
            Category::SchemaTable,
            Category::Column,
            Category::Function,
            Category::Operator,
            Category::Aggregate,
            Category::Snippet,
        ],
        CompletionClause::Unknown => &[
            Category::Column,
            Category::Table,
            Category::SchemaTable,
            Category::Keyword,
            Category::Function,
            Category::Operator,
            Category::Aggregate,
            Category::Snippet,
        ],
    }
}

pub(super) fn category_score(clause: CompletionClause, category: CompletionItemCategory) -> i32 {
    let order = clause_category_order(clause);
    let index = order
        .iter()
        .position(|item| *item == category)
        .unwrap_or(order.len());
    1000 - (index as i32 * 100)
}

pub(super) fn prefix_score(label: &str, token: &str) -> i32 {
    if token.is_empty() {
        return 0;
    }
    let normalized_label = label.to_lowercase();
    if normalized_label == token {
        return 300;
    }
    if normalized_label.starts_with(token) {
        return 200;
    }
    if normalized_label.contains(token) {
        return 100;
    }
    0
}

/// Extracts the column name portion from a potentially qualified label.
///
/// Used for prefix scoring to match user input against just the column name,
/// even when the label includes a table qualifier for disambiguation.
///
/// # Examples
/// - `"name"` → `"name"`
/// - `"users.name"` → `"name"`
/// - `"public.users.name"` → `"name"`
pub(super) fn column_name_from_label(label: &str) -> &str {
    label.rsplit_once('.').map(|(_, col)| col).unwrap_or(label)
}

pub(super) fn should_show_for_cursor(sql: &str, cursor_offset: usize, token_value: &str) -> bool {
    if !token_value.is_empty() {
        return true;
    }
    // cursor_offset must be > 0 (we need to look at the previous character) and at a
    // valid UTF-8 char boundary. The is_char_boundary check also catches out-of-bounds
    // offsets (returns false for cursor_offset > sql.len()) and handles the case where
    // an external client (e.g., LSP) sends a byte offset in the middle of a multi-byte
    // character.
    if cursor_offset == 0 || !sql.is_char_boundary(cursor_offset) {
        return false;
    }

    // Optimized previous character lookup: O(1) for ASCII (common case),
    // O(n) fallback only for multi-byte UTF-8 characters.
    let prev_byte = sql.as_bytes()[cursor_offset - 1];

    // Fast path: if it's an ASCII byte, we can check directly without UTF-8 decoding
    if prev_byte.is_ascii() {
        let prev_char = prev_byte as char;
        if prev_char == '.' || prev_char == '(' || prev_char == ',' {
            return true;
        }
        // Whitespace after SQL keywords is a valid completion position
        // (e.g., "SELECT |" or "FROM |"). Return true to allow completions.
        if prev_char.is_ascii_whitespace() {
            return true;
        }
        // Not a trigger character - don't show completions in middle of identifiers
        return false;
    }

    // Slow path: non-ASCII byte, need to properly decode UTF-8.
    // This handles multi-byte characters like Unicode whitespace.
    // Find the previous character by scanning backwards to the character boundary.
    // UTF-8 continuation bytes have the pattern 10xxxxxx (0x80-0xBF), so we scan
    // backwards until we find a byte that isn't a continuation byte.
    // This is O(1) bounded since UTF-8 characters are at most 4 bytes.
    let mut char_start = cursor_offset - 1;
    // Safety: UTF-8 characters are at most 4 bytes, so we need at most 3 backward steps
    for _ in 0..3 {
        if char_start == 0 || sql.is_char_boundary(char_start) {
            break;
        }
        char_start -= 1;
    }
    // If we still haven't found a valid boundary, the string is malformed
    if !sql.is_char_boundary(char_start) {
        return false;
    }
    let prev_char = match sql[char_start..cursor_offset].chars().next() {
        Some(ch) => ch,
        None => return false,
    };
    if prev_char == '.' || prev_char == '(' || prev_char == ',' {
        return true;
    }
    if prev_char.is_whitespace() {
        return true;
    }
    // Not a trigger character - don't show completions in middle of identifiers
    false
}

/// Checks if a character is valid in an unquoted SQL identifier.
///
/// Currently only handles ASCII identifiers (alphanumeric, underscore, dollar sign).
/// Note: Some SQL dialects support Unicode identifiers, but this function intentionally
/// restricts to ASCII for consistent cross-dialect behavior. Quoted identifiers can
/// still contain any Unicode characters.
pub(super) fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

/// Extracts the last identifier from a SQL fragment.
///
/// Handles both quoted identifiers (e.g., `"My Table"`) and unquoted identifiers.
/// Returns `None` if the source is empty or contains only non-identifier characters.
///
/// # Examples
/// - `"SELECT users"` → `Some("users")`
/// - `"\"My Table\""` → `Some("My Table")`
/// - `"schema.table"` → `Some("table")`
pub(super) fn extract_last_identifier(source: &str) -> Option<String> {
    let trimmed = source.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(stripped) = trimmed.strip_suffix('"') {
        if let Some(start) = stripped.rfind('"') {
            return Some(stripped[start + 1..].to_string());
        }
    }

    let end = trimmed.len();
    let mut start = end;
    for (idx, ch) in trimmed.char_indices().rev() {
        if is_identifier_char(ch) {
            start = idx;
        } else {
            break;
        }
    }

    if start == end {
        None
    } else {
        Some(trimmed[start..end].to_string())
    }
}

/// Extracts the qualifier (table alias or schema name) from SQL at the cursor position.
///
/// This function identifies when the user is typing after a dot (`.`), indicating
/// they want completions scoped to a specific table, alias, or schema.
///
/// # Examples
/// - `"users."` at offset 6 → `Some("users")` (trailing dot)
/// - `"u.name"` at offset 6 → `Some("u")` (mid-token after dot)
/// - `"SELECT"` at offset 6 → `None` (no qualifier)
///
/// # Safety
/// Returns `None` if `cursor_offset` is out of bounds or not on a valid UTF-8 boundary.
pub(super) fn extract_qualifier(sql: &str, cursor_offset: usize) -> Option<String> {
    if cursor_offset == 0 || cursor_offset > sql.len() {
        return None;
    }
    // Ensure cursor_offset lands on a valid UTF-8 char boundary to prevent panic
    if !sql.is_char_boundary(cursor_offset) {
        return None;
    }

    let prefix = &sql[..cursor_offset];
    let trimmed = prefix.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(stripped) = trimmed.strip_suffix('.') {
        let before_dot = stripped.trim_end();
        return extract_last_identifier(before_dot);
    }

    if let Some(dot_idx) = trimmed.rfind('.') {
        let whitespace_idx = trimmed.rfind(|ch: char| ch.is_whitespace());
        let dot_after_space = whitespace_idx.is_none_or(|space| dot_idx > space);
        if dot_after_space {
            let before_dot = trimmed[..dot_idx].trim_end();
            return extract_last_identifier(before_dot);
        }
    }

    None
}

pub(super) fn build_columns_from_schema(
    schema: &SchemaMetadata,
    registry: &SchemaRegistry,
) -> Vec<CompletionColumn> {
    let mut columns = Vec::new();
    let mut column_counts = std::collections::HashMap::new();

    for table in &schema.tables {
        for column in &table.columns {
            let normalized = registry.normalize_identifier(&column.name);
            *column_counts.entry(normalized).or_insert(0usize) += 1;
        }
    }

    for table in &schema.tables {
        let table_label = table.name.clone();
        for column in &table.columns {
            let normalized = registry.normalize_identifier(&column.name);
            let is_ambiguous = column_counts.get(&normalized).copied().unwrap_or(0) > 1;
            columns.push(CompletionColumn {
                name: column.name.clone(),
                data_type: column.data_type.clone(),
                table: Some(table_label.clone()),
                canonical_table: Some(table_label.clone()),
                is_ambiguous,
            });
        }
    }

    columns
}

pub(super) fn build_columns_for_table(
    schema: &SchemaMetadata,
    registry: &SchemaRegistry,
    target_schema: Option<&str>,
    table_name: &str,
) -> Vec<CompletionColumn> {
    let normalized_target = registry.normalize_identifier(table_name);
    let mut columns = Vec::new();

    for table in &schema.tables {
        let schema_matches = target_schema.is_none_or(|schema_name| {
            table
                .schema
                .as_ref()
                .map(|schema| {
                    registry.normalize_identifier(schema)
                        == registry.normalize_identifier(schema_name)
                })
                .unwrap_or(false)
        });
        if !schema_matches {
            continue;
        }
        if registry.normalize_identifier(&table.name) != normalized_target {
            continue;
        }

        for column in &table.columns {
            columns.push(CompletionColumn {
                name: column.name.clone(),
                data_type: column.data_type.clone(),
                table: Some(table.name.clone()),
                canonical_table: Some(table.name.clone()),
                is_ambiguous: false,
            });
        }
    }

    columns
}

pub(super) fn schema_tables_for_qualifier(
    schema: &SchemaMetadata,
    registry: &SchemaRegistry,
    qualifier: &str,
) -> Vec<(String, String)> {
    let normalized = registry.normalize_identifier(qualifier);
    let mut tables = Vec::new();

    for table in &schema.tables {
        let schema_matches = table
            .schema
            .as_ref()
            .is_some_and(|table_schema| registry.normalize_identifier(table_schema) == normalized);
        let catalog_matches = table
            .catalog
            .as_ref()
            .is_some_and(|catalog| registry.normalize_identifier(catalog) == normalized);

        if schema_matches {
            let label = match table.schema.as_ref() {
                Some(table_schema) => format!("{table_schema}.{}", table.name),
                None => table.name.clone(),
            };
            tables.push((label, table.name.clone()));
            continue;
        }

        if catalog_matches {
            let label = match table.catalog.as_ref() {
                Some(catalog) => format!("{catalog}.{}", table.name),
                None => table.name.clone(),
            };
            tables.push((label, table.name.clone()));
        }
    }

    tables
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum QualifierTarget {
    ColumnLabel,
    SchemaTable,
    SchemaOnly,
}

#[derive(Debug)]
pub(super) struct QualifierResolution {
    pub(super) target: QualifierTarget,
    pub(super) label: Option<String>,
    pub(super) schema: Option<String>,
    pub(super) table: Option<String>,
}

pub(super) fn resolve_qualifier(
    qualifier: &str,
    tables: &[CompletionTable],
    schema: Option<&SchemaMetadata>,
    registry: &SchemaRegistry,
) -> Option<QualifierResolution> {
    let normalized = registry.normalize_identifier(qualifier);

    for table in tables {
        if let Some(alias) = table.alias.as_ref() {
            if registry.normalize_identifier(alias) == normalized {
                return Some(QualifierResolution {
                    target: QualifierTarget::ColumnLabel,
                    label: Some(alias.clone()),
                    schema: None,
                    table: None,
                });
            }
        }
    }

    let schema = schema?;

    let schema_name = schema.tables.iter().find_map(|table| {
        table.schema.as_ref().and_then(|table_schema| {
            if registry.normalize_identifier(table_schema) == normalized {
                Some(table_schema.clone())
            } else {
                None
            }
        })
    });
    let catalog_name = schema.tables.iter().find_map(|table| {
        table.catalog.as_ref().and_then(|catalog| {
            if registry.normalize_identifier(catalog) == normalized {
                Some(catalog.clone())
            } else {
                None
            }
        })
    });
    let table_name_matches_schema = schema
        .tables
        .iter()
        .any(|table| registry.normalize_identifier(&table.name) == normalized);

    if let Some(schema_name) = schema_name.as_ref() {
        if !table_name_matches_schema {
            return Some(QualifierResolution {
                target: QualifierTarget::SchemaOnly,
                label: None,
                schema: Some(schema_name.clone()),
                table: None,
            });
        }
    }

    if let Some(catalog_name) = catalog_name.as_ref() {
        if !table_name_matches_schema {
            return Some(QualifierResolution {
                target: QualifierTarget::SchemaOnly,
                label: None,
                schema: Some(catalog_name.clone()),
                table: None,
            });
        }
    }

    for table in tables {
        if registry.normalize_identifier(&table.name) == normalized {
            let label = table.alias.clone().unwrap_or_else(|| table.name.clone());
            return Some(QualifierResolution {
                target: QualifierTarget::ColumnLabel,
                label: Some(label),
                schema: None,
                table: None,
            });
        }
    }

    for table in &schema.tables {
        if registry.normalize_identifier(&table.name) == normalized {
            return Some(QualifierResolution {
                target: QualifierTarget::SchemaTable,
                label: None,
                schema: table.schema.clone(),
                table: Some(table.name.clone()),
            });
        }
    }

    if let Some(schema_name) = schema_name {
        return Some(QualifierResolution {
            target: QualifierTarget::SchemaOnly,
            label: None,
            schema: Some(schema_name),
            table: None,
        });
    }

    None
}

pub(super) fn uppercase_keyword(value: &str) -> String {
    value.to_ascii_uppercase()
}

/// Determines if completions should be suppressed in SELECT clause.
///
/// Suppresses completions when schema metadata suggests columns should exist
/// but we couldn't derive any for this context. This prevents showing misleading
/// keyword-only completions when the user expects column suggestions.
///
/// Returns `true` (suppress) in these cases:
/// - Schema is provided but contains no column metadata at all
/// - Schema has columns but none could be derived for the current scope
///
/// Returns `false` (show completions) when:
/// - Not in SELECT clause
/// - A qualifier is present (e.g., `users.`)
/// - Columns were successfully derived
/// - No schema metadata was provided
pub(super) fn should_suppress_select_completions(
    clause: CompletionClause,
    has_qualifier: bool,
    columns_empty: bool,
    schema_provided: bool,
    schema_has_columns: bool,
) -> bool {
    // Only applies to SELECT clause without qualifier and no columns
    if clause != CompletionClause::Select || has_qualifier || !columns_empty {
        return false;
    }

    // Suppress when schema is provided but has no column metadata
    if schema_provided && !schema_has_columns {
        return true;
    }

    // Suppress when schema has columns but we couldn't derive any for this context
    if schema_has_columns {
        return true;
    }

    false
}

/// Generate completion items from a keyword set with the given clause_specific flag.
pub(super) fn items_from_keyword_set(
    keyword_set: &CompletionKeywordSet,
    clause_specific: bool,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    for keyword in &keyword_set.keywords {
        let label = uppercase_keyword(keyword);
        items.push(CompletionItem {
            label: label.clone(),
            insert_text: label,
            kind: CompletionItemKind::Keyword,
            category: CompletionItemCategory::Keyword,
            score: 0,
            clause_specific,
            detail: None,
        });
    }

    for operator in &keyword_set.operators {
        items.push(CompletionItem {
            label: operator.clone(),
            insert_text: operator.clone(),
            kind: CompletionItemKind::Operator,
            category: CompletionItemCategory::Operator,
            score: 0,
            clause_specific,
            detail: None,
        });
    }

    for aggregate in &keyword_set.aggregates {
        let label = uppercase_keyword(aggregate);
        items.push(CompletionItem {
            label: label.clone(),
            insert_text: format!("{label}("),
            kind: CompletionItemKind::Function,
            category: CompletionItemCategory::Aggregate,
            score: 0,
            clause_specific,
            detail: None,
        });
    }

    for snippet in &keyword_set.snippets {
        items.push(CompletionItem {
            label: snippet.clone(),
            insert_text: snippet.clone(),
            kind: CompletionItemKind::Snippet,
            category: CompletionItemCategory::Snippet,
            score: 0,
            clause_specific,
            detail: None,
        });
    }

    items
}

/// Enrich columns with CTE and subquery columns from AST context.
///
/// Uses a HashSet for O(1) deduplication instead of O(n²) iteration.
pub(super) fn enrich_columns_from_ast(
    columns: &mut Vec<CompletionColumn>,
    tables: &[CompletionTable],
    ast_ctx: &AstContext,
) {
    use std::collections::HashSet;

    // Build a set of existing (table, column) pairs for O(1) dedup lookups
    // Key: (lowercased_table_name, lowercased_column_name)
    let mut seen: HashSet<(String, String)> = columns
        .iter()
        .filter_map(|c| {
            c.table
                .as_ref()
                .map(|t| (t.to_lowercase(), c.name.to_lowercase()))
        })
        .collect();

    // Add columns from CTEs
    for (cte_name, cte_info) in &ast_ctx.cte_definitions {
        // Check if this CTE is referenced in tables
        let cte_in_scope = tables.iter().any(|t| {
            t.name.eq_ignore_ascii_case(cte_name) || t.canonical.eq_ignore_ascii_case(cte_name)
        });

        if cte_in_scope {
            // Use declared columns if available, otherwise use projected columns
            let cte_columns = if !cte_info.declared_columns.is_empty() {
                cte_info
                    .declared_columns
                    .iter()
                    .map(|name| CompletionColumn {
                        name: name.clone(),
                        table: Some(cte_name.clone()),
                        canonical_table: Some(cte_name.clone()),
                        data_type: None,
                        is_ambiguous: false,
                    })
                    .collect::<Vec<_>>()
            } else {
                cte_info
                    .projected_columns
                    .iter()
                    .filter(|c| c.name != "*") // Skip wildcards
                    .map(|col| CompletionColumn {
                        name: col.name.clone(),
                        table: Some(cte_name.clone()),
                        canonical_table: Some(cte_name.clone()),
                        data_type: col.data_type.clone(),
                        is_ambiguous: false,
                    })
                    .collect::<Vec<_>>()
            };

            for col in cte_columns {
                let key = (cte_name.to_lowercase(), col.name.to_lowercase());
                if seen.insert(key) {
                    columns.push(col);
                }
            }
        }
    }

    // Add columns from subquery aliases
    for (alias, subquery_info) in &ast_ctx.subquery_aliases {
        let subquery_in_scope = tables.iter().any(|t| {
            t.name.eq_ignore_ascii_case(alias)
                || t.alias
                    .as_ref()
                    .map(|a| a.eq_ignore_ascii_case(alias))
                    .unwrap_or(false)
        });

        if subquery_in_scope {
            for col in &subquery_info.projected_columns {
                if col.name == "*" {
                    continue; // Skip wildcards
                }

                let key = (alias.to_lowercase(), col.name.to_lowercase());
                if seen.insert(key) {
                    columns.push(CompletionColumn {
                        name: col.name.clone(),
                        table: Some(alias.clone()),
                        canonical_table: Some(alias.clone()),
                        data_type: col.data_type.clone(),
                        is_ambiguous: false,
                    });
                }
            }
        }
    }
}

/// Enrich tables with CTE definitions from AST context.
pub(super) fn enrich_tables_from_ast(tables: &mut Vec<CompletionTable>, ast_ctx: &AstContext) {
    // Add CTE definitions as completable tables
    for cte_name in ast_ctx.cte_definitions.keys() {
        if !tables.iter().any(|t| t.name.eq_ignore_ascii_case(cte_name)) {
            tables.push(CompletionTable {
                name: cte_name.clone(),
                canonical: cte_name.clone(),
                alias: None,
                matched_schema: false,
            });
        }
    }
}
