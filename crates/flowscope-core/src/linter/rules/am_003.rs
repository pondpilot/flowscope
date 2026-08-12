//! LINT_AM_003: Ambiguous ORDER BY direction.
//!
//! SQLFluff AM03 parity: if any ORDER BY item specifies ASC/DESC, all should.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::{
    CreateView, Expr, FunctionArg, FunctionArgExpr, FunctionArguments, OrderByKind, Query, Select,
    SetExpr, Statement, TableFactor, WindowType,
};
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

use super::semantic_helpers::join_on_expr;

pub struct AmbiguousOrderBy;

impl BuiltinLintRule for AmbiguousOrderBy {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_003
    }

    fn name(&self) -> &'static str {
        "Ambiguous ORDER BY"
    }

    fn description(&self) -> &'static str {
        "Ambiguous ordering directions for columns in order by clause."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let mut violation_count = 0usize;
        check_statement(statement, &mut violation_count);
        let clause_autofixes = am003_clause_autofixes(ctx.statement_sql(), ctx.dialect());
        let clauses_align = clause_autofixes.len() == violation_count;

        (0..violation_count)
            .map(|index| {
                let mut issue = Issue::warning(
                    issue_codes::LINT_AM_003,
                    "Ambiguous ORDER BY clause. Specify ASC/DESC for all columns or none.",
                )
                .with_statement(ctx.statement_index);

                if clauses_align {
                    let clause_fix = &clause_autofixes[index];
                    let span =
                        ctx.span_from_statement_offset(clause_fix.span.start, clause_fix.span.end);
                    let edits = clause_fix
                        .edits
                        .iter()
                        .map(|edit| {
                            IssuePatchEdit::new(
                                ctx.span_from_statement_offset(edit.span.start, edit.span.end),
                                edit.replacement.clone(),
                            )
                        })
                        .collect();
                    issue = issue
                        .with_span(span)
                        .with_autofix_edits(IssueAutofixApplicability::Safe, edits);
                }

                issue
            })
            .collect()
    }
}

fn check_statement(statement: &Statement, violations: &mut usize) {
    match statement {
        Statement::Query(query) => check_query(query, violations),
        Statement::Insert(insert) => {
            if let Some(source) = &insert.source {
                check_query(source, violations);
            }
        }
        Statement::CreateView(CreateView { query, .. }) => check_query(query, violations),
        Statement::CreateTable(create) => {
            if let Some(query) = &create.query {
                check_query(query, violations);
            }
        }
        _ => {}
    }
}

fn check_query(query: &Query, violations: &mut usize) {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            check_query(&cte.query, violations);
        }
    }

    check_set_expr(&query.body, violations);

    if order_by_mixes_explicit_and_implicit_direction(query) {
        *violations += 1;
    }
}

fn check_set_expr(set_expr: &SetExpr, violations: &mut usize) {
    match set_expr {
        SetExpr::Select(select) => check_select(select, violations),
        SetExpr::Query(query) => check_query(query, violations),
        SetExpr::SetOperation { left, right, .. } => {
            check_set_expr(left, violations);
            check_set_expr(right, violations);
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => check_statement(statement, violations),
        _ => {}
    }
}

fn check_select(select: &Select, violations: &mut usize) {
    for table in &select.from {
        check_table_factor(&table.relation, violations);
        for join in &table.joins {
            check_table_factor(&join.relation, violations);
            if let Some(on_expr) = join_on_expr(&join.join_operator) {
                check_expr_for_subqueries(on_expr, violations);
            }
        }
    }

    for item in &select.projection {
        if let sqlparser::ast::SelectItem::UnnamedExpr(expr)
        | sqlparser::ast::SelectItem::ExprWithAlias { expr, .. } = item
        {
            check_expr_for_subqueries(expr, violations);
        }
    }

    if let Some(prewhere) = &select.prewhere {
        check_expr_for_subqueries(prewhere, violations);
    }

    if let Some(selection) = &select.selection {
        check_expr_for_subqueries(selection, violations);
    }

    if let sqlparser::ast::GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            check_expr_for_subqueries(expr, violations);
        }
    }

    if let Some(having) = &select.having {
        check_expr_for_subqueries(having, violations);
    }

    if let Some(qualify) = &select.qualify {
        check_expr_for_subqueries(qualify, violations);
    }

    for order_expr in &select.sort_by {
        check_expr_for_subqueries(&order_expr.expr, violations);
    }
}

fn check_table_factor(table_factor: &TableFactor, violations: &mut usize) {
    match table_factor {
        TableFactor::Derived { subquery, .. } => check_query(subquery, violations),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            check_table_factor(&table_with_joins.relation, violations);
            for join in &table_with_joins.joins {
                check_table_factor(&join.relation, violations);
                if let Some(on_expr) = join_on_expr(&join.join_operator) {
                    check_expr_for_subqueries(on_expr, violations);
                }
            }
        }
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => check_table_factor(table, violations),
        _ => {}
    }
}

fn check_expr_for_subqueries(expr: &Expr, violations: &mut usize) {
    match expr {
        Expr::Subquery(query)
        | Expr::Exists {
            subquery: query, ..
        } => check_query(query, violations),
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => {
            check_expr_for_subqueries(inner, violations);
            check_query(subquery, violations);
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::AnyOp { left, right, .. }
        | Expr::AllOp { left, right, .. } => {
            check_expr_for_subqueries(left, violations);
            check_expr_for_subqueries(right, violations);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => check_expr_for_subqueries(inner, violations),
        Expr::InList { expr, list, .. } => {
            check_expr_for_subqueries(expr, violations);
            for item in list {
                check_expr_for_subqueries(item, violations);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            check_expr_for_subqueries(expr, violations);
            check_expr_for_subqueries(low, violations);
            check_expr_for_subqueries(high, violations);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand {
                check_expr_for_subqueries(operand, violations);
            }
            for when in conditions {
                check_expr_for_subqueries(&when.condition, violations);
                check_expr_for_subqueries(&when.result, violations);
            }
            if let Some(otherwise) = else_result {
                check_expr_for_subqueries(otherwise, violations);
            }
        }
        Expr::Function(function) => {
            if let FunctionArguments::List(arguments) = &function.args {
                for arg in &arguments.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                        | FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(expr),
                            ..
                        } => check_expr_for_subqueries(expr, violations),
                        _ => {}
                    }
                }
            }

            if let Some(filter) = &function.filter {
                check_expr_for_subqueries(filter, violations);
            }

            for order_expr in &function.within_group {
                check_expr_for_subqueries(&order_expr.expr, violations);
            }

            if let Some(WindowType::WindowSpec(spec)) = &function.over {
                for expr in &spec.partition_by {
                    check_expr_for_subqueries(expr, violations);
                }
                for order_expr in &spec.order_by {
                    check_expr_for_subqueries(&order_expr.expr, violations);
                }
            }
        }
        _ => {}
    }
}

fn order_by_mixes_explicit_and_implicit_direction(query: &Query) -> bool {
    let Some(order_by) = &query.order_by else {
        return false;
    };

    let OrderByKind::Expressions(order_exprs) = &order_by.kind else {
        return false;
    };

    let mut has_explicit = false;
    let mut has_implicit = false;

    for order_expr in order_exprs {
        if order_expr.options.asc.is_some() {
            has_explicit = true;
        } else {
            has_implicit = true;
        }
    }

    has_explicit && has_implicit
}

#[derive(Clone, Debug)]
struct Am003ClauseAutofix {
    span: Span,
    edits: Vec<IssuePatchEdit>,
}

#[derive(Clone, Debug)]
struct Am003OrderItem {
    has_direction: bool,
    insert_offset: usize,
}

fn am003_clause_autofixes(sql: &str, dialect: Dialect) -> Vec<Am003ClauseAutofix> {
    let Some(tokens) = tokenized(sql, dialect) else {
        return Vec::new();
    };

    let mut fixes = Vec::new();
    let mut index = 0usize;
    while index < tokens.len() {
        let order_index = index;
        if !is_order_keyword(&tokens[order_index].token) {
            index += 1;
            continue;
        }

        let Some(by_index) = next_non_trivia_index(&tokens, order_index + 1) else {
            break;
        };
        if !is_by_keyword(&tokens[by_index].token) {
            index += 1;
            continue;
        }

        let Some(clause_start) = next_non_trivia_index(&tokens, by_index + 1) else {
            break;
        };
        let (items, clause_end, next_index) = collect_order_by_items(sql, &tokens, clause_start);
        index = next_index;

        if items.len() < 2 {
            continue;
        }

        let has_explicit = items.iter().any(|item| item.has_direction);
        let has_implicit = items.iter().any(|item| !item.has_direction);
        if !has_explicit || !has_implicit {
            continue;
        }

        let mut edits = Vec::new();
        for item in items {
            if !item.has_direction {
                edits.push(IssuePatchEdit::new(
                    Span::new(item.insert_offset, item.insert_offset),
                    " ASC",
                ));
            }
        }
        if edits.is_empty() {
            continue;
        }

        let Some((clause_start_offset, _)) = token_with_span_offsets(sql, &tokens[order_index])
        else {
            continue;
        };
        fixes.push(Am003ClauseAutofix {
            span: Span::new(clause_start_offset, clause_end),
            edits,
        });
    }

    fixes
}

fn collect_order_by_items(
    sql: &str,
    tokens: &[TokenWithSpan],
    start_index: usize,
) -> (Vec<Am003OrderItem>, usize, usize) {
    let mut depth = 0usize;
    let mut cursor = start_index;
    let mut item_start = start_index;
    let mut items = Vec::new();

    while cursor < tokens.len() {
        let token = &tokens[cursor].token;
        if is_trivia(token) {
            cursor += 1;
            continue;
        }

        match token {
            Token::LParen => {
                depth += 1;
                cursor += 1;
            }
            Token::RParen => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                cursor += 1;
            }
            Token::Comma if depth == 0 => {
                if let Some(item) = analyze_order_item(sql, tokens, item_start, cursor) {
                    items.push(item);
                }
                cursor += 1;
                item_start = cursor;
            }
            Token::SemiColon if depth == 0 => break,
            Token::Word(word) if depth == 0 && order_by_clause_terminator(word.keyword) => break,
            _ => cursor += 1,
        }
    }

    if let Some(item) = analyze_order_item(sql, tokens, item_start, cursor) {
        items.push(item);
    }

    let clause_end = clause_end_offset(sql, tokens, start_index, cursor);
    (items, clause_end, cursor)
}

fn analyze_order_item(
    sql: &str,
    tokens: &[TokenWithSpan],
    start_index: usize,
    end_index: usize,
) -> Option<Am003OrderItem> {
    let mut depth = 0usize;
    let mut has_direction = false;
    let mut nulls_insert_offset = None;
    let mut last_significant_end = None;

    for token in tokens.iter().take(end_index).skip(start_index) {
        let raw = &token.token;
        if is_trivia(raw) {
            continue;
        }

        match raw {
            Token::LParen => {
                depth += 1;
            }
            Token::RParen => {
                depth = depth.saturating_sub(1);
            }
            Token::Word(word) if depth == 0 => {
                if word.keyword == Keyword::ASC || word.keyword == Keyword::DESC {
                    has_direction = true;
                } else if word.keyword == Keyword::NULLS && nulls_insert_offset.is_none() {
                    nulls_insert_offset = last_significant_end;
                }
            }
            _ => {}
        }

        last_significant_end = token_with_span_offsets(sql, token).map(|(_, end)| end);
    }

    let fallback_insert = last_significant_end?;
    Some(Am003OrderItem {
        has_direction,
        insert_offset: nulls_insert_offset.unwrap_or(fallback_insert),
    })
}

fn clause_end_offset(
    sql: &str,
    tokens: &[TokenWithSpan],
    start_index: usize,
    end_index: usize,
) -> usize {
    for token in tokens.iter().take(end_index).skip(start_index).rev() {
        if is_trivia(&token.token) {
            continue;
        }
        if let Some((_, end)) = token_with_span_offsets(sql, token) {
            return end;
        }
    }
    sql.len()
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn is_order_keyword(token: &Token) -> bool {
    matches!(token, Token::Word(word) if word.keyword == Keyword::ORDER)
}

fn is_by_keyword(token: &Token) -> bool {
    matches!(token, Token::Word(word) if word.keyword == Keyword::BY)
}

fn order_by_clause_terminator(keyword: Keyword) -> bool {
    matches!(
        keyword,
        Keyword::LIMIT
            | Keyword::OFFSET
            | Keyword::FETCH
            | Keyword::UNION
            | Keyword::EXCEPT
            | Keyword::INTERSECT
            | Keyword::WINDOW
            | Keyword::INTO
            | Keyword::FOR
    )
}

fn next_non_trivia_index(tokens: &[TokenWithSpan], mut index: usize) -> Option<usize> {
    while index < tokens.len() {
        if !is_trivia(&tokens[index].token) {
            return Some(index);
        }
        index += 1;
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AmbiguousOrderBy;
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
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

    // --- Edge cases adopted from sqlfluff AM03 ---

    #[test]
    fn allows_unspecified_single_order_item() {
        let issues = run("SELECT * FROM t ORDER BY a");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_unspecified_all_order_items() {
        let issues = run("SELECT * FROM t ORDER BY a, b");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_all_explicit_order_items() {
        let issues = run("SELECT * FROM t ORDER BY a ASC, b DESC");
        assert!(issues.is_empty());

        let issues = run("SELECT * FROM t ORDER BY a DESC, b ASC");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_mixed_implicit_and_explicit_order_items() {
        let issues = run("SELECT * FROM t ORDER BY a, b DESC");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_003);

        let issues = run("SELECT * FROM t ORDER BY a DESC, b");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_nulls_clause_without_explicit_direction_when_mixed() {
        let issues = run("SELECT * FROM t ORDER BY a DESC, b NULLS LAST");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_consistent_order_by_with_comments() {
        let issues = run("SELECT * FROM t ORDER BY a /* Comment */ DESC, b ASC");
        assert!(issues.is_empty());
    }

    #[test]
    fn mixed_order_by_emits_safe_autofix_patch() {
        let sql = "SELECT * FROM t ORDER BY a DESC, b";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT * FROM t ORDER BY a DESC, b ASC");
    }

    #[test]
    fn mixed_order_by_with_nulls_clause_inserts_asc_before_nulls() {
        let sql = "SELECT * FROM t ORDER BY a DESC, b NULLS LAST";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT * FROM t ORDER BY a DESC, b ASC NULLS LAST");
    }
}
