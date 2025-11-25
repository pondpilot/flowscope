use flowscope_core::AnalyzeResult;

/// Helper to prepare result for snapshotting by removing volatile fields and sorting lists
pub fn prepare_for_snapshot(mut result: AnalyzeResult) -> AnalyzeResult {
    // 1. Clear timestamps in resolved schema
    if let Some(ref mut schema) = result.resolved_schema {
        for table in &mut schema.tables {
            table.updated_at = "2024-01-01T00:00:00Z".to_string();
        }
    }

    // 2. Clear spans and sort nodes/edges in statements
    for stmt in &mut result.statements {
        stmt.span = None;
        for node in &mut stmt.nodes {
            node.span = None;
        }
        // Sort nodes by ID for deterministic output
        stmt.nodes.sort_by(|a, b| a.id.cmp(&b.id));
        stmt.edges.sort_by(|a, b| a.id.cmp(&b.id));
    }

    // 3. Sort global lineage
    result.global_lineage.nodes.sort_by(|a, b| a.id.cmp(&b.id));
    result.global_lineage.edges.sort_by(|a, b| a.id.cmp(&b.id));

    // 4. Sort issues
    for issue in &mut result.issues {
        issue.span = None;
    }
    // Sort issues by code then message
    result
        .issues
        .sort_by(|a, b| a.code.cmp(&b.code).then_with(|| a.message.cmp(&b.message)));

    result
}
