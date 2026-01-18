use flowscope_core::{analyze, AnalyzeRequest, Dialect};
use flowscope_export::{
    export_csv_bundle, export_html, export_json, export_mermaid, export_xlsx, ExportNaming,
    MermaidView,
};
use std::io::Read;

fn analyze_sample() -> flowscope_core::AnalyzeResult {
    analyze(&AnalyzeRequest {
        sql: "SELECT u.id, o.total FROM users u JOIN orders o ON u.id = o.user_id".to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: None,
        options: None,
        schema: None,
    })
}

#[test]
fn exports_mermaid_views() {
    let result = analyze_sample();
    let mermaid = export_mermaid(&result, MermaidView::Table).expect("mermaid export");
    assert!(mermaid.contains("flowchart LR"));
    assert!(mermaid.contains("users"));
    assert!(mermaid.contains("orders"));
}

#[test]
fn exports_json_pretty() {
    let result = analyze_sample();
    let json = export_json(&result, false).expect("json export");
    assert!(json.contains("\n"));
    assert!(json.contains("summary"));
}

#[test]
fn exports_html_report() {
    let result = analyze_sample();
    let naming = ExportNaming::new("Test Project");
    let html = export_html(&result, "Test Project", naming.exported_at()).expect("html export");
    assert!(html.contains("<title>Test Project - Lineage Export</title>"));
    assert!(html.contains("mermaid"));
}

#[test]
fn exports_csv_archive() {
    let result = analyze_sample();
    let bytes = export_csv_bundle(&result).expect("csv bundle");

    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).expect("zip archive");
    let mut file = archive
        .by_name("column_mappings.csv")
        .expect("column mappings file");
    let mut content = String::new();
    file.read_to_string(&mut content).expect("read csv content");
    assert!(content.contains("Source Table"));
}

#[test]
fn exports_xlsx_bytes() {
    let result = analyze_sample();
    let bytes = export_xlsx(&result).expect("xlsx export");
    assert!(!bytes.is_empty());
}
