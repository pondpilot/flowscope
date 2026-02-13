//! Integration tests for the SQL linter.
//!
//! These tests verify that lint issues flow through the analyze() pipeline
//! and appear in AnalyzeResult.issues alongside analysis diagnostics.

use flowscope_core::{
    analyze, issue_codes, AnalysisOptions, AnalyzeRequest, Dialect, LintConfig, Severity,
};
#[cfg(feature = "templating")]
use flowscope_core::{TemplateConfig, TemplateMode};

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
    run_lint_with_config_in_dialect(sql, Dialect::Generic, config)
}

fn run_lint_with_config_in_dialect(
    sql: &str,
    dialect: Dialect,
    config: LintConfig,
) -> Vec<(String, String)> {
    let result = analyze(&AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect,
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

#[cfg(feature = "templating")]
fn run_lint_in_dialect_with_jinja_template(sql: &str, dialect: Dialect) -> Vec<(String, String)> {
    run_lint_with_config_in_dialect_with_jinja_template(sql, dialect, LintConfig::default())
}

#[cfg(feature = "templating")]
fn run_lint_with_config_in_dialect_with_jinja_template(
    sql: &str,
    dialect: Dialect,
    config: LintConfig,
) -> Vec<(String, String)> {
    let result = analyze(&AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect,
        source_name: None,
        options: Some(AnalysisOptions {
            lint: Some(config),
            ..Default::default()
        }),
        schema: None,
        template_config: Some(TemplateConfig {
            mode: TemplateMode::Jinja,
            context: std::collections::HashMap::new(),
        }),
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
        "SELECT 1\nUNION\nSELECT 2\n",
        LintConfig {
            enabled: true,
            disabled_rules: vec!["LINT_AM_002".to_string()],
            rule_configs: std::collections::BTreeMap::new(),
        },
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AM_002),
        "disabled rule should not be reported: {issues:?}"
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

#[test]
fn lint_rule_config_count_rows_prefer_count_one() {
    let issues = run_lint_with_config(
        "SELECT COUNT(*) FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.count_rows".to_string(),
                serde_json::json!({"prefer_count_1": true}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CV_004"),
        "prefer_count_1 should flag COUNT(*): {issues:?}"
    );
}

#[test]
fn lint_rule_config_terminator_require_final_semicolon() {
    let issues = run_lint_with_config(
        "SELECT 1",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.terminator".to_string(),
                serde_json::json!({"require_final_semicolon": true}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CV_006"),
        "require_final_semicolon should flag missing final semicolon: {issues:?}"
    );
}

#[test]
fn lint_rule_config_terminator_require_final_semicolon_mssql_go_batches() {
    let result = analyze(&AnalyzeRequest {
        sql: "CREATE SCHEMA staging;\nGO\nCREATE TABLE test (id INT)\n".to_string(),
        files: None,
        dialect: Dialect::Mssql,
        source_name: None,
        options: Some(AnalysisOptions {
            lint: Some(LintConfig {
                enabled: true,
                disabled_rules: vec![],
                rule_configs: std::collections::BTreeMap::from([(
                    "convention.terminator".to_string(),
                    serde_json::json!({"require_final_semicolon": true}),
                )]),
            }),
            ..Default::default()
        }),
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    });

    assert!(
        !result
            .issues
            .iter()
            .any(|issue| issue.code == issue_codes::PARSE_ERROR),
        "MSSQL GO batches should not fail parsing in lint mode: {:?}",
        result.issues
    );
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_CV_006),
        "require_final_semicolon should flag last statement after GO batch separator: {:?}",
        result.issues
    );
}

#[test]
fn lint_rule_config_select_trailing_comma_require() {
    let issues = run_lint_with_config(
        "SELECT a FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.select_trailing_comma".to_string(),
                serde_json::json!({"select_clause_trailing_comma": "require"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CV_003"),
        "require trailing comma policy should flag missing trailing comma: {issues:?}"
    );
}

#[test]
fn lint_rule_config_blocked_words_custom_list() {
    let issues = run_lint_with_config(
        "SELECT wip FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.blocked_words".to_string(),
                serde_json::json!({"blocked_words": ["wip"]}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CV_009"),
        "blocked_words custom list should flag configured terms: {issues:?}"
    );
}

#[test]
fn lint_rule_config_quoted_literals_double_quotes_preference() {
    let issues = run_lint_with_config(
        "SELECT 'abc' FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CV_010".to_string(),
                serde_json::json!({"preferred_quoted_literal_style": "double_quotes"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CV_010"),
        "double_quotes preference should flag single-quoted literals: {issues:?}"
    );
}

#[test]
fn lint_rule_config_casting_style_shorthand() {
    let issues = run_lint_with_config(
        "SELECT CAST(a AS INT) FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "convention.casting_style".to_string(),
                serde_json::json!({"preferred_type_casting_style": "shorthand"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CV_011"),
        "shorthand casting style should flag CAST(...) usage: {issues:?}"
    );
}

#[test]
fn lint_rule_config_long_lines_max_line_length() {
    let issues = run_lint_with_config(
        "SELECT this_is_far_too_long FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.long_lines".to_string(),
                serde_json::json!({"max_line_length": 20}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_LT_005"),
        "configured max_line_length should flag long lines: {issues:?}"
    );
}

#[test]
fn lint_rule_config_long_lines_ignore_comment_lines() {
    let sql = format!("SELECT 1;\n-- {}\nSELECT 2", "x".repeat(120));
    let issues = run_lint_with_config(
        &sql,
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.long_lines".to_string(),
                serde_json::json!({
                    "max_line_length": 20,
                    "ignore_comment_lines": true
                }),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_LT_005"),
        "ignore_comment_lines=true should suppress LT_005 on comment-only lines: {issues:?}"
    );
}

#[test]
fn lint_rule_config_long_lines_ignore_comment_clauses() {
    let sql = format!("SELECT 1 -- {}", "x".repeat(120));
    let issues = run_lint_with_config(
        &sql,
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.long_lines".to_string(),
                serde_json::json!({
                    "max_line_length": 20,
                    "ignore_comment_clauses": true
                }),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_LT_005"),
        "ignore_comment_clauses=true should suppress LT_005 on trailing comments: {issues:?}"
    );
}

#[test]
fn lint_rule_config_layout_newlines_inside_limit() {
    let issues = run_lint_with_config(
        "SELECT 1\n\n\nFROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.newlines".to_string(),
                serde_json::json!({"maximum_empty_lines_inside_statements": 2}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_LT_015"),
        "inside limit=2 should allow two blank lines inside statements: {issues:?}"
    );
}

#[test]
fn lint_rule_config_layout_newlines_between_limit() {
    let issues = run_lint_with_config(
        "SELECT 1;\n\n\nSELECT 2",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_LT_015".to_string(),
                serde_json::json!({"maximum_empty_lines_between_statements": 1}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_LT_015"),
        "between limit=1 should flag larger inter-statement gaps: {issues:?}"
    );
}

#[test]
fn lint_rule_config_layout_set_operators_leading_position() {
    let issues = run_lint_with_config(
        "SELECT 1\nUNION SELECT 2\nUNION SELECT 3",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.set_operators".to_string(),
                serde_json::json!({"line_position": "leading"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_LT_011"),
        "line_position=leading should allow leading set operators: {issues:?}"
    );
}

#[test]
fn lint_rule_config_layout_set_operators_trailing_position() {
    let issues = run_lint_with_config(
        "SELECT 1\nUNION SELECT 2",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_LT_011".to_string(),
                serde_json::json!({"line_position": "trailing"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_LT_011"),
        "line_position=trailing should flag leading set operators: {issues:?}"
    );
}

#[test]
fn lint_rule_config_select_targets_wildcard_policy_multiple() {
    let issues = run_lint_with_config(
        "SELECT * FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_LT_009".to_string(),
                serde_json::json!({"wildcard_policy": "multiple"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_LT_009"),
        "wildcard_policy=multiple should flag single-line wildcard target: {issues:?}"
    );
}

#[test]
fn lint_rule_config_layout_operators_trailing_line_position() {
    let issues = run_lint_with_config(
        "SELECT a\n + b FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.operators".to_string(),
                serde_json::json!({"line_position": "trailing"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_LT_003"),
        "line_position=trailing should flag leading operators: {issues:?}"
    );
}

#[test]
fn lint_rule_config_layout_commas_leading_line_position() {
    let issues = run_lint_with_config(
        "SELECT a,\n b FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.commas".to_string(),
                serde_json::json!({"line_position": "leading"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_LT_004"),
        "line_position=leading should flag trailing commas: {issues:?}"
    );
}

#[test]
fn lint_rule_config_layout_indent_custom_unit() {
    let issues = run_lint_with_config(
        "SELECT a\n  , b\nFROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.indent".to_string(),
                serde_json::json!({"indent_unit": 4}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_LT_002"),
        "indent_unit=4 should flag two-space indentation: {issues:?}"
    );
}

#[test]
fn lint_rule_config_structure_subquery_forbid_join_only() {
    let issues = run_lint_with_config(
        "SELECT * FROM (SELECT * FROM t) sub",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "structure.subquery".to_string(),
                serde_json::json!({"forbid_subquery_in": "join"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_005"),
        "forbid_subquery_in=join should not flag FROM subqueries: {issues:?}"
    );
}

#[test]
fn lint_st_005_allows_correlated_join_subquery_with_outer_alias_reference() {
    let issues = run_lint(
        "SELECT pd.* \
         FROM person_dates AS pd \
         JOIN (SELECT * FROM events AS ce WHERE ce.name = pd.name)",
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_ST_005),
        "correlated JOIN subqueries should not trigger ST_005: {issues:?}"
    );
}

#[test]
fn lint_st_005_allows_correlated_join_subquery_with_outer_table_name_reference() {
    let issues = run_lint(
        "SELECT pd.* \
         FROM person_dates AS pd \
         JOIN (SELECT * FROM events AS ce WHERE ce.name = person_dates.name)",
    );
    assert!(
        !issues.iter().any(|(code, _)| code == issue_codes::LINT_ST_005),
        "correlated JOIN subqueries referencing outer table names should not trigger ST_005: {issues:?}"
    );
}

#[test]
fn lint_rule_config_join_condition_order_later_preference() {
    let issues = run_lint_with_config(
        "SELECT * FROM foo JOIN bar ON foo.id = bar.id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_ST_009".to_string(),
                serde_json::json!({"preferred_first_table_in_join_clause": "later"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_ST_009"),
        "later join condition preference should flag earlier-first ordering: {issues:?}"
    );
}

#[test]
fn lint_rule_config_references_from_force_enable_false() {
    let issues = run_lint_with_config(
        "SELECT * FROM my_tbl WHERE foo.bar > 0",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.from".to_string(),
                serde_json::json!({"force_enable": false}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_RF_001"),
        "force_enable=false should disable RF_001 checks: {issues:?}"
    );
}

#[test]
fn lint_rule_config_references_qualification_force_enable_false() {
    let issues = run_lint_with_config(
        "SELECT id FROM a JOIN b ON a.id = b.id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.qualification".to_string(),
                serde_json::json!({"force_enable": false}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_RF_002"),
        "force_enable=false should disable RF_002 checks: {issues:?}"
    );
}

#[test]
fn lint_rule_config_references_consistent_qualified_mode() {
    let issues = run_lint_with_config(
        "SELECT bar FROM my_tbl",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.consistent".to_string(),
                serde_json::json!({"single_table_references": "qualified"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_RF_003"),
        "single_table_references=qualified should flag unqualified refs: {issues:?}"
    );
}

#[test]
fn lint_rule_config_references_quoting_prefer_quoted_identifiers() {
    let issues = run_lint_with_config(
        "SELECT \"good_name\" FROM \"t\"",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_RF_006".to_string(),
                serde_json::json!({"prefer_quoted_identifiers": true}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_RF_006"),
        "prefer_quoted_identifiers=true should suppress unnecessary-quote warnings: {issues:?}"
    );
}

#[test]
fn lint_rule_config_references_quoting_prefer_quoted_identifiers_flags_unquoted() {
    let issues = run_lint_with_config(
        "SELECT good_name FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_RF_006".to_string(),
                serde_json::json!({"prefer_quoted_identifiers": true}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_RF_006"),
        "prefer_quoted_identifiers=true should flag unquoted identifiers: {issues:?}"
    );
}

#[test]
fn lint_rule_config_references_keywords_quoted_policy_all() {
    let issues = run_lint_with_config(
        "SELECT \"select\".id FROM users AS \"select\"",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.keywords".to_string(),
                serde_json::json!({"quoted_identifiers_policy": "all"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_RF_004"),
        "quoted_identifiers_policy=all should flag quoted keyword identifiers: {issues:?}"
    );
}

#[test]
fn lint_rule_config_references_special_chars_additional_allowed() {
    let issues = run_lint_with_config(
        "SELECT \"bad-name\" FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "references.special_chars".to_string(),
                serde_json::json!({"additional_allowed_characters": "-"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_RF_005"),
        "additional_allowed_characters should suppress RF_005 for configured chars: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_forbid_force_enable_false() {
    let issues = run_lint_with_config(
        "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.forbid".to_string(),
                serde_json::json!({"force_enable": false}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AL_007"),
        "force_enable=false should disable AL_007 checks: {issues:?}"
    );
}

#[test]
fn lint_al_007_disabled_by_default() {
    let issues = run_lint("SELECT * FROM users u JOIN orders o ON u.id = o.user_id");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AL_007"),
        "AL_007 should be disabled by default unless force_enable=true: {issues:?}"
    );
}

#[test]
fn lint_al_007_flags_unnecessary_aliases_in_multi_source_non_self_join_query() {
    let issues = run_lint_with_config(
        "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.forbid".to_string(),
                serde_json::json!({"force_enable": true}),
            )]),
        },
    );
    let al07_count = issues
        .iter()
        .filter(|(code, _)| code == "LINT_AL_007")
        .count();
    assert_eq!(
        al07_count, 2,
        "both non-self-join table aliases should trigger AL_007: {issues:?}"
    );
}

#[test]
fn lint_al_007_allows_self_join_aliases_but_flags_extra_unique_alias() {
    let issues = run_lint_with_config(
        "SELECT * FROM users u1 JOIN users u2 ON u1.id = u2.id JOIN orders o ON o.user_id = u1.id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.forbid".to_string(),
                serde_json::json!({"force_enable": true}),
            )]),
        },
    );
    let al07_count = issues
        .iter()
        .filter(|(code, _)| code == "LINT_AL_007")
        .count();
    assert_eq!(
        al07_count, 1,
        "self-join aliases should be allowed while unrelated unique aliases are still flagged: {issues:?}"
    );
}

#[test]
fn lint_al_005_flags_unused_alias_in_single_table_query() {
    let issues = run_lint("SELECT * FROM users u");
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "single-table unused aliases should trigger AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_allows_used_alias_in_single_table_query() {
    let issues = run_lint("SELECT u.id FROM users u");
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "single-table aliases referenced in expressions should not trigger AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_dialect_mode_generic_allows_quoted_alias_fold_match() {
    let issues = run_lint("SELECT a.col1 FROM tab1 AS \"A\"");
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "generic dialect should treat naked/quoted casefold matches as alias usage in AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_dialect_mode_snowflake_flags_quoted_case_mismatch() {
    let issues = run_lint_in_dialect("SELECT a.col_1 FROM table_a AS \"a\"", Dialect::Snowflake);
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "Snowflake naked/quoted case mismatch should be treated as unused alias in AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_dialect_mode_redshift_allows_lower_fold_for_quoted_alias() {
    let issues = run_lint_in_dialect("SELECT A.col_1 FROM table_a AS \"a\"", Dialect::Redshift);
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "Redshift should fold naked identifiers to lower-case for quoted/naked alias matching in AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_dialect_mode_redshift_flags_mixed_quoted_case_mismatch() {
    let issues = run_lint_in_dialect("SELECT a.col_1 FROM table_a AS \"A\"", Dialect::Redshift);
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "Redshift quoted/naked mismatched-case aliases should still trigger AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_dialect_mode_mysql_allows_backtick_reference_with_unquoted_alias() {
    let issues = run_lint_in_dialect(
        "SELECT `nih`.`userID` FROM `flight_notification_item_history` AS nih",
        Dialect::Mysql,
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "MySQL backtick-qualified references should count as usage for unquoted aliases in AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_dialect_mode_duckdb_allows_case_insensitive_quoted_reference() {
    let issues = run_lint_in_dialect("SELECT \"a\".col_1 FROM table_a AS A", Dialect::Duckdb);
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "DuckDB quoted identifier references should match unquoted aliases case-insensitively in AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_dialect_mode_hive_allows_case_insensitive_quoted_reference() {
    let issues = run_lint_in_dialect("SELECT `a`.col1 FROM tab1 AS A", Dialect::Hive);
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "Hive quoted identifier references should match unquoted aliases case-insensitively in AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_bigquery_escaped_quoted_identifiers_do_not_parse_error() {
    let result = analyze(&AnalyzeRequest {
        sql: "SELECT `\\`a`.col1\nFROM tab1 as `\\`A`".to_string(),
        files: None,
        dialect: Dialect::Bigquery,
        source_name: None,
        options: Some(AnalysisOptions {
            lint: Some(LintConfig::default()),
            ..Default::default()
        }),
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    });

    assert!(
        !result
            .issues
            .iter()
            .any(|issue| issue.code == issue_codes::PARSE_ERROR),
        "escaped BigQuery quoted identifiers should parse in fallback mode: {:?}",
        result.issues
    );
    assert!(
        !result
            .issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_AL_005),
        "escaped BigQuery quoted identifiers should not trigger AL_005 when alias is referenced: {:?}",
        result.issues
    );
}

#[test]
fn lint_al_005_clickhouse_escaped_quoted_identifiers_do_not_parse_error() {
    let result = analyze(&AnalyzeRequest {
        sql: "SELECT \"\\\"`a`\"\"\".col1,\nFROM tab1 as `\"\\`a``\"`".to_string(),
        files: None,
        dialect: Dialect::Clickhouse,
        source_name: None,
        options: Some(AnalysisOptions {
            lint: Some(LintConfig::default()),
            ..Default::default()
        }),
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    });

    assert!(
        !result
            .issues
            .iter()
            .any(|issue| issue.code == issue_codes::PARSE_ERROR),
        "escaped ClickHouse quoted identifiers should parse in fallback mode: {:?}",
        result.issues
    );
    assert!(
        !result
            .issues
            .iter()
            .any(|issue| issue.code == issue_codes::LINT_AL_005),
        "escaped ClickHouse quoted identifiers should not trigger AL_005 when alias is referenced: {:?}",
        result.issues
    );
}

#[test]
fn lint_al_005_alias_used_in_qualify_clause() {
    let issues = run_lint_in_dialect(
        "SELECT u.id FROM users u JOIN orders o ON users.id = orders.user_id QUALIFY ROW_NUMBER() OVER (PARTITION BY o.user_id ORDER BY o.user_id) = 1",
        Dialect::Snowflake,
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "alias usage in QUALIFY should satisfy AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_alias_used_in_named_window_clause() {
    let issues = run_lint(
        "SELECT SUM(u.id) OVER w FROM users u JOIN orders o ON users.id = orders.user_id WINDOW w AS (PARTITION BY o.user_id)",
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "alias usage in WINDOW clause should satisfy AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_alias_used_only_in_lateral_subquery_relation() {
    let issues = run_lint(
        "SELECT 1 \
         FROM users u \
         JOIN LATERAL (SELECT u.id) lx ON TRUE",
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "alias usage inside LATERAL subquery relation should satisfy AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_alias_used_only_in_unnest_join_relation() {
    let issues = run_lint(
        "SELECT 1 \
         FROM users u \
         LEFT JOIN UNNEST(u.tags) tag ON TRUE",
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "alias usage inside UNNEST join relation should satisfy AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_allows_unreferenced_subquery_alias() {
    let issues = run_lint("SELECT * FROM (SELECT 1 AS a) subquery");
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "derived-subquery aliases should not be treated as unused table aliases in AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_flags_inner_subquery_unused_alias() {
    let issues = run_lint("SELECT * FROM (SELECT * FROM my_tbl AS foo)");
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "inner subquery table aliases should still be checked by AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_allows_postgres_generate_series_alias() {
    let issues = run_lint_in_dialect(
        "SELECT date_trunc('day', dd)::timestamp \
         FROM generate_series('2022-02-01'::timestamp, NOW()::timestamp, '1 day'::interval) dd",
        Dialect::Postgres,
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "Postgres value-table-function aliases should be ignored for AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_flags_unused_snowflake_lateral_flatten_alias() {
    let issues = run_lint_in_dialect(
        "SELECT a.test1, a.test2, b.test3 \
         FROM table1 AS a, \
         LATERAL flatten(input => some_field) AS b, \
         LATERAL flatten(input => b.value) AS c, \
         LATERAL flatten(input => c.value) AS d, \
         LATERAL flatten(input => d.value) AS e, \
         LATERAL flatten(input => e.value) AS f",
        Dialect::Snowflake,
    );
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "unused aliases in chained Snowflake LATERAL FLATTEN factors should be flagged by AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_flags_unused_alias_inside_snowflake_delete_using_cte() {
    let issues = run_lint_in_dialect(
        "DELETE FROM MYTABLE1 \
         USING ( \
             WITH MYCTE AS (SELECT COLUMN2 FROM MYTABLE3 AS MT3) \
             SELECT COLUMN3 FROM MYTABLE3 \
         ) X \
         WHERE COLUMN1 = X.COLUMN3",
        Dialect::Snowflake,
    );
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "unused aliases inside Snowflake DELETE USING CTE scopes should be flagged by AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_allows_bigquery_to_json_string_table_alias_argument() {
    let issues = run_lint_in_dialect(
        "SELECT TO_JSON_STRING(t) FROM my_table AS t",
        Dialect::Bigquery,
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "BigQuery TO_JSON_STRING(table_alias) should count as alias usage for AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_flags_ansi_to_json_string_table_alias_argument() {
    let issues = run_lint_in_dialect("SELECT TO_JSON_STRING(t) FROM my_table AS t", Dialect::Ansi);
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "ANSI TO_JSON_STRING(table_alias) should still be treated as unused alias in AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_redshift_qualify_after_from_counts_alias_usage() {
    let issues = run_lint_in_dialect(
        "SELECT * \
         FROM store AS s \
         INNER JOIN store_sales AS ss \
         QUALIFY ROW_NUMBER() OVER (PARTITION BY ss.sold_date ORDER BY ss.sales_price DESC) <= 2",
        Dialect::Redshift,
    );
    let al05_count = issues
        .iter()
        .filter(|(code, _)| code == issue_codes::LINT_AL_005)
        .count();
    assert_eq!(
        al05_count, 1,
        "Redshift QUALIFY after FROM should only preserve aliases referenced from QUALIFY: {issues:?}"
    );
}

#[test]
fn lint_al_005_redshift_qualify_after_where_does_not_count_alias_usage() {
    let issues = run_lint_in_dialect(
        "SELECT * \
         FROM store AS s \
         INNER JOIN store_sales AS ss \
         WHERE col = 1 \
         QUALIFY ROW_NUMBER() OVER (PARTITION BY ss.sold_date ORDER BY ss.sales_price DESC) <= 2",
        Dialect::Redshift,
    );
    let al05_count = issues
        .iter()
        .filter(|(code, _)| code == issue_codes::LINT_AL_005)
        .count();
    assert_eq!(
        al05_count, 2,
        "Redshift QUALIFY after WHERE should not preserve alias references for AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_redshift_qualify_unqualified_alias_prefixed_identifier_counts_usage() {
    let issues = run_lint_in_dialect(
        "SELECT * \
         FROM #store_sales AS ss \
         QUALIFY ROW_NUMBER() OVER (PARTITION BY ss_sold_date ORDER BY ss_sales_price DESC) <= 2",
        Dialect::Redshift,
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "Redshift QUALIFY unqualified alias-prefixed identifiers should count as usage in AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_allows_bigquery_implicit_array_table_reference() {
    let issues = run_lint_in_dialect(
        "WITH table_arr AS (SELECT [1,2,4,2] AS arr) \
         SELECT arr \
         FROM table_arr AS t, t.arr",
        Dialect::Bigquery,
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "BigQuery implicit array table references should count as alias usage in AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_allows_redshift_super_array_relation_reference() {
    let issues = run_lint_in_dialect(
        "SELECT my_column, my_array_value \
         FROM my_schema.my_table AS t, t.super_array AS my_array_value",
        Dialect::Redshift,
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "Redshift super-array relation references should count as alias usage in AL_005: {issues:?}"
    );
}

#[test]
fn lint_al_005_allows_repeat_referenced_table_aliases() {
    let issues = run_lint(
        "SELECT ROW_NUMBER() OVER(PARTITION BY a.object_id ORDER BY a.object_id) \
         FROM sys.objects a \
         CROSS JOIN sys.objects b \
         CROSS JOIN sys.objects c",
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_005),
        "repeat-referenced table aliases should not trigger AL_005: {issues:?}"
    );
}

#[test]
fn lint_rule_config_ambiguous_join_outer_mode() {
    let issues = run_lint_with_config(
        "SELECT * FROM foo LEFT JOIN bar ON foo.id = bar.id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "ambiguous.join".to_string(),
                serde_json::json!({"fully_qualify_join_types": "outer"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AM_005"),
        "outer join qualification mode should flag LEFT JOIN without OUTER: {issues:?}"
    );
}

#[test]
fn lint_rule_config_ambiguous_join_outer_mode_right_join() {
    let issues = run_lint_with_config(
        "SELECT * FROM foo RIGHT JOIN bar ON foo.id = bar.id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "ambiguous.join".to_string(),
                serde_json::json!({"fully_qualify_join_types": "outer"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AM_005"),
        "outer join qualification mode should flag RIGHT JOIN without OUTER: {issues:?}"
    );
}

#[test]
fn lint_rule_config_ambiguous_column_refs_explicit_mode() {
    let issues = run_lint_with_config(
        "SELECT foo, bar FROM fake_table GROUP BY 1, 2",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_AM_006".to_string(),
                serde_json::json!({"group_by_and_order_by_style": "explicit"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AM_006"),
        "explicit mode should flag implicit GROUP BY positions: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_unused_case_sensitive() {
    let issues = run_lint_with_config(
        "SELECT zoo.id, b.id FROM users AS \"Zoo\" JOIN books b ON zoo.id = b.user_id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unused".to_string(),
                serde_json::json!({"alias_case_check": "case_sensitive"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_005"),
        "alias_case_check=case_sensitive should flag case-mismatched alias refs: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_unused_quoted_cs_naked_upper() {
    let issues = run_lint_with_config(
        "SELECT foo.id, b.id FROM users AS \"FOO\" JOIN books b ON foo.id = b.user_id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unused".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_upper"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AL_005"),
        "alias_case_check=quoted_cs_naked_upper should upper-fold naked refs for quoted aliases: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_unused_quoted_cs_naked_lower() {
    let issues = run_lint_with_config(
        "SELECT FOO.id, b.id FROM users AS \"foo\" JOIN books b ON FOO.id = b.user_id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unused".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_lower"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AL_005"),
        "alias_case_check=quoted_cs_naked_lower should lower-fold naked refs for quoted aliases: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_self_alias_case_sensitive() {
    let issues = run_lint_with_config(
        "SELECT a AS A FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.self_alias.column".to_string(),
                serde_json::json!({"alias_case_check": "case_sensitive"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AL_009"),
        "alias_case_check=case_sensitive should not flag case-mismatched self aliasing: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_self_alias_quoted_cs_naked_upper() {
    let issues = run_lint_with_config(
        "SELECT \"FOO\" AS foo FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.self_alias.column".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_upper"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_009"),
        "quoted_cs_naked_upper should flag quoted-vs-unquoted self aliases matching after upper folding: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_self_alias_quoted_cs_naked_lower() {
    let issues = run_lint_with_config(
        "SELECT \"foo\" AS FOO FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.self_alias.column".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_lower"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_009"),
        "quoted_cs_naked_lower should flag quoted-vs-unquoted self aliases matching after lower folding: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_unique_column_case_sensitive() {
    let issues = run_lint_with_config(
        "SELECT a, A FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unique.column".to_string(),
                serde_json::json!({"alias_case_check": "case_sensitive"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AL_008"),
        "alias_case_check=case_sensitive should allow case-mismatched projection aliases: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_unique_column_quoted_cs_naked_upper() {
    let issues = run_lint_with_config(
        "SELECT \"FOO\", foo FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unique.column".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_upper"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_008"),
        "quoted_cs_naked_upper should flag quoted-vs-unquoted projection aliases matching after upper folding: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_unique_column_quoted_cs_naked_lower() {
    let issues = run_lint_with_config(
        "SELECT \"foo\", FOO FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unique.column".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_lower"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_008"),
        "quoted_cs_naked_lower should flag quoted-vs-unquoted projection aliases matching after lower folding: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_unique_table_case_sensitive() {
    let issues = run_lint_with_config(
        "SELECT * FROM users a JOIN orders A ON a.id = A.user_id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unique.table".to_string(),
                serde_json::json!({"alias_case_check": "case_sensitive"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AL_004"),
        "alias_case_check=case_sensitive should allow case-mismatched table aliases: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_unique_table_quoted_cs_naked_upper() {
    let issues = run_lint_with_config(
        "SELECT * FROM users AS \"FOO\" JOIN orders foo ON \"FOO\".id = foo.user_id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unique.table".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_upper"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_004"),
        "quoted_cs_naked_upper should flag quoted-vs-unquoted aliases that match after upper folding: {issues:?}"
    );
}

#[test]
fn lint_rule_config_aliasing_unique_table_quoted_cs_naked_lower() {
    let issues = run_lint_with_config(
        "SELECT * FROM users AS \"foo\" JOIN orders FOO ON \"foo\".id = FOO.user_id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.unique.table".to_string(),
                serde_json::json!({"alias_case_check": "quoted_cs_naked_lower"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_004"),
        "quoted_cs_naked_lower should flag quoted-vs-unquoted aliases that match after lower folding: {issues:?}"
    );
}

#[test]
fn lint_rule_config_capitalisation_keywords_upper_policy() {
    let issues = run_lint_with_config(
        "select a from t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.keywords".to_string(),
                serde_json::json!({"capitalisation_policy": "upper"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CP_001"),
        "keyword upper policy should flag lowercase keywords: {issues:?}"
    );
}

#[test]
fn lint_rule_config_capitalisation_keywords_ignore_words() {
    let issues = run_lint_with_config(
        "SELECT a from t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CP_001".to_string(),
                serde_json::json!({"ignore_words": ["FROM"]}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_CP_001"),
        "ignore_words should suppress configured keywords: {issues:?}"
    );
}

#[test]
fn lint_rule_config_capitalisation_keywords_ignore_words_regex() {
    let issues = run_lint_with_config(
        "SELECT a from t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.keywords".to_string(),
                serde_json::json!({"ignore_words_regex": "^from$"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_CP_001"),
        "ignore_words_regex should suppress configured keyword patterns: {issues:?}"
    );
}

#[test]
#[cfg(feature = "templating")]
fn lint_rule_config_capitalisation_keywords_ignore_templated_areas_true() {
    let sql = "{{ \"select\" }} a\nFROM foo\nWHERE 1";
    let issues = run_lint_with_config_in_dialect_with_jinja_template(
        sql,
        Dialect::Ansi,
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([
                (
                    "capitalisation.keywords".to_string(),
                    serde_json::json!({"capitalisation_policy": "upper"}),
                ),
                ("core".to_string(), serde_json::json!({"ignore_templated_areas": true})),
            ]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_CP_001"),
        "ignore_templated_areas=true should ignore templated keyword tokens: {issues:?}"
    );
}

#[test]
#[cfg(feature = "templating")]
fn lint_rule_config_capitalisation_keywords_ignore_templated_areas_false() {
    let sql = "{{ \"select\" }} a\nFROM foo\nWHERE 1";
    let issues = run_lint_with_config_in_dialect_with_jinja_template(
        sql,
        Dialect::Ansi,
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([
                (
                    "capitalisation.keywords".to_string(),
                    serde_json::json!({"capitalisation_policy": "upper"}),
                ),
                (
                    "core".to_string(),
                    serde_json::json!({"ignore_templated_areas": false}),
                ),
            ]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CP_001"),
        "ignore_templated_areas=false should include templated keyword tokens: {issues:?}"
    );
}

#[test]
fn lint_rule_config_capitalisation_identifiers_upper_policy() {
    let issues = run_lint_with_config(
        "SELECT col FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.identifiers".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "upper"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CP_002"),
        "identifier upper policy should flag lowercase identifiers: {issues:?}"
    );
}

#[test]
fn lint_rule_config_capitalisation_identifiers_ignore_words_regex() {
    let issues = run_lint_with_config(
        "SELECT Col, col FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.identifiers".to_string(),
                serde_json::json!({"ignore_words_regex": "^col$"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_CP_002"),
        "identifier ignore_words_regex should suppress matching identifiers: {issues:?}"
    );
}

#[test]
fn lint_rule_config_capitalisation_identifiers_aliases_policy() {
    let issues = run_lint_with_config(
        "SELECT Col AS alias FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.identifiers".to_string(),
                serde_json::json!({"unquoted_identifiers_policy": "aliases"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_CP_002"),
        "identifier aliases policy should ignore non-alias identifier case mix: {issues:?}"
    );
}

#[test]
fn lint_rule_config_capitalisation_functions_lower_policy() {
    let issues = run_lint_with_config(
        "SELECT COUNT(x) FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.functions".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "lower"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CP_003"),
        "function lower policy should flag uppercase function names: {issues:?}"
    );
}

#[test]
fn lint_rule_config_capitalisation_functions_ignore_words_regex() {
    let issues = run_lint_with_config(
        "SELECT COUNT(*), count(x) FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.functions".to_string(),
                serde_json::json!({"ignore_words_regex": "^count$"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_CP_003"),
        "function ignore_words_regex should suppress matching function names: {issues:?}"
    );
}

#[test]
fn lint_rule_config_capitalisation_literals_upper_policy() {
    let issues = run_lint_with_config(
        "SELECT true FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "LINT_CP_004".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "upper"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CP_004"),
        "literal upper policy should flag lowercase literals: {issues:?}"
    );
}

#[test]
fn lint_rule_config_capitalisation_literals_ignore_words_regex() {
    let issues = run_lint_with_config(
        "SELECT NULL, true FROM t",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.literals".to_string(),
                serde_json::json!({"ignore_words_regex": "^true$"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_CP_004"),
        "literal ignore_words_regex should suppress matching literals: {issues:?}"
    );
}

#[test]
fn lint_rule_config_capitalisation_types_upper_policy() {
    let issues = run_lint_with_config(
        "CREATE TABLE t (a int)",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.types".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "upper"}),
            )]),
        },
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_CP_005"),
        "type upper policy should flag lowercase type names: {issues:?}"
    );
}

#[test]
fn lint_rule_config_capitalisation_types_ignore_words_regex() {
    let issues = run_lint_with_config(
        "CREATE TABLE t (a INT, b varchar(10))",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.types".to_string(),
                serde_json::json!({"ignore_words_regex": "^varchar$"}),
            )]),
        },
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_CP_005"),
        "type ignore_words_regex should suppress matching type names: {issues:?}"
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
fn lint_am_007_nested_join_alias_wildcard_set_mismatch() {
    let issues = run_lint(
        "SELECT j.* FROM ((SELECT a FROM t1) AS a1 JOIN (SELECT b FROM t2) AS b1 ON a1.a = b1.b) AS j UNION ALL SELECT x FROM t3",
    );
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_007"));
}

#[test]
fn lint_am_007_declared_cte_columns_resolve_set_width() {
    let issues =
        run_lint("WITH cte(a, b) AS (SELECT * FROM t) SELECT * FROM cte UNION SELECT c, d FROM t2");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AM_007"),
        "declared CTE column list should resolve set-branch width for AM_007: {issues:?}"
    );
}

#[test]
fn lint_am_007_declared_derived_alias_columns_set_mismatch() {
    let issues = run_lint(
        "SELECT t_alias.* FROM (SELECT * FROM t) AS t_alias(a, b, c) UNION SELECT d, e FROM t2",
    );
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_007"));
}

#[test]
fn lint_am_007_nested_join_using_width_resolves_for_set_comparison() {
    let issues = run_lint(
        "SELECT j.* FROM ((SELECT a FROM t1) AS a1 JOIN (SELECT a FROM t2) AS b1 USING(a)) AS j UNION ALL SELECT x FROM t3",
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AM_007"),
        "resolved USING-join width should avoid AM_007 mismatch: {issues:?}"
    );
}

#[test]
fn lint_am_007_nested_join_natural_width_resolves_for_set_comparison() {
    let issues = run_lint(
        "SELECT j.* FROM ((SELECT a FROM t1) AS a1 NATURAL JOIN (SELECT a FROM t2) AS b1) AS j UNION ALL SELECT x FROM t3",
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AM_007"),
        "resolved NATURAL-join width should avoid AM_007 mismatch: {issues:?}"
    );
}

#[test]
fn lint_am_007_nested_join_natural_width_unknown_does_not_trigger_for_set_comparison() {
    let issues = run_lint(
        "SELECT j.* FROM ((SELECT * FROM t1) AS a1 NATURAL JOIN (SELECT a FROM t2) AS b1) AS j UNION ALL SELECT x FROM t3",
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AM_007"),
        "unknown NATURAL-join width should defer AM_007 mismatch checks: {issues:?}"
    );
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
fn lint_cv_012_inner_join_without_on_with_where_predicate() {
    let issues = run_lint("SELECT a.x, b.y FROM a INNER JOIN b WHERE a.id = b.id");
    assert!(issues.iter().any(|(code, _)| code == "LINT_CV_012"));
}

#[test]
fn lint_cv_012_multi_join_not_all_references_is_not_flagged() {
    let issues = run_lint("SELECT a.id FROM a JOIN b JOIN c WHERE a.a = b.a AND b.b > 1");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_CV_012"),
        "CV_012 should not fire when not all naked joins are represented by WHERE join predicates: {issues:?}"
    );
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
                "LINT_AL_007".to_string(),
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
fn lint_am_004_nested_join_alias_known_columns_ok() {
    let issues = run_lint(
        "SELECT j.* FROM ((SELECT a FROM t1) AS a1 JOIN (SELECT b FROM t2) AS b1 ON a1.a = b1.b) AS j",
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AM_004"),
        "resolved nested-join alias wildcard should not trigger AM_004: {issues:?}"
    );
}

#[test]
fn lint_am_004_declared_cte_columns_known_width_ok() {
    let issues = run_lint("WITH cte(a, b) AS (SELECT * FROM t) SELECT * FROM cte");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AM_004"),
        "declared CTE column list should resolve wildcard width for AM_004: {issues:?}"
    );
}

#[test]
fn lint_am_004_declared_derived_alias_columns_known_width_ok() {
    let issues = run_lint("SELECT t_alias.* FROM (SELECT * FROM t) AS t_alias(a, b)");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AM_004"),
        "declared derived alias columns should resolve wildcard width for AM_004: {issues:?}"
    );
}

#[test]
fn lint_am_004_nested_join_using_width_known_columns_ok() {
    let issues = run_lint(
        "SELECT j.* FROM ((SELECT a FROM t1) AS a1 JOIN (SELECT a FROM t2) AS b1 USING(a)) AS j",
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AM_004"),
        "resolved USING-join width should avoid AM_004 unknown-width warning: {issues:?}"
    );
}

#[test]
fn lint_am_004_nested_join_natural_width_resolved_ok() {
    let issues = run_lint(
        "SELECT j.* FROM ((SELECT a FROM t1) AS a1 NATURAL JOIN (SELECT a FROM t2) AS b1) AS j",
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AM_004"),
        "known NATURAL-join output width should avoid AM_004 warning: {issues:?}"
    );
}

#[test]
fn lint_am_004_nested_join_natural_width_unknown_flags() {
    let issues = run_lint(
        "SELECT j.* FROM ((SELECT * FROM t1) AS a1 NATURAL JOIN (SELECT a FROM t2) AS b1) AS j",
    );
    assert!(issues.iter().any(|(code, _)| code == "LINT_AM_004"));
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
fn lint_al_004_duplicate_implicit_table_name_aliases() {
    let issues = run_lint(
        "SELECT * FROM analytics.foo JOIN reporting.foo ON analytics.foo.id = reporting.foo.id",
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_004"),
        "duplicate implicit table-name aliases should trigger AL_004: {issues:?}"
    );
}

#[test]
fn lint_al_004_duplicate_alias_between_parent_and_subquery_scope() {
    let issues =
        run_lint("SELECT * FROM (SELECT * FROM users a) s JOIN orders a ON s.id = a.user_id");
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_004"),
        "parent/subquery alias collisions should trigger AL_004: {issues:?}"
    );
}

#[test]
fn lint_al_004_duplicate_alias_between_outer_scope_and_where_subquery() {
    let issues = run_lint("SELECT * FROM tbl AS t WHERE t.val IN (SELECT t.val FROM tbl2 AS t)");
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_004"),
        "outer/WHERE-subquery alias collisions should trigger AL_004: {issues:?}"
    );
}

#[test]
fn lint_al_004_duplicate_implicit_table_name_in_where_subquery() {
    let issues = run_lint("SELECT * FROM tbl WHERE val IN (SELECT tbl.val FROM tbl)");
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_004"),
        "implicit table-name collisions across WHERE subqueries should trigger AL_004: {issues:?}"
    );
}

#[test]
fn lint_al_001_bigquery_merge_requires_explicit_aliases() {
    let issues = run_lint_in_dialect(
        "MERGE dataset.inventory t USING dataset.newarrivals s ON t.product = s.product WHEN MATCHED THEN UPDATE SET quantity = t.quantity + s.quantity",
        Dialect::Bigquery,
    );
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_001"),
        "BigQuery MERGE aliases without AS should trigger AL_001: {issues:?}"
    );
}

#[test]
fn lint_al_002_tsql_assignment_alias_syntax_is_allowed() {
    let issues = run_lint_in_dialect("SELECT alias1 = col1", Dialect::Mssql);
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_AL_002"),
        "TSQL assignment-style aliases should not trigger AL_002: {issues:?}"
    );
}

#[test]
fn lint_al_008_duplicate_unaliased_column_reference() {
    let issues = run_lint("SELECT foo, foo FROM t");
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_AL_008"),
        "duplicate unaliased projection refs should trigger AL_008: {issues:?}"
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
fn lint_rf_003_mixed_qualified_wildcard_and_unqualified_ref() {
    let issues = run_lint("SELECT t.*, id FROM t");
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_RF_003"),
        "qualified wildcard mixed with unqualified refs should trigger RF_003: {issues:?}"
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
fn lint_st_010_non_equality_literal_comparison_ok() {
    let issues = run_lint("SELECT * FROM t WHERE 1 < 2");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_010"),
        "non-equality literal comparison should not trigger ST_010: {issues:?}"
    );
}

#[test]
fn lint_st_010_flags_self_comparison_with_inequality_operators() {
    let issues = run_lint("SELECT * FROM t WHERE a < a OR b >= b");
    let st_010_count = issues
        .iter()
        .filter(|(code, _)| code == "LINT_ST_010")
        .count();
    assert_eq!(
        st_010_count, 2,
        "self-comparisons using inequality operators should trigger ST_010 per occurrence: {issues:?}"
    );
}

#[test]
fn lint_st_010_reports_each_constant_expression_occurrence() {
    let issues = run_lint("SELECT * FROM t WHERE a = a AND b = b");
    let st_010_count = issues
        .iter()
        .filter(|(code, _)| code == "LINT_ST_010")
        .count();
    assert_eq!(
        st_010_count, 2,
        "expected two ST_010 violations: {issues:?}"
    );
}

#[test]
fn lint_st_010_flags_equal_string_concat_expression() {
    let issues = run_lint("SELECT * FROM t WHERE 'A' || 'B' = 'A' || 'B'");
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_ST_010"),
        "equivalent string-concat expressions should trigger ST_010: {issues:?}"
    );
}

#[test]
fn lint_st_011_inner_join_not_checked() {
    let issues = run_lint("SELECT a.id FROM a INNER JOIN b ON a.id = b.id");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "inner joins should not trigger ST_011 by default: {issues:?}"
    );
}

#[test]
fn lint_st_011_unqualified_wildcard_counts_as_reference() {
    let issues = run_lint("SELECT * FROM a LEFT JOIN b ON a.id = b.id");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "unqualified wildcard should count as joined-source usage: {issues:?}"
    );
}

#[test]
fn lint_st_011_snowflake_qualified_wildcard_exclude_counts_as_reference() {
    let issues = run_lint_in_dialect(
        "select \
            simulation_source_data_reference.*, \
            sourcings.* exclude sourcing_job_id \
         from simulation_source_data_reference \
         left join sourcings \
             on simulation_source_data_reference.sourcing_job_id = sourcings.sourcing_job_id",
        Dialect::Snowflake,
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "Snowflake qualified wildcard EXCLUDE should count as joined-source usage: {issues:?}"
    );
}

#[test]
fn lint_st_011_query_order_by_reference_counts_as_usage() {
    let issues = run_lint("SELECT a.id FROM a LEFT JOIN b ON a.id = b.id ORDER BY b.id");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "query-level ORDER BY references should count as joined-source usage: {issues:?}"
    );
}

#[test]
fn lint_st_011_named_window_reference_counts_as_usage() {
    let issues =
        run_lint("SELECT SUM(a.value) OVER w FROM a LEFT JOIN b ON a.id = b.id WINDOW w AS (PARTITION BY b.group_key)");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "named WINDOW clause references should count as joined-source usage: {issues:?}"
    );
}

#[test]
fn lint_st_011_named_window_unqualified_reference_defers_check() {
    let issues =
        run_lint("SELECT SUM(a.value) OVER w FROM a LEFT JOIN b ON a.id = b.id WINDOW w AS (PARTITION BY group_key)");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "unqualified named WINDOW references should defer ST_011: {issues:?}"
    );
}

#[test]
fn lint_st_011_distinct_on_reference_counts_as_usage() {
    let issues = run_lint("SELECT DISTINCT ON (b.id) a.id FROM a LEFT JOIN b ON a.id = b.id");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "DISTINCT ON joined-source references should count as usage: {issues:?}"
    );
}

#[test]
fn lint_st_011_distinct_on_unqualified_reference_defers_check() {
    let issues = run_lint("SELECT DISTINCT ON (id) a.id FROM a LEFT JOIN b ON a.id = b.id");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "unqualified DISTINCT ON references should defer ST_011: {issues:?}"
    );
}

#[test]
fn lint_st_011_cluster_by_reference_counts_as_usage() {
    let issues = run_lint("SELECT a.id FROM a LEFT JOIN b ON a.id = b.id CLUSTER BY b.id");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "CLUSTER BY joined-source references should count as usage: {issues:?}"
    );
}

#[test]
fn lint_st_011_distribute_by_reference_counts_as_usage() {
    let issues = run_lint("SELECT a.id FROM a LEFT JOIN b ON a.id = b.id DISTRIBUTE BY b.id");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "DISTRIBUTE BY joined-source references should count as usage: {issues:?}"
    );
}

#[test]
fn lint_st_011_mysql_backtick_quoted_joined_source_reference_counts_as_usage() {
    let issues = run_lint_in_dialect(
        "SELECT `test`.one, `test-2`.two FROM `test` LEFT JOIN `test-2` ON `test`.id = `test-2`.id",
        Dialect::Mysql,
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "backtick-quoted joined-source references should count as usage: {issues:?}"
    );
}

#[test]
fn lint_st_011_mssql_bracket_quoted_joined_source_reference_counts_as_usage() {
    let issues = run_lint_in_dialect(
        "SELECT [test].one, [test-2].two FROM [test] LEFT JOIN [test-2] ON [test].id = [test-2].id",
        Dialect::Mssql,
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "bracket-quoted joined-source references should count as usage: {issues:?}"
    );
}

#[test]
fn lint_st_011_hive_lateral_view_reference_counts_as_usage() {
    let issues = run_lint_in_dialect(
        "SELECT a.id FROM a LEFT JOIN b ON a.id = b.id LATERAL VIEW explode(b.items) lv AS item",
        Dialect::Hive,
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "LATERAL VIEW joined-source references should count as usage: {issues:?}"
    );
}

#[test]
fn lint_st_011_hive_lateral_view_unqualified_reference_defers_check() {
    let issues = run_lint_in_dialect(
        "SELECT a.id FROM a LEFT JOIN b ON a.id = b.id LATERAL VIEW explode(items) lv AS item",
        Dialect::Hive,
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "unqualified LATERAL VIEW references should defer ST_011: {issues:?}"
    );
}

#[test]
fn lint_st_011_connect_by_reference_counts_as_usage() {
    let issues = run_lint_in_dialect(
        "SELECT a.id FROM a LEFT JOIN b ON a.id = b.id START WITH b.id IS NOT NULL CONNECT BY PRIOR a.id = b.id",
        Dialect::Snowflake,
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "CONNECT BY joined-source references should count as usage: {issues:?}"
    );
}

#[test]
fn lint_st_011_connect_by_unqualified_reference_defers_check() {
    let issues = run_lint_in_dialect(
        "SELECT a.id FROM a LEFT JOIN b ON a.id = b.id START WITH id IS NOT NULL CONNECT BY PRIOR a.id = b.id",
        Dialect::Snowflake,
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "unqualified CONNECT BY references should defer ST_011: {issues:?}"
    );
}

#[test]
fn lint_st_011_does_not_flag_base_from_with_using_join() {
    let issues = run_lint("SELECT b.id FROM a LEFT JOIN b USING(id)");
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "base FROM source should not be considered an unused joined source: {issues:?}"
    );
}

#[test]
fn lint_st_011_flags_multi_root_unused_outer_join_source() {
    let issues = run_lint("SELECT a.id FROM a, b LEFT JOIN c ON b.id = c.id");
    assert!(
        issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "unused joined source in multi-root FROM should trigger ST_011: {issues:?}"
    );
}

#[test]
fn lint_st_011_allows_unnest_chain_reference_between_join_relations() {
    let issues = run_lint(
        "SELECT ft.id, n.generic_field FROM fact_table AS ft LEFT JOIN UNNEST(ft.generic_array) AS g LEFT JOIN UNNEST(g.nested_array) AS n",
    );
    assert!(
        !issues.iter().any(|(code, _)| code == "LINT_ST_011"),
        "UNNEST join-chain relation references should count as joined-source usage: {issues:?}"
    );
}

#[test]
fn lint_lt_007_cte_bracket_missing() {
    let issues = run_lint("WITH cte AS (\n  SELECT 1) SELECT * FROM cte");
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_LT_007),
        "expected {}: {issues:?}",
        issue_codes::LINT_LT_007,
    );
}

#[test]
#[cfg(feature = "templating")]
fn lint_lt_007_jinja_whitespace_consumption_expression_on_own_line_passes() {
    let sql = "with cte as (\n    select 1\n    {{- ' from i_consume_whitespace ' -}}\n) select * from cte";
    let issues = run_lint_in_dialect_with_jinja_template(sql, Dialect::Ansi);
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_LT_007),
        "expected no {} for whitespace-consuming template line with own-line close: {issues:?}",
        issue_codes::LINT_LT_007,
    );
}

#[test]
#[cfg(feature = "templating")]
fn lint_lt_007_jinja_whitespace_consumption_comment_on_own_line_passes() {
    let sql = "with cte as (\n    select 1\n    {#- consumed -#}\n) select * from cte";
    let issues = run_lint_in_dialect_with_jinja_template(sql, Dialect::Ansi);
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_LT_007),
        "expected no {} for whitespace-consuming template comment with own-line close: {issues:?}",
        issue_codes::LINT_LT_007,
    );
}

#[test]
#[cfg(feature = "templating")]
fn lint_lt_007_jinja_whitespace_consumption_same_line_close_still_flags() {
    let sql = "with cte as (\n    select 1\n    {%- if False -%}{%- endif -%}) select * from cte";
    let issues = run_lint_in_dialect_with_jinja_template(sql, Dialect::Ansi);
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_LT_007),
        "expected {} for same-line close after whitespace-consuming template block: {issues:?}",
        issue_codes::LINT_LT_007,
    );
}

#[test]
fn lint_lt_012_multiple_trailing_newlines() {
    let issues = run_lint("SELECT 1\nFROM t\n\n");
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_LT_012),
        "expected {} for multiple trailing newlines: {issues:?}",
        issue_codes::LINT_LT_012,
    );
}

#[test]
fn lint_jj_001_missing_padding_before_statement_close_tag() {
    let issues = run_lint("SELECT '{% for x in y%}' AS templated");
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_JJ_001),
        "expected {} for missing close-tag padding: {issues:?}",
        issue_codes::LINT_JJ_001,
    );
}

#[test]
fn lint_rf_004_keyword_identifier() {
    let issues = run_lint("SELECT sum.id FROM users AS sum");
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_RF_004),
        "expected {}: {issues:?}",
        issue_codes::LINT_RF_004,
    );
}

#[test]
fn lint_rf_004_keyword_cte_identifier() {
    let issues = run_lint("WITH sum AS (SELECT 1 AS value) SELECT value FROM sum");
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_RF_004),
        "expected {} for keyword CTE identifiers: {issues:?}",
        issue_codes::LINT_RF_004,
    );
}

#[test]
fn lint_rf_005_special_chars() {
    let issues = run_lint("SELECT \"bad-name\" FROM t");
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_RF_005),
        "expected {} for quoted identifier special chars: {issues:?}",
        issue_codes::LINT_RF_005,
    );
}

#[test]
fn lint_rf_002_flags_projection_self_alias_in_multi_source_query() {
    let issues = run_lint("SELECT foo AS foo FROM a LEFT JOIN b ON a.id = b.id");
    assert!(
        issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_RF_002),
        "expected {} for self-alias projection in multi-source query: {issues:?}",
        issue_codes::LINT_RF_002,
    );
}

#[test]
fn lint_rf_002_allows_later_projection_reference_to_previous_alias() {
    let issues = run_lint("SELECT a.bar AS baz, baz FROM a LEFT JOIN b ON a.id = b.id");
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_RF_002),
        "did not expect {} for valid later alias reference: {issues:?}",
        issue_codes::LINT_RF_002,
    );
}

#[test]
fn lint_rf_002_allows_bigquery_date_part_function_argument() {
    let issues = run_lint_in_dialect(
        "SELECT timestamp_trunc(a.ts, month) AS t FROM a JOIN b ON a.id = b.id",
        Dialect::Bigquery,
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_RF_002),
        "datepart function-argument keywords should not trigger {}: {issues:?}",
        issue_codes::LINT_RF_002,
    );
}

#[test]
fn lint_rf_002_allows_snowflake_datediff_date_part_argument() {
    let issues = run_lint_in_dialect(
        "SELECT datediff(year, a.column1, b.column2) FROM a JOIN b ON a.id = b.id",
        Dialect::Snowflake,
    );
    assert!(
        !issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_RF_002),
        "datediff datepart keywords should not trigger {}: {issues:?}",
        issue_codes::LINT_RF_002,
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
        ("LINT_CV_006", "SELECT 1 ;"),
        ("LINT_CV_007", "(SELECT 1)"),
        ("LINT_CV_009", "SELECT foo FROM t"),
        ("LINT_CV_010", "SELECT 'abc' AS a, \"def\" AS b FROM t"),
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
        ("LINT_ST_005", "SELECT * FROM t JOIN (SELECT * FROM u) sub ON t.id = sub.id"),
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

    let al07_issues = run_lint_with_config(
        "SELECT * FROM users u",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.forbid".to_string(),
                serde_json::json!({"force_enable": true}),
            )]),
        },
    );
    assert!(
        al07_issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_007),
        "expected {} with force_enable=true in smoke case: {al07_issues:?}",
        issue_codes::LINT_AL_007,
    );

    let al06_issues = run_lint_with_config(
        "SELECT * FROM a x JOIN b yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy ON x.id = yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy.id",
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "aliasing.length".to_string(),
                serde_json::json!({"max_alias_length": 30}),
            )]),
        },
    );
    assert!(
        al06_issues
            .iter()
            .any(|(code, _)| code == issue_codes::LINT_AL_006),
        "expected {} with max_alias_length=30 in smoke case: {al06_issues:?}",
        issue_codes::LINT_AL_006,
    );
}
