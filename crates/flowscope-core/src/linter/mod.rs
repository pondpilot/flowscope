//! SQL linter module.
//!
//! Provides a modular, rule-based linting system for SQL statements.
//! Each rule implements the `LintRule` trait and checks parsed AST nodes
//! for anti-patterns, producing `Issue` objects that flow through the
//! existing analysis pipeline.

pub mod config;
pub mod helpers;
pub mod rule;
pub mod rules;
pub(crate) mod visit;

use config::LintConfig;
use rule::{LintContext, LintRule};
use sqlparser::ast::Statement;

use crate::types::Issue;

/// The SQL linter, holding a set of rules and configuration.
pub struct Linter {
    rules: Vec<Box<dyn LintRule>>,
    config: LintConfig,
}

impl Linter {
    /// Creates a new linter with the given configuration.
    pub fn new(config: LintConfig) -> Self {
        Self {
            rules: rules::all_rules(),
            config,
        }
    }

    /// Returns true if linting is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Checks a single statement against all enabled lint rules.
    pub fn check_statement(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        if !self.config.enabled {
            return Vec::new();
        }

        let mut issues = Vec::new();
        for rule in &self.rules {
            if self.config.is_rule_enabled(rule.code()) {
                issues.extend(rule.check(stmt, ctx));
            }
        }
        issues
    }
}
