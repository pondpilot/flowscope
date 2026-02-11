//! Integration tests for the SQL linter.
//!
//! These tests verify that lint issues flow through the analyze() pipeline
//! and appear in AnalyzeResult.issues alongside analysis diagnostics.

use flowscope_core::{
    analyze, issue_codes, AnalysisOptions, AnalyzeRequest, Dialect, LintConfig, Severity,
};

fn run_lint(sql: &str) -> Vec<(String, String)> {
    let result = analyze(&AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: None,
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
        options: None,
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
    assert_eq!(lint_issues[0].code, issue_codes::LINT_AM_001);
    assert_eq!(lint_issues[0].severity, Severity::Warning);
}

#[test]
fn lint_issues_have_statement_index() {
    let result = analyze(&AnalyzeRequest {
        sql: "SELECT 1; SELECT 1 UNION SELECT 2".to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: None,
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    });

    let lint_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.code == issue_codes::LINT_AM_001)
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
            disabled_rules: vec!["LINT_AM_001".to_string()],
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
        },
    );
    assert!(
        issues.is_empty(),
        "globally disabled linter should produce no issues"
    );
}

// =============================================================================
// Rule-specific integration tests
// =============================================================================

#[test]
fn lint_am_001_bare_union() {
    let issues = run_lint("SELECT 1 UNION SELECT 2");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_001"));
}

#[test]
fn lint_am_001_union_all_ok() {
    let issues = run_lint("SELECT 1 UNION ALL SELECT 2");
    assert!(!issues.iter().any(|(code, _)| code == "LINT_AM_001"));
}

#[test]
fn lint_am_002_order_by_in_cte() {
    let issues = run_lint("WITH cte AS (SELECT * FROM t ORDER BY id) SELECT * FROM cte");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_002"));
}

#[test]
fn lint_am_003_distinct_with_group_by() {
    let issues = run_lint("SELECT DISTINCT col FROM t GROUP BY col");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_003"));
}

#[test]
fn lint_cv_002_count_one() {
    let issues = run_lint("SELECT COUNT(1) FROM t");
    assert!(issues.iter().any(|(code, _)| code == "LINT_CV_002"));
}

#[test]
fn lint_st_001_unused_cte() {
    let issues = run_lint("WITH unused AS (SELECT 1) SELECT 2");
    assert!(issues.iter().any(|(code, _)| code == "LINT_ST_001"));
}

#[test]
fn lint_st_002_else_null() {
    let issues = run_lint("SELECT CASE WHEN x > 1 THEN 'a' ELSE NULL END FROM t");
    assert!(issues.iter().any(|(code, _)| code == "LINT_ST_002"));
}

#[test]
fn lint_st_003_nested_case() {
    let sql = "SELECT CASE WHEN a THEN CASE WHEN b THEN CASE WHEN c THEN CASE WHEN d THEN 1 END END END END FROM t";
    let issues = run_lint(sql);
    assert!(issues.iter().any(|(code, _)| code == "LINT_ST_003"));
}

#[test]
fn lint_al_001_implicit_alias() {
    let issues = run_lint("SELECT a + b FROM t");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AL_001"));
}

#[test]
fn lint_al_001_explicit_alias_ok() {
    let issues = run_lint("SELECT a + b AS total FROM t");
    assert!(!issues.iter().any(|(code, _)| code == "LINT_AL_001"));
}

#[test]
fn lint_cv_001_coalesce_pattern() {
    let issues = run_lint("SELECT CASE WHEN x IS NULL THEN 'default' ELSE x END FROM t");
    assert!(issues.iter().any(|(code, _)| code == "LINT_CV_001"));
}

#[test]
fn lint_clean_query_no_issues() {
    let issues = run_lint("SELECT id, name FROM users WHERE active = true");
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
    // This query triggers both LINT_AM_001 (bare UNION) and LINT_AL_001 (implicit alias)
    let issues = run_lint("SELECT a + b UNION SELECT c + d");
    let codes: Vec<&str> = issues.iter().map(|(c, _)| c.as_str()).collect();
    assert!(
        codes.contains(&"LINT_AM_001"),
        "expected bare union: {codes:?}"
    );
    assert!(
        codes.contains(&"LINT_AL_001"),
        "expected implicit alias: {codes:?}"
    );
}

#[test]
fn lint_unused_cte_case_insensitive() {
    // CTE name case shouldn't matter
    let issues = run_lint("WITH My_Cte AS (SELECT 1) SELECT * FROM my_cte");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_001"),
        "case-insensitive CTE name should be recognized as used"
    );
}

#[test]
fn lint_chained_ctes_all_used() {
    let issues = run_lint(
        "WITH a AS (SELECT 1 AS x), b AS (SELECT * FROM a), c AS (SELECT * FROM b) SELECT * FROM c",
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_001"),
        "chained CTEs should all count as used"
    );
}

#[test]
fn lint_bare_union_in_create_view() {
    let issues = run_lint("CREATE VIEW v AS SELECT 1 UNION SELECT 2");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_001"));
}

#[test]
fn lint_else_null_nested_both_detected() {
    let issues =
        run_lint("SELECT CASE WHEN a THEN CASE WHEN b THEN 1 ELSE NULL END ELSE NULL END FROM t");
    let st002_count = issues
        .iter()
        .filter(|(code, _)| code == "LINT_ST_002")
        .count();
    assert_eq!(st002_count, 2, "both nested ELSE NULLs should be detected");
}

#[test]
fn lint_disable_multiple_rules() {
    let issues = run_lint_with_config(
        "SELECT a + b UNION SELECT c + d",
        LintConfig {
            enabled: true,
            disabled_rules: vec!["LINT_AM_001".to_string(), "LINT_AL_001".to_string()],
        },
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == "LINT_AM_001" || code == "LINT_AL_001"),
        "disabled rules should not appear: {issues:?}"
    );
}

#[test]
fn lint_count_one_in_having() {
    let issues = run_lint("SELECT col FROM t GROUP BY col HAVING COUNT(1) > 5");
    assert!(issues.iter().any(|(code, _)| code == "LINT_CV_002"));
}

#[test]
fn lint_order_by_in_subquery_no_limit() {
    let issues = run_lint("SELECT * FROM (SELECT * FROM t ORDER BY id) AS sub");
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_002"));
}

#[test]
fn lint_clean_complex_query_no_issues() {
    // A well-written query should produce no lint issues
    let issues = run_lint(
        "WITH recent_orders AS (SELECT * FROM orders LIMIT 100) \
         SELECT u.name, COUNT(*) AS order_count \
         FROM users u \
         JOIN recent_orders o ON u.id = o.user_id \
         GROUP BY u.name \
         ORDER BY order_count DESC",
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
                "disabledRules": ["LINT_AM_001"]
            }
        }
    }"#;
    let request: AnalyzeRequest = serde_json::from_str(json).unwrap();
    let lint = request.options.unwrap().lint.unwrap();
    assert!(lint.enabled);
    assert_eq!(lint.disabled_rules, vec!["LINT_AM_001"]);
}
