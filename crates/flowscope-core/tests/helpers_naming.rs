use flowscope_core::analyzer::helpers::{
    extract_simple_name, is_quoted_identifier, parse_canonical_name, split_qualified_identifiers,
    unquote_identifier,
};

#[test]
fn split_handles_quotes_and_brackets() {
    assert_eq!(
        split_qualified_identifiers(r#"[db].[schema]."Table"."Col""#),
        vec!["[db]", "[schema]", "\"Table\"", "\"Col\""]
    );
}

#[test]
fn extract_simple_name_returns_last_part() {
    assert_eq!(extract_simple_name("schema.table"), "table");
    assert_eq!(extract_simple_name("just_name"), "just_name");
}

#[test]
fn parse_canonical_name_supports_three_parts() {
    let c = parse_canonical_name("cat.sch.tbl");
    assert_eq!(c.catalog.as_deref(), Some("cat"));
    assert_eq!(c.schema.as_deref(), Some("sch"));
    assert_eq!(c.name, "tbl");
}

#[test]
fn quoted_identifier_detection() {
    assert!(is_quoted_identifier("\"tbl\""));
    assert!(is_quoted_identifier("[tbl]"));
    assert!(!is_quoted_identifier("tbl"));
    assert_eq!(unquote_identifier("\"tbl\""), "tbl");
}
