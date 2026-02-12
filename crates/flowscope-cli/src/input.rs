//! Input handling for file reading and stdin support.

use anyhow::{Context, Result};
use flowscope_core::FileSource;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Lint input source containing SQL content and optional file path.
pub struct LintInputSource {
    pub source: FileSource,
    pub path: Option<PathBuf>,
}

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

/// Read lint input from files/directories or stdin.
///
/// Directory paths are expanded recursively and only `.sql` files are included.
/// Direct file paths are always included (regardless of extension) for backwards compatibility.
pub fn read_lint_input(paths: &[PathBuf]) -> Result<Vec<LintInputSource>> {
    if paths.is_empty() {
        return read_from_stdin().map(|sources| {
            sources
                .into_iter()
                .map(|source| LintInputSource { source, path: None })
                .collect()
        });
    }

    let expanded_paths = expand_lint_paths(paths)?;
    if expanded_paths.is_empty() {
        anyhow::bail!("No .sql files found in provided directories");
    }

    expanded_paths
        .into_iter()
        .map(|path| {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read file: {}", path.display()))?;

            Ok(LintInputSource {
                source: FileSource {
                    name: path.display().to_string(),
                    content,
                },
                path: Some(path),
            })
        })
        .collect()
}

/// Read SQL from stdin
fn read_from_stdin() -> Result<Vec<FileSource>> {
    let mut content = String::new();
    io::stdin()
        .read_to_string(&mut content)
        .context("Failed to read from stdin")?;

    Ok(vec![FileSource {
        // Use .sql extension so frontend filters include stdin content
        name: "<stdin>.sql".to_string(),
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

fn expand_lint_paths(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut expanded_paths = Vec::new();

    for path in paths {
        let metadata = std::fs::metadata(path)
            .with_context(|| format!("Failed to read file metadata: {}", path.display()))?;

        if metadata.is_dir() {
            collect_sql_files_recursive(path, &mut expanded_paths)?;
        } else {
            expanded_paths.push(path.clone());
        }
    }

    expanded_paths.sort();
    expanded_paths.dedup();
    Ok(expanded_paths)
}

fn collect_sql_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("Failed to read directory: {}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to read file type: {}", path.display()))?;

        if file_type.is_dir() {
            collect_sql_files_recursive(path.as_path(), out)?;
        } else if file_type.is_file() && is_sql_file(&path) {
            out.push(path);
        }
    }

    Ok(())
}

fn is_sql_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("sql"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
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

    #[test]
    fn test_read_lint_input_from_directory_recursively() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();

        let sql_one = dir.path().join("one.sql");
        let sql_two = nested.join("two.SQL");
        let txt_file = nested.join("ignore.txt");

        std::fs::write(&sql_one, "SELECT 1").unwrap();
        std::fs::write(&sql_two, "SELECT 2").unwrap();
        std::fs::write(&txt_file, "SELECT 3").unwrap();

        let inputs = read_lint_input(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(inputs.len(), 2);

        let names: Vec<String> = inputs.into_iter().map(|i| i.source.name).collect();
        assert!(names.iter().any(|n| n.ends_with("one.sql")));
        assert!(names.iter().any(|n| n.ends_with("two.SQL")));
        assert!(!names.iter().any(|n| n.ends_with("ignore.txt")));
    }
}
