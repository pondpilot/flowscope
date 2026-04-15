//! DuckDB backend implementation.

use crate::join_export::representative_join_edge_ids;
use crate::schema::{tables_ddl, views_ddl};
use crate::ExportError;
use duckdb::{params, Connection};
use flowscope_core::AnalyzeResult;
use std::collections::HashMap;
use std::fs;
use tempfile::NamedTempFile;

/// Export analysis result to DuckDB database bytes.
pub fn export(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    // Create temp file for database path, then remove it so DuckDB can create fresh
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_path_buf();
    drop(temp_file); // Close and remove the empty file

    // Create database and connection
    let conn = Connection::open(&db_path)?;

    // Create schema
    create_schema(&conn)?;

    // Write data
    write_data(&conn, result)?;

    // Close connection before reading
    drop(conn);

    // Read file bytes
    let bytes = fs::read(&db_path)?;

    // Clean up the database file
    let _ = fs::remove_file(&db_path);

    Ok(bytes)
}

fn create_schema(conn: &Connection) -> Result<(), ExportError> {
    // Execute table DDL (no prefix for standalone DuckDB file)
    conn.execute_batch(&tables_ddl(""))?;

    // Execute view DDL
    conn.execute_batch(&views_ddl(""))?;

    Ok(())
}

fn write_data(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    write_meta(conn)?;
    let statement_row_ids = write_statements(conn, result)?;
    write_nodes(conn, result, &statement_row_ids)?;
    write_edges(conn, result, &statement_row_ids)?;
    write_issues(conn, result, &statement_row_ids)?;
    write_schema_tables(conn, result)?;
    Ok(())
}

/// Schema version for the export format.
/// Increment this when making breaking changes to the schema structure.
const SCHEMA_VERSION: &str = "2";

fn write_meta(conn: &Connection) -> Result<(), ExportError> {
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES (?, ?)",
        params!["schema_version", SCHEMA_VERSION],
    )?;
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES (?, ?)",
        params!["version", env!("CARGO_PKG_VERSION")],
    )?;
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES (?, ?)",
        params!["exported_at", chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

fn write_statements(
    conn: &Connection,
    result: &AnalyzeResult,
) -> Result<HashMap<usize, i64>, ExportError> {
    let mut stmt = conn.prepare(
        "INSERT INTO statements (id, statement_index, statement_type, source_name, span_start, span_end, join_count, complexity_score)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )?;

    let mut statement_row_ids = HashMap::with_capacity(result.statements.len());
    for (idx, s) in result.statements.iter().enumerate() {
        statement_row_ids.insert(s.statement_index, idx as i64);
        let (span_start, span_end) = s
            .span
            .map(|sp| (Some(sp.start as i64), Some(sp.end as i64)))
            .unwrap_or((None, None));
        stmt.execute(params![
            idx as i64,
            s.statement_index as i64,
            &s.statement_type,
            &s.source_name,
            span_start,
            span_end,
            s.join_count as i64,
            s.complexity_score as i64,
        ])?;
    }
    Ok(statement_row_ids)
}

fn statement_row_id(
    statement_row_ids: &HashMap<usize, i64>,
    statement_index: usize,
) -> Result<i64, ExportError> {
    statement_row_ids.get(&statement_index).copied().ok_or_else(|| {
        ExportError::Serialization(format!(
            "statement index {statement_index} is referenced by the graph but missing from result.statements"
        ))
    })
}

fn write_nodes(
    conn: &Connection,
    result: &AnalyzeResult,
    statement_row_ids: &HashMap<usize, i64>,
) -> Result<(), ExportError> {
    let mut node_stmt = conn.prepare(
        "INSERT INTO nodes (id, node_type, label, qualified_name, canonical_catalog, canonical_schema, canonical_name, canonical_column, expression, span_start, span_end, body_span_start, body_span_end, resolution_source)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )?;

    let mut node_stmt_ref =
        conn.prepare("INSERT INTO node_statements (node_id, statement_id) VALUES (?, ?)")?;

    let mut name_span_stmt = conn.prepare(
        "INSERT INTO node_name_spans (id, node_id, span_start, span_end) VALUES (?, ?, ?, ?)",
    )?;

    let mut filter_stmt = conn.prepare(
        "INSERT INTO filters (id, node_id, statement_id, predicate, filter_type) VALUES (?, ?, ?, ?, ?)",
    )?;

    let mut agg_stmt = conn.prepare(
        "INSERT INTO aggregations (node_id, statement_id, is_grouping_key, function, is_distinct) VALUES (?, ?, ?, ?, ?)",
    )?;

    let mut filter_id: i64 = 0;
    let mut name_span_id: i64 = 0;

    for node in &result.nodes {
        let (span_start, span_end) = node
            .span
            .map(|sp| (Some(sp.start as i64), Some(sp.end as i64)))
            .unwrap_or((None, None));
        let (body_start, body_end) = node
            .body_span
            .map(|sp| (Some(sp.start as i64), Some(sp.end as i64)))
            .unwrap_or((None, None));
        let node_type = format!("{:?}", node.node_type).to_lowercase();
        let resolution = node
            .resolution_source
            .map(|r| format!("{:?}", r).to_lowercase());

        node_stmt.execute(params![
            node.id.as_ref(),
            node_type,
            node.label.as_ref(),
            node.qualified_name.as_ref().map(|s| s.as_ref()),
            node.canonical_name
                .as_ref()
                .and_then(|c| c.catalog.as_deref()),
            node.canonical_name
                .as_ref()
                .and_then(|c| c.schema.as_deref()),
            node.canonical_name.as_ref().map(|c| c.name.as_str()),
            node.canonical_name
                .as_ref()
                .and_then(|c| c.column.as_deref()),
            node.expression.as_ref().map(|s| s.as_ref()),
            span_start,
            span_end,
            body_start,
            body_end,
            resolution,
        ])?;

        for stmt_id in &node.statement_ids {
            let statement_row_id = statement_row_id(statement_row_ids, *stmt_id)?;
            node_stmt_ref.execute(params![node.id.as_ref(), statement_row_id])?;
        }

        for span in node.all_name_spans() {
            name_span_stmt.execute(params![
                name_span_id,
                node.id.as_ref(),
                span.start as i64,
                span.end as i64,
            ])?;
            name_span_id += 1;
        }

        for stmt_id in &node.statement_ids {
            let statement_row_id = statement_row_id(statement_row_ids, *stmt_id)?;
            for filter in node.filters_for_statement(*stmt_id) {
                let ft = format!("{:?}", filter.clause_type).to_lowercase();
                filter_stmt.execute(params![
                    filter_id,
                    node.id.as_ref(),
                    statement_row_id,
                    &filter.expression,
                    ft,
                ])?;
                filter_id += 1;
            }

            if let Some(agg) = node.aggregation_for_statement(*stmt_id) {
                agg_stmt.execute(params![
                    node.id.as_ref(),
                    statement_row_id,
                    agg.is_grouping_key,
                    &agg.function,
                    agg.distinct,
                ])?;
            }
        }
    }
    Ok(())
}

fn write_edges(
    conn: &Connection,
    result: &AnalyzeResult,
    statement_row_ids: &HashMap<usize, i64>,
) -> Result<(), ExportError> {
    let mut stmt = conn.prepare(
        "INSERT INTO edges (id, edge_type, from_node_id, to_node_id, expression, operation, is_approximate)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )?;

    let mut edge_stmt_ref =
        conn.prepare("INSERT INTO edge_statements (edge_id, statement_id) VALUES (?, ?)")?;

    let mut join_stmt = conn.prepare(
        "INSERT INTO joins (id, edge_id, join_type, join_condition) VALUES (?, ?, ?, ?)",
    )?;

    let join_edge_ids = representative_join_edge_ids(&result.nodes, &result.edges);
    let mut join_id: i64 = 0;

    for edge in &result.edges {
        let edge_type = format!("{:?}", edge.edge_type).to_lowercase();
        stmt.execute(params![
            edge.id.as_ref(),
            edge_type,
            edge.from.as_ref(),
            edge.to.as_ref(),
            edge.expression.as_ref().map(|s| s.as_ref()),
            edge.operation.as_ref().map(|s| s.as_ref()),
            edge.approximate.unwrap_or(false),
        ])?;

        for stmt_id in &edge.statement_ids {
            let statement_row_id = statement_row_id(statement_row_ids, *stmt_id)?;
            edge_stmt_ref.execute(params![edge.id.as_ref(), statement_row_id])?;
        }

        if join_edge_ids.contains(edge.id.as_ref()) {
            let join_type = edge
                .join_type
                .as_ref()
                .expect("representative join edge must carry join metadata");
            let jt = format!("{:?}", join_type).to_uppercase();
            join_stmt.execute(params![
                join_id,
                edge.id.as_ref(),
                jt,
                edge.join_condition.as_ref().map(|s| s.as_ref()),
            ])?;
            join_id += 1;
        }
    }
    Ok(())
}

fn write_issues(
    conn: &Connection,
    result: &AnalyzeResult,
    statement_row_ids: &HashMap<usize, i64>,
) -> Result<(), ExportError> {
    let mut stmt = conn.prepare(
        "INSERT INTO issues (id, statement_id, severity, code, message, span_start, span_end)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )?;

    for (issue_id, issue) in result.issues.iter().enumerate() {
        let severity = format!("{:?}", issue.severity).to_lowercase();
        let (span_start, span_end) = issue
            .span
            .map(|sp| (Some(sp.start as i64), Some(sp.end as i64)))
            .unwrap_or((None, None));
        let statement_row_id = issue
            .statement_index
            .map(|statement_index| statement_row_id(statement_row_ids, statement_index))
            .transpose()?;
        stmt.execute(params![
            issue_id as i64,
            statement_row_id,
            severity,
            &issue.code,
            &issue.message,
            span_start,
            span_end,
        ])?;
    }
    Ok(())
}

fn write_schema_tables(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    let Some(schema) = &result.resolved_schema else {
        return Ok(());
    };

    let mut table_stmt = conn.prepare(
        "INSERT INTO schema_tables (id, catalog, schema_name, name, resolution_source)
         VALUES (?, ?, ?, ?, ?)",
    )?;

    let mut col_stmt = conn.prepare(
        "INSERT INTO schema_columns (id, table_id, name, data_type, is_nullable, is_primary_key)
         VALUES (?, ?, ?, ?, ?, ?)",
    )?;

    let mut col_id: i64 = 0;
    for (table_id, table) in schema.tables.iter().enumerate() {
        let origin = format!("{:?}", table.origin).to_lowercase();
        table_stmt.execute(params![
            table_id as i64,
            &table.catalog,
            &table.schema,
            &table.name,
            origin,
        ])?;

        for col in &table.columns {
            col_stmt.execute(params![
                col_id,
                table_id as i64,
                &col.name,
                &col.data_type,
                None::<bool>, // is_nullable not in current schema
                col.is_primary_key,
            ])?;
            col_id += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flowscope_core::{analyze, AnalyzeRequest, Dialect};

    #[test]
    fn test_export_empty_result() {
        let result = AnalyzeResult::default();
        let bytes = export(&result).expect("Export should succeed");
        assert!(!bytes.is_empty(), "Database file should not be empty");
    }

    #[test]
    fn test_export_simple_query() {
        let request = AnalyzeRequest {
            sql: "SELECT id, name FROM users WHERE active = true".to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
            #[cfg(feature = "templating")]
            template_config: None,
        };
        let result = analyze(&request);
        let bytes = export(&result).expect("Export should succeed");
        assert!(!bytes.is_empty());

        // Verify we can open the database and query it
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), &bytes).unwrap();
        let conn = Connection::open(temp_file.path()).unwrap();

        // Check statements table
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM statements", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        // Check nodes exist
        let node_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
            .unwrap();
        assert!(node_count > 0);
    }

    #[test]
    fn test_export_with_joins() {
        let request = AnalyzeRequest {
            sql: "SELECT u.name, o.total, o.status FROM users u LEFT JOIN orders o ON u.id = o.user_id"
                .to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
            #[cfg(feature = "templating")]
            template_config: None,
        };
        let result = analyze(&request);
        let bytes = export(&result).expect("Export should succeed");

        let temp_file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), &bytes).unwrap();
        let conn = Connection::open(temp_file.path()).unwrap();

        // Check joins table has data
        let join_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM joins", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            join_count, 1,
            "joined projections should export one logical join"
        );

        let join_graph_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM join_graph", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            join_graph_count, 1,
            "join_graph should collapse column-level join metadata to one row"
        );

        let (from_label, to_label): (String, String) = conn
            .query_row("SELECT from_label, to_label FROM join_graph", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(from_label, "orders");
        assert_eq!(to_label, "Output");
    }
}
