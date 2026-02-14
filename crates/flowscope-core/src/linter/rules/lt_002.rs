//! LINT_LT_002: Layout indent.
//!
//! SQLFluff LT02 parity (current scope): flag odd indentation widths on
//! subsequent lines.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};
use std::collections::BTreeMap;

pub struct LayoutIndent {
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndentStyle {
    Spaces,
    Tabs,
}

impl LayoutIndent {
    pub fn from_config(config: &LintConfig) -> Self {
        let tab_space_size = config
            .rule_option_usize(issue_codes::LINT_LT_002, "tab_space_size")
            .or_else(|| config.section_option_usize("indentation", "tab_space_size"))
            .unwrap_or(4)
            .max(1);

        let indent_style = match config
            .rule_option_str(issue_codes::LINT_LT_002, "indent_unit")
            .or_else(|| config.section_option_str("indentation", "indent_unit"))
        {
            Some(value) if value.eq_ignore_ascii_case("tab") => IndentStyle::Tabs,
            _ => IndentStyle::Spaces,
        };

        let indent_unit_numeric = config
            .rule_option_usize(issue_codes::LINT_LT_002, "indent_unit")
            .or_else(|| config.section_option_usize("indentation", "indent_unit"));
        let indent_unit = match indent_style {
            IndentStyle::Spaces => indent_unit_numeric.unwrap_or(4).max(1),
            IndentStyle::Tabs => indent_unit_numeric.unwrap_or(tab_space_size).max(1),
        };
        Self {
            indent_unit,
            tab_space_size,
            indent_style,
        }
    }
}

impl Default for LayoutIndent {
    fn default() -> Self {
        Self {
            indent_unit: 4,
            tab_space_size: 4,
            indent_style: IndentStyle::Spaces,
        }
    }
}

impl LintRule for LayoutIndent {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_002
    }

    fn name(&self) -> &'static str {
        "Layout indent"
    }

    fn description(&self) -> &'static str {
        "Indentation should use consistent step sizes."
    }

    fn check(&self, _statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut has_violation = first_line_is_indented(ctx);
        for snapshot in line_indent_snapshots(ctx, self.tab_space_size) {
            let indent = snapshot.indent;

            // Rule-level tests and fallback contexts may not expose raw leading
            // whitespace via `statement_range`; keep direct first-line check.
            if snapshot.line_index == 0 && indent.width > 0 {
                has_violation = true;
                break;
            }

            if indent.has_mixed_indent_chars {
                has_violation = true;
                break;
            }

            if matches!(self.indent_style, IndentStyle::Spaces) && indent.tab_count > 0 {
                has_violation = true;
                break;
            }

            if matches!(self.indent_style, IndentStyle::Tabs) && indent.space_count > 0 {
                has_violation = true;
                break;
            }

            if indent.width > 0 && indent.width % self.indent_unit != 0 {
                has_violation = true;
                break;
            }
        }

        if has_violation {
            vec![Issue::info(
                issue_codes::LINT_LT_002,
                "Indentation appears inconsistent.",
            )
            .with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn first_line_is_indented(ctx: &LintContext) -> bool {
    let statement_start = ctx.statement_range.start;
    if statement_start == 0 {
        return false;
    }

    let line_start = ctx.sql[..statement_start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let leading = &ctx.sql[line_start..statement_start];
    !leading.is_empty() && leading.chars().all(char::is_whitespace)
}

#[derive(Clone, Copy)]
struct LeadingIndent {
    width: usize,
    space_count: usize,
    tab_count: usize,
    has_mixed_indent_chars: bool,
}

#[derive(Clone, Copy)]
struct LineIndentSnapshot {
    line_index: usize,
    indent: LeadingIndent,
}

fn line_indent_snapshots(ctx: &LintContext, tab_space_size: usize) -> Vec<LineIndentSnapshot> {
    if let Some(tokens) = tokenize_with_offsets_for_context(ctx) {
        let statement_start_line = offset_to_line(ctx.sql, ctx.statement_range.start);
        let mut first_token_by_line: BTreeMap<usize, usize> = BTreeMap::new();
        for token in &tokens {
            if token.start < ctx.statement_range.start || token.start >= ctx.statement_range.end {
                continue;
            }
            if is_whitespace_token(&token.token) {
                continue;
            }
            first_token_by_line
                .entry(token.start_line)
                .or_insert(token.start);
        }

        return first_token_by_line
            .into_iter()
            .filter_map(|(line, token_start)| {
                let line_start = ctx.sql[..token_start]
                    .rfind('\n')
                    .map_or(0, |index| index + 1);
                let leading = &ctx.sql[line_start..token_start];
                Some(LineIndentSnapshot {
                    line_index: line.saturating_sub(statement_start_line),
                    indent: leading_indent_from_prefix(leading, tab_space_size),
                })
            })
            .collect();
    }

    let sql = ctx.statement_sql();
    let Some(tokens) = tokenize_with_locations(sql, ctx.dialect()) else {
        return sql
            .lines()
            .enumerate()
            .filter_map(|(line_index, line)| {
                if line.trim().is_empty() {
                    return None;
                }
                Some(LineIndentSnapshot {
                    line_index,
                    indent: leading_indent(line, tab_space_size),
                })
            })
            .collect();
    };

    let mut first_token_by_line: std::collections::BTreeMap<usize, usize> =
        std::collections::BTreeMap::new();
    for token in &tokens {
        if is_whitespace_token(&token.token) {
            continue;
        }
        let line = token.span.start.line as usize;
        let column = token.span.start.column as usize;
        first_token_by_line.entry(line).or_insert(column);
    }

    first_token_by_line
        .into_iter()
        .filter_map(|(line, column)| {
            let line_start = line_col_to_offset(sql, line, 1)?;
            let token_start = line_col_to_offset(sql, line, column)?;
            let leading = &sql[line_start..token_start];
            Some(LineIndentSnapshot {
                line_index: line.saturating_sub(1),
                indent: leading_indent_from_prefix(leading, tab_space_size),
            })
        })
        .collect()
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    start_line: usize,
}

fn tokenize_with_locations(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn tokenize_with_offsets_for_context(ctx: &LintContext) -> Option<Vec<LocatedToken>> {
    ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        Some(
            tokens
                .iter()
                .filter_map(|token| {
                    token_with_span_offsets(ctx.sql, token).map(|(start, _end)| LocatedToken {
                        token: token.token.clone(),
                        start,
                        start_line: token.span.start.line as usize,
                    })
                })
                .collect::<Vec<_>>(),
        )
    })
}

fn is_whitespace_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
    )
}

fn leading_indent(line: &str, tab_space_size: usize) -> LeadingIndent {
    leading_indent_from_prefix(line, tab_space_size)
}

fn leading_indent_from_prefix(prefix: &str, tab_space_size: usize) -> LeadingIndent {
    let mut width = 0usize;
    let mut space_count = 0usize;
    let mut tab_count = 0usize;

    for ch in prefix.chars() {
        match ch {
            ' ' => {
                space_count += 1;
                width += 1;
            }
            '\t' => {
                tab_count += 1;
                width += tab_space_size;
            }
            _ => break,
        }
    }

    LeadingIndent {
        width,
        space_count,
        tab_count,
        has_mixed_indent_chars: space_count > 0 && tab_count > 0,
    }
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

fn offset_to_line(sql: &str, offset: usize) -> usize {
    1 + sql[..offset.min(sql.len())]
        .chars()
        .filter(|ch| *ch == '\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        run_with_config(sql, LintConfig::default())
    }

    fn run_with_config(sql: &str, config: LintConfig) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutIndent::from_config(&config);
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check(
                    statement,
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
    fn flags_odd_indent_width() {
        let issues = run("SELECT a\n   , b\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn flags_first_line_indentation() {
        let issues = run("   SELECT 1");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn does_not_flag_even_indent_width() {
        assert!(run("SELECT a\n    , b\nFROM t").is_empty());
    }

    #[test]
    fn flags_mixed_tab_and_space_indentation() {
        let issues = run("SELECT a\n \t, b\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn tab_space_size_config_is_applied_for_tab_indentation_width() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.indent".to_string(),
                serde_json::json!({"tab_space_size": 2, "indent_unit": "tab"}),
            )]),
        };
        let issues = run_with_config("SELECT a\n\t, b\nFROM t", config);
        assert!(issues.is_empty());
    }

    #[test]
    fn tab_indent_unit_disallows_space_indent() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.indent".to_string(),
                serde_json::json!({"indent_unit": "tab"}),
            )]),
        };
        let issues = run_with_config("SELECT a\n    , b\nFROM t", config);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn indentation_section_options_are_supported() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "indentation".to_string(),
                serde_json::json!({"indent_unit": "tab", "tab_space_size": 2}),
            )]),
        };
        let issues = run_with_config("SELECT a\n\t, b\nFROM t", config);
        assert!(issues.is_empty());
    }

    #[test]
    fn indentation_on_comment_line_is_checked() {
        let issues = run("SELECT 1\n   -- comment\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }
}
