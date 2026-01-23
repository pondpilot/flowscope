//! File system watcher for SQL files.
//!
//! This module watches the configured directories for changes to SQL files
//! and triggers a reload of the application state when changes are detected.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebouncedEventKind};

use super::AppState;

/// Check if a file's mtime has actually changed compared to stored value.
fn has_mtime_changed(
    path: &std::path::Path,
    stored_mtimes: &std::collections::HashMap<std::path::PathBuf, std::time::SystemTime>,
) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) => match meta.modified() {
            Ok(current_mtime) => {
                // File changed if we don't have a stored mtime or it differs
                stored_mtimes
                    .get(path)
                    .map(|&stored| stored != current_mtime)
                    .unwrap_or(true)
            }
            Err(_) => true, // Can't read mtime, assume changed
        },
        Err(_) => true, // Can't read metadata, assume changed (file might be deleted)
    }
}

/// Debounce duration for file system events (100ms).
///
/// Groups rapid file changes (e.g., save + format + lint) into a single reload.
/// 100ms is short enough to feel responsive while filtering editor noise.
/// Increase if seeing duplicate reloads; decrease for faster feedback.
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
                // Get stored mtimes for comparison
                let stored_mtimes = state.mtimes.read().await.clone();

                // Filter to SQL files with actual mtime changes
                let changed_files: Vec<_> = events
                    .iter()
                    .filter(|event| {
                        event.path.extension().is_some_and(|ext| ext == "sql")
                            && matches!(
                                event.kind,
                                DebouncedEventKind::Any | DebouncedEventKind::AnyContinuous
                            )
                    })
                    .filter(|event| has_mtime_changed(&event.path, &stored_mtimes))
                    .map(|e| e.path.display().to_string())
                    .collect();

                if !changed_files.is_empty() {
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
