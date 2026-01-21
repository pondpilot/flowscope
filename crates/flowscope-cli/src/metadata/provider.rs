//! Metadata provider trait for database schema introspection.

use flowscope_core::SchemaMetadata;
use std::error::Error;

/// A provider that can fetch schema metadata from a database.
///
/// Implementations connect to databases and query system catalogs
/// (e.g., information_schema) to extract table and column definitions.
pub trait MetadataProvider {
    /// Fetch schema metadata from the connected database.
    ///
    /// Returns a `SchemaMetadata` structure containing all discovered
    /// tables and their columns, suitable for use in SQL lineage analysis.
    fn fetch_schema(&self) -> Result<SchemaMetadata, Box<dyn Error + Send + Sync>>;
}
