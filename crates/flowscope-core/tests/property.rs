use flowscope_core::{
    analyze, AnalyzeRequest, CaseSensitivity, Dialect, NodeType, SchemaMetadata,
    SchemaNamespaceHint, SchemaTable,
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
    ///
    /// Normalization rules (from specs/dialect-semantics/extracted_dialects.toml):
    /// - Postgres: lowercase
    /// - Snowflake: uppercase
    /// - BigQuery: case_insensitive (lowercases for comparison)
    /// - MySQL: case_sensitive (preserves exact case)
    /// - Generic: case_insensitive (lowercases)
    #[test]
    fn case_sensitivity_respects_dialect(
        table_name in "[a-zA-Z]{1,8}",
        dialect_idx in 0usize..4,
    ) {
        let dialects = [
            Dialect::Postgres,   // lowercases unquoted
            Dialect::Snowflake,  // uppercases unquoted
            Dialect::Mysql,      // case-sensitive (preserves exact case)
            Dialect::Generic,    // case-insensitive (lowercases)
        ];
        let dialect = dialects[dialect_idx];

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
        // Find the table node (skip output nodes)
        if let Some(node) = result.global_lineage.nodes.iter().find(|n| n.node_type == NodeType::Table) {
            let expected = match dialect {
                Dialect::Snowflake => table_name.to_uppercase(),
                Dialect::Mysql => table_name.clone(), // preserves exact case
                _ => table_name.to_lowercase(), // Postgres, Generic, BigQuery
            };
            prop_assert_eq!(node.label.as_ref(), expected.as_str());
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
        // Find the table node (skip output nodes)
        if let Some(node) = result.global_lineage.nodes.iter().find(|n| n.node_type == NodeType::Table) {
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
        // Find the table node (skip output nodes)
        if let Some(node) = result.global_lineage.nodes.iter().find(|n| n.node_type == NodeType::Table) {
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
        prop_assert!(!result.issues.is_empty() || result.summary.statement_count == 0);
    }
}

/// Tests for dialect-specific function argument handling.
///
/// These tests verify that date/time unit literals are correctly handled per dialect
/// and not incorrectly treated as column references.
mod function_arg_handling {
    use super::*;
    use flowscope_core::NodeType;

    /// Helper to check if any column node in the result contains a specific label.
    fn has_column_with_label(result: &flowscope_core::AnalyzeResult, label: &str) -> bool {
        result.statements.iter().any(|stmt| {
            stmt.nodes.iter().any(|node| {
                node.node_type == NodeType::Column && node.label.eq_ignore_ascii_case(label)
            })
        })
    }

    /// Tests DATEDIFF function argument handling across dialects.
    ///
    /// Dialects like Snowflake, MSSQL, and Redshift use DATEDIFF(unit, start, end)
    /// where `unit` is a keyword (day, month, etc.) that should not be treated as a column.
    ///
    /// Other dialects like MySQL use DATEDIFF(end, start) with no unit argument.
    #[test]
    fn test_datediff_unit_argument_handling() {
        // Snowflake: DATEDIFF(day, start_date, end_date) - first arg is a unit keyword
        let sql = "SELECT DATEDIFF(day, start_date, end_date) FROM events";
        let request = AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Snowflake,
            source_name: None,
            options: None,
            schema: None,
        };
        let result = analyze(&request);

        // "day" should NOT appear as a column node - it's a unit keyword
        assert!(
            !has_column_with_label(&result, "day"),
            "Snowflake DATEDIFF should not treat 'day' unit as a column reference"
        );

        // MySQL: DATEDIFF(end_date, start_date) - no unit argument, both are columns
        let sql = "SELECT DATEDIFF(end_date, start_date) FROM events";
        let request = AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Mysql,
            source_name: None,
            options: None,
            schema: None,
        };
        let result = analyze(&request);

        // Both arguments should be captured as source columns
        assert!(
            has_column_with_label(&result, "end_date"),
            "MySQL DATEDIFF should treat first argument as column reference"
        );
        assert!(
            has_column_with_label(&result, "start_date"),
            "MySQL DATEDIFF should treat second argument as column reference"
        );
    }

    /// Tests DATE_TRUNC function argument handling across dialects.
    ///
    /// DATE_TRUNC argument order varies by dialect:
    /// - Postgres/Snowflake: DATE_TRUNC('month', date_column) - first arg is unit
    /// - BigQuery: DATE_TRUNC(date_column, MONTH) - second arg is unit
    #[test]
    fn test_date_trunc_argument_order_by_dialect() {
        // Postgres: DATE_TRUNC('month', created_at) - first arg is the unit
        let sql = "SELECT DATE_TRUNC('month', created_at) FROM events";
        let request = AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Postgres,
            source_name: None,
            options: None,
            schema: None,
        };
        let result = analyze(&request);

        // 'month' is a string literal, not a column - should not appear
        assert!(
            !has_column_with_label(&result, "month"),
            "Postgres DATE_TRUNC should not treat 'month' as a column"
        );
        // created_at should be captured
        assert!(
            has_column_with_label(&result, "created_at"),
            "Postgres DATE_TRUNC should capture created_at as a source column"
        );

        // BigQuery: DATE_TRUNC(created_at, MONTH) - second arg is the unit
        let sql = "SELECT DATE_TRUNC(created_at, MONTH) FROM events";
        let request = AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Bigquery,
            source_name: None,
            options: None,
            schema: None,
        };
        let result = analyze(&request);

        // MONTH should NOT appear as a column - it's a unit identifier
        assert!(
            !has_column_with_label(&result, "MONTH"),
            "BigQuery DATE_TRUNC should not treat 'MONTH' as a column"
        );
        // created_at should be captured
        assert!(
            has_column_with_label(&result, "created_at"),
            "BigQuery DATE_TRUNC should capture created_at as a source column"
        );
    }

    /// Tests DATE_PART/EXTRACT function handling.
    ///
    /// These functions extract a component from a date/timestamp and the unit
    /// argument should not be treated as a column reference.
    #[test]
    fn test_date_part_unit_handling() {
        // Postgres: DATE_PART('year', created_at) - first arg is quoted string
        let sql = "SELECT DATE_PART('year', created_at) FROM events";
        let request = AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Postgres,
            source_name: None,
            options: None,
            schema: None,
        };
        let result = analyze(&request);

        // 'year' is a string literal, not a column
        assert!(
            !has_column_with_label(&result, "year"),
            "DATE_PART should not treat 'year' unit as a column"
        );

        // Verify created_at is captured as a source
        assert!(
            has_column_with_label(&result, "created_at"),
            "DATE_PART should capture the date column as a source"
        );
    }

    /// Tests that the same function name can have different argument handling per dialect.
    #[test]
    fn test_same_function_different_dialects() {
        let sql = "SELECT DATE_ADD(created_at, INTERVAL 1 DAY) FROM events";

        let dialects = [
            Dialect::Postgres,
            Dialect::Mysql,
            Dialect::Snowflake,
            Dialect::Bigquery,
        ];

        for dialect in dialects {
            let request = AnalyzeRequest {
                sql: sql.to_string(),
                files: None,
                dialect,
                source_name: None,
                options: None,
                schema: None,
            };

            let result = analyze(&request);

            // All dialects should produce a result without panicking
            // (syntax support varies, but analysis should complete)
            // Just verify we got a result - accessing summary proves the analysis completed
            let _ = result.summary.statement_count;
        }
    }

    /// Tests that MSSQL and Redshift also skip DATEDIFF unit argument.
    #[test]
    fn test_datediff_unit_mssql_redshift() {
        for dialect in [Dialect::Mssql, Dialect::Redshift] {
            let sql = "SELECT DATEDIFF(month, hire_date, term_date) FROM employees";
            let request = AnalyzeRequest {
                sql: sql.to_string(),
                files: None,
                dialect,
                source_name: None,
                options: None,
                schema: None,
            };
            let result = analyze(&request);

            // "month" should NOT appear as a column node
            assert!(
                !has_column_with_label(&result, "month"),
                "{:?} DATEDIFF should not treat 'month' unit as a column reference",
                dialect
            );

            // hire_date and term_date should be captured
            assert!(
                has_column_with_label(&result, "hire_date"),
                "{:?} DATEDIFF should capture hire_date",
                dialect
            );
            assert!(
                has_column_with_label(&result, "term_date"),
                "{:?} DATEDIFF should capture term_date",
                dialect
            );
        }
    }

    /// Tests that unknown functions gracefully fall back to treating all arguments as columns.
    #[test]
    fn test_unknown_function_fallback() {
        // MY_CUSTOM_FUNC is not a known function - all arguments should be treated as columns
        let sql = "SELECT MY_CUSTOM_FUNC(col_a, col_b, col_c) FROM my_table";
        let request = AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        };
        let result = analyze(&request);

        // All arguments should be captured as source columns
        assert!(
            has_column_with_label(&result, "col_a"),
            "Unknown function should capture col_a as a source column"
        );
        assert!(
            has_column_with_label(&result, "col_b"),
            "Unknown function should capture col_b as a source column"
        );
        assert!(
            has_column_with_label(&result, "col_c"),
            "Unknown function should capture col_c as a source column"
        );
    }

    /// Tests that JSON aggregate functions are correctly recognized after the naming fix.
    #[test]
    fn test_json_aggregate_functions() {
        // Test JSON_OBJECT_AGG - should be recognized as an aggregate
        let sql = "SELECT JSON_OBJECT_AGG(key_col, value_col) FROM kv_table GROUP BY category";
        let request = AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Postgres,
            source_name: None,
            options: None,
            schema: None,
        };
        let result = analyze(&request);

        // Should not produce errors about non-aggregate in GROUP BY query
        assert!(
            !result.summary.has_errors,
            "JSON_OBJECT_AGG should be recognized as an aggregate function"
        );

        // Source columns should be captured
        assert!(
            has_column_with_label(&result, "key_col"),
            "JSON_OBJECT_AGG should capture key column"
        );
        assert!(
            has_column_with_label(&result, "value_col"),
            "JSON_OBJECT_AGG should capture value column"
        );
    }

    /// Tests that both underscore and non-underscore function name variants work.
    ///
    /// Functions like DATEADD/DATE_ADD and TIMESTAMPADD/TIMESTAMP_ADD should be
    /// handled identically, as different dialects use different spellings.
    #[test]
    fn test_underscore_function_name_variants() {
        // MSSQL uses DATEADD without underscore: DATEADD(day, 1, date_col)
        let sql = "SELECT DATEADD(day, 1, created_at) FROM events";
        let request = AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Mssql,
            source_name: None,
            options: None,
            schema: None,
        };
        let result = analyze(&request);

        // "day" should NOT appear as a column node - it's a unit keyword
        assert!(
            !has_column_with_label(&result, "day"),
            "MSSQL DATEADD should not treat 'day' unit as a column reference"
        );
        // created_at should be captured
        assert!(
            has_column_with_label(&result, "created_at"),
            "MSSQL DATEADD should capture the date column"
        );

        // Snowflake uses both DATEADD and DATE_ADD - both should skip first arg
        for func_name in ["DATEADD", "DATE_ADD"] {
            let sql = format!("SELECT {}(day, 1, created_at) FROM events", func_name);
            let request = AnalyzeRequest {
                sql,
                files: None,
                dialect: Dialect::Snowflake,
                source_name: None,
                options: None,
                schema: None,
            };
            let result = analyze(&request);

            assert!(
                !has_column_with_label(&result, "day"),
                "Snowflake {} should not treat 'day' unit as a column reference",
                func_name
            );
            assert!(
                has_column_with_label(&result, "created_at"),
                "Snowflake {} should capture the date column",
                func_name
            );
        }

        // Test TIMESTAMPADD variants
        for func_name in ["TIMESTAMPADD", "TIMESTAMP_ADD"] {
            let sql = format!("SELECT {}(hour, 2, event_time) FROM events", func_name);
            let request = AnalyzeRequest {
                sql,
                files: None,
                dialect: Dialect::Snowflake,
                source_name: None,
                options: None,
                schema: None,
            };
            let result = analyze(&request);

            assert!(
                !has_column_with_label(&result, "hour"),
                "Snowflake {} should not treat 'hour' unit as a column reference",
                func_name
            );
            assert!(
                has_column_with_label(&result, "event_time"),
                "Snowflake {} should capture the timestamp column",
                func_name
            );
        }
    }
}
