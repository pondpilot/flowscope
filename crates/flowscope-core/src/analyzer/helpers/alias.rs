//! Alias visibility checking helpers.
//!
//! This module provides utilities for checking SQL alias visibility rules
//! across different dialects. Different SQL dialects have varying rules about
//! where SELECT list aliases can be referenced (GROUP BY, HAVING, ORDER BY, etc.).

use crate::types::{issue_codes, Issue};
use crate::Dialect;

/// Emits a warning for unsupported alias usage in a SQL clause.
///
/// This helper centralizes the warning emission logic for alias visibility
/// checks across different clauses (GROUP BY, HAVING, ORDER BY, lateral aliases).
///
/// # Arguments
/// * `dialect` - The SQL dialect being used
/// * `clause_name` - The name of the clause where the alias is used (e.g., "GROUP BY", "HAVING")
/// * `alias_name` - The name of the alias being referenced
/// * `statement_index` - The index of the statement in the analysis request
///
/// # Returns
/// An `Issue` warning that can be pushed to the analyzer's issue list.
pub fn alias_visibility_warning(
    dialect: Dialect,
    clause_name: &str,
    alias_name: &str,
    statement_index: usize,
) -> Issue {
    Issue::warning(
        issue_codes::UNSUPPORTED_SYNTAX,
        format!(
            "Dialect '{dialect:?}' does not support referencing aliases in {clause_name} (alias '{alias_name}' used). This may fail at runtime."
        ),
    )
    .with_statement(statement_index)
}

/// Emits a warning for unsupported lateral column alias usage.
///
/// Lateral column aliases allow referencing an alias defined earlier in the same
/// SELECT list. Not all dialects support this feature.
///
/// # Arguments
/// * `dialect` - The SQL dialect being used
/// * `alias_name` - The name of the alias being referenced
/// * `statement_index` - The index of the statement in the analysis request
///
/// # Returns
/// An `Issue` warning that can be pushed to the analyzer's issue list.
pub fn lateral_alias_warning(dialect: Dialect, alias_name: &str, statement_index: usize) -> Issue {
    Issue::warning(
        issue_codes::UNSUPPORTED_SYNTAX,
        format!(
            "Dialect '{dialect:?}' does not support lateral column aliases (referencing alias '{alias_name}' from earlier in the SELECT list). This may fail at runtime."
        ),
    )
    .with_statement(statement_index)
}
