use super::*;

pub(super) fn token_span_to_offsets(sql: &str, span: &sqlparser::tokenizer::Span) -> Option<Span> {
    let start = line_col_to_offset(sql, span.start.line as usize, span.start.column as usize)?;
    let end = line_col_to_offset(sql, span.end.line as usize, span.end.column as usize)?;
    Some(Span::new(start, end))
}

pub(super) fn tokenize_sql(sql: &str, dialect: Dialect) -> Result<Vec<TokenInfo>, String> {
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
pub(super) fn split_statements(tokens: &[TokenInfo], sql_len: usize) -> Vec<StatementInfo> {
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

pub(super) fn find_statement_for_cursor(
    statements: &[StatementInfo],
    cursor_offset: usize,
) -> StatementInfo {
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

pub(super) fn keyword_from_token(token: &Token) -> Option<String> {
    match token {
        Token::Word(word) if word.keyword != Keyword::NoKeyword => Some(word.value.to_uppercase()),
        _ => None,
    }
}

pub(super) fn is_identifier_word(word: &Word) -> bool {
    word.quote_style.is_some() || word.keyword == Keyword::NoKeyword
}

pub(super) fn detect_clause(tokens: &[TokenInfo], cursor_offset: usize) -> CompletionClause {
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
pub(super) fn has_group_by(tokens: &[TokenInfo]) -> bool {
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
pub(super) fn in_over_clause(tokens: &[TokenInfo], cursor_offset: usize) -> bool {
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
pub(super) fn infer_type_context(
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
                if paren_depth == 0 && matches!(word.keyword, Keyword::AND | Keyword::OR) =>
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
pub(super) fn type_compatibility_score(column_type: Option<&str>, expected: &TypeContext) -> i32 {
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

pub(super) fn token_kind(token: &Token) -> CompletionTokenKind {
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

pub(super) fn find_token_at_cursor(
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

#[derive(Debug)]
pub(super) struct SelectScope {
    parent: Option<usize>,
    depth: usize,
    start: usize,
    end: usize,
}

/// Assign each token to its nearest SELECT scope.
///
/// Completion SQL is often incomplete and cannot be parsed into an AST. Tracking
/// parentheses and set operators here keeps scope resolution available for inputs
/// such as `u.` while still separating nested queries and set-operation branches.
pub(super) fn select_scopes(
    tokens: &[TokenInfo],
    statement_end: usize,
) -> (Vec<SelectScope>, Vec<Option<usize>>) {
    let mut scopes: Vec<SelectScope> = Vec::new();
    let mut token_scopes = vec![None; tokens.len()];
    let mut active_scopes: Vec<usize> = Vec::new();
    let mut paren_depth = 0usize;

    for (index, token) in tokens.iter().enumerate() {
        if matches!(token.token, Token::RParen) {
            while active_scopes
                .last()
                .is_some_and(|scope_id| scopes[*scope_id].depth == paren_depth)
            {
                let scope_id = active_scopes.pop().expect("active scope exists");
                scopes[scope_id].end = token.span.start;
            }
        }

        let keyword = keyword_from_token(&token.token);
        let select_star_except = keyword.as_deref() == Some("EXCEPT")
            && index > 0
            && matches!(tokens[index - 1].token, Token::Mul)
            && matches!(
                tokens.get(index + 1).map(|token| &token.token),
                Some(Token::LParen)
            );
        if matches!(keyword.as_deref(), Some("UNION" | "EXCEPT" | "INTERSECT"))
            && !select_star_except
        {
            while active_scopes
                .last()
                .is_some_and(|scope_id| scopes[*scope_id].depth == paren_depth)
            {
                let scope_id = active_scopes.pop().expect("active scope exists");
                scopes[scope_id].end = token.span.start;
            }
        }

        if keyword.as_deref() == Some("SELECT") {
            while active_scopes
                .last()
                .is_some_and(|scope_id| scopes[*scope_id].depth >= paren_depth)
            {
                let scope_id = active_scopes.pop().expect("active scope exists");
                scopes[scope_id].end = token.span.start;
            }

            let scope_id = scopes.len();
            scopes.push(SelectScope {
                parent: active_scopes.last().copied(),
                depth: paren_depth,
                start: token.span.start,
                end: statement_end,
            });
            active_scopes.push(scope_id);
        }

        token_scopes[index] = active_scopes.last().copied();

        match token.token {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            _ => {}
        }
    }

    (scopes, token_scopes)
}

pub(super) fn parse_tables_for_scope(
    tokens: &[TokenInfo],
    token_scopes: &[Option<usize>],
    scope_id: usize,
) -> Vec<(String, Option<String>)> {
    let mut tables = Vec::new();
    let mut in_from_clause = false;
    let mut expecting_table = false;
    let mut index = 0;

    while index < tokens.len() {
        if token_scopes[index] != Some(scope_id) {
            index += 1;
            continue;
        }

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

            let (alias, consumed) = if token_scopes.get(index) == Some(&Some(scope_id)) {
                parse_alias(tokens, index)
            } else {
                (None, index)
            };
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

pub(super) fn table_binding_label<'a>(name: &'a str, alias: &'a Option<String>) -> Option<&'a str> {
    alias
        .as_deref()
        .or_else(|| name.rsplit('.').next().filter(|label| !label.is_empty()))
}

pub(super) fn parse_tables_in_scope(
    tokens: &[TokenInfo],
    cursor_offset: usize,
    statement_end: usize,
    registry: &SchemaRegistry,
) -> Vec<(String, Option<String>)> {
    let (scopes, token_scopes) = select_scopes(tokens, statement_end);
    let Some(active_scope) = scopes
        .iter()
        .enumerate()
        .filter(|(_, scope)| scope.start <= cursor_offset && cursor_offset <= scope.end)
        .max_by_key(|(_, scope)| scope.depth)
        .map(|(scope_id, _)| scope_id)
    else {
        return parse_tables_for_scope(tokens, &vec![Some(0); tokens.len()], 0);
    };

    let mut visible_scopes = Vec::new();
    let mut current_scope = Some(active_scope);
    while let Some(scope_id) = current_scope {
        visible_scopes.push(scope_id);
        current_scope = scopes[scope_id].parent;
    }

    let mut tables = Vec::new();
    let mut shadowed_labels = std::collections::HashSet::new();
    for scope_id in visible_scopes {
        let scope_tables = parse_tables_for_scope(tokens, &token_scopes, scope_id);
        for (name, alias) in &scope_tables {
            let label = table_binding_label(name, alias);
            if label.is_some_and(|label| {
                shadowed_labels.contains(&registry.normalize_identifier(label))
            }) {
                continue;
            }
            tables.push((name.clone(), alias.clone()));
        }
        shadowed_labels.extend(scope_tables.iter().filter_map(|(name, alias)| {
            table_binding_label(name, alias).map(|label| registry.normalize_identifier(label))
        }));
    }

    tables
}

pub(super) fn parse_tables(tokens: &[TokenInfo]) -> Vec<(String, Option<String>)> {
    parse_tables_for_scope(tokens, &vec![Some(0); tokens.len()], 0)
}

pub(super) fn parse_table_name(tokens: &[TokenInfo], start: usize) -> Option<(String, usize)> {
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

pub(super) fn parse_alias(tokens: &[TokenInfo], start: usize) -> (Option<String>, usize) {
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

pub(super) fn build_columns(
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

pub(super) fn resolve_tables(
    tables_raw: Vec<(String, Option<String>)>,
    registry: &SchemaRegistry,
) -> Vec<CompletionTable> {
    tables_raw
        .into_iter()
        .map(|(name, alias)| {
            if name.is_empty() {
                return CompletionTable {
                    name,
                    canonical: String::new(),
                    alias,
                    matched_schema: false,
                };
            }

            let resolution = registry.canonicalize_table_reference(&name);
            CompletionTable {
                name,
                canonical: resolution.canonical,
                alias,
                matched_schema: resolution.matched_schema,
            }
        })
        .collect()
}

pub(super) fn token_list_for_statement(tokens: &[TokenInfo], span: &Span) -> Vec<TokenInfo> {
    tokens
        .iter()
        .filter(|token| token.span.start >= span.start && token.span.end <= span.end)
        .cloned()
        .collect()
}
