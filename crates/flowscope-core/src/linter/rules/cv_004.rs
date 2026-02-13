//! LINT_CV_004: Prefer COUNT(*) over COUNT(1).
//!
//! `COUNT(1)` and `COUNT(*)` are semantically identical in all major databases,
//! but `COUNT(*)` is the standard convention and more clearly expresses intent.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::linter::visit;
use crate::types::{issue_codes, Issue};
use sqlparser::ast::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CountPreference {
    Star,
    One,
    Zero,
}

impl CountPreference {
    fn from_config(config: &LintConfig) -> Self {
        let prefer_one = config
            .rule_option_bool(issue_codes::LINT_CV_004, "prefer_count_1")
            .unwrap_or(false);
        let prefer_zero = config
            .rule_option_bool(issue_codes::LINT_CV_004, "prefer_count_0")
            .unwrap_or(false);

        if prefer_one {
            Self::One
        } else if prefer_zero {
            Self::Zero
        } else {
            Self::Star
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Star => "Use COUNT(*) for row counts.",
            Self::One => "Use COUNT(1) for row counts.",
            Self::Zero => "Use COUNT(0) for row counts.",
        }
    }

    fn violates(self, kind: CountArgKind) -> bool {
        match self {
            Self::Star => matches!(kind, CountArgKind::One | CountArgKind::Zero),
            Self::One => matches!(kind, CountArgKind::Star | CountArgKind::Zero),
            Self::Zero => matches!(kind, CountArgKind::Star | CountArgKind::One),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CountArgKind {
    Star,
    One,
    Zero,
    Other,
}

pub struct CountStyle {
    preference: CountPreference,
}

impl CountStyle {
    pub fn from_config(config: &LintConfig) -> Self {
        Self {
            preference: CountPreference::from_config(config),
        }
    }
}

impl Default for CountStyle {
    fn default() -> Self {
        Self {
            preference: CountPreference::Star,
        }
    }
}

impl LintRule for CountStyle {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CV_004
    }

    fn name(&self) -> &'static str {
        "COUNT style"
    }

    fn description(&self) -> &'static str {
        "Prefer COUNT(*) over COUNT(1) for clarity."
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        visit::visit_expressions(stmt, &mut |expr| {
            if let Expr::Function(func) = expr {
                let fname = func.name.to_string().to_uppercase();
                if fname == "COUNT" && self.preference.violates(count_argument_kind(&func.args)) {
                    issues.push(
                        Issue::info(issue_codes::LINT_CV_004, self.preference.message())
                            .with_statement(ctx.statement_index),
                    );
                }
            }
        });
        issues
    }
}

fn count_argument_kind(args: &FunctionArguments) -> CountArgKind {
    let arg_list = match args {
        FunctionArguments::List(list) => list,
        _ => return CountArgKind::Other,
    };

    if arg_list.args.len() != 1 {
        return CountArgKind::Other;
    }

    match &arg_list.args[0] {
        FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => CountArgKind::Star,
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(ValueWithSpan {
            value: Value::Number(n, _),
            ..
        }))) if n == "1" => CountArgKind::One,
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(ValueWithSpan {
            value: Value::Number(n, _),
            ..
        }))) if n == "0" => CountArgKind::Zero,
        _ => CountArgKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    fn check_sql(sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).unwrap();
        let rule = CountStyle::default();
        let ctx = LintContext {
            sql,
            statement_range: 0..sql.len(),
            statement_index: 0,
        };
        let mut issues = Vec::new();
        for stmt in &stmts {
            issues.extend(rule.check(stmt, &ctx));
        }
        issues
    }

    #[test]
    fn test_count_one_detected() {
        let issues = check_sql("SELECT COUNT(1) FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "LINT_CV_004");
    }

    #[test]
    fn test_count_star_ok() {
        let issues = check_sql("SELECT COUNT(*) FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_count_column_ok() {
        let issues = check_sql("SELECT COUNT(id) FROM t");
        assert!(issues.is_empty());
    }

    // --- Edge cases ---

    #[test]
    fn test_count_zero_detected_with_default_star_preference() {
        let issues = check_sql("SELECT COUNT(0) FROM t");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_count_one_in_having() {
        let issues = check_sql("SELECT col FROM t GROUP BY col HAVING COUNT(1) > 5");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_count_one_in_subquery() {
        let issues =
            check_sql("SELECT * FROM t WHERE id IN (SELECT COUNT(1) FROM t2 GROUP BY col)");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_multiple_count_one() {
        let issues = check_sql("SELECT COUNT(1), COUNT(1) FROM t");
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_count_distinct_ok() {
        let issues = check_sql("SELECT COUNT(DISTINCT id) FROM t");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_count_one_in_cte() {
        let issues = check_sql("WITH cte AS (SELECT COUNT(1) AS cnt FROM t) SELECT * FROM cte");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_count_one_in_qualify() {
        let issues = check_sql("SELECT a FROM t QUALIFY COUNT(1) > 0");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_prefer_count_one_flags_count_star() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.count_rows".to_string(),
                serde_json::json!({"prefer_count_1": true}),
            )]),
        };
        let rule = CountStyle::from_config(&config);
        let stmts = parse_sql("SELECT COUNT(*) FROM t").unwrap();
        let issues = rule.check(
            &stmts[0],
            &LintContext {
                sql: "SELECT COUNT(*) FROM t",
                statement_range: 0.."SELECT COUNT(*) FROM t".len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_prefer_count_zero_flags_count_one() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_004".to_string(),
                serde_json::json!({"prefer_count_0": true}),
            )]),
        };
        let rule = CountStyle::from_config(&config);
        let stmts = parse_sql("SELECT COUNT(1) FROM t").unwrap();
        let issues = rule.check(
            &stmts[0],
            &LintContext {
                sql: "SELECT COUNT(1) FROM t",
                statement_range: 0.."SELECT COUNT(1) FROM t".len(),
                statement_index: 0,
            },
        );
        assert_eq!(issues.len(), 1);
    }
}
