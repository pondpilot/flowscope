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

// =============================================================================
// QUOTED IDENTIFIER EDGE CASES
// =============================================================================
// These tests document behavior when identifiers contain dots within quotes.

#[test]
fn split_handles_dots_inside_double_quotes() {
    // Double-quoted identifiers with dots should be kept together
    assert_eq!(
        split_qualified_identifiers(r#""schema.with.dots".table"#),
        vec!["\"schema.with.dots\"", "table"]
    );
}

#[test]
fn split_handles_dots_inside_brackets() {
    // Bracket-quoted identifiers (SQL Server style) with dots
    assert_eq!(
        split_qualified_identifiers("[schema.with.dots].table"),
        vec!["[schema.with.dots]", "table"]
    );
}

#[test]
fn split_handles_dots_inside_backticks() {
    // Backtick-quoted identifiers (MySQL style) with dots
    assert_eq!(
        split_qualified_identifiers("`schema.with.dots`.table"),
        vec!["`schema.with.dots`", "table"]
    );
}

#[test]
fn split_handles_complex_mixed_quoting() {
    // Mixed quoting styles in a single name
    assert_eq!(
        split_qualified_identifiers(r#"[catalog]."schema.name".`table`"#),
        vec!["[catalog]", "\"schema.name\"", "`table`"]
    );
}

#[test]
fn split_handles_escaped_quotes() {
    // Escaped double quotes inside quoted identifier (SQL standard: "" for ")
    assert_eq!(
        split_qualified_identifiers(r#""table""with""quotes".column"#),
        vec!["\"table\"\"with\"\"quotes\"", "column"]
    );
}

#[test]
fn split_simple_unquoted_identifiers() {
    // Basic unquoted multi-part names
    assert_eq!(
        split_qualified_identifiers("catalog.schema.table"),
        vec!["catalog", "schema", "table"]
    );
    assert_eq!(
        split_qualified_identifiers("schema.table"),
        vec!["schema", "table"]
    );
    assert_eq!(split_qualified_identifiers("table"), vec!["table"]);
}

#[test]
fn split_handles_empty_and_whitespace() {
    // Empty string returns empty vector
    assert!(split_qualified_identifiers("").is_empty());

    // Whitespace is trimmed from parts
    assert_eq!(
        split_qualified_identifiers("  schema  .  table  "),
        vec!["schema", "table"]
    );
}

#[test]
fn parse_canonical_handles_quoted_with_dots() {
    // Canonical name parsing with dots inside quotes
    let c = parse_canonical_name(r#""my.catalog"."my.schema"."my.table""#);
    assert_eq!(c.catalog.as_deref(), Some("\"my.catalog\""));
    assert_eq!(c.schema.as_deref(), Some("\"my.schema\""));
    assert_eq!(c.name, "\"my.table\"");
}
