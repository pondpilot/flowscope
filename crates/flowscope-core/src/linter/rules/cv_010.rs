//! LINT_CV_010: Quoted literals style.
//!
//! SQLFluff CV10 parity (current scope): detect double-quoted literal-like
//! segments.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit::visit_expressions;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Statement, Value};

use super::references_quoted_helpers::double_quoted_identifiers_in_statement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreferredQuotedLiteralStyle {
    Consistent,
    SingleQuotes,
    DoubleQuotes,
}

impl PreferredQuotedLiteralStyle {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_CV_010, "preferred_quoted_literal_style")
            .unwrap_or("consistent")
            .to_ascii_lowercase()
            .as_str()
        {
            "single_quotes" | "single" => Self::SingleQuotes,
            "double_quotes" | "double" => Self::DoubleQuotes,
            _ => Self::Consistent,
        }
    }
}

pub struct ConventionQuotedLiterals {
    preferred_style: PreferredQuotedLiteralStyle,
}

impl ConventionQuotedLiterals {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            preferred_style: PreferredQuotedLiteralStyle::from_config(config),
        }
    }
}

impl Default for ConventionQuotedLiterals {
    fn default() -> Self {
        Self {
            preferred_style: PreferredQuotedLiteralStyle::Consistent,
        }
    }
}

impl LintRule for ConventionQuotedLiterals {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_010
    }

    fn name(&self) -> &'static str {
        "Quoted literals style"
    }

    fn description(&self) -> &'static str {
        "Quoted literal style is inconsistent with SQL convention."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let has_double_quoted = !double_quoted_identifiers_in_statement(statement).is_empty();
        let has_single_quoted = statement_contains_single_quoted_literal(statement);

        let violation = match self.preferred_style {
            PreferredQuotedLiteralStyle::Consistent => has_double_quoted,
            PreferredQuotedLiteralStyle::SingleQuotes => has_double_quoted,
            PreferredQuotedLiteralStyle::DoubleQuotes => has_single_quoted,
        };

        if violation {
            let message = match self.preferred_style {
                PreferredQuotedLiteralStyle::Consistent => {
                    "Quoted literal style appears inconsistent."
                }
                PreferredQuotedLiteralStyle::SingleQuotes => {
                    "Use single quotes for quoted literals."
                }
                PreferredQuotedLiteralStyle::DoubleQuotes => {
                    "Use double quotes for quoted literals."
                }
            };
            vec![Issue::info(issue_codes::LINT_CV_010, message).with_statement(ctx.statement_index)]
        } else {
            Vec::new()
        }
    }
}

fn statement_contains_single_quoted_literal(statement: &Statement) -> bool {
    let mut found = false;
    visit_expressions(statement, &mut |expr| {
        if found {
            return;
        }
        if let sqlparser::ast::Expr::Value(value) = expr {
            found = matches!(
                value.value,
                Value::SingleQuotedString(_)
                    | Value::DollarQuotedString(_)
                    | Value::NationalStringLiteral(_)
                    | Value::EscapedStringLiteral(_)
            );
        }
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionQuotedLiterals::default();
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
    fn flags_double_quoted_literal_like_token() {
        let issues = run("SELECT \"abc\" FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_010);
    }

    #[test]
    fn does_not_flag_single_quoted_literal() {
        assert!(run("SELECT 'abc' FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_double_quotes_inside_single_quoted_literal() {
        assert!(run("SELECT '\"abc\"' FROM t").is_empty());
    }

    #[test]
    fn single_quotes_preference_flags_double_quoted_identifier_usage() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.quoted_literals".to_string(),
                serde_json::json!({"preferred_quoted_literal_style": "single_quotes"}),
            )]),
        };
        let rule = ConventionQuotedLiterals::from_config(&config);
        let sql = "SELECT \"abc\" FROM t";
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
    fn double_quotes_preference_flags_single_quoted_literal_usage() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_010".to_string(),
                serde_json::json!({"preferred_quoted_literal_style": "double_quotes"}),
            )]),
        };
        let rule = ConventionQuotedLiterals::from_config(&config);
        let sql = "SELECT 'abc' FROM t";
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
}
