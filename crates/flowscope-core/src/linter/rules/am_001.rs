//! LINT_AM_001: DISTINCT with GROUP BY.
//!
//! Using DISTINCT with GROUP BY is redundant because GROUP BY already
//! collapses duplicate rows. The DISTINCT can be safely removed.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::*;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct DistinctWithGroupBy;

impl BuiltinLintRule for DistinctWithGroupBy {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_001
    }

    fn name(&self) -> &'static str {
        "DISTINCT with GROUP BY"
    }

    fn description(&self) -> &'static str {
        "Ambiguous use of 'DISTINCT' in a 'SELECT' statement with 'GROUP BY'."
    }

    fn check_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        check_statement(stmt, ctx, &mut issues);

        let mut distinct_ranges = distinct_removal_ranges(ctx.statement_sql(), ctx.dialect());
        if distinct_ranges.ranges.len() == issues.len() {
            for issue in &mut issues {
                if let Some((start, end)) = distinct_ranges.next() {
                    let span = ctx.span_from_statement_offset(start, end);
                    *issue = issue.clone().with_span(span).with_autofix_edits(
                        IssueAutofixApplicability::Safe,
                        vec![IssuePatchEdit::new(span, "")],
                    );
                }
            }
        }

        issues
    }
}

fn check_statement(stmt: &Statement, ctx: &RuleContext, issues: &mut Vec<Issue>) {
    match stmt {
        Statement::Query(q) => check_query(q, ctx, issues),
        Statement::Insert(ins) => {
            if let Some(ref source) = ins.source {
                check_query(source, ctx, issues);
            }
        }
        Statement::CreateView(CreateView { query, .. }) => check_query(query, ctx, issues),
        Statement::CreateTable(create) => {
            if let Some(ref q) = create.query {
                check_query(q, ctx, issues);
            }
        }
        _ => {}
    }
}

fn check_query(query: &Query, ctx: &RuleContext, issues: &mut Vec<Issue>) {
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            check_query(&cte.query, ctx, issues);
        }
    }
    check_set_expr(&query.body, ctx, issues);
}

fn check_set_expr(body: &SetExpr, ctx: &RuleContext, issues: &mut Vec<Issue>) {
    match body {
        SetExpr::Select(select) => {
            let has_distinct = matches!(
                select.distinct,
                Some(Distinct::Distinct) | Some(Distinct::On(_))
            );
            let has_group_by = match &select.group_by {
                GroupByExpr::All(_) => true,
                GroupByExpr::Expressions(exprs, _) => !exprs.is_empty(),
            };

            if has_distinct && has_group_by {
                issues.push(
                    Issue::warning(
                        issue_codes::LINT_AM_001,
                        "DISTINCT is redundant when GROUP BY is present.",
                    )
                    .with_statement(ctx.statement_index),
                );
            }

            // Recurse into derived tables (subqueries in FROM)
            for from_item in &select.from {
                check_table_factor(&from_item.relation, ctx, issues);
                for join in &from_item.joins {
                    check_table_factor(&join.relation, ctx, issues);
                }
            }
        }
        SetExpr::Query(q) => check_query(q, ctx, issues),
        SetExpr::SetOperation { left, right, .. } => {
            check_set_expr(left, ctx, issues);
            check_set_expr(right, ctx, issues);
        }
        _ => {}
    }
}

fn check_table_factor(relation: &TableFactor, ctx: &RuleContext, issues: &mut Vec<Issue>) {
    match relation {
        TableFactor::Derived { subquery, .. } => check_query(subquery, ctx, issues),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            check_table_factor(&table_with_joins.relation, ctx, issues);
            for join in &table_with_joins.joins {
                check_table_factor(&join.relation, ctx, issues);
            }
        }
        _ => {}
    }
}

struct DistinctRemovalRanges {
    ranges: Vec<(usize, usize)>,
    index: usize,
}

impl DistinctRemovalRanges {
    fn next(&mut self) -> Option<(usize, usize)> {
        let range = self.ranges.get(self.index).copied();
        if range.is_some() {
            self.index += 1;
        }
        range
    }
}

fn distinct_removal_ranges(sql: &str, dialect: Dialect) -> DistinctRemovalRanges {
    let Some(tokens) = tokenized(sql, dialect) else {
        return DistinctRemovalRanges {
            ranges: Vec::new(),
            index: 0,
        };
    };

    let mut ranges = Vec::new();
    for distinct_index in 0..tokens.len() {
        if !is_distinct_keyword(&tokens[distinct_index].token) {
            continue;
        }

        let phrase_end_index = if let Some(on_index) =
            next_non_trivia_index(&tokens, distinct_index + 1)
        {
            if is_on_keyword(&tokens[on_index].token) {
                let Some(left_paren_index) = next_non_trivia_index(&tokens, on_index + 1) else {
                    continue;
                };
                if !matches!(tokens[left_paren_index].token, Token::LParen) {
                    continue;
                }
                let Some(right_paren_index) = find_matching_rparen(&tokens, left_paren_index)
                else {
                    continue;
                };
                right_paren_index
            } else {
                distinct_index
            }
        } else {
            distinct_index
        };

        let Some((start, _)) = token_with_span_offsets(sql, &tokens[distinct_index]) else {
            continue;
        };
        let end = match next_non_trivia_index(&tokens, phrase_end_index + 1) {
            Some(next_index) => match token_with_span_offsets(sql, &tokens[next_index]) {
                Some((next_start, _)) => next_start,
                None => continue,
            },
            None => match token_with_span_offsets(sql, &tokens[phrase_end_index]) {
                Some((_, phrase_end)) => phrase_end,
                None => continue,
            },
        };

        if start < end {
            ranges.push((start, end));
        }
    }

    DistinctRemovalRanges { ranges, index: 0 }
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn is_distinct_keyword(token: &Token) -> bool {
    matches!(token, Token::Word(word) if word.keyword == Keyword::DISTINCT)
}

fn is_on_keyword(token: &Token) -> bool {
    matches!(token, Token::Word(word) if word.keyword == Keyword::ON)
}

fn next_non_trivia_index(tokens: &[TokenWithSpan], mut index: usize) -> Option<usize> {
    while index < tokens.len() {
        if !is_trivia_token(&tokens[index].token) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn find_matching_rparen(tokens: &[TokenWithSpan], left_paren_index: usize) -> Option<usize> {
    let mut depth = 0usize;

    for (index, token) in tokens.iter().enumerate().skip(left_paren_index) {
        if is_trivia_token(&token.token) {
            continue;
        }

        match token.token {
            Token::LParen => {
                depth += 1;
            }
            Token::RParen => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }

    None
}

fn is_trivia_token(token: &Token) -> bool {
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
    use crate::types::IssueAutofixApplicability;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = DistinctWithGroupBy;
        let ctx = RuleContext::new(sql, 0..sql.len(), 0);
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check_with_context(stmt, &ctx));
        }
        issues
    }

    fn apply_issue_autofix(sql: &str, issue: &Issue) -> Option<String> {
        let autofix = issue.autofix.as_ref()?;
        let mut edits = autofix.edits.clone();
        edits.sort_by(|left, right| right.span.start.cmp(&left.span.start));

        let mut out = sql.to_string();
        for edit in edits {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    #[test]
    fn test_distinct_with_group_by_detected() {
        let issues = check_sql("SELECT DISTINCT col FROM t GROUP BY col");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_AM_001");
    }

    #[test]
    fn test_distinct_without_group_by_ok() {
        let issues = check_sql("SELECT DISTINCT col FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_group_by_without_distinct_ok() {
        let issues = check_sql("SELECT col FROM t GROUP BY col");
        assert!(issues.is_empty());
    }

    // --- Edge cases adopted from sqlfluff AM01 (ambiguous.distinct) ---

    #[test]
    fn test_distinct_group_by_in_subquery() {
        let issues = check_sql("SELECT * FROM (SELECT DISTINCT a FROM t GROUP BY a) AS sub");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_distinct_group_by_in_cte() {
        let issues =
            check_sql("WITH cte AS (SELECT DISTINCT a FROM t GROUP BY a) SELECT * FROM cte");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_distinct_group_by_in_create_view() {
        let issues = check_sql("CREATE VIEW v AS SELECT DISTINCT a FROM t GROUP BY a");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_distinct_group_by_in_insert() {
        let issues = check_sql("INSERT INTO target SELECT DISTINCT a FROM t GROUP BY a");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_no_distinct_no_group_by_ok() {
        let issues = check_sql("SELECT a, b FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_distinct_group_by_in_union_branch() {
        let issues = check_sql("SELECT a FROM t UNION ALL SELECT DISTINCT b FROM t2 GROUP BY b");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_distinct_group_by_emits_safe_autofix_patch() {
        let sql = "SELECT DISTINCT a FROM t GROUP BY a";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);

        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT a FROM t GROUP BY a");
    }
}
