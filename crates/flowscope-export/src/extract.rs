use std::collections::{BTreeMap, BTreeSet, HashMap};

use flowscope_core::{AnalyzeResult, EdgeType, NodeType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScriptInfo {
    pub source_name: String,
    pub statement_count: usize,
    pub tables_read: Vec<String>,
    pub tables_written: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub qualified_name: String,
    #[serde(rename = "type")]
    pub table_type: TableType,
    pub columns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TableType {
    Table,
    View,
    Cte,
}

impl TableType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TableType::Table => "table",
            TableType::View => "view",
            TableType::Cte => "cte",
        }
    }
}

impl std::fmt::Display for TableType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMapping {
    pub source_table: String,
    pub source_column: String,
    pub target_table: String,
    pub target_column: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    pub edge_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TableDependency {
    pub source_table: String,
    pub target_table: String,
}

pub fn extract_script_info(result: &AnalyzeResult) -> Vec<ScriptInfo> {
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
                statement_count: 0,
                tables_read: Vec::new(),
                tables_written: Vec::new(),
            });

        entry.statement_count += 1;

        let mut tables_read: BTreeSet<String> = entry.tables_read.iter().cloned().collect();
        let mut tables_written: BTreeSet<String> = entry.tables_written.iter().cloned().collect();

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
                    .as_deref()
                    .unwrap_or(&node.label)
                    .to_string();

                if is_written {
                    tables_written.insert(table_name.clone());
                }
                // A table is considered "read" if it's explicitly read OR if it's
                // referenced but not written (implying it's an external/source table)
                if is_read || !is_written {
                    tables_read.insert(table_name);
                }
            }
        }

        entry.tables_read = tables_read.into_iter().collect();
        entry.tables_written = tables_written.into_iter().collect();
    }

    let mut values: Vec<_> = script_map.into_values().collect();
    values.sort_by(|a, b| a.source_name.cmp(&b.source_name));
    values
}

pub fn extract_table_info(result: &AnalyzeResult) -> Vec<TableInfo> {
    let mut table_map: BTreeMap<String, TableInfo> = BTreeMap::new();

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

        for table_node in table_nodes {
            let key = table_node
                .qualified_name
                .as_deref()
                .unwrap_or(&table_node.label)
                .to_string();

            let owned_column_ids: BTreeSet<_> = stmt
                .edges
                .iter()
                .filter(|edge| edge.edge_type == EdgeType::Ownership && edge.from == table_node.id)
                .map(|edge| edge.to.as_ref())
                .collect();

            let columns: BTreeSet<String> = column_nodes
                .iter()
                .filter(|col| owned_column_ids.contains(col.id.as_ref()))
                .map(|col| col.label.to_string())
                .collect();

            let table_type = match table_node.node_type {
                NodeType::View => TableType::View,
                NodeType::Cte => TableType::Cte,
                _ => TableType::Table,
            };

            let entry = table_map.entry(key.clone()).or_insert_with(|| TableInfo {
                name: table_node.label.to_string(),
                qualified_name: key.clone(),
                table_type,
                columns: Vec::new(),
                source_name: stmt.source_name.clone(),
            });

            let mut merged: BTreeSet<String> = entry.columns.iter().cloned().collect();
            merged.extend(columns);
            entry.columns = merged.into_iter().collect();
        }
    }

    table_map.into_values().collect()
}

pub fn extract_column_mappings(result: &AnalyzeResult) -> Vec<ColumnMapping> {
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

                    let expression = edge
                        .expression
                        .as_ref()
                        .map(|value| value.to_string())
                        .or_else(|| target.expression.as_ref().map(|value| value.to_string()));

                    mappings.push(ColumnMapping {
                        source_table: source_table.to_string(),
                        source_column: source.label.to_string(),
                        target_table: target_table.to_string(),
                        target_column: target.label.to_string(),
                        expression,
                        edge_type: edge_type_label(edge.edge_type).to_string(),
                    });
                }
            }
        }
    }

    mappings
}

pub fn extract_table_dependencies(result: &AnalyzeResult) -> Vec<TableDependency> {
    let mut dependencies = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for stmt in &result.statements {
        let relation_nodes: Vec<_> = stmt
            .nodes
            .iter()
            .filter(|node| node.node_type.is_relation())
            .collect();

        for edge in &stmt.edges {
            if edge.edge_type == EdgeType::DataFlow || edge.edge_type == EdgeType::JoinDependency {
                let source_node = relation_nodes.iter().find(|node| node.id == edge.from);
                let target_node = relation_nodes.iter().find(|node| node.id == edge.to);

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
                    let dep_key = format!("{source_key}->{target_key}");

                    if source_key != target_key && seen.insert(dep_key) {
                        dependencies.push(TableDependency {
                            source_table: source_key,
                            target_table: target_key,
                        });
                    }
                }
            }
        }
    }

    dependencies
}

fn edge_type_label(edge_type: EdgeType) -> &'static str {
    match edge_type {
        EdgeType::Ownership => "ownership",
        EdgeType::DataFlow => "data_flow",
        EdgeType::Derivation => "derivation",
        EdgeType::JoinDependency => "join_dependency",
        EdgeType::CrossStatement => "cross_statement",
    }
}
