//! Output formatting modules.

pub mod lint;
pub mod table;

pub use lint::{format_lint_json, format_lint_results, FileLintResult, LintIssue};
pub use table::format_table;
