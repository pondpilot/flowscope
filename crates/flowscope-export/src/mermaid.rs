use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use flowscope_core::{AnalyzeResult, EdgeType, NodeType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MermaidView {
    All,
    Script,
    Table,
    Column,
    Hybrid,
}

pub fn export_mermaid(result: &AnalyzeResult, view: MermaidView) -> String {
    match view {
        MermaidView::All => generate_all_views(result),
        MermaidView::Script => generate_script_view(result),
        MermaidView::Table => generate_table_view(result),
        MermaidView::Column => generate_column_view(result),
        MermaidView::Hybrid => generate_hybrid_view(result),
    }
}

fn generate_all_views(result: &AnalyzeResult) -> String {
    let sections = vec![
        "# Lineage Diagrams".to_string(),
        String::new(),
        "## Script View".to_string(),
        "```mermaid".to_string(),
        generate_script_view(result),
        "```".to_string(),
        String::new(),
        "## Hybrid View (Scripts + Tables)".to_string(),
        "```mermaid".to_string(),
        generate_hybrid_view(result),
        "```".to_string(),
        String::new(),
        "## Table View".to_string(),
        "```mermaid".to_string(),
        generate_table_view(result),
        "```".to_string(),
        String::new(),
        "## Column View".to_string(),
        "```mermaid".to_string(),
        generate_column_view(result),
        "```".to_string(),
    ];

    sections.join("\n")
}

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

fn escape_label(label: &str) -> String {
    label.replace('"', "\\\"").replace('\n', " ")
}

#[derive(Debug)]
struct ScriptInfo {
    source_name: String,
    tables_read: HashSet<Arc<str>>,
    tables_written: HashSet<Arc<str>>,
}

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
            if matches!(node.node_type, NodeType::Table | NodeType::View) {
                let is_written = stmt
                    .edges
                    .iter()
                    .any(|edge| edge.to == node.id && edge.edge_type == EdgeType::DataFlow);
                let is_read = stmt
                    .edges
                    .iter()
                    .any(|edge| edge.from == node.id && edge.edge_type == EdgeType::DataFlow);

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

fn generate_script_view(result: &AnalyzeResult) -> String {
    let scripts = extract_script_info(result);
    let mut lines = vec!["flowchart LR".to_string()];

    for script in &scripts {
        let id = sanitize_id(&script.source_name);
        let label = escape_label(&script.source_name);
        lines.push(format!("    {id}[\"{label}\"]"));
    }

    for producer in &scripts {
        for consumer in &scripts {
            if producer.source_name == consumer.source_name {
                continue;
            }

            let shared_tables: Vec<_> = producer
                .tables_written
                .iter()
                .filter(|table| consumer.tables_read.contains(*table))
                .collect();

            if !shared_tables.is_empty() {
                let producer_id = sanitize_id(&producer.source_name);
                let consumer_id = sanitize_id(&consumer.source_name);
                let label = if shared_tables.len() > 3 {
                    let first_three: Vec<_> = shared_tables.iter().take(3).collect();
                    format!(
                        "{}...",
                        first_three
                            .iter()
                            .map(|value| value.as_ref())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                } else {
                    shared_tables
                        .iter()
                        .map(|value| value.as_ref())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                lines.push(format!(
                    "    {producer_id} -->|\"{}\"| {consumer_id}",
                    escape_label(&label)
                ));
            }
        }
    }

    lines.join("\n")
}

fn generate_table_view(result: &AnalyzeResult) -> String {
    let mut lines = vec!["flowchart LR".to_string()];
    let mut table_ids: HashMap<String, String> = HashMap::new();
    let mut edges = HashSet::new();

    for stmt in &result.statements {
        let table_nodes: Vec<_> = stmt
            .nodes
            .iter()
            .filter(|node| node.node_type.is_table_like())
            .collect();

        for node in &table_nodes {
            let key = node
                .qualified_name
                .as_deref()
                .unwrap_or(&node.label)
                .to_string();
            if !table_ids.contains_key(&key) {
                let id = sanitize_id(&key);
                table_ids.insert(key.clone(), id.clone());
                let escaped_label = escape_label(&node.label);
                let shape = match node.node_type {
                    NodeType::Cte => format!("([\"{escaped_label}\"])"),
                    NodeType::View => format!("[/\"{escaped_label}\"/]"),
                    _ => format!("[\"{escaped_label}\"]"),
                };
                lines.push(format!("    {id}{shape}"));
            }
        }

        for edge in &stmt.edges {
            if edge.edge_type == EdgeType::DataFlow || edge.edge_type == EdgeType::Derivation {
                let source_node = table_nodes.iter().find(|node| node.id == edge.from);
                let target_node = table_nodes.iter().find(|node| node.id == edge.to);

                if let (Some(source), Some(target)) = (source_node, target_node) {
                    let source_key = source
                        .qualified_name
                        .as_deref()
                        .unwrap_or(&source.label)
                        .to_string();
                    let target_key = target
                        .qualified_name
                        .as_deref()
                        .unwrap_or(&target.label)
                        .to_string();
                    let edge_key = format!("{source_key}->{target_key}");

                    if source_key != target_key && edges.insert(edge_key) {
                        let source_id = table_ids.get(&source_key).cloned().unwrap_or_else(|| {
                            let id = sanitize_id(&source_key);
                            table_ids.insert(source_key.clone(), id.clone());
                            id
                        });
                        let target_id = table_ids.get(&target_key).cloned().unwrap_or_else(|| {
                            let id = sanitize_id(&target_key);
                            table_ids.insert(target_key.clone(), id.clone());
                            id
                        });
                        lines.push(format!("    {source_id} --> {target_id}"));
                    }
                }
            }
        }
    }

    lines.join("\n")
}

#[derive(Debug)]
struct ColumnMapping {
    source_table: String,
    source_column: String,
    target_table: String,
    target_column: String,
    edge_type: EdgeType,
}

fn extract_column_mappings(result: &AnalyzeResult) -> Vec<ColumnMapping> {
    let mut mappings = Vec::new();

    for stmt in &result.statements {
        let table_nodes: Vec<_> = stmt
            .nodes
            .iter()
            .filter(|node| node.node_type.is_table_like())
            .collect();
        let column_nodes: Vec<_> = stmt
            .nodes
            .iter()
            .filter(|node| node.node_type == NodeType::Column)
            .collect();

        let mut column_to_table: HashMap<&str, &str> = HashMap::new();
        for edge in &stmt.edges {
            if edge.edge_type == EdgeType::Ownership {
                if let Some(table_node) = table_nodes.iter().find(|node| node.id == edge.from) {
                    let table_name = table_node
                        .qualified_name
                        .as_deref()
                        .unwrap_or(&table_node.label);
                    column_to_table.insert(edge.to.as_ref(), table_name);
                }
            }
        }

        for edge in &stmt.edges {
            if edge.edge_type == EdgeType::Derivation || edge.edge_type == EdgeType::DataFlow {
                let source_col = column_nodes.iter().find(|col| col.id == edge.from);
                let target_col = column_nodes.iter().find(|col| col.id == edge.to);

                if let (Some(source), Some(target)) = (source_col, target_col) {
                    let source_table = column_to_table
                        .get(edge.from.as_ref())
                        .copied()
                        .unwrap_or("Output");
                    let target_table = column_to_table
                        .get(edge.to.as_ref())
                        .copied()
                        .unwrap_or("Output");

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

fn generate_column_view(result: &AnalyzeResult) -> String {
    let mut lines = vec!["flowchart LR".to_string()];
    let mappings = extract_column_mappings(result);
    let mut nodes = HashSet::new();
    let mut edges = HashSet::new();

    for mapping in mappings {
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

        if nodes.insert(source_id.clone()) {
            lines.push(format!(
                "    {source_id}[\"{}\"]",
                escape_label(&source_label)
            ));
        }
        if nodes.insert(target_id.clone()) {
            lines.push(format!(
                "    {target_id}[\"{}\"]",
                escape_label(&target_label)
            ));
        }

        let edge_key = format!("{source_id}->{target_id}");
        if edges.insert(edge_key) {
            let edge_label = match mapping.edge_type {
                EdgeType::Derivation => "derived",
                _ => "flows",
            };
            lines.push(format!("    {source_id} -->|{edge_label}| {target_id}"));
        }
    }

    lines.join("\n")
}

fn generate_hybrid_view(result: &AnalyzeResult) -> String {
    let mut lines = vec!["flowchart LR".to_string()];
    let scripts = extract_script_info(result);

    let mut script_ids = HashMap::new();
    for script in &scripts {
        let id = sanitize_id(&format!("script_{}", script.source_name));
        script_ids.insert(script.source_name.clone(), id.clone());
        lines.push(format!(
            "    {id}{{\"{}\"}}",
            escape_label(&script.source_name)
        ));
    }

    let mut table_ids = HashMap::new();
    for stmt in &result.statements {
        for node in &stmt.nodes {
            if node.node_type.is_table_like() {
                let key = node
                    .qualified_name
                    .as_deref()
                    .unwrap_or(&node.label)
                    .to_string();
                if !table_ids.contains_key(&key) {
                    let id = sanitize_id(&format!("table_{}", key));
                    table_ids.insert(key.clone(), id.clone());
                    lines.push(format!("    {id}[\"{}\"]", escape_label(&node.label)));
                }
            }
        }
    }

    for script in scripts {
        let script_id = script_ids
            .get(&script.source_name)
            .cloned()
            .unwrap_or_else(|| {
                let id = sanitize_id(&format!("script_{}", script.source_name));
                script_ids.insert(script.source_name.clone(), id.clone());
                id
            });

        for table in &script.tables_read {
            if let Some(table_id) = table_ids.get(table.as_ref()) {
                lines.push(format!("    {script_id} --> {table_id}"));
            }
        }
        for table in &script.tables_written {
            if let Some(table_id) = table_ids.get(table.as_ref()) {
                lines.push(format!("    {table_id} --> {script_id}"));
            }
        }
    }

    lines.join("\n")
}
