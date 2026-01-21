//! DuckDB backend implementation.

use crate::schema::{tables_ddl, views_ddl};
use crate::ExportError;
use duckdb::{params, Connection};
use flowscope_core::AnalyzeResult;
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
    write_statements(conn, result)?;
    write_nodes(conn, result)?;
    write_edges(conn, result)?;
    write_issues(conn, result)?;
    write_schema_tables(conn, result)?;
    write_global_lineage(conn, result)?;
    Ok(())
}

/// Schema version for the export format.
/// Increment this when making breaking changes to the schema structure.
const SCHEMA_VERSION: &str = "1";

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

fn write_statements(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    let mut stmt = conn.prepare(
        "INSERT INTO statements (id, statement_index, statement_type, source_name, span_start, span_end, join_count, complexity_score)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )?;

    for (idx, s) in result.statements.iter().enumerate() {
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
    Ok(())
}

fn write_nodes(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    let mut node_stmt = conn.prepare(
        "INSERT INTO nodes (id, statement_id, node_type, label, qualified_name, expression, span_start, span_end, resolution_source)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )?;

    let mut join_stmt = conn.prepare(
        "INSERT INTO joins (id, node_id, statement_id, join_type, join_condition) VALUES (?, ?, ?, ?, ?)",
    )?;

    let mut filter_stmt = conn.prepare(
        "INSERT INTO filters (id, node_id, statement_id, predicate, filter_type) VALUES (?, ?, ?, ?, ?)",
    )?;

    let mut agg_stmt = conn.prepare(
        "INSERT INTO aggregations (node_id, statement_id, is_grouping_key, function, is_distinct) VALUES (?, ?, ?, ?, ?)",
    )?;

    let mut join_id: i64 = 0;
    let mut filter_id: i64 = 0;

    for (stmt_idx, statement) in result.statements.iter().enumerate() {
        for node in &statement.nodes {
            let (span_start, span_end) = node
                .span
                .map(|sp| (Some(sp.start as i64), Some(sp.end as i64)))
                .unwrap_or((None, None));
            let node_type = format!("{:?}", node.node_type).to_lowercase();
            let resolution = node
                .resolution_source
                .map(|r| format!("{:?}", r).to_lowercase());

            node_stmt.execute(params![
                node.id.as_ref(),
                stmt_idx as i64,
                node_type,
                node.label.as_ref(),
                node.qualified_name.as_ref().map(|s| s.as_ref()),
                node.expression.as_ref().map(|s| s.as_ref()),
                span_start,
                span_end,
                resolution,
            ])?;

            // Write join info if present
            if let Some(join_type) = &node.join_type {
                let jt = format!("{:?}", join_type).to_uppercase();
                join_stmt.execute(params![
                    join_id,
                    node.id.as_ref(),
                    stmt_idx as i64,
                    jt,
                    node.join_condition.as_ref().map(|s| s.as_ref()),
                ])?;
                join_id += 1;
            }

            // Write filters
            for filter in &node.filters {
                let ft = format!("{:?}", filter.clause_type).to_lowercase();
                filter_stmt.execute(params![
                    filter_id,
                    node.id.as_ref(),
                    stmt_idx as i64,
                    &filter.expression,
                    ft,
                ])?;
                filter_id += 1;
            }

            // Write aggregation info
            if let Some(agg) = &node.aggregation {
                agg_stmt.execute(params![
                    node.id.as_ref(),
                    stmt_idx as i64,
                    agg.is_grouping_key,
                    &agg.function,
                    agg.distinct,
                ])?;
            }
        }
    }
    Ok(())
}

fn write_edges(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    let mut stmt = conn.prepare(
        "INSERT INTO edges (id, statement_id, edge_type, from_node_id, to_node_id, expression, operation, is_approximate)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )?;

    let mut edge_id: i64 = 0;
    for (stmt_idx, statement) in result.statements.iter().enumerate() {
        for edge in &statement.edges {
            let edge_type = format!("{:?}", edge.edge_type).to_lowercase();
            stmt.execute(params![
                edge_id,
                stmt_idx as i64,
                edge_type,
                edge.from.as_ref(),
                edge.to.as_ref(),
                edge.expression.as_ref().map(|s| s.as_ref()),
                edge.operation.as_ref().map(|s| s.as_ref()),
                edge.approximate.unwrap_or(false),
            ])?;
            edge_id += 1;
        }
    }
    Ok(())
}

fn write_issues(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
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
        stmt.execute(params![
            issue_id as i64,
            issue.statement_index.map(|i| i as i64),
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

fn write_global_lineage(conn: &Connection, result: &AnalyzeResult) -> Result<(), ExportError> {
    let mut node_stmt = conn.prepare(
        "INSERT INTO global_nodes (id, node_type, label, canonical_catalog, canonical_schema, canonical_name, canonical_column, resolution_source)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )?;

    let mut ref_stmt = conn.prepare(
        "INSERT INTO global_node_statement_refs (id, global_node_id, statement_index, local_node_id)
         VALUES (?, ?, ?, ?)",
    )?;

    let mut edge_stmt = conn.prepare(
        "INSERT INTO global_edges (id, from_node_id, to_node_id, edge_type)
         VALUES (?, ?, ?, ?)",
    )?;

    let mut ref_id: i64 = 0;
    for node in &result.global_lineage.nodes {
        let node_type = format!("{:?}", node.node_type).to_lowercase();
        let resolution = node
            .resolution_source
            .map(|r| format!("{:?}", r).to_lowercase());

        node_stmt.execute(params![
            node.id.as_ref(),
            node_type,
            node.label.as_ref(),
            &node.canonical_name.catalog,
            &node.canonical_name.schema,
            &node.canonical_name.name,
            &node.canonical_name.column,
            resolution,
        ])?;

        for stmt_ref in &node.statement_refs {
            ref_stmt.execute(params![
                ref_id,
                node.id.as_ref(),
                stmt_ref.statement_index as i64,
                stmt_ref.node_id.as_ref().map(|s| s.as_ref()),
            ])?;
            ref_id += 1;
        }
    }

    for edge in &result.global_lineage.edges {
        let edge_type = format!("{:?}", edge.edge_type).to_lowercase();
        edge_stmt.execute(params![
            edge.id.as_ref(),
            edge.from.as_ref(),
            edge.to.as_ref(),
            edge_type,
        ])?;
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
            sql: "SELECT u.name, o.total FROM users u LEFT JOIN orders o ON u.id = o.user_id"
                .to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
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
        assert!(join_count > 0);
    }
}
