//! LINT_LT_002: Layout indent.
//!
//! SQLFluff LT02 parity (current scope): flag odd indentation widths on
//! subsequent lines.

use crate::linter::config::LintConfig;
use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use sqlparser::ast::Statement;

pub struct LayoutIndent {
    indent_unit: usize,
    tab_space_size: usize,
}

impl LayoutIndent {
    pub fn from_config(config: &LintConfig) -> Self {
        let tab_space_size = config
            .rule_option_usize(issue_codes::LINT_LT_002, "tab_space_size")
            .unwrap_or(4)
            .max(1);
        let indent_unit = config
            .rule_option_usize(issue_codes::LINT_LT_002, "indent_unit")
            .unwrap_or(2)
            .max(1);

        Self {
            indent_unit,
            tab_space_size,
        }
    }
}

impl Default for LayoutIndent {
    fn default() -> Self {
        Self {
            indent_unit: 2,
            tab_space_size: 4,
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
        let sql = ctx.statement_sql();
        let has_violation = sql.contains('\n')
            && sql.lines().skip(1).any(|line| {
                if line.trim().is_empty() {
                    return false;
                }

                let (indent_width, has_mixed_indent_chars) =
                    leading_indent_width(line, self.tab_space_size);
                has_mixed_indent_chars || (indent_width > 0 && indent_width % self.indent_unit != 0)
            });

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

fn leading_indent_width(line: &str, tab_space_size: usize) -> (usize, bool) {
    let mut width = 0usize;
    let mut saw_space = false;
    let mut saw_tab = false;

    for ch in line.chars() {
        match ch {
            ' ' => {
                saw_space = true;
                width += 1;
            }
            '\t' => {
                saw_tab = true;
                width += tab_space_size;
            }
            _ => break,
        }
    }

    (width, saw_space && saw_tab)
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
                serde_json::json!({"tab_space_size": 2, "indent_unit": 2}),
            )]),
        };
        let issues = run_with_config("SELECT a\n\t, b\nFROM t", config);
        assert!(issues.is_empty());
    }
}
