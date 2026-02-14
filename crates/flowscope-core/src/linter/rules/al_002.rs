//! LINT_AL_002: Column alias style.
//!
//! SQLFluff parity: configurable column aliasing style (`explicit`/`implicit`).

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::{Ident, SelectItem, Statement};
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};
use std::collections::HashMap;

use super::semantic_helpers::visit_selects_in_statement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AliasingPreference {
    Explicit,
    Implicit,
}

impl AliasingPreference {
    fn from_config(config: &LintConfig, rule_code: &str) -> Self {
        match config
            .rule_option_str(rule_code, "aliasing")
            .unwrap_or("explicit")
            .to_ascii_lowercase()
            .as_str()
        {
            "implicit" => Self::Implicit,
            _ => Self::Explicit,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Explicit => "Use explicit AS when aliasing columns.",
            Self::Implicit => "Use implicit aliasing when aliasing columns (omit AS).",
        }
    }

    fn violation(self, explicit_as: bool) -> bool {
        match self {
            Self::Explicit => !explicit_as,
            Self::Implicit => explicit_as,
        }
    }
}

pub struct AliasingColumnStyle {
    aliasing: AliasingPreference,
}

impl AliasingColumnStyle {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            aliasing: AliasingPreference::from_config(config, issue_codes::LINT_AL_002),
        }
    }
}

impl Default for AliasingColumnStyle {
    fn default() -> Self {
        Self {
            aliasing: AliasingPreference::Explicit,
        }
    }
}

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

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let alias_style = alias_style_index(ctx.statement_sql(), ctx.dialect());
        let mut issues = Vec::new();

        visit_selects_in_statement(statement, &mut |select| {
            for item in &select.projection {
                let SelectItem::ExprWithAlias { alias, .. } = item else {
                    continue;
                };

                let Some(occurrence) = alias_occurrence_in_statement(alias, ctx, &alias_style)
                else {
                    continue;
                };

                if occurrence.tsql_equals_assignment {
                    // TSQL supports `SELECT alias = expr`, which SQLFluff excludes from AL02.
                    continue;
                }

                if !self.aliasing.violation(occurrence.explicit_as) {
                    continue;
                }

                issues.push(
                    Issue::info(issue_codes::LINT_AL_002, self.aliasing.message())
                        .with_statement(ctx.statement_index)
                        .with_span(
                            ctx.span_from_statement_offset(occurrence.start, occurrence.end),
                        ),
                );
            }
        });

        issues
    }
}

#[derive(Clone, Copy)]
struct AliasOccurrence {
    start: usize,
    end: usize,
    explicit_as: bool,
    tsql_equals_assignment: bool,
}

fn alias_occurrence_in_statement(
    alias: &Ident,
    ctx: &LintContext,
    style_index: &HashMap<usize, AliasOccurrence>,
) -> Option<AliasOccurrence> {
    let abs_start = line_col_to_offset(
        ctx.sql,
        alias.span.start.line as usize,
        alias.span.start.column as usize,
    )?;
    let abs_end = line_col_to_offset(
        ctx.sql,
        alias.span.end.line as usize,
        alias.span.end.column as usize,
    )?;

    if abs_start < ctx.statement_range.start || abs_end > ctx.statement_range.end {
        return None;
    }

    let rel_start = abs_start - ctx.statement_range.start;
    let rel_end = abs_end - ctx.statement_range.start;

    let occurrence = style_index.get(&rel_start)?;
    (occurrence.end == rel_end).then_some(*occurrence)
}

fn alias_style_index(sql: &str, dialect: Dialect) -> HashMap<usize, AliasOccurrence> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return HashMap::new();
    };

    let significant: Vec<&TokenWithSpan> = tokens
        .iter()
        .filter(|token| !is_trivia_token(&token.token))
        .collect();

    let mut styles = HashMap::new();

    for (index, token) in significant.iter().enumerate() {
        let Token::Word(_) = token.token else {
            continue;
        };

        let Some(start) = token_start_offset(sql, token) else {
            continue;
        };
        let Some(end) = token_end_offset(sql, token) else {
            continue;
        };

        let explicit_as = index > 0
            && matches!(
                significant[index - 1].token,
                Token::Word(ref word) if word.keyword == Keyword::AS
            );
        let tsql_equals_assignment = matches!(
            significant.get(index + 1),
            Some(next) if matches!(next.token, Token::Eq)
        );

        styles.insert(
            start,
            AliasOccurrence {
                start,
                end,
                explicit_as,
                tsql_equals_assignment,
            },
        );
    }

    styles
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
    use crate::{
        parser::{parse_sql, parse_sql_with_dialect},
        Dialect,
    };

    fn run_with_rule(sql: &str, rule: AliasingColumnStyle) -> Vec<Issue> {
        let stmts = parse_sql(sql).expect("parse");
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

    fn run(sql: &str) -> Vec<Issue> {
        run_with_rule(sql, AliasingColumnStyle::default())
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

    #[test]
    fn flags_explicit_aliases_when_implicit_policy_requested() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.column".to_string(),
                serde_json::json!({"aliasing": "implicit"}),
            )]),
        };
        let issues = run_with_rule(
            "select a + 1 as total, b + 1 value from t",
            AliasingColumnStyle::from_config(&config),
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_002);
    }

    #[test]
    fn does_not_flag_alias_text_in_string_literal() {
        let issues = run("select 'a as label' as value from t");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_tsql_assignment_style_alias() {
        let sql = "select alias1 = col1";
        let statements = parse_sql_with_dialect(sql, Dialect::Mssql).expect("parse");
        let issues = AliasingColumnStyle::default().check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert!(issues.is_empty());
    }
}
