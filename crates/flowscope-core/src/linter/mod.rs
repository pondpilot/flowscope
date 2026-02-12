//! SQL linter module.
//!
//! Provides a modular linting system split into semantic, lexical, and document
//! engines. Semantic checks are AST-driven, while lexical/document checks can
//! use tokenizer-level context.

pub mod config;
pub mod document;
pub mod helpers;
pub mod rule;
pub mod rules;
pub(crate) mod visit;

use config::LintConfig;
use document::{LintDocument, LintStatement};
use rule::{LintContext, LintRule};
use sqlparser::ast::Statement;

use crate::{
    types::{Issue, LintConfidence, LintEngine, LintFallbackSource, Severity},
    Dialect,
};

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

    /// Checks a full lint document across semantic, lexical, and document engines.
    pub fn check_document(&self, document: &LintDocument<'_>) -> Vec<Issue> {
        if !self.config.enabled {
            return Vec::new();
        }

        let mut issues = Vec::new();

        for engine in [
            LintEngine::Semantic,
            LintEngine::Lexical,
            LintEngine::Document,
        ] {
            for rule in &self.rules {
                if !self.config.is_rule_enabled(rule.code())
                    || rule_engine(rule.code()) != engine
                    || !rule_supported_in_dialect(rule.code(), document.dialect)
                {
                    continue;
                }

                let (confidence, fallback) = lint_quality_for_rule(rule.code(), engine, document);

                for statement in &document.statements {
                    let ctx = LintContext {
                        sql: document.sql,
                        statement_range: statement.statement_range.clone(),
                        statement_index: statement.statement_index,
                    };

                    for issue in rule.check(statement.statement, &ctx) {
                        let issue = issue
                            .with_lint_engine(engine)
                            .with_lint_confidence(confidence);

                        let issue = if let Some(source) = fallback {
                            issue.with_lint_fallback_source(source)
                        } else {
                            issue
                        };

                        issues.push(issue);
                    }
                }
            }
        }

        normalize_issues(issues)
    }

    /// Checks a single statement against all enabled lint rules.
    ///
    /// This adapter is kept for tests and rule-level helpers. Production paths
    /// should prefer `check_document()`.
    pub fn check_statement(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let document = LintDocument::new(
            ctx.sql,
            crate::Dialect::Generic,
            vec![LintStatement {
                statement: stmt,
                statement_index: ctx.statement_index,
                statement_range: ctx.statement_range.clone(),
            }],
        );
        self.check_document(&document)
    }
}

fn normalize_issues(mut issues: Vec<Issue>) -> Vec<Issue> {
    issues.sort_by(|left, right| issue_sort_key(left).cmp(&issue_sort_key(right)));
    issues.dedup_by(|left, right| {
        left.span.is_some()
            && right.span.is_some()
            && left.statement_index == right.statement_index
            && left.span == right.span
            && left.severity == right.severity
            && left.code == right.code
            && left.message == right.message
    });
    issues
}

fn issue_sort_key(issue: &Issue) -> (usize, usize, usize, u8, &str, &str) {
    (
        issue.statement_index.unwrap_or(usize::MAX),
        issue.span.map_or(usize::MAX, |span| span.start),
        issue.span.map_or(usize::MAX, |span| span.end),
        severity_rank(issue.severity),
        issue.code.as_str(),
        issue.message.as_str(),
    )
}

const fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

fn rule_engine(code: &str) -> LintEngine {
    match code {
        crate::types::issue_codes::LINT_LT_012
        | crate::types::issue_codes::LINT_LT_013
        | crate::types::issue_codes::LINT_LT_015
        | crate::types::issue_codes::LINT_ST_012 => LintEngine::Document,
        c if c.starts_with("LINT_CP_")
            || c.starts_with("LINT_JJ_")
            || c.starts_with("LINT_LT_")
            || c.starts_with("LINT_TQ_") =>
        {
            LintEngine::Lexical
        }
        _ => LintEngine::Semantic,
    }
}

fn rule_supported_in_dialect(code: &str, dialect: Dialect) -> bool {
    match code {
        crate::types::issue_codes::LINT_AM_001 => matches!(
            dialect,
            Dialect::Generic
                | Dialect::Ansi
                | Dialect::Bigquery
                | Dialect::Clickhouse
                | Dialect::Databricks
                | Dialect::Hive
                | Dialect::Mysql
                | Dialect::Redshift
                | Dialect::Snowflake
        ),
        _ => true,
    }
}

fn lint_quality_for_rule(
    code: &str,
    engine: LintEngine,
    document: &LintDocument<'_>,
) -> (LintConfidence, Option<LintFallbackSource>) {
    if document.parser_fallback_used {
        return (
            LintConfidence::Medium,
            Some(LintFallbackSource::ParserFallback),
        );
    }

    if document.tokenizer_fallback_used && engine != LintEngine::Semantic {
        return (
            LintConfidence::Medium,
            Some(LintFallbackSource::TokenizerFallback),
        );
    }

    if ast_rule_code(code) {
        return (LintConfidence::High, None);
    }

    (LintConfidence::Low, Some(LintFallbackSource::HeuristicRule))
}

fn ast_rule_code(code: &str) -> bool {
    matches!(
        code,
        crate::types::issue_codes::LINT_AL_001
            | crate::types::issue_codes::LINT_AL_002
            | crate::types::issue_codes::LINT_AM_001
            | crate::types::issue_codes::LINT_AM_002
            | crate::types::issue_codes::LINT_AM_003
            | crate::types::issue_codes::LINT_AM_004
            | crate::types::issue_codes::LINT_AM_005
            | crate::types::issue_codes::LINT_AM_006
            | crate::types::issue_codes::LINT_AM_007
            | crate::types::issue_codes::LINT_AM_008
            | crate::types::issue_codes::LINT_AM_009
            | crate::types::issue_codes::LINT_CV_001
            | crate::types::issue_codes::LINT_CV_002
            | crate::types::issue_codes::LINT_CV_003
            | crate::types::issue_codes::LINT_CV_004
            | crate::types::issue_codes::LINT_CV_012
            | crate::types::issue_codes::LINT_RF_001
            | crate::types::issue_codes::LINT_RF_002
            | crate::types::issue_codes::LINT_RF_003
            | crate::types::issue_codes::LINT_ST_001
            | crate::types::issue_codes::LINT_ST_002
            | crate::types::issue_codes::LINT_ST_003
            | crate::types::issue_codes::LINT_ST_004
            | crate::types::issue_codes::LINT_ST_009
            | crate::types::issue_codes::LINT_ST_010
            | crate::types::issue_codes::LINT_ST_011
    )
}
