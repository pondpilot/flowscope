//! Function classification and argument handling.
//!
//! This module provides wrappers around generated function data from
//! `specs/dialect-semantics/`. For dialect-aware queries, use the
//! generated module directly via `crate::generated`.

use crate::generated;

// Re-export generated function classification helpers
pub(crate) use generated::is_aggregate_function;

/// Information about an aggregate function call found in an expression.
#[derive(Debug, Clone)]
pub(crate) struct AggregateCall {
    /// The aggregate function name (uppercase, e.g., "SUM", "COUNT")
    pub(crate) function: String,
    /// Whether DISTINCT was specified
    pub(crate) distinct: bool,
}
