use flowscope_core::{
    analyze, AnalyzeRequest, CaseSensitivity, Dialect, SchemaMetadata, SchemaNamespaceHint,
    SchemaTable,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn analyze_random_simple_join(
        table_a in "[a-z]{1,8}",
        table_b in "[a-z]{1,8}",
        col_a in "[a-z]{1,8}",
        col_b in "[a-z]{1,8}",
    ) {
        // Require distinct table names so the analyzer should find two tables.
        prop_assume!(table_a != table_b);

        let sql = format!(
            "SELECT \"{ta}\".\"{ca}\", \"{tb}\".\"{cb}\" FROM \"{ta}\" JOIN \"{tb}\" ON \"{ta}\".\"{ca}\" = \"{tb}\".\"{cb}\"",
            ta = table_a,
            tb = table_b,
            ca = col_a,
            cb = col_b,
        );

        let request = AnalyzeRequest {
            sql,
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        };

        let result = analyze(&request);

        prop_assert!(!result.summary.has_errors, "analysis reported errors: {:?}", result.issues);
        prop_assert_eq!(result.summary.statement_count, 1);
        prop_assert!(result.summary.table_count >= 2);
    }

    /// Tests that case sensitivity is respected across dialects.
    #[test]
    fn case_sensitivity_respects_dialect(
        table_name in "[a-zA-Z]{1,8}",
        dialect_idx in 0usize..4,
    ) {
        let dialects = [
            (Dialect::Postgres, true),   // lowercases unquoted
            (Dialect::Snowflake, true),  // uppercases unquoted
            (Dialect::Bigquery, false),  // exact case
            (Dialect::Generic, true),    // lowercases unquoted
        ];
        let (dialect, changes_case) = dialects[dialect_idx];

        let sql = format!("SELECT * FROM {}", table_name);

        let request = AnalyzeRequest {
            sql,
            files: None,
            dialect,
            source_name: None,
            options: None,
            schema: None,
        };

        let result = analyze(&request);

        // Should always produce one statement with one table
        prop_assert_eq!(result.summary.statement_count, 1);
        prop_assert!(result.summary.table_count >= 1);

        // Check the table name in global lineage reflects case handling
        if let Some(node) = result.global_lineage.nodes.first() {
            if changes_case {
                // Postgres/Generic lowercase, Snowflake uppercases
                let expected = match dialect {
                    Dialect::Snowflake => table_name.to_uppercase(),
                    _ => table_name.to_lowercase(),
                };
                prop_assert_eq!(node.label.as_ref(), expected.as_str());
            } else {
                // BigQuery preserves exact case
                prop_assert_eq!(node.label.as_ref(), table_name.as_str());
            }
        }
    }

    /// Tests that search path resolves tables correctly.
    #[test]
    fn search_path_resolution(
        table in "[a-z]{1,8}",
        schema1 in "[a-z]{1,8}",
        schema2 in "[a-z]{1,8}",
        use_first_schema in any::<bool>(),
    ) {
        prop_assume!(schema1 != schema2);

        // Create schema with table in one of the schemas
        let target_schema = if use_first_schema { &schema1 } else { &schema2 };

        let schema_metadata = SchemaMetadata {
            default_catalog: None,
            default_schema: None,
            search_path: Some(vec![
                SchemaNamespaceHint { catalog: None, schema: schema1.clone() },
                SchemaNamespaceHint { catalog: None, schema: schema2.clone() },
            ]),
            case_sensitivity: Some(CaseSensitivity::Lower),
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some(target_schema.clone()),
                name: table.clone(),
                columns: vec![],
            }],
            allow_implied: false,
        };

        // Query using unqualified table name
        let sql = format!("SELECT * FROM {}", table);

        let request = AnalyzeRequest {
            sql,
            files: None,
            dialect: Dialect::Postgres,
            source_name: None,
            options: None,
            schema: Some(schema_metadata),
        };

        let result = analyze(&request);

        // Should resolve to the qualified name via search path
        prop_assert_eq!(result.summary.statement_count, 1);

        // The resolved table should have the correct schema and name
        if let Some(node) = result.global_lineage.nodes.first() {
            let got_schema = node.canonical_name.schema.as_deref();
            let got_name = &node.canonical_name.name;
            prop_assert_eq!(got_schema, Some(target_schema.as_str()), "Schema mismatch");
            prop_assert_eq!(got_name, &table, "Table name mismatch");
        }
    }

    /// Tests that qualified identifiers bypass search path.
    #[test]
    fn qualified_name_bypasses_search_path(
        table in "[a-z]{1,8}",
        schema in "[a-z]{1,8}",
        other_schema in "[a-z]{1,8}",
    ) {
        prop_assume!(schema != other_schema);

        // Schema metadata with table in 'schema', search path pointing to 'other_schema'
        let schema_metadata = SchemaMetadata {
            default_catalog: None,
            default_schema: None,
            search_path: Some(vec![
                SchemaNamespaceHint { catalog: None, schema: other_schema.clone() },
            ]),
            case_sensitivity: Some(CaseSensitivity::Lower),
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some(schema.clone()),
                name: table.clone(),
                columns: vec![],
            }],
            allow_implied: false,
        };

        // Query using qualified table name - should resolve directly, not via search path
        let sql = format!("SELECT * FROM {}.{}", schema, table);

        let request = AnalyzeRequest {
            sql,
            files: None,
            dialect: Dialect::Postgres,
            source_name: None,
            options: None,
            schema: Some(schema_metadata),
        };

        let result = analyze(&request);

        prop_assert_eq!(result.summary.statement_count, 1);

        // Should resolve to the explicitly qualified name
        if let Some(node) = result.global_lineage.nodes.first() {
            let got_schema = node.canonical_name.schema.as_deref();
            let got_name = &node.canonical_name.name;
            prop_assert_eq!(got_schema, Some(schema.as_str()), "Schema mismatch");
            prop_assert_eq!(got_name, &table, "Table name mismatch");
        }
    }

    /// Tests that empty SQL produces empty analysis without errors.
    #[test]
    fn empty_or_whitespace_sql(
        whitespace in prop::string::string_regex("[ \t\n\r]*").unwrap(),
    ) {
        let request = AnalyzeRequest {
            sql: whitespace,
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        };

        let result = analyze(&request);

        // Empty SQL should produce one issue (no input) but not crash
        prop_assert!(result.issues.len() >= 1 || result.summary.statement_count == 0);
    }
}
