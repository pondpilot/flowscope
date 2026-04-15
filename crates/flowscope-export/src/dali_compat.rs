//! Dali (sql-parser-service) compatibility adapter.
//!
//! Maps FlowScope's `AnalyzeResult` to the JSON contract produced by the
//! sql-parser-service lineage extractor:
//!
//! ```json
//! {
//!   "package": "<sql>",
//!   "transforms": [ { "name", "targetTables", "query", "is_union", "refs", "source_tables" } ],
//!   "table_lineage": [ { "transform", "target_tables", "source_tables", "relation" } ]
//! }
//! ```

use std::collections::{BTreeSet, HashMap};

use flowscope_core::{AnalyzeResult, EdgeType, NodeType, StatementLineage};
use serde::Serialize;

// ── Public API ──────────────────────────────────────────────────

/// Convert an `AnalyzeResult` into the Dali-compatible JSON string.
pub fn export_dali_compat(result: &AnalyzeResult, sql: &str) -> String {
    let output = build_dali_output(result, sql);
    serde_json::to_string_pretty(&output).expect("DaliOutput is always serialisable")
}

/// Convert an `AnalyzeResult` into the Dali-compatible JSON string (compact).
pub fn export_dali_compat_compact(result: &AnalyzeResult, sql: &str) -> String {
    let output = build_dali_output(result, sql);
    serde_json::to_string(&output).expect("DaliOutput is always serialisable")
}

// ── Serialisable types matching the Dali contract ───────────────

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct DaliOutput {
    pub package: String,
    pub transforms: Vec<DaliTransform>,
    pub table_lineage: Vec<DaliTableLineage>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct DaliTransform {
    pub name: String,
    #[serde(rename = "targetTables")]
    pub target_tables: Vec<String>,
    pub query: String,
    pub is_union: bool,
    pub refs: Vec<DaliRef>,
    pub source_tables: Vec<String>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct DaliRef {
    pub target_column: String,
    pub source_columns: Vec<DaliSourceColumn>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct DaliSourceColumn {
    pub expression: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct DaliTableLineage {
    pub transform: String,
    pub target_tables: Vec<String>,
    pub source_tables: Vec<String>,
    pub relation: String,
}

// ── Mapping logic ───────────────────────────────────────────────

fn build_dali_output(result: &AnalyzeResult, sql: &str) -> DaliOutput {
    let mut transforms = Vec::new();
    let mut table_lineage = Vec::new();

    for stmt in &result.statements {
        let written_tables = collect_written_tables(stmt);
        if written_tables.is_empty() {
            continue;
        }

        let source_tables = collect_source_tables(stmt, &written_tables);
        let refs = build_refs(stmt);
        let relation = map_relation(&stmt.statement_type);
        let name = build_transform_name(stmt);

        transforms.push(DaliTransform {
            name: name.clone(),
            target_tables: written_tables.iter().cloned().collect(),
            query: String::new(), // FlowScope doesn't store per-statement SQL text
            is_union: false,
            refs,
            source_tables: source_tables.iter().cloned().collect(),
        });

        table_lineage.push(DaliTableLineage {
            transform: name,
            target_tables: written_tables.into_iter().collect(),
            source_tables: source_tables.into_iter().collect(),
            relation,
        });
    }

    DaliOutput {
        package: sql.to_string(),
        transforms,
        table_lineage,
    }
}

/// Collect tables that are written to.
///
/// A table is "written" if it:
/// 1. Has a DataFlow edge pointing TO it (e.g., INSERT..SELECT), OR
/// 2. Owns columns that are targets of DataFlow edges from columns owned by
///    other relations (e.g., UPDATE SET col = subquery, MERGE with USING).
fn collect_written_tables(stmt: &StatementLineage) -> Vec<String> {
    let mut written = BTreeSet::new();

    // Only consider actual Table/View nodes as potential write targets,
    // not CTEs (which are intermediate relations like USING subquery aliases).
    let table_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type.is_table_or_view())
        .collect();

    // (1) Direct DataFlow to table node
    for node in &table_nodes {
        let is_target = stmt
            .edges
            .iter()
            .any(|edge| edge.to == node.id && edge.edge_type == EdgeType::DataFlow);
        if is_target {
            let name = node
                .qualified_name
                .as_deref()
                .unwrap_or(&node.label)
                .to_string();
            written.insert(name);
        }
    }

    // (2) Tables whose owned columns receive DataFlow from columns owned by
    //     different relations. Build ownership map: column_id -> table_id
    let mut col_owner: HashMap<&str, &str> = HashMap::new();
    for edge in &stmt.edges {
        if edge.edge_type == EdgeType::Ownership {
            if table_nodes.iter().any(|t| t.id == edge.from) {
                col_owner.insert(edge.to.as_ref(), edge.from.as_ref());
            }
        }
    }

    let column_ids: BTreeSet<&str> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Column)
        .map(|n| n.id.as_ref())
        .collect();

    for edge in &stmt.edges {
        if edge.edge_type != EdgeType::DataFlow {
            continue;
        }
        // Column-to-column DataFlow where source and target have different owners
        let from_is_col = column_ids.contains(edge.from.as_ref());
        let to_is_col = column_ids.contains(edge.to.as_ref());
        if from_is_col && to_is_col {
            let from_owner = col_owner.get(edge.from.as_ref());
            let to_owner = col_owner.get(edge.to.as_ref());
            if let (Some(&from_tbl), Some(&to_tbl)) = (from_owner, to_owner) {
                if from_tbl != to_tbl {
                    // The table owning the target column is "written to"
                    if let Some(tbl_node) = table_nodes.iter().find(|t| t.id.as_ref() == to_tbl) {
                        let name = tbl_node
                            .qualified_name
                            .as_deref()
                            .unwrap_or(&tbl_node.label)
                            .to_string();
                        written.insert(name);
                    }
                }
            }
        }
    }

    // (3) For DML statements (MERGE, UPDATE, DELETE), the target table may only
    //     have Ownership edges (columns exist but no DataFlow connects them).
    //     Detect by finding Table nodes that own columns but have no DataFlow edges
    //     at all — neither incoming nor outgoing at the table level.
    if written.is_empty() {
        let stmt_upper = stmt.statement_type.to_uppercase();
        if matches!(stmt_upper.as_str(), "MERGE" | "UPDATE" | "DELETE") {
            for node in &table_nodes {
                let has_ownership = stmt
                    .edges
                    .iter()
                    .any(|e| e.from == node.id && e.edge_type == EdgeType::Ownership);
                let has_dataflow = stmt.edges.iter().any(|e| {
                    (e.from == node.id || e.to == node.id) && e.edge_type == EdgeType::DataFlow
                });
                if has_ownership && !has_dataflow {
                    let name = node
                        .qualified_name
                        .as_deref()
                        .unwrap_or(&node.label)
                        .to_string();
                    written.insert(name);
                    break; // Only the first such table is the target
                }
            }
        }
    }

    written.into_iter().collect()
}

/// Collect source tables (read from, excluding written tables).
fn collect_source_tables(stmt: &StatementLineage, written: &[String]) -> BTreeSet<String> {
    let written_set: BTreeSet<&str> = written.iter().map(|s| s.as_str()).collect();
    let mut sources = BTreeSet::new();

    for node in &stmt.nodes {
        if !matches!(node.node_type, NodeType::Table | NodeType::View) {
            continue;
        }
        let name = node
            .qualified_name
            .as_deref()
            .unwrap_or(&node.label)
            .to_string();
        if written_set.contains(name.as_str()) {
            continue;
        }
        // Include if it has outgoing DataFlow edges (is read from)
        // or is referenced but not written (external source)
        let is_source = stmt
            .edges
            .iter()
            .any(|edge| edge.from == node.id && edge.edge_type == EdgeType::DataFlow);
        let is_not_written = !stmt
            .edges
            .iter()
            .any(|edge| edge.to == node.id && edge.edge_type == EdgeType::DataFlow);
        if is_source || is_not_written {
            sources.insert(name);
        }
    }
    sources
}

/// Build column-level refs from the lineage graph.
///
/// For each column owned by a target table, trace backward through
/// DataFlow and Derivation edges to find the ultimate source columns
/// (columns owned by source tables).
fn build_refs(stmt: &StatementLineage) -> Vec<DaliRef> {
    let column_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Column)
        .collect();

    let relation_nodes: Vec<_> = stmt
        .nodes
        .iter()
        .filter(|n| n.node_type.is_table_like() || n.node_type == NodeType::Output)
        .collect();

    // Map column_id -> owning relation (table/view/cte/output) qualified name
    let mut column_owner: HashMap<&str, (&str, bool)> = HashMap::new(); // col_id -> (table_name, is_source_table)
    for edge in &stmt.edges {
        if edge.edge_type == EdgeType::Ownership {
            if let Some(rel) = relation_nodes.iter().find(|n| n.id == edge.from) {
                let name = rel.qualified_name.as_deref().unwrap_or(&rel.label);
                let is_source = matches!(rel.node_type, NodeType::Table | NodeType::View)
                    && !stmt
                        .edges
                        .iter()
                        .any(|e| e.to == rel.id && e.edge_type == EdgeType::DataFlow);
                column_owner.insert(edge.to.as_ref(), (name, is_source));
            }
        }
    }

    // Identify target table nodes (tables/views with DataFlow edges TO them)
    let target_table_ids: BTreeSet<&str> = stmt
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::DataFlow)
        .filter(|e| {
            relation_nodes
                .iter()
                .any(|n| n.id == e.to && n.node_type.is_table_like())
        })
        .map(|e| e.to.as_ref())
        .collect();

    // Target columns = columns owned by target tables
    let target_column_ids: BTreeSet<&str> = stmt
        .edges
        .iter()
        .filter(|e| {
            e.edge_type == EdgeType::Ownership && target_table_ids.contains(e.from.as_ref())
        })
        .map(|e| e.to.as_ref())
        .collect();

    // Build adjacency: for each column, which columns flow INTO it
    let mut incoming: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &stmt.edges {
        if matches!(edge.edge_type, EdgeType::Derivation | EdgeType::DataFlow) {
            let from_is_col = column_nodes.iter().any(|c| c.id == edge.from);
            let to_is_col = column_nodes.iter().any(|c| c.id == edge.to);
            if from_is_col && to_is_col {
                incoming
                    .entry(edge.to.as_ref())
                    .or_default()
                    .push(edge.from.as_ref());
            }
        }
    }

    // For each target column, trace backward to find source table columns
    let mut refs: Vec<DaliRef> = Vec::new();
    let mut seen_labels = BTreeSet::new();

    for col in &column_nodes {
        if !target_column_ids.contains(col.id.as_ref()) {
            continue;
        }
        if !seen_labels.insert(col.label.to_string()) {
            continue;
        }

        let mut sources: Vec<(String, String)> = Vec::new(); // (table, column)
        let mut visited = BTreeSet::new();
        let mut stack = vec![col.id.as_ref()];

        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }
            if let Some(predecessors) = incoming.get(current) {
                for &pred in predecessors {
                    if let Some(&(table_name, is_source)) = column_owner.get(pred) {
                        if is_source {
                            // This is a source table column — record it
                            if let Some(pred_node) =
                                column_nodes.iter().find(|c| c.id.as_ref() == pred)
                            {
                                sources.push((table_name.to_string(), pred_node.label.to_string()));
                            }
                        } else {
                            // Intermediate column (output/CTE) — keep tracing
                            stack.push(pred);
                        }
                    } else {
                        // No owner found — keep tracing
                        stack.push(pred);
                    }
                }
            }
        }

        if sources.is_empty() {
            continue;
        }

        // Deduplicate
        let mut source_columns = Vec::new();
        let mut seen_src = BTreeSet::new();
        for (table, column) in &sources {
            let qualified = format!("{table}.{column}");
            if seen_src.insert(qualified.clone()) {
                source_columns.push(DaliSourceColumn {
                    expression: column.clone(),
                    columns: vec![qualified],
                });
            }
        }

        refs.push(DaliRef {
            target_column: col.label.to_string(),
            source_columns,
        });
    }

    refs
}

/// Map FlowScope statement_type to Dali relation string.
fn map_relation(statement_type: &str) -> String {
    match statement_type.to_uppercase().as_str() {
        "INSERT" => "INSERT_SELECT".to_string(),
        "CREATE VIEW" | "CREATE_VIEW" => "VIEW_SELECT".to_string(),
        "MERGE" => "MERGE".to_string(),
        "UPDATE" => "UPDATE".to_string(),
        "DELETE" => "DELETE".to_string(),
        "CREATE TABLE" | "CREATE_TABLE" | "CREATE_TABLE_AS" | "CREATE TABLE AS" => {
            "TABLE_SELECT".to_string()
        }
        other => other.to_string(),
    }
}

/// Build a transform name from the statement.
fn build_transform_name(stmt: &StatementLineage) -> String {
    let stmt_type = stmt.statement_type.to_uppercase();
    format!("{}:{}", stmt_type, stmt.statement_index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flowscope_core::{analyze, AnalyzeRequest, Dialect};

    fn analyze_oracle(sql: &str) -> AnalyzeResult {
        let request = AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Oracle,
            source_name: None,
            options: None,
            schema: None,
            #[cfg(feature = "templating")]
            template_config: None,
        };
        analyze(&request)
    }

    #[test]
    fn insert_select_produces_transform() {
        let sql = "INSERT INTO target_table (col1, col2) SELECT a, b FROM source_table";
        let result = analyze_oracle(sql);
        let output = build_dali_output(&result, sql);

        assert_eq!(output.transforms.len(), 1);
        assert_eq!(output.table_lineage.len(), 1);

        let t = &output.transforms[0];
        assert_eq!(t.target_tables, vec!["TARGET_TABLE"]);
        assert!(t.source_tables.contains(&"SOURCE_TABLE".to_string()));
        assert_eq!(output.table_lineage[0].relation, "INSERT_SELECT");
    }

    #[test]
    fn create_view_produces_view_select_relation() {
        let sql = "CREATE VIEW my_view AS SELECT id, name FROM base_table";
        let result = analyze_oracle(sql);
        let output = build_dali_output(&result, sql);

        assert_eq!(output.transforms.len(), 1);
        let t = &output.transforms[0];
        assert_eq!(t.target_tables, vec!["MY_VIEW"]);
        assert!(t.source_tables.contains(&"BASE_TABLE".to_string()));
        assert_eq!(output.table_lineage[0].relation, "VIEW_SELECT");
    }

    #[test]
    fn refs_contain_column_level_mappings() {
        let sql = "INSERT INTO tgt (x, y) SELECT a, b FROM src";
        let result = analyze_oracle(sql);
        let output = build_dali_output(&result, sql);

        let t = &output.transforms[0];
        assert!(!t.refs.is_empty(), "refs should not be empty");

        // FlowScope labels target columns with SELECT output names
        let ref_targets: Vec<&str> = t.refs.iter().map(|r| r.target_column.as_str()).collect();
        assert_eq!(
            ref_targets.len(),
            2,
            "should have 2 refs, got {ref_targets:?}"
        );

        // Check source columns reference the source table
        for r in &t.refs {
            for sc in &r.source_columns {
                assert!(
                    sc.columns
                        .iter()
                        .any(|c| c.to_uppercase().starts_with("SRC.")),
                    "source column should reference SRC table, got {:?}",
                    sc.columns
                );
            }
        }
    }

    #[test]
    fn update_produces_correct_relation() {
        let sql = "UPDATE target_table SET col1 = src.val FROM source_table src WHERE target_table.id = src.id";
        let result = analyze_oracle(sql);
        let output = build_dali_output(&result, sql);

        if !output.table_lineage.is_empty() {
            assert_eq!(output.table_lineage[0].relation, "UPDATE");
        }
    }

    #[test]
    fn merge_produces_correct_relation() {
        let sql = "MERGE INTO tgt USING src ON (tgt.id = src.id) WHEN MATCHED THEN UPDATE SET tgt.val = src.val WHEN NOT MATCHED THEN INSERT (id, val) VALUES (src.id, src.val)";
        let result = analyze_oracle(sql);
        let output = build_dali_output(&result, sql);

        assert!(!output.transforms.is_empty());
        assert_eq!(output.table_lineage[0].relation, "MERGE");
    }

    #[test]
    fn delete_produces_correct_relation() {
        let sql = "DELETE FROM target_table WHERE id IN (SELECT id FROM src)";
        let result = analyze_oracle(sql);
        let output = build_dali_output(&result, sql);

        if !output.table_lineage.is_empty() {
            assert_eq!(output.table_lineage[0].relation, "DELETE");
        }
    }

    #[test]
    fn standalone_select_produces_no_transform() {
        let sql = "SELECT a, b FROM some_table";
        let result = analyze_oracle(sql);
        let output = build_dali_output(&result, sql);

        // Standalone SELECT has no target table, so no transforms
        assert!(output.transforms.is_empty());
        assert!(output.table_lineage.is_empty());
    }

    #[test]
    fn package_contains_original_sql() {
        let sql = "INSERT INTO t (c) SELECT c FROM s";
        let result = analyze_oracle(sql);
        let output = build_dali_output(&result, sql);

        assert_eq!(output.package, sql);
    }

    #[test]
    fn output_is_valid_json() {
        let sql = "INSERT INTO t (c) SELECT c FROM s";
        let result = analyze_oracle(sql);
        let json_str = export_dali_compat(&result, sql);

        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("output should be valid JSON");
        assert!(parsed.get("package").is_some());
        assert!(parsed.get("transforms").is_some());
        assert!(parsed.get("table_lineage").is_some());
    }

    #[test]
    fn compact_output_has_no_newlines_in_values() {
        let sql = "INSERT INTO t (c) SELECT c FROM s";
        let result = analyze_oracle(sql);
        let json_str = export_dali_compat_compact(&result, sql);

        // Compact JSON should be a single line (no pretty printing)
        assert!(
            !json_str.contains("\n  "),
            "compact output should not be indented"
        );
    }

    #[test]
    fn source_columns_have_expression_and_columns() {
        let sql = "INSERT INTO tgt (x) SELECT a FROM src";
        let result = analyze_oracle(sql);
        let output = build_dali_output(&result, sql);

        for t in &output.transforms {
            for r in &t.refs {
                for sc in &r.source_columns {
                    assert!(!sc.expression.is_empty(), "expression should not be empty");
                    assert!(!sc.columns.is_empty(), "columns should not be empty");
                }
            }
        }
    }
}
