//! Live database metadata providers for schema introspection.
//!
//! This module provides the infrastructure for fetching schema metadata directly
//! from databases at runtime. This enables accurate wildcard expansion (SELECT *)
//! without requiring manual DDL files.
//!
//! Note: This is a CLI-only feature. WASM/browser builds cannot make direct
//! database connections and should use the DDL-based schema loading instead.

#[cfg(feature = "metadata-provider")]
mod sqlx_provider;

#[cfg(feature = "metadata-provider")]
pub use sqlx_provider::fetch_metadata_from_database;
