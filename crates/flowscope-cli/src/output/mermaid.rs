//! Mermaid diagram generation.
//!
//! Ported from packages/react/src/utils/exportUtils.ts

use flowscope_core::{AnalyzeResult, EdgeType, NodeType};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// View mode for Mermaid diagrams
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MermaidViewMode {
    Script,
    Table,
    Column,
    Hybrid,
}

/// Format the analysis result as a Mermaid diagram.
pub fn format_mermaid(result: &AnalyzeResult, view: MermaidViewMode) -> String {
    match view {
        MermaidViewMode::Script => generate_script_view(result),
        MermaidViewMode::Table => generate_table_view(result),
        MermaidViewMode::Column => generate_column_view(result),
        MermaidViewMode::Hybrid => generate_hybrid_view(result),
    }
}

/// Sanitize node ID for Mermaid (remove special chars)
fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Escape label for Mermaid
fn escape_label(label: &str) -> String {
    label.replace('"', "\\\"").replace('\n', " ")
}

/// Script information extracted from statements
#[derive(Debug)]
struct ScriptInfo {
    source_name: String,
    tables_read: HashSet<Arc<str>>,
    tables_written: HashSet<Arc<str>>,
}

/// Extract script information from statements
fn extract_script_info(result: &AnalyzeResult) -> Vec<ScriptInfo> {
    let mut script_map: HashMap<String, ScriptInfo> = HashMap::new();

    for stmt in &result.statements {
        let source_name = stmt
            .source_name
            .clone()
            .unwrap_or_else(|| "default".to_string());

        let entry = script_map
            .entry(source_name.clone())
            .or_insert_with(|| ScriptInfo {
                source_name: source_name.clone(),
                tables_read: HashSet::new(),
                tables_written: HashSet::new(),
            });

        for node in &stmt.nodes {
            if node.node_type == NodeType::Table {
                // A table is written to if it has incoming data_flow edges
                let is_written = stmt
                    .edges
                    .iter()
                    .any(|e| e.to == node.id && e.edge_type == EdgeType::DataFlow);
                // A table is read from if it has outgoing data_flow edges
                let is_read = stmt
                    .edges
                    .iter()
                    .any(|e| e.from == node.id && e.edge_type == EdgeType::DataFlow);

                let table_name = node
                    .qualified_name
                    .clone()
                    .unwrap_or_else(|| node.label.clone());

                if is_written {
                    entry.tables_written.insert(table_name.clone());
                }
                if is_read || !is_written {
                    entry.tables_read.insert(table_name);
                }
            }
        }
    }

    script_map.into_values().collect()
}

/// Column mapping for column-level view
#[derive(Debug)]
struct ColumnMapping {
    source_table: String,
    source_column: String,
    target_table: String,
    target_column: String,
    edge_type: EdgeType,
}

/// Extract column-level lineage mappings
fn extract_column_mappings(result: &AnalyzeResult) -> Vec<ColumnMapping> {
    let mut mappings = Vec::new();

    for stmt in &result.statements {
        let table_nodes: Vec<_> = stmt
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Table || n.node_type == NodeType::Cte)
            .collect();
        let column_nodes: Vec<_> = stmt
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Column)
            .collect();

        // Build column-to-table lookup
        let mut column_to_table: HashMap<&str, &str> = HashMap::new();
        for edge in &stmt.edges {
            if edge.edge_type == EdgeType::Ownership {
                if let Some(table_node) = table_nodes.iter().find(|t| t.id == edge.from) {
                    let table_name = table_node
                        .qualified_name
                        .as_deref()
                        .unwrap_or(&table_node.label);
                    column_to_table.insert(&edge.to, table_name);
                }
            }
        }

        // Find derivation/data_flow edges between columns
        for edge in &stmt.edges {
            if edge.edge_type == EdgeType::Derivation || edge.edge_type == EdgeType::DataFlow {
                let source_col = column_nodes.iter().find(|c| c.id == edge.from);
                let target_col = column_nodes.iter().find(|c| c.id == edge.to);

                if let (Some(source), Some(target)) = (source_col, target_col) {
                    let source_table = column_to_table
                        .get(&*edge.from)
                        .copied()
                        .unwrap_or("Output");
                    let target_table = column_to_table.get(&*edge.to).copied().unwrap_or("Output");

                    mappings.push(ColumnMapping {
                        source_table: source_table.to_string(),
                        source_column: source.label.to_string(),
                        target_table: target_table.to_string(),
                        target_column: target.label.to_string(),
                        edge_type: edge.edge_type,
                    });
                }
            }
        }
    }

    mappings
}

/// Generate Mermaid diagram for script-level view
fn generate_script_view(result: &AnalyzeResult) -> String {
    let scripts = extract_script_info(result);
    let mut lines = vec!["flowchart LR".to_string()];

    // Create script nodes
    for script in &scripts {
        let id = sanitize_id(&script.source_name);
        let label = escape_label(&script.source_name);
        lines.push(format!("    {id}[\"{label}\"]"));
    }

    // Create edges based on shared tables (script A writes -> script B reads)
    for producer in &scripts {
        for consumer in &scripts {
            if producer.source_name == consumer.source_name {
                continue;
            }

            let shared_tables: Vec<_> = producer
                .tables_written
                .iter()
                .filter(|t| consumer.tables_read.contains(*t))
                .collect();

            if !shared_tables.is_empty() {
                let producer_id = sanitize_id(&producer.source_name);
                let consumer_id = sanitize_id(&consumer.source_name);
                let label: String = if shared_tables.len() > 3 {
                    let first_three: Vec<&str> =
                        shared_tables.iter().take(3).map(|s| s.as_ref()).collect();
                    format!("{}...", first_three.join(", "))
                } else {
                    shared_tables
                        .iter()
                        .map(|s| s.as_ref())
                        .collect::<Vec<&str>>()
                        .join(", ")
                };
                lines.push(format!(
                    "    {} -->|\"{}\"| {}",
                    producer_id,
                    escape_label(&label),
                    consumer_id
                ));
            }
        }
    }

    lines.join("\n")
}

/// Generate Mermaid diagram for table-level view
fn generate_table_view(result: &AnalyzeResult) -> String {
    let mut lines = vec!["flowchart LR".to_string()];
    let mut table_ids: HashMap<Arc<str>, String> = HashMap::new();
    let mut edges: HashSet<String> = HashSet::new();

    for stmt in &result.statements {
        let table_nodes: Vec<_> = stmt
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Table || n.node_type == NodeType::Cte)
            .collect();

        // Add table nodes
        for node in &table_nodes {
            let key = node
                .qualified_name
                .clone()
                .unwrap_or_else(|| node.label.clone());
            if let std::collections::hash_map::Entry::Vacant(e) = table_ids.entry(key.clone()) {
                let id = sanitize_id(&key);
                e.insert(id.clone());
                let shape = if node.node_type == NodeType::Cte {
                    format!("([\"{}\"])", escape_label(&node.label))
                } else {
                    format!("[\"{}\"]", escape_label(&node.label))
                };
                lines.push(format!("    {id}{shape}"));
            }
        }

        // Add edges
        for edge in &stmt.edges {
            if edge.edge_type == EdgeType::DataFlow || edge.edge_type == EdgeType::Derivation {
                let source_node = table_nodes.iter().find(|n| n.id == edge.from);
                let target_node = table_nodes.iter().find(|n| n.id == edge.to);

                if let (Some(source), Some(target)) = (source_node, target_node) {
                    let source_key = source
                        .qualified_name
                        .clone()
                        .unwrap_or_else(|| source.label.clone());
                    let target_key = target
                        .qualified_name
                        .clone()
                        .unwrap_or_else(|| target.label.clone());
                    let edge_key = format!("{source_key}->{target_key}");

                    if !edges.contains(&edge_key) && source_key != target_key {
                        edges.insert(edge_key);
                        if let (Some(source_id), Some(target_id)) =
                            (table_ids.get(&source_key), table_ids.get(&target_key))
                        {
                            lines.push(format!("    {source_id} --> {target_id}"));
                        }
                    }
                }
            }
        }
    }

    lines.join("\n")
}

/// Generate Mermaid diagram for column-level view
fn generate_column_view(result: &AnalyzeResult) -> String {
    let mut lines = vec!["flowchart LR".to_string()];
    let mappings = extract_column_mappings(result);
    let mut nodes: HashSet<String> = HashSet::new();
    let mut edges: HashSet<String> = HashSet::new();

    for mapping in &mappings {
        let source_id = sanitize_id(&format!(
            "{}_{}",
            mapping.source_table, mapping.source_column
        ));
        let target_id = sanitize_id(&format!(
            "{}_{}",
            mapping.target_table, mapping.target_column
        ));
        let source_label = format!("{}.{}", mapping.source_table, mapping.source_column);
        let target_label = format!("{}.{}", mapping.target_table, mapping.target_column);

        if !nodes.contains(&source_id) {
            nodes.insert(source_id.clone());
            lines.push(format!(
                "    {}[\"{}\"]",
                source_id,
                escape_label(&source_label)
            ));
        }

        if !nodes.contains(&target_id) {
            nodes.insert(target_id.clone());
            lines.push(format!(
                "    {}[\"{}\"]",
                target_id,
                escape_label(&target_label)
            ));
        }

        let edge_key = format!("{source_id}->{target_id}");
        if !edges.contains(&edge_key) {
            edges.insert(edge_key);
            let style = if mapping.edge_type == EdgeType::Derivation {
                "-.->".to_string()
            } else {
                "-->".to_string()
            };
            lines.push(format!("    {source_id} {style} {target_id}"));
        }
    }

    lines.join("\n")
}

/// Generate Mermaid diagram for hybrid view (scripts + tables)
fn generate_hybrid_view(result: &AnalyzeResult) -> String {
    let mut lines = vec!["flowchart LR".to_string()];
    let scripts = extract_script_info(result);
    let mut all_tables: HashSet<Arc<str>> = HashSet::new();

    // Collect all tables
    for script in &scripts {
        all_tables.extend(script.tables_read.iter().cloned());
        all_tables.extend(script.tables_written.iter().cloned());
    }

    // Add script nodes
    for script in &scripts {
        let id = sanitize_id(&format!("script_{}", script.source_name));
        lines.push(format!(
            "    {}{{{{\"{}\"}}}}",
            id,
            escape_label(&script.source_name)
        ));
    }

    // Add table nodes
    for table in &all_tables {
        let id = sanitize_id(&format!("table_{table}"));
        let short_name = table.rsplit('.').next().unwrap_or(table);
        lines.push(format!("    {id}[\"{}\"]", escape_label(short_name)));
    }

    // Add edges: script -> written tables
    for script in &scripts {
        let script_id = sanitize_id(&format!("script_{}", script.source_name));
        for table in &script.tables_written {
            let table_id = sanitize_id(&format!("table_{table}"));
            lines.push(format!("    {script_id} --> {table_id}"));
        }
    }

    // Add edges: read tables -> script
    for script in &scripts {
        let script_id = sanitize_id(&format!("script_{}", script.source_name));
        for table in &script.tables_read {
            let table_id = sanitize_id(&format!("table_{table}"));
            lines.push(format!("    {table_id} --> {script_id}"));
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use flowscope_core::{analyze, AnalyzeRequest, Dialect, FileSource};

    #[test]
    fn test_sanitize_id() {
        assert_eq!(sanitize_id("my.table"), "my_table");
        assert_eq!(sanitize_id("schema.table"), "schema_table");
        assert_eq!(sanitize_id("my-table-name"), "my_table_name");
    }

    #[test]
    fn test_escape_label() {
        assert_eq!(escape_label("table\"name"), "table\\\"name");
        assert_eq!(escape_label("multi\nline"), "multi line");
    }

    #[test]
    fn test_table_view() {
        let result = analyze(&AnalyzeRequest {
            sql: "SELECT * FROM users; SELECT * FROM orders".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        });

        let mermaid = format_mermaid(&result, MermaidViewMode::Table);
        assert!(mermaid.starts_with("flowchart LR"));
    }

    #[test]
    fn test_script_view_multi_file() {
        let result = analyze(&AnalyzeRequest {
            sql: String::new(),
            files: Some(vec![
                FileSource {
                    name: "etl.sql".to_string(),
                    content: "INSERT INTO staging SELECT * FROM raw_data".to_string(),
                },
                FileSource {
                    name: "transform.sql".to_string(),
                    content: "INSERT INTO final SELECT * FROM staging".to_string(),
                },
            ]),
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        });

        let mermaid = format_mermaid(&result, MermaidViewMode::Script);
        assert!(mermaid.starts_with("flowchart LR"));
        assert!(mermaid.contains("etl_sql") || mermaid.contains("etl"));
    }
}
