use flowscope_core::analyzer::helpers::{
    canonical_name_from_object_name, extract_simple_name, extract_simple_name_from_object_name,
    ident_value, is_quoted_identifier, parse_canonical_name, split_qualified_identifiers,
    unquote_identifier,
};
use sqlparser::ast::{Ident, ObjectName, ObjectNamePart};

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

// =============================================================================
// OBJECTNAME-BASED HELPERS
// =============================================================================
// These tests verify the new functions that work directly with AST types.

fn make_ident(value: &str) -> Ident {
    Ident::new(value)
}

fn make_object_name(parts: &[&str]) -> ObjectName {
    ObjectName(
        parts
            .iter()
            .map(|s| ObjectNamePart::Identifier(make_ident(s)))
            .collect(),
    )
}

#[test]
fn extract_simple_name_from_object_name_single_part() {
    let name = make_object_name(&["users"]);
    assert_eq!(extract_simple_name_from_object_name(&name), "users");
}

#[test]
fn extract_simple_name_from_object_name_two_parts() {
    let name = make_object_name(&["public", "users"]);
    assert_eq!(extract_simple_name_from_object_name(&name), "users");
}

#[test]
fn extract_simple_name_from_object_name_three_parts() {
    let name = make_object_name(&["mydb", "public", "users"]);
    assert_eq!(extract_simple_name_from_object_name(&name), "users");
}

#[test]
fn extract_simple_name_from_object_name_empty() {
    let name = ObjectName(vec![]);
    assert_eq!(extract_simple_name_from_object_name(&name), "");
}

#[test]
fn canonical_name_from_object_name_single_part() {
    let name = make_object_name(&["users"]);
    let c = canonical_name_from_object_name(&name);
    assert_eq!(c.catalog, None);
    assert_eq!(c.schema, None);
    assert_eq!(c.name, "users");
}

#[test]
fn canonical_name_from_object_name_two_parts() {
    let name = make_object_name(&["public", "users"]);
    let c = canonical_name_from_object_name(&name);
    assert_eq!(c.catalog, None);
    assert_eq!(c.schema.as_deref(), Some("public"));
    assert_eq!(c.name, "users");
}

#[test]
fn canonical_name_from_object_name_three_parts() {
    let name = make_object_name(&["mydb", "public", "users"]);
    let c = canonical_name_from_object_name(&name);
    assert_eq!(c.catalog.as_deref(), Some("mydb"));
    assert_eq!(c.schema.as_deref(), Some("public"));
    assert_eq!(c.name, "users");
}

#[test]
fn canonical_name_from_object_name_four_parts() {
    // More than 3 parts should keep the full identifier together
    let name = make_object_name(&["server", "extra", "mydb", "public", "users"]);
    let c = canonical_name_from_object_name(&name);
    assert!(c.catalog.is_none());
    assert!(c.schema.is_none());
    assert_eq!(c.name, "server.extra.mydb.public.users");
}

#[test]
fn ident_value_extracts_value() {
    let ident = make_ident("my_column");
    assert_eq!(ident_value(&ident), "my_column");
}

// =============================================================================
// QUOTED IDENTIFIER TESTS FOR AST-BASED HELPERS
// =============================================================================
// Note: sqlparser stores the unquoted value in Ident.value, so AST-based helpers
// return unquoted values. This differs from string-based helpers which preserve
// quotes in their output.

fn make_quoted_ident(quote: char, value: &str) -> Ident {
    Ident::with_quote(quote, value)
}

fn make_quoted_object_name(parts: &[(char, &str)]) -> ObjectName {
    ObjectName(
        parts
            .iter()
            .map(|(quote, value)| ObjectNamePart::Identifier(make_quoted_ident(*quote, value)))
            .collect(),
    )
}

#[test]
fn extract_simple_name_from_object_name_quoted() {
    // Double-quoted identifier with dots inside
    let ident = make_quoted_ident('"', "User Table");
    let name = ObjectName(vec![ObjectNamePart::Identifier(ident)]);
    // AST-based helper returns unquoted value
    assert_eq!(extract_simple_name_from_object_name(&name), "User Table");
}

#[test]
fn canonical_name_from_object_name_quoted_with_dots() {
    // Schema and table both quoted with dots inside
    let name = make_quoted_object_name(&[('"', "my.schema"), ('"', "my.table")]);
    let c = canonical_name_from_object_name(&name);
    assert_eq!(c.catalog, None);
    // AST-based helper returns unquoted values
    assert_eq!(c.schema.as_deref(), Some("my.schema"));
    assert_eq!(c.name, "my.table");
}

#[test]
fn canonical_name_from_object_name_mixed_quoting() {
    // Mix of quoted and unquoted identifiers
    let parts = vec![
        ObjectNamePart::Identifier(make_ident("mydb")),
        ObjectNamePart::Identifier(make_quoted_ident('"', "My Schema")),
        ObjectNamePart::Identifier(make_quoted_ident('`', "My Table")),
    ];
    let name = ObjectName(parts);
    let c = canonical_name_from_object_name(&name);
    assert_eq!(c.catalog.as_deref(), Some("mydb"));
    assert_eq!(c.schema.as_deref(), Some("My Schema"));
    assert_eq!(c.name, "My Table");
}

#[test]
fn ident_value_quoted_preserves_inner_value() {
    // ident_value should return the unquoted inner value
    let ident = make_quoted_ident('"', "Column With Spaces");
    assert_eq!(ident_value(&ident), "Column With Spaces");
}

#[test]
fn canonical_name_from_object_name_empty() {
    // Empty ObjectName should produce empty canonical name
    let name = ObjectName(vec![]);
    let c = canonical_name_from_object_name(&name);
    assert!(c.catalog.is_none());
    assert!(c.schema.is_none());
    assert_eq!(c.name, "");
}

#[test]
fn canonical_name_from_object_name_exactly_four_parts() {
    // Exactly 4 parts should trigger the >3 fallback
    let name = make_object_name(&["server", "mydb", "public", "users"]);
    let c = canonical_name_from_object_name(&name);
    assert!(c.catalog.is_none());
    assert!(c.schema.is_none());
    assert_eq!(c.name, "server.mydb.public.users");
}

#[test]
fn canonical_name_from_object_name_preserves_case() {
    // Case should be preserved in the output
    let name = make_object_name(&["MyDB", "PublicSchema", "UsersTable"]);
    let c = canonical_name_from_object_name(&name);
    assert_eq!(c.catalog.as_deref(), Some("MyDB"));
    assert_eq!(c.schema.as_deref(), Some("PublicSchema"));
    assert_eq!(c.name, "UsersTable");
}

#[test]
fn canonical_name_from_object_name_unicode() {
    // Unicode identifiers should work correctly
    let name = make_object_name(&["数据库", "スキーマ", "表名"]);
    let c = canonical_name_from_object_name(&name);
    assert_eq!(c.catalog.as_deref(), Some("数据库"));
    assert_eq!(c.schema.as_deref(), Some("スキーマ"));
    assert_eq!(c.name, "表名");
}
