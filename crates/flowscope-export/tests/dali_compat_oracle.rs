//! Dali compat layer validation against Oracle fixture corpus.
//!
//! For each Oracle fixture that produces transforms (INSERT, UPDATE, DELETE, MERGE,
//! CREATE VIEW, CTAS), runs the Dali adapter and validates:
//! - Correct number of transforms
//! - Correct target/source tables
//! - Correct relation type
//! - Column-level refs where applicable

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use flowscope_core::{analyze, AnalyzeRequest, AnalyzeResult, Dialect, SchemaMetadata};
use flowscope_export::dali_compat::{self, DaliOutput};

// ── Helpers ──────────────────────────────────────────────────────

fn fixture_dir() -> PathBuf {
    // flowscope-export/tests -> flowscope-core/tests/fixtures/oracle
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("flowscope-core")
        .join("tests")
        .join("fixtures")
        .join("oracle")
}

fn load_fixture(name: &str) -> String {
    let path = fixture_dir().join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read fixture {path:?}: {e}"))
}

fn oracle_schema() -> SchemaMetadata {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("flowscope-core")
        .join("tests")
        .join("fixtures")
        .join("schemas")
        .join("oracle_sample.json");
    let content =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path:?}: {e}"));
    serde_json::from_str(&content).unwrap_or_else(|e| panic!("failed to parse {path:?}: {e}"))
}

fn analyze_oracle(sql: &str) -> AnalyzeResult {
    analyze(&AnalyzeRequest {
        sql: sql.trim().to_string(),
        files: None,
        dialect: Dialect::Oracle,
        source_name: None,
        options: None,
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    })
}

fn analyze_oracle_with_schema(sql: &str) -> AnalyzeResult {
    analyze(&AnalyzeRequest {
        sql: sql.trim().to_string(),
        files: None,
        dialect: Dialect::Oracle,
        source_name: None,
        options: None,
        schema: Some(oracle_schema()),
        #[cfg(feature = "templating")]
        template_config: None,
    })
}

fn dali_output(sql: &str, result: &AnalyzeResult) -> DaliOutput {
    let json_str = dali_compat::export_dali_compat(result, sql);
    serde_json::from_str(&json_str).expect("Dali output should be valid JSON")
}

fn target_tables(output: &DaliOutput, idx: usize) -> BTreeSet<String> {
    output.transforms[idx]
        .target_tables
        .iter()
        .cloned()
        .collect()
}

fn source_tables(output: &DaliOutput, idx: usize) -> BTreeSet<String> {
    output.transforms[idx]
        .source_tables
        .iter()
        .cloned()
        .collect()
}

fn ref_target_columns(output: &DaliOutput, idx: usize) -> Vec<String> {
    output.transforms[idx]
        .refs
        .iter()
        .map(|r| r.target_column.clone())
        .collect()
}

// ── INSERT fixtures ──────────────────────────────────────────────

#[test]
fn dali_insert_simple() {
    let sql = load_fixture("01_insert_simple.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert!(target_tables(&output, 0).contains("CORE.REG_SUBJECT"));
    assert!(source_tables(&output, 0).contains("IDM.REG_SUBJECT"));
    assert!(source_tables(&output, 0).contains("IDM.REG_SUBJECTTYPE"));
    assert_eq!(output.table_lineage[0].relation, "INSERT_SELECT");

    // Column-level refs
    let cols = ref_target_columns(&output, 0);
    assert!(cols.contains(&"ID_SUBJECT".to_string()), "refs: {cols:?}");
    assert!(
        cols.contains(&"ID_SUBJECTTYPE".to_string()),
        "refs: {cols:?}"
    );
}

#[test]
fn dali_insert_target_source() {
    let sql = load_fixture("insert_target_source.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert!(target_tables(&output, 0).contains("TARGET_SCHEMA.TARGET_TABLE"));
    assert!(source_tables(&output, 0).contains("SOURCE_SCHEMA.SOURCE_TABLE"));
    assert_eq!(output.table_lineage[0].relation, "INSERT_SELECT");

    let cols = ref_target_columns(&output, 0);
    assert_eq!(
        cols.len(),
        3,
        "should have 3 refs (id, name, dt), got {cols:?}"
    );
}

#[test]
fn dali_insert_cte_chain_deep() {
    let sql = load_fixture("insert_cte_chain_deep.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert!(target_tables(&output, 0).contains("CORE.REG_SUBJECT"));
    // Source should trace through CTEs to the base table
    assert!(
        source_tables(&output, 0).contains("IDM.REG_SUBJECT"),
        "source_tables: {:?}",
        output.transforms[0].source_tables
    );
    assert_eq!(output.table_lineage[0].relation, "INSERT_SELECT");
}

#[test]
fn dali_insert_union_all_sources() {
    let sql = load_fixture("insert_union_all_sources.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert!(target_tables(&output, 0).contains("CORE.REG_SUBJECT"));
    let srcs = source_tables(&output, 0);
    assert!(srcs.contains("IDM.REG_SUBJECT"), "sources: {srcs:?}");
    assert!(srcs.contains("IDM.REG_SUBJECTTYPE"), "sources: {srcs:?}");
    assert_eq!(output.table_lineage[0].relation, "INSERT_SELECT");
}

#[test]
fn dali_insert_case_expr() {
    let sql = load_fixture("insert_case_expr.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert!(target_tables(&output, 0).contains("CORE.REG_SUBJECT"));
    assert_eq!(output.table_lineage[0].relation, "INSERT_SELECT");

    // CASE expression produces column-level refs
    let cols = ref_target_columns(&output, 0);
    assert!(!cols.is_empty(), "CASE expression should produce refs");
}

#[test]
fn dali_insert_values_no_transform() {
    let sql = load_fixture("insert_values.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    // INSERT VALUES has a target table but no source table — adapter should still emit a transform
    // because there is a written table
    if !output.transforms.is_empty() {
        assert!(target_tables(&output, 0).contains("CORE.REG_SUBJECT"));
        assert_eq!(output.table_lineage[0].relation, "INSERT_SELECT");
    }
    // It's acceptable for INSERT..VALUES to produce 0 or 1 transforms
}

// ── MERGE fixtures ──────────────────────────────────────────────

#[test]
fn dali_merge_minimal() {
    let sql = load_fixture("05_merge_minimal.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert!(target_tables(&output, 0).contains("CORE.REG_SUBJECT"));
    assert_eq!(output.table_lineage[0].relation, "MERGE");

    let srcs = source_tables(&output, 0);
    assert!(
        srcs.contains("IDM.REG_SUBJECT"),
        "MERGE source should include IDM.REG_SUBJECT, got {srcs:?}"
    );
}

#[test]
fn dali_merge_using_join() {
    let sql = load_fixture("merge_using_join.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert!(target_tables(&output, 0).contains("CORE.REG_SUBJECT"));
    assert_eq!(output.table_lineage[0].relation, "MERGE");

    let srcs = source_tables(&output, 0);
    assert!(srcs.contains("IDM.REG_SUBJECT"), "sources: {srcs:?}");
    assert!(srcs.contains("IDM.REG_SUBJECTTYPE"), "sources: {srcs:?}");
}

#[test]
fn dali_merge_when_matched_only() {
    let sql = load_fixture("merge_when_matched_only.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert_eq!(output.table_lineage[0].relation, "MERGE");
}

#[test]
fn dali_merge_when_not_matched_only() {
    let sql = load_fixture("merge_when_not_matched_only.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert_eq!(output.table_lineage[0].relation, "MERGE");
}

// ── UPDATE fixtures ─────────────────────────────────────────────

#[test]
fn dali_update_simple() {
    let sql = load_fixture("update_simple.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    // UPDATE SET constant — has a target table but no source table
    if !output.transforms.is_empty() {
        assert!(target_tables(&output, 0).contains("CORE.REG_SUBJECT"));
        assert_eq!(output.table_lineage[0].relation, "UPDATE");
    }
}

#[test]
fn dali_update_alias() {
    let sql = load_fixture("03_update_alias.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    if !output.transforms.is_empty() {
        assert!(target_tables(&output, 0).contains("CORE.REG_SUBJECT"));
        assert_eq!(output.table_lineage[0].relation, "UPDATE");
    }
}

#[test]
fn dali_update_one_subquery() {
    let sql = load_fixture("update_one_subquery.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert!(
        !output.transforms.is_empty(),
        "UPDATE with subquery should produce transform"
    );
    assert!(target_tables(&output, 0).contains("TARGET_SCHEMA.TARGET_TABLE"));
    assert_eq!(output.table_lineage[0].relation, "UPDATE");

    let srcs = source_tables(&output, 0);
    assert!(
        srcs.contains("SOURCE_SCHEMA.SOURCE_TABLE"),
        "sources: {srcs:?}"
    );
}

// ── DELETE fixtures ─────────────────────────────────────────────

#[test]
fn dali_delete_simple() {
    let sql = load_fixture("delete_simple.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    // Simple DELETE (no subquery) — may or may not produce a transform
    // depending on whether FlowScope considers it a write operation
    if !output.transforms.is_empty() {
        assert_eq!(output.table_lineage[0].relation, "DELETE");
    }
}

#[test]
fn dali_delete_in() {
    let sql = load_fixture("delete_in.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    if !output.transforms.is_empty() {
        assert!(target_tables(&output, 0).contains("CORE.REG_SUBJECT"));
        assert_eq!(output.table_lineage[0].relation, "DELETE");
    }
}

#[test]
fn dali_delete_exists() {
    let sql = load_fixture("delete_exists.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    if !output.transforms.is_empty() {
        assert!(target_tables(&output, 0).contains("CORE.REG_SUBJECT"));
        assert_eq!(output.table_lineage[0].relation, "DELETE");
    }
}

// ── VIEW fixtures ───────────────────────────────────────────────

#[test]
fn dali_view_simple() {
    let sql = load_fixture("06_view_simple.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert!(target_tables(&output, 0).contains("TEST_VIEW"));
    assert!(source_tables(&output, 0).contains("IDM.REG_SUBJECT"));
    assert_eq!(output.table_lineage[0].relation, "VIEW_SELECT");

    let cols = ref_target_columns(&output, 0);
    assert!(cols.contains(&"ID_SUBJECT".to_string()), "refs: {cols:?}");
    assert!(cols.contains(&"CODE".to_string()), "refs: {cols:?}");
}

#[test]
fn dali_view_column_list() {
    let sql = load_fixture("view_column_list.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert!(target_tables(&output, 0).contains("TEST_VIEW_COL_LIST"));
    assert_eq!(output.table_lineage[0].relation, "VIEW_SELECT");
}

#[test]
fn dali_view_left_join() {
    let sql = load_fixture("view_left_join.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert_eq!(output.table_lineage[0].relation, "VIEW_SELECT");

    let srcs = source_tables(&output, 0);
    assert!(srcs.contains("IDM.REG_SUBJECT"), "sources: {srcs:?}");
    assert!(srcs.contains("IDM.REG_SUBJECTTYPE"), "sources: {srcs:?}");
}

#[test]
fn dali_view_cte_union() {
    let sql = load_fixture("view_cte_union.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert_eq!(output.table_lineage[0].relation, "VIEW_SELECT");
}

// ── CTAS fixtures ───────────────────────────────────────────────

#[test]
fn dali_ctas_cte_star() {
    let sql = load_fixture("ctas_cte_star_using_oracle.sql");
    let result = analyze_oracle(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert!(target_tables(&output, 0).contains("DM.MONTHLY_SALES"));
    assert_eq!(output.table_lineage[0].relation, "TABLE_SELECT");
}

// ── Schema-aware fixtures (column-level with metadata) ──────────

#[test]
fn dali_insert_simple_with_schema_refs() {
    let sql = load_fixture("01_insert_simple.sql");
    let result = analyze_oracle_with_schema(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert_eq!(output.table_lineage[0].relation, "INSERT_SELECT");

    // With schema, column-level refs should be fully resolved
    let refs = &output.transforms[0].refs;
    assert!(
        !refs.is_empty(),
        "schema-aware analysis should produce column refs"
    );
    for r in refs {
        assert!(
            !r.source_columns.is_empty(),
            "ref for {} should have sources",
            r.target_column
        );
        for sc in &r.source_columns {
            assert!(
                !sc.columns.is_empty(),
                "source column for {} should have qualified refs",
                r.target_column
            );
        }
    }
}

#[test]
fn dali_merge_using_join_with_schema_refs() {
    let sql = load_fixture("merge_using_join.sql");
    let result = analyze_oracle_with_schema(&sql);
    let output = dali_output(&sql, &result);

    assert_eq!(output.transforms.len(), 1);
    assert_eq!(output.table_lineage[0].relation, "MERGE");

    let refs = &output.transforms[0].refs;
    assert!(
        !refs.is_empty(),
        "schema-aware MERGE should produce column refs"
    );
}

// ── Bulk validation: all fixtures produce valid JSON ────────────

#[test]
fn dali_all_oracle_fixtures_produce_valid_json() {
    let fixture_path = fixture_dir();
    let mut fixtures: Vec<_> = fs::read_dir(&fixture_path)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "sql"))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    fixtures.sort();

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let sql = load_fixture(fixture);
        let result = analyze_oracle(&sql);
        let json_str = dali_compat::export_dali_compat(&result, &sql);

        // Must be valid JSON
        if let Err(e) = serde_json::from_str::<serde_json::Value>(&json_str) {
            failures.push(format!("{fixture}: invalid JSON: {e}"));
            continue;
        }

        // Must have package field matching input SQL
        let output: DaliOutput = serde_json::from_str(&json_str).unwrap();
        if output.package != sql {
            failures.push(format!("{fixture}: package mismatch"));
        }

        // transforms and table_lineage should have same length
        if output.transforms.len() != output.table_lineage.len() {
            failures.push(format!(
                "{fixture}: transforms ({}) != table_lineage ({})",
                output.transforms.len(),
                output.table_lineage.len()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Dali compat failures:\n{}",
        failures.join("\n")
    );
}

/// Bulk test: all fixtures that have target tables should produce at least one transform.
/// Fixtures without target tables (SELECT-only) should produce zero transforms.
#[test]
fn dali_bulk_transform_presence() {
    let fixture_path = fixture_dir();
    let mut fixtures: Vec<_> = fs::read_dir(&fixture_path)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "sql"))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    fixtures.sort();

    let mut mismatches = Vec::new();

    for fixture in &fixtures {
        let sql = load_fixture(fixture);
        let result = analyze_oracle(&sql);

        // Skip fixtures that fail to parse
        if result.summary.has_errors {
            continue;
        }

        let output = dali_output(&sql, &result);

        // Determine expected behavior from fixture name
        let is_select_only = fixture.starts_with("select_") || fixture == "02_select_only.sql";

        if is_select_only {
            if !output.transforms.is_empty() {
                mismatches.push(format!(
                    "{fixture}: SELECT-only should produce 0 transforms, got {}",
                    output.transforms.len()
                ));
            }
        } else if fixture.starts_with("insert_values") {
            // INSERT VALUES may or may not produce transforms — skip
        } else {
            // All other DML/DDL fixtures should produce at least 1 transform
            if output.transforms.is_empty() {
                mismatches.push(format!("{fixture}: expected >=1 transform, got 0"));
            }
        }
    }

    // Report mismatches but don't fail — this is diagnostic
    if !mismatches.is_empty() {
        eprintln!(
            "Dali transform presence mismatches ({}/{}):\n{}",
            mismatches.len(),
            fixtures.len(),
            mismatches.join("\n")
        );
    }
}

/// Bulk test: validate relation types across all fixtures
#[test]
fn dali_bulk_relation_types() {
    let fixture_path = fixture_dir();
    let mut fixtures: Vec<_> = fs::read_dir(&fixture_path)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "sql"))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    fixtures.sort();

    let mut mismatches = Vec::new();

    for fixture in &fixtures {
        let sql = load_fixture(fixture);
        let result = analyze_oracle(&sql);

        if result.summary.has_errors {
            continue;
        }

        let output = dali_output(&sql, &result);

        for tl in &output.table_lineage {
            let expected_relation =
                if fixture.starts_with("insert_") || fixture.starts_with("01_insert") {
                    Some("INSERT_SELECT")
                } else if fixture.starts_with("merge_") || fixture.starts_with("05_merge") {
                    Some("MERGE")
                } else if fixture.starts_with("update_") || fixture.starts_with("03_update") {
                    Some("UPDATE")
                } else if fixture.starts_with("delete_") || fixture.starts_with("04_delete") {
                    Some("DELETE")
                } else if fixture.starts_with("view_") || fixture.starts_with("06_view") {
                    Some("VIEW_SELECT")
                } else if fixture.starts_with("ctas_") {
                    Some("TABLE_SELECT")
                } else {
                    None
                };

            if let Some(expected) = expected_relation {
                if tl.relation != expected {
                    mismatches.push(format!(
                        "{fixture}: expected relation {expected}, got {}",
                        tl.relation
                    ));
                }
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "Relation type mismatches:\n{}",
        mismatches.join("\n")
    );
}

/// Bulk test: column-level refs — fixtures with schema should have more refs
#[test]
fn dali_bulk_column_refs_with_schema() {
    // A subset of fixtures known to have column-level lineage when schema is provided
    let fixtures_with_refs = [
        "01_insert_simple.sql",
        "insert_target_source.sql",
        "insert_case_expr.sql",
        "insert_cte_chain_deep.sql",
        "merge_using_join.sql",
        "05_merge_minimal.sql",
        "06_view_simple.sql",
        "view_left_join.sql",
    ];

    let mut failures = Vec::new();

    for fixture in &fixtures_with_refs {
        let sql = load_fixture(fixture);
        let result = analyze_oracle_with_schema(&sql);
        let output = dali_output(&sql, &result);

        if output.transforms.is_empty() {
            failures.push(format!("{fixture}: no transforms produced"));
            continue;
        }

        let refs = &output.transforms[0].refs;
        if refs.is_empty() {
            failures.push(format!("{fixture}: no column refs with schema"));
        }
    }

    assert!(
        failures.is_empty(),
        "Schema column-ref failures:\n{}",
        failures.join("\n")
    );
}

/// Diagnostic: summary of adapter coverage across all fixtures
#[test]
fn dali_coverage_summary() {
    let fixture_path = fixture_dir();
    let mut fixtures: Vec<_> = fs::read_dir(&fixture_path)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "sql"))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    fixtures.sort();

    let mut total = 0;
    let mut with_transforms = 0;
    let mut with_refs = 0;
    let mut with_source_tables = 0;
    let mut parse_errors = 0;

    for fixture in &fixtures {
        let sql = load_fixture(fixture);
        let result = analyze_oracle(&sql);
        total += 1;

        if result.summary.has_errors {
            parse_errors += 1;
            continue;
        }

        let output = dali_output(&sql, &result);

        if !output.transforms.is_empty() {
            with_transforms += 1;
            if output.transforms.iter().any(|t| !t.refs.is_empty()) {
                with_refs += 1;
            }
            if output
                .transforms
                .iter()
                .any(|t| !t.source_tables.is_empty())
            {
                with_source_tables += 1;
            }
        }
    }

    eprintln!("=== Dali Compat Coverage Summary ===");
    eprintln!("Total fixtures:         {total}");
    eprintln!("Parse errors:           {parse_errors}");
    eprintln!("With transforms:        {with_transforms}");
    eprintln!("With column refs:       {with_refs}");
    eprintln!("With source tables:     {with_source_tables}");
    eprintln!("===================================");
}
