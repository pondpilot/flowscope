//! Integration tests for the SQL linter.
//!
//! These tests verify that lint issues flow through the analyze() pipeline
//! and appear in AnalyzeResult.issues alongside analysis diagnostics.

use flowscope_core::{
    analyze, issue_codes, AnalysisOptions, AnalyzeRequest, Dialect, LintConfig, Severity,
};

fn run_lint(sql: &str) -> Vec<(String, String)> {
    run_lint_in_dialect(sql, Dialect::Generic)
}

fn run_lint_in_dialect(sql: &str, dialect: Dialect) -> Vec<(String, String)> {
    let result = analyze(&AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect,
        source_name: None,
        options: Some(AnalysisOptions {
            lint: Some(LintConfig::default()),
            ..Default::default()
        }),
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    });
    result
        .issues
        .iter()
        .filter(|i| i.code.starts_with("LINT_"))
        .map(|i| (i.code.clone(), i.message.clone()))
        .collect()
}

fn run_lint_with_config(sql: &str, config: LintConfig) -> Vec<(String, String)> {
    let result = analyze(&AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: Some(AnalysisOptions {
            lint: Some(config),
            ..Default::default()
        }),
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    });
    result
        .issues
        .iter()
        .filter(|i| i.code.starts_with("LINT_"))
        .map(|i| (i.code.clone(), i.message.clone()))
        .collect()
}

// =============================================================================
// Integration: lint issues flow through analyze()
// =============================================================================

#[test]
fn lint_issues_appear_in_analyze_result() {
    let result = analyze(&AnalyzeRequest {
        sql: "SELECT 1 UNION SELECT 2".to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: Some(AnalysisOptions {
            lint: Some(LintConfig::default()),
            ..Default::default()
        }),
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    });

    let lint_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code.starts_with("LINT_"))
        .collect();

    assert!(!lint_issues.is_empty(), "expected lint issues in result");
    assert_eq!(lint_issues[0].code, issue_codes::LINT_AM_002);
    assert_eq!(lint_issues[0].severity, Severity::Warning);
}

#[test]
fn lint_issues_have_statement_index() {
    let result = analyze(&AnalyzeRequest {
        sql: "SELECT 1; SELECT 1 UNION SELECT 2".to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: Some(AnalysisOptions {
            lint: Some(LintConfig::default()),
            ..Default::default()
        }),
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    });

    let lint_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == issue_codes::LINT_AM_002)
        .collect();

    assert_eq!(lint_issues.len(), 1);
    assert_eq!(
        lint_issues[0].statement_index,
        Some(1),
        "lint issue should reference the second statement"
    );
}

// =============================================================================
// Configuration: disable rules
// =============================================================================

#[test]
fn lint_disabled_rule_not_reported() {
    let issues = run_lint_with_config(
        "SELECT 1 UNION SELECT 2",
        LintConfig {
            enabled: true,
            disabled_rules: vec!["LINT_AM_002".to_string()],
            rule_configs: std::collections::BTreeMap::new(),
        },
    );
    assert!(
        issues.is_empty(),
        "disabled rule should not produce issues: {issues:?}"
    );
}

#[test]
fn lint_disabled_globally() {
    let issues = run_lint_with_config(
        "SELECT 1 UNION SELECT 2",
        LintConfig {
            enabled: false,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::new(),
        },
    );
    assert!(
        issues.is_empty(),
        "globally disabled linter should produce no issues"
    );
}

#[test]
fn lint_rule_config_aliasing_table_implicit_policy() {
    let issues = run_lint_with_config(
        "SELECT * FROM users AS u",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.table".to_string(),
                serde_json::json!({"aliasing": "implicit"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_001"),
        "implicit aliasing policy should flag explicit AS: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_column_implicit_policy() {
    let issues = run_lint_with_config(
        "SELECT a + 1 AS value FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_AL_002".to_string(),
                serde_json::json!({"aliasing": "implicit"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_002"),
        "implicit aliasing policy should flag explicit AS: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_length_max() {
    let issues = run_lint_with_config(
        "SELECT * FROM users eleven_chars",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_AL_006".to_string(),
                serde_json::json!({"max_alias_length": 10}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_006"),
        "configured max alias length should flag long alias: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_expression_allow_scalar_false() {
    let issues = run_lint_with_config(
        "SELECT 1 FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_AL_003".to_string(),
                serde_json::json!({"allow_scalar": false}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_003"),
        "allow_scalar=false should flag scalar expression aliases: {issues:?}"
    );
}

#[test]
fn lint_rule_config_not_equal_style_c_style() {
    let issues = run_lint_with_config(
        "SELECT * FROM t WHERE a <> b",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_001".to_string(),
                serde_json::json!({"preferred_not_equal_style": "c_style"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CV_001"),
        "c_style not-equal preference should flag <> usage: {issues:?}"
    );
}

// =============================================================================
// Rule-specific integration tests
// =============================================================================

#[test]
fn lint_am_001_bare_union() {
    let issues = run_lint("SELECT 1 UNION SELECT 2");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_002"));
}

#[test]
fn lint_am_001_union_all_ok() {
    let issues = run_lint("SELECT 1 UNION ALL SELECT 2");
    assert!(!issues.iter().any(|(code, _)| code == "LINT_AM_002"));
}

#[test]
fn lint_am_001_enabled_for_postgres_dialect() {
    let issues = run_lint_in_dialect("SELECT 1 UNION SELECT 2", Dialect::Postgres);
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AM_002"),
        "AM_001 should be enabled for postgres dialect: {issues:?}"
    );
}

#[test]
fn lint_am_001_enabled_for_redshift_dialect() {
    let issues = run_lint_in_dialect("SELECT 1 UNION SELECT 2", Dialect::Redshift);
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AM_002"),
        "AM_001 should be enabled for redshift dialect: {issues:?}"
    );
}

#[test]
fn lint_am_002_limit_without_order_by() {
    let issues = run_lint("SELECT * FROM t LIMIT 10");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_009"));
}

#[test]
fn lint_am_002_limit_with_order_by_ok() {
    let issues = run_lint("SELECT * FROM t ORDER BY id LIMIT 10");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AM_009"),
        "LIMIT with ORDER BY should not trigger AM_002: {issues:?}"
    );
}

#[test]
fn lint_am_003_distinct_with_group_by() {
    let issues = run_lint("SELECT DISTINCT col FROM t GROUP BY col");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_001"));
}

#[test]
fn lint_am_004_unknown_result_columns() {
    let issues = run_lint("SELECT * FROM t");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_004"));
}

#[test]
fn lint_cv_002_count_one() {
    let issues = run_lint("SELECT COUNT(1) FROM t");
    assert!(issues.iter().any(|(code, _)| code == "LINT_CV_004"));
}

#[test]
fn lint_cv_003_null_comparison() {
    let issues = run_lint("SELECT * FROM t WHERE a = NULL");
    assert!(issues.iter().any(|(code, _)| code == "LINT_CV_005"));
}

#[test]
fn lint_cv_004_right_join() {
    let issues = run_lint("SELECT * FROM a RIGHT JOIN b ON a.id = b.id");
    assert!(issues.iter().any(|(code, _)| code == "LINT_CV_008"));
}

#[test]
fn lint_st_001_unused_cte() {
    let issues = run_lint("WITH unused AS (SELECT 1) SELECT 2");
    assert!(issues.iter().any(|(code, _)| code == "LINT_ST_003"));
}

#[test]
fn lint_st_002_else_null() {
    let issues = run_lint("SELECT CASE WHEN x > 1 THEN 'a' ELSE NULL END FROM t");
    assert!(issues.iter().any(|(code, _)| code == "LINT_ST_001"));
}

#[test]
fn lint_st_003_flattenable_else_case() {
    let sql = "SELECT CASE WHEN species = 'Rat' THEN 'Squeak' ELSE CASE WHEN species = 'Dog' THEN 'Woof' END END AS sound FROM mytable";
    let issues = run_lint(sql);
    assert!(issues.iter().any(|(code, _)| code == "LINT_ST_004"));
}

#[test]
fn lint_st_004_using_join() {
    let issues = run_lint("SELECT * FROM a JOIN b USING (id)");
    assert!(issues.iter().any(|(code, _)| code == "LINT_ST_007"));
}

#[test]
fn lint_al_001_implicit_alias() {
    let issues = run_lint("SELECT a + b FROM t");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AL_003"));
}

#[test]
fn lint_al_001_explicit_alias_ok() {
    let issues = run_lint("SELECT a + b AS total FROM t");
    assert!(!issues.iter().any(|(code, _)| code == "LINT_AL_003"));
}

#[test]
fn lint_cv_001_ifnull_usage() {
    let issues = run_lint("SELECT IFNULL(x, 'default') FROM t");
    assert!(issues.iter().any(|(code, _)| code == "LINT_CV_002"));
}

#[test]
fn lint_clean_query_no_issues() {
    let issues = run_lint_with_config(
        "SELECT id, name FROM users WHERE active = true",
        LintConfig {
            enabled: true,
            disabled_rules: vec!["LINT_LT_009".to_string(), "LINT_LT_014".to_string()],
            rule_configs: std::collections::BTreeMap::new(),
        },
    );
    assert!(
        issues.is_empty(),
        "clean query should produce no lint issues: {issues:?}"
    );
}

// =============================================================================
// Edge cases: rules work inside different statement contexts
// =============================================================================

#[test]
fn lint_multiple_rules_on_single_query() {
    // This query triggers both LINT_AM_002 (bare UNION) and LINT_AL_003 (implicit alias)
    let issues = run_lint("SELECT a + b UNION SELECT c + d");
    let codes: Vec<&str> = issues.iter().map(|(c, _)| c.as_str()).collect();
    assert!(
        codes.contains(&"LINT_AM_002"),
        "expected bare union: {codes:?}"
    );
    assert!(
        codes.contains(&"LINT_AL_003"),
        "expected implicit alias: {codes:?}"
    );
}

#[test]
fn lint_unused_cte_case_insensitive() {
    // CTE name case shouldn't matter
    let issues = run_lint("WITH My_Cte AS (SELECT 1) SELECT * FROM my_cte");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_003"),
        "case-insensitive CTE name should be recognized as used"
    );
}

#[test]
fn lint_chained_ctes_all_used() {
    let issues = run_lint(
        "WITH a AS (SELECT 1 AS x), b AS (SELECT * FROM a), c AS (SELECT * FROM b) SELECT * FROM c",
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_003"),
        "chained CTEs should all count as used"
    );
}

#[test]
fn lint_bare_union_in_create_view() {
    let issues = run_lint("CREATE VIEW v AS SELECT 1 UNION SELECT 2");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_002"));
}

#[test]
fn lint_else_null_nested_both_detected() {
    let issues =
        run_lint("SELECT CASE WHEN a THEN CASE WHEN b THEN 1 ELSE NULL END ELSE NULL END FROM t");
    let st002_count = issues
        .iter()
        .filter(|(code, _)| code == "LINT_ST_001")
        .count();
    assert_eq!(st002_count, 2, "both nested ELSE NULLs should be detected");
}

#[test]
fn lint_disable_multiple_rules() {
    let issues = run_lint_with_config(
        "SELECT a + b UNION SELECT c + d",
        LintConfig {
            enabled: true,
            disabled_rules: vec!["LINT_AM_002".to_string(), "LINT_AL_003".to_string()],
            rule_configs: std::collections::BTreeMap::new(),
        },
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == "LINT_AM_002" || code == "LINT_AL_003"),
        "disabled rules should not appear: {issues:?}"
    );
}

#[test]
fn lint_count_one_in_having() {
    let issues = run_lint("SELECT col FROM t GROUP BY col HAVING COUNT(1) > 5");
    assert!(issues.iter().any(|(code, _)| code == "LINT_CV_004"));
}

#[test]
fn lint_limit_in_subquery_without_order_by() {
    let issues = run_lint("SELECT * FROM (SELECT * FROM t LIMIT 10) AS sub");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_009"));
}

#[test]
fn lint_clean_complex_query_no_issues() {
    // A well-written query should produce no lint issues once optional style-only
    // parity rules are disabled for this integration assertion.
    let issues = run_lint_with_config(
        "WITH recent_orders AS (SELECT * FROM orders ORDER BY user_id LIMIT 100) \
         SELECT u.name, COUNT(*) AS order_count \
         FROM users u \
         JOIN recent_orders o ON u.id = o.user_id \
         GROUP BY u.name \
         ORDER BY order_count DESC",
        LintConfig {
            enabled: true,
            disabled_rules: vec![
                "LINT_LT_009".to_string(),
                "LINT_LT_008".to_string(),
                "LINT_LT_005".to_string(),
                "LINT_AL_001".to_string(),
                "LINT_AM_005".to_string(),
                "LINT_ST_011".to_string(),
            ],
            rule_configs: std::collections::BTreeMap::new(),
        },
    );
    assert!(
        issues.is_empty(),
        "well-structured query should have no lint issues: {issues:?}"
    );
}

// =============================================================================
// Serialization: LintConfig round-trips through JSON
// =============================================================================

#[test]
fn lint_config_serialization() {
    let config = LintConfig {
        enabled: true,
        disabled_rules: vec!["LINT_AM_002".to_string()],
        rule_configs: std::collections::BTreeMap::new(),
    };
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"disabledRules\""));
    let deserialized: LintConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.disabled_rules, vec!["LINT_AM_002"]);
}

#[test]
fn lint_config_in_analyze_request() {
    let json = r#"{
        "sql": "SELECT 1",
        "dialect": "generic",
        "options": {
            "lint": {
                "enabled": true,
                "disabledRules": ["LINT_AM_002"]
            }
        }
    }"#;
    let request: AnalyzeRequest = serde_json::from_str(json).unwrap();
    let lint = request.options.unwrap().lint.unwrap();
    assert!(lint.enabled);
    assert_eq!(lint.disabled_rules, vec!["LINT_AM_002"]);
}

// =============================================================================
// Negative tests: rules should NOT fire on clean SQL
// =============================================================================

#[test]
fn lint_am_004_known_columns_ok() {
    let issues = run_lint("WITH cte AS (SELECT a, b FROM t) SELECT * FROM cte");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AM_004"),
        "known output width should not trigger AM_004: {issues:?}"
    );
}

#[test]
fn lint_cv_003_is_null_ok() {
    let issues = run_lint("SELECT * FROM t WHERE a IS NULL");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_CV_005"),
        "IS NULL should not trigger CV_003: {issues:?}"
    );
}

#[test]
fn lint_cv_004_left_join_ok() {
    let issues = run_lint("SELECT * FROM a LEFT JOIN b ON a.id = b.id");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_CV_008"),
        "LEFT JOIN should not trigger CV_004: {issues:?}"
    );
}

#[test]
fn lint_st_004_on_join_ok() {
    let issues = run_lint("SELECT * FROM a JOIN b ON a.id = b.id");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_007"),
        "JOIN ON should not trigger ST_004: {issues:?}"
    );
}

#[test]
fn lint_al_003_explicit_aliases_ok() {
    let issues =
        run_lint("SELECT a.id, b.name FROM users AS a JOIN orders AS b ON a.id = b.user_id");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AL_003"),
        "explicit aliases should not trigger AL_003: {issues:?}"
    );
}

#[test]
fn lint_am_005_order_by_name_ok() {
    let issues = run_lint("SELECT name FROM t ORDER BY name");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AM_003"),
        "ORDER BY column name should not trigger AM_005: {issues:?}"
    );
}

#[test]
fn lint_am_005_mixed_order_by_direction() {
    let issues = run_lint("SELECT a, b FROM t ORDER BY a, b DESC");
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AM_003"),
        "mixed ORDER BY direction should trigger AM_005: {issues:?}"
    );
}

#[test]
fn lint_cp_001_consistent_case_ok() {
    let issues = run_lint("SELECT id FROM users WHERE active = true");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_CP_001"),
        "consistent keyword case should not trigger CP_001: {issues:?}"
    );
}

#[test]
fn lint_st_010_no_constant_expression_ok() {
    let issues = run_lint("SELECT * FROM t WHERE status = 'active'");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_010"),
        "normal WHERE should not trigger ST_010: {issues:?}"
    );
}

#[test]
fn lint_lt_007_cte_bracket_missing() {
    let issues = run_lint("SELECT 'WITH cte AS SELECT 1' AS sql_snippet");
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_LT_007),
        "expected {}: {issues:?}",
        issue_codes::LINT_LT_007,
    );
}

#[test]
fn lint_rf_004_keyword_identifier() {
    let issues = run_lint("SELECT \"select\".id FROM users AS \"select\"");
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_RF_004),
        "expected {}: {issues:?}",
        issue_codes::LINT_RF_004,
    );
}

// =============================================================================
// SQLFluff parity smoke tests
// =============================================================================

#[test]
fn lint_sqlfluff_parity_rule_smoke_cases() {
    let cases = [
        ("LINT_AL_001", "SELECT * FROM a x JOIN b y ON x.id = y.id"),
        ("LINT_AL_002", "SELECT a + 1 AS x, b + 2 y FROM t"),
        ("LINT_AL_004", "SELECT * FROM a t JOIN b t ON t.id = t.id"),
        (
            "LINT_AL_006",
            "SELECT * FROM a x JOIN b yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy ON x.id = yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy.id",
        ),
        ("LINT_AL_007", "SELECT * FROM users u"),
        ("LINT_AL_008", "SELECT a AS x, b AS x FROM t"),
        ("LINT_AL_009", "SELECT a AS a FROM t"),
        ("LINT_AM_009", "SELECT a FROM t LIMIT 10"),
        ("LINT_AM_003", "SELECT a, b FROM t ORDER BY a, b DESC"),
        ("LINT_AM_005", "SELECT * FROM a JOIN b ON a.id = b.id"),
        ("LINT_AM_006", "SELECT foo, bar FROM fake_table GROUP BY 1, bar"),
        ("LINT_AM_007", "WITH cte AS (SELECT a, b, c FROM t) SELECT * FROM cte UNION SELECT d, e FROM t2"),
        ("LINT_AM_008", "SELECT foo.a, bar.b FROM foo INNER JOIN bar"),
        ("LINT_CP_001", "SELECT a from t"),
        ("LINT_CP_002", "SELECT Col, col FROM t"),
        ("LINT_CP_003", "SELECT COUNT(*), count(name) FROM t"),
        ("LINT_CP_004", "SELECT NULL, true FROM t"),
        ("LINT_CP_005", "CREATE TABLE t (a INT, b varchar(10))"),
        ("LINT_CV_001", "SELECT * FROM t WHERE a <> b AND c != d"),
        ("LINT_CV_002", "SELECT IFNULL(a, 0) FROM t"),
        ("LINT_CV_003", "SELECT a, FROM t"),
        ("LINT_CV_006", "SELECT 1; SELECT 2"),
        ("LINT_CV_007", "(SELECT 1)"),
        ("LINT_CV_009", "SELECT foo FROM t"),
        ("LINT_CV_010", "SELECT \"abc\" FROM t"),
        ("LINT_CV_011", "SELECT CAST(a AS INT)::TEXT FROM t"),
        ("LINT_CV_012", "SELECT foo.a, bar.b FROM foo JOIN bar WHERE foo.x = bar.y"),
        ("LINT_JJ_001", "SELECT '{{foo}}' AS templated"),
        ("LINT_LT_001", "SELECT payload->>'id' FROM t"),
        ("LINT_LT_002", "SELECT a\n   , b\nFROM t"),
        ("LINT_LT_003", "SELECT a +\n b FROM t"),
        ("LINT_LT_004", "SELECT a,b FROM t"),
        (
            "LINT_LT_005",
            "SELECT this_is_a_very_long_column_name_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx FROM t",
        ),
        ("LINT_LT_006", "SELECT COUNT (1) FROM t"),
        ("LINT_LT_008", "WITH cte AS (SELECT 1) SELECT * FROM cte"),
        ("LINT_LT_009", "SELECT a,b,c,d,e FROM t"),
        ("LINT_LT_010", "SELECT\nDISTINCT a\nFROM t"),
        ("LINT_LT_011", "SELECT 1 UNION SELECT 2\nUNION SELECT 3"),
        ("LINT_LT_012", "SELECT 1\nFROM t"),
        ("LINT_LT_013", "\n\nSELECT 1"),
        ("LINT_LT_014", "SELECT a FROM t WHERE a = 1 ORDER BY a"),
        ("LINT_LT_014", "SELECT a FROM t\nWHERE a = 1"),
        ("LINT_LT_015", "SELECT 1\n\n\nFROM t"),
        ("LINT_RF_001", "SELECT x.a FROM t"),
        ("LINT_RF_002", "SELECT id FROM a JOIN b ON a.id = b.id"),
        ("LINT_RF_003", "SELECT a.id, id2 FROM a"),
        ("LINT_RF_005", "SELECT \"bad-name\" FROM t"),
        ("LINT_RF_006", "SELECT \"good_name\" FROM t"),
        (
            "LINT_ST_002",
            "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' END FROM t",
        ),
        (
            "LINT_ST_004",
            "SELECT CASE WHEN species = 'Rat' THEN 'Squeak' ELSE CASE WHEN species = 'Dog' THEN 'Woof' END END FROM t",
        ),
        ("LINT_ST_005", "SELECT * FROM (SELECT * FROM t) sub"),
        ("LINT_ST_006", "SELECT a + 1, a FROM t"),
        ("LINT_ST_008", "SELECT DISTINCT(a) FROM t"),
        ("LINT_ST_009", "SELECT * FROM a x JOIN b y ON y.id = x.id"),
        ("LINT_ST_010", "SELECT * FROM t WHERE col = col"),
        ("LINT_ST_011", "SELECT a.id FROM a LEFT JOIN b b1 ON a.id = b1.id"),
        ("LINT_ST_012", "SELECT 1;;"),
        ("LINT_TQ_001", "CREATE PROCEDURE sp_legacy AS SELECT 1;"),
        ("LINT_TQ_002", "CREATE PROCEDURE p AS SELECT 1;"),
    ];

    for (code, sql) in cases {
        let issues = run_lint(sql);
        assert!(
            issues.iter().any(|(c, _)| c == code),
            "expected {code} for SQL: {sql}; got: {issues:?}"
        );
    }
}
