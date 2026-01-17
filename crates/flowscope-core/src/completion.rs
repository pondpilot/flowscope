use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Word};

use crate::analyzer::helpers::line_col_to_offset;
use crate::analyzer::schema_registry::SchemaRegistry;
use crate::types::{
    CompletionClause, CompletionColumn, CompletionContext, CompletionItem, CompletionItemCategory,
    CompletionItemKind, CompletionItemsResult, CompletionKeywordHints, CompletionKeywordSet,
    CompletionRequest, CompletionTable, CompletionToken, CompletionTokenKind, Dialect, Span,
};

/// Maximum SQL input size (10MB) to prevent memory exhaustion.
/// This matches the TypeScript validation limit.
const MAX_SQL_LENGTH: usize = 10 * 1024 * 1024;

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
    "=",
    "!=",
    "<>",
    "<",
    "<=",
    ">",
    ">=",
    "+",
    "-",
    "*",
    "/",
    "%",
    "||",
    "AND",
    "OR",
    "NOT",
    "IN",
    "LIKE",
    "ILIKE",
    "IS",
    "IS NOT",
    "BETWEEN",
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
    "DISTINCT",
    "ALL",
    "AS",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "NULLIF",
    "COALESCE",
    "CAST",
    "FILTER",
    "OVER",
];

const FROM_KEYWORDS: &[&str] = &[
    "JOIN",
    "LEFT",
    "RIGHT",
    "FULL",
    "INNER",
    "CROSS",
    "OUTER",
    "LATERAL",
    "UNNEST",
    "AS",
    "ON",
    "USING",
];

const WHERE_KEYWORDS: &[&str] = &[
    "AND",
    "OR",
    "NOT",
    "IN",
    "EXISTS",
    "LIKE",
    "ILIKE",
    "IS",
    "NULL",
    "TRUE",
    "FALSE",
    "BETWEEN",
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
        snippets: SNIPPET_HINTS.iter().map(|snippet| snippet.to_string()).collect(),
    }
}

fn global_keyword_set() -> CompletionKeywordSet {
    CompletionKeywordSet {
        keywords: GLOBAL_KEYWORDS.iter().map(|k| k.to_string()).collect(),
        operators: OPERATOR_HINTS.iter().map(|op| op.to_string()).collect(),
        aggregates: AGGREGATE_HINTS.iter().map(|agg| agg.to_string()).collect(),
        snippets: SNIPPET_HINTS.iter().map(|snippet| snippet.to_string()).collect(),
    }
}

fn token_span_to_offsets(sql: &str, span: &sqlparser::tokenizer::Span) -> Option<Span> {
    let start = line_col_to_offset(sql, span.start.line as usize, span.start.column as usize)?;
    let end = line_col_to_offset(sql, span.end.line as usize, span.end.column as usize)?;
    Some(Span::new(start, end))
}

fn tokenize_sql(sql: &str, dialect: Dialect) -> Result<Vec<TokenInfo>, String> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens: Vec<TokenWithSpan> = tokenizer
        .tokenize_with_location()
        .map_err(|err| err.to_string())?;

    let mut token_infos = Vec::new();
    for token in tokens {
        if matches!(token.token, Token::Whitespace(_)) {
            continue;
        }
        if let Some(span) = token_span_to_offsets(sql, &token.span) {
            token_infos.push(TokenInfo {
                token: token.token,
                span,
            });
        }
    }

    Ok(token_infos)
}

/// Split tokenized SQL into statement boundaries.
///
/// Note: This is intentionally separate from `analyzer/input.rs::compute_statement_ranges`.
/// That function operates on raw SQL text (for parsing before tokenization), while this
/// function works with already-tokenized input and preserves per-statement token lists
/// for clause detection and completion context building.
fn split_statements(tokens: &[TokenInfo], sql_len: usize) -> Vec<StatementInfo> {
    if tokens.is_empty() {
        return vec![StatementInfo {
            index: 0,
            span: Span::new(0, sql_len),
            tokens: Vec::new(),
        }];
    }

    let mut statements = Vec::new();
    let mut current_tokens = Vec::new();
    let mut current_start: Option<usize> = None;
    let mut statement_index = 0;

    for token in tokens {
        if current_start.is_none() {
            current_start = Some(token.span.start);
        }

        if matches!(token.token, Token::SemiColon) {
            let end = token.span.start;
            if let Some(start) = current_start {
                statements.push(StatementInfo {
                    index: statement_index,
                    span: Span::new(start, end.max(start)),
                    tokens: current_tokens.clone(),
                });
                statement_index += 1;
                current_tokens.clear();
                current_start = None;
            }
            continue;
        }

        current_tokens.push(token.clone());
    }

    if let Some(start) = current_start {
        let end = current_tokens
            .last()
            .map(|token| token.span.end)
            .unwrap_or(start);
        statements.push(StatementInfo {
            index: statement_index,
            span: Span::new(start, end.max(start)),
            tokens: current_tokens,
        });
    }

    statements
}

fn find_statement_for_cursor(statements: &[StatementInfo], cursor_offset: usize) -> StatementInfo {
    if statements.is_empty() {
        return StatementInfo {
            index: 0,
            span: Span::new(0, 0),
            tokens: Vec::new(),
        };
    }

    // Cursor is within a statement's bounds
    for statement in statements {
        if cursor_offset >= statement.span.start && cursor_offset <= statement.span.end {
            return statement.clone();
        }
    }

    // Cursor is between statements or after all statements - find the closest preceding statement
    let mut candidate = &statements[0];
    for statement in statements {
        if cursor_offset < statement.span.start {
            return candidate.clone();
        }
        candidate = statement;
    }

    // Cursor is after all statements - return the last one
    candidate.clone()
}

fn keyword_from_token(token: &Token) -> Option<String> {
    match token {
        Token::Word(word) if word.keyword != Keyword::NoKeyword => Some(word.value.to_uppercase()),
        _ => None,
    }
}

fn is_identifier_word(word: &Word) -> bool {
    word.quote_style.is_some() || word.keyword == Keyword::NoKeyword
}

fn detect_clause(tokens: &[TokenInfo], cursor_offset: usize) -> CompletionClause {
    let mut clause = CompletionClause::Unknown;

    for (index, token_info) in tokens.iter().enumerate() {
        if token_info.span.start > cursor_offset {
            break;
        }

        if let Some(keyword) = keyword_from_token(&token_info.token) {
            match keyword.as_str() {
                "SELECT" => clause = CompletionClause::Select,
                "FROM" => clause = CompletionClause::From,
                "WHERE" => clause = CompletionClause::Where,
                "JOIN" => clause = CompletionClause::Join,
                "ON" => clause = CompletionClause::On,
                "HAVING" => clause = CompletionClause::Having,
                "LIMIT" => clause = CompletionClause::Limit,
                "QUALIFY" => clause = CompletionClause::Qualify,
                "WINDOW" => clause = CompletionClause::Window,
                "INSERT" => clause = CompletionClause::Insert,
                "UPDATE" => clause = CompletionClause::Update,
                "DELETE" => clause = CompletionClause::Delete,
                "WITH" => clause = CompletionClause::With,
                "GROUP" => {
                    if let Some(next) = tokens.get(index + 1) {
                        if keyword_from_token(&next.token).as_deref() == Some("BY") {
                            clause = CompletionClause::GroupBy;
                        }
                    }
                }
                "ORDER" => {
                    if let Some(next) = tokens.get(index + 1) {
                        if keyword_from_token(&next.token).as_deref() == Some("BY") {
                            clause = CompletionClause::OrderBy;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    clause
}

fn token_kind(token: &Token) -> CompletionTokenKind {
    match token {
        Token::Word(word) => {
            if word.keyword == Keyword::NoKeyword {
                CompletionTokenKind::Identifier
            } else {
                CompletionTokenKind::Keyword
            }
        }
        Token::Number(_, _)
        | Token::SingleQuotedString(_)
        | Token::DoubleQuotedString(_)
        | Token::NationalStringLiteral(_)
        | Token::EscapedStringLiteral(_)
        | Token::HexStringLiteral(_) => CompletionTokenKind::Literal,
        Token::Eq
        | Token::Neq
        | Token::Lt
        | Token::Gt
        | Token::LtEq
        | Token::GtEq
        | Token::Plus
        | Token::Minus
        | Token::Mul
        | Token::Div
        | Token::Mod
        | Token::StringConcat => CompletionTokenKind::Operator,
        Token::Comma
        | Token::Period
        | Token::LParen
        | Token::RParen
        | Token::SemiColon
        | Token::LBracket
        | Token::RBracket
        | Token::LBrace
        | Token::RBrace
        | Token::Colon
        | Token::DoubleColon
        | Token::Assignment => CompletionTokenKind::Symbol,
        _ => CompletionTokenKind::Unknown,
    }
}

fn find_token_at_cursor(tokens: &[TokenInfo], cursor_offset: usize, sql: &str) -> Option<CompletionToken> {
    for token in tokens {
        if cursor_offset >= token.span.start && cursor_offset <= token.span.end {
            let value = sql
                .get(token.span.start..token.span.end)
                .unwrap_or_default()
                .to_string();
            return Some(CompletionToken {
                value,
                kind: token_kind(&token.token),
                span: token.span,
            });
        }
    }
    None
}

fn parse_tables(tokens: &[TokenInfo]) -> Vec<(String, Option<String>)> {
    let mut tables = Vec::new();
    let mut in_from_clause = false;
    let mut expecting_table = false;
    let mut index = 0;

    while index < tokens.len() {
        let token = &tokens[index].token;
        let keyword = keyword_from_token(token);

        if let Some(keyword) = keyword.as_deref() {
            match keyword {
                "FROM" => {
                    in_from_clause = true;
                    expecting_table = true;
                    index += 1;
                    continue;
                }
                "JOIN" => {
                    expecting_table = true;
                    index += 1;
                    continue;
                }
                "WHERE" | "GROUP" | "ORDER" | "HAVING" | "LIMIT" | "QUALIFY" | "WINDOW" => {
                    in_from_clause = false;
                    expecting_table = false;
                }
                "UPDATE" | "INTO" => {
                    expecting_table = true;
                    index += 1;
                    continue;
                }
                _ => {}
            }
        }

        if in_from_clause && matches!(token, Token::Comma) {
            expecting_table = true;
            index += 1;
            continue;
        }

        if !expecting_table {
            index += 1;
            continue;
        }

        if matches!(token, Token::LParen) {
            let mut depth = 1;
            index += 1;
            while index < tokens.len() && depth > 0 {
                match tokens[index].token {
                    Token::LParen => depth += 1,
                    Token::RParen => depth -= 1,
                    _ => {}
                }
                index += 1;
            }

            let (alias, consumed) = parse_alias(tokens, index);
            tables.push((String::new(), alias));
            index = consumed;

            expecting_table = false;
            continue;
        }

        let (table_name, consumed) = match parse_table_name(tokens, index) {
            Some(result) => result,
            None => {
                index += 1;
                continue;
            }
        };

        let (alias, consumed_alias) = parse_alias(tokens, consumed);
        tables.push((table_name, alias));
        index = consumed_alias;
        expecting_table = false;
    }

    tables
}

fn parse_table_name(tokens: &[TokenInfo], start: usize) -> Option<(String, usize)> {
    let mut parts = Vec::new();
    let mut index = start;

    loop {
        let token = tokens.get(index)?;
        match &token.token {
            Token::Word(word) if is_identifier_word(word) => {
                parts.push(word.value.clone());
                index += 1;
            }
            _ => break,
        }

        if matches!(tokens.get(index).map(|t| &t.token), Some(Token::Period)) {
            index += 1;
            continue;
        }
        break;
    }

    if parts.is_empty() {
        None
    } else {
        Some((parts.join("."), index))
    }
}

fn parse_alias(tokens: &[TokenInfo], start: usize) -> (Option<String>, usize) {
    let mut index = start;

    if let Some(token) = tokens.get(index) {
        if keyword_from_token(&token.token).as_deref() == Some("AS") {
            index += 1;
        }
    }

    if let Some(token) = tokens.get(index) {
        if let Token::Word(word) = &token.token {
            if is_identifier_word(word) {
                return (Some(word.value.clone()), index + 1);
            }
        }
    }

    (None, index)
}

fn build_columns(
    tables: &[CompletionTable],
    registry: &SchemaRegistry,
) -> Vec<CompletionColumn> {
    let mut columns = Vec::new();
    let mut column_counts = std::collections::HashMap::new();

    for table in tables {
        if table.canonical.is_empty() {
            continue;
        }
        if let Some(entry) = registry.get(&table.canonical) {
            for column in &entry.table.columns {
                let normalized = registry.normalize_identifier(&column.name);
                *column_counts.entry(normalized).or_insert(0usize) += 1;
            }
        }
    }

    for table in tables {
        if table.canonical.is_empty() {
            continue;
        }
        let table_label = table.alias.clone().unwrap_or_else(|| table.name.clone());
        if let Some(entry) = registry.get(&table.canonical) {
            for column in &entry.table.columns {
                let normalized = registry.normalize_identifier(&column.name);
                let is_ambiguous = column_counts.get(&normalized).copied().unwrap_or(0) > 1;
                columns.push(CompletionColumn {
                    name: column.name.clone(),
                    data_type: column.data_type.clone(),
                    table: Some(table_label.clone()),
                    canonical_table: Some(table.canonical.clone()),
                    is_ambiguous,
                });
            }
        }
    }

    columns
}

fn token_list_for_statement(tokens: &[TokenInfo], span: &Span) -> Vec<TokenInfo> {
    tokens
        .iter()
        .filter(|token| token.span.start >= span.start && token.span.end <= span.end)
        .cloned()
        .collect()
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

    let (registry, _) = SchemaRegistry::new(request.schema.as_ref(), request.dialect);

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
    let mut tables = Vec::new();

    for (name, alias) in tables_raw {
        if name.is_empty() {
            tables.push(CompletionTable {
                name: name.clone(),
                canonical: String::new(),
                alias,
                matched_schema: false,
            });
            continue;
        }

        let resolution = registry.canonicalize_table_reference(&name);
        tables.push(CompletionTable {
            name,
            canonical: resolution.canonical,
            alias,
            matched_schema: resolution.matched_schema,
        });
    }

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

fn clause_category_order(clause: CompletionClause) -> &'static [CompletionItemCategory] {
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

fn category_score(clause: CompletionClause, category: CompletionItemCategory) -> i32 {
    let order = clause_category_order(clause);
    let index = order
        .iter()
        .position(|item| *item == category)
        .unwrap_or(order.len());
    1000 - (index as i32 * 100)
}

fn prefix_score(label: &str, token: &str) -> i32 {
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

fn should_show_for_cursor(sql: &str, cursor_offset: usize, token_value: &str) -> bool {
    if !token_value.is_empty() {
        return true;
    }
    if cursor_offset == 0 || cursor_offset > sql.len() {
        return false;
    }
    let bytes = sql.as_bytes();
    let prev = bytes[cursor_offset.saturating_sub(1)];
    let prev_char = prev as char;
    if prev_char == '.' || prev_char == '(' || prev_char == ',' {
        return true;
    }
    if prev_char.is_whitespace() {
        return false;
    }
    true
}

fn uppercase_keyword(value: &str) -> String {
    value.to_ascii_uppercase()
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

    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut push_item = |item: CompletionItem| {
        let key = format!("{:?}:{}:{}", item.category, item.label, item.insert_text);
        if seen.insert(key) {
            items.push(item);
        }
    };

    for keyword in &context.keyword_hints.clause.keywords {
        let label = uppercase_keyword(keyword);
        push_item(CompletionItem {
            label: label.clone(),
            insert_text: label,
            kind: CompletionItemKind::Keyword,
            category: CompletionItemCategory::Keyword,
            score: 0,
            clause_specific: true,
            detail: None,
        });
    }

    for operator in &context.keyword_hints.clause.operators {
        push_item(CompletionItem {
            label: operator.clone(),
            insert_text: operator.clone(),
            kind: CompletionItemKind::Operator,
            category: CompletionItemCategory::Operator,
            score: 0,
            clause_specific: true,
            detail: None,
        });
    }

    for aggregate in &context.keyword_hints.clause.aggregates {
        let label = uppercase_keyword(aggregate);
        push_item(CompletionItem {
            label: label.clone(),
            insert_text: format!("{label}("),
            kind: CompletionItemKind::Function,
            category: CompletionItemCategory::Aggregate,
            score: 0,
            clause_specific: true,
            detail: None,
        });
    }

    for snippet in &context.keyword_hints.clause.snippets {
        push_item(CompletionItem {
            label: snippet.clone(),
            insert_text: snippet.clone(),
            kind: CompletionItemKind::Snippet,
            category: CompletionItemCategory::Snippet,
            score: 0,
            clause_specific: true,
            detail: None,
        });
    }

    for keyword in &context.keyword_hints.global.keywords {
        let label = uppercase_keyword(keyword);
        push_item(CompletionItem {
            label: label.clone(),
            insert_text: label,
            kind: CompletionItemKind::Keyword,
            category: CompletionItemCategory::Keyword,
            score: 0,
            clause_specific: false,
            detail: None,
        });
    }

    for operator in &context.keyword_hints.global.operators {
        push_item(CompletionItem {
            label: operator.clone(),
            insert_text: operator.clone(),
            kind: CompletionItemKind::Operator,
            category: CompletionItemCategory::Operator,
            score: 0,
            clause_specific: false,
            detail: None,
        });
    }

    for aggregate in &context.keyword_hints.global.aggregates {
        let label = uppercase_keyword(aggregate);
        push_item(CompletionItem {
            label: label.clone(),
            insert_text: format!("{label}("),
            kind: CompletionItemKind::Function,
            category: CompletionItemCategory::Aggregate,
            score: 0,
            clause_specific: false,
            detail: None,
        });
    }

    for snippet in &context.keyword_hints.global.snippets {
        push_item(CompletionItem {
            label: snippet.clone(),
            insert_text: snippet.clone(),
            kind: CompletionItemKind::Snippet,
            category: CompletionItemCategory::Snippet,
            score: 0,
            clause_specific: false,
            detail: None,
        });
    }

    for column in &context.columns_in_scope {
        let label = if column.is_ambiguous {
            if let Some(table) = &column.table {
                format!("{table}.{}", column.name)
            } else {
                column.name.clone()
            }
        } else {
            column.name.clone()
        };
        push_item(CompletionItem {
            label: label.clone(),
            insert_text: label,
            kind: CompletionItemKind::Column,
            category: CompletionItemCategory::Column,
            score: 0,
            clause_specific: true,
            detail: column.data_type.clone(),
        });
    }

    for table in &context.tables_in_scope {
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

    for item in items.iter_mut() {
        let base = category_score(context.clause, item.category);
        let prefix = prefix_score(&item.label, &token_value);
        let clause_score = if item.clause_specific { 50 } else { 0 };
        let mut special = 0;
        if context.clause == CompletionClause::Select && token_value.starts_with('f') {
            let label_lower = item.label.to_lowercase();
            if item.category == CompletionItemCategory::Keyword && label_lower == "from" {
                special = 800;
            } else if item.category == CompletionItemCategory::Keyword {
                special = -200;
            } else if item.kind == CompletionItemKind::Function && label_lower.starts_with("from_") {
                special = -300;
            } else if item.kind == CompletionItemKind::Function && label_lower.starts_with('f') {
                special = -250;
            }
        }
        item.score = base + prefix + clause_score + special;
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
mod tests {
    use super::*;
    use crate::types::{ColumnSchema, CompletionClause, CompletionRequest, Dialect, SchemaMetadata, SchemaTable};

    #[test]
    fn test_completion_clause_detection() {
        let sql = "SELECT * FROM users WHERE ";
        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            // Cursor at end of string (after trailing space)
            cursor_offset: sql.len(),
            schema: None,
        };

        let context = completion_context(&request);
        assert_eq!(context.clause, CompletionClause::Where);
    }

    #[test]
    fn test_completion_tables_and_columns() {
        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![
                SchemaTable {
                    catalog: None,
                    schema: Some("public".to_string()),
                    name: "users".to_string(),
                    columns: vec![
                        ColumnSchema {
                            name: "id".to_string(),
                            data_type: Some("integer".to_string()),
                            is_primary_key: None,
                            foreign_key: None,
                        },
                        ColumnSchema {
                            name: "name".to_string(),
                            data_type: Some("varchar".to_string()),
                            is_primary_key: None,
                            foreign_key: None,
                        },
                    ],
                },
                SchemaTable {
                    catalog: None,
                    schema: Some("public".to_string()),
                    name: "orders".to_string(),
                    columns: vec![
                        ColumnSchema {
                            name: "id".to_string(),
                            data_type: Some("integer".to_string()),
                            is_primary_key: None,
                            foreign_key: None,
                        },
                        ColumnSchema {
                            name: "user_id".to_string(),
                            data_type: Some("integer".to_string()),
                            is_primary_key: None,
                            foreign_key: None,
                        },
                    ],
                },
            ],
        };

        let sql = "SELECT u. FROM users u JOIN orders o ON u.id = o.user_id";
        let cursor_offset = sql.find("u.").unwrap() + 2;

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: Some(schema),
        };

        let context = completion_context(&request);
        assert_eq!(context.tables_in_scope.len(), 2);
        assert!(context.columns_in_scope.iter().any(|col| col.name == "name"));
        assert!(context.columns_in_scope.iter().any(|col| col.name == "user_id"));
        assert!(context.columns_in_scope.iter().any(|col| col.name == "id" && col.is_ambiguous));
    }
}
