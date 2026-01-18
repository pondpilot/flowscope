use std::collections::BTreeSet;
use std::io::{Cursor, Write};

use csv::WriterBuilder;
use flowscope_core::{AnalyzeResult, SchemaOrigin};
use zip::write::FileOptions;
use zip::CompressionMethod;

use crate::extract::{
    extract_column_mappings, extract_script_info, extract_table_dependencies, extract_table_info,
};
use crate::ExportError;

pub fn export_csv_bundle(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    let files: Vec<(String, Vec<u8>)> = vec![
        ("scripts.csv".to_string(), export_scripts_csv(result)?),
        ("tables.csv".to_string(), export_tables_csv(result)?),
        (
            "column_mappings.csv".to_string(),
            export_column_mappings_csv(result)?,
        ),
        (
            "table_dependencies.csv".to_string(),
            export_table_dependencies_csv(result)?,
        ),
        ("summary.csv".to_string(), export_summary_csv(result)?),
        ("issues.csv".to_string(), export_issues_csv(result)?),
        (
            "resolved_schema.csv".to_string(),
            export_schema_csv(result)?,
        ),
    ];

    let cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for (name, content) in files {
        zip.start_file(name, options)
            .map_err(|err| ExportError::Archive(err.to_string()))?;
        zip.write_all(&content)
            .map_err(|err| ExportError::Archive(err.to_string()))?;
    }

    let cursor = zip
        .finish()
        .map_err(|err| ExportError::Archive(err.to_string()))?;
    Ok(cursor.into_inner())
}

fn export_scripts_csv(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    let scripts = extract_script_info(result);
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .from_writer(Vec::new());

    writer
        .write_record([
            "Script Name",
            "Statement Count",
            "Tables Read",
            "Tables Written",
        ])
        .map_err(|err| ExportError::Csv(err.to_string()))?;

    for script in scripts {
        writer
            .write_record([
                script.source_name,
                script.statement_count.to_string(),
                script.tables_read.join(", "),
                script.tables_written.join(", "),
            ])
            .map_err(|err| ExportError::Csv(err.to_string()))?;
    }

    writer
        .into_inner()
        .map_err(|err| ExportError::Csv(err.to_string()))
}

fn export_tables_csv(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    let tables = extract_table_info(result);
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .from_writer(Vec::new());

    writer
        .write_record(["Table Name", "Qualified Name", "Type", "Columns", "Source"])
        .map_err(|err| ExportError::Csv(err.to_string()))?;

    for table in tables {
        writer
            .write_record([
                table.name,
                table.qualified_name,
                table.table_type.as_str().to_string(),
                table.columns.join(", "),
                table.source_name.unwrap_or_default(),
            ])
            .map_err(|err| ExportError::Csv(err.to_string()))?;
    }

    writer
        .into_inner()
        .map_err(|err| ExportError::Csv(err.to_string()))
}

fn export_column_mappings_csv(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    let mappings = extract_column_mappings(result);
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .from_writer(Vec::new());

    writer
        .write_record([
            "Source Table",
            "Source Column",
            "Target Table",
            "Target Column",
            "Expression",
            "Edge Type",
        ])
        .map_err(|err| ExportError::Csv(err.to_string()))?;

    for mapping in mappings {
        writer
            .write_record([
                mapping.source_table,
                mapping.source_column,
                mapping.target_table,
                mapping.target_column,
                mapping.expression.unwrap_or_default(),
                mapping.edge_type,
            ])
            .map_err(|err| ExportError::Csv(err.to_string()))?;
    }

    writer
        .into_inner()
        .map_err(|err| ExportError::Csv(err.to_string()))
}

fn export_table_dependencies_csv(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    let dependencies = extract_table_dependencies(result);
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .from_writer(Vec::new());

    writer
        .write_record(["Source Table", "Target Table"])
        .map_err(|err| ExportError::Csv(err.to_string()))?;

    for dependency in dependencies {
        writer
            .write_record([dependency.source_table, dependency.target_table])
            .map_err(|err| ExportError::Csv(err.to_string()))?;
    }

    writer
        .into_inner()
        .map_err(|err| ExportError::Csv(err.to_string()))
}

fn export_summary_csv(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    let summary = &result.summary;
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .from_writer(Vec::new());

    writer
        .write_record(["Metric", "Value"])
        .map_err(|err| ExportError::Csv(err.to_string()))?;

    let metrics = [
        ("Total Statements", summary.statement_count.to_string()),
        ("Total Tables", summary.table_count.to_string()),
        ("Total Columns", summary.column_count.to_string()),
        ("Total Joins", summary.join_count.to_string()),
        ("Complexity Score", summary.complexity_score.to_string()),
        ("Errors", summary.issue_count.errors.to_string()),
        ("Warnings", summary.issue_count.warnings.to_string()),
        ("Info", summary.issue_count.infos.to_string()),
    ];

    for (metric, value) in metrics {
        writer
            .write_record([metric, &value])
            .map_err(|err| ExportError::Csv(err.to_string()))?;
    }

    writer
        .into_inner()
        .map_err(|err| ExportError::Csv(err.to_string()))
}

fn export_issues_csv(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .from_writer(Vec::new());

    writer
        .write_record([
            "Severity",
            "Code",
            "Message",
            "Statement",
            "Span Start",
            "Span End",
        ])
        .map_err(|err| ExportError::Csv(err.to_string()))?;

    for issue in &result.issues {
        let statement = issue
            .statement_index
            .map(|idx| idx.to_string())
            .unwrap_or_default();
        let (start, end) = issue
            .span
            .map(|span| (span.start.to_string(), span.end.to_string()))
            .unwrap_or_default();

        writer
            .write_record([
                format!("{:?}", issue.severity).to_lowercase(),
                issue.code.clone(),
                issue.message.clone(),
                statement,
                start,
                end,
            ])
            .map_err(|err| ExportError::Csv(err.to_string()))?;
    }

    writer
        .into_inner()
        .map_err(|err| ExportError::Csv(err.to_string()))
}

fn export_schema_csv(result: &AnalyzeResult) -> Result<Vec<u8>, ExportError> {
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .from_writer(Vec::new());

    writer
        .write_record([
            "Catalog",
            "Schema",
            "Table",
            "Column",
            "Data Type",
            "Origin",
            "Primary Key",
            "Foreign Key",
        ])
        .map_err(|err| ExportError::Csv(err.to_string()))?;

    if let Some(resolved_schema) = &result.resolved_schema {
        for table in &resolved_schema.tables {
            let origin = match table.origin {
                SchemaOrigin::Imported => "imported",
                SchemaOrigin::Implied => "implied",
            };

            let mut column_names = BTreeSet::new();
            for column in &table.columns {
                if column_names.insert(column.name.clone()) {
                    let fk = column
                        .foreign_key
                        .as_ref()
                        .map(|fk| format!("{}.{}", fk.table, fk.column));

                    writer
                        .write_record([
                            table.catalog.clone().unwrap_or_default(),
                            table.schema.clone().unwrap_or_default(),
                            table.name.clone(),
                            column.name.clone(),
                            column.data_type.clone().unwrap_or_default(),
                            origin.to_string(),
                            column
                                .is_primary_key
                                .map(|value| value.to_string())
                                .unwrap_or_default(),
                            fk.unwrap_or_default(),
                        ])
                        .map_err(|err| ExportError::Csv(err.to_string()))?;
                }
            }
        }
    }

    writer
        .into_inner()
        .map_err(|err| ExportError::Csv(err.to_string()))
}
