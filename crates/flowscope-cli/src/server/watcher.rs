//! File system watcher for SQL files.
//!
//! This module watches the configured directories for changes to SQL files
//! and triggers a reload of the application state when changes are detected.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebouncedEventKind};

use super::AppState;

/// Debounce duration for file changes.
const DEBOUNCE_DURATION: Duration = Duration::from_millis(100);

/// Start watching directories for SQL file changes.
///
/// This function runs until the task is cancelled. File changes are debounced
/// and trigger a reload of the application state.
pub async fn start_watcher(state: Arc<AppState>) -> Result<()> {
    let watch_dirs = state.config.watch_dirs.clone();

    if watch_dirs.is_empty() {
        // No directories to watch
        return Ok(());
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel(16);

    // Create debounced watcher
    let mut debouncer = new_debouncer(DEBOUNCE_DURATION, move |result| {
        if let Err(e) = tx.blocking_send(result) {
            eprintln!("flowscope: warning: failed to send file event: {e}");
        }
    })
    .map_err(|e| anyhow::anyhow!("Failed to create file watcher: {e}"))?;

    // Watch all configured directories
    for dir in &watch_dirs {
        if dir.exists() {
            debouncer
                .watcher()
                .watch(dir, RecursiveMode::Recursive)
                .map_err(|e| anyhow::anyhow!("Failed to watch {}: {e}", dir.display()))?;
            println!("flowscope: watching {}", dir.display());
        }
    }

    // Process file change events
    while let Some(result) = rx.recv().await {
        match result {
            Ok(events) => {
                // Check if any SQL files changed
                let sql_changed = events.iter().any(|event| {
                    event.path.extension().is_some_and(|ext| ext == "sql")
                        && matches!(
                            event.kind,
                            DebouncedEventKind::Any | DebouncedEventKind::AnyContinuous
                        )
                });

                if sql_changed {
                    // Log which files changed
                    let changed_files: Vec<_> = events
                        .iter()
                        .filter(|e| e.path.extension().is_some_and(|ext| ext == "sql"))
                        .map(|e| e.path.display().to_string())
                        .collect();
                    for file in &changed_files {
                        println!("flowscope: file changed: {file}");
                    }

                    if let Err(e) = state.reload_files().await {
                        eprintln!("flowscope: failed to reload files: {e}");
                    }
                }
            }
            Err(error) => {
                eprintln!("flowscope: watcher error: {error}");
            }
        }
    }

    Ok(())
}
