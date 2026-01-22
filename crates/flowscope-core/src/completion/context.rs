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

fn token_span_to_offsets(sql: &str, span: &sqlparser::tokenizer::Span) -> Option<Span> {
    let start = line_col_to_offset(sql, span.start.line as usize, span.start.column as usize)?;
    let end = line_col_to_offset(sql, span.end.line as usize, span.end.column as usize)?;
    Some(Span::new(start, end))
}

fn tokenize_sql(sql: &str, dialect: Dialect) -> Result<Vec<TokenInfo>, String> {
    use sqlparser::tokenizer::Whitespace;

    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens: Vec<TokenWithSpan> = tokenizer
        .tokenize_with_location()
        .map_err(|err| err.to_string())?;

    let mut token_infos = Vec::new();
    for token in tokens {
        // Skip regular whitespace but keep comments for cursor detection
        if let Token::Whitespace(ws) = &token.token {
            match ws {
                Whitespace::SingleLineComment { .. } | Whitespace::MultiLineComment(_) => {
                    // Keep comment tokens
                }
                _ => continue, // Skip spaces, newlines, tabs
            }
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

/// Detects whether the statement contains a GROUP BY clause.
///
/// This is used for context-aware function scoring - aggregates get boosted
/// when GROUP BY is present.
fn has_group_by(tokens: &[TokenInfo]) -> bool {
    for (index, token_info) in tokens.iter().enumerate() {
        if let Some(keyword) = keyword_from_token(&token_info.token) {
            if keyword == "GROUP" {
                if let Some(next) = tokens.get(index + 1) {
                    if keyword_from_token(&next.token).as_deref() == Some("BY") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Detects whether the cursor is currently inside an `OVER (...)` window clause.
///
/// Clause detection never reports `CompletionClause::Window` when typing inside
/// regular `OVER` expressions, so we manually track parentheses that follow an
/// `OVER` keyword before the cursor position.
fn in_over_clause(tokens: &[TokenInfo], cursor_offset: usize) -> bool {
    let mut pending_over = false;
    let mut paren_depth: usize = 0;
    let mut over_stack: Vec<usize> = Vec::new();

    for token_info in tokens {
        if token_info.span.start >= cursor_offset {
            break;
        }

        match &token_info.token {
            Token::Word(word) => {
                if word.keyword == Keyword::NoKeyword {
                    pending_over = false;
                } else if keyword_from_token(&token_info.token).as_deref() == Some("OVER") {
                    pending_over = true;
                }
            }
            Token::LParen => {
                paren_depth = paren_depth.saturating_add(1);
                if pending_over {
                    over_stack.push(paren_depth);
                    pending_over = false;
                }
            }
            Token::RParen => {
                if paren_depth > 0 {
                    if over_stack.last() == Some(&paren_depth) {
                        over_stack.pop();
                    }
                    paren_depth -= 1;
                }
                if pending_over {
                    pending_over = false;
                }
            }
            Token::Whitespace(_) => {}
            _ => {
                if pending_over {
                    pending_over = false;
                }
            }
        }
    }

    !over_stack.is_empty()
}

use crate::generated::{can_implicitly_cast, normalize_type_name, CanonicalType};

/// Represents the expected type context for completion scoring.
///
/// When the cursor is in a binary expression context (e.g., `WHERE age > |`),
/// we can infer the expected type from the left operand and score columns
/// by type compatibility.
#[derive(Debug, Clone)]
pub(crate) struct TypeContext {
    /// The expected canonical type for completions
    pub expected_type: CanonicalType,
    /// The column/expression name that provided the expected type (for debugging)
    #[allow(dead_code)]
    pub source_name: String,
}

/// Attempts to infer the expected type context from the tokens before the cursor.
///
/// This is used in WHERE, HAVING, and ON clauses to boost type-compatible columns.
/// For example, in `WHERE age > |`, we detect that `age` is an INTEGER and boost
/// integer-compatible columns in the completion list.
///
/// # Supported patterns
/// - `column > |` - simple comparison
/// - `(column) > |` - parenthesized column
/// - `NOT column > |` - NOT prefix (skipped)
/// - `((column)) > |` - nested parentheses
///
/// # Boundary conditions
/// - `column > 10 AND |` - returns None (new expression after AND/OR)
/// - `WHERE |` - returns None (no comparison context)
fn infer_type_context(
    tokens: &[TokenInfo],
    cursor_offset: usize,
    sql: &str,
    registry: &SchemaRegistry,
    tables: &[CompletionTable],
) -> Option<TypeContext> {
    // Collect tokens before cursor
    let tokens_before: Vec<&TokenInfo> = tokens
        .iter()
        .filter(|t| t.span.end <= cursor_offset)
        .collect();

    if tokens_before.is_empty() {
        return None;
    }

    // Phase 1: Walk backward to find comparison operator, skipping balanced parentheses
    let mut idx = tokens_before.len();
    let mut paren_depth: i32 = 0;
    let mut comparison_idx: Option<usize> = None;

    while idx > 0 {
        idx -= 1;
        let token = &tokens_before[idx].token;

        match token {
            // Track parentheses (walking backward: ) increases depth, ( decreases)
            Token::RParen => {
                paren_depth += 1;
            }
            Token::LParen => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    // Unbalanced - we've gone past the start of this expression
                    return None;
                }
            }
            // AND/OR mark a boolean boundary - cursor is in a new expression
            Token::Word(word)
                if paren_depth == 0
                    && matches!(word.keyword, Keyword::AND | Keyword::OR) =>
            {
                return None;
            }
            // Clause boundaries - stop searching
            Token::Word(word)
                if paren_depth == 0
                    && matches!(
                        word.keyword,
                        Keyword::WHERE
                            | Keyword::FROM
                            | Keyword::SELECT
                            | Keyword::HAVING
                            | Keyword::ON
                            | Keyword::JOIN
                    ) =>
            {
                return None;
            }
            // Found comparison operator at depth 0
            Token::Eq | Token::Neq | Token::Lt | Token::Gt | Token::LtEq | Token::GtEq
                if paren_depth == 0 =>
            {
                comparison_idx = Some(idx);
                break;
            }
            _ => {}
        }
    }

    let comp_idx = comparison_idx?;
    if comp_idx == 0 {
        return None; // No tokens before the operator
    }

    // Phase 2: Find identifier before the comparison operator, skipping NOT and parentheses
    // For `(age) > |`, we need to find `age` which is inside the parens
    idx = comp_idx;
    paren_depth = 0;

    while idx > 0 {
        idx -= 1;
        let token = &tokens_before[idx].token;

        match token {
            // Track closing parens (walking backward: ) increases depth)
            Token::RParen => {
                paren_depth += 1;
            }
            // Track opening parens (walking backward: ( decreases depth)
            Token::LParen => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    return None; // Unbalanced - we've exited the expression
                }
            }
            // Skip NOT keyword (unary prefix)
            Token::Word(word) if word.keyword == Keyword::NOT => {
                continue;
            }
            // AND/OR boundary at depth 0 - stop
            Token::Word(word)
                if paren_depth == 0 && matches!(word.keyword, Keyword::AND | Keyword::OR) =>
            {
                return None;
            }
            // Found identifier - accept at any depth (it's inside grouping parens)
            // For `(age) > |`, we find `age` at depth 1
            Token::Word(word) if word.keyword == Keyword::NoKeyword => {
                let identifier = sql
                    .get(tokens_before[idx].span.start..tokens_before[idx].span.end)
                    .unwrap_or(&word.value)
                    .to_string();

                // Look up type in schema
                for table in tables {
                    if let Some(data_type) =
                        registry.lookup_column_type(&table.canonical, &identifier)
                    {
                        if let Some(canonical_type) = normalize_type_name(&data_type) {
                            return Some(TypeContext {
                                expected_type: canonical_type,
                                source_name: identifier,
                            });
                        }
                    }
                }
                return None; // Identifier found but not in schema
            }
            _ => {}
        }
    }

    None
}

/// Calculates a type compatibility score for a column given an expected type.
///
/// Returns a positive score bonus for compatible types, negative for incompatible.
/// Compatibility is determined by whether the column type can be implicitly cast
/// to the expected type (one direction only).
fn type_compatibility_score(column_type: Option<&str>, expected: &TypeContext) -> i32 {
    match column_type.and_then(normalize_type_name) {
        Some(col_type) => {
            // Check if column type can be cast TO expected type
            // (e.g., for "age > |" where age is INTEGER, we want other integers)
            if col_type == expected.expected_type
                || can_implicitly_cast(col_type, expected.expected_type)
            {
                SCORE_TYPE_COMPATIBLE
            } else {
                SCORE_TYPE_INCOMPATIBLE
            }
        }
        None => {
            // Unknown type - no adjustment
            0
        }
    }
}

fn token_kind(token: &Token) -> CompletionTokenKind {
    use sqlparser::tokenizer::Whitespace;

    match token {
        Token::Word(word) => {
            // Quoted identifiers (double quotes, backticks, brackets depending on dialect)
            // should suppress completions when cursor is inside them
            if word.quote_style.is_some() {
                CompletionTokenKind::QuotedIdentifier
            } else if word.keyword == Keyword::NoKeyword {
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
        // Comments (line and block)
        Token::Whitespace(Whitespace::SingleLineComment { .. })
        | Token::Whitespace(Whitespace::MultiLineComment(_)) => CompletionTokenKind::Comment,
        _ => CompletionTokenKind::Unknown,
    }
}

fn find_token_at_cursor(
    tokens: &[TokenInfo],
    cursor_offset: usize,
    sql: &str,
) -> Option<CompletionToken> {
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
            // Accept any word token in table name context.
            // SQL keywords like PUBLIC, USER, TABLE are commonly used as schema/table names.
            Token::Word(word) => {
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

fn build_columns(tables: &[CompletionTable], registry: &SchemaRegistry) -> Vec<CompletionColumn> {
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

/// Extracts the column name portion from a potentially qualified label.
///
/// Used for prefix scoring to match user input against just the column name,
/// even when the label includes a table qualifier for disambiguation.
///
/// # Examples
/// - `"name"` → `"name"`
/// - `"users.name"` → `"name"`
/// - `"public.users.name"` → `"name"`
fn column_name_from_label(label: &str) -> &str {
    label.rsplit_once('.').map(|(_, col)| col).unwrap_or(label)
}

fn should_show_for_cursor(sql: &str, cursor_offset: usize, token_value: &str) -> bool {
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
fn is_identifier_char(ch: char) -> bool {
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
fn extract_last_identifier(source: &str) -> Option<String> {
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
fn extract_qualifier(sql: &str, cursor_offset: usize) -> Option<String> {
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

fn build_columns_from_schema(
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

fn build_columns_for_table(
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

fn schema_tables_for_qualifier(
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
enum QualifierTarget {
    ColumnLabel,
    SchemaTable,
    SchemaOnly,
}

#[derive(Debug)]
struct QualifierResolution {
    target: QualifierTarget,
    label: Option<String>,
    schema: Option<String>,
    table: Option<String>,
}

fn resolve_qualifier(
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

fn uppercase_keyword(value: &str) -> String {
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
fn should_suppress_select_completions(
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
fn items_from_keyword_set(
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
fn enrich_columns_from_ast(
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
fn enrich_tables_from_ast(tables: &mut Vec<CompletionTable>, ast_ctx: &AstContext) {
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
    let qualifier = extract_qualifier(&request.sql, request.cursor_offset);
    let qualifier_resolution = qualifier.as_ref().and_then(|value| {
        resolve_qualifier(
            value,
            &context.tables_in_scope,
            request.schema.as_ref(),
            &registry,
        )
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

    // Tokenize once for GROUP BY detection and type context inference
    let tokens_opt = tokenize_sql(&request.sql, request.dialect).ok();
    let statement_tokens_opt = tokens_opt
        .as_ref()
        .map(|tokens| token_list_for_statement(tokens, &context.statement_span));

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

    let mut columns = context.columns_in_scope.clone();
    if columns.is_empty() && context.clause == CompletionClause::Select {
        if let Some(schema) = request.schema.as_ref() {
            columns = build_columns_from_schema(schema, &registry);
        }
    }

    // Try AST-based enrichment for CTE and subquery columns
    let mut tables_enriched = context.tables_in_scope.clone();
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
mod tests {
    use super::*;
    use crate::types::{
        ColumnSchema, CompletionClause, CompletionItemCategory, CompletionRequest, Dialect,
        SchemaMetadata, SchemaTable,
    };

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
        assert!(context
            .columns_in_scope
            .iter()
            .any(|col| col.name == "name"));
        assert!(context
            .columns_in_scope
            .iter()
            .any(|col| col.name == "user_id"));
        assert!(context
            .columns_in_scope
            .iter()
            .any(|col| col.name == "id" && col.is_ambiguous));
    }

    #[test]
    fn test_completion_items_respects_table_qualifier() {
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
                    columns: vec![ColumnSchema {
                        name: "total".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    }],
                },
            ],
        };

        let sql = "SELECT u. FROM users u";
        let cursor_offset = sql.find("u.").unwrap() + 2;

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: Some(schema),
        };

        let result = completion_items(&request);
        assert!(result.should_show);
        assert!(result
            .items
            .iter()
            .all(|item| item.category == CompletionItemCategory::Column));
        assert!(result.items.iter().any(|item| item.label == "id"));
        assert!(!result.items.iter().any(|item| item.label == "total"));
    }

    #[test]
    fn test_completion_items_select_prefers_columns_over_tables() {
        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![ColumnSchema {
                    name: "email".to_string(),
                    data_type: Some("varchar".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                }],
            }],
        };

        let sql = "SELECT e";
        let cursor_offset = sql.len();

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: Some(schema),
        };

        let result = completion_items(&request);
        assert!(result.should_show);
        assert!(result
            .items
            .iter()
            .any(|item| item.category == CompletionItemCategory::Column));
        assert!(!result
            .items
            .iter()
            .any(|item| item.category == CompletionItemCategory::Table));
        assert!(!result
            .items
            .iter()
            .any(|item| item.category == CompletionItemCategory::SchemaTable));
    }

    // Unit tests for string helper functions

    #[test]
    fn test_extract_last_identifier_simple() {
        assert_eq!(extract_last_identifier("users"), Some("users".to_string()));
        assert_eq!(
            extract_last_identifier("foo_bar"),
            Some("foo_bar".to_string())
        );
        assert_eq!(
            extract_last_identifier("table123"),
            Some("table123".to_string())
        );
    }

    #[test]
    fn test_extract_last_identifier_with_spaces() {
        assert_eq!(
            extract_last_identifier("SELECT users"),
            Some("users".to_string())
        );
        assert_eq!(extract_last_identifier("users "), Some("users".to_string()));
        assert_eq!(
            extract_last_identifier("  users  "),
            Some("users".to_string())
        );
    }

    #[test]
    fn test_extract_last_identifier_quoted() {
        assert_eq!(
            extract_last_identifier("\"MyTable\""),
            Some("MyTable".to_string())
        );
        assert_eq!(
            extract_last_identifier("SELECT \"My Table\""),
            Some("My Table".to_string())
        );
        assert_eq!(
            extract_last_identifier("\"schema\".\"table\""),
            Some("table".to_string())
        );
    }

    #[test]
    fn test_extract_last_identifier_empty() {
        assert_eq!(extract_last_identifier(""), None);
        assert_eq!(extract_last_identifier("   "), None);
        // Note: "SELECT " extracts "SELECT" because the function doesn't distinguish keywords
        assert_eq!(
            extract_last_identifier("SELECT "),
            Some("SELECT".to_string())
        );
        // Only punctuation/operators return None
        assert_eq!(extract_last_identifier("("), None);
        assert_eq!(extract_last_identifier(", "), None);
    }

    #[test]
    fn test_extract_qualifier_with_trailing_dot() {
        assert_eq!(extract_qualifier("users.", 6), Some("users".to_string()));
        assert_eq!(extract_qualifier("SELECT u.", 9), Some("u".to_string()));
        assert_eq!(
            extract_qualifier("schema.table.", 13),
            Some("table".to_string())
        );
    }

    #[test]
    fn test_extract_qualifier_mid_token() {
        assert_eq!(
            extract_qualifier("users.name", 10),
            Some("users".to_string())
        );
        assert_eq!(extract_qualifier("SELECT u.id", 11), Some("u".to_string()));
    }

    #[test]
    fn test_extract_qualifier_no_qualifier() {
        assert_eq!(extract_qualifier("SELECT", 6), None);
        assert_eq!(extract_qualifier("users", 5), None);
        assert_eq!(extract_qualifier("", 0), None);
    }

    #[test]
    fn test_extract_qualifier_cursor_at_start() {
        assert_eq!(extract_qualifier("users.name", 0), None);
    }

    #[test]
    fn test_extract_qualifier_cursor_out_of_bounds() {
        assert_eq!(extract_qualifier("users", 100), None);
    }

    #[test]
    fn test_extract_qualifier_utf8_boundary() {
        // Multi-byte UTF-8 character (emoji is 4 bytes)
        let sql = "SELECT 🎉.";
        // Cursor in middle of emoji (invalid boundary) should return None
        assert_eq!(extract_qualifier(sql, 8), None); // Middle of emoji
                                                     // Cursor after emoji + dot should work
        assert_eq!(extract_qualifier(sql, sql.len()), None); // 🎉 is not identifier char
    }

    #[test]
    fn test_extract_qualifier_quoted_identifier() {
        assert_eq!(
            extract_qualifier("\"My Schema\".", 12),
            Some("My Schema".to_string())
        );
    }

    // Unit tests for resolve_qualifier

    #[test]
    fn test_resolve_qualifier_alias_match() {
        let tables = vec![CompletionTable {
            name: "users".to_string(),
            canonical: "public.users".to_string(),
            alias: Some("u".to_string()),
            matched_schema: true,
        }];
        let (registry, _) = SchemaRegistry::new(None, Dialect::Duckdb);

        let result = resolve_qualifier("u", &tables, None, &registry);
        assert!(result.is_some());
        let resolution = result.unwrap();
        assert_eq!(resolution.target, QualifierTarget::ColumnLabel);
        assert_eq!(resolution.label, Some("u".to_string()));
    }

    #[test]
    fn test_resolve_qualifier_table_name_match() {
        // When table is in tables_in_scope (without alias), qualifier matches table name
        // Note: Schema metadata is required for table name matching (vs just alias matching)
        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![],
            }],
        };
        let tables = vec![CompletionTable {
            name: "users".to_string(),
            canonical: "public.users".to_string(),
            alias: None,
            matched_schema: true,
        }];
        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);

        let result = resolve_qualifier("users", &tables, Some(&schema), &registry);
        assert!(
            result.is_some(),
            "Should match table name in tables_in_scope"
        );
        let resolution = result.unwrap();
        assert_eq!(resolution.target, QualifierTarget::ColumnLabel);
        // When no alias, label is the table name itself
        assert_eq!(resolution.label, Some("users".to_string()));
    }

    #[test]
    fn test_resolve_qualifier_schema_only() {
        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("myschema".to_string()),
                name: "mytable".to_string(),
                columns: vec![],
            }],
        };
        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);

        let result = resolve_qualifier("myschema", &[], Some(&schema), &registry);
        assert!(result.is_some());
        let resolution = result.unwrap();
        assert_eq!(resolution.target, QualifierTarget::SchemaOnly);
        assert_eq!(resolution.schema, Some("myschema".to_string()));
    }

    #[test]
    fn test_resolve_qualifier_schema_table() {
        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![ColumnSchema {
                    name: "id".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                }],
            }],
        };
        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);

        // When qualifier matches a table name in schema (but not in tables_in_scope)
        let result = resolve_qualifier("users", &[], Some(&schema), &registry);
        assert!(result.is_some());
        let resolution = result.unwrap();
        assert_eq!(resolution.target, QualifierTarget::SchemaTable);
        assert_eq!(resolution.table, Some("users".to_string()));
    }

    #[test]
    fn test_resolve_qualifier_no_match() {
        let (registry, _) = SchemaRegistry::new(None, Dialect::Duckdb);
        let result = resolve_qualifier("nonexistent", &[], None, &registry);
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_qualifier_case_insensitive() {
        let tables = vec![CompletionTable {
            name: "Users".to_string(),
            canonical: "public.users".to_string(),
            alias: Some("U".to_string()),
            matched_schema: true,
        }];
        let (registry, _) = SchemaRegistry::new(None, Dialect::Duckdb);

        // Should match case-insensitively
        let result = resolve_qualifier("u", &tables, None, &registry);
        assert!(result.is_some());
        assert_eq!(result.unwrap().target, QualifierTarget::ColumnLabel);
    }

    // Test for column_name_from_label

    #[test]
    fn test_column_name_from_label() {
        assert_eq!(column_name_from_label("name"), "name");
        assert_eq!(column_name_from_label("users.name"), "name");
        assert_eq!(column_name_from_label("public.users.name"), "name");
    }

    // Tests for hybrid AST-based completion enrichment

    #[test]
    fn test_cte_column_completion() {
        // Test that CTE columns appear in completion
        let sql = "WITH cte AS (SELECT id, name FROM users) SELECT cte. FROM cte";
        let cursor_offset = sql.find("cte.").unwrap() + 4; // Position after "cte."

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Generic,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);
        assert!(result.should_show, "Should show completions after 'cte.'");

        // Check that CTE columns are in the completion items
        let column_names: Vec<&str> = result
            .items
            .iter()
            .filter(|item| item.category == CompletionItemCategory::Column)
            .map(|item| item.label.as_str())
            .collect();

        assert!(
            column_names.contains(&"id"),
            "Should have 'id' column from CTE. Columns found: {:?}",
            column_names
        );
        assert!(
            column_names.contains(&"name"),
            "Should have 'name' column from CTE. Columns found: {:?}",
            column_names
        );
    }

    #[test]
    fn test_cte_with_declared_columns() {
        // Test CTE with explicit column declaration: WITH cte(a, b) AS (...)
        let sql = "WITH cte(x, y) AS (SELECT id, name FROM users) SELECT cte. FROM cte";
        let cursor_offset = sql.find("cte.").unwrap() + 4;

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Generic,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);
        assert!(result.should_show);

        let column_names: Vec<&str> = result
            .items
            .iter()
            .filter(|item| item.category == CompletionItemCategory::Column)
            .map(|item| item.label.as_str())
            .collect();

        // Should use declared names (x, y) not projected names (id, name)
        assert!(
            column_names.contains(&"x"),
            "Should have declared column 'x'. Columns found: {:?}",
            column_names
        );
        assert!(
            column_names.contains(&"y"),
            "Should have declared column 'y'. Columns found: {:?}",
            column_names
        );
    }

    #[test]
    fn test_subquery_alias_column_completion() {
        // Test that subquery alias columns appear in completion
        // Note: The cursor must be AFTER the FROM clause for AST parsing to include the subquery
        let sql = "SELECT * FROM (SELECT a, b FROM t) AS sub WHERE sub.";
        let cursor_offset = sql.len(); // Position at the end after "sub."

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Generic,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);
        assert!(result.should_show, "Should show completions after 'sub.'");

        let column_names: Vec<&str> = result
            .items
            .iter()
            .filter(|item| item.category == CompletionItemCategory::Column)
            .map(|item| item.label.as_str())
            .collect();

        assert!(
            column_names.contains(&"a"),
            "Should have 'a' column from subquery. Columns found: {:?}",
            column_names
        );
        assert!(
            column_names.contains(&"b"),
            "Should have 'b' column from subquery. Columns found: {:?}",
            column_names
        );
    }

    #[test]
    fn test_recursive_cte_column_completion() {
        // Test that recursive CTE base case columns appear in completion
        let sql = r#"
            WITH RECURSIVE cte AS (
                SELECT 1 AS n
                UNION ALL
                SELECT n + 1 FROM cte WHERE n < 10
            )
            SELECT cte. FROM cte
        "#;
        let cursor_offset = sql.find("cte.").unwrap() + 4;

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Generic,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);
        assert!(result.should_show);

        let column_names: Vec<&str> = result
            .items
            .iter()
            .filter(|item| item.category == CompletionItemCategory::Column)
            .map(|item| item.label.as_str())
            .collect();

        assert!(
            column_names.contains(&"n"),
            "Should have 'n' column from recursive CTE base case. Columns found: {:?}",
            column_names
        );
    }

    #[test]
    fn test_multiple_ctes_column_completion() {
        // Test completion with multiple CTEs
        let sql = r#"
            WITH
                users_cte AS (SELECT id, name FROM users),
                orders_cte AS (SELECT order_id, user_id FROM orders)
            SELECT users_cte. FROM users_cte, orders_cte
        "#;
        let cursor_offset = sql.find("users_cte.").unwrap() + 10;

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Generic,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);
        assert!(result.should_show);

        let column_names: Vec<&str> = result
            .items
            .iter()
            .filter(|item| item.category == CompletionItemCategory::Column)
            .map(|item| item.label.as_str())
            .collect();

        // Should have columns from users_cte (the qualified table)
        assert!(
            column_names.contains(&"id"),
            "Should have 'id' column from users_cte. Columns found: {:?}",
            column_names
        );
        assert!(
            column_names.contains(&"name"),
            "Should have 'name' column from users_cte. Columns found: {:?}",
            column_names
        );
    }

    #[test]
    fn test_type_context_inference() {
        // Direct test of type context inference
        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "age".to_string(),
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
            }],
        };

        let sql = "SELECT * FROM users WHERE age > ";
        let cursor_offset = sql.len();

        // Tokenize
        let tokens = tokenize_sql(sql, Dialect::Duckdb).expect("tokenization should succeed");

        // Create registry and completion context
        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);

        // Get completion context to have tables with canonical names
        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: Some(schema.clone()),
        };
        let ctx = completion_context(&request);

        // Test type context inference
        let type_ctx =
            infer_type_context(&tokens, cursor_offset, sql, &registry, &ctx.tables_in_scope);

        assert!(
            type_ctx.is_some(),
            "Should infer type context from 'age > '. Tables in scope: {:?}",
            ctx.tables_in_scope
                .iter()
                .map(|t| format!("{}(canonical:{})", t.name, t.canonical))
                .collect::<Vec<_>>()
        );

        let type_ctx = type_ctx.unwrap();
        assert_eq!(
            type_ctx.expected_type,
            CanonicalType::Integer,
            "Expected type should be Integer for 'age' column"
        );
    }

    #[test]
    fn test_type_aware_column_completion_in_where() {
        // Test that type-compatible columns score higher in comparison contexts
        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "age".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                    ColumnSchema {
                        name: "created_at".to_string(),
                        data_type: Some("timestamp".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                    ColumnSchema {
                        name: "name".to_string(),
                        data_type: Some("varchar".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                    ColumnSchema {
                        name: "score".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                    },
                ],
            }],
        };

        // Cursor after "age > " - should boost integer-compatible columns
        let sql = "SELECT * FROM users WHERE age > ";
        let cursor_offset = sql.len();

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: Some(schema),
        };

        let result = completion_items(&request);
        assert!(result.should_show);

        // Find column completions
        let columns: Vec<_> = result
            .items
            .iter()
            .filter(|item| item.category == CompletionItemCategory::Column)
            .collect();

        // age and score (both integers) should score higher than name (varchar)
        let age_item = columns.iter().find(|c| c.label == "age");
        let score_item = columns.iter().find(|c| c.label == "score");
        let name_item = columns.iter().find(|c| c.label == "name");

        assert!(age_item.is_some(), "age column should be in completions");
        assert!(
            score_item.is_some(),
            "score column should be in completions"
        );
        assert!(name_item.is_some(), "name column should be in completions");

        // Integer columns should score higher than varchar in "age > " context
        let age_score = age_item.unwrap().score;
        let score_score = score_item.unwrap().score;
        let name_score = name_item.unwrap().score;

        assert!(
            age_score > name_score,
            "Integer column 'age' (score: {}) should rank higher than varchar 'name' (score: {}) in integer comparison context",
            age_score,
            name_score
        );
        assert!(
            score_score > name_score,
            "Integer column 'score' (score: {}) should rank higher than varchar 'name' (score: {}) in integer comparison context",
            score_score,
            name_score
        );
    }

    #[test]
    fn test_type_context_with_parentheses() {
        // Test that parentheses around identifier are handled: WHERE (age) > |
        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![ColumnSchema {
                    name: "age".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                }],
            }],
        };

        let sql = "SELECT * FROM users WHERE (age) > ";
        let cursor_offset = sql.len();

        let tokens = tokenize_sql(sql, Dialect::Duckdb).expect("tokenization should succeed");
        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);
        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: Some(schema),
        };
        let ctx = completion_context(&request);

        let type_ctx =
            infer_type_context(&tokens, cursor_offset, sql, &registry, &ctx.tables_in_scope);

        assert!(
            type_ctx.is_some(),
            "Should infer type context from '(age) > '"
        );
        assert_eq!(type_ctx.unwrap().expected_type, CanonicalType::Integer);
    }

    #[test]
    fn test_type_context_with_nested_parentheses() {
        // Test nested parens: WHERE ((age)) > |
        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![ColumnSchema {
                    name: "age".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                }],
            }],
        };

        let sql = "SELECT * FROM users WHERE ((age)) > ";
        let cursor_offset = sql.len();

        let tokens = tokenize_sql(sql, Dialect::Duckdb).expect("tokenization should succeed");
        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);
        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: Some(schema),
        };
        let ctx = completion_context(&request);

        let type_ctx =
            infer_type_context(&tokens, cursor_offset, sql, &registry, &ctx.tables_in_scope);

        assert!(
            type_ctx.is_some(),
            "Should infer type context from '((age)) > '"
        );
        assert_eq!(type_ctx.unwrap().expected_type, CanonicalType::Integer);
    }

    #[test]
    fn test_type_context_after_and_returns_none() {
        // After AND/OR, we're in a new expression - should return None
        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![ColumnSchema {
                    name: "age".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                }],
            }],
        };

        let sql = "SELECT * FROM users WHERE age > 10 AND ";
        let cursor_offset = sql.len();

        let tokens = tokenize_sql(sql, Dialect::Duckdb).expect("tokenization should succeed");
        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);
        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: Some(schema),
        };
        let ctx = completion_context(&request);

        let type_ctx =
            infer_type_context(&tokens, cursor_offset, sql, &registry, &ctx.tables_in_scope);

        assert!(
            type_ctx.is_none(),
            "Should return None after AND (new expression context)"
        );
    }

    #[test]
    fn test_type_context_after_or_returns_none() {
        // After OR, we're in a new expression - should return None
        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![ColumnSchema {
                    name: "age".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                }],
            }],
        };

        let sql = "SELECT * FROM users WHERE age > 10 OR ";
        let cursor_offset = sql.len();

        let tokens = tokenize_sql(sql, Dialect::Duckdb).expect("tokenization should succeed");
        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Duckdb);
        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: Some(schema),
        };
        let ctx = completion_context(&request);

        let type_ctx =
            infer_type_context(&tokens, cursor_offset, sql, &registry, &ctx.tables_in_scope);

        assert!(
            type_ctx.is_none(),
            "Should return None after OR (new expression context)"
        );
    }

    // Lateral column alias completion tests

    #[test]
    fn test_lateral_alias_completion_duckdb() {
        let sql = "SELECT price * qty AS total, ";
        let cursor_offset = sql.len();

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);

        // 'total' should be available as a lateral alias
        let total_item = result
            .items
            .iter()
            .find(|i| i.label == "total" && i.detail == Some("lateral alias".to_string()));
        assert!(
            total_item.is_some(),
            "Lateral alias 'total' should be in completions for DuckDB"
        );
    }

    #[test]
    fn test_lateral_alias_not_available_postgres() {
        let sql = "SELECT price * qty AS total, ";
        let cursor_offset = sql.len();

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Postgres,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);

        // 'total' should NOT be available as a lateral alias in PostgreSQL
        let total_item = result
            .items
            .iter()
            .find(|i| i.label == "total" && i.detail == Some("lateral alias".to_string()));
        assert!(
            total_item.is_none(),
            "Lateral alias should not appear for PostgreSQL"
        );
    }

    #[test]
    fn test_lateral_alias_position_aware() {
        // Cursor is within the SELECT but before the alias definition ends
        let sql = "SELECT a + b AS total FROM t";
        let cursor_offset = 9; // After "SELECT a "

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);

        // 'total' should NOT be available - cursor is before alias definition
        let total_item = result
            .items
            .iter()
            .find(|i| i.label == "total" && i.detail == Some("lateral alias".to_string()));
        assert!(
            total_item.is_none(),
            "Alias defined after cursor should not appear"
        );
    }

    #[test]
    fn test_multiple_lateral_aliases() {
        let sql = "SELECT a AS x, b AS y, ";
        let cursor_offset = sql.len();

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);

        // Both 'x' and 'y' should be available
        let x_item = result
            .items
            .iter()
            .find(|i| i.label == "x" && i.detail == Some("lateral alias".to_string()));
        let y_item = result
            .items
            .iter()
            .find(|i| i.label == "y" && i.detail == Some("lateral alias".to_string()));
        assert!(
            x_item.is_some(),
            "Lateral alias 'x' should be in completions"
        );
        assert!(
            y_item.is_some(),
            "Lateral alias 'y' should be in completions"
        );
    }

    #[test]
    fn test_lateral_alias_quoted() {
        let sql = r#"SELECT a AS "My Total", "#;
        let cursor_offset = sql.len();

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);

        // Quoted alias should be available
        let alias_item = result
            .items
            .iter()
            .find(|i| i.label == "My Total" && i.detail == Some("lateral alias".to_string()));
        assert!(
            alias_item.is_some(),
            "Quoted lateral alias should be in completions"
        );
    }

    #[test]
    fn test_lateral_alias_bigquery_dialect() {
        // BigQuery also supports lateral aliases
        let sql = "SELECT price AS p, p * 0.1 AS ";
        let cursor_offset = sql.len();

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Bigquery,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);

        // 'p' should be available as a lateral alias
        let p_item = result
            .items
            .iter()
            .find(|i| i.label == "p" && i.detail == Some("lateral alias".to_string()));
        assert!(
            p_item.is_some(),
            "Lateral alias 'p' should be in completions for BigQuery"
        );
    }

    #[test]
    fn test_lateral_alias_snowflake_dialect() {
        // Snowflake also supports lateral aliases
        let sql = "SELECT amount AS amt, ";
        let cursor_offset = sql.len();

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Snowflake,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);

        // 'amt' should be available as a lateral alias
        let amt_item = result
            .items
            .iter()
            .find(|i| i.label == "amt" && i.detail == Some("lateral alias".to_string()));
        assert!(
            amt_item.is_some(),
            "Lateral alias 'amt' should be in completions for Snowflake"
        );
    }

    #[test]
    fn test_lateral_alias_not_in_from_clause() {
        // Lateral aliases should not appear when cursor is in FROM clause
        let sql = "SELECT a AS x FROM ";
        let cursor_offset = sql.len();

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: None,
        };

        let result = completion_items(&request);

        // 'x' should NOT be available in FROM clause context
        let x_item = result
            .items
            .iter()
            .find(|i| i.label == "x" && i.detail == Some("lateral alias".to_string()));
        assert!(
            x_item.is_none(),
            "Lateral alias should not appear in FROM clause"
        );
    }

    #[test]
    fn test_lateral_alias_not_with_qualifier() {
        // Lateral aliases should not appear when there's a table qualifier (e.g., "t.")
        let sql = "SELECT a AS x, t.";
        let cursor_offset = sql.len();

        let schema = SchemaMetadata {
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
            tables: vec![SchemaTable {
                catalog: None,
                schema: None,
                name: "t".to_string(),
                columns: vec![ColumnSchema {
                    name: "col1".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                }],
            }],
        };

        let request = CompletionRequest {
            sql: sql.to_string(),
            dialect: Dialect::Duckdb,
            cursor_offset,
            schema: Some(schema),
        };

        let result = completion_items(&request);

        // When there's a qualifier, we should only show columns from that table
        // Lateral aliases should not appear (they don't have a table qualifier)
        let x_item = result
            .items
            .iter()
            .find(|i| i.label == "x" && i.detail == Some("lateral alias".to_string()));
        assert!(
            x_item.is_none(),
            "Lateral alias should not appear when using table qualifier"
        );
    }

    #[test]
    fn test_should_show_for_cursor_utf8_boundary() {
        // Multi-byte UTF-8 character (emoji is 4 bytes)
        let sql = "SELECT 🎉 FROM";
        // Emoji starts at byte 7, cursor at byte 8 is mid-character
        let mid_emoji_offset = 8;

        // Should not panic, should return false for invalid boundary
        assert!(!should_show_for_cursor(sql, mid_emoji_offset, ""));
    }

    #[test]
    fn test_should_show_for_cursor_valid_positions() {
        // Test various valid cursor positions
        let sql = "SELECT . FROM";
        assert!(should_show_for_cursor(sql, 8, "")); // After dot
        assert!(!should_show_for_cursor(sql, 0, "")); // At start (no prev char)
        assert!(should_show_for_cursor(sql, 7, "")); // After space
    }

    #[test]
    fn test_should_show_for_cursor_out_of_bounds() {
        let sql = "SELECT";
        assert!(!should_show_for_cursor(sql, 100, "")); // Way out of bounds
        assert!(!should_show_for_cursor(sql, sql.len() + 1, "")); // Just past end
    }
}
