//! LINT_AL_002: Column alias style.
//!
//! Require explicit `AS` when aliasing SELECT expressions.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

pub struct AliasingColumnStyle;

impl LintRule for AliasingColumnStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_002
    }

    fn name(&self) -> &'static str {
        "Column alias style"
    }

    fn description(&self) -> &'static str {
        "Implicit/explicit aliasing of columns."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let sql = ctx.statement_sql();
        let mut issues = Vec::new();

        for (clause, clause_start) in select_clauses_with_spans(sql) {
            for (item, item_start) in split_top_level_commas_with_offsets(&clause) {
                if item_has_implicit_alias(&item) {
                    let start = clause_start + item_start;
                    let end = start + item.len();
                    issues.push(
                        Issue::info(
                            issue_codes::LINT_AL_002,
                            "Use explicit AS when aliasing columns.",
                        )
                        .with_statement(ctx.statement_index)
                        .with_span(ctx.span_from_statement_offset(start, end)),
                    );
                }
            }
        }

        issues
    }
}

fn select_clauses_with_spans(sql: &str) -> Vec<(String, usize)> {
    let dialect = sqlparser::dialect::GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return Vec::new();
    };

    let mut clauses = Vec::new();
    let mut depth = 0usize;

    for (index, token) in tokens.iter().enumerate() {
        if is_select_keyword(token) {
            let select_depth = depth;
            let Some(clause_start) = token_end_offset(sql, token) else {
                continue;
            };

            let mut local_depth = depth;
            let mut from_start = None;

            for candidate in tokens.iter().skip(index + 1) {
                match candidate.token {
                    Token::LParen => local_depth += 1,
                    Token::RParen => local_depth = local_depth.saturating_sub(1),
                    Token::SemiColon if local_depth == select_depth => break,
                    _ => {}
                }

                if is_from_keyword(candidate) && local_depth == select_depth {
                    from_start = token_start_offset(sql, candidate);
                    break;
                }
            }

            if let Some(from_start) = from_start {
                if from_start >= clause_start {
                    clauses.push((sql[clause_start..from_start].to_string(), clause_start));
                }
            }
        }

        match token.token {
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            _ => {}
        }
    }

    clauses
}

fn is_select_keyword(token: &TokenWithSpan) -> bool {
    matches!(token.token, Token::Word(ref word) if word.keyword == Keyword::SELECT)
}

fn is_from_keyword(token: &TokenWithSpan) -> bool {
    matches!(token.token, Token::Word(ref word) if word.keyword == Keyword::FROM)
}

fn token_start_offset(sql: &str, token: &TokenWithSpan) -> Option<usize> {
    line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )
}

fn token_end_offset(sql: &str, token: &TokenWithSpan) -> Option<usize> {
    line_col_to_offset(
        sql,
        token.span.end.line as usize,
        token.span.end.column as usize,
    )
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

fn split_top_level_commas_with_offsets(input: &str) -> Vec<(String, usize)> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut part_start = 0usize;

    for (idx, ch) in input.char_indices() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(ch);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(ch);
            }
            '(' if !in_single && !in_double => {
                depth += 1;
                current.push(ch);
            }
            ')' if !in_single && !in_double && depth > 0 => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !in_single && !in_double && depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    let trim_offset = current.find(trimmed.as_str()).unwrap_or(0);
                    parts.push((trimmed, part_start + trim_offset));
                }
                current.clear();
                part_start = idx + ch.len_utf8();
            }
            _ => current.push(ch),
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        let trim_offset = current.find(trimmed.as_str()).unwrap_or(0);
        parts.push((trimmed, part_start + trim_offset));
    }

    parts
}

fn item_has_as_alias(item: &str) -> bool {
    let Some((alias_start, alias)) = trailing_identifier(item) else {
        return false;
    };
    if !is_simple_identifier(alias) {
        return false;
    }

    let before_alias = item[..alias_start].trim_end();
    let Some((_, last_word)) = trailing_word(before_alias) else {
        return false;
    };

    last_word.eq_ignore_ascii_case("AS")
}

fn trailing_word(input: &str) -> Option<(usize, &str)> {
    let trimmed = input.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    let bytes = trimmed.as_bytes();
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if end == 0 {
        return None;
    }

    let mut start = end;
    while start > 0 && is_identifier_char(bytes[start - 1]) {
        start -= 1;
    }
    if start == end {
        return None;
    }

    Some((start, &trimmed[start..end]))
}

fn trailing_identifier(input: &str) -> Option<(usize, &str)> {
    let (start, word) = trailing_word(input)?;
    is_simple_identifier(word).then_some((start, word))
}

fn is_simple_identifier(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(is_identifier_char_char)
}

fn is_identifier_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_identifier_char_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_keyword(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "SELECT"
            | "FROM"
            | "WHERE"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "FULL"
            | "OUTER"
            | "INNER"
            | "CROSS"
            | "ON"
            | "USING"
            | "AS"
            | "GROUP"
            | "ORDER"
            | "HAVING"
            | "LIMIT"
            | "OFFSET"
            | "UNION"
            | "ALL"
            | "DISTINCT"
            | "BY"
            | "WHEN"
            | "THEN"
            | "ELSE"
            | "END"
    )
}

fn item_has_implicit_alias(item: &str) -> bool {
    let trimmed = item.trim();
    if trimmed.is_empty() || trimmed == "*" || trimmed.ends_with(".*") || item_has_as_alias(trimmed)
    {
        return false;
    }

    let mut depth = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut split_at: Option<usize> = None;

    for (idx, ch) in trimmed.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '(' if !in_single && !in_double => depth += 1,
            ')' if !in_single && !in_double && depth > 0 => depth -= 1,
            c if c.is_whitespace() && !in_single && !in_double && depth == 0 => {
                split_at = Some(idx)
            }
            _ => {}
        }
    }

    let Some(split_idx) = split_at else {
        return false;
    };

    let expr = trimmed[..split_idx].trim_end();
    let alias = trimmed[split_idx..].trim_start();
    if expr.is_empty() || alias.is_empty() || !is_simple_identifier(alias) || is_keyword(alias) {
        return false;
    }

    let expr_ends_with_operator = [
        '+', '-', '*', '/', '%', '^', '|', '&', '=', '<', '>', ',', '(',
    ]
    .iter()
    .any(|ch| expr.ends_with(*ch));

    !expr_ends_with_operator
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).expect("parse");
        let rule = AliasingColumnStyle;
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

    #[test]
    fn flags_implicit_column_alias() {
        let issues = run("select a + 1 total from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_002);
    }

    #[test]
    fn allows_explicit_column_alias() {
        let issues = run("select a + 1 as total from t");
        assert!(issues.is_empty());
    }
}
