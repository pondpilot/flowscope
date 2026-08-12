use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Word};

use crate::analyzer::helpers::line_col_to_offset;
use crate::analyzer::schema_registry::SchemaRegistry;
use crate::types::{
    AstContext, CompletionClause, CompletionColumn, CompletionContext, CompletionItem,
    CompletionItemCategory, CompletionItemKind, CompletionItemsResult, CompletionKeywordHints,
    CompletionKeywordSet, CompletionRequest, CompletionTable, CompletionToken, CompletionTokenKind,
    Dialect, SchemaMetadata, Span,
};

use super::ast_extractor::{extract_ast_context, extract_lateral_aliases};
use super::functions::{get_function_completions, FunctionCompletionContext};
use super::parse_strategies::try_parse_for_completion;

mod analysis;
mod items;

use analysis::*;
use items::*;

/// Maximum SQL input size (10MB) to prevent memory exhaustion.
/// This matches the TypeScript validation limit.
const MAX_SQL_LENGTH: usize = 10 * 1024 * 1024;

// Scoring constants for completion item ranking.
// Higher scores = higher priority in completion list.
//
// Scoring guidelines:
// - Base category scores start at 1000 and decrease by 100 per rank
// - Prefix matches add 100-300 depending on match quality
// - Context-aware adjustments range from -300 to +800
// - Type compatibility adds +100 for matches, -50 for mismatches

/// Bonus for column name prefix matches (when typing matches the column name portion of "table.column")
const SCORE_COLUMN_NAME_MATCH_BONUS: i32 = 150;
/// Bonus for items that are specific to the current clause context
const SCORE_CLAUSE_SPECIFIC_BONUS: i32 = 50;
/// Special boost for FROM keyword when typing 'f' in SELECT clause (most common transition)
const SCORE_FROM_KEYWORD_BOOST: i32 = 800;
/// Penalty for non-FROM keywords when typing 'f' in SELECT clause
const SCORE_OTHER_KEYWORD_PENALTY: i32 = -200;
/// Penalty for function names starting with 'f' to deprioritize vs FROM keyword
const SCORE_F_FUNCTION_PENALTY: i32 = -250;
/// Additional penalty for functions starting with 'from_' (e.g., from_json)
const SCORE_FROM_FUNCTION_PENALTY: i32 = -300;
/// Bonus for columns whose type matches the expected type in comparison context.
/// Applied when the column can be implicitly cast to the expected type (e.g., INT matches INT).
const SCORE_TYPE_COMPATIBLE: i32 = 100;
/// Penalty for columns whose type is incompatible with expected type.
/// Smaller magnitude than bonus to avoid completely hiding potentially useful columns.
const SCORE_TYPE_INCOMPATIBLE: i32 = -50;

#[derive(Debug, Clone)]
struct TokenInfo {
    token: Token,
    span: Span,
}

#[derive(Debug, Clone)]
struct StatementInfo {
    index: usize,
    span: Span,
    tokens: Vec<TokenInfo>,
}

const GLOBAL_KEYWORDS: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "JOIN",
    "LEFT",
    "RIGHT",
    "FULL",
    "INNER",
    "CROSS",
    "OUTER",
    "ON",
    "USING",
    "GROUP",
    "BY",
    "HAVING",
    "ORDER",
    "LIMIT",
    "OFFSET",
    "QUALIFY",
    "WINDOW",
    "INSERT",
    "UPDATE",
    "DELETE",
    "CREATE",
    "ALTER",
    "DROP",
    "VALUES",
    "WITH",
    "DISTINCT",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "ATTACH",
    "DETACH",
    "COPY",
    "EXPORT",
    "IMPORT",
    "PIVOT",
    "UNPIVOT",
    "EXPLAIN",
    "SUMMARIZE",
    "DESCRIBE",
    "SHOW",
];

const OPERATOR_HINTS: &[&str] = &[
    "=", "!=", "<>", "<", "<=", ">", ">=", "+", "-", "*", "/", "%", "||", "AND", "OR", "NOT", "IN",
    "LIKE", "ILIKE", "IS", "IS NOT", "BETWEEN",
];

const AGGREGATE_HINTS: &[&str] = &[
    "COUNT",
    "SUM",
    "AVG",
    "MIN",
    "MAX",
    "ARRAY_AGG",
    "STRING_AGG",
    "BOOL_AND",
    "BOOL_OR",
    "STDDEV",
    "VARIANCE",
];

const SNIPPET_HINTS: &[&str] = &[
    "CASE WHEN ... THEN ... END",
    "COALESCE(expr, ...)",
    "CAST(expr AS type)",
    "COUNT(*)",
    "FILTER (WHERE ...)",
    "OVER (PARTITION BY ...)",
];

const SELECT_KEYWORDS: &[&str] = &[
    "DISTINCT", "ALL", "AS", "CASE", "WHEN", "THEN", "ELSE", "END", "NULLIF", "COALESCE", "CAST",
    "FILTER", "OVER",
];

const FROM_KEYWORDS: &[&str] = &[
    "JOIN", "LEFT", "RIGHT", "FULL", "INNER", "CROSS", "OUTER", "LATERAL", "UNNEST", "AS", "ON",
    "USING",
];

const WHERE_KEYWORDS: &[&str] = &[
    "AND", "OR", "NOT", "IN", "EXISTS", "LIKE", "ILIKE", "IS", "NULL", "TRUE", "FALSE", "BETWEEN",
];

const GROUP_BY_KEYWORDS: &[&str] = &["HAVING", "ROLLUP", "CUBE", "GROUPING", "SETS"];

const ORDER_BY_KEYWORDS: &[&str] = &["ASC", "DESC", "NULLS", "FIRST", "LAST"];

const JOIN_KEYWORDS: &[&str] = &["ON", "USING"];

fn keyword_set_for_clause(clause: CompletionClause) -> CompletionKeywordSet {
    let keywords = match clause {
        CompletionClause::Select => SELECT_KEYWORDS,
        CompletionClause::From => FROM_KEYWORDS,
        CompletionClause::Where | CompletionClause::On => WHERE_KEYWORDS,
        CompletionClause::GroupBy => GROUP_BY_KEYWORDS,
        CompletionClause::OrderBy => ORDER_BY_KEYWORDS,
        CompletionClause::Join => JOIN_KEYWORDS,
        CompletionClause::Limit => &["OFFSET"],
        CompletionClause::Qualify => &["OVER", "WINDOW"],
        CompletionClause::Window => &["PARTITION", "ORDER", "ROWS", "RANGE"],
        CompletionClause::Insert => &["INTO", "VALUES", "SELECT"],
        CompletionClause::Update => &["SET", "WHERE"],
        CompletionClause::Delete => &["FROM", "WHERE"],
        CompletionClause::With => &["AS", "SELECT"],
        CompletionClause::Having => WHERE_KEYWORDS,
        CompletionClause::Unknown => &[],
    };

    CompletionKeywordSet {
        keywords: keywords.iter().map(|k| k.to_string()).collect(),
        operators: OPERATOR_HINTS.iter().map(|op| op.to_string()).collect(),
        aggregates: AGGREGATE_HINTS.iter().map(|agg| agg.to_string()).collect(),
        snippets: SNIPPET_HINTS
            .iter()
            .map(|snippet| snippet.to_string())
            .collect(),
    }
}

fn global_keyword_set() -> CompletionKeywordSet {
    CompletionKeywordSet {
        keywords: GLOBAL_KEYWORDS.iter().map(|k| k.to_string()).collect(),
        operators: OPERATOR_HINTS.iter().map(|op| op.to_string()).collect(),
        aggregates: AGGREGATE_HINTS.iter().map(|agg| agg.to_string()).collect(),
        snippets: SNIPPET_HINTS
            .iter()
            .map(|snippet| snippet.to_string())
            .collect(),
    }
}

#[must_use]
pub fn completion_context(request: &CompletionRequest) -> CompletionContext {
    let sql = request.sql.as_str();
    let sql_len = sql.len();

    // Validate input size to prevent memory exhaustion
    if sql_len > MAX_SQL_LENGTH {
        return CompletionContext::from_error(format!(
            "SQL exceeds maximum length of {} bytes ({} bytes provided)",
            MAX_SQL_LENGTH, sql_len
        ));
    }

    // Validate cursor_offset is within bounds and on a valid UTF-8 char boundary
    if request.cursor_offset > sql_len {
        return CompletionContext::from_error(format!(
            "cursor_offset ({}) exceeds SQL length ({})",
            request.cursor_offset, sql_len
        ));
    }
    if !sql.is_char_boundary(request.cursor_offset) {
        return CompletionContext::from_error(format!(
            "cursor_offset ({}) does not land on a valid UTF-8 character boundary",
            request.cursor_offset
        ));
    }

    // SchemaRegistry::new returns (registry, issues) where issues contains schema validation
    // warnings. We intentionally discard these for completion context since we want to
    // provide completions even when schema metadata has minor issues.
    let (registry, _schema_issues) = SchemaRegistry::new(request.schema.as_ref(), request.dialect);

    let tokens = match tokenize_sql(sql, request.dialect) {
        Ok(tokens) => tokens,
        Err(_) => {
            return CompletionContext::empty();
        }
    };

    let statements = split_statements(&tokens, sql_len);
    let statement = find_statement_for_cursor(&statements, request.cursor_offset);
    let statement_tokens = if statement.tokens.is_empty() {
        token_list_for_statement(&tokens, &statement.span)
    } else {
        statement.tokens.clone()
    };

    let clause = detect_clause(&statement_tokens, request.cursor_offset);
    let token = find_token_at_cursor(&statement_tokens, request.cursor_offset, sql);

    let tables_raw = parse_tables(&statement_tokens);
    let tables = resolve_tables(tables_raw, &registry);

    let columns = build_columns(&tables, &registry);

    CompletionContext {
        statement_index: statement.index,
        statement_span: statement.span,
        clause,
        token,
        tables_in_scope: tables,
        columns_in_scope: columns,
        keyword_hints: CompletionKeywordHints {
            global: global_keyword_set(),
            clause: keyword_set_for_clause(clause),
        },
        error: None,
    }
}

#[must_use]
pub fn completion_items(request: &CompletionRequest) -> CompletionItemsResult {
    let context = completion_context(request);
    if let Some(error) = context.error.clone() {
        return CompletionItemsResult {
            clause: context.clause,
            token: context.token,
            should_show: false,
            items: Vec::new(),
            error: Some(error),
        };
    }

    let token_value = context
        .token
        .as_ref()
        .map(|token| token.value.trim().to_lowercase())
        .unwrap_or_default();

    // Suppress completions when cursor is inside special tokens
    // (string literals, number literals, comments, quoted identifiers)
    if let Some(ref token) = context.token {
        let suppress_inside = matches!(
            token.kind,
            CompletionTokenKind::Literal
                | CompletionTokenKind::Comment
                | CompletionTokenKind::QuotedIdentifier
        );
        if suppress_inside
            && request.cursor_offset > token.span.start
            && request.cursor_offset < token.span.end
        {
            return CompletionItemsResult {
                clause: context.clause,
                token: context.token,
                should_show: false,
                items: Vec::new(),
                error: None,
            };
        }
    }

    let should_show = should_show_for_cursor(&request.sql, request.cursor_offset, &token_value);
    if !should_show {
        return CompletionItemsResult {
            clause: context.clause,
            token: context.token,
            should_show,
            items: Vec::new(),
            error: None,
        };
    }

    // SchemaRegistry::new returns (registry, issues). Issues are intentionally discarded
    // because completion should work even with schema validation warnings.
    let (registry, _schema_issues) = SchemaRegistry::new(request.schema.as_ref(), request.dialect);

    // Tokenize once for scoped qualifier resolution, GROUP BY detection, and type inference.
    let tokens_opt = tokenize_sql(&request.sql, request.dialect).ok();
    let statement_tokens_opt = tokens_opt
        .as_ref()
        .map(|tokens| token_list_for_statement(tokens, &context.statement_span));

    let scoped_tables = statement_tokens_opt
        .as_ref()
        .map(|tokens| {
            resolve_tables(
                parse_tables_in_scope(
                    tokens,
                    request.cursor_offset,
                    context.statement_span.end.max(request.cursor_offset),
                    &registry,
                ),
                &registry,
            )
        })
        .unwrap_or_else(|| context.tables_in_scope.clone());

    let qualifier = extract_qualifier(&request.sql, request.cursor_offset);
    let qualifier_resolution = qualifier.as_ref().and_then(|value| {
        resolve_qualifier(value, &scoped_tables, request.schema.as_ref(), &registry)
    });
    let restrict_to_columns = qualifier_resolution.is_some();

    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut push_item = |item: CompletionItem| {
        let key = format!("{:?}:{}:{}", item.category, item.label, item.insert_text);
        if seen.insert(key) {
            items.push(item);
        }
    };

    if !restrict_to_columns {
        // Add smart function completions with context-aware scoring before keyword hints so they
        // retain signature metadata and clause-specific scoring.
        let group_by_present = statement_tokens_opt
            .as_ref()
            .map(|tokens| has_group_by(tokens))
            .unwrap_or(false);
        let in_window_context = if context.clause == CompletionClause::Window {
            true
        } else {
            statement_tokens_opt
                .as_ref()
                .map(|tokens| in_over_clause(tokens, request.cursor_offset))
                .unwrap_or(false)
        };

        let function_prefix = context.token.as_ref().and_then(|token| match token.kind {
            CompletionTokenKind::Identifier
            | CompletionTokenKind::Keyword
            | CompletionTokenKind::QuotedIdentifier => {
                let trimmed = token.value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        });

        let func_ctx = FunctionCompletionContext {
            clause: context.clause,
            has_group_by: group_by_present,
            in_window_context,
            prefix: function_prefix,
        };

        for item in get_function_completions(&func_ctx) {
            push_item(item);
        }

        for item in items_from_keyword_set(&context.keyword_hints.clause, true) {
            push_item(item);
        }
        for item in items_from_keyword_set(&context.keyword_hints.global, false) {
            push_item(item);
        }
    }

    // Infer type context for WHERE/HAVING/ON clauses
    // This is used for type-aware column scoring (reuses tokens from above)
    let type_context = if matches!(
        context.clause,
        CompletionClause::Where | CompletionClause::Having | CompletionClause::On
    ) {
        statement_tokens_opt.as_ref().and_then(|tokens| {
            infer_type_context(
                tokens,
                request.cursor_offset,
                &request.sql,
                &registry,
                &context.tables_in_scope,
            )
        })
    } else {
        None
    };

    let use_scoped_columns = qualifier_resolution
        .as_ref()
        .is_some_and(|resolution| resolution.target == QualifierTarget::ColumnLabel);
    let mut columns = if use_scoped_columns {
        build_columns(&scoped_tables, &registry)
    } else {
        context.columns_in_scope.clone()
    };
    if columns.is_empty() && context.clause == CompletionClause::Select {
        if let Some(schema) = request.schema.as_ref() {
            columns = build_columns_from_schema(schema, &registry);
        }
    }

    // Try AST-based enrichment for CTE and subquery columns
    let mut tables_enriched = if use_scoped_columns {
        scoped_tables.clone()
    } else {
        context.tables_in_scope.clone()
    };
    let parse_result =
        try_parse_for_completion(&request.sql, request.cursor_offset, request.dialect);
    if let Some(ref result) = parse_result {
        let ast_ctx = extract_ast_context(&result.statements);
        // Enrich tables with CTE definitions
        enrich_tables_from_ast(&mut tables_enriched, &ast_ctx);
        // Enrich columns with CTE and subquery columns
        enrich_columns_from_ast(&mut columns, &tables_enriched, &ast_ctx);
    }

    // Extract lateral aliases for dialects that support them (e.g., DuckDB, BigQuery, Snowflake)
    // Lateral aliases are only available in SELECT clause, without a table qualifier
    let should_add_lateral_aliases = context.clause == CompletionClause::Select
        && request.dialect.lateral_column_alias()
        && !restrict_to_columns;

    if should_add_lateral_aliases {
        if let Some(ref result) = parse_result {
            for alias in extract_lateral_aliases(&result.statements, &request.sql) {
                // Only include aliases within the current statement and before cursor
                let statement_span = context.statement_span;
                if alias.definition_end >= request.cursor_offset
                    || statement_span.end <= statement_span.start
                {
                    continue;
                }
                if alias.definition_end <= statement_span.start
                    || alias.definition_end > statement_span.end
                {
                    continue;
                }
                // Only include aliases from the SELECT projection that contains the cursor
                // This prevents CTE aliases from leaking into outer SELECT scopes
                if request.cursor_offset < alias.projection_start
                    || request.cursor_offset > alias.projection_end
                {
                    continue;
                }
                // Avoid duplicating if the alias name matches an existing column
                let already_exists = columns
                    .iter()
                    .any(|c| c.name.eq_ignore_ascii_case(&alias.name));
                if !already_exists {
                    columns.push(CompletionColumn {
                        name: alias.name,
                        data_type: Some("lateral alias".to_string()),
                        table: None,
                        canonical_table: None,
                        is_ambiguous: false,
                    });
                }
            }
        }
    }

    if let Some(resolution) = qualifier_resolution.as_ref() {
        match resolution.target {
            QualifierTarget::ColumnLabel => {
                if let Some(label) = resolution.label.as_ref() {
                    let normalized = registry.normalize_identifier(label);
                    columns.retain(|column| {
                        column
                            .table
                            .as_ref()
                            .map(|table| registry.normalize_identifier(table) == normalized)
                            .unwrap_or(false)
                    });
                }
            }
            QualifierTarget::SchemaTable => {
                columns = request
                    .schema
                    .as_ref()
                    .map(|schema| {
                        build_columns_for_table(
                            schema,
                            &registry,
                            resolution.schema.as_deref(),
                            resolution.table.as_deref().unwrap_or_default(),
                        )
                    })
                    .unwrap_or_default();
            }
            QualifierTarget::SchemaOnly => {
                columns.clear();
            }
        }
    }

    let schema_has_columns = request
        .schema
        .as_ref()
        .map(|schema| schema.tables.iter().any(|table| !table.columns.is_empty()))
        .unwrap_or(false);
    let schema_provided = request.schema.is_some();

    // Cache emptiness check before consuming columns to avoid clone during iteration
    let has_columns = !columns.is_empty();

    if should_suppress_select_completions(
        context.clause,
        qualifier_resolution.is_some(),
        !has_columns,
        schema_provided,
        schema_has_columns,
    ) {
        return CompletionItemsResult {
            clause: context.clause,
            token: context.token,
            should_show: false,
            items: Vec::new(),
            error: None,
        };
    }

    // Use into_iter() to take ownership of columns, avoiding clones where possible
    for column in columns {
        let (label, insert_text) = if restrict_to_columns {
            // Both label and insert_text are the column name
            let name = column.name;
            (name.clone(), name)
        } else if column.is_ambiguous {
            if let Some(table) = &column.table {
                let label = format!("{table}.{}", column.name);
                let insert_text = label.clone();
                (label, insert_text)
            } else {
                let name = column.name;
                (name.clone(), name)
            }
        } else {
            let name = column.name;
            (name.clone(), name)
        };
        push_item(CompletionItem {
            label,
            insert_text,
            kind: CompletionItemKind::Column,
            category: CompletionItemCategory::Column,
            score: 0,
            clause_specific: true,
            detail: column.data_type,
        });
    }

    let schema_tables_only = qualifier_resolution
        .as_ref()
        .map(|resolution| resolution.target == QualifierTarget::SchemaOnly)
        .unwrap_or(false);

    if schema_tables_only {
        if let Some(schema_name) = qualifier_resolution
            .as_ref()
            .and_then(|resolution| resolution.schema.as_deref())
        {
            if let Some(schema) = request.schema.as_ref() {
                for (label, insert_text) in
                    schema_tables_for_qualifier(schema, &registry, schema_name)
                {
                    push_item(CompletionItem {
                        label,
                        insert_text,
                        kind: CompletionItemKind::SchemaTable,
                        category: CompletionItemCategory::SchemaTable,
                        score: 0,
                        clause_specific: false,
                        detail: None,
                    });
                }
            }
        }
    }

    let suppress_tables = restrict_to_columns
        || schema_tables_only
        || (context.clause == CompletionClause::Select && has_columns);

    if !suppress_tables {
        for table in &tables_enriched {
            let label = table
                .alias
                .as_ref()
                .map(|alias| format!("{alias} ({})", table.name))
                .unwrap_or_else(|| table.name.clone());
            let insert_text = table.alias.clone().unwrap_or_else(|| table.name.clone());
            push_item(CompletionItem {
                label,
                insert_text,
                kind: CompletionItemKind::Table,
                category: CompletionItemCategory::Table,
                score: 0,
                clause_specific: true,
                detail: if table.canonical.is_empty() {
                    None
                } else {
                    Some(table.canonical.clone())
                },
            });
        }

        if let Some(schema) = &request.schema {
            for table in &schema.tables {
                let label = match &table.schema {
                    Some(schema_name) => format!("{schema_name}.{}", table.name),
                    None => table.name.clone(),
                };
                let insert_text = label.clone();
                push_item(CompletionItem {
                    label,
                    insert_text,
                    kind: CompletionItemKind::SchemaTable,
                    category: CompletionItemCategory::SchemaTable,
                    score: 0,
                    clause_specific: false,
                    detail: None,
                });
            }
        }
    }

    for item in items.iter_mut() {
        let precomputed_score = item.score;
        let category_base = category_score(context.clause, item.category);
        let prefix = prefix_score(&item.label, &token_value);
        let column_prefix = if item.category == CompletionItemCategory::Column {
            let column_name = column_name_from_label(&item.label);
            let column_score = prefix_score(column_name, &token_value);
            if column_score > 0 {
                column_score.saturating_add(SCORE_COLUMN_NAME_MATCH_BONUS)
            } else {
                0
            }
        } else {
            0
        };
        let clause_score = if item.clause_specific {
            SCORE_CLAUSE_SPECIFIC_BONUS
        } else {
            0
        };

        // Type compatibility scoring for columns in comparison contexts.
        //
        // Design note: For columns, `item.detail` contains the SQL data type (e.g., "INTEGER").
        // This coupling is intentional - the detail field displays type info in the UI, and we
        // reuse it for type-aware scoring. If `detail` format changes for columns, update
        // `type_compatibility_score` accordingly.
        let type_score = if item.category == CompletionItemCategory::Column {
            if let Some(ref ctx) = type_context {
                type_compatibility_score(item.detail.as_deref(), ctx)
            } else {
                0
            }
        } else {
            0
        };

        let mut special = 0;
        if context.clause == CompletionClause::Select && token_value.starts_with('f') {
            let label_lower = item.label.to_lowercase();
            if item.category == CompletionItemCategory::Keyword && label_lower == "from" {
                special = SCORE_FROM_KEYWORD_BOOST;
            } else if item.category == CompletionItemCategory::Keyword {
                special = SCORE_OTHER_KEYWORD_PENALTY;
            } else if item.kind == CompletionItemKind::Function && label_lower.starts_with("from_")
            {
                special = SCORE_FROM_FUNCTION_PENALTY;
            } else if item.kind == CompletionItemKind::Function && label_lower.starts_with('f') {
                special = SCORE_F_FUNCTION_PENALTY;
            }
        }
        let prefix_score = prefix.max(column_prefix);
        // Use saturating arithmetic to prevent overflow with extreme inputs
        item.score = precomputed_score
            .saturating_add(category_base)
            .saturating_add(prefix_score)
            .saturating_add(clause_score)
            .saturating_add(type_score)
            .saturating_add(special);
    }

    items.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
    });

    CompletionItemsResult {
        clause: context.clause,
        token: context.token,
        should_show,
        items,
        error: None,
    }
}

#[cfg(test)]
mod tests;
