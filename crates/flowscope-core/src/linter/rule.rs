//! Lint rule trait, descriptors, and execution context for SQL linting.

use super::config::sqlfluff_name_for_code;
use crate::types::{Dialect, Issue, LintEngine, Span};
use sqlparser::ast::Statement;
use sqlparser::tokenizer::TokenWithSpan;
use std::ops::{Deref, Range};

/// Metadata shared by every rule invocation in a lint document.
///
/// The context is passed explicitly so concurrent and nested lint runs cannot
/// observe each other's dialect, token stream, or templating state.
#[derive(Debug, Clone, Copy)]
pub struct RuleExecutionContext<'a> {
    dialect: Dialect,
    document_tokens: &'a [TokenWithSpan],
    is_templated: bool,
}

impl<'a> RuleExecutionContext<'a> {
    /// Creates execution metadata for a lint document.
    #[must_use]
    pub const fn new(
        dialect: Dialect,
        document_tokens: &'a [TokenWithSpan],
        is_templated: bool,
    ) -> Self {
        Self {
            dialect,
            document_tokens,
            is_templated,
        }
    }

    /// Returns the document's SQL dialect.
    #[must_use]
    pub const fn dialect(self) -> Dialect {
        self.dialect
    }

    /// Returns the token stream produced for the complete lint document.
    #[must_use]
    pub const fn document_tokens(self) -> &'a [TokenWithSpan] {
        self.document_tokens
    }

    /// Returns whether a templater processed the document before linting.
    #[must_use]
    pub const fn is_templated(self) -> bool {
        self.is_templated
    }

    #[must_use]
    const fn with_dialect(mut self, dialect: Dialect) -> Self {
        self.dialect = dialect;
        self
    }

    #[must_use]
    const fn with_tokens(mut self, document_tokens: &'a [TokenWithSpan]) -> Self {
        self.document_tokens = document_tokens;
        self
    }

    #[must_use]
    const fn with_templated(mut self, is_templated: bool) -> Self {
        self.is_templated = is_templated;
        self
    }
}

impl Default for RuleExecutionContext<'_> {
    fn default() -> Self {
        Self::new(Dialect::Generic, &[], false)
    }
}

/// Context provided to lint rules during analysis.
#[derive(Debug, Clone)]
pub struct LintContext<'a> {
    /// The full SQL source text.
    pub sql: &'a str,
    /// Byte range of the current statement within the SQL source.
    pub statement_range: Range<usize>,
    /// Zero-based index of the current statement.
    pub statement_index: usize,
}

impl<'a> LintContext<'a> {
    /// Creates a statement context.
    #[must_use]
    pub fn new(sql: &'a str, statement_range: Range<usize>, statement_index: usize) -> Self {
        Self {
            sql,
            statement_range,
            statement_index,
        }
    }

    /// Adds explicit document execution metadata to this statement context.
    #[must_use]
    pub fn with_execution(self, execution: RuleExecutionContext<'a>) -> RuleContext<'a> {
        RuleContext::with_execution(
            self.sql,
            self.statement_range,
            self.statement_index,
            execution,
        )
    }

    /// Adds a dialect to this statement context.
    #[must_use]
    pub fn with_dialect(self, dialect: Dialect) -> RuleContext<'a> {
        self.with_execution(RuleExecutionContext::default().with_dialect(dialect))
    }

    /// Adds a document token stream to this statement context.
    #[must_use]
    pub fn with_tokens(self, document_tokens: &'a [TokenWithSpan]) -> RuleContext<'a> {
        self.with_execution(RuleExecutionContext::default().with_tokens(document_tokens))
    }

    /// Adds templating state to this statement context.
    #[must_use]
    pub fn with_templated(self, is_templated: bool) -> RuleContext<'a> {
        self.with_execution(RuleExecutionContext::default().with_templated(is_templated))
    }

    /// Returns the SQL text for the current statement.
    pub fn statement_sql(&self) -> &str {
        &self.sql[self.statement_range.clone()]
    }

    /// Converts a byte offset relative to the statement into an absolute `Span`.
    pub fn span_from_statement_offset(&self, start: usize, end: usize) -> Span {
        Span::new(
            self.statement_range.start + start,
            self.statement_range.start + end,
        )
    }

    /// Returns the default dialect for direct legacy rule calls.
    pub const fn dialect(&self) -> Dialect {
        Dialect::Generic
    }

    /// Invokes `f` with the empty default token stream used by direct legacy calls.
    pub fn with_document_tokens<T>(&self, f: impl FnOnce(&[TokenWithSpan]) -> T) -> T {
        f(&[])
    }

    /// Returns false because direct legacy rule calls have no templating metadata.
    pub const fn is_templated(&self) -> bool {
        false
    }
}

/// A statement context paired with immutable document execution metadata.
///
/// The scheduler passes this type to registered rules. `LintContext` remains
/// available for source-compatible direct rule calls, which use default
/// execution metadata.
#[derive(Debug, Clone)]
pub struct RuleContext<'a> {
    lint: LintContext<'a>,
    execution: RuleExecutionContext<'a>,
}

impl<'a> RuleContext<'a> {
    /// Creates a context with generic, non-templated execution metadata.
    #[must_use]
    pub fn new(sql: &'a str, statement_range: Range<usize>, statement_index: usize) -> Self {
        Self::with_execution(
            sql,
            statement_range,
            statement_index,
            RuleExecutionContext::default(),
        )
    }

    /// Creates a statement context with explicit document execution metadata.
    #[must_use]
    pub fn with_execution(
        sql: &'a str,
        statement_range: Range<usize>,
        statement_index: usize,
        execution: RuleExecutionContext<'a>,
    ) -> Self {
        Self {
            lint: LintContext::new(sql, statement_range, statement_index),
            execution,
        }
    }

    /// Returns the source-compatible statement context.
    #[must_use]
    pub const fn lint_context(&self) -> &LintContext<'a> {
        &self.lint
    }

    /// Returns all document execution metadata.
    #[must_use]
    pub const fn execution(&self) -> RuleExecutionContext<'a> {
        self.execution
    }

    /// Overrides the dialect on a newly constructed context.
    #[must_use]
    pub fn with_dialect(mut self, dialect: Dialect) -> Self {
        self.execution = self.execution.with_dialect(dialect);
        self
    }

    /// Overrides the document token stream on a newly constructed context.
    #[must_use]
    pub fn with_tokens(mut self, document_tokens: &'a [TokenWithSpan]) -> Self {
        self.execution = self.execution.with_tokens(document_tokens);
        self
    }

    /// Marks a newly constructed context as templated or untemplated.
    #[must_use]
    pub fn with_templated(mut self, is_templated: bool) -> Self {
        self.execution = self.execution.with_templated(is_templated);
        self
    }

    /// Creates another statement view while preserving execution metadata.
    #[must_use]
    pub fn statement_view(
        &self,
        sql: &'a str,
        statement_range: Range<usize>,
        statement_index: usize,
    ) -> Self {
        Self::with_execution(sql, statement_range, statement_index, self.execution)
    }

    /// Returns the dialect active for this rule invocation.
    pub const fn dialect(&self) -> Dialect {
        self.execution.dialect()
    }

    /// Invokes `f` with the document token stream.
    ///
    /// Tokens include location spans from the single tokenizer pass performed
    /// during `LintDocument` construction.
    pub fn with_document_tokens<T>(&self, f: impl FnOnce(&[TokenWithSpan]) -> T) -> T {
        f(self.execution.document_tokens())
    }

    /// Returns true if the document was processed through a templater
    /// (Jinja, dbt, etc.) before linting.
    pub const fn is_templated(&self) -> bool {
        self.execution.is_templated()
    }
}

impl<'a> Deref for RuleContext<'a> {
    type Target = LintContext<'a>;

    fn deref(&self) -> &Self::Target {
        &self.lint
    }
}

/// The source view supplied to a statement-scoped rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatementSource {
    /// Use the rendered SQL statement range.
    Rendered,
    /// Prefer the corresponding original-source statement range when mapped.
    MappedSource,
    /// Use the complete original source when available.
    WholeSource,
    /// Include trailing whitespace trimmed from ordinary statement ranges.
    TrailingWhitespace,
}

/// The source view supplied to a document-scoped rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentSource {
    /// Use original source and mask templated areas when configured.
    MaskedSource,
    /// Use the complete original source when available.
    OriginalSource,
}

/// How often a rule is invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuleScope {
    Statement(StatementSource),
    Document(DocumentSource),
}

/// The analysis quality of a rule under normal parser/tokenizer operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuleQuality {
    Ast,
    Heuristic,
}

/// Dialects accepted by a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialectSupport {
    All,
    Only(&'static [Dialect]),
}

impl DialectSupport {
    pub fn supports(self, dialect: Dialect) -> bool {
        match self {
            Self::All => true,
            Self::Only(dialects) => dialects.contains(&dialect),
        }
    }
}

/// Declarative scheduling and fallback policy for a registered rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuleDescriptor {
    pub engine: LintEngine,
    pub scope: RuleScope,
    pub dialects: DialectSupport,
    pub quality: RuleQuality,
    pub statementless_fallback: bool,
}

impl RuleDescriptor {
    pub const fn new(
        engine: LintEngine,
        scope: RuleScope,
        dialects: DialectSupport,
        quality: RuleQuality,
        statementless_fallback: bool,
    ) -> Self {
        Self {
            engine,
            scope,
            dialects,
            quality,
            statementless_fallback,
        }
    }
}

/// A rule paired with the metadata used to schedule it.
pub(crate) struct RegisteredRule {
    pub descriptor: RuleDescriptor,
    pub rule: Box<dyn LintRule>,
}

impl RegisteredRule {
    pub fn new(rule: Box<dyn LintRule>, descriptor: RuleDescriptor) -> Self {
        Self { descriptor, rule }
    }
}

/// A single lint rule that checks a parsed SQL statement for anti-patterns.
pub trait LintRule: Send + Sync {
    /// Machine-readable rule code (e.g., "LINT_AM_008").
    fn code(&self) -> &'static str;

    /// Short human-readable name (e.g., "Bare UNION").
    fn name(&self) -> &'static str;

    /// Longer description of what this rule checks.
    fn description(&self) -> &'static str;

    /// SQLFluff dotted identifier (e.g., `aliasing.table`).
    fn sqlfluff_name(&self) -> &'static str {
        sqlfluff_name_for_code(self.code()).unwrap_or("")
    }

    /// Check a statement using generic, non-templated execution metadata.
    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue>;

    /// Check a statement with explicit document execution metadata.
    fn check_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        self.check(stmt, ctx.lint_context())
    }
}

/// Internal contract for built-in rules that consume explicit execution metadata.
pub(crate) trait BuiltinLintRule: Send + Sync {
    fn code(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn check_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue>;
}

impl<T: BuiltinLintRule> LintRule for T {
    fn code(&self) -> &'static str {
        BuiltinLintRule::code(self)
    }

    fn name(&self) -> &'static str {
        BuiltinLintRule::name(self)
    }

    fn description(&self) -> &'static str {
        BuiltinLintRule::description(self)
    }

    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
        BuiltinLintRule::check_with_context(
            self,
            stmt,
            &RuleContext::new(ctx.sql, ctx.statement_range.clone(), ctx.statement_index),
        )
    }

    fn check_with_context(&self, stmt: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        BuiltinLintRule::check_with_context(self, stmt, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::{LintContext, LintRule, RuleContext, RuleExecutionContext};
    use crate::{Dialect, Issue};
    use sqlparser::ast::Statement;
    use sqlparser::tokenizer::{Token, TokenWithSpan};

    struct LegacyRule;

    impl LintRule for LegacyRule {
        fn code(&self) -> &'static str {
            "LINT_TEST"
        }

        fn name(&self) -> &'static str {
            "Legacy test rule"
        }

        fn description(&self) -> &'static str {
            "Exercises the source-compatible public rule contract."
        }

        fn check(&self, _stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
            assert_eq!(ctx.dialect(), Dialect::Generic);
            assert!(!ctx.is_templated());
            ctx.with_document_tokens(|tokens| assert!(tokens.is_empty()));
            Vec::new()
        }
    }

    #[test]
    fn execution_contexts_are_isolated() {
        let postgres_tokens = [TokenWithSpan::wrap(Token::Comma)];
        let mysql_tokens = [TokenWithSpan::wrap(Token::Period)];
        let postgres = RuleContext::with_execution(
            "SELECT 1",
            0..8,
            0,
            RuleExecutionContext::new(Dialect::Postgres, &postgres_tokens, true),
        );
        let mysql = RuleContext::with_execution(
            "SELECT 2",
            0..8,
            0,
            RuleExecutionContext::new(Dialect::Mysql, &mysql_tokens, false),
        );

        assert_eq!(postgres.dialect(), Dialect::Postgres);
        assert!(postgres.is_templated());
        postgres.with_document_tokens(|tokens| assert_eq!(tokens, postgres_tokens));

        assert_eq!(mysql.dialect(), Dialect::Mysql);
        assert!(!mysql.is_templated());
        mysql.with_document_tokens(|tokens| assert_eq!(tokens, mysql_tokens));

        assert_eq!(postgres.dialect(), Dialect::Postgres);
        assert!(postgres.is_templated());

        let legacy = LintContext {
            sql: "SELECT 3",
            statement_range: 0..8,
            statement_index: 0,
        };
        assert_eq!(legacy.statement_sql(), "SELECT 3");
        assert_eq!(legacy.dialect(), Dialect::Generic);
        assert!(!legacy.is_templated());
        legacy.with_document_tokens(|tokens| assert!(tokens.is_empty()));
    }

    #[test]
    fn legacy_rule_contract_has_one_required_non_recursive_entry_point() {
        let statements = crate::parse_sql("SELECT 1").expect("parse");
        let legacy = LintContext {
            sql: "SELECT 1",
            statement_range: 0..8,
            statement_index: 0,
        };

        assert!(LegacyRule.check(&statements[0], &legacy).is_empty());
        assert!(LegacyRule
            .check_with_context(&statements[0], &RuleContext::new("SELECT 1", 0..8, 0))
            .is_empty());
    }
}
