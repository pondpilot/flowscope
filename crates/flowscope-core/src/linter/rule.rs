//! Lint rule trait and context for SQL linting.

use super::config::sqlfluff_name_for_code;
use crate::types::{Dialect, Issue, Span};
use sqlparser::ast::Statement;
use std::cell::Cell;
use std::ops::Range;

thread_local! {
    static ACTIVE_DIALECT: Cell<Dialect> = const { Cell::new(Dialect::Generic) };
}

/// Context provided to lint rules during analysis.
pub struct LintContext<'a> {
    /// The full SQL source text.
    pub sql: &'a str,
    /// Byte range of the current statement within the SQL source.
    pub statement_range: Range<usize>,
    /// Zero-based index of the current statement.
    pub statement_index: usize,
}

impl<'a> LintContext<'a> {
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

    /// Returns the dialect active for the current lint pass.
    pub fn dialect(&self) -> Dialect {
        ACTIVE_DIALECT.with(Cell::get)
    }
}

pub(crate) fn with_active_dialect<T>(dialect: Dialect, f: impl FnOnce() -> T) -> T {
    ACTIVE_DIALECT.with(|active| {
        struct DialectReset<'a> {
            cell: &'a Cell<Dialect>,
            previous: Dialect,
        }

        impl Drop for DialectReset<'_> {
            fn drop(&mut self) {
                self.cell.set(self.previous);
            }
        }

        let reset = DialectReset {
            cell: active,
            previous: active.replace(dialect),
        };
        let result = f();
        drop(reset);
        result
    })
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

    /// Check a single parsed statement and return any issues found.
    fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue>;
}
