//! LINT_ST_008: Structure distinct.
//!
//! SQLFluff ST08 parity: `SELECT DISTINCT(<expr>)` should be rewritten to
//! `SELECT DISTINCT <expr>`.

use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

pub struct StructureDistinct;

impl BuiltinLintRule for StructureDistinct {
    fn code(&self) -> &'static str {
        issue_codes::LINT_ST_008
    }

    fn name(&self) -> &'static str {
        "Structure distinct"
    }

    fn description(&self) -> &'static str {
        "'DISTINCT' used with parentheses."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let candidates = st008_autofix_candidates(ctx.statement_sql(), ctx.dialect());

        candidates
            .into_iter()
            .map(|candidate| {
                Issue::info(issue_codes::LINT_ST_008, "DISTINCT used with parentheses.")
                    .with_statement(ctx.statement_index)
                    .with_span(candidate.span)
                    .with_autofix_edits(IssueAutofixApplicability::Safe, candidate.edits)
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
struct St008AutofixCandidate {
    span: Span,
    edits: Vec<IssuePatchEdit>,
}

fn st008_autofix_candidates(sql: &str, dialect: Dialect) -> Vec<St008AutofixCandidate> {
    let Some(tokens) = tokenized(sql, dialect) else {
        return Vec::new();
    };

    let mut candidates = Vec::new();
    for distinct_index in 0..tokens.len() {
        if !is_distinct_keyword(&tokens[distinct_index].token) {
            continue;
        }

        let Some(next_index) = next_non_trivia_index(&tokens, distinct_index + 1) else {
            continue;
        };

        // Skip `DISTINCT ON(...)` — valid Postgres syntax.
        if matches!(&tokens[next_index].token, Token::Word(word) if word.keyword == Keyword::ON) {
            continue;
        }

        let left_paren_index = next_index;
        if !matches!(tokens[left_paren_index].token, Token::LParen) {
            continue;
        }

        // Check if there's already a space between DISTINCT and `(`.
        let has_space_before_paren =
            has_whitespace_between(&tokens, distinct_index, left_paren_index);

        let Some((right_paren_index, has_projection_comma, has_subquery)) =
            find_matching_distinct_rparen(&tokens, left_paren_index)
        else {
            continue;
        };
        if has_projection_comma || has_subquery {
            continue;
        }

        // Determine whether parens can be removed or only a space is needed.
        let paren_removable = next_token_allows_paren_removal(&tokens, right_paren_index + 1);

        // If parens are needed and there's already a space, no violation.
        if !paren_removable && has_space_before_paren {
            continue;
        }

        let Some((distinct_start, distinct_end)) =
            token_with_span_offsets(sql, &tokens[distinct_index])
        else {
            continue;
        };
        let Some((_, left_paren_end)) = token_with_span_offsets(sql, &tokens[left_paren_index])
        else {
            continue;
        };
        let Some((right_paren_start, right_paren_end)) =
            token_with_span_offsets(sql, &tokens[right_paren_index])
        else {
            continue;
        };
        if left_paren_end < distinct_end || right_paren_end <= right_paren_start {
            continue;
        }

        let edits = if paren_removable {
            // Remove parentheses: `DISTINCT(x)` → `DISTINCT x`
            vec![
                IssuePatchEdit::new(Span::new(distinct_end, left_paren_end), " "),
                IssuePatchEdit::new(Span::new(right_paren_start, right_paren_end), ""),
            ]
        } else {
            // Keep parentheses but add space: `DISTINCT(x) * y` → `DISTINCT (x) * y`
            vec![IssuePatchEdit::new(
                Span::new(distinct_end, distinct_end),
                " ",
            )]
        };

        candidates.push(St008AutofixCandidate {
            span: Span::new(distinct_start, distinct_end),
            edits,
        });
    }

    candidates
}

fn is_distinct_keyword(token: &Token) -> bool {
    matches!(token, Token::Word(word) if word.keyword == Keyword::DISTINCT)
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

/// Finds the matching `)` for the opening `(` at `left_paren_index`.
/// Returns `(index, has_comma, has_subquery)`.
fn find_matching_distinct_rparen(
    tokens: &[TokenWithSpan],
    left_paren_index: usize,
) -> Option<(usize, bool, bool)> {
    let mut depth = 0usize;
    let mut has_projection_comma = false;
    let mut has_subquery = false;

    for (index, token) in tokens.iter().enumerate().skip(left_paren_index) {
        if is_trivia_token(&token.token) {
            continue;
        }

        match &token.token {
            Token::LParen => {
                depth += 1;
            }
            Token::RParen => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some((index, has_projection_comma, has_subquery));
                }
            }
            Token::Comma if depth == 1 => {
                has_projection_comma = true;
            }
            Token::Word(word) if depth == 1 && word.keyword == Keyword::SELECT => {
                has_subquery = true;
            }
            _ => {}
        }
    }

    None
}

/// Returns true when the parentheses around the DISTINCT expression can be
/// safely removed.  When the next meaningful token after the closing paren is
/// an operator, the parens serve as grouping and should be kept.
fn next_token_allows_paren_removal(tokens: &[TokenWithSpan], start: usize) -> bool {
    let Some(index) = next_non_trivia_index(tokens, start) else {
        return true;
    };

    match &tokens[index].token {
        // A comma means there are more projections — parens can still be removed
        // since they only wrap the first projection item.
        Token::Comma | Token::SemiColon | Token::RParen => true,
        Token::Word(word) => {
            matches!(
                word.keyword,
                Keyword::FROM
                    | Keyword::WHERE
                    | Keyword::GROUP
                    | Keyword::HAVING
                    | Keyword::QUALIFY
                    | Keyword::ORDER
                    | Keyword::LIMIT
                    | Keyword::FETCH
                    | Keyword::OFFSET
                    | Keyword::UNION
                    | Keyword::EXCEPT
                    | Keyword::INTERSECT
                    | Keyword::WINDOW
                    | Keyword::INTO
            )
        }
        _ => false,
    }
}

fn has_whitespace_between(tokens: &[TokenWithSpan], start: usize, end: usize) -> bool {
    (start + 1..end).any(|i| is_trivia_token(&tokens[i].token))
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

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = StructureDistinct;
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

    #[test]
    fn flags_distinct_parenthesized_projection() {
        let issues = run("SELECT DISTINCT(a) FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_ST_008);
    }

    #[test]
    fn does_not_flag_normal_distinct_projection() {
        let issues = run("SELECT DISTINCT a FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_multiple_projections_removes_parens() {
        // SQLFluff: test_fail_distinct_with_parenthesis_6
        let sql = "SELECT DISTINCT(a), b\n";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT DISTINCT a, b\n");
    }

    #[test]
    fn flags_in_nested_select_scope() {
        let issues = run("SELECT * FROM (SELECT DISTINCT(a) FROM t) AS sub");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn emits_safe_autofix_for_distinct_parenthesized_projection() {
        let sql = "SELECT DISTINCT(a) FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);

        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 2);

        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT DISTINCT a FROM t");
    }

    #[test]
    fn adds_space_when_parens_needed_for_grouping() {
        // SQLFluff: test_fail_distinct_with_parenthesis_2
        let sql = "SELECT DISTINCT(a + b) * c";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT DISTINCT (a + b) * c");
    }

    #[test]
    fn does_not_flag_distinct_with_space_and_needed_parens() {
        // SQLFluff: test_fail_distinct_with_parenthesis_4
        assert!(run("SELECT DISTINCT (a + b) * c").is_empty());
    }

    #[test]
    fn flags_distinct_space_paren_single_column() {
        // SQLFluff: test_fail_distinct_with_parenthesis_3
        let sql = "SELECT DISTINCT (a)";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT DISTINCT a");
    }

    #[test]
    fn flags_distinct_inside_count() {
        // SQLFluff: test_fail_distinct_column_inside_count
        let sql = "SELECT COUNT(DISTINCT(unique_key))\n";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT COUNT(DISTINCT unique_key)\n");
    }

    #[test]
    fn flags_distinct_concat_inside_count() {
        // SQLFluff: test_fail_distinct_concat_inside_count
        let sql = "SELECT COUNT(DISTINCT(CONCAT(col1, '-', col2, '-', col3)))\n";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "SELECT COUNT(DISTINCT CONCAT(col1, '-', col2, '-', col3))\n"
        );
    }

    #[test]
    fn does_not_flag_distinct_on_postgres() {
        // SQLFluff: test_fail_distinct_with_parenthesis_7
        // DISTINCT ON(...) is valid Postgres syntax, not flagged.
        assert!(run("SELECT DISTINCT ON(bcolor) bcolor, fcolor FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_distinct_subquery_inside_count() {
        // SQLFluff: test_pass_distinct_subquery_inside_count
        let sql = "SELECT COUNT(DISTINCT(SELECT ANY_VALUE(id) FROM UNNEST(tag) t))";
        assert!(run(sql).is_empty());
    }
}
