use chrono::{DateTime, Utc};

use crate::ExportFormat;

#[derive(Debug, Clone)]
pub struct ExportNaming {
    project_name: String,
    exported_at: DateTime<Utc>,
}

impl ExportNaming {
    pub fn new(project_name: impl Into<String>) -> Self {
        Self::with_exported_at(project_name, Utc::now())
    }

    pub fn with_exported_at(project_name: impl Into<String>, exported_at: DateTime<Utc>) -> Self {
        let mut name = project_name.into();
        if name.trim().is_empty() {
            name = "lineage".to_string();
        }
        Self {
            project_name: sanitize_project_name(&name),
            exported_at,
        }
    }

    pub fn exported_at(&self) -> DateTime<Utc> {
        self.exported_at
    }

    pub fn project_name(&self) -> &str {
        &self.project_name
    }

    pub fn filename(&self, format: ExportFormat) -> String {
        let timestamp = self.exported_at.format("%Y%m%d-%H%M%S");
        let (suffix, extension) = format_filename_parts(format);
        format!(
            "{}-{}-{}.{}",
            self.project_name, timestamp, suffix, extension
        )
    }
}

fn format_filename_parts(format: ExportFormat) -> (&'static str, &'static str) {
    match format {
        ExportFormat::Json { .. } => ("json", "json"),
        ExportFormat::Mermaid { view } => match view {
            crate::MermaidView::All => ("mermaid", "md"),
            crate::MermaidView::Script => ("mermaid-script", "md"),
            crate::MermaidView::Table => ("mermaid-table", "md"),
            crate::MermaidView::Column => ("mermaid-column", "md"),
            crate::MermaidView::Hybrid => ("mermaid-hybrid", "md"),
        },
        ExportFormat::Html => ("report", "html"),
        ExportFormat::Sql { .. } => ("duckdb", "sql"),
        ExportFormat::CsvBundle => ("csv", "zip"),
        ExportFormat::Xlsx => ("xlsx", "xlsx"),
        ExportFormat::DuckDb => ("duckdb", "duckdb"),
        ExportFormat::Png => ("png", "png"),
    }
}

fn sanitize_project_name(name: &str) -> String {
    let mut cleaned = String::new();
    let mut last_dash = false;

    for ch in name.trim().chars() {
        let normalized = ch.to_ascii_lowercase();
        if normalized.is_ascii_alphanumeric() {
            cleaned.push(normalized);
            last_dash = false;
        } else if matches!(normalized, '-' | '_' | ' ') && !last_dash {
            cleaned.push('-');
            last_dash = true;
        }
    }

    let cleaned = cleaned.trim_matches('-').to_string();
    if cleaned.is_empty() {
        "lineage".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn test_filename_generation() {
        let timestamp = Utc.with_ymd_and_hms(2026, 1, 18, 12, 30, 5).unwrap();
        let naming = ExportNaming::with_exported_at("FlowScope Demo", timestamp);
        let name = naming.filename(ExportFormat::Json { compact: false });
        assert_eq!(name, "flowscope-demo-20260118-123005-json.json");
    }

    #[test]
    fn test_project_name_sanitization() {
        let naming = ExportNaming::with_exported_at("  !!! ", Utc::now());
        assert_eq!(naming.project_name(), "lineage");
    }
}
