//! FlowScope CLI library.
//!
//! This module exposes internal types for testing purposes.
//! The main entry point is the `flowscope` binary.

pub mod cli;
pub mod fix;
pub mod input;
#[cfg(feature = "metadata-provider")]
pub mod metadata;
pub mod output;
pub mod schema;
#[cfg(feature = "serve")]
pub mod server;

// Re-export commonly used types
pub use cli::Args;
