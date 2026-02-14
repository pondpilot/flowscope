//! LINT_CV_011: Casting style.
//!
//! SQLFluff CV11 parity (current scope): detect mixed use of `::` and `CAST()`
//! within the same statement.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit::visit_expressions;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::{CastKind, Expr, Statement};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreferredTypeCastingStyle {
    Consistent,
    Shorthand,
    Cast,
    Convert,
}

impl PreferredTypeCastingStyle {
    fn from_config(config: &LintConfig) -> Self {
        match config
            .rule_option_str(issue_codes::LINT_CV_011, "preferred_type_casting_style")
            .unwrap_or("consistent")
            .to_ascii_lowercase()
            .as_str()
        {
            "shorthand" => Self::Shorthand,
            "cast" => Self::Cast,
            "convert" => Self::Convert,
            _ => Self::Consistent,
        }
    }

    fn violation(self, usage: &CastStyleUsage) -> bool {
        match self {
            Self::Consistent => usage.style_count() > 1,
            Self::Shorthand => usage.saw_function_style_cast || usage.saw_convert_style_cast,
            Self::Cast => usage.saw_double_colon_cast || usage.saw_convert_style_cast,
            Self::Convert => usage.saw_function_style_cast || usage.saw_double_colon_cast,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Consistent => "Use consistent casting style (avoid mixing CAST styles).",
            Self::Shorthand => "Use `::` shorthand casting style.",
            Self::Cast => "Use `CAST(...)` style casts.",
            Self::Convert => "Use `CONVERT(...)` style casts.",
        }
    }
}

pub struct ConventionCastingStyle {
    preferred_style: PreferredTypeCastingStyle,
}

impl ConventionCastingStyle {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            preferred_style: PreferredTypeCastingStyle::from_config(config),
        }
    }
}

impl Default for ConventionCastingStyle {
    fn default() -> Self {
        Self {
            preferred_style: PreferredTypeCastingStyle::Consistent,
        }
    }
}

impl LintRule for ConventionCastingStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_011
    }

    fn name(&self) -> &'static str {
        "Casting style"
    }

    fn description(&self) -> &'static str {
        "Use consistent casting style."
    }

    fn check(&self, statement: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if self
            .preferred_style
            .violation(&statement_cast_style_usage(statement))
        {
            vec![
                Issue::info(issue_codes::LINT_CV_011, self.preferred_style.message())
                    .with_statement(ctx.statement_index),
            ]
        } else {
            Vec::new()
        }
    }
}

#[derive(Default)]
struct CastStyleUsage {
    saw_function_style_cast: bool,
    saw_double_colon_cast: bool,
    saw_convert_style_cast: bool,
}

impl CastStyleUsage {
    fn style_count(&self) -> usize {
        usize::from(self.saw_function_style_cast)
            + usize::from(self.saw_double_colon_cast)
            + usize::from(self.saw_convert_style_cast)
    }
}

fn statement_cast_style_usage(statement: &Statement) -> CastStyleUsage {
    let mut usage = CastStyleUsage::default();

    visit_expressions(statement, &mut |expr| {
        if usage.style_count() > 1 {
            return;
        }

        match expr {
            Expr::Cast { kind, .. } => match kind {
                CastKind::DoubleColon => usage.saw_double_colon_cast = true,
                CastKind::Cast | CastKind::TryCast | CastKind::SafeCast => {
                    usage.saw_function_style_cast = true
                }
            },
            Expr::Function(function)
                if function.name.to_string().eq_ignore_ascii_case("CONVERT") =>
            {
                usage.saw_convert_style_cast = true;
            }
            _ => {}
        }
    });

    usage
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = ConventionCastingStyle::default();
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
    fn flags_mixed_casting_styles() {
        let issues = run("SELECT CAST(amount AS INT)::TEXT FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_011);
    }

    #[test]
    fn does_not_flag_single_casting_style() {
        assert!(run("SELECT amount::INT FROM t").is_empty());
        assert!(run("SELECT CAST(amount AS INT) FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_cast_like_tokens_inside_string_literal() {
        assert!(run("SELECT 'value::TEXT and CAST(value AS INT)' AS note").is_empty());
    }

    #[test]
    fn flags_mixed_try_cast_and_double_colon_styles() {
        let issues = run("SELECT TRY_CAST(amount AS INT)::TEXT FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CV_011);
    }

    #[test]
    fn shorthand_preference_flags_cast_function_style() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.casting_style".to_string(),
                serde_json::json!({"preferred_type_casting_style": "shorthand"}),
            )]),
        };
        let rule = ConventionCastingStyle::from_config(&config);
        let sql = "SELECT CAST(amount AS INT) FROM t";
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
    fn cast_preference_flags_shorthand_style() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_011".to_string(),
                serde_json::json!({"preferred_type_casting_style": "cast"}),
            )]),
        };
        let rule = ConventionCastingStyle::from_config(&config);
        let sql = "SELECT amount::INT FROM t";
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
