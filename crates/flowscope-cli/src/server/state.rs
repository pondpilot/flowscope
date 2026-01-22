//! Shared application state for the server.
//!
//! This module defines the `AppState` struct that holds the server configuration,
//! watched files, and schema metadata. State is shared across handlers via `Arc`.

use std::path::PathBuf;

use anyhow::Result;
use flowscope_core::{Dialect, FileSource, SchemaMetadata};
use tokio::sync::RwLock;

/// Server configuration derived from CLI arguments.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// SQL dialect for analysis
    pub dialect: Dialect,
    /// Directories to watch for SQL files
    pub watch_dirs: Vec<PathBuf>,
    /// Database connection URL for live schema introspection
    pub metadata_url: Option<String>,
    /// Schema name filter for metadata provider
    pub metadata_schema: Option<String>,
    /// Port to listen on
    pub port: u16,
    /// Whether to open browser on startup
    pub open_browser: bool,
}

/// Shared application state.
pub struct AppState {
    /// Server configuration
    pub config: ServerConfig,
    /// Watched SQL files (updated by file watcher)
    pub files: RwLock<Vec<FileSource>>,
    /// Schema metadata from DDL or database
    pub schema: RwLock<Option<SchemaMetadata>>,
}

impl AppState {
    /// Create new application state, loading initial files and schema.
    pub async fn new(config: ServerConfig) -> Result<Self> {
        // Load initial files from watch directories
        let files = super::scan_sql_files(&config.watch_dirs)?;
        let file_count = files.len();

        // Load schema from database if URL provided
        let schema = Self::load_schema(&config).await?;

        if file_count > 0 {
            println!("flowscope: loaded {} SQL file(s)", file_count);
        }

        Ok(Self {
            config,
            files: RwLock::new(files),
            schema: RwLock::new(schema),
        })
    }

    /// Load schema metadata from database connection.
    #[cfg(feature = "metadata-provider")]
    async fn load_schema(config: &ServerConfig) -> Result<Option<SchemaMetadata>> {
        if let Some(ref url) = config.metadata_url {
            let schema =
                crate::metadata::fetch_metadata_from_database(url, config.metadata_schema.clone())?;
            println!("flowscope: loaded schema from database");
            return Ok(Some(schema));
        }
        Ok(None)
    }

    #[cfg(not(feature = "metadata-provider"))]
    async fn load_schema(_config: &ServerConfig) -> Result<Option<SchemaMetadata>> {
        Ok(None)
    }

    /// Reload files from watch directories.
    pub async fn reload_files(&self) -> Result<()> {
        let files = super::scan_sql_files(&self.config.watch_dirs)?;
        let count = files.len();
        *self.files.write().await = files;
        println!("flowscope: reloaded {} SQL file(s)", count);
        Ok(())
    }
}
