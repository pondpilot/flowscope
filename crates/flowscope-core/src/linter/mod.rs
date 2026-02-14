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
use rule::{with_active_dialect, with_active_document_tokens, LintContext, LintRule};
use sqlparser::ast::Statement;
use std::borrow::Cow;

use crate::{
    parser::parse_sql,
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
            rules: rules::all_rules(&config),
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

        with_active_document_tokens(&document.raw_tokens, || {
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

                    let (confidence, fallback) =
                        lint_quality_for_rule(rule.code(), engine, document);

                    if rule_uses_document_scope(rule.code()) {
                        let Some(synthetic_statement) = parse_sql("SELECT 1")
                            .ok()
                            .and_then(|mut statements| statements.drain(..).next())
                        else {
                            continue;
                        };

                        let document_scope_sql =
                            document_scope_sql_for_rule(&self.config, rule.code(), document);
                        let ctx = LintContext {
                            sql: document_scope_sql.as_ref(),
                            statement_range: 0..document_scope_sql.len(),
                            statement_index: 0,
                        };

                        with_active_dialect(document.dialect, || {
                            for issue in rule.check(&synthetic_statement, &ctx) {
                                let mut issue = issue
                                    .with_lint_engine(engine)
                                    .with_lint_confidence(confidence);

                                if let Some(source) = fallback {
                                    issue = issue.with_lint_fallback_source(source);
                                }

                                let sqlfluff_name = rule.sqlfluff_name();
                                if !sqlfluff_name.is_empty() {
                                    issue = issue.with_sqlfluff_name(sqlfluff_name);
                                }

                                issues.push(issue);
                            }
                        });
                        continue;
                    }

                    if document.statements.is_empty() {
                        if !rule_supports_statementless_fallback(rule.code()) {
                            continue;
                        }

                        let Some(synthetic_statement) = parse_sql("SELECT 1")
                            .ok()
                            .and_then(|mut statements| statements.drain(..).next())
                        else {
                            continue;
                        };

                        let ctx = LintContext {
                            sql: document.sql,
                            statement_range: 0..document.sql.len(),
                            statement_index: 0,
                        };

                        with_active_dialect(document.dialect, || {
                            for issue in rule.check(&synthetic_statement, &ctx) {
                                let mut issue = issue
                                    .with_lint_engine(engine)
                                    .with_lint_confidence(confidence);

                                if let Some(source) = fallback {
                                    issue = issue.with_lint_fallback_source(source);
                                }

                                let sqlfluff_name = rule.sqlfluff_name();
                                if !sqlfluff_name.is_empty() {
                                    issue = issue.with_sqlfluff_name(sqlfluff_name);
                                }

                                issues.push(issue);
                            }
                        });
                        continue;
                    }

                    for statement in &document.statements {
                        let (ctx_sql, ctx_statement_range) =
                            if rule.code() == crate::types::issue_codes::LINT_LT_007 {
                                match (
                                    document.source_sql,
                                    document
                                        .source_statement_ranges
                                        .get(statement.statement_index)
                                        .and_then(|range| range.clone()),
                                ) {
                                    (Some(source_sql), Some(source_statement_range)) => {
                                        (source_sql, source_statement_range)
                                    }
                                    _ => (document.sql, statement.statement_range.clone()),
                                }
                            } else {
                                (document.sql, statement.statement_range.clone())
                            };

                        let ctx = LintContext {
                            sql: ctx_sql,
                            statement_range: ctx_statement_range,
                            statement_index: statement.statement_index,
                        };

                        with_active_dialect(document.dialect, || {
                            for issue in rule.check(statement.statement, &ctx) {
                                let mut issue = issue
                                    .with_lint_engine(engine)
                                    .with_lint_confidence(confidence);

                                if let Some(source) = fallback {
                                    issue = issue.with_lint_fallback_source(source);
                                }

                                let sqlfluff_name = rule.sqlfluff_name();
                                if !sqlfluff_name.is_empty() {
                                    issue = issue.with_sqlfluff_name(sqlfluff_name);
                                }

                                issues.push(issue);
                            }
                        });
                    }
                }
            }

            let issues = suppress_noqa_issues(issues, document);
            normalize_issues(issues)
        })
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
        crate::types::issue_codes::LINT_AM_007 => matches!(
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
        crate::types::issue_codes::LINT_AL_003
            | crate::types::issue_codes::LINT_AL_004
            | crate::types::issue_codes::LINT_AL_005
            | crate::types::issue_codes::LINT_AL_006
            | crate::types::issue_codes::LINT_AL_007
            | crate::types::issue_codes::LINT_AL_008
            | crate::types::issue_codes::LINT_AL_009
            | crate::types::issue_codes::LINT_AM_001
            | crate::types::issue_codes::LINT_AM_002
            | crate::types::issue_codes::LINT_AM_003
            | crate::types::issue_codes::LINT_AM_004
            | crate::types::issue_codes::LINT_AM_005
            | crate::types::issue_codes::LINT_AM_006
            | crate::types::issue_codes::LINT_AM_007
            | crate::types::issue_codes::LINT_AM_008
            | crate::types::issue_codes::LINT_CV_002
            | crate::types::issue_codes::LINT_CV_004
            | crate::types::issue_codes::LINT_CV_005
            | crate::types::issue_codes::LINT_CV_008
            | crate::types::issue_codes::LINT_CV_012
            | crate::types::issue_codes::LINT_RF_001
            | crate::types::issue_codes::LINT_RF_002
            | crate::types::issue_codes::LINT_RF_003
            | crate::types::issue_codes::LINT_ST_001
            | crate::types::issue_codes::LINT_ST_002
            | crate::types::issue_codes::LINT_ST_003
            | crate::types::issue_codes::LINT_ST_004
            | crate::types::issue_codes::LINT_ST_005
            | crate::types::issue_codes::LINT_ST_006
            | crate::types::issue_codes::LINT_ST_007
            | crate::types::issue_codes::LINT_ST_008
            | crate::types::issue_codes::LINT_ST_009
            | crate::types::issue_codes::LINT_ST_010
            | crate::types::issue_codes::LINT_ST_011
    )
}

fn rule_uses_document_scope(code: &str) -> bool {
    matches!(
        code,
        crate::types::issue_codes::LINT_CP_001
            | crate::types::issue_codes::LINT_CP_003
            | crate::types::issue_codes::LINT_CP_004
            | crate::types::issue_codes::LINT_CP_005
    )
}

fn rule_supports_statementless_fallback(code: &str) -> bool {
    matches!(
        code,
        crate::types::issue_codes::LINT_LT_005
            | crate::types::issue_codes::LINT_CP_001
            | crate::types::issue_codes::LINT_CP_003
            | crate::types::issue_codes::LINT_CP_004
            | crate::types::issue_codes::LINT_CP_005
    )
}

fn document_scope_sql_for_rule<'a>(
    config: &LintConfig,
    code: &str,
    document: &LintDocument<'a>,
) -> Cow<'a, str> {
    if !rule_uses_document_scope(code) {
        return Cow::Borrowed(document.sql);
    }
    if !config
        .core_option_bool("ignore_templated_areas")
        .unwrap_or(false)
    {
        return Cow::Borrowed(document.sql);
    }
    let Some(source_sql) = document.source_sql else {
        return Cow::Borrowed(document.sql);
    };
    Cow::Owned(strip_templated_areas(source_sql))
}

fn strip_templated_areas(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut index = 0usize;

    while let Some((open_index, close_marker)) = find_next_template_open(sql, index) {
        out.push_str(&sql[index..open_index]);
        let marker_start = open_index + 2;
        if let Some(close_offset) = sql[marker_start..].find(close_marker) {
            let close_index = marker_start + close_offset + close_marker.len();
            out.push_str(&mask_non_newlines(&sql[open_index..close_index]));
            index = close_index;
        } else {
            out.push_str(&mask_non_newlines(&sql[open_index..]));
            return out;
        }
    }

    out.push_str(&sql[index..]);
    out
}

fn find_next_template_open(sql: &str, from: usize) -> Option<(usize, &'static str)> {
    let rest = sql.get(from..)?;
    let candidates = [("{{", "}}"), ("{%", "%}"), ("{#", "#}")];

    candidates
        .into_iter()
        .filter_map(|(open, close)| rest.find(open).map(|offset| (from + offset, close)))
        .min_by_key(|(index, _)| *index)
}

fn mask_non_newlines(segment: &str) -> String {
    segment
        .chars()
        .map(|ch| if ch == '\n' { '\n' } else { ' ' })
        .collect()
}

fn suppress_noqa_issues(issues: Vec<Issue>, document: &LintDocument<'_>) -> Vec<Issue> {
    issues
        .into_iter()
        .filter(|issue| {
            let Some(line) = issue_line(issue, document) else {
                return true;
            };
            !document.noqa.is_suppressed(line, &issue.code)
        })
        .collect()
}

fn issue_line(issue: &Issue, document: &LintDocument<'_>) -> Option<usize> {
    if let Some(span) = issue.span {
        return Some(offset_to_line(document.sql, span.start));
    }

    let statement_index = issue.statement_index?;
    let statement = document
        .statements
        .iter()
        .find(|statement| statement.statement_index == statement_index)?;
    Some(offset_to_line(
        document.sql,
        statement.statement_range.start,
    ))
}

fn offset_to_line(sql: &str, offset: usize) -> usize {
    1 + sql
        .as_bytes()
        .iter()
        .take(offset.min(sql.len()))
        .filter(|byte| **byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::strip_templated_areas;

    #[test]
    fn strip_templated_areas_preserves_lines_and_replaces_tag_content() {
        let sql = "SELECT {{ \"x\" }} AS x\nFROM t\nWHERE {% if true %}1{% endif %} = 1";
        let stripped = strip_templated_areas(sql);

        assert_eq!(stripped.lines().count(), sql.lines().count());
        assert!(!stripped.contains("{{"));
        assert!(!stripped.contains("{%"));
        assert!(stripped.contains("SELECT"));
        assert!(stripped.contains("FROM t"));
    }
}
