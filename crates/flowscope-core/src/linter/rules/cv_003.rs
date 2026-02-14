//! LINT_CV_003: Select trailing comma.
//!
//! Avoid trailing comma before FROM.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Dialect, Issue};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectClauseTrailingCommaPolicy {
    Forbid,
    Require,
}

impl SelectClauseTrailingCommaPolicy {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_CV_003, "select_clause_trailing_comma")
            .unwrap_or("forbid")
            .to_ascii_lowercase()
            .as_str()
        {
            "require" => Self::Require,
            _ => Self::Forbid,
        }
    }

    fn violated(self, trailing_comma_present: bool) -> bool {
        match self {
            Self::Forbid => trailing_comma_present,
            Self::Require => !trailing_comma_present,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Forbid => "Avoid trailing comma before FROM in SELECT clause.",
            Self::Require => "Use trailing comma before FROM in SELECT clause.",
        }
    }
}

pub struct ConventionSelectTrailingComma {
    policy: SelectClauseTrailingCommaPolicy,
}

impl ConventionSelectTrailingComma {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            policy: SelectClauseTrailingCommaPolicy::from_config(config),
        }
    }
}

impl Default for ConventionSelectTrailingComma {
    fn default() -> Self {
        Self {
            policy: SelectClauseTrailingCommaPolicy::Forbid,
        }
    }
}

impl LintRule for ConventionSelectTrailingComma {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_003
    }

    fn name(&self) -> &'static str {
        "Select trailing comma"
    }

    fn description(&self) -> &'static str {
        "Trailing commas within select clause."
    }

    fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if has_select_trailing_comma_violation(ctx.statement_sql(), self.policy, ctx.dialect()) {
            vec![
                Issue::warning(issue_codes::LINT_CV_003, self.policy.message())
                    .with_statement(ctx.statement_index),
            ]
        } else {
            Vec::new()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SignificantToken {
    Comma,
    Other,
}

#[derive(Clone, Copy)]
struct SelectClauseState {
    depth: usize,
    last_significant: Option<SignificantToken>,
}

fn has_select_trailing_comma_violation(
    sql: &str,
    policy: SelectClauseTrailingCommaPolicy,
    dialect: Dialect,
) -> bool {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let Ok(tokens) = tokenizer.tokenize() else {
        return false;
    };

    let mut depth = 0usize;
    let mut select_stack: Vec<SelectClauseState> = Vec::new();

    for token in tokens {
        if is_trivia_token(&token) {
            continue;
        }

        let significant = match token {
            Token::Comma => Some(SignificantToken::Comma),
            _ => Some(SignificantToken::Other),
        };

        if let Token::Word(ref word) = token {
            if word.keyword == Keyword::SELECT {
                select_stack.push(SelectClauseState {
                    depth,
                    last_significant: None,
                });
                continue;
            }
        }

        if should_terminate_select_clause(&token, depth, select_stack.last()) {
            if let Some(state) = select_stack.pop() {
                if policy.violated(state.last_significant == Some(SignificantToken::Comma)) {
                    return true;
                }
            }
            continue;
        }

        if let Some(state) = select_stack.last_mut() {
            if state.depth == depth {
                state.last_significant = significant;
            }
        }

        match token {
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            _ => {}
        }

        while let Some(state) = select_stack.last().copied() {
            if state.depth > depth {
                let Some(ended) = select_stack.pop() else {
                    break;
                };
                if policy.violated(ended.last_significant == Some(SignificantToken::Comma)) {
                    return true;
                }
            } else {
                break;
            }
        }
    }

    while let Some(state) = select_stack.pop() {
        if policy.violated(state.last_significant == Some(SignificantToken::Comma)) {
            return true;
        }
    }

    false
}

fn should_terminate_select_clause(
    token: &Token,
    depth: usize,
    state: Option<&SelectClauseState>,
) -> bool {
    let Some(state) = state else {
        return false;
    };
    if state.depth != depth {
        return false;
    }

    match token {
        Token::Word(word) => is_select_clause_terminator(word.keyword),
        Token::RParen | Token::SemiColon => true,
        _ => false,
    }
}

fn is_select_clause_terminator(keyword: Keyword) -> bool {
    matches!(
        keyword,
        Keyword::FROM
            | Keyword::INTO
            | Keyword::WHERE
            | Keyword::GROUP
            | Keyword::HAVING
            | Keyword::ORDER
            | Keyword::LIMIT
            | Keyword::OFFSET
            | Keyword::FETCH
            | Keyword::QUALIFY
            | Keyword::WINDOW
            | Keyword::FOR
            | Keyword::UNION
            | Keyword::EXCEPT
            | Keyword::INTERSECT
    )
}

fn is_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        run_with_rule(sql, ConventionSelectTrailingComma::default())
    }

    fn run_with_rule(sql: &str, rule: ConventionSelectTrailingComma) -> Vec<Issue> {
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

    #[test]
    fn flags_trailing_comma_before_from() {
        let issues = run("select a, from t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_003);
    }

    #[test]
    fn allows_no_trailing_comma() {
        let issues = run("select a from t");
        assert!(issues.is_empty());
    }

    #[test]
    fn flags_nested_select_trailing_comma() {
        let issues = run("SELECT (SELECT a, FROM t) AS x FROM outer_t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_003);
    }

    #[test]
    fn does_not_flag_comma_in_string_or_comment() {
        let issues = run("SELECT 'a, from t' AS txt -- select a, from t\nFROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn require_policy_flags_missing_trailing_comma() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.select_trailing_comma".to_string(),
                serde_json::json!({"select_clause_trailing_comma": "require"}),
            )]),
        };
        let rule = ConventionSelectTrailingComma::from_config(&config);
        let issues = run_with_rule("SELECT a FROM t", rule);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn require_policy_allows_trailing_comma() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_003".to_string(),
                serde_json::json!({"select_clause_trailing_comma": "require"}),
            )]),
        };
        let rule = ConventionSelectTrailingComma::from_config(&config);
        let issues = run_with_rule("SELECT a, FROM t", rule);
        assert!(issues.is_empty());
    }

    #[test]
    fn require_policy_flags_select_without_from() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.select_trailing_comma".to_string(),
                serde_json::json!({"select_clause_trailing_comma": "require"}),
            )]),
        };
        let rule = ConventionSelectTrailingComma::from_config(&config);
        let issues = run_with_rule("SELECT 1", rule);
        assert_eq!(issues.len(), 1);
    }
}
