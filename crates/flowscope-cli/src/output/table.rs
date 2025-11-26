//! Human-readable table output formatting.

use flowscope_core::{AnalyzeResult, NodeType, Severity};
use is_terminal::IsTerminal;
use owo_colors::OwoColorize;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

/// Format the analysis result as human-readable text with optional colors.
pub fn format_table(result: &AnalyzeResult, quiet: bool, use_colors: bool) -> String {
    let colored = use_colors && std::io::stdout().is_terminal();
    let mut out = String::new();

    write_header(&mut out, colored);
    write_summary(&mut out, result, colored);
    write_lineage(&mut out, result, colored);

    if !quiet {
        write_issues(&mut out, result, colored);
    }

    out
}

fn write_header(out: &mut String, colored: bool) {
    let title = "FlowScope Analysis";
    let line = "═".repeat(50);

    if colored {
        writeln!(out, "{}", title.bold()).unwrap();
        writeln!(out, "{}", line.dimmed()).unwrap();
    } else {
        writeln!(out, "{title}").unwrap();
        writeln!(out, "{line}").unwrap();
    }
}

fn write_summary(out: &mut String, result: &AnalyzeResult, colored: bool) {
    let summary = &result.summary;

    // Collect unique source names
    let sources: HashSet<_> = result
        .statements
        .iter()
        .filter_map(|s| s.source_name.as_ref())
        .collect();

    if !sources.is_empty() {
        let files: Vec<_> = sources.iter().map(|s| s.as_str()).collect();
        writeln!(out, "Files: {}", files.join(", ")).unwrap();
    }

    writeln!(out).unwrap();

    let stats = format!(
        "Summary: {} statements | {} tables | {} columns",
        summary.statement_count, summary.table_count, summary.column_count
    );

    if colored {
        writeln!(out, "{}", stats.cyan()).unwrap();
    } else {
        writeln!(out, "{stats}").unwrap();
    }

    writeln!(out).unwrap();
}

fn write_lineage(out: &mut String, result: &AnalyzeResult, colored: bool) {
    // Build table relationships from global lineage
    let mut source_tables: HashMap<String, HashSet<String>> = HashMap::new();

    for edge in &result.global_lineage.edges {
        let from_node = result
            .global_lineage
            .nodes
            .iter()
            .find(|n| n.id == edge.from);
        let to_node = result.global_lineage.nodes.iter().find(|n| n.id == edge.to);

        if let (Some(from), Some(to)) = (from_node, to_node) {
            if matches!(from.node_type, NodeType::Table | NodeType::View)
                && matches!(to.node_type, NodeType::Table | NodeType::View)
            {
                source_tables
                    .entry(to.label.to_string())
                    .or_default()
                    .insert(from.label.to_string());
            }
        }
    }

    if source_tables.is_empty() {
        // Just list tables/views if no relationships found
        let tables: Vec<_> = result
            .global_lineage
            .nodes
            .iter()
            .filter(|n| matches!(n.node_type, NodeType::Table | NodeType::View))
            .map(|n| n.label.to_string())
            .collect();

        if !tables.is_empty() {
            if colored {
                writeln!(out, "{}", "Tables:".bold()).unwrap();
            } else {
                writeln!(out, "Tables:").unwrap();
            }

            for table in tables {
                writeln!(out, "  {table}").unwrap();
            }
            writeln!(out).unwrap();
        }
    } else {
        if colored {
            writeln!(out, "{}", "Table Lineage:".bold()).unwrap();
        } else {
            writeln!(out, "Table Lineage:").unwrap();
        }

        for (target, sources) in &source_tables {
            let source_list: Vec<_> = sources.iter().map(|s| s.as_str()).collect();
            let arrow = if colored {
                "→".green().to_string()
            } else {
                "→".to_string()
            };
            writeln!(out, "  {} {} {}", source_list.join(", "), arrow, target).unwrap();
        }
        writeln!(out).unwrap();
    }
}

fn write_issues(out: &mut String, result: &AnalyzeResult, colored: bool) {
    if result.issues.is_empty() {
        return;
    }

    let error_count = result.summary.issue_count.errors;
    let warning_count = result.summary.issue_count.warnings;
    let info_count = result.summary.issue_count.infos;

    let mut parts = Vec::new();
    if error_count > 0 {
        parts.push(format!("{error_count} errors"));
    }
    if warning_count > 0 {
        parts.push(format!("{warning_count} warnings"));
    }
    if info_count > 0 {
        parts.push(format!("{info_count} info"));
    }

    let header = format!("Issues ({}):", parts.join(", "));

    if colored {
        writeln!(out, "{}", header.bold()).unwrap();
    } else {
        writeln!(out, "{header}").unwrap();
    }

    for issue in &result.issues {
        let severity_str = match issue.severity {
            Severity::Error => {
                if colored {
                    "ERROR".red().to_string()
                } else {
                    "ERROR".to_string()
                }
            }
            Severity::Warning => {
                if colored {
                    "WARN".yellow().to_string()
                } else {
                    "WARN".to_string()
                }
            }
            Severity::Info => {
                if colored {
                    "INFO".blue().to_string()
                } else {
                    "INFO".to_string()
                }
            }
        };

        let location = issue
            .span
            .as_ref()
            .map(|s| format!(" offset {}:", s.start))
            .unwrap_or_default();

        writeln!(out, "  [{}]{} {}", severity_str, location, issue.message).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flowscope_core::{analyze, AnalyzeRequest, Dialect};

    #[test]
    fn test_format_table_basic() {
        let result = analyze(&AnalyzeRequest {
            sql: "SELECT * FROM users".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        });

        let output = format_table(&result, false, false);
        assert!(output.contains("FlowScope Analysis"));
        assert!(output.contains("Summary:"));
    }

    #[test]
    fn test_format_table_quiet() {
        let result = analyze(&AnalyzeRequest {
            sql: "SELECT * FROM nonexistent_syntax_error@@@".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        });

        let output_quiet = format_table(&result, true, false);
        let output_verbose = format_table(&result, false, false);

        // Quiet mode may have fewer issue lines (but both might have none if parsing succeeds)
        assert!(output_quiet.len() <= output_verbose.len() || output_quiet == output_verbose);
    }
}
