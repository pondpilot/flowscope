//! LINT_AL_001: Table alias style.
//!
//! Require explicit `AS` when aliasing tables.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct AliasingTableStyle;

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

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        implicit_table_alias_spans(ctx.statement_sql())
            .into_iter()
            .map(|(start, end)| {
                Issue::warning(
                    issue_codes::LINT_AL_001,
                    "Use explicit AS when aliasing tables.",
                )
                .with_statement(ctx.statement_index)
                .with_span(ctx.span_from_statement_offset(start, end))
            })
            .collect()
    }
}

fn implicit_table_alias_spans(sql: &str) -> Vec<(usize, usize)> {
    let dialect = sqlparser::dialect::GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return Vec::new();
    };

    let significant: Vec<&TokenWithSpan> = tokens
        .iter()
        .filter(|token| !is_trivia_token(&token.token))
        .collect();

    let mut spans = Vec::new();

    for (index, token) in significant.iter().enumerate() {
        if !is_from_or_join_keyword(&token.token) {
            continue;
        }

        let mut relation_start = index + 1;
        while relation_start < significant.len()
            && is_relation_prefix(&significant[relation_start].token)
        {
            relation_start += 1;
        }
        if relation_start >= significant.len() {
            continue;
        }

        let relation_end = consume_relation(&significant, relation_start);
        if relation_end >= significant.len() {
            continue;
        }

        let alias_token = significant[relation_end];
        if matches!(alias_token.token, Token::Word(ref word) if word.keyword == Keyword::AS) {
            continue;
        }

        if !is_alias_candidate(&alias_token.token) {
            continue;
        }

        let Some(start) = token_start_offset(sql, alias_token) else {
            continue;
        };
        let Some(end) = token_end_offset(sql, alias_token) else {
            continue;
        };
        spans.push((start, end));
    }

    spans
}

fn consume_relation(tokens: &[&TokenWithSpan], start: usize) -> usize {
    if start >= tokens.len() {
        return start;
    }

    match tokens[start].token {
        Token::LParen => {
            let mut depth = 1usize;
            let mut index = start + 1;
            while index < tokens.len() {
                match tokens[index].token {
                    Token::LParen => depth += 1,
                    Token::RParen => {
                        depth -= 1;
                        if depth == 0 {
                            return index + 1;
                        }
                    }
                    _ => {}
                }
                index += 1;
            }
            tokens.len()
        }
        Token::Word(_) => {
            let mut index = start + 1;

            while index + 1 < tokens.len() {
                let dot_then_word = matches!(tokens[index].token, Token::Period)
                    && matches!(tokens[index + 1].token, Token::Word(_));
                if !dot_then_word {
                    break;
                }
                index += 2;
            }

            if index < tokens.len() && matches!(tokens[index].token, Token::LParen) {
                let mut depth = 1usize;
                index += 1;
                while index < tokens.len() {
                    match tokens[index].token {
                        Token::LParen => depth += 1,
                        Token::RParen => {
                            depth -= 1;
                            if depth == 0 {
                                index += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    index += 1;
                }
            }

            index
        }
        _ => start + 1,
    }
}

fn is_from_or_join_keyword(token: &Token) -> bool {
    matches!(token, Token::Word(word) if matches!(word.keyword, Keyword::FROM | Keyword::JOIN))
}

fn is_relation_prefix(token: &Token) -> bool {
    matches!(token, Token::Word(word) if matches!(word.keyword, Keyword::LATERAL | Keyword::ONLY))
}

fn is_alias_candidate(token: &Token) -> bool {
    let Token::Word(word) = token else {
        return false;
    };

    word.quote_style.is_none() && !is_keyword(word.value.as_str())
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

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).expect("parse");
        let rule = AliasingTableStyle;
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
    fn flags_implicit_derived_table_alias() {
        let issues = run("select * from (select 1) d");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_001);
    }
}
