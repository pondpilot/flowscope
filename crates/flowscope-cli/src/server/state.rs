//! Shared application state for the server.
//!
//! This module defines the `AppState` struct that holds the server configuration,
//! watched files, and schema metadata. State is shared across handlers via `Arc`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::{Context, Result};
#[cfg(feature = "templating")]
use flowscope_core::TemplateConfig;
use flowscope_core::{Dialect, FileSource, SchemaMetadata};
use tokio::sync::RwLock;

/// Server configuration derived from CLI arguments.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// SQL dialect for analysis
    pub dialect: Dialect,
    /// Directories to watch for SQL files
    pub watch_dirs: Vec<PathBuf>,
    /// Static files to serve (when not using watch directories)
    pub static_files: Option<Vec<FileSource>>,
    /// Database connection URL for live schema introspection
    pub metadata_url: Option<String>,
    /// Schema name filter for metadata provider
    pub metadata_schema: Option<String>,
    /// Port to listen on
    pub port: u16,
    /// Whether to open browser on startup
    pub open_browser: bool,
    /// Optional schema DDL file path
    pub schema_path: Option<PathBuf>,
    /// Default template configuration (from CLI flags)
    #[cfg(feature = "templating")]
    pub template_config: Option<TemplateConfig>,
}

/// Shared application state.
pub struct AppState {
    /// Server configuration
    pub config: ServerConfig,
    /// Watched SQL files (updated by file watcher)
    pub files: RwLock<Vec<FileSource>>,
    /// Schema metadata from DDL or database
    pub schema: RwLock<Option<SchemaMetadata>>,
    /// File modification times for change detection
    pub mtimes: RwLock<HashMap<PathBuf, SystemTime>>,
}

impl AppState {
    /// Create new application state, loading initial files and schema.
    pub async fn new(config: ServerConfig) -> Result<Self> {
        // Load files either from static_files or by scanning watch directories
        let (files, mtimes) = if let Some(ref static_files) = config.static_files {
            // Use static files directly, no mtimes needed (no watching)
            (static_files.clone(), HashMap::new())
        } else {
            // Scan watch directories in a blocking thread pool
            let watch_dirs = config.watch_dirs.clone();
            let scan_result = tokio::task::spawn_blocking(move || super::scan_sql_files(&watch_dirs))
                .await
                .context("File scan task was cancelled")?;
            scan_result.context("Failed to scan SQL files")?
        };
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
            mtimes: RwLock::new(mtimes),
        })
    }

    /// Load schema metadata from database connection.
    ///
    /// Uses `spawn_blocking` because `fetch_metadata_from_database` internally creates
    /// a tokio runtime and blocks. Running blocking code on the async executor would
    /// stall other tasks.
    #[cfg(feature = "metadata-provider")]
    async fn load_schema(config: &ServerConfig) -> Result<Option<SchemaMetadata>> {
        if let Some(ref url) = config.metadata_url {
            let url = url.clone();
            let schema_filter = config.metadata_schema.clone();
            let fetch_result = tokio::task::spawn_blocking(move || {
                crate::metadata::fetch_metadata_from_database(&url, schema_filter)
            })
            .await
            .context("Metadata fetch task was cancelled")?;
            let schema = fetch_result.context("Failed to fetch database metadata")?;
            println!("flowscope: loaded schema from database");
            return Ok(Some(schema));
        }
        Self::load_schema_from_file(config)
    }

    #[cfg(not(feature = "metadata-provider"))]
    async fn load_schema(config: &ServerConfig) -> Result<Option<SchemaMetadata>> {
        Self::load_schema_from_file(config)
    }

    fn load_schema_from_file(config: &ServerConfig) -> Result<Option<SchemaMetadata>> {
        if let Some(ref path) = config.schema_path {
            let schema = crate::schema::load_schema_from_ddl(path, config.dialect)?;
            println!("flowscope: loaded schema from DDL file: {}", path.display());
            return Ok(Some(schema));
        }
        Ok(None)
    }

    /// Reload files from watch directories.
    ///
    /// File scanning is performed in a blocking thread pool to avoid blocking
    /// the async executor, which could delay other requests.
    ///
    /// Returns early for static files mode since there's nothing to reload.
    pub async fn reload_files(&self) -> Result<()> {
        // Static files don't reload - they were provided at startup
        if self.config.static_files.is_some() {
            return Ok(());
        }

        let watch_dirs = self.config.watch_dirs.clone();

        // Run file scanning in a blocking thread pool since it does I/O
        let scan_result =
            tokio::task::spawn_blocking(move || super::scan_sql_files(&watch_dirs))
                .await
                .context("File scan task was cancelled")?;
        let (files, mtimes) = scan_result.context("Failed to scan SQL files")?;

        let count = files.len();
        *self.files.write().await = files;
        *self.mtimes.write().await = mtimes;
        println!("flowscope: reloaded {} SQL file(s)", count);
        Ok(())
    }
}
