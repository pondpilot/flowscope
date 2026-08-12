//! LINT_LT_015: Layout newlines.
//!
//! SQLFluff LT15 parity (current scope): detect excessive blank lines.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{
    Location, Span as TokenSpan, Token, TokenWithSpan, Tokenizer, Whitespace,
};
use std::ops::Range;

pub struct LayoutNewlines {
    maximum_empty_lines_inside_statements: usize,
    maximum_empty_lines_between_statements: usize,
    maximum_empty_lines_between_batches: Option<usize>,
}

impl LayoutNewlines {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            maximum_empty_lines_inside_statements: config
                .rule_option_usize(
                    issue_codes::LINT_LT_015,
                    "maximum_empty_lines_inside_statements",
                )
                .unwrap_or(1),
            maximum_empty_lines_between_statements: config
                .rule_option_usize(
                    issue_codes::LINT_LT_015,
                    "maximum_empty_lines_between_statements",
                )
                .unwrap_or(1),
            maximum_empty_lines_between_batches: config.rule_option_usize(
                issue_codes::LINT_LT_015,
                "maximum_empty_lines_between_batches",
            ),
        }
    }
}

impl Default for LayoutNewlines {
    fn default() -> Self {
        Self {
            maximum_empty_lines_inside_statements: 1,
            maximum_empty_lines_between_statements: 1,
            maximum_empty_lines_between_batches: None,
        }
    }
}

impl BuiltinLintRule for LayoutNewlines {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_015
    }

    fn name(&self) -> &'static str {
        "Layout newlines"
    }

    fn description(&self) -> &'static str {
        "Too many consecutive blank lines."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let (inside_range, statement_sql) = trimmed_statement_range_and_sql(ctx);
        let inside_tokens = tokenized_for_range(ctx, inside_range.clone());
        let effective_batch_limit = self
            .maximum_empty_lines_between_batches
            .unwrap_or(self.maximum_empty_lines_between_statements);
        let inside_blank_run = if ctx.dialect() == Dialect::Mssql
            && contains_tsql_batch_separator_line(statement_sql)
        {
            max_consecutive_blank_lines_in_tsql_batches(statement_sql)
        } else {
            max_consecutive_blank_lines(statement_sql, ctx.dialect(), inside_tokens.as_deref())
        };
        let excessive_inside = inside_blank_run > self.maximum_empty_lines_inside_statements;

        let mut gap_range = None;
        let excessive_between = if ctx.statement_index > 0 {
            let range = inter_statement_gap_range(ctx.sql, ctx.statement_range.start);
            let gap_sql = &ctx.sql[range.clone()];
            let gap_tokens = tokenized_for_range(ctx, range.clone());
            gap_range = Some(range);
            if ctx.dialect() == Dialect::Mssql && contains_tsql_batch_separator_line(gap_sql) {
                max_blank_lines_around_tsql_batch_separator(gap_sql) > effective_batch_limit
            } else if ctx.dialect() == Dialect::Mssql {
                blank_lines_in_inter_statement_gap(gap_sql, gap_tokens.as_deref())
                    > self.maximum_empty_lines_between_statements
            } else {
                max_consecutive_blank_lines(gap_sql, ctx.dialect(), gap_tokens.as_deref())
                    > self.maximum_empty_lines_between_statements
            }
        } else {
            false
        };

        if excessive_inside || excessive_between {
            let mut edits = Vec::new();
            if excessive_inside {
                edits.extend(excessive_blank_line_edits_for_range(
                    ctx.sql,
                    inside_range.clone(),
                    self.maximum_empty_lines_inside_statements,
                ));
            }
            if excessive_between {
                if let Some(range) = gap_range {
                    let max_gap_lines = if ctx.dialect() == Dialect::Mssql {
                        effective_batch_limit
                    } else {
                        self.maximum_empty_lines_between_statements
                    };
                    edits.extend(excessive_blank_line_edits_for_range(
                        ctx.sql,
                        range,
                        max_gap_lines,
                    ));
                }
            }

            let mut issue = Issue::info(
                issue_codes::LINT_LT_015,
                "SQL contains excessive blank lines.",
            )
            .with_statement(ctx.statement_index);
            if let Some(first_edit) = edits.first() {
                issue = issue.with_span(first_edit.span);
            }
            if !edits.is_empty() {
                issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, edits);
            }
            vec![issue]
        } else {
            Vec::new()
        }
    }
}

fn trimmed_statement_range_and_sql<'a>(ctx: &'a RuleContext) -> (Range<usize>, &'a str) {
    if let Some(range) = trimmed_statement_range_from_tokens(ctx) {
        return (range.clone(), &ctx.sql[range]);
    }

    let statement_sql = ctx.statement_sql();
    let (start, end) = trim_ascii_whitespace_bounds(statement_sql);

    (
        (ctx.statement_range.start + start)..(ctx.statement_range.start + end),
        &statement_sql[start..end],
    )
}

fn trim_ascii_whitespace_bounds(sql: &str) -> (usize, usize) {
    let mut start = sql.len();
    for (index, ch) in sql.char_indices() {
        if !ch.is_ascii_whitespace() {
            start = index;
            break;
        }
    }
    if start == sql.len() {
        return (sql.len(), sql.len());
    }

    let mut end = start;
    for (index, ch) in sql.char_indices().rev() {
        if !ch.is_ascii_whitespace() {
            end = index + ch.len_utf8();
            break;
        }
    }

    (start, end)
}

fn trimmed_statement_range_from_tokens(ctx: &RuleContext) -> Option<Range<usize>> {
    let statement_start = ctx.statement_range.start;
    let statement_end = ctx.statement_range.end;

    ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut first = None::<usize>;
        let mut last = None::<usize>;

        for token in tokens {
            let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                continue;
            };
            if start < statement_start || end > statement_end {
                continue;
            }
            if is_spacing_whitespace_token(&token.token) {
                continue;
            }

            first = Some(first.map_or(start, |current| current.min(start)));
            last = Some(last.map_or(end, |current| current.max(end)));
        }

        Some(match (first, last) {
            (Some(start), Some(end)) => start..end,
            _ => statement_start..statement_start,
        })
    })
}

fn max_consecutive_blank_lines(
    sql: &str,
    dialect: Dialect,
    tokens: Option<&[TokenWithSpan]>,
) -> usize {
    max_consecutive_blank_lines_tokenized(sql, dialect, tokens)
}

fn max_consecutive_blank_lines_tokenized(
    sql: &str,
    dialect: Dialect,
    tokens: Option<&[TokenWithSpan]>,
) -> usize {
    if sql.is_empty() {
        return 0;
    }

    let owned_tokens;
    let tokens = if let Some(tokens) = tokens {
        tokens
    } else {
        owned_tokens = match tokenized(sql, dialect) {
            Some(tokens) => tokens,
            None => return 0,
        };
        &owned_tokens
    };

    let mut non_blank_lines = std::collections::BTreeSet::new();
    for token in tokens {
        if is_spacing_whitespace_token(&token.token) {
            continue;
        }
        let start_line = token.span.start.line as usize;
        let end_line = match &token.token {
            Token::Whitespace(Whitespace::SingleLineComment { .. }) => start_line,
            _ => token.span.end.line as usize,
        };
        for line in start_line..=end_line {
            non_blank_lines.insert(line);
        }
    }
    if dialect == Dialect::Mssql {
        mark_tsql_batch_separator_lines(sql, &mut non_blank_lines);
    }

    let mut blank_run = 0usize;
    let mut max_run = 0usize;
    let line_count = line_count_from_tokens_or_sql(sql, tokens);

    for line in 1..=line_count {
        if non_blank_lines.contains(&line) {
            blank_run = 0;
        } else {
            blank_run += 1;
            max_run = max_run.max(blank_run);
        }
    }

    max_run
}

fn contains_tsql_batch_separator_line(sql: &str) -> bool {
    sql.lines()
        .any(|line| line.trim().eq_ignore_ascii_case("GO"))
}

fn max_consecutive_blank_lines_in_tsql_batches(sql: &str) -> usize {
    let mut batches = Vec::<String>::new();
    let mut current = String::new();

    for line in sql.split_inclusive('\n') {
        if line
            .trim_end_matches(['\n', '\r'])
            .trim()
            .eq_ignore_ascii_case("GO")
        {
            batches.push(std::mem::take(&mut current));
        } else {
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        batches.push(current);
    }

    if batches.is_empty() {
        return 0;
    }

    batches
        .iter()
        .map(|batch| {
            let (start, end) = trim_ascii_whitespace_bounds(batch);
            if start >= end {
                0
            } else {
                max_consecutive_blank_lines(&batch[start..end], Dialect::Mssql, None)
            }
        })
        .max()
        .unwrap_or(0)
}

fn max_blank_lines_around_tsql_batch_separator(gap_sql: &str) -> usize {
    let lines: Vec<&str> = gap_sql.split('\n').collect();
    let mut max_blank = 0usize;

    for (index, line) in lines.iter().enumerate() {
        if !line.trim().eq_ignore_ascii_case("GO") {
            continue;
        }

        let mut before = 0usize;
        let mut cursor = index;
        while cursor > 0 {
            let prev = lines[cursor - 1].trim_end_matches('\r');
            if !prev.trim().is_empty() {
                break;
            }
            before += 1;
            cursor -= 1;
        }

        let mut after = 0usize;
        let mut cursor = index + 1;
        while cursor < lines.len() {
            let next = lines[cursor].trim_end_matches('\r');
            if !next.trim().is_empty() {
                break;
            }
            after += 1;
            cursor += 1;
        }

        max_blank = max_blank.max(before.saturating_sub(1));
        max_blank = max_blank.max(after.saturating_sub(1));
    }

    max_blank
}

fn blank_lines_in_inter_statement_gap(gap_sql: &str, tokens: Option<&[TokenWithSpan]>) -> usize {
    if gap_sql.is_empty() {
        return 0;
    }

    if gap_sql.chars().all(|ch| ch.is_ascii_whitespace()) {
        return count_line_breaks(gap_sql).saturating_sub(1);
    }

    max_consecutive_blank_lines(gap_sql, Dialect::Mssql, tokens)
}

fn mark_tsql_batch_separator_lines(
    sql: &str,
    non_blank_lines: &mut std::collections::BTreeSet<usize>,
) {
    for (line_index, line) in sql.lines().enumerate() {
        if line.trim().eq_ignore_ascii_case("GO") {
            non_blank_lines.insert(line_index + 1);
        }
    }
}

fn line_count_from_tokens_or_sql(sql: &str, tokens: &[TokenWithSpan]) -> usize {
    let token_line_max = tokens
        .iter()
        .map(|token| match &token.token {
            Token::Whitespace(Whitespace::SingleLineComment { .. }) => token.span.start.line,
            _ => token.span.end.line,
        } as usize)
        .max()
        .unwrap_or(0);
    let fallback = count_line_breaks(sql) + 1;
    token_line_max.max(fallback)
}

fn count_line_breaks(text: &str) -> usize {
    let mut count = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\n' {
            count += 1;
            continue;
        }
        if ch == '\r' {
            count += 1;
            if matches!(chars.peek(), Some('\n')) {
                let _ = chars.next();
            }
        }
    }
    count
}

fn is_spacing_whitespace_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
    )
}

fn inter_statement_gap_range(sql: &str, statement_start: usize) -> Range<usize> {
    let before = &sql[..statement_start];
    let boundary = before
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_ascii_whitespace())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);
    boundary..statement_start
}

fn excessive_blank_line_edits_for_range(
    sql: &str,
    range: Range<usize>,
    max_empty_lines: usize,
) -> Vec<IssuePatchEdit> {
    if range.is_empty() || range.end > sql.len() {
        return Vec::new();
    }

    let bytes = sql.as_bytes();
    let allowed_newlines = max_empty_lines.saturating_add(1);
    let replacement = "\n".repeat(allowed_newlines);
    let mut edits = Vec::new();

    let mut i = range.start;
    while i < range.end {
        if bytes[i] != b'\n' {
            i += 1;
            continue;
        }

        let mut j = i + 1;
        let mut newline_count = 1usize;
        while j < range.end {
            let mut k = j;
            while k < range.end && is_ascii_whitespace_byte(bytes[k]) && bytes[k] != b'\n' {
                k += 1;
            }
            if k < range.end && bytes[k] == b'\n' {
                newline_count += 1;
                j = k + 1;
            } else {
                break;
            }
        }

        if newline_count > allowed_newlines {
            edits.push(IssuePatchEdit::new(Span::new(i, j), replacement.clone()));
        }
        i = j;
    }

    edits
}

fn is_ascii_whitespace_byte(byte: u8) -> bool {
    (byte as char).is_ascii_whitespace()
}

fn tokenized(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn tokenized_for_range(ctx: &RuleContext, range: Range<usize>) -> Option<Vec<TokenWithSpan>> {
    if range.is_empty() {
        return Some(Vec::new());
    }

    let (range_start_line, range_start_column) = offset_to_line_col(ctx.sql, range.start)?;
    ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut out = Vec::new();
        for token in tokens {
            let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                continue;
            };
            if start < range.start || end > range.end {
                continue;
            }

            let Some(start_loc) =
                relative_location(token.span.start, range_start_line, range_start_column)
            else {
                continue;
            };
            let Some(end_loc) =
                relative_location(token.span.end, range_start_line, range_start_column)
            else {
                continue;
            };

            out.push(TokenWithSpan::new(
                token.token.clone(),
                TokenSpan::new(start_loc, end_loc),
            ));
        }

        Some(out)
    })
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
    range_start_line: usize,
    range_start_column: usize,
) -> Option<Location> {
    let line = location.line as usize;
    let column = location.column as usize;
    if line < range_start_line {
        return None;
    }

    if line == range_start_line {
        if column < range_start_column {
            return None;
        }
        return Some(Location::new(1, (column - range_start_column + 1) as u64));
    }

    Some(Location::new(
        (line - range_start_line + 1) as u64,
        column as u64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run_with_rule(sql: &str, rule: &LayoutNewlines) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let mut ranges = Vec::with_capacity(statements.len());
        let mut search_start = 0usize;
        for index in 0..statements.len() {
            if index > 0 {
                search_start = first_non_whitespace_offset(sql, search_start);
            }
            let end = if index + 1 < statements.len() {
                sql[search_start..]
                    .find(';')
                    .map(|offset| search_start + offset + 1)
                    .unwrap_or(sql.len())
            } else {
                sql.len()
            };
            ranges.push(search_start..end);
            search_start = end;
        }

        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(
                    statement,
                    &RuleContext::new(sql, ranges[index].clone(), index),
                )
            })
            .collect()
    }

    fn first_non_whitespace_offset(sql: &str, from: usize) -> usize {
        let mut offset = from;
        for ch in sql[from..].chars() {
            if ch.is_ascii_whitespace() {
                offset += ch.len_utf8();
            } else {
                break;
            }
        }
        offset
    }

    fn run(sql: &str) -> Vec<Issue> {
        run_with_rule(sql, &LayoutNewlines::default())
    }

    fn run_statementless_with_rule_in_dialect(
        sql: &str,
        rule: &LayoutNewlines,
        dialect: Dialect,
    ) -> Vec<Issue> {
        let placeholder = parse_sql("SELECT 1").expect("parse placeholder");
        rule.check_with_context(
            &placeholder[0],
            &RuleContext::new(sql, 0..sql.len(), 0).with_dialect(dialect),
        )
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
    fn flags_excessive_blank_lines() {
        let issues = run("SELECT 1\n\n\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix("SELECT 1\n\n\nFROM t", &issues[0]).expect("apply fix");
        assert_eq!(fixed, "SELECT 1\n\nFROM t");
    }

    #[test]
    fn does_not_flag_single_blank_line() {
        assert!(run("SELECT 1\n\nFROM t").is_empty());
    }

    #[test]
    fn flags_blank_lines_with_whitespace() {
        let issues = run("SELECT 1\n\n   \nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }

    #[test]
    fn configured_inside_limit_allows_two_blank_lines() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.newlines".to_string(),
                serde_json::json!({"maximum_empty_lines_inside_statements": 2}),
            )]),
        };
        let issues = run_with_rule(
            "SELECT 1\n\n\nFROM t",
            &LayoutNewlines::from_config(&config),
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn configured_between_limit_flags_statement_gap() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_LT_015".to_string(),
                serde_json::json!({"maximum_empty_lines_between_statements": 1}),
            )]),
        };
        let issues = run_with_rule(
            "SELECT 1;\n\n\nSELECT 2",
            &LayoutNewlines::from_config(&config),
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
        let fixed = apply_issue_autofix("SELECT 1;\n\n\nSELECT 2", &issues[0]).expect("apply fix");
        assert_eq!(fixed, "SELECT 1;\n\nSELECT 2");
    }

    #[test]
    fn flags_blank_lines_after_inline_comment() {
        let issues = run("SELECT 1 -- inline\n\n\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }

    #[test]
    fn flags_blank_lines_between_statements_with_comment_gap() {
        let sql = "SELECT 1;\n-- there was a comment\n\n\nSELECT 2";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply fix");
        assert!(
            fixed.contains("-- there was a comment"),
            "comment should remain after LT015 autofix: {fixed}"
        );
        assert_eq!(fixed, "SELECT 1;\n-- there was a comment\n\nSELECT 2");
    }

    #[test]
    fn flags_excessive_blank_lines_with_crlf_line_breaks() {
        let issues = run("SELECT 1\r\n\r\n\r\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_015);
    }

    #[test]
    fn trim_ascii_whitespace_bounds_handles_all_whitespace_input() {
        let (start, end) = trim_ascii_whitespace_bounds(" \t\r\n ");
        assert_eq!((start, end), (5, 5));
    }

    #[test]
    fn mssql_go_batch_separator_breaks_blank_line_runs() {
        let sql = "SELECT 1;\n\nGO\n\nSELECT 2;\n";
        let max = max_consecutive_blank_lines(sql, Dialect::Mssql, None);
        assert_eq!(
            max, 1,
            "GO should be treated as a non-blank batch separator line",
        );
    }

    #[test]
    fn mssql_go_batch_separator_with_two_blank_lines_still_flags() {
        let sql = "SELECT 1;\n\nGO\n\n\nSELECT 2;\n";
        let max = max_consecutive_blank_lines(sql, Dialect::Mssql, None);
        assert_eq!(max, 2);
    }

    #[test]
    fn mssql_between_statement_gap_counts_empty_lines_not_line_breaks() {
        assert_eq!(blank_lines_in_inter_statement_gap("\n\n", None), 1);
        assert_eq!(blank_lines_in_inter_statement_gap("\n\n\n", None), 2);
    }

    #[test]
    fn mssql_passes_single_empty_line_between_batches() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.newlines".to_string(),
                serde_json::json!({"maximum_empty_lines_between_batches": 1}),
            )]),
        };
        let sql = "SELECT 1;\n\nGO\n\nSELECT 2;\n";
        let issues = run_statementless_with_rule_in_dialect(
            sql,
            &LayoutNewlines::from_config(&config),
            Dialect::Mssql,
        );
        assert!(
            issues.is_empty(),
            "mssql GO batch with one empty line should pass"
        );
    }

    #[test]
    fn mssql_passes_inside_batch_statement_limit_before_go() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.newlines".to_string(),
                serde_json::json!({
                    "maximum_empty_lines_inside_statements": 1,
                    "maximum_empty_lines_between_statements": 1
                }),
            )]),
        };
        let sql = "SELECT 1;\n\nSELECT 2;\n\nGO\n";
        let issues = run_statementless_with_rule_in_dialect(
            sql,
            &LayoutNewlines::from_config(&config),
            Dialect::Mssql,
        );
        assert!(
            issues.is_empty(),
            "inside-batch statement spacing should be evaluated independently of GO separator"
        );
    }
}
