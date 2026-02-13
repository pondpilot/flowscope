//! LINT_AM_005: Ambiguous JOIN style.
//!
//! Require explicit JOIN type keywords (`INNER`, `LEFT`, etc.) instead of bare
//! `JOIN` for clearer intent.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{JoinOperator, Statement};
use sqlparser::dialect::GenericDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

use super::semantic_helpers::visit_selects_in_statement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FullyQualifyJoinTypes {
    Inner,
    Outer,
    Both,
}

impl FullyQualifyJoinTypes {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_AM_005, "fully_qualify_join_types")
            .unwrap_or("inner")
            .to_ascii_lowercase()
            .as_str()
        {
            "outer" => Self::Outer,
            "both" => Self::Both,
            _ => Self::Inner,
        }
    }
}

pub struct AmbiguousJoinStyle {
    qualify_mode: FullyQualifyJoinTypes,
}

impl AmbiguousJoinStyle {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            qualify_mode: FullyQualifyJoinTypes::from_config(config),
        }
    }
}

impl Default for AmbiguousJoinStyle {
    fn default() -> Self {
        Self {
            qualify_mode: FullyQualifyJoinTypes::Inner,
        }
    }
}

impl LintRule for AmbiguousJoinStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AM_005
    }

    fn name(&self) -> &'static str {
        "Ambiguous join style"
    }

    fn description(&self) -> &'static str {
        "Join clauses should be fully qualified."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut plain_join_count = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            for table in &select.from {
                for join in &table.joins {
                    if matches!(join.join_operator, JoinOperator::Join(_)) {
                        plain_join_count += 1;
                    }
                }
            }
        });

        let outer_unqualified_count = count_unqualified_outer_joins(ctx.statement_sql());
        let violation_count = match self.qualify_mode {
            FullyQualifyJoinTypes::Inner => plain_join_count,
            FullyQualifyJoinTypes::Outer => outer_unqualified_count,
            FullyQualifyJoinTypes::Both => plain_join_count + outer_unqualified_count,
        };

        (0..violation_count)
            .map(|_| {
                Issue::warning(
                    issue_codes::LINT_AM_005,
                    "Join clauses should be fully qualified.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn count_unqualified_outer_joins(sql: &str) -> usize {
    let dialect = GenericDialect {};
    let mut tokenizer = Tokenizer::new(&dialect, sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return 0;
    };

    let significant: Vec<&Token> = tokens.iter().filter(|token| !is_trivia(token)).collect();

    let mut count = 0usize;
    let mut idx = 0usize;
    while idx < significant.len() {
        let Token::Word(word) = significant[idx] else {
            idx += 1;
            continue;
        };

        if !matches!(word.keyword, Keyword::LEFT | Keyword::RIGHT | Keyword::FULL) {
            idx += 1;
            continue;
        }

        let Some(next) = significant.get(idx + 1) else {
            break;
        };

        match next {
            Token::Word(next_word) if next_word.keyword == Keyword::OUTER => {
                idx += 3;
            }
            Token::Word(next_word) if next_word.keyword == Keyword::JOIN => {
                count += 1;
                idx += 2;
            }
            _ => idx += 1,
        }
    }

    count
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

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AmbiguousJoinStyle::default();
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

    // --- Edge cases adopted from sqlfluff AM05 ---

    #[test]
    fn flags_plain_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo JOIN bar");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AM_005);
    }

    #[test]
    fn flags_lowercase_plain_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo join bar");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn allows_inner_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo INNER JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_left_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo LEFT JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_right_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo RIGHT JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_full_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo FULL JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_left_outer_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo LEFT OUTER JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_right_outer_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo RIGHT OUTER JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_full_outer_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo FULL OUTER JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn allows_cross_join() {
        let issues = run("SELECT foo.a, bar.b FROM foo CROSS JOIN bar");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_each_plain_join_in_chain() {
        let issues = run("SELECT * FROM a JOIN b ON a.id = b.id JOIN c ON b.id = c.id");
        assert_eq!(issues.len(), 2);
        assert!(issues
            .iter()
            .all(|issue| issue.code == issue_codes::LINT_AM_005));
    }

    #[test]
    fn outer_mode_flags_left_join_without_outer_keyword() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "ambiguous.join".to_string(),
                serde_json::json!({"fully_qualify_join_types": "outer"}),
            )]),
        };
        let rule = AmbiguousJoinStyle::from_config(&config);
        let sql = "SELECT foo.a, bar.b FROM foo LEFT JOIN bar ON foo.id = bar.id";
        let statements = parse_sql(sql).expect("parse");
        let issues = rule.check(
            &statements[0],
            &LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn outer_mode_allows_left_outer_join() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_AM_005".to_string(),
                serde_json::json!({"fully_qualify_join_types": "outer"}),
            )]),
        };
        let rule = AmbiguousJoinStyle::from_config(&config);
        let sql = "SELECT foo.a, bar.b FROM foo LEFT OUTER JOIN bar ON foo.id = bar.id";
        let statements = parse_sql(sql).expect("parse");
        let issues = rule.check(
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
