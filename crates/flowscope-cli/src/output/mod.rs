//! Output formatting modules.

pub mod json;
pub mod mermaid;
pub mod table;

pub use json::format_json;
pub use mermaid::{format_mermaid, MermaidViewMode};
pub use table::format_table;
