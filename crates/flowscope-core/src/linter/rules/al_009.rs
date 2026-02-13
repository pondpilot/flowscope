//! LINT_AL_009: Self alias column.
//!
//! SQLFluff AL09 parity: avoid aliasing a column to its own name.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{Expr, Ident, SelectItem, Statement};

use super::semantic_helpers::visit_selects_in_statement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AliasCaseCheck {
    Dialect,
    CaseInsensitive,
    QuotedCsNakedUpper,
    QuotedCsNakedLower,
    CaseSensitive,
}

impl AliasCaseCheck {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_AL_009, "alias_case_check")
            .unwrap_or("dialect")
            .to_ascii_lowercase()
            .as_str()
        {
            "case_insensitive" => Self::CaseInsensitive,
            "quoted_cs_naked_upper" => Self::QuotedCsNakedUpper,
            "quoted_cs_naked_lower" => Self::QuotedCsNakedLower,
            "case_sensitive" => Self::CaseSensitive,
            _ => Self::Dialect,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct NameRef<'a> {
    name: &'a str,
    quoted: bool,
}

pub struct AliasingSelfAliasColumn {
    alias_case_check: AliasCaseCheck,
}

impl AliasingSelfAliasColumn {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            alias_case_check: AliasCaseCheck::from_config(config),
        }
    }
}

impl Default for AliasingSelfAliasColumn {
    fn default() -> Self {
        Self {
            alias_case_check: AliasCaseCheck::Dialect,
        }
    }
}

impl LintRule for AliasingSelfAliasColumn {
    fn code(&self) -> &'static str {
        issue_codes::LINT_AL_009
    }

    fn name(&self) -> &'static str {
        "Self alias column"
    }

    fn description(&self) -> &'static str {
        "Column aliases should not alias to itself."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut violations = 0usize;

        visit_selects_in_statement(statement, &mut |select| {
            for item in &select.projection {
                let SelectItem::ExprWithAlias { expr, alias } = item else {
                    continue;
                };

                if aliases_expression_to_itself(expr, alias, self.alias_case_check) {
                    violations += 1;
                }
            }
        });

        (0..violations)
            .map(|_| {
                Issue::info(
                    issue_codes::LINT_AL_009,
                    "Column aliases should not alias to itself.",
                )
                .with_statement(ctx.statement_index)
            })
            .collect()
    }
}

fn aliases_expression_to_itself(
    expr: &Expr,
    alias: &Ident,
    alias_case_check: AliasCaseCheck,
) -> bool {
    let Some(source_name) = expression_name(expr) else {
        return false;
    };

    let alias_name = NameRef {
        name: alias.value.as_str(),
        quoted: alias.quote_style.is_some(),
    };

    names_match(source_name, alias_name, alias_case_check)
}

fn expression_name(expr: &Expr) -> Option<NameRef<'_>> {
    match expr {
        Expr::Identifier(identifier) => Some(NameRef {
            name: identifier.value.as_str(),
            quoted: identifier.quote_style.is_some(),
        }),
        Expr::CompoundIdentifier(parts) => parts.last().map(|part| NameRef {
            name: part.value.as_str(),
            quoted: part.quote_style.is_some(),
        }),
        Expr::Nested(inner) => expression_name(inner),
        _ => None,
    }
}

fn names_match(left: NameRef<'_>, right: NameRef<'_>, alias_case_check: AliasCaseCheck) -> bool {
    match alias_case_check {
        AliasCaseCheck::CaseInsensitive => left.name.eq_ignore_ascii_case(right.name),
        AliasCaseCheck::CaseSensitive => left.name == right.name,
        AliasCaseCheck::Dialect
        | AliasCaseCheck::QuotedCsNakedUpper
        | AliasCaseCheck::QuotedCsNakedLower => {
            if left.quoted || right.quoted {
                left.name == right.name
            } else {
                left.name.eq_ignore_ascii_case(right.name)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingSelfAliasColumn::default();
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
    fn flags_plain_self_alias() {
        let issues = run("SELECT a AS a FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_AL_009);
    }

    #[test]
    fn flags_qualified_self_alias() {
        let issues = run("SELECT t.a AS a FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn flags_case_insensitive_self_alias() {
        let issues = run("SELECT a AS A FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_alias_name() {
        let issues = run("SELECT a AS b FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn does_not_flag_non_identifier_expression() {
        let issues = run("SELECT a + 1 AS a FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn default_dialect_mode_does_not_flag_quoted_case_mismatch() {
        let issues = run("SELECT \"A\" AS a FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn default_dialect_mode_flags_exact_quoted_match() {
        let issues = run("SELECT \"A\" AS \"A\" FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn alias_case_check_case_sensitive_respects_case() {
        let sql = "SELECT a AS A FROM t";
        let statements = parse_sql(sql).expect("parse");
        let rule = AliasingSelfAliasColumn::from_config(&LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.self_alias.column".to_string(),
                serde_json::json!({"alias_case_check": "case_sensitive"}),
            )]),
        });
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
