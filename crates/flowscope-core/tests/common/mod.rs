use flowscope_core::AnalyzeResult;

/// Helper to prepare result for snapshotting by removing volatile fields and sorting lists
pub fn prepare_for_snapshot(mut result: AnalyzeResult) -> AnalyzeResult {
    // 1. Clear timestamps in resolved schema
    if let Some(ref mut schema) = result.resolved_schema {
        for table in &mut schema.tables {
            table.updated_at = "2024-01-01T00:00:00Z".to_string();
        }
    }

    // 2. Clear per-statement spans
    for stmt in &mut result.statements {
        stmt.span = None;
    }

    // 3. Clear node spans and sort the flat graph for deterministic output.
    for node in &mut result.nodes {
        node.span = None;
        node.name_spans.clear();
        node.body_span = None;
    }
    result.nodes.sort_by(|a, b| a.id.cmp(&b.id));
    result.edges.sort_by(|a, b| a.id.cmp(&b.id));

    // 4. Sort issues
    for issue in &mut result.issues {
        issue.span = None;
    }
    result
        .issues
        .sort_by(|a, b| a.code.cmp(&b.code).then_with(|| a.message.cmp(&b.message)));

    result
}
