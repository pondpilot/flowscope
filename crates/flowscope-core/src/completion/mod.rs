//! SQL completion module.
//!
//! This module provides SQL completion functionality with hybrid token/AST-based
//! strategies for handling incomplete SQL input.

pub mod ast_extractor;
mod context;
pub mod functions;
pub mod parse_strategies;

// Re-export the main completion API from context module
pub use context::{completion_context, completion_items};
pub use functions::{get_function_completions, FunctionCompletionContext};
