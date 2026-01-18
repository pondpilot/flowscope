use chrono::{DateTime, Utc};

use flowscope_core::AnalyzeResult;

use crate::extract::{extract_column_mappings, extract_script_info, extract_table_info};
use crate::mermaid::{export_mermaid, MermaidView};

pub fn export_html(
    result: &AnalyzeResult,
    project_name: &str,
    exported_at: DateTime<Utc>,
) -> String {
    let script_view = export_mermaid(result, MermaidView::Script);
    let hybrid_view = export_mermaid(result, MermaidView::Hybrid);
    let table_view = export_mermaid(result, MermaidView::Table);
    let column_view = export_mermaid(result, MermaidView::Column);

    let scripts = extract_script_info(result);
    let tables = extract_table_info(result);
    let mappings = extract_column_mappings(result);
    let issues = &result.issues;

    let export_date = exported_at.format("%Y-%m-%d %H:%M:%S UTC");

    let issues_section = if issues.is_empty() {
        String::new()
    } else {
        let rows = issues
            .iter()
            .map(|issue| {
                let severity_class = severity_class(issue.severity);
                let severity_label = severity_label(issue.severity);
                format!(
                    "<tr><td><span class=\"badge badge-{}\">{}</span></td><td>{}</td><td>{}</td></tr>",
                    severity_class,
                    escape_html(severity_label),
                    escape_html(&issue.code),
                    escape_html(&issue.message)
                )
            })
            .collect::<Vec<_>>()
            .join("");

        format!(
            "<div class=\"section-title\">Issues</div>\
<table>\
  <thead><tr><th>Severity</th><th>Code</th><th>Message</th></tr></thead>\
  <tbody>{rows}</tbody>\
</table>"
        )
    };

    let script_rows = scripts
        .iter()
        .map(|script| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(&script.source_name),
                script.statement_count,
                escape_html(&script.tables_read.join(", ")),
                escape_html(&script.tables_written.join(", "))
            )
        })
        .collect::<Vec<_>>()
        .join("");

    let table_rows = tables
        .iter()
        .map(|table| {
            format!(
                "<tr><td>{}</td><td>{}</td><td><span class=\"badge badge-{}\">{}</span></td><td>{}</td></tr>",
                escape_html(&table.name),
                escape_html(&table.qualified_name),
                table.table_type.as_str(),
                escape_html(&table.table_type.to_string().to_uppercase()),
                escape_html(&table.columns.join(", "))
            )
        })
        .collect::<Vec<_>>()
        .join("");

    let mapping_rows = mappings
        .iter()
        .map(|mapping| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(&mapping.source_table),
                escape_html(&mapping.source_column),
                escape_html(&mapping.target_table),
                escape_html(&mapping.target_column),
                escape_html(mapping.expression.as_deref().unwrap_or(""))
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{title} - Lineage Export</title>
  <script src="https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js"></script>
  <style>
    :root {{
      --bg-primary: #ffffff;
      --bg-secondary: #f8fafc;
      --text-primary: #1e293b;
      --text-secondary: #64748b;
      --border-color: #e2e8f0;
      --accent-color: #3b82f6;
      --error-color: #ef4444;
      --warning-color: #f59e0b;
    }}

    * {{ box-sizing: border-box; margin: 0; padding: 0; }}

    body {{
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
      background-color: var(--bg-secondary);
      color: var(--text-primary);
      line-height: 1.6;
    }}

    .container {{ max-width: 1400px; margin: 0 auto; padding: 2rem; }}

    header {{
      background: var(--bg-primary);
      border-bottom: 1px solid var(--border-color);
      padding: 1.5rem 2rem;
      margin-bottom: 2rem;
    }}

    h1 {{ font-size: 1.75rem; font-weight: 600; }}
    .export-date {{ color: var(--text-secondary); font-size: 0.875rem; margin-top: 0.5rem; }}

    .summary-cards {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
      gap: 1rem;
      margin-bottom: 2rem;
    }}

    .card {{
      background: var(--bg-primary);
      border: 1px solid var(--border-color);
      border-radius: 8px;
      padding: 1rem;
    }}

    .card-label {{ font-size: 0.75rem; text-transform: uppercase; color: var(--text-secondary); }}
    .card-value {{ font-size: 1.5rem; font-weight: 600; }}
    .card-value.error {{ color: var(--error-color); }}
    .card-value.warning {{ color: var(--warning-color); }}

    .tabs {{ display: flex; gap: 0.5rem; margin-bottom: 1rem; border-bottom: 1px solid var(--border-color); padding-bottom: 0.5rem; }}
    .tab {{ padding: 0.5rem 1rem; border: none; background: transparent; cursor: pointer; font-size: 0.875rem; color: var(--text-secondary); border-radius: 4px; }}
    .tab:hover {{ background: var(--bg-secondary); }}
    .tab.active {{ background: var(--accent-color); color: white; }}

    .tab-content {{ display: none; }}
    .tab-content.active {{ display: block; }}

    .diagram-container {{
      background: var(--bg-primary);
      border: 1px solid var(--border-color);
      border-radius: 8px;
      padding: 2rem;
      margin-bottom: 2rem;
      overflow-x: auto;
    }}

    .mermaid {{ text-align: center; }}

    table {{ width: 100%; border-collapse: collapse; background: var(--bg-primary); border: 1px solid var(--border-color); border-radius: 8px; overflow: hidden; margin-bottom: 2rem; }}
    th, td {{ padding: 0.75rem 1rem; text-align: left; border-bottom: 1px solid var(--border-color); }}
    th {{ background: var(--bg-secondary); font-weight: 600; font-size: 0.75rem; text-transform: uppercase; color: var(--text-secondary); }}
    tr:last-child td {{ border-bottom: none; }}
    tr:hover td {{ background: var(--bg-secondary); }}

    .section-title {{ font-size: 1.25rem; font-weight: 600; margin-bottom: 1rem; }}

    .badge {{ display: inline-block; padding: 0.125rem 0.5rem; border-radius: 9999px; font-size: 0.75rem; font-weight: 500; }}
    .badge-table {{ background: #dbeafe; color: #1d4ed8; }}
    .badge-view {{ background: #dcfce7; color: #16a34a; }}
    .badge-cte {{ background: #fef3c7; color: #d97706; }}
    .badge-error {{ background: #fee2e2; color: #dc2626; }}
    .badge-warning {{ background: #fef3c7; color: #d97706; }}
    .badge-info {{ background: #dbeafe; color: #2563eb; }}
  </style>
</head>
<body>
  <header>
    <h1>{title}</h1>
    <div class="export-date">Exported on {export_date}</div>
  </header>

  <div class="container">
    <div class="summary-cards">
      <div class="card"><div class="card-label">Statements</div><div class="card-value">{statement_count}</div></div>
      <div class="card"><div class="card-label">Tables</div><div class="card-value">{table_count}</div></div>
      <div class="card"><div class="card-label">Columns</div><div class="card-value">{column_count}</div></div>
      <div class="card"><div class="card-label">Joins</div><div class="card-value">{join_count}</div></div>
      <div class="card"><div class="card-label">Errors</div><div class="card-value{error_class}">{errors}</div></div>
      <div class="card"><div class="card-label">Warnings</div><div class="card-value{warning_class}">{warnings}</div></div>
    </div>

    <div class="section-title">Diagrams</div>
    <div class="tabs">
      <button class="tab active" data-tab="script">Script View</button>
      <button class="tab" data-tab="hybrid">Hybrid View</button>
      <button class="tab" data-tab="table">Table View</button>
      <button class="tab" data-tab="column">Column View</button>
    </div>

    <div class="diagram-container">
      <div id="script" class="tab-content active"><div class="mermaid">{script_view}</div></div>
      <div id="hybrid" class="tab-content"><div class="mermaid">{hybrid_view}</div></div>
      <div id="table" class="tab-content"><div class="mermaid">{table_view}</div></div>
      <div id="column" class="tab-content"><div class="mermaid">{column_view}</div></div>
    </div>

    {issues_section}

    <div class="section-title">Scripts</div>
    <table>
      <thead><tr><th>Script Name</th><th>Statements</th><th>Tables Read</th><th>Tables Written</th></tr></thead>
      <tbody>{script_rows}</tbody>
    </table>

    <div class="section-title">Tables</div>
    <table>
      <thead><tr><th>Name</th><th>Qualified Name</th><th>Type</th><th>Columns</th></tr></thead>
      <tbody>{table_rows}</tbody>
    </table>

    <div class="section-title">Column Mappings</div>
    <table>
      <thead><tr><th>Source Table</th><th>Source Column</th><th>Target Table</th><th>Target Column</th><th>Expression</th></tr></thead>
      <tbody>{mapping_rows}</tbody>
    </table>
  </div>

  <script>
    mermaid.initialize({{ startOnLoad: true, theme: 'neutral' }});
    document.querySelectorAll('.tab').forEach(tab => {{
      tab.addEventListener('click', () => {{
        document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
        document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
        tab.classList.add('active');
        const targetId = tab.dataset.tab;
        document.getElementById(targetId).classList.add('active');
        mermaid.init(undefined, document.querySelector('#' + targetId + ' .mermaid'));
      }});
    }});
  </script>
</body>
</html>"#,
        title = escape_html(project_name),
        export_date = escape_html(&export_date.to_string()),
        statement_count = result.summary.statement_count,
        table_count = result.summary.table_count,
        column_count = result.summary.column_count,
        join_count = result.summary.join_count,
        errors = result.summary.issue_count.errors,
        warnings = result.summary.issue_count.warnings,
        error_class = if result.summary.issue_count.errors > 0 {
            " error"
        } else {
            ""
        },
        warning_class = if result.summary.issue_count.warnings > 0 {
            " warning"
        } else {
            ""
        },
        script_view = script_view,
        hybrid_view = hybrid_view,
        table_view = table_view,
        column_view = column_view,
        issues_section = issues_section,
        script_rows = script_rows,
        table_rows = table_rows,
        mapping_rows = mapping_rows,
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#039;")
}

fn severity_class(severity: flowscope_core::Severity) -> &'static str {
    match severity {
        flowscope_core::Severity::Error => "error",
        flowscope_core::Severity::Warning => "warning",
        flowscope_core::Severity::Info => "info",
    }
}

fn severity_label(severity: flowscope_core::Severity) -> &'static str {
    match severity {
        flowscope_core::Severity::Error => "ERROR",
        flowscope_core::Severity::Warning => "WARNING",
        flowscope_core::Severity::Info => "INFO",
    }
}
