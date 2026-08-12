//! LINT_ST_007: Avoid JOIN ... USING (...).
//!
//! USING can hide which side a column originates from and may create ambiguity
//! in complex joins. Prefer explicit ON conditions.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::{Spanned, *};
use sqlparser::tokenizer::{Span as SqlParserSpan, Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct AvoidUsingJoin;

impl BuiltinLintRule for AvoidUsingJoin {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_007
    }

    fn name(&self) -> &'static str {
        "Avoid USING in JOIN"
    }

    fn description(&self) -> &'static str {
        "Prefer specifying join keys instead of using 'USING'."
    }

    fn check_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        check_statement(stmt, ctx, &mut issues);
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
            for from_item in &select.from {
                let mut left_ref = table_factor_reference_name(&from_item.relation);
                check_table_factor(&from_item.relation, ctx, issues);
                for join in &from_item.joins {
                    let right_ref = table_factor_reference_name(&join.relation);
                    if let Some(issue) = using_join_issue(
                        ctx,
                        &join.join_operator,
                        left_ref.as_deref(),
                        right_ref.as_deref(),
                    ) {
                        issues.push(issue);
                    }
                    check_table_factor(&join.relation, ctx, issues);

                    if join_uses_using(&join.join_operator) {
                        // SQLFluff parity: once a USING join is encountered, the
                        // left side becomes a merged relation and follow-up USING
                        // joins are not safely autofixable.
                        left_ref = None;
                    } else if right_ref.is_some() {
                        left_ref = right_ref;
                    }
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
            let mut left_ref = table_factor_reference_name(&table_with_joins.relation);
            check_table_factor(&table_with_joins.relation, ctx, issues);
            for join in &table_with_joins.joins {
                let right_ref = table_factor_reference_name(&join.relation);
                if let Some(issue) = using_join_issue(
                    ctx,
                    &join.join_operator,
                    left_ref.as_deref(),
                    right_ref.as_deref(),
                ) {
                    issues.push(issue);
                }
                check_table_factor(&join.relation, ctx, issues);

                if join_uses_using(&join.join_operator) {
                    left_ref = None;
                } else if right_ref.is_some() {
                    left_ref = right_ref;
                }
            }
        }
        _ => {}
    }
}

fn using_join_issue(
    ctx: &RuleContext,
    join_operator: &JoinOperator,
    left_ref: Option<&str>,
    right_ref: Option<&str>,
) -> Option<Issue> {
    let constraint = join_constraint(join_operator)?;
    let JoinConstraint::Using(columns) = constraint else {
        return None;
    };

    let mut issue = Issue::warning(
        issue_codes::LINT_ST_007,
        "Avoid JOIN ... USING (...); prefer explicit ON conditions.",
    )
    .with_statement(ctx.statement_index);

    if let Some((span, replacement)) =
        using_join_autofix(ctx, constraint, columns, left_ref, right_ref)
    {
        issue = issue.with_span(span).with_autofix_edits(
            IssueAutofixApplicability::Safe,
            vec![IssuePatchEdit::new(span, replacement)],
        );
    }

    Some(issue)
}

fn join_constraint(op: &JoinOperator) -> Option<&JoinConstraint> {
    match op {
        JoinOperator::Join(constraint)
        | JoinOperator::Inner(constraint)
        | JoinOperator::Left(constraint)
        | JoinOperator::LeftOuter(constraint)
        | JoinOperator::Right(constraint)
        | JoinOperator::RightOuter(constraint)
        | JoinOperator::FullOuter(constraint)
        | JoinOperator::CrossJoin(constraint)
        | JoinOperator::Semi(constraint)
        | JoinOperator::LeftSemi(constraint)
        | JoinOperator::RightSemi(constraint)
        | JoinOperator::Anti(constraint)
        | JoinOperator::LeftAnti(constraint)
        | JoinOperator::RightAnti(constraint)
        | JoinOperator::StraightJoin(constraint) => Some(constraint),
        JoinOperator::AsOf { constraint, .. } => Some(constraint),
        JoinOperator::CrossApply | JoinOperator::OuterApply => None,
    }
}

fn join_uses_using(op: &JoinOperator) -> bool {
    matches!(join_constraint(op), Some(JoinConstraint::Using(_)))
}

#[derive(Clone)]
struct PositionedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn using_join_autofix(
    ctx: &RuleContext,
    constraint: &JoinConstraint,
    columns: &[ObjectName],
    left_ref: Option<&str>,
    right_ref: Option<&str>,
) -> Option<(Span, String)> {
    let left_ref = left_ref?;
    let right_ref = right_ref?;
    let replacement = format!(
        "ON {}",
        using_columns_to_on_expr(columns, left_ref, right_ref)?
    );

    let (constraint_start, constraint_end) = constraint_statement_offsets(ctx, constraint)?;
    let span = locate_using_clause_span(ctx, constraint_start, constraint_end)?;
    if span_contains_comment(ctx, span) {
        return None;
    }

    Some((span, replacement))
}

fn using_columns_to_on_expr(
    columns: &[ObjectName],
    left_ref: &str,
    right_ref: &str,
) -> Option<String> {
    let mut combined: Option<Expr> = None;

    for object_name in columns {
        let column_ident = object_name
            .0
            .last()
            .and_then(|part| part.as_ident())
            .cloned()?;

        let equality = Expr::BinaryOp {
            left: Box::new(Expr::CompoundIdentifier(vec![
                Ident::new(left_ref),
                column_ident.clone(),
            ])),
            op: BinaryOperator::Eq,
            right: Box::new(Expr::CompoundIdentifier(vec![
                Ident::new(right_ref),
                column_ident,
            ])),
        };

        combined = Some(match combined {
            Some(prev) => Expr::BinaryOp {
                left: Box::new(prev),
                op: BinaryOperator::And,
                right: Box::new(equality),
            },
            None => equality,
        });
    }

    Some(combined?.to_string())
}

fn constraint_statement_offsets(
    ctx: &RuleContext,
    constraint: &JoinConstraint,
) -> Option<(usize, usize)> {
    if let Some((start, end)) = sqlparser_span_offsets(ctx.statement_sql(), constraint.span()) {
        return Some((start, end));
    }

    let (start, end) = sqlparser_span_offsets(ctx.sql, constraint.span())?;
    if start < ctx.statement_range.start || end > ctx.statement_range.end {
        return None;
    }
    Some((
        start - ctx.statement_range.start,
        end - ctx.statement_range.start,
    ))
}

fn locate_using_clause_span(
    ctx: &RuleContext,
    constraint_start: usize,
    constraint_end: usize,
) -> Option<Span> {
    let tokens = positioned_statement_tokens(ctx)?;
    if tokens.is_empty() {
        return None;
    }

    let abs_constraint_start = ctx.statement_range.start + constraint_start;
    let abs_constraint_end = ctx.statement_range.start + constraint_end;

    let using_indexes = tokens
        .iter()
        .enumerate()
        .filter_map(|(idx, token)| {
            (token.start <= abs_constraint_start && token_word_equals(&token.token, "USING"))
                .then_some(idx)
        })
        .collect::<Vec<_>>();

    for using_idx in using_indexes.into_iter().rev() {
        let Some(lparen_idx) = next_non_trivia_token_index(&tokens, using_idx + 1) else {
            continue;
        };
        if !matches!(tokens[lparen_idx].token, Token::LParen) {
            continue;
        }

        let mut depth = 0usize;
        let mut close_idx = None;
        for (idx, token) in tokens.iter().enumerate().skip(lparen_idx) {
            match token.token {
                Token::LParen => depth += 1,
                Token::RParen => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    if depth == 0 {
                        close_idx = Some(idx);
                        break;
                    }
                }
                _ => {}
            }
        }

        let Some(close_idx) = close_idx else {
            continue;
        };

        let span = Span::new(tokens[using_idx].start, tokens[close_idx].end);
        if span.start <= abs_constraint_start && span.end >= abs_constraint_end {
            return Some(span);
        }
    }

    None
}

fn next_non_trivia_token_index(tokens: &[PositionedToken], start: usize) -> Option<usize> {
    tokens
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(idx, token)| (!is_trivia(&token.token)).then_some(idx))
}

fn table_factor_reference_name(relation: &TableFactor) -> Option<String> {
    match relation {
        TableFactor::Table { name, alias, .. } => {
            if let Some(alias) = alias {
                Some(alias.name.value.clone())
            } else {
                name.0
                    .last()
                    .and_then(|part| part.as_ident())
                    .map(|ident| ident.value.clone())
            }
        }
        TableFactor::Derived { alias, .. }
        | TableFactor::TableFunction { alias, .. }
        | TableFactor::Function { alias, .. }
        | TableFactor::UNNEST { alias, .. }
        | TableFactor::JsonTable { alias, .. }
        | TableFactor::OpenJsonTable { alias, .. }
        | TableFactor::NestedJoin { alias, .. }
        | TableFactor::Pivot { alias, .. }
        | TableFactor::Unpivot { alias, .. } => alias.as_ref().map(|a| a.name.value.clone()),
        _ => None,
    }
}

fn positioned_statement_tokens(ctx: &RuleContext) -> Option<Vec<PositionedToken>> {
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
        return Some(tokens);
    }

    let tokens = tokenize_with_spans(ctx.statement_sql(), ctx.dialect())?;
    let mut positioned = Vec::new();
    for token in tokens {
        let (start, end) = token_with_span_offsets(ctx.statement_sql(), &token)?;
        positioned.push(PositionedToken {
            token: token.token,
            start: ctx.statement_range.start + start,
            end: ctx.statement_range.start + end,
        });
    }
    Some(positioned)
}

fn span_contains_comment(ctx: &RuleContext, span: Span) -> bool {
    positioned_statement_tokens(ctx).is_some_and(|tokens| {
        tokens.iter().any(|token| {
            token.start >= span.start && token.end <= span.end && is_comment_token(&token.token)
        })
    })
}

fn tokenize_with_spans(sql: &str, dialect: crate::types::Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
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

fn sqlparser_span_offsets(sql: &str, span: SqlParserSpan) -> Option<(usize, usize)> {
    if span.start.line == 0 || span.start.column == 0 || span.end.line == 0 || span.end.column == 0
    {
        return None;
    }

    let start = line_col_to_offset(sql, span.start.line as usize, span.start.column as usize)?;
    let end = line_col_to_offset(sql, span.end.line as usize, span.end.column as usize)?;
    (end >= start).then_some((start, end))
}

fn line_col_to_offset(sql: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }

    let mut current_line = 1usize;
    let mut line_start = 0usize;
    for (idx, ch) in sql.char_indices() {
        if current_line == line {
            break;
        }
        if ch == '\n' {
            current_line += 1;
            line_start = idx + ch.len_utf8();
        }
    }
    if current_line != line {
        return None;
    }

    let mut current_column = 1usize;
    for (rel_idx, ch) in sql[line_start..].char_indices() {
        if current_column == column {
            return Some(line_start + rel_idx);
        }
        if ch == '\n' {
            return None;
        }
        current_column += 1;
    }
    if current_column == column {
        return Some(sql.len());
    }
    None
}

fn token_word_equals(token: &Token, word: &str) -> bool {
    match token {
        Token::Word(w) => w.value.eq_ignore_ascii_case(word),
        _ => false,
    }
}

fn is_trivia(token: &Token) -> bool {
    matches!(token, Token::Whitespace(_))
}

fn is_comment_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::SingleLineComment { .. } | Whitespace::MultiLineComment(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::{IssueAutofixApplicability, IssuePatchEdit};

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = AvoidUsingJoin;
        let ctx = RuleContext::new(sql, 0..sql.len(), 0);
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check_with_context(stmt, &ctx));
        }
        issues
    }

    fn apply_edits(sql: &str, edits: &[IssuePatchEdit]) -> String {
        let mut output = sql.to_string();
        let mut ordered = edits.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|edit| edit.span.start);
        for edit in ordered.into_iter().rev() {
            output.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        output
    }

    #[test]
    fn test_using_join_detected() {
        let sql = "SELECT * FROM a JOIN b USING (id)";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_ST_007");

        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected ST007 core autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(fixed, "SELECT * FROM a JOIN b ON a.id = b.id");
    }

    #[test]
    fn test_on_join_ok() {
        let issues = check_sql("SELECT * FROM a JOIN b ON a.id = b.id");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_using_join_comment_blocks_safe_autofix() {
        let issues = check_sql("SELECT * FROM a JOIN b USING (id /*keep*/)");
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0].autofix.is_none(),
            "comment-bearing USING join should not emit safe autofix metadata"
        );
    }

    #[test]
    fn test_using_join_subquery_alias_autofix() {
        let sql = "select x.a from (SELECT 1 AS a) AS x inner join y using (id)";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0]
            .autofix
            .as_ref()
            .expect("expected ST007 core autofix metadata");
        let fixed = apply_edits(sql, &autofix.edits);
        assert_eq!(
            fixed,
            "select x.a from (SELECT 1 AS a) AS x inner join y ON x.id = y.id"
        );
    }

    #[test]
    fn test_using_chain_stops_after_first_using_fix() {
        let sql = "select x.a\nfrom x\ninner join y using(id, foo)\ninner join z using(id)";
        let issues = check_sql(sql);
        assert_eq!(issues.len(), 2);

        let all_edits: Vec<IssuePatchEdit> = issues
            .iter()
            .filter_map(|issue| issue.autofix.as_ref())
            .flat_map(|autofix| autofix.edits.iter().cloned())
            .collect();
        let fixed = apply_edits(sql, &all_edits);
        assert_eq!(
            fixed,
            "select x.a\nfrom x\ninner join y ON x.id = y.id AND x.foo = y.foo\ninner join z using(id)"
        );
        assert!(
            issues[1].autofix.is_none(),
            "second USING join should remain unfixed for safety"
        );
    }
}
