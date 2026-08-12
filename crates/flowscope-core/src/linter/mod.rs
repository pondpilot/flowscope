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
use rule::{
    DocumentSource, LintContext, RegisteredRule, RuleContext, RuleExecutionContext, RuleQuality,
    RuleScope, StatementSource,
};
use sqlparser::ast::Statement;
use std::borrow::Cow;

use crate::{
    parser::parse_sql,
    types::{Issue, LintConfidence, LintEngine, LintFallbackSource, Severity},
};

/// The SQL linter, holding a set of rules and configuration.
pub struct Linter {
    rules: Vec<RegisteredRule>,
    config: LintConfig,
}

impl Linter {
    /// Creates a new linter with the given configuration.
    pub fn new(config: LintConfig) -> Self {
        Self {
            rules: rules::registered_rules(&config),
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

        let execution = RuleExecutionContext::new(
            document.dialect,
            &document.raw_tokens,
            document.source_sql.is_some(),
        );
        self.check_document_with_execution(document, execution)
    }

    fn check_document_with_execution(
        &self,
        document: &LintDocument<'_>,
        execution: RuleExecutionContext<'_>,
    ) -> Vec<Issue> {
        let synthetic_statement = parse_sql("SELECT 1")
            .ok()
            .and_then(|mut statements| statements.drain(..).next());
        let mut issues = Vec::new();

        for engine in [
            LintEngine::Semantic,
            LintEngine::Lexical,
            LintEngine::Document,
        ] {
            for registered in &self.rules {
                let descriptor = registered.descriptor;
                let rule = registered.rule.as_ref();
                if !self.config.is_rule_enabled(rule.code())
                    || descriptor.engine != engine
                    || !descriptor.dialects.supports(document.dialect)
                {
                    continue;
                }

                let (confidence, fallback) = lint_quality(descriptor.quality, engine, document);
                match descriptor.scope {
                    RuleScope::Document(source) => {
                        let Some(synthetic_statement) = synthetic_statement.as_ref() else {
                            continue;
                        };
                        let document_scope_sql = document_scope_sql(&self.config, source, document);
                        let ctx = RuleContext::with_execution(
                            document_scope_sql.as_ref(),
                            0..document_scope_sql.len(),
                            0,
                            execution,
                        );
                        append_rule_issues(
                            &mut issues,
                            registered,
                            synthetic_statement,
                            &ctx,
                            confidence,
                            fallback,
                        );
                    }
                    RuleScope::Statement(source) => {
                        if document.statements.is_empty() {
                            if !descriptor.statementless_fallback {
                                continue;
                            }
                            let Some(synthetic_statement) = synthetic_statement.as_ref() else {
                                continue;
                            };
                            let ctx = RuleContext::with_execution(
                                document.sql,
                                0..document.sql.len(),
                                0,
                                execution,
                            );
                            append_rule_issues(
                                &mut issues,
                                registered,
                                synthetic_statement,
                                &ctx,
                                confidence,
                                fallback,
                            );
                            continue;
                        }

                        for statement in &document.statements {
                            let (ctx_sql, ctx_statement_range) =
                                statement_source_view(&self.config, source, document, statement);
                            let ctx = RuleContext::with_execution(
                                ctx_sql,
                                ctx_statement_range,
                                statement.statement_index,
                                execution,
                            );
                            append_rule_issues(
                                &mut issues,
                                registered,
                                statement.statement,
                                &ctx,
                                confidence,
                                fallback,
                            );
                        }
                    }
                }
            }
        }

        let issues = suppress_noqa_issues(issues, document);
        normalize_issues(issues)
    }

    /// Checks a single statement against all enabled lint rules.
    ///
    /// This adapter is kept for tests and rule-level helpers. Production paths
    /// should prefer `check_document()`.
    pub fn check_statement(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        let ctx = RuleContext::new(ctx.sql, ctx.statement_range.clone(), ctx.statement_index);
        self.check_statement_with_context(stmt, &ctx)
    }

    /// Checks one statement while preserving explicit document execution metadata.
    pub fn check_statement_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let document = LintDocument::new(
            ctx.sql,
            ctx.dialect(),
            vec![LintStatement {
                statement: stmt,
                statement_index: ctx.statement_index,
                statement_range: ctx.statement_range.clone(),
            }],
        );
        self.check_document_with_execution(&document, ctx.execution())
    }
}

fn append_rule_issues(
    issues: &mut Vec<Issue>,
    registered: &RegisteredRule,
    statement: &Statement,
    ctx: &RuleContext<'_>,
    confidence: LintConfidence,
    fallback: Option<LintFallbackSource>,
) {
    for issue in registered.rule.check_with_context(statement, ctx) {
        let mut issue = issue
            .with_lint_engine(registered.descriptor.engine)
            .with_lint_confidence(confidence);

        if let Some(source) = fallback {
            issue = issue.with_lint_fallback_source(source);
        }

        let sqlfluff_name = registered.rule.sqlfluff_name();
        if !sqlfluff_name.is_empty() {
            issue = issue.with_sqlfluff_name(sqlfluff_name);
        }

        issues.push(issue);
    }
}

fn statement_source_view<'a>(
    config: &LintConfig,
    source: StatementSource,
    document: &'a LintDocument<'_>,
    statement: &LintStatement<'_>,
) -> (&'a str, std::ops::Range<usize>) {
    match source {
        StatementSource::Rendered => (document.sql, statement.statement_range.clone()),
        StatementSource::MappedSource => match (
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
        },
        StatementSource::WholeSource => document.source_sql.map_or_else(
            || (document.sql, statement.statement_range.clone()),
            |source_sql| (source_sql, 0..source_sql.len()),
        ),
        StatementSource::TrailingWhitespace => {
            let ignore_templated = config
                .core_option_bool("ignore_templated_areas")
                .unwrap_or(true);
            match (
                document.source_sql,
                document
                    .source_statement_ranges
                    .get(statement.statement_index)
                    .and_then(|range| range.clone()),
            ) {
                (Some(source_sql), Some(source_statement_range)) if ignore_templated => {
                    let range = extend_range_with_trailing_whitespace(
                        source_sql,
                        &source_statement_range,
                        next_source_statement_start(
                            &document.source_statement_ranges,
                            statement.statement_index,
                        ),
                    );
                    (source_sql, range)
                }
                _ => {
                    let range = extend_range_with_trailing_whitespace(
                        document.sql,
                        &statement.statement_range,
                        next_statement_start(&document.statements, statement.statement_index),
                    );
                    (document.sql, range)
                }
            }
        }
    }
}

/// Extends a statement range to include trailing whitespace (spaces, tabs) and
/// the terminating newline. This is used by LT01 so it can detect and fix
/// trailing whitespace that `trim_statement_range` normally strips.
fn extend_range_with_trailing_whitespace(
    sql: &str,
    range: &std::ops::Range<usize>,
    next_start: Option<usize>,
) -> std::ops::Range<usize> {
    let bytes = sql.as_bytes();
    let limit = next_start.unwrap_or(sql.len());
    let mut end = range.end;
    while end < limit {
        match bytes[end] {
            b' ' | b'\t' => end += 1,
            b'\n' => {
                end += 1;
                break;
            }
            b'\r' => {
                end += 1;
                if end < limit && bytes[end] == b'\n' {
                    end += 1;
                }
                break;
            }
            _ => break,
        }
    }
    range.start..end
}

/// Returns the start byte of the next statement's range, if any.
fn next_statement_start(statements: &[LintStatement], current_index: usize) -> Option<usize> {
    statements
        .iter()
        .find(|s| s.statement_index == current_index + 1)
        .map(|s| s.statement_range.start)
}

fn next_source_statement_start(
    source_statement_ranges: &[Option<std::ops::Range<usize>>],
    current_index: usize,
) -> Option<usize> {
    source_statement_ranges
        .iter()
        .enumerate()
        .find_map(|(index, range)| {
            (index > current_index)
                .then(|| range.as_ref().map(|value| value.start))
                .flatten()
        })
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
            && left.autofix == right.autofix
    });
    issues
}

fn issue_sort_key(
    issue: &Issue,
) -> (
    usize,
    usize,
    usize,
    u8,
    &str,
    &str,
    Option<&crate::types::IssueAutofix>,
) {
    (
        issue.statement_index.unwrap_or(usize::MAX),
        issue.span.map_or(usize::MAX, |span| span.start),
        issue.span.map_or(usize::MAX, |span| span.end),
        severity_rank(issue.severity),
        issue.code.as_str(),
        issue.message.as_str(),
        issue.autofix.as_ref(),
    )
}

const fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

fn lint_quality(
    quality: RuleQuality,
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

    if quality == RuleQuality::Ast {
        return (LintConfidence::High, None);
    }

    (LintConfidence::Low, Some(LintFallbackSource::HeuristicRule))
}

fn document_scope_sql<'a>(
    config: &LintConfig,
    source: DocumentSource,
    document: &LintDocument<'a>,
) -> Cow<'a, str> {
    if source == DocumentSource::OriginalSource {
        if let Some(source_sql) = document.source_sql {
            return Cow::Borrowed(source_sql);
        }
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
    use super::{normalize_issues, strip_templated_areas, LintDocument, LintStatement, Linter};
    use crate::linter::config::LintConfig;
    use crate::linter::rule::RuleContext;
    use crate::parser::parse_sql_with_dialect;
    use crate::types::{
        issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span,
    };

    fn lint_parsed(sql: &str, dialect: Dialect) -> Vec<Issue> {
        let statements = parse_sql_with_dialect(sql, dialect).expect("parse");
        let lint_statements = statements
            .iter()
            .enumerate()
            .map(|(statement_index, statement)| LintStatement {
                statement,
                statement_index,
                statement_range: 0..sql.len(),
            })
            .collect();
        let document = LintDocument::new(sql, dialect, lint_statements);
        Linter::new(LintConfig::default()).check_document(&document)
    }

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

    #[test]
    fn normalize_issues_keeps_distinct_autofix_metadata() {
        let base = Issue::warning("LINT_X", "lint message")
            .with_statement(0)
            .with_span(Span::new(0, 1));

        let safe = base.clone().with_autofix_edits(
            IssueAutofixApplicability::Safe,
            vec![IssuePatchEdit::new(Span::new(0, 1), "x")],
        );
        let unsafe_fix = base.with_autofix_edits(
            IssueAutofixApplicability::Unsafe,
            vec![IssuePatchEdit::new(Span::new(0, 1), "x")],
        );

        let normalized = normalize_issues(vec![unsafe_fix, safe]);
        assert_eq!(normalized.len(), 2);
    }

    #[test]
    fn normalize_issues_dedups_when_autofix_matches() {
        let issue = Issue::warning("LINT_X", "lint message")
            .with_statement(0)
            .with_span(Span::new(0, 1))
            .with_autofix_edits(
                IssueAutofixApplicability::Safe,
                vec![IssuePatchEdit::new(Span::new(0, 1), "x")],
            );

        let normalized = normalize_issues(vec![issue.clone(), issue]);
        assert_eq!(normalized.len(), 1);
    }

    #[test]
    fn scheduler_filters_rules_using_descriptor_dialect_support() {
        let sql = "SELECT a FROM t UNION SELECT b, c FROM u";
        let generic = lint_parsed(sql, Dialect::Generic);
        let postgres = lint_parsed(sql, Dialect::Postgres);

        assert!(generic
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_AM_007));
        assert!(!postgres
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_AM_007));
    }

    #[test]
    fn scheduler_honors_statementless_fallback_descriptor() {
        let sql = "SELECT  1 UNION SELECT 2";
        let document = LintDocument::new(sql, Dialect::Generic, Vec::new());
        let issues = Linter::new(LintConfig::default()).check_document(&document);

        assert!(issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_LT_001));
        assert!(!issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_AM_002));
    }

    #[test]
    fn statement_adapter_preserves_explicit_execution_context() {
        let source_sql = "SELECT {{ \"greatest(a, b)\" }}, GREATEST(i, j)";
        let rendered_sql = "SELECT greatest(a, b), GREATEST(i, j)";
        let rendered = LintDocument::new(rendered_sql, Dialect::Ansi, Vec::new());
        let statements = parse_sql_with_dialect("SELECT 1", Dialect::Ansi).expect("parse");
        let config = LintConfig {
            rule_configs: std::collections::BTreeMap::from([(
                "core".to_string(),
                serde_json::json!({"ignore_templated_areas": false}),
            )]),
            ..LintConfig::default()
        };
        let context = RuleContext::new(source_sql, 0..source_sql.len(), 0)
            .with_dialect(Dialect::Ansi)
            .with_tokens(&rendered.raw_tokens)
            .with_templated(true);

        let issues = Linter::new(config).check_statement_with_context(&statements[0], &context);

        assert!(issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_CP_003));
    }
}
