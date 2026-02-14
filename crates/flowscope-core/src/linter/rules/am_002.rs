//! LINT_AM_002: Bare UNION quantifier.
//!
//! `UNION` should be explicit (`UNION DISTINCT` or `UNION ALL`) to avoid ambiguous implicit behavior.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::*;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Location, Span, Token, TokenWithSpan, Tokenizer};

pub struct BareUnion;

impl LintRule for BareUnion {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_002
    }

    fn name(&self) -> &'static str {
        "Ambiguous UNION quantifier"
    }

    fn description(&self) -> &'static str {
        "UNION should be explicit about DISTINCT or ALL."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut unions = union_keyword_ranges_for_context(ctx);
        match stmt {
            Statement::Query(query) => check_query(query, &mut unions, ctx, &mut issues),
            Statement::Insert(insert) => {
                if let Some(ref source) = insert.source {
                    check_query(source, &mut unions, ctx, &mut issues);
                }
            }
            Statement::CreateView { query, .. } => {
                check_query(query, &mut unions, ctx, &mut issues)
            }
            Statement::CreateTable(create) => {
                if let Some(ref query) = create.query {
                    check_query(query, &mut unions, ctx, &mut issues);
                }
            }
            _ => {}
        }
        issues
    }
}

fn union_keyword_ranges_for_context(ctx: &LintContext) -> UnionKeywordRanges {
    let tokens = tokenized_for_context(ctx);
    union_keyword_ranges(ctx.statement_sql(), ctx.dialect(), tokens.as_deref())
}

fn check_query(
    query: &Query,
    unions: &mut UnionKeywordRanges,
    ctx: &LintContext,
    issues: &mut Vec<Issue>,
) {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            check_query(&cte.query, unions, ctx, issues);
        }
    }
    check_query_body(&query.body, unions, ctx, issues);
}

fn check_query_body(
    body: &SetExpr,
    unions: &mut UnionKeywordRanges,
    ctx: &LintContext,
    issues: &mut Vec<Issue>,
) {
    match body {
        SetExpr::SetOperation {
            op: SetOperator::Union,
            set_quantifier,
            left,
            right,
        } => {
            check_query_body(left, unions, ctx, issues);
            let union_span = unions.next();

            if matches!(set_quantifier, SetQuantifier::None | SetQuantifier::ByName) {
                let mut issue = Issue::warning(
                    issue_codes::LINT_AM_002,
                    "Use UNION DISTINCT or UNION ALL instead of bare UNION.",
                )
                .with_statement(ctx.statement_index);
                if let Some((start, end)) = union_span {
                    let s = ctx.span_from_statement_offset(start, end);
                    issue = issue.with_span(s);
                }
                issues.push(issue);
            }
            check_query_body(right, unions, ctx, issues);
        }
        SetExpr::SetOperation { left, right, .. } => {
            check_query_body(left, unions, ctx, issues);
            check_query_body(right, unions, ctx, issues);
        }
        SetExpr::Select(_) => {}
        SetExpr::Query(q) => {
            check_query(q, unions, ctx, issues);
        }
        _ => {}
    }
}

struct UnionKeywordRanges {
    ranges: Vec<(usize, usize)>,
    index: usize,
}

impl UnionKeywordRanges {
    fn next(&mut self) -> Option<(usize, usize)> {
        let range = self.ranges.get(self.index).copied();
        if range.is_some() {
            self.index += 1;
        }
        range
    }
}

fn union_keyword_ranges(
    sql: &str,
    dialect: Dialect,
    tokens: Option<&[TokenWithSpan]>,
) -> UnionKeywordRanges {
    let owned_tokens;
    let tokens = if let Some(tokens) = tokens {
        tokens
    } else {
        owned_tokens = match tokenized(sql, dialect) {
            Some(tokens) => tokens,
            None => {
                return UnionKeywordRanges {
                    ranges: Vec::new(),
                    index: 0,
                };
            }
        };
        &owned_tokens
    };

    let ranges = tokens
        .iter()
        .filter_map(|token| {
            let Token::Word(word) = &token.token else {
                return None;
            };
            if word.keyword != Keyword::UNION {
                return None;
            }

            token_offsets(sql, token)
        })
        .collect();

    UnionKeywordRanges { ranges, index: 0 }
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn tokenized_for_context(ctx: &LintContext) -> Option<Vec<TokenWithSpan>> {
    let (statement_start_line, statement_start_column) =
        offset_to_line_col(ctx.sql, ctx.statement_range.start)?;

    ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut out = Vec::new();
        for token in tokens {
            let Some((start, end)) = token_offsets(ctx.sql, token) else {
                continue;
            };
            if start < ctx.statement_range.start || end > ctx.statement_range.end {
                continue;
            }

            let Some(start_loc) = relative_location(
                token.span.start,
                statement_start_line,
                statement_start_column,
            ) else {
                continue;
            };
            let Some(end_loc) =
                relative_location(token.span.end, statement_start_line, statement_start_column)
            else {
                continue;
            };

            out.push(TokenWithSpan::new(
                token.token.clone(),
                Span::new(start_loc, end_loc),
            ));
        }

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    })
}

fn token_offsets(sql: &str, token: &TokenWithSpan) -> Option<(usize, usize)> {
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

fn offset_to_line_col(sql: &str, offset: usize) -> Option<(usize, usize)> {
    if offset > sql.len() {
        return None;
    }
    if offset == sql.len() {
        let mut line = 1usize;
        let mut column = 1usize;
        for ch in sql.chars() {
            if ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        return Some((line, column));
    }

    let mut line = 1usize;
    let mut column = 1usize;
    for (index, ch) in sql.char_indices() {
        if index == offset {
            return Some((line, column));
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    None
}

fn relative_location(
    location: Location,
    statement_start_line: usize,
    statement_start_column: usize,
) -> Option<Location> {
    let line = location.line as usize;
    let column = location.column as usize;
    if line < statement_start_line {
        return None;
    }

    if line == statement_start_line {
        if column < statement_start_column {
            return None;
        }
        return Some(Location::new(
            1,
            (column - statement_start_column + 1) as u64,
        ));
    }

    Some(Location::new(
        (line - statement_start_line + 1) as u64,
        column as u64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = BareUnion;
        let ctx = LintContext {
            sql,
            statement_range: 0..sql.len(),
            statement_index: 0,
        };
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check(stmt, &ctx));
        }
        issues
    }

    #[test]
    fn test_bare_union_detected() {
        let issues = check_sql("SELECT 1 UNION SELECT 2");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_AM_002");
    }

    #[test]
    fn test_union_all_ok() {
        let issues = check_sql("SELECT 1 UNION ALL SELECT 2");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_multiple_bare_unions() {
        let issues = check_sql("SELECT 1 UNION SELECT 2 UNION SELECT 3");
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_mixed_union() {
        let issues = check_sql("SELECT 1 UNION ALL SELECT 2 UNION SELECT 3");
        assert_eq!(issues.len(), 1);
    }

    // --- Edge cases adopted from sqlfluff AM02 ---

    #[test]
    fn test_union_distinct_ok() {
        let issues = check_sql("SELECT a, b FROM t1 UNION DISTINCT SELECT a, b FROM t2");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_bare_union_in_insert() {
        let issues = check_sql("INSERT INTO target SELECT 1 UNION SELECT 2");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_bare_union_in_create_view() {
        let issues = check_sql("CREATE VIEW v AS SELECT 1 UNION SELECT 2");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_bare_union_in_cte() {
        let issues = check_sql("WITH cte AS (SELECT 1 UNION SELECT 2) SELECT * FROM cte");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_union_all_in_cte_ok() {
        let issues = check_sql("WITH cte AS (SELECT 1 UNION ALL SELECT 2) SELECT * FROM cte");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_triple_bare_union() {
        let issues = check_sql("SELECT 1 UNION SELECT 2 UNION SELECT 3");
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_multiple_bare_unions_have_distinct_spans() {
        let issues = check_sql("SELECT 1 UNION SELECT 2 UNION SELECT 3");
        assert_eq!(issues.len(), 2);
        let first_span = issues[0].span.expect("first UNION should have span");
        let second_span = issues[1].span.expect("second UNION should have span");
        assert!(first_span.start < second_span.start);
    }

    #[test]
    fn test_except_and_intersect_ok() {
        let issues = check_sql("SELECT 1 EXCEPT SELECT 2");
        assert!(issues.is_empty());
        let issues = check_sql("SELECT 1 INTERSECT SELECT 2");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_union_identifier_with_underscore_does_not_steal_span() {
        let sql = "SELECT union_col FROM t UNION SELECT 2";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        let span = issues[0].span.expect("UNION issue should include a span");
        let union_pos = sql.find("UNION").expect("query should contain UNION");
        assert_eq!(span.start, union_pos);
    }

    #[test]
    fn test_union_with_comments_keeps_keyword_span() {
        let sql = "WITH cte AS (SELECT 1 /* left */ UNION /* right */ SELECT 2) SELECT * FROM cte";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        let span = issues[0].span.expect("UNION issue should include a span");
        let union_pos = sql.find("UNION").expect("query should contain UNION");
        assert_eq!(span.start, union_pos);
    }
}
