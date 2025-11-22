//! Test utilities for loading fixtures and running integration tests.

use std::path::PathBuf;

/// Get the path to the test fixtures directory
pub fn fixtures_dir() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(manifest_dir).join("tests").join("fixtures")
}

/// Load a SQL fixture file by dialect and name
pub fn load_sql_fixture(dialect: &str, name: &str) -> String {
    let path = fixtures_dir().join(dialect).join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to load fixture {path:?}: {e}"))
}

/// Load a schema JSON fixture by name
pub fn load_schema_fixture(name: &str) -> crate::SchemaMetadata {
    let path = fixtures_dir().join("schemas").join(name);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to load schema {path:?}: {e}"));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse schema {path:?}: {e}"))
}

/// List all SQL fixtures for a given dialect
pub fn list_fixtures(dialect: &str) -> Vec<String> {
    let dir = fixtures_dir().join(dialect);
    if !dir.exists() {
        return Vec::new();
    }

    std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".sql") {
                Some(name)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixtures_dir_exists() {
        let dir = fixtures_dir();
        assert!(dir.exists(), "Fixtures directory should exist: {dir:?}");
    }

    #[test]
    fn test_list_generic_fixtures() {
        let fixtures = list_fixtures("generic");
        assert!(!fixtures.is_empty(), "Should have generic fixtures");
    }

    #[test]
    fn test_load_sql_fixture() {
        let sql = load_sql_fixture("generic", "01_basic_select.sql");
        assert!(sql.contains("SELECT"), "Should contain SELECT");
    }
}
