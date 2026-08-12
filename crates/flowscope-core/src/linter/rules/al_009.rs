//! LINT_AL_009: Self alias column.
//!
//! SQLFluff AL09 parity: avoid aliasing a column to its own name.

use crate::generated::NormalizationStrategy;
use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use regex::Regex;
use sqlparser::ast::{Expr, Ident, SelectItem, Statement};
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

use super::semantic_helpers::visit_selects_in_statement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AliasCaseCheck {
    Dialect,
    CaseInsensitive,
    QuotedCsNakedUpper,
    QuotedCsNakedLower,
    CaseSensitive,
}

impl AliasCaseCheck {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_AL_009, "alias_case_check")
            .unwrap_or("dialect")
            .to_ascii_lowercase()
            .as_str()
        {
            "case_insensitive" => Self::CaseInsensitive,
            "quoted_cs_naked_upper" => Self::QuotedCsNakedUpper,
            "quoted_cs_naked_lower" => Self::QuotedCsNakedLower,
            "case_sensitive" => Self::CaseSensitive,
            _ => Self::Dialect,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct NameRef<'a> {
    name: &'a str,
    quoted: bool,
}

pub struct AliasingSelfAliasColumn {
    alias_case_check: AliasCaseCheck,
}

impl AliasingSelfAliasColumn {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            alias_case_check: AliasCaseCheck::from_config(config),
        }
    }
}

impl Default for AliasingSelfAliasColumn {
    fn default() -> Self {
        Self {
            alias_case_check: AliasCaseCheck::Dialect,
        }
    }
}

impl BuiltinLintRule for AliasingSelfAliasColumn {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_009
    }

    fn name(&self) -> &'static str {
        "Self alias column"
    }

    fn description(&self) -> &'static str {
        "Column aliases should not alias to itself, i.e. self-alias."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut violating_aliases = Vec::new();

        // Resolve Dialect mode to the concrete normalization strategy at check time
        // so the matching logic uses the actual dialect rules.
        let strategy = match self.alias_case_check {
            AliasCaseCheck::Dialect => Some(ctx.dialect().normalization_strategy()),
            _ => None,
        };

        visit_selects_in_statement(statement, &mut |select| {
            for item in &select.projection {
                let SelectItem::ExprWithAlias { expr, alias } = item else {
                    continue;
                };

                if aliases_expression_to_itself(expr, alias, self.alias_case_check, strategy) {
                    violating_aliases.push(alias.clone());
                }
            }
        });
        let violation_count = violating_aliases.len();
        let mut autofix_candidates = al009_autofix_candidates_for_context(ctx, &violating_aliases);
        autofix_candidates.sort_by_key(|candidate| candidate.span.start);
        let candidates_align = autofix_candidates.len() == violation_count;
        let legacy_candidates =
            legacy_self_alias_candidates_for_context(ctx, self.alias_case_check, strategy);
        if !legacy_candidates.is_empty()
            && (violation_count == 0
                || !candidates_align
                || contains_assignment_alias_pattern(ctx.statement_sql()))
        {
            return vec![Issue::info(
                issue_codes::LINT_AL_009,
                "Column aliases should not alias to itself.",
            )
            .with_statement(ctx.statement_index)
            .with_span(legacy_candidates[0].span)
            .with_autofix_edits(
                IssueAutofixApplicability::Safe,
                legacy_candidates
                    .into_iter()
                    .flat_map(|candidate| candidate.edits)
                    .collect(),
            )];
        }

        (0..violation_count)
            .map(|index| {
                let mut issue = Issue::info(
                    issue_codes::LINT_AL_009,
                    "Column aliases should not alias to itself.",
                )
                .with_statement(ctx.statement_index);

                if candidates_align {
                    let candidate = &autofix_candidates[index];
                    issue = issue.with_span(candidate.span).with_autofix_edits(
                        IssueAutofixApplicability::Safe,
                        candidate.edits.clone(),
                    );
                }

                issue
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
struct PositionedToken {
    token: Token,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug)]
struct Al009AutofixCandidate {
    span: Span,
    edits: Vec<IssuePatchEdit>,
}

fn al009_autofix_candidates_for_context(
    ctx: &RuleContext,
    aliases: &[Ident],
) -> Vec<Al009AutofixCandidate> {
    if aliases.is_empty() {
        return Vec::new();
    }

    let tokens = statement_positioned_tokens(ctx);
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::new();

    for alias in aliases {
        let Some((alias_start, alias_end)) = ident_span_offsets(ctx.sql, alias) else {
            continue;
        };
        if alias_start < ctx.statement_range.start || alias_end > ctx.statement_range.end {
            continue;
        }

        let Some(alias_token_index) = tokens
            .iter()
            .position(|token| token.start == alias_start && token.end == alias_end)
        else {
            continue;
        };

        let Some(removal_span) = alias_removal_span(&tokens, alias_token_index) else {
            continue;
        };

        candidates.push(Al009AutofixCandidate {
            span: Span::new(alias_start, alias_end),
            edits: vec![IssuePatchEdit::new(removal_span, "")],
        });
    }

    candidates
}

fn statement_positioned_tokens(ctx: &RuleContext) -> Vec<PositionedToken> {
    let from_document_tokens = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut positioned = Vec::new();
        for token in tokens {
            let (start, end) = token_with_span_offsets(ctx.sql, token)?;
            if start < ctx.statement_range.start || end > ctx.statement_range.end {
                continue;
            }

            positioned.push(PositionedToken {
                token: token.token.clone(),
                start,
                end,
            });
        }

        Some(positioned)
    });

    if let Some(tokens) = from_document_tokens {
        return tokens;
    }

    let dialect = ctx.dialect().to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), ctx.statement_sql());
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return Vec::new();
    };

    let mut positioned = Vec::new();
    for token in &tokens {
        let Some((start, end)) = token_with_span_offsets(ctx.statement_sql(), token) else {
            continue;
        };
        positioned.push(PositionedToken {
            token: token.token.clone(),
            start: ctx.statement_range.start + start,
            end: ctx.statement_range.start + end,
        });
    }
    positioned
}

fn alias_removal_span(tokens: &[PositionedToken], alias_token_index: usize) -> Option<Span> {
    let alias = &tokens[alias_token_index];
    let previous_non_trivia = previous_non_trivia_index(tokens, alias_token_index)?;

    if token_is_as_keyword(&tokens[previous_non_trivia].token) {
        let expression_token = previous_non_trivia_index(tokens, previous_non_trivia)?;
        let gap_start = expression_token + 1;
        if gap_start > previous_non_trivia
            || trivia_contains_comment(tokens, gap_start, previous_non_trivia)
            || trivia_contains_comment(tokens, previous_non_trivia + 1, alias_token_index)
        {
            return None;
        }
        return Some(Span::new(tokens[gap_start].start, alias.end));
    }

    let gap_start = previous_non_trivia + 1;
    if gap_start >= alias_token_index
        || trivia_contains_comment(tokens, gap_start, alias_token_index)
    {
        return None;
    }

    Some(Span::new(tokens[gap_start].start, alias.end))
}

fn previous_non_trivia_index(tokens: &[PositionedToken], before: usize) -> Option<usize> {
    if before == 0 {
        return None;
    }

    let mut index = before - 1;
    loop {
        if !is_trivia(&tokens[index].token) {
            return Some(index);
        }
        if index == 0 {
            return None;
        }
        index -= 1;
    }
}

fn trivia_contains_comment(tokens: &[PositionedToken], start: usize, end: usize) -> bool {
    if start >= end {
        return false;
    }

    tokens[start..end].iter().any(|token| {
        matches!(
            token.token,
            Token::Whitespace(
                Whitespace::SingleLineComment { .. } | Whitespace::MultiLineComment(_)
            )
        )
    })
}

fn token_is_as_keyword(token: &Token) -> bool {
    matches!(token, Token::Word(word) if word.value.eq_ignore_ascii_case("AS"))
}

fn is_trivia(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(
            Whitespace::Space
                | Whitespace::Newline
                | Whitespace::Tab
                | Whitespace::SingleLineComment { .. }
                | Whitespace::MultiLineComment(_)
        )
    )
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

fn ident_span_offsets(sql: &str, ident: &Ident) -> Option<(usize, usize)> {
    let start = line_col_to_offset(
        sql,
        ident.span.start.line as usize,
        ident.span.start.column as usize,
    )?;
    let end = line_col_to_offset(
        sql,
        ident.span.end.line as usize,
        ident.span.end.column as usize,
    )?;
    Some((start, end))
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

fn aliases_expression_to_itself(
    expr: &Expr,
    alias: &Ident,
    alias_case_check: AliasCaseCheck,
    dialect_strategy: Option<NormalizationStrategy>,
) -> bool {
    let Some(source_name) = expression_name(expr) else {
        return false;
    };

    let alias_name = NameRef {
        name: alias.value.as_str(),
        quoted: alias.quote_style.is_some(),
    };

    names_match(source_name, alias_name, alias_case_check, dialect_strategy)
}

fn expression_name(expr: &Expr) -> Option<NameRef<'_>> {
    match expr {
        Expr::Identifier(identifier) => Some(NameRef {
            name: identifier.value.as_str(),
            quoted: identifier.quote_style.is_some(),
        }),
        Expr::CompoundIdentifier(parts) => parts.last().map(|part| NameRef {
            name: part.value.as_str(),
            quoted: part.quote_style.is_some(),
        }),
        Expr::Nested(inner) => expression_name(inner),
        _ => None,
    }
}

fn names_match(
    left: NameRef<'_>,
    right: NameRef<'_>,
    alias_case_check: AliasCaseCheck,
    dialect_strategy: Option<NormalizationStrategy>,
) -> bool {
    match alias_case_check {
        AliasCaseCheck::CaseInsensitive => left.name.eq_ignore_ascii_case(right.name),
        AliasCaseCheck::CaseSensitive => left.name == right.name,
        AliasCaseCheck::Dialect => {
            let strategy = dialect_strategy.unwrap_or(NormalizationStrategy::CaseInsensitive);

            // When quoting differs between the column and alias, the user
            // deliberately chose different quoting styles. This signals
            // intent rather than a redundant self-alias, so never flag it.
            if left.quoted != right.quoted {
                return false;
            }

            if left.quoted {
                // Both quoted — exact match required.
                left.name == right.name
            } else {
                // Both unquoted — compare using the dialect's folding strategy.
                match strategy {
                    NormalizationStrategy::CaseSensitive => left.name == right.name,
                    NormalizationStrategy::CaseInsensitive
                    | NormalizationStrategy::Lowercase
                    | NormalizationStrategy::Uppercase => {
                        left.name.eq_ignore_ascii_case(right.name)
                    }
                }
            }
        }
        AliasCaseCheck::QuotedCsNakedUpper | AliasCaseCheck::QuotedCsNakedLower => {
            normalize_name_for_mode(left, alias_case_check)
                == normalize_name_for_mode(right, alias_case_check)
        }
    }
}

fn normalize_name_for_mode(name_ref: NameRef<'_>, mode: AliasCaseCheck) -> String {
    match mode {
        AliasCaseCheck::QuotedCsNakedUpper => {
            if name_ref.quoted {
                name_ref.name.to_string()
            } else {
                name_ref.name.to_ascii_uppercase()
            }
        }
        AliasCaseCheck::QuotedCsNakedLower => {
            if name_ref.quoted {
                name_ref.name.to_string()
            } else {
                name_ref.name.to_ascii_lowercase()
            }
        }
        _ => name_ref.name.to_string(),
    }
}

fn legacy_self_alias_candidates_for_context(
    ctx: &RuleContext,
    alias_case_check: AliasCaseCheck,
    dialect_strategy: Option<NormalizationStrategy>,
) -> Vec<Al009AutofixCandidate> {
    let sql = ctx.statement_sql();
    let Ok(select_clause_regex) = Regex::new(r"(?is)\bselect\b(?P<clause>.*?)\bfrom\b") else {
        return Vec::new();
    };
    let Some(captures) = select_clause_regex.captures(sql) else {
        return Vec::new();
    };
    let Some(clause) = captures.name("clause") else {
        return Vec::new();
    };

    let clause_start = clause.start();
    let clause_sql = clause.as_str();
    let mut line_offset = 0usize;
    let mut candidates = Vec::new();

    for line in clause_sql.split_inclusive('\n') {
        let line_no_newline = line.strip_suffix('\n').unwrap_or(line);
        let mut content_start = 0usize;
        while content_start < line_no_newline.len()
            && line_no_newline.as_bytes()[content_start].is_ascii_whitespace()
        {
            content_start += 1;
        }

        let mut content_end = line_no_newline.len();
        while content_end > content_start
            && line_no_newline.as_bytes()[content_end - 1].is_ascii_whitespace()
        {
            content_end -= 1;
        }
        if content_end > content_start && line_no_newline.as_bytes()[content_end - 1] == b',' {
            content_end -= 1;
        }
        while content_end > content_start
            && line_no_newline.as_bytes()[content_end - 1].is_ascii_whitespace()
        {
            content_end -= 1;
        }
        if content_end <= content_start {
            line_offset += line.len();
            continue;
        }

        let content = &line_no_newline[content_start..content_end];
        let Some(replacement) = legacy_self_alias_replacement(
            content,
            ctx.dialect(),
            alias_case_check,
            dialect_strategy,
        ) else {
            line_offset += line.len();
            continue;
        };
        if replacement == content {
            line_offset += line.len();
            continue;
        }

        let edit_start = clause_start + line_offset + content_start;
        let edit_end = clause_start + line_offset + content_end;
        let span = ctx.span_from_statement_offset(edit_start, edit_end);
        candidates.push(Al009AutofixCandidate {
            span,
            edits: vec![IssuePatchEdit::new(span, replacement)],
        });

        line_offset += line.len();
    }

    candidates
}

fn legacy_self_alias_replacement(
    target: &str,
    dialect: crate::types::Dialect,
    alias_case_check: AliasCaseCheck,
    dialect_strategy: Option<NormalizationStrategy>,
) -> Option<String> {
    if dialect == crate::types::Dialect::Bigquery
        && target.starts_with('`')
        && target.ends_with('`')
    {
        let inner = &target[1..target.len().saturating_sub(1)];
        if let Some(split_at) = inner.find("``") {
            let left = &inner[..split_at];
            let right = &inner[split_at + 2..];
            if !left.is_empty() && left == right {
                return Some(format!("`{left}`"));
            }
        }
    }

    if let Some(eq_pos) = target.find('=') {
        let prev = eq_pos
            .checked_sub(1)
            .and_then(|idx| target.as_bytes().get(idx).copied());
        let next = target.as_bytes().get(eq_pos + 1).copied();
        if !matches!(prev, Some(b'!') | Some(b'<') | Some(b'>')) && !matches!(next, Some(b'=')) {
            let alias_raw = target[..eq_pos].trim();
            let expr_raw = target[eq_pos + 1..].trim();
            if let (Some(expr_name), Some(alias_name)) = (
                parse_identifier_name(expr_raw),
                parse_identifier_name(alias_raw),
            ) {
                if names_match(expr_name, alias_name, alias_case_check, dialect_strategy) {
                    return Some(expr_raw.to_string());
                }
            }
        }
    }

    let upper = target.to_ascii_uppercase();
    if let Some(as_pos) = upper.find(" AS ") {
        let expr_raw = target[..as_pos].trim();
        let alias_raw = target[as_pos + 4..].trim();
        if let (Some(expr_name), Some(alias_name)) = (
            parse_identifier_name(expr_raw),
            parse_identifier_name(alias_raw),
        ) {
            if names_match(expr_name, alias_name, alias_case_check, dialect_strategy) {
                return Some(expr_raw.to_string());
            }
        }
    }

    let mut parts = target.split_whitespace();
    let first = parts.next()?;
    let second = parts.next()?;
    if parts.next().is_none() {
        if let (Some(expr_name), Some(alias_name)) =
            (parse_identifier_name(first), parse_identifier_name(second))
        {
            if names_match(expr_name, alias_name, alias_case_check, dialect_strategy) {
                return Some(first.to_string());
            }
        }
    }

    None
}

fn parse_identifier_name(raw: &str) -> Option<NameRef<'_>> {
    if raw.len() >= 2 {
        let bytes = raw.as_bytes();
        if (bytes[0] == b'"' && bytes[raw.len() - 1] == b'"')
            || (bytes[0] == b'`' && bytes[raw.len() - 1] == b'`')
            || (bytes[0] == b'[' && bytes[raw.len() - 1] == b']')
        {
            return Some(NameRef {
                name: &raw[1..raw.len() - 1],
                quoted: true,
            });
        }
    }

    let mut chars = raw.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$')) {
        return None;
    }
    Some(NameRef {
        name: raw,
        quoted: false,
    })
}

fn contains_assignment_alias_pattern(sql: &str) -> bool {
    let Ok(pattern) =
        Regex::new(r"(?im)^\s*[A-Za-z_][A-Za-z0-9_$]*\s*=\s*[A-Za-z_][A-Za-z0-9_$]*\s*,?\s*$")
    else {
        return false;
    };
    pattern.is_match(sql)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_sql, parse_sql_with_dialect};
    use crate::types::{Dialect, IssueAutofixApplicability};

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingSelfAliasColumn::default();
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
    }

    fn run_in_dialect(sql: &str, dialect: Dialect) -> Vec<Issue> {
        let statements = parse_sql_with_dialect(sql, dialect).expect("parse");
        let rule = AliasingSelfAliasColumn::default();
        let mut issues = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            issues.extend(rule.check_with_context(
                statement,
                &RuleContext::new(sql, 0..sql.len(), index).with_dialect(dialect),
            ));
        }
        issues
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
    fn flags_plain_self_alias() {
        let issues = run("SELECT a AS a FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_009);
    }

    #[test]
    fn flags_qualified_self_alias() {
        let issues = run("SELECT t.a AS a FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_case_insensitive_self_alias() {
        let issues = run("SELECT a AS A FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_alias_name() {
        let issues = run("SELECT a AS b FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_non_identifier_expression() {
        let issues = run("SELECT a + 1 AS a FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn default_dialect_mode_does_not_flag_quoted_case_mismatch() {
        let issues = run("SELECT \"A\" AS a FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn default_dialect_mode_flags_exact_quoted_match() {
        let issues = run("SELECT \"A\" AS \"A\" FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn alias_case_check_case_sensitive_respects_case() {
        let sql = "SELECT a AS A FROM t";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingSelfAliasColumn::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.self_alias.column".to_string(),
                serde_json::json!({"alias_case_check": "case_sensitive"}),
            )]),
        });
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn alias_case_check_quoted_cs_naked_upper_flags_upper_fold_match() {
        let sql = "SELECT \"FOO\" AS foo FROM t";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingSelfAliasColumn::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.self_alias.column".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_upper"}),
            )]),
        });
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn alias_case_check_quoted_cs_naked_upper_allows_nonmatching_quoted_case() {
        let sql = "SELECT \"foo\" AS foo FROM t";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingSelfAliasColumn::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.self_alias.column".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_upper"}),
            )]),
        });
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn alias_case_check_quoted_cs_naked_lower_flags_lower_fold_match() {
        let sql = "SELECT \"foo\" AS FOO FROM t";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingSelfAliasColumn::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.self_alias.column".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_lower"}),
            )]),
        });
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn alias_case_check_quoted_cs_naked_lower_allows_nonmatching_quoted_case() {
        let sql = "SELECT \"FOO\" AS FOO FROM t";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingSelfAliasColumn::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.self_alias.column".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_lower"}),
            )]),
        });
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }

    #[test]
    fn self_alias_with_as_emits_safe_autofix_patch() {
        let sql = "SELECT a AS a FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected AL009 core autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        let edit = &autofix.edits[0];
        assert_eq!(&sql[edit.span.start..edit.span.end], " AS a");
        assert_eq!(edit.replacement, "");
    }

    #[test]
    fn self_alias_without_as_emits_safe_autofix_patch() {
        let sql = "SELECT a a FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected AL009 core autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        let edit = &autofix.edits[0];
        assert_eq!(&sql[edit.span.start..edit.span.end], " a");
        assert_eq!(edit.replacement, "");
    }

    // --- SQLFluff parity: dialect-aware matching ---

    #[test]
    fn clickhouse_case_sensitive_no_false_positives() {
        // SQLFluff: test_pass_different_case_clickhouse
        // ClickHouse is CaseSensitive — different case means different identifier.
        let sql = "select col_b as Col_B, COL_C as col_c, Col_D as COL_D from foo";
        let issues = run_in_dialect(sql, Dialect::Clickhouse);
        assert!(issues.is_empty());
    }

    #[test]
    fn clickhouse_quoted_case_sensitive_no_false_positives() {
        // SQLFluff: test_pass_different_case_clickhouse (quoted portion)
        let sql = r#"select "col_b" as "Col_B", "COL_C" as "col_c", "Col_D" as "COL_D" from foo"#;
        let issues = run_in_dialect(sql, Dialect::Clickhouse);
        assert!(issues.is_empty());
    }

    #[test]
    fn different_quotes_not_flagged() {
        // SQLFluff: test_pass_different_quotes (ansi dialect)
        // When one side is quoted and the other is not, the identifier
        // semantics differ (quoted preserves case, unquoted folds to upper
        // in ANSI). These should not be flagged as self-aliases.
        let sql = r#"select "col_b" as col_b, COL_C as "COL_C", "Col_D" as Col_D from foo"#;
        let issues = run_in_dialect(sql, Dialect::Ansi);
        assert!(issues.is_empty());
    }

    #[test]
    fn bigquery_backtick_self_alias_detected() {
        // SQLFluff: test_fail_bigquery_quoted_column_no_space_with_as
        let sql = "SELECT `col`as`col` FROM clients as c";
        let issues = run_in_dialect(sql, Dialect::Bigquery);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn tsql_self_alias_assignments_use_legacy_fallback_fix() {
        let sql = "select\n    this_alias_is_fine = col_a,\n    col_b = col_b,\n    COL_C AS COL_C,\n    Col_D = Col_D,\n    col_e col_e,\n    COL_F COL_F,\n    Col_G Col_G\nfrom foo";
        let statements = parse_sql("SELECT 1").expect("synthetic parse");
        let rule = AliasingSelfAliasColumn::default();
        let issues = rule.check_with_context(
            &statements[0],
            &RuleContext::new(sql, 0..sql.len(), 0).with_dialect(Dialect::Mssql),
        );
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "select\n    this_alias_is_fine = col_a,\n    col_b,\n    COL_C,\n    Col_D,\n    col_e,\n    COL_F,\n    Col_G\nfrom foo"
        );
    }

    #[test]
    fn bigquery_adjacent_backtick_self_alias_uses_legacy_fallback_fix() {
        let sql = "SELECT `col``col`\nFROM clients as c";
        let statements = parse_sql("SELECT 1").expect("synthetic parse");
        let rule = AliasingSelfAliasColumn::default();
        let issues = rule.check_with_context(
            &statements[0],
            &RuleContext::new(sql, 0..sql.len(), 0).with_dialect(Dialect::Bigquery),
        );
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT `col`\nFROM clients as c");
    }

    #[test]
    fn tsql_parsed_statement_still_gets_self_alias_autofix() {
        let sql = "select\n    this_alias_is_fine = col_a,\n    col_b = col_b,\n    COL_C AS COL_C,\n    Col_D = Col_D,\n    col_e col_e,\n    COL_F COL_F,\n    Col_G Col_G\nfrom foo";
        let issues = run_in_dialect(sql, Dialect::Mssql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "select\n    this_alias_is_fine = col_a,\n    col_b,\n    COL_C,\n    Col_D,\n    col_e,\n    COL_F,\n    Col_G\nfrom foo"
        );
    }
}
