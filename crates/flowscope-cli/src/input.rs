//! Input handling for file reading and stdin support.

use anyhow::{Context, Result};
use flowscope_core::FileSource;
use std::io::{self, Read};
use std::path::PathBuf;

/// Read SQL input from files or stdin.
///
/// If no files are provided, reads from stdin.
/// Returns a vector of FileSource for multi-file analysis.
pub fn read_input(files: &[PathBuf]) -> Result<Vec<FileSource>> {
    if files.is_empty() {
        read_from_stdin()
    } else {
        read_from_files(files)
    }
}

/// Read SQL from stdin
fn read_from_stdin() -> Result<Vec<FileSource>> {
    let mut content = String::new();
    io::stdin()
        .read_to_string(&mut content)
        .context("Failed to read from stdin")?;

    Ok(vec![FileSource {
        name: "<stdin>".to_string(),
        content,
    }])
}

/// Read SQL from multiple files
fn read_from_files(files: &[PathBuf]) -> Result<Vec<FileSource>> {
    files
        .iter()
        .map(|path| {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read file: {}", path.display()))?;

            Ok(FileSource {
                name: path.display().to_string(),
                content,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_single_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "SELECT * FROM users").unwrap();

        let sources = read_from_files(&[file.path().to_path_buf()]).unwrap();
        assert_eq!(sources.len(), 1);
        assert!(sources[0].content.contains("SELECT * FROM users"));
    }

    #[test]
    fn test_read_multiple_files() {
        let mut file1 = NamedTempFile::new().unwrap();
        let mut file2 = NamedTempFile::new().unwrap();
        writeln!(file1, "SELECT * FROM users").unwrap();
        writeln!(file2, "SELECT * FROM orders").unwrap();

        let sources =
            read_from_files(&[file1.path().to_path_buf(), file2.path().to_path_buf()]).unwrap();
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn test_read_missing_file() {
        let result = read_from_files(&[PathBuf::from("/nonexistent/file.sql")]);
        assert!(result.is_err());
    }
}
