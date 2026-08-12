//! LINT_CV_001: Not-equal style.
//!
//! SQLFluff CV01 parity (current scope): flag statements that mix `<>` and
//! `!=` not-equal operators.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::linter::visit::visit_expressions;
use crate::types::{issue_codes, Issue, IssueAutofixApplicability, IssuePatchEdit};
use sqlparser::ast::{BinaryOperator, Expr, Spanned, Statement};
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreferredNotEqualStyle {
    Consistent,
    CStyle,
    Ansi,
}

impl PreferredNotEqualStyle {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_CV_001, "preferred_not_equal_style")
            .unwrap_or("consistent")
            .to_ascii_lowercase()
            .as_str()
        {
            "c_style" => Self::CStyle,
            "ansi" => Self::Ansi,
            _ => Self::Consistent,
        }
    }

    fn violation(self, usage: &NotEqualUsage) -> bool {
        match self {
            Self::Consistent => usage.saw_angle_style && usage.saw_bang_style,
            Self::CStyle => usage.saw_angle_style,
            Self::Ansi => usage.saw_bang_style,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Consistent => "Use consistent not-equal style.",
            Self::CStyle => "Use `!=` for not-equal comparisons.",
            Self::Ansi => "Use `<>` for not-equal comparisons.",
        }
    }

    fn target_style(self, occurrences: &[NotEqualOccurrence]) -> Option<NotEqualStyle> {
        match self {
            // In consistent mode, normalize to whichever style appears first
            // in the statement (SQLFluff parity).
            Self::Consistent => occurrences.first().map(|o| o.style),
            Self::CStyle => Some(NotEqualStyle::Bang),
            Self::Ansi => Some(NotEqualStyle::Angle),
        }
    }

    fn violating_occurrences(self, occurrences: &[NotEqualOccurrence]) -> Vec<NotEqualOccurrence> {
        let Some(target_style) = self.target_style(occurrences) else {
            return Vec::new();
        };

        occurrences
            .iter()
            .copied()
            .filter(|occurrence| occurrence.style != target_style)
            .collect()
    }
}

pub struct ConventionNotEqual {
    preferred_style: PreferredNotEqualStyle,
}

impl ConventionNotEqual {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            preferred_style: PreferredNotEqualStyle::from_config(config),
        }
    }
}

impl Default for ConventionNotEqual {
    fn default() -> Self {
        Self {
            preferred_style: PreferredNotEqualStyle::Consistent,
        }
    }
}

impl BuiltinLintRule for ConventionNotEqual {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_001
    }

    fn name(&self) -> &'static str {
        "Not-equal style"
    }

    fn description(&self) -> &'static str {
        "Consistent usage of '!=' or '<>' for \"not equal to\" operator."
    }

    fn check_with_context(&self, statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let tokens =
            tokenized_for_context(ctx).or_else(|| tokenized(ctx.statement_sql(), ctx.dialect()));
        let mut occurrences = statement_not_equal_occurrences_with_tokens(
            statement,
            ctx.statement_sql(),
            tokens.as_deref(),
        );
        occurrences.sort_by_key(|occurrence| (occurrence.start, occurrence.end));
        let usage = usage_from_occurrences(&occurrences);

        if self.preferred_style.violation(&usage) {
            let violating_occurrences = self.preferred_style.violating_occurrences(&occurrences);
            let mut issue = Issue::info(issue_codes::LINT_CV_001, self.preferred_style.message())
                .with_statement(ctx.statement_index);

            if let (Some(target_style), Some(first_occurrence)) = (
                self.preferred_style.target_style(&occurrences),
                violating_occurrences.first().copied(),
            ) {
                let issue_span =
                    ctx.span_from_statement_offset(first_occurrence.start, first_occurrence.end);
                let mut edits = Vec::new();
                for occurrence in violating_occurrences {
                    if let Some((first_pos, second_pos)) = occurrence.split_positions {
                        let (first_replacement, second_replacement) =
                            target_style.split_replacements();
                        edits.push(IssuePatchEdit::new(
                            ctx.span_from_statement_offset(first_pos, first_pos + 1),
                            first_replacement,
                        ));
                        edits.push(IssuePatchEdit::new(
                            ctx.span_from_statement_offset(second_pos, second_pos + 1),
                            second_replacement,
                        ));
                    } else {
                        edits.push(IssuePatchEdit::new(
                            ctx.span_from_statement_offset(occurrence.start, occurrence.end),
                            target_style.replacement(),
                        ));
                    }
                }
                issue = issue
                    .with_span(issue_span)
                    .with_autofix_edits(IssueAutofixApplicability::Safe, edits);
            }

            vec![issue]
        } else {
            Vec::new()
        }
    }
}

#[derive(Default)]
struct NotEqualUsage {
    saw_angle_style: bool,
    saw_bang_style: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NotEqualStyle {
    Angle,
    Bang,
}

impl NotEqualStyle {
    fn replacement(self) -> &'static str {
        match self {
            Self::Angle => "<>",
            Self::Bang => "!=",
        }
    }

    fn split_replacements(self) -> (&'static str, &'static str) {
        match self {
            Self::Angle => ("<", ">"),
            Self::Bang => ("!", "="),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NotEqualOccurrence {
    style: NotEqualStyle,
    start: usize,
    end: usize,
    split_positions: Option<(usize, usize)>,
}

fn statement_not_equal_occurrences_with_tokens(
    statement: &Statement,
    sql: &str,
    tokens: Option<&[LocatedToken]>,
) -> Vec<NotEqualOccurrence> {
    let mut occurrences = Vec::new();
    visit_expressions(statement, &mut |expr| {
        let occurrence = match expr {
            Expr::BinaryOp { left, op, right } if *op == BinaryOperator::NotEq => {
                not_equal_occurrence_between(sql, left.as_ref(), right.as_ref(), tokens)
            }
            Expr::AnyOp {
                left,
                compare_op,
                right,
                ..
            } if *compare_op == BinaryOperator::NotEq => {
                not_equal_occurrence_between(sql, left.as_ref(), right.as_ref(), tokens)
            }
            Expr::AllOp {
                left,
                compare_op,
                right,
            } if *compare_op == BinaryOperator::NotEq => {
                not_equal_occurrence_between(sql, left.as_ref(), right.as_ref(), tokens)
            }
            _ => None,
        };

        if let Some(occurrence) = occurrence {
            occurrences.push(occurrence);
        }
    });

    occurrences.extend(scan_split_not_equal_occurrences(sql));
    occurrences.sort_by_key(|occurrence| (occurrence.start, occurrence.end));
    occurrences.dedup_by(|left, right| {
        left.style == right.style
            && left.start == right.start
            && left.end == right.end
            && left.split_positions == right.split_positions
    });

    occurrences
}

fn usage_from_occurrences(occurrences: &[NotEqualOccurrence]) -> NotEqualUsage {
    let mut usage = NotEqualUsage::default();
    for occurrence in occurrences {
        match occurrence.style {
            NotEqualStyle::Angle => usage.saw_angle_style = true,
            NotEqualStyle::Bang => usage.saw_bang_style = true,
        }
    }
    usage
}

fn not_equal_occurrence_between(
    sql: &str,
    left: &Expr,
    right: &Expr,
    tokens: Option<&[LocatedToken]>,
) -> Option<NotEqualOccurrence> {
    let left_end = left.span().end;
    let right_start = right.span().start;
    if left_end.line == 0
        || left_end.column == 0
        || right_start.line == 0
        || right_start.column == 0
    {
        return None;
    }

    let start = line_col_to_offset(sql, left_end.line as usize, left_end.column as usize)?;
    let end = line_col_to_offset(sql, right_start.line as usize, right_start.column as usize)?;
    if end < start {
        return None;
    }

    if let Some(tokens) = tokens {
        return not_equal_occurrence_in_tokens(sql, tokens, start, end);
    }

    None
}

fn not_equal_occurrence_in_tokens(
    sql: &str,
    tokens: &[LocatedToken],
    start: usize,
    end: usize,
) -> Option<NotEqualOccurrence> {
    for token in tokens {
        if token.end <= start || token.start >= end {
            continue;
        }
        if is_trivia_token(&token.token) {
            continue;
        }

        if !matches!(token.token, Token::Neq) {
            return None;
        }
        if token.end > sql.len() {
            return None;
        }

        let raw = &sql[token.start..token.end];
        let style = match raw {
            "<>" => Some(NotEqualStyle::Angle),
            "!=" => Some(NotEqualStyle::Bang),
            _ => None,
        }?;

        return Some(NotEqualOccurrence {
            style,
            start: token.start,
            end: token.end,
            split_positions: None,
        });
    }

    None
}

fn scan_split_not_equal_occurrences(sql: &str) -> Vec<NotEqualOccurrence> {
    let mut occurrences = Vec::new();
    let bytes = sql.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        let (style, expected_second) = match bytes[index] {
            b'<' => (NotEqualStyle::Angle, b'>'),
            b'!' => (NotEqualStyle::Bang, b'='),
            _ => {
                index += 1;
                continue;
            }
        };

        let mut probe = index + 1;
        let mut saw_separator = false;

        while probe < bytes.len() {
            match bytes[probe] {
                b' ' | b'\t' | b'\n' | b'\r' => {
                    saw_separator = true;
                    probe += 1;
                }
                b'-' if probe + 1 < bytes.len() && bytes[probe + 1] == b'-' => {
                    saw_separator = true;
                    probe += 2;
                    while probe < bytes.len() && bytes[probe] != b'\n' {
                        probe += 1;
                    }
                }
                b'/' if probe + 1 < bytes.len() && bytes[probe + 1] == b'*' => {
                    saw_separator = true;
                    probe += 2;
                    while probe + 1 < bytes.len() {
                        if bytes[probe] == b'*' && bytes[probe + 1] == b'/' {
                            probe += 2;
                            break;
                        }
                        probe += 1;
                    }
                }
                _ => break,
            }
        }

        if saw_separator && probe < bytes.len() && bytes[probe] == expected_second {
            occurrences.push(NotEqualOccurrence {
                style,
                start: index,
                end: probe + 1,
                split_positions: Some((index, probe)),
            });
            index = probe + 1;
            continue;
        }

        index += 1;
    }

    occurrences
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn tokenized(sql: &str, dialect: crate::types::Dialect) -> Option<Vec<LocatedToken>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        let Some((start, end)) = token_with_span_offsets(sql, &token) else {
            continue;
        };
        out.push(LocatedToken {
            token: token.token,
            start,
            end,
        });
    }
    Some(out)
}

fn tokenized_for_context(ctx: &RuleContext) -> Option<Vec<LocatedToken>> {
    let statement_start = ctx.statement_range.start;
    let from_document = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        Some(
            tokens
                .iter()
                .filter_map(|token| {
                    let (start, end) = token_with_span_offsets(ctx.sql, token)?;
                    if start < ctx.statement_range.start || end > ctx.statement_range.end {
                        return None;
                    }

                    Some(LocatedToken {
                        token: token.token.clone(),
                        start: start - statement_start,
                        end: end - statement_start,
                    })
                })
                .collect::<Vec<_>>(),
        )
    });

    if let Some(tokens) = from_document {
        return Some(tokens);
    }

    tokenized(ctx.statement_sql(), ctx.dialect())
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

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
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
        Some(sql.len())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionNotEqual::default();
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
        let mut output = sql.to_string();
        let mut edits = autofix.edits.clone();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        for edit in edits.into_iter().rev() {
            output.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(output)
    }

    #[test]
    fn flags_mixed_not_equal_styles() {
        // Consistent mode: first occurrence (`<>`) determines the target style.
        // The violating occurrence is `!=`.
        let sql = "SELECT * FROM t WHERE a <> b AND c != d";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_001);

        let bang_start = sql.find("!=").expect("bang operator");
        let issue_span = issues[0].span.expect("issue span");
        assert_eq!(issue_span.start, bang_start);
        assert_eq!(issue_span.end, bang_start + 2);

        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        assert_eq!(autofix.edits[0].span.start, bang_start);
        assert_eq!(autofix.edits[0].span.end, bang_start + 2);
        assert_eq!(autofix.edits[0].replacement, "<>");
    }

    #[test]
    fn does_not_flag_single_not_equal_style() {
        assert!(run("SELECT * FROM t WHERE a <> b").is_empty());
        assert!(run("SELECT * FROM t WHERE a != b").is_empty());
    }

    #[test]
    fn does_not_flag_not_equal_tokens_inside_string_literal() {
        assert!(run("SELECT 'a <> b and c != d' AS txt FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_not_equal_tokens_inside_comments() {
        assert!(run("SELECT * FROM t -- a <> b and c != d").is_empty());
    }

    #[test]
    fn c_style_preference_flags_angle_bracket_operator() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.not_equal".to_string(),
                serde_json::json!({"preferred_not_equal_style": "c_style"}),
            )]),
        };
        let rule = ConventionNotEqual::from_config(&config);
        let sql = "SELECT * FROM t WHERE a <> b";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        let angle_start = sql.find("<>").expect("angle operator");
        let issue_span = issues[0].span.expect("issue span");
        assert_eq!(issue_span.start, angle_start);
        assert_eq!(issue_span.end, angle_start + 2);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        assert_eq!(autofix.edits[0].span.start, angle_start);
        assert_eq!(autofix.edits[0].span.end, angle_start + 2);
        assert_eq!(autofix.edits[0].replacement, "!=");
    }

    #[test]
    fn c_style_preference_includes_all_angle_operator_edits() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.not_equal".to_string(),
                serde_json::json!({"preferred_not_equal_style": "c_style"}),
            )]),
        };
        let rule = ConventionNotEqual::from_config(&config);
        let sql = "SELECT * FROM t WHERE a <> b AND c <> d";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);

        let first_start = sql.find("<>").expect("first angle operator");
        let second_start = sql[first_start + 2..]
            .find("<>")
            .map(|offset| first_start + 2 + offset)
            .expect("second angle operator");

        let issue_span = issues[0].span.expect("issue span");
        assert_eq!(issue_span.start, first_start);
        assert_eq!(issue_span.end, first_start + 2);

        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 2);
        assert_eq!(autofix.edits[0].span.start, first_start);
        assert_eq!(autofix.edits[0].span.end, first_start + 2);
        assert_eq!(autofix.edits[0].replacement, "!=");
        assert_eq!(autofix.edits[1].span.start, second_start);
        assert_eq!(autofix.edits[1].span.end, second_start + 2);
        assert_eq!(autofix.edits[1].replacement, "!=");
    }

    #[test]
    fn ansi_preference_flags_bang_operator() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_001".to_string(),
                serde_json::json!({"preferred_not_equal_style": "ansi"}),
            )]),
        };
        let rule = ConventionNotEqual::from_config(&config);
        let sql = "SELECT * FROM t WHERE a != b";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        let bang_start = sql.find("!=").expect("bang operator");
        let issue_span = issues[0].span.expect("issue span");
        assert_eq!(issue_span.start, bang_start);
        assert_eq!(issue_span.end, bang_start + 2);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        assert_eq!(autofix.edits.len(), 1);
        assert_eq!(autofix.edits[0].span.start, bang_start);
        assert_eq!(autofix.edits[0].span.end, bang_start + 2);
        assert_eq!(autofix.edits[0].replacement, "<>");
    }

    #[test]
    fn c_style_preference_fixes_split_angle_operator_around_comment() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.not_equal".to_string(),
                serde_json::json!({"preferred_not_equal_style": "c_style"}),
            )]),
        };
        let rule = ConventionNotEqual::from_config(&config);
        let sql = "SELECT * FROM X WHERE 1  <\n  -- some comment\n> 2\n";
        let statements = parse_sql("SELECT 1").expect("synthetic parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.edits.len(), 2);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "SELECT * FROM X WHERE 1  !\n  -- some comment\n= 2\n"
        );
    }

    #[test]
    fn ansi_preference_fixes_split_bang_operator_around_comment() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.not_equal".to_string(),
                serde_json::json!({"preferred_not_equal_style": "ansi"}),
            )]),
        };
        let rule = ConventionNotEqual::from_config(&config);
        let sql = "SELECT * FROM X WHERE 1  !\n  -- some comment\n= 2\n";
        let statements = parse_sql("SELECT 1").expect("synthetic parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.edits.len(), 2);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "SELECT * FROM X WHERE 1  <\n  -- some comment\n> 2\n"
        );
    }
}
