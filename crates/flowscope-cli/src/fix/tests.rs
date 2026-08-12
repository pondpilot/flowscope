use super::*;
use flowscope_core::{analyze, issue_codes, AnalysisOptions, AnalyzeRequest, Dialect, LintConfig};

fn default_lint_config() -> LintConfig {
    LintConfig {
        enabled: true,
        disabled_rules: vec![],
        rule_configs: std::collections::BTreeMap::new(),
    }
}

fn lint_config_keep_only_rule(rule_code: &str, mut config: LintConfig) -> LintConfig {
    let disabled_rules = flowscope_core::linter::rules::all_rules(&default_lint_config())
        .into_iter()
        .map(|rule| rule.code().to_string())
        .filter(|code| !code.eq_ignore_ascii_case(rule_code))
        .collect();
    config.disabled_rules = disabled_rules;
    config
}

fn lint_rule_count_with_config(sql: &str, code: &str, lint_config: &LintConfig) -> usize {
    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: Some(AnalysisOptions {
            lint: Some(lint_config.clone()),
            ..Default::default()
        }),
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    };

    analyze(&request)
        .issues
        .iter()
        .filter(|issue| issue.code == code)
        .count()
}

fn lint_rule_count_with_config_in_dialect(
    sql: &str,
    code: &str,
    dialect: Dialect,
    lint_config: &LintConfig,
) -> usize {
    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect,
        source_name: None,
        options: Some(AnalysisOptions {
            lint: Some(lint_config.clone()),
            ..Default::default()
        }),
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    };

    analyze(&request)
        .issues
        .iter()
        .filter(|issue| issue.code == code)
        .count()
}

fn lint_rule_count(sql: &str, code: &str) -> usize {
    lint_rule_count_with_config(sql, code, &default_lint_config())
}

fn apply_fix_with_config(sql: &str, lint_config: &LintConfig) -> FixOutcome {
    apply_lint_fixes_with_lint_config(sql, Dialect::Generic, lint_config).expect("fix result")
}

fn apply_core_only_fixes(sql: &str) -> FixOutcome {
    apply_lint_fixes_with_options(
        sql,
        Dialect::Generic,
        &default_lint_config(),
        FixOptions {
            include_unsafe_fixes: true,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix result")
}

fn sample_outcome(skipped_counts: FixSkippedCounts) -> FixOutcome {
    FixOutcome {
        sql: String::new(),
        counts: FixCounts::default(),
        changed: false,
        skipped_due_to_comments: false,
        skipped_due_to_regression: false,
        skipped_counts,
    }
}

#[test]
fn collect_fix_candidate_stats_always_counts_display_only_as_blocked() {
    let outcome = sample_outcome(FixSkippedCounts {
        unsafe_skipped: 1,
        protected_range_blocked: 2,
        overlap_conflict_blocked: 3,
        display_only: 4,
    });

    let stats = collect_fix_candidate_stats(
        &outcome,
        LintFixRuntimeOptions {
            include_unsafe_fixes: false,
            legacy_ast_fixes: false,
        },
    );

    assert_eq!(stats.skipped, 0);
    assert_eq!(stats.blocked, 10);
    assert_eq!(stats.blocked_unsafe, 1);
    assert_eq!(stats.blocked_display_only, 4);
    assert_eq!(stats.blocked_protected_range, 2);
    assert_eq!(stats.blocked_overlap_conflict, 3);
}

#[test]
fn collect_fix_candidate_stats_excludes_unsafe_when_unsafe_fixes_enabled() {
    let outcome = sample_outcome(FixSkippedCounts {
        unsafe_skipped: 2,
        protected_range_blocked: 1,
        overlap_conflict_blocked: 1,
        display_only: 3,
    });

    let stats = collect_fix_candidate_stats(
        &outcome,
        LintFixRuntimeOptions {
            include_unsafe_fixes: true,
            legacy_ast_fixes: false,
        },
    );

    assert_eq!(stats.blocked, 5);
    assert_eq!(stats.blocked_unsafe, 0);
    assert_eq!(stats.blocked_display_only, 3);
}

#[test]
fn mostly_unfixable_residual_detects_dominated_known_residuals() {
    let counts = std::collections::BTreeMap::from([
        (issue_codes::LINT_LT_005.to_string(), 140usize),
        (issue_codes::LINT_RF_002.to_string(), 116usize),
        (issue_codes::LINT_AL_003.to_string(), 43usize),
        (issue_codes::LINT_RF_004.to_string(), 2usize),
        (issue_codes::LINT_ST_009.to_string(), 1usize),
    ]);
    assert!(is_mostly_unfixable_residual(&counts));
}

#[test]
fn mostly_unfixable_residual_rejects_when_fixable_tail_is_material() {
    let counts = std::collections::BTreeMap::from([
        (issue_codes::LINT_LT_005.to_string(), 20usize),
        (issue_codes::LINT_RF_002.to_string(), 10usize),
        (issue_codes::LINT_ST_009.to_string(), 8usize),
        (issue_codes::LINT_LT_003.to_string(), 3usize),
    ]);
    assert!(!is_mostly_unfixable_residual(&counts));
}

#[test]
fn am005_outer_mode_full_join_fix_output() {
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: vec![issue_codes::LINT_CV_008.to_string()],
        rule_configs: std::collections::BTreeMap::from([(
            "ambiguous.join".to_string(),
            serde_json::json!({"fully_qualify_join_types": "outer"}),
        )]),
    };
    let sql = "SELECT a FROM t FULL JOIN u ON t.id = u.id";
    assert_eq!(
        lint_rule_count_with_config(
            "SELECT a FROM t FULL OUTER JOIN u ON t.id = u.id",
            issue_codes::LINT_AM_005,
            &lint_config,
        ),
        0
    );
    let out = apply_fix_with_config(sql, &lint_config);
    assert!(
        out.sql.to_ascii_uppercase().contains("FULL OUTER JOIN"),
        "expected FULL OUTER JOIN in fixed SQL, got: {}",
        out.sql
    );
    assert_eq!(fix_count_for_code(&out.counts, issue_codes::LINT_AM_005), 1);
}

fn fix_count_for_code(counts: &FixCounts, code: &str) -> usize {
    counts.get(code)
}

#[test]
fn lint_rule_counts_includes_parse_errors() {
    let counts = lint_rule_counts("SELECT (", Dialect::Generic, &default_lint_config());
    assert!(
        counts.get(issue_codes::PARSE_ERROR).copied().unwrap_or(0) > 0,
        "invalid SQL should contribute PARSE_ERROR to regression counts"
    );
}

#[test]
fn parse_error_regression_is_detected_even_with_lint_improvements() {
    let before = std::collections::BTreeMap::from([(issue_codes::LINT_ST_005.to_string(), 1)]);
    let after = std::collections::BTreeMap::from([(issue_codes::PARSE_ERROR.to_string(), 1)]);
    let removed = FixCounts::from_removed(&before, &after);

    assert_eq!(
        removed.total(),
        1,
        "lint-only comparison can still look improved"
    );
    assert!(
        parse_errors_increased(&before, &after),
        "introduced parse errors must force regression"
    );
}

#[test]
fn lint_improvements_can_mask_total_violation_regressions() {
    let before = std::collections::BTreeMap::from([
        (issue_codes::LINT_LT_002.to_string(), 2usize),
        (issue_codes::LINT_LT_001.to_string(), 0usize),
    ]);
    let after = std::collections::BTreeMap::from([
        (issue_codes::LINT_LT_002.to_string(), 1usize),
        (issue_codes::LINT_LT_001.to_string(), 2usize),
    ]);
    let removed = FixCounts::from_removed(&before, &after);
    let before_total: usize = before.values().sum();
    let after_total: usize = after.values().sum();

    assert_eq!(
        removed.total(),
        1,
        "a rule-level improvement can still be observed"
    );
    assert!(
        after_total > before_total,
        "strict regression guard must reject net-violation increases"
    );
}

#[test]
fn lt03_improvement_allows_lt05_tradeoff_at_equal_totals() {
    let before = std::collections::BTreeMap::from([
        (issue_codes::LINT_LT_003.to_string(), 1usize),
        (issue_codes::LINT_LT_005.to_string(), 5usize),
    ]);
    let after = std::collections::BTreeMap::from([
        (issue_codes::LINT_LT_003.to_string(), 0usize),
        (issue_codes::LINT_LT_005.to_string(), 6usize),
    ]);
    let core_rules = std::collections::HashSet::from([
        issue_codes::LINT_LT_003.to_string(),
        issue_codes::LINT_LT_005.to_string(),
    ]);

    assert!(
        !core_autofix_rules_not_improved(&before, &after, &core_rules),
        "LT03 improvements should be allowed to trade against LT05 at equal totals"
    );
}

#[test]
fn lt05_tradeoff_is_not_allowed_without_lt03_improvement() {
    let before = std::collections::BTreeMap::from([
        (issue_codes::LINT_LT_003.to_string(), 1usize),
        (issue_codes::LINT_LT_005.to_string(), 5usize),
    ]);
    let after = std::collections::BTreeMap::from([
        (issue_codes::LINT_LT_003.to_string(), 1usize),
        (issue_codes::LINT_LT_005.to_string(), 6usize),
    ]);
    let core_rules = std::collections::HashSet::from([
        issue_codes::LINT_LT_003.to_string(),
        issue_codes::LINT_LT_005.to_string(),
    ]);

    assert!(
        core_autofix_rules_not_improved(&before, &after, &core_rules),
        "without LT03 improvement, LT05 worsening remains blocked"
    );
}

fn assert_rule_case(
    sql: &str,
    code: &str,
    expected_before: usize,
    expected_after: usize,
    expected_fix_count: usize,
) {
    let before = lint_rule_count(sql, code);
    assert_eq!(
        before, expected_before,
        "unexpected initial lint count for {code} in SQL: {sql}"
    );

    let out = apply_core_only_fixes(sql);
    assert!(
        !out.skipped_due_to_comments,
        "test SQL should not be skipped"
    );
    assert_eq!(
        fix_count_for_code(&out.counts, code),
        expected_fix_count,
        "unexpected fix count for {code} in SQL: {sql}"
    );

    if expected_fix_count > 0 {
        assert!(out.changed, "expected SQL to change for {code}: {sql}");
    }

    let after = lint_rule_count(&out.sql, code);
    assert_eq!(
        after, expected_after,
        "unexpected lint count after fix for {code}. SQL: {}",
        out.sql
    );

    let second_pass = apply_core_only_fixes(&out.sql);
    assert_eq!(
        fix_count_for_code(&second_pass.counts, code),
        0,
        "expected idempotent second pass for {code}"
    );
}

fn assert_rule_case_with_config(
    sql: &str,
    code: &str,
    expected_before: usize,
    expected_after: usize,
    expected_fix_count: usize,
    lint_config: &LintConfig,
) {
    let before = lint_rule_count_with_config(sql, code, lint_config);
    assert_eq!(
        before, expected_before,
        "unexpected initial lint count for {code} in SQL: {sql}"
    );

    let out = apply_fix_with_config(sql, lint_config);
    assert!(
        !out.skipped_due_to_comments,
        "test SQL should not be skipped"
    );
    assert_eq!(
        fix_count_for_code(&out.counts, code),
        expected_fix_count,
        "unexpected fix count for {code} in SQL: {sql}"
    );

    if expected_fix_count > 0 {
        assert!(out.changed, "expected SQL to change for {code}: {sql}");
    }

    let after = lint_rule_count_with_config(&out.sql, code, lint_config);
    assert_eq!(
        after, expected_after,
        "unexpected lint count after fix for {code}. SQL: {}",
        out.sql
    );

    let second_pass = apply_fix_with_config(&out.sql, lint_config);
    assert_eq!(
        fix_count_for_code(&second_pass.counts, code),
        0,
        "expected idempotent second pass for {code}"
    );
}

#[test]
fn sqlfluff_am003_cases_are_fixed() {
    let cases = [
        ("SELECT DISTINCT col FROM t GROUP BY col", 1, 0, 1),
        (
            "SELECT * FROM (SELECT DISTINCT a FROM t GROUP BY a) AS sub",
            1,
            0,
            1,
        ),
        (
            "WITH cte AS (SELECT DISTINCT a FROM t GROUP BY a) SELECT * FROM cte",
            1,
            0,
            1,
        ),
        (
            "CREATE VIEW v AS SELECT DISTINCT a FROM t GROUP BY a",
            1,
            0,
            1,
        ),
        (
            "INSERT INTO target SELECT DISTINCT a FROM t GROUP BY a",
            1,
            0,
            1,
        ),
        (
            "SELECT a FROM t UNION ALL SELECT DISTINCT b FROM t2 GROUP BY b",
            1,
            0,
            1,
        ),
        ("SELECT a, b FROM t", 0, 0, 0),
    ];

    for (sql, before, after, fix_count) in cases {
        assert_rule_case(sql, issue_codes::LINT_AM_001, before, after, fix_count);
    }
}

#[test]
fn sqlfluff_am001_cases_are_fixed_or_unchanged() {
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: vec![issue_codes::LINT_LT_011.to_string()],
        rule_configs: std::collections::BTreeMap::new(),
    };
    let cases = [
        (
            "SELECT a, b FROM tbl UNION SELECT c, d FROM tbl1",
            1,
            0,
            1,
            Some("DISTINCT SELECT"),
        ),
        (
            "SELECT a, b FROM tbl UNION ALL SELECT c, d FROM tbl1",
            0,
            0,
            0,
            None,
        ),
        (
            "SELECT a, b FROM tbl UNION DISTINCT SELECT c, d FROM tbl1",
            0,
            0,
            0,
            None,
        ),
        (
            "select a, b from tbl union select c, d from tbl1",
            1,
            0,
            1,
            Some("DISTINCT SELECT"),
        ),
    ];

    for (sql, before, after, fix_count, expected_text) in cases {
        assert_rule_case_with_config(
            sql,
            issue_codes::LINT_AM_002,
            before,
            after,
            fix_count,
            &lint_config,
        );

        if let Some(expected) = expected_text {
            let out = apply_fix_with_config(sql, &lint_config);
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }
}

#[test]
fn sqlfluff_am005_cases_are_fixed_or_unchanged() {
    let cases = [
        (
            "SELECT * FROM t ORDER BY a, b DESC",
            1,
            0,
            1,
            Some("ORDER BY A ASC, B DESC"),
        ),
        (
            "SELECT * FROM t ORDER BY a DESC, b",
            1,
            0,
            1,
            Some("ORDER BY A DESC, B ASC"),
        ),
        (
            "SELECT * FROM t ORDER BY a DESC, b NULLS LAST",
            1,
            0,
            1,
            Some("ORDER BY A DESC, B ASC NULLS LAST"),
        ),
        ("SELECT * FROM t ORDER BY a, b", 0, 0, 0, None),
        ("SELECT * FROM t ORDER BY a ASC, b DESC", 0, 0, 0, None),
    ];

    for (sql, before, after, fix_count, expected_text) in cases {
        assert_rule_case(sql, issue_codes::LINT_AM_003, before, after, fix_count);

        if let Some(expected) = expected_text {
            let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }
}

#[test]
fn sqlfluff_am006_cases_are_fixed_or_unchanged() {
    let cases = [
        (
            "SELECT a FROM t JOIN u ON t.id = u.id",
            1,
            0,
            1,
            Some("INNER JOIN"),
        ),
        (
            "SELECT a FROM t JOIN u ON t.id = u.id JOIN v ON u.id = v.id",
            2,
            0,
            2,
            Some("INNER JOIN U"),
        ),
        ("SELECT a FROM t INNER JOIN u ON t.id = u.id", 0, 0, 0, None),
        ("SELECT a FROM t LEFT JOIN u ON t.id = u.id", 0, 0, 0, None),
    ];

    for (sql, before, after, fix_count, expected_text) in cases {
        assert_rule_case(sql, issue_codes::LINT_AM_005, before, after, fix_count);

        if let Some(expected) = expected_text {
            let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }
}

#[test]
fn sqlfluff_am005_outer_and_both_configs_are_fixed() {
    let outer_config = LintConfig {
        enabled: true,
        disabled_rules: vec![issue_codes::LINT_CV_008.to_string()],
        rule_configs: std::collections::BTreeMap::from([(
            "ambiguous.join".to_string(),
            serde_json::json!({"fully_qualify_join_types": "outer"}),
        )]),
    };
    let both_config = LintConfig {
        enabled: true,
        disabled_rules: vec![issue_codes::LINT_CV_008.to_string()],
        rule_configs: std::collections::BTreeMap::from([(
            "ambiguous.join".to_string(),
            serde_json::json!({"fully_qualify_join_types": "both"}),
        )]),
    };

    let outer_cases = [
        (
            "SELECT a FROM t LEFT JOIN u ON t.id = u.id",
            1,
            0,
            1,
            Some("LEFT OUTER JOIN"),
        ),
        (
            "SELECT a FROM t RIGHT JOIN u ON t.id = u.id",
            1,
            0,
            1,
            Some("RIGHT OUTER JOIN"),
        ),
        (
            "SELECT a FROM t FULL JOIN u ON t.id = u.id",
            1,
            0,
            1,
            Some("FULL OUTER JOIN"),
        ),
        (
            "SELECT a FROM t full join u ON t.id = u.id",
            1,
            0,
            1,
            Some("FULL OUTER JOIN"),
        ),
        ("SELECT a FROM t JOIN u ON t.id = u.id", 0, 0, 0, None),
    ];
    for (sql, before, after, fix_count, expected_text) in outer_cases {
        assert_rule_case_with_config(
            sql,
            issue_codes::LINT_AM_005,
            before,
            after,
            fix_count,
            &outer_config,
        );
        if let Some(expected) = expected_text {
            let out = apply_fix_with_config(sql, &outer_config);
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }

    let both_cases = [
        (
            "SELECT a FROM t JOIN u ON t.id = u.id",
            1,
            0,
            1,
            Some("INNER JOIN"),
        ),
        (
            "SELECT a FROM t LEFT JOIN u ON t.id = u.id",
            1,
            0,
            1,
            Some("LEFT OUTER JOIN"),
        ),
        (
            "SELECT a FROM t FULL JOIN u ON t.id = u.id",
            1,
            0,
            1,
            Some("FULL OUTER JOIN"),
        ),
    ];
    for (sql, before, after, fix_count, expected_text) in both_cases {
        assert_rule_case_with_config(
            sql,
            issue_codes::LINT_AM_005,
            before,
            after,
            fix_count,
            &both_config,
        );
        if let Some(expected) = expected_text {
            let out = apply_fix_with_config(sql, &both_config);
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }
}

#[test]
fn sqlfluff_am009_cases_are_fixed_or_unchanged() {
    let cases = [
        (
            "SELECT foo.a, bar.b FROM foo INNER JOIN bar",
            1,
            0,
            1,
            Some("CROSS JOIN BAR"),
        ),
        (
            "SELECT foo.a, bar.b FROM foo LEFT JOIN bar",
            1,
            0,
            1,
            Some("CROSS JOIN BAR"),
        ),
        (
            "SELECT foo.a, bar.b FROM foo JOIN bar WHERE foo.a = bar.a OR foo.x = 3",
            0,
            0,
            0,
            None,
        ),
        ("SELECT foo.a, bar.b FROM foo CROSS JOIN bar", 0, 0, 0, None),
        (
            "SELECT foo.id, bar.id FROM foo LEFT JOIN bar USING (id)",
            0,
            0,
            0,
            None,
        ),
    ];

    for (sql, before, after, fix_count, expected_text) in cases {
        assert_rule_case(sql, issue_codes::LINT_AM_008, before, after, fix_count);

        if let Some(expected) = expected_text {
            let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }
}

#[test]
fn sqlfluff_al007_force_enabled_single_table_alias_is_fixed() {
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: vec![],
        rule_configs: std::collections::BTreeMap::from([(
            "aliasing.forbid".to_string(),
            serde_json::json!({"force_enable": true}),
        )]),
    };
    let sql = "SELECT u.id FROM users u";
    assert_rule_case_with_config(sql, issue_codes::LINT_AL_007, 1, 0, 1, &lint_config);

    let out = apply_fix_with_config(sql, &lint_config);
    let fixed_upper = out.sql.to_ascii_uppercase();
    assert!(
        fixed_upper.contains("FROM USERS"),
        "expected table alias to be removed: {}",
        out.sql
    );
    assert!(
        !fixed_upper.contains("FROM USERS U"),
        "expected unnecessary table alias to be removed: {}",
        out.sql
    );
    assert!(
        fixed_upper.contains("USERS.ID"),
        "expected references to use table name after alias removal: {}",
        out.sql
    );
}

#[test]
fn sqlfluff_al009_fix_respects_case_sensitive_mode() {
    let lint_config = LintConfig {
        enabled: true,
        // Disable CP_002 so identifier lowercasing does not turn `A` into `a`,
        // which would create a new AL_009 self-alias violation.
        disabled_rules: vec![issue_codes::LINT_CP_002.to_string()],
        rule_configs: std::collections::BTreeMap::from([(
            "aliasing.self_alias.column".to_string(),
            serde_json::json!({"alias_case_check": "case_sensitive"}),
        )]),
    };
    let sql = "SELECT a AS A FROM t";
    assert_rule_case_with_config(sql, issue_codes::LINT_AL_009, 0, 0, 0, &lint_config);

    let out = apply_fix_with_config(sql, &lint_config);
    assert!(
        out.sql.contains("AS A"),
        "case-sensitive mode should keep case-mismatched alias: {}",
        out.sql
    );
}

#[test]
fn sqlfluff_al009_ast_fix_keeps_table_aliases() {
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: vec![issue_codes::LINT_AL_007.to_string()],
        rule_configs: std::collections::BTreeMap::new(),
    };
    let sql = "SELECT t.a AS a FROM t AS t";
    assert_rule_case_with_config(sql, issue_codes::LINT_AL_009, 1, 0, 1, &lint_config);

    let out = apply_fix_with_config(sql, &lint_config);
    let fixed_upper = out.sql.to_ascii_uppercase();
    assert!(
        fixed_upper.contains("FROM T AS T"),
        "AL09 fix should not remove table alias declarations: {}",
        out.sql
    );
    assert!(
        !fixed_upper.contains("T.A AS A"),
        "expected only column self-alias to be removed: {}",
        out.sql
    );
}

#[test]
fn sqlfluff_st002_unnecessary_case_fix_cases() {
    let cases = [
        // Bool coalesce: CASE WHEN cond THEN TRUE ELSE FALSE END → coalesce(cond, false)
        (
            "SELECT CASE WHEN x > 0 THEN true ELSE false END FROM t",
            1,
            0,
            1,
            Some("COALESCE(X > 0, FALSE)"),
        ),
        // Negated bool: CASE WHEN cond THEN FALSE ELSE TRUE END → not coalesce(cond, false)
        (
            "SELECT CASE WHEN x > 0 THEN false ELSE true END FROM t",
            1,
            0,
            1,
            Some("NOT COALESCE(X > 0, FALSE)"),
        ),
        // Null coalesce: CASE WHEN x IS NULL THEN y ELSE x END → coalesce(x, y)
        (
            "SELECT CASE WHEN x IS NULL THEN 0 ELSE x END FROM t",
            1,
            0,
            1,
            Some("COALESCE(X, 0)"),
        ),
        // Not flagged: regular searched CASE (not an unnecessary pattern)
        (
            "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' END FROM t",
            0,
            0,
            0,
            None,
        ),
    ];

    for (sql, before, after, fix_count, expected_text) in cases {
        assert_rule_case(sql, issue_codes::LINT_ST_002, before, after, fix_count);

        if let Some(expected) = expected_text {
            let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }
}

#[test]
fn sqlfluff_st006_cases_are_fixed_or_unchanged() {
    let cases = [
        ("SELECT a + 1, a FROM t", 1, 0, 1, Some("A,\n    A + 1")),
        (
            "SELECT a + 1, b + 2, a FROM t",
            1,
            0,
            1,
            Some("A,\n    A + 1,\n    B + 2"),
        ),
        (
            "SELECT a + 1, b AS b_alias FROM t",
            1,
            0,
            1,
            Some("B AS B_ALIAS,\n    A + 1"),
        ),
        ("SELECT a, b + 1 FROM t", 0, 0, 0, None),
        ("SELECT a + 1, b + 2 FROM t", 0, 0, 0, None),
    ];

    for (sql, before, after, fix_count, expected_text) in cases {
        assert_rule_case(sql, issue_codes::LINT_ST_006, before, after, fix_count);

        if let Some(expected) = expected_text {
            let out = apply_core_only_fixes(sql);
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }
}

#[test]
fn sqlfluff_st008_cases_are_fixed_or_unchanged() {
    let cases = [
        (
            "SELECT DISTINCT(a) FROM t",
            1,
            0,
            1,
            Some("SELECT DISTINCT A"),
        ),
        ("SELECT DISTINCT a FROM t", 0, 0, 0, None),
        ("SELECT a FROM t", 0, 0, 0, None),
    ];

    for (sql, before, after, fix_count, expected_text) in cases {
        assert_rule_case(sql, issue_codes::LINT_ST_008, before, after, fix_count);

        if let Some(expected) = expected_text {
            let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }
}

#[test]
fn sqlfluff_st009_cases_are_fixed_or_unchanged() {
    let cases = [
        (
            "SELECT foo.a, bar.b FROM foo LEFT JOIN bar ON bar.a = foo.a",
            1,
            0,
            1,
            Some("ON FOO.A = BAR.A"),
        ),
        (
            "SELECT foo.a, foo.b, bar.c FROM foo LEFT JOIN bar ON bar.a = foo.a AND bar.b = foo.b",
            1,
            1,
            0,
            None,
        ),
        (
            "SELECT foo.a, bar.b FROM foo LEFT JOIN bar ON foo.a = bar.a",
            0,
            0,
            0,
            None,
        ),
        (
            "SELECT foo.a, bar.b FROM foo LEFT JOIN bar ON bar.b = a",
            0,
            0,
            0,
            None,
        ),
        (
            "SELECT foo.a, bar.b FROM foo AS x LEFT JOIN bar AS y ON y.a = x.a",
            1,
            0,
            1,
            Some("ON X.A = Y.A"),
        ),
    ];

    for (sql, before, after, fix_count, expected_text) in cases {
        if before == after && fix_count == 0 {
            let initial = lint_rule_count(sql, issue_codes::LINT_ST_009);
            assert_eq!(
                initial,
                before,
                "unexpected initial lint count for {} in SQL: {}",
                issue_codes::LINT_ST_009,
                sql
            );

            let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
            assert_eq!(
                fix_count_for_code(&out.counts, issue_codes::LINT_ST_009),
                0,
                "unexpected fix count for {} in SQL: {}",
                issue_codes::LINT_ST_009,
                sql
            );
            let after_count = lint_rule_count(&out.sql, issue_codes::LINT_ST_009);
            assert_eq!(
                after_count,
                after,
                "unexpected lint count after fix for {}. SQL: {}",
                issue_codes::LINT_ST_009,
                out.sql
            );
        } else {
            assert_rule_case(sql, issue_codes::LINT_ST_009, before, after, fix_count);
        }

        if let Some(expected) = expected_text {
            let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }
}

#[test]
fn sqlfluff_st007_cases_are_fixed_or_unchanged() {
    let cases = [
        (
            "SELECT * FROM a JOIN b USING (id)",
            1,
            0,
            1,
            Some("ON A.ID = B.ID"),
        ),
        (
            "SELECT * FROM a AS x JOIN b AS y USING (id)",
            1,
            0,
            1,
            Some("ON X.ID = Y.ID"),
        ),
        (
            "SELECT * FROM a JOIN b USING (id, tenant_id)",
            1,
            0,
            1,
            Some("ON A.ID = B.ID AND A.TENANT_ID = B.TENANT_ID"),
        ),
        ("SELECT * FROM a JOIN b ON a.id = b.id", 0, 0, 0, None),
    ];

    for (sql, before, after, fix_count, expected_text) in cases {
        assert_rule_case(sql, issue_codes::LINT_ST_007, before, after, fix_count);

        if let Some(expected) = expected_text {
            let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }
}

#[test]
fn sqlfluff_st004_cases_are_fixed_or_unchanged() {
    let cases = [
            (
                "SELECT CASE WHEN species = 'Rat' THEN 'Squeak' ELSE CASE WHEN species = 'Dog' THEN 'Woof' END END AS sound FROM mytable",
                1,
                1,
                0,
            ),
            (
                "SELECT CASE WHEN species = 'Rat' THEN 'Squeak' ELSE CASE WHEN species = 'Dog' THEN 'Woof' WHEN species = 'Mouse' THEN 'Squeak' ELSE 'Other' END END AS sound FROM mytable",
                1,
                1,
                0,
            ),
            (
                "SELECT CASE WHEN species = 'Rat' THEN CASE WHEN colour = 'Black' THEN 'Growl' WHEN colour = 'Grey' THEN 'Squeak' END END AS sound FROM mytable",
                0,
                0,
                0,
            ),
            (
                "SELECT CASE WHEN day_of_month IN (11, 12, 13) THEN 'TH' ELSE CASE MOD(day_of_month, 10) WHEN 1 THEN 'ST' WHEN 2 THEN 'ND' WHEN 3 THEN 'RD' ELSE 'TH' END END AS ordinal_suffix FROM calendar",
                0,
                0,
                0,
            ),
            (
                "SELECT CASE x WHEN 0 THEN 'zero' WHEN 5 THEN 'five' ELSE CASE x WHEN 10 THEN 'ten' WHEN 20 THEN 'twenty' ELSE 'other' END END FROM tab_a",
                1,
                1,
                0,
            ),
        ];

    for (sql, before, after, fix_count) in cases {
        assert_rule_case(sql, issue_codes::LINT_ST_004, before, after, fix_count);
    }
}

#[test]
fn sqlfluff_cv003_cases_are_fixed_or_unchanged() {
    let cases = [
        ("SELECT a FROM foo WHERE a IS NULL", 0, 0, 0, None),
        ("SELECT a FROM foo WHERE a IS NOT NULL", 0, 0, 0, None),
        (
            "SELECT a FROM foo WHERE a <> NULL",
            1,
            0,
            1,
            Some("WHERE A IS NOT NULL"),
        ),
        (
            "SELECT a FROM foo WHERE a <> NULL AND b != NULL AND c = 'foo'",
            2,
            0,
            2,
            Some("A IS NOT NULL AND B IS NOT NULL"),
        ),
        (
            "SELECT a FROM foo WHERE a = NULL",
            1,
            0,
            1,
            Some("WHERE A IS NULL"),
        ),
        (
            "SELECT a FROM foo WHERE a=NULL",
            1,
            0,
            1,
            Some("WHERE A IS NULL"),
        ),
        (
            "SELECT a FROM foo WHERE a = b OR (c > d OR e = NULL)",
            1,
            0,
            1,
            Some("OR E IS NULL"),
        ),
        ("UPDATE table1 SET col = NULL WHERE col = ''", 0, 0, 0, None),
    ];

    for (sql, before, after, fix_count, expected_text) in cases {
        assert_rule_case(sql, issue_codes::LINT_CV_005, before, after, fix_count);

        if let Some(expected) = expected_text {
            let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }
}

#[test]
fn sqlfluff_cv001_cases_are_fixed_or_unchanged() {
    let cases = [
        ("SELECT coalesce(foo, 0) AS bar FROM baz", 0, 0, 0),
        ("SELECT ifnull(foo, 0) AS bar FROM baz", 1, 0, 1),
        ("SELECT nvl(foo, 0) AS bar FROM baz", 1, 0, 1),
        (
            "SELECT CASE WHEN x IS NULL THEN 'default' ELSE x END FROM t",
            0,
            0,
            0,
        ),
    ];

    for (sql, before, after, fix_count) in cases {
        assert_rule_case(sql, issue_codes::LINT_CV_002, before, after, fix_count);
    }
}

#[test]
fn sqlfluff_cv003_trailing_comma_cases_are_fixed_or_unchanged() {
    let cases = [
        ("SELECT a, FROM t", 1, 0, 1),
        ("SELECT a , FROM t", 1, 0, 1),
        ("SELECT a FROM t", 0, 0, 0),
    ];

    for (sql, before, after, fix_count) in cases {
        assert_rule_case(sql, issue_codes::LINT_CV_003, before, after, fix_count);
    }
}

#[test]
fn sqlfluff_cv001_not_equal_style_cases_are_fixed_or_unchanged() {
    let cases = [
        ("SELECT * FROM t WHERE a <> b AND c != d", 1, 0, 1),
        ("SELECT * FROM t WHERE a != b", 0, 0, 0),
    ];

    for (sql, before, after, fix_count) in cases {
        assert_rule_case(sql, issue_codes::LINT_CV_001, before, after, fix_count);
    }
}

#[test]
fn sqlfluff_cv008_cases_are_fixed_or_unchanged() {
    let cases: [(&str, usize, usize, usize, Option<&str>); 4] = [
        ("SELECT * FROM a RIGHT JOIN b ON a.id = b.id", 1, 1, 0, None),
        (
            "SELECT a.id FROM a JOIN b ON a.id = b.id RIGHT JOIN c ON b.id = c.id",
            1,
            1,
            0,
            None,
        ),
        (
            "SELECT a.id FROM a RIGHT JOIN b ON a.id = b.id RIGHT JOIN c ON b.id = c.id",
            2,
            2,
            0,
            None,
        ),
        ("SELECT * FROM a LEFT JOIN b ON a.id = b.id", 0, 0, 0, None),
    ];

    for (sql, before, after, fix_count, expected_text) in cases {
        assert_rule_case(sql, issue_codes::LINT_CV_008, before, after, fix_count);

        if let Some(expected) = expected_text {
            let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
            assert!(
                out.sql.to_ascii_uppercase().contains(expected),
                "expected {expected:?} in fixed SQL, got: {}",
                out.sql
            );
        }
    }
}

#[test]
fn sqlfluff_cv007_cases_are_fixed_or_unchanged() {
    let cases = [
        ("(SELECT 1)", 1, 0, 1),
        ("((SELECT 1))", 1, 0, 1),
        ("SELECT 1", 0, 0, 0),
    ];

    for (sql, before, after, fix_count) in cases {
        assert_rule_case(sql, issue_codes::LINT_CV_007, before, after, fix_count);
    }
}

#[test]
fn cv007_fix_respects_disabled_rules() {
    let sql = "(SELECT 1)\n";
    let out = apply_lint_fixes(
        sql,
        Dialect::Generic,
        &[issue_codes::LINT_CV_007.to_string()],
    )
    .expect("fix result");
    assert_eq!(out.sql, sql);
    assert_eq!(out.counts.get(issue_codes::LINT_CV_007), 0);
}

#[test]
fn cv010_fix_converts_double_to_single_in_bigquery() {
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: vec![],
        rule_configs: std::collections::BTreeMap::from([(
            "convention.quoted_literals".to_string(),
            serde_json::json!({"preferred_quoted_literal_style": "single_quotes"}),
        )]),
    };
    // In BigQuery, both "abc" and 'abc' are string literals.
    let sql = "SELECT \"abc\"";
    let before = lint_rule_count_with_config_in_dialect(
        sql,
        issue_codes::LINT_CV_010,
        Dialect::Bigquery,
        &lint_config,
    );
    assert_eq!(
        before, 1,
        "CV10 should flag double-quoted string in BigQuery with single_quotes preference"
    );

    let out = apply_lint_fixes_with_lint_config(sql, Dialect::Bigquery, &lint_config)
        .expect("fix result");
    assert!(
        out.sql.contains("'abc'"),
        "expected double-quoted string to be converted to single-quoted: {}",
        out.sql
    );
}

#[test]
fn cv011_cast_preference_rewrites_double_colon_style() {
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: vec![],
        rule_configs: std::collections::BTreeMap::from([(
            "convention.casting_style".to_string(),
            serde_json::json!({"preferred_type_casting_style": "cast"}),
        )]),
    };
    let sql = "SELECT amount::INT FROM t";
    assert_rule_case_with_config(sql, issue_codes::LINT_CV_011, 1, 0, 1, &lint_config);

    let out = apply_fix_with_config(sql, &lint_config);
    assert!(
        out.sql.to_ascii_uppercase().contains("CAST(AMOUNT AS INT)"),
        "expected CAST(...) rewrite for CV_011 fix: {}",
        out.sql
    );
}

#[test]
fn cv011_shorthand_preference_rewrites_cast_style_when_safe() {
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: vec![],
        rule_configs: std::collections::BTreeMap::from([(
            "LINT_CV_011".to_string(),
            serde_json::json!({"preferred_type_casting_style": "shorthand"}),
        )]),
    };
    let sql = "SELECT CAST(amount AS INT) FROM t";
    assert_rule_case_with_config(sql, issue_codes::LINT_CV_011, 1, 0, 1, &lint_config);

    let out = apply_fix_with_config(sql, &lint_config);
    assert!(
        out.sql.to_ascii_uppercase().contains("AMOUNT::INT"),
        "expected :: rewrite for CV_011 fix: {}",
        out.sql
    );
}

#[test]
fn sqlfluff_st012_cases_are_fixed_or_unchanged() {
    let cases = [
        ("SELECT 1;;", 1, 0, 1),
        ("SELECT 1;\n \t ;", 1, 0, 1),
        ("SELECT 1;", 0, 0, 0),
    ];

    for (sql, before, after, fix_count) in cases {
        assert_rule_case(sql, issue_codes::LINT_ST_012, before, after, fix_count);
    }
}

#[test]
fn sqlfluff_st002_cases_are_fixed_or_unchanged() {
    let cases = [
            ("SELECT CASE WHEN x > 1 THEN 'a' ELSE NULL END FROM t", 1, 0, 1),
            (
                "SELECT CASE name WHEN 'cat' THEN 'meow' WHEN 'dog' THEN 'woof' ELSE NULL END FROM t",
                1,
                0,
                1,
            ),
            (
                "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' WHEN x = 3 THEN 'c' ELSE NULL END FROM t",
                1,
                0,
                1,
            ),
            (
                "SELECT CASE WHEN x > 0 THEN CASE WHEN y > 0 THEN 'pos' ELSE NULL END ELSE NULL END FROM t",
                2,
                0,
                2,
            ),
            (
                "SELECT * FROM t WHERE (CASE WHEN x > 0 THEN 1 ELSE NULL END) IS NOT NULL",
                1,
                0,
                1,
            ),
            (
                "WITH cte AS (SELECT CASE WHEN x > 0 THEN 'yes' ELSE NULL END AS flag FROM t) SELECT * FROM cte",
                1,
                0,
                1,
            ),
            ("SELECT CASE WHEN x > 1 THEN 'a' END FROM t", 0, 0, 0),
            (
                "SELECT CASE name WHEN 'cat' THEN 'meow' ELSE UPPER(name) END FROM t",
                0,
                0,
                0,
            ),
            ("SELECT CASE WHEN x > 1 THEN 'a' ELSE 'b' END FROM t", 0, 0, 0),
        ];

    for (sql, before, after, fix_count) in cases {
        assert_rule_case(sql, issue_codes::LINT_ST_001, before, after, fix_count);
    }
}

#[test]
fn count_style_cases_are_fixed_or_unchanged() {
    let cases = [
        ("SELECT COUNT(1) FROM t", 1, 0, 1),
        (
            "SELECT col FROM t GROUP BY col HAVING COUNT(1) > 5",
            1,
            0,
            1,
        ),
        (
            "SELECT * FROM t WHERE id IN (SELECT COUNT(1) FROM t2 GROUP BY col)",
            1,
            0,
            1,
        ),
        ("SELECT COUNT(1), COUNT(1) FROM t", 2, 0, 2),
        (
            "WITH cte AS (SELECT COUNT(1) AS cnt FROM t) SELECT * FROM cte",
            1,
            0,
            1,
        ),
        ("SELECT COUNT(*) FROM t", 0, 0, 0),
        ("SELECT COUNT(id) FROM t", 0, 0, 0),
        ("SELECT COUNT(0) FROM t", 1, 0, 1),
        ("SELECT COUNT(01) FROM t", 1, 0, 1),
        ("SELECT COUNT(DISTINCT id) FROM t", 0, 0, 0),
    ];

    for (sql, before, after, fix_count) in cases {
        assert_rule_case(sql, issue_codes::LINT_CV_004, before, after, fix_count);
    }
}

#[test]
fn safe_mode_blocks_template_tag_edits_but_applies_non_template_fixes() {
    let sql = "SELECT '{{foo}}' AS templated, COUNT(1) FROM t";
    let out = apply_lint_fixes_with_options(
        sql,
        Dialect::Generic,
        &default_lint_config(),
        FixOptions {
            include_unsafe_fixes: false,
            include_rewrite_candidates: true,
        },
    )
    .expect("fix result");

    assert!(
        out.sql.contains("{{foo}}"),
        "template tag bytes should be preserved in safe mode: {}",
        out.sql
    );
    assert!(
        out.sql.to_ascii_uppercase().contains("COUNT(*)"),
        "non-template safe fixes should still apply: {}",
        out.sql
    );
    assert!(
        out.skipped_counts.protected_range_blocked > 0,
        "template-tag edits should be blocked in safe mode"
    );
}

#[test]
fn unsafe_mode_allows_template_tag_edits() {
    let sql = "SELECT '{{foo}}' AS templated, COUNT(1) FROM t";
    let out = apply_lint_fixes_with_options(
        sql,
        Dialect::Generic,
        &default_lint_config(),
        FixOptions {
            include_unsafe_fixes: true,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix result");

    assert!(
        out.sql.contains("{{ foo }}"),
        "unsafe mode should allow template-tag formatting fixes: {}",
        out.sql
    );
    assert!(
        out.sql.to_ascii_uppercase().contains("COUNT(*)"),
        "other fixes should still apply: {}",
        out.sql
    );
}

#[test]
fn comments_are_not_globally_skipped() {
    let sql = "-- keep this comment\nSELECT COUNT(1) FROM t";
    let out = apply_lint_fixes_with_options(
        sql,
        Dialect::Generic,
        &default_lint_config(),
        FixOptions {
            include_unsafe_fixes: false,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix result");
    assert!(
        !out.skipped_due_to_comments,
        "comment presence should not skip all fixes"
    );
    assert!(
        out.sql.contains("-- keep this comment"),
        "comment text must be preserved: {}",
        out.sql
    );
    assert!(
        out.sql.to_ascii_uppercase().contains("COUNT(*)"),
        "non-comment region should still be fixable: {}",
        out.sql
    );
}

#[test]
fn mysql_hash_comments_are_not_globally_skipped() {
    let sql = "# keep this comment\nSELECT COUNT(1) FROM t";
    let out = apply_lint_fixes_with_options(
        sql,
        Dialect::Mysql,
        &default_lint_config(),
        FixOptions {
            include_unsafe_fixes: false,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix result");
    assert!(
        !out.skipped_due_to_comments,
        "comment presence should not skip all fixes"
    );
    assert!(
        out.sql.contains("# keep this comment"),
        "comment text must be preserved: {}",
        out.sql
    );
    assert!(
        out.sql.to_ascii_uppercase().contains("COUNT(*)"),
        "non-comment region should still be fixable: {}",
        out.sql
    );
}

#[test]
fn does_not_treat_double_quoted_comment_markers_as_comments() {
    let sql = "SELECT \"a--b\", \"x/*y\" FROM t";
    assert!(!contains_comment_markers(sql, Dialect::Generic));
}

#[test]
fn does_not_treat_backtick_or_bracketed_markers_as_comments() {
    let sql = "SELECT `a--b`, [x/*y] FROM t";
    assert!(!contains_comment_markers(sql, Dialect::Mysql));
}

#[test]
fn fix_mode_does_not_skip_double_quoted_markers() {
    let sql = "SELECT \"a--b\", COUNT(1) FROM t";
    let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
    assert!(!out.skipped_due_to_comments);
}

#[test]
fn fix_mode_does_not_skip_backtick_markers() {
    let sql = "SELECT `a--b`, COUNT(1) FROM t";
    let out = apply_lint_fixes(sql, Dialect::Mysql, &[]).expect("fix result");
    assert!(!out.skipped_due_to_comments);
}

#[test]
fn planner_blocks_protected_ranges_and_applies_non_overlapping_edits() {
    let sql = "SELECT '{{foo}}' AS templated, 1";
    let protected = collect_comment_protected_ranges(sql, Dialect::Generic, true);
    let template_idx = sql.find("{{foo}}").expect("template exists");
    let one_idx = sql.rfind('1').expect("digit exists");

    let planned = plan_fix_candidates(
        sql,
        vec![
            FixCandidate {
                start: template_idx,
                end: template_idx + "{{foo}}".len(),
                replacement: String::new(),
                applicability: FixCandidateApplicability::Safe,
                source: FixCandidateSource::PrimaryRewrite,
                rule_code: None,
            },
            FixCandidate {
                start: one_idx,
                end: one_idx + 1,
                replacement: "2".to_string(),
                applicability: FixCandidateApplicability::Safe,
                source: FixCandidateSource::PrimaryRewrite,
                rule_code: None,
            },
        ],
        &protected,
        false,
    );

    let applied = apply_planned_edits(sql, &planned.edits);
    assert!(
        applied.contains("{{foo}}"),
        "template span should remain protected: {applied}"
    );
    assert!(
        applied.ends_with("2"),
        "expected non-overlapping edit: {applied}"
    );
    assert_eq!(planned.skipped.protected_range_blocked, 1);
}

#[test]
fn planner_is_deterministic_for_conflicting_candidates() {
    let sql = "SELECT 1";
    let one_idx = sql.rfind('1').expect("digit exists");

    let left_first = plan_fix_candidates(
        sql,
        vec![
            FixCandidate {
                start: one_idx,
                end: one_idx + 1,
                replacement: "9".to_string(),
                applicability: FixCandidateApplicability::Safe,
                source: FixCandidateSource::PrimaryRewrite,
                rule_code: None,
            },
            FixCandidate {
                start: one_idx,
                end: one_idx + 1,
                replacement: "2".to_string(),
                applicability: FixCandidateApplicability::Safe,
                source: FixCandidateSource::PrimaryRewrite,
                rule_code: None,
            },
        ],
        &[],
        false,
    );
    let right_first = plan_fix_candidates(
        sql,
        vec![
            FixCandidate {
                start: one_idx,
                end: one_idx + 1,
                replacement: "2".to_string(),
                applicability: FixCandidateApplicability::Safe,
                source: FixCandidateSource::PrimaryRewrite,
                rule_code: None,
            },
            FixCandidate {
                start: one_idx,
                end: one_idx + 1,
                replacement: "9".to_string(),
                applicability: FixCandidateApplicability::Safe,
                source: FixCandidateSource::PrimaryRewrite,
                rule_code: None,
            },
        ],
        &[],
        false,
    );

    let left_sql = apply_planned_edits(sql, &left_first.edits);
    let right_sql = apply_planned_edits(sql, &right_first.edits);
    assert_eq!(left_sql, "SELECT 2");
    assert_eq!(left_sql, right_sql);
    assert_eq!(left_first.skipped.overlap_conflict_blocked, 1);
    assert_eq!(right_first.skipped.overlap_conflict_blocked, 1);
}

#[test]
fn core_autofix_candidates_are_collected_and_applied() {
    let sql = "SELECT 1";
    let one_idx = sql.rfind('1').expect("digit exists");
    let issues = vec![serde_json::json!({
        "code": issue_codes::LINT_CV_004,
        "span": { "start": one_idx, "end": one_idx + 1 },
        "autofix": {
            "applicability": "safe",
            "edits": [
                {
                    "start": one_idx,
                    "end": one_idx + 1,
                    "replacement": "2"
                }
            ]
        }
    })];
    let candidates = build_fix_candidates_from_issue_values(sql, &issues);

    assert_eq!(candidates.len(), 1);
    let planned = plan_fix_candidates(sql, candidates, &[], false);
    let applied = apply_planned_edits(sql, &planned.edits);
    assert_eq!(applied, "SELECT 2");
}

#[test]
fn st002_core_autofix_candidates_apply_cleanly_in_safe_mode() {
    let sql = "SELECT CASE WHEN x > 0 THEN true ELSE false END FROM t\n";
    let issues = lint_issues(sql, Dialect::Generic, &default_lint_config());
    let candidates = build_fix_candidates_from_issue_autofixes(sql, &issues);
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.rule_code.as_deref() == Some(issue_codes::LINT_ST_002)),
        "expected ST002 core candidate from lint issues: {candidates:?}"
    );

    let protected = collect_comment_protected_ranges(sql, Dialect::Generic, true);
    let planned = plan_fix_candidates(sql, candidates, &protected, false);
    let applied = apply_planned_edits(sql, &planned.edits);
    assert_eq!(
        applied, "SELECT coalesce(x > 0, false) FROM t\n",
        "unexpected ST002 planned edits with skipped={:?}",
        planned.skipped
    );
}

#[test]
fn incremental_core_plan_applies_st009_even_when_not_top_priority() {
    let sql = "select foo.a, bar.b from foo left join bar on bar.a = foo.a";
    let lint_config = default_lint_config();
    let before_counts = lint_rule_counts(sql, Dialect::Generic, &lint_config);
    assert_eq!(
        before_counts
            .get(issue_codes::LINT_ST_009)
            .copied()
            .unwrap_or(0),
        1
    );

    let out = try_incremental_core_fix_plan(
        sql,
        Dialect::Generic,
        &lint_config,
        &before_counts,
        None,
        false,
        24,
        usize::MAX,
    )
    .expect("expected incremental ST009 fix");
    assert!(
        out.sql.contains("foo.a = bar.a"),
        "expected ST009 join condition reorder, got: {}",
        out.sql
    );

    let after_counts = lint_rule_counts(&out.sql, Dialect::Generic, &lint_config);
    assert_eq!(
        after_counts
            .get(issue_codes::LINT_ST_009)
            .copied()
            .unwrap_or(0),
        0
    );
}

#[test]
fn cached_pre_lint_state_matches_uncached_next_pass_behavior() {
    let sql = "SELECT 1 UNION SELECT 2";
    let lint_config = default_lint_config();
    let fix_options = FixOptions {
        include_unsafe_fixes: false,
        include_rewrite_candidates: false,
    };

    let first_pass = apply_lint_fixes_with_options_and_lint_state(
        sql,
        Dialect::Generic,
        &lint_config,
        fix_options,
        None,
    )
    .expect("first fix pass");

    let second_cached = apply_lint_fixes_with_options_and_lint_state(
        &first_pass.outcome.sql,
        Dialect::Generic,
        &lint_config,
        fix_options,
        Some(first_pass.post_lint_state.clone()),
    )
    .expect("second cached pass");
    let second_uncached = apply_lint_fixes_with_options_and_lint_state(
        &first_pass.outcome.sql,
        Dialect::Generic,
        &lint_config,
        fix_options,
        None,
    )
    .expect("second uncached pass");

    assert_eq!(second_cached.outcome.sql, second_uncached.outcome.sql);
    assert_eq!(second_cached.outcome.counts, second_uncached.outcome.counts);
    assert_eq!(
        second_cached.outcome.changed,
        second_uncached.outcome.changed
    );
    assert_eq!(
        second_cached.outcome.skipped_due_to_regression,
        second_uncached.outcome.skipped_due_to_regression
    );
}

#[test]
fn cp03_templated_case_emits_core_autofix_candidate() {
    let sql = "SELECT\n    {{ \"greatest(a, b)\" }},\n    GREATEST(i, j)\n";
    let config = lint_config_keep_only_rule(
        issue_codes::LINT_CP_003,
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "core".to_string(),
                serde_json::json!({"ignore_templated_areas": false}),
            )]),
        },
    );
    let issues = lint_issues(sql, Dialect::Ansi, &config);
    assert!(
        issues
            .iter()
            .any(|issue| { issue.code == issue_codes::LINT_CP_003 && issue.autofix.is_some() }),
        "expected CP03 issue with autofix metadata, got issues={issues:?}"
    );

    let candidates = build_fix_candidates_from_issue_autofixes(sql, &issues);
    assert!(
        candidates.iter().any(|candidate| {
            candidate.rule_code.as_deref() == Some(issue_codes::LINT_CP_003)
                && &sql[candidate.start..candidate.end] == "GREATEST"
                && candidate.replacement == "greatest"
        }),
        "expected CP03 GREATEST candidate, got candidates={candidates:?}"
    );
}

#[test]
fn planner_prefers_core_autofix_over_rewrite_conflicts() {
    let sql = "SELECT 1";
    let one_idx = sql.rfind('1').expect("digit exists");
    let core_issue = serde_json::json!({
        "code": issue_codes::LINT_CV_004,
        "autofix": {
            "start": one_idx,
            "end": one_idx + 1,
            "replacement": "9",
            "applicability": "safe"
        }
    });
    let core_candidate = build_fix_candidates_from_issue_values(sql, &[core_issue])[0].clone();
    let rewrite_candidate = FixCandidate {
        start: one_idx,
        end: one_idx + 1,
        replacement: "2".to_string(),
        applicability: FixCandidateApplicability::Safe,
        source: FixCandidateSource::PrimaryRewrite,
        rule_code: None,
    };

    let left_first = plan_fix_candidates(
        sql,
        vec![rewrite_candidate.clone(), core_candidate.clone()],
        &[],
        false,
    );
    let right_first = plan_fix_candidates(sql, vec![core_candidate, rewrite_candidate], &[], false);

    let left_sql = apply_planned_edits(sql, &left_first.edits);
    let right_sql = apply_planned_edits(sql, &right_first.edits);
    assert_eq!(left_sql, "SELECT 9");
    assert_eq!(left_sql, right_sql);
    assert_eq!(left_first.skipped.overlap_conflict_blocked, 1);
    assert_eq!(right_first.skipped.overlap_conflict_blocked, 1);
}

#[test]
fn rewrite_mode_falls_back_to_core_plan_when_core_rule_is_not_improved() {
    // Consistent mode normalizes to whichever style appears first.
    // `<>` is first, so the fix normalizes `!=` to `<>`.
    let sql = "SELECT * FROM t WHERE a <> b AND c != d";
    let out = apply_lint_fixes_with_options(
        sql,
        Dialect::Generic,
        &default_lint_config(),
        FixOptions {
            include_unsafe_fixes: true,
            include_rewrite_candidates: true,
        },
    )
    .expect("fix result");

    assert_eq!(fix_count_for_code(&out.counts, issue_codes::LINT_CV_001), 1);
    assert!(
        out.sql.contains("a <> b"),
        "expected CV001 style fix: {}",
        out.sql
    );
    assert!(
        out.sql.contains("c <> d"),
        "expected CV001 style fix: {}",
        out.sql
    );
    assert!(
        !out.sql.contains("!="),
        "expected no bang-style operator: {}",
        out.sql
    );
}

#[test]
fn core_autofix_applicability_is_mapped_to_existing_planner_logic() {
    let sql = "SELECT 1";
    let one_idx = sql.rfind('1').expect("digit exists");
    let issues = vec![
        serde_json::json!({
            "code": issue_codes::LINT_ST_005,
            "autofix": {
                "start": one_idx,
                "end": one_idx + 1,
                "replacement": "2",
                "applicability": "unsafe"
            }
        }),
        serde_json::json!({
            "code": issue_codes::LINT_ST_005,
            "autofix": {
                "start": one_idx,
                "end": one_idx + 1,
                "replacement": "3",
                "applicability": "display_only"
            }
        }),
    ];
    let candidates = build_fix_candidates_from_issue_values(sql, &issues);

    assert_eq!(
        candidates[0].applicability,
        FixCandidateApplicability::Unsafe
    );
    assert_eq!(
        candidates[1].applicability,
        FixCandidateApplicability::DisplayOnly
    );

    let planned_safe = plan_fix_candidates(sql, candidates.clone(), &[], false);
    assert_eq!(apply_planned_edits(sql, &planned_safe.edits), sql);
    assert_eq!(planned_safe.skipped.unsafe_skipped, 1);
    assert_eq!(planned_safe.skipped.display_only, 1);

    let planned_unsafe = plan_fix_candidates(sql, candidates, &[], true);
    assert_eq!(apply_planned_edits(sql, &planned_unsafe.edits), "SELECT 2");
    assert_eq!(planned_unsafe.skipped.display_only, 1);
}

#[test]
fn planner_tracks_unsafe_and_display_only_skips() {
    let sql = "SELECT 1";
    let one_idx = sql.rfind('1').expect("digit exists");
    let planned = plan_fix_candidates(
        sql,
        vec![
            FixCandidate {
                start: one_idx,
                end: one_idx + 1,
                replacement: "2".to_string(),
                applicability: FixCandidateApplicability::Unsafe,
                source: FixCandidateSource::UnsafeFallback,
                rule_code: None,
            },
            FixCandidate {
                start: 0,
                end: 0,
                replacement: String::new(),
                applicability: FixCandidateApplicability::DisplayOnly,
                source: FixCandidateSource::DisplayHint,
                rule_code: None,
            },
        ],
        &[],
        false,
    );
    let applied = apply_planned_edits(sql, &planned.edits);
    assert_eq!(applied, sql);
    assert_eq!(planned.skipped.unsafe_skipped, 1);
    assert_eq!(planned.skipped.display_only, 1);
}

#[test]
fn does_not_collapse_independent_select_statements() {
    let sql = "SELECT 1; SELECT 2;";
    let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
    assert!(
        !out.sql.to_ascii_uppercase().contains("DISTINCT SELECT"),
        "auto-fix must preserve statement boundaries: {}",
        out.sql
    );
    let parsed = parse_sql_with_dialect(&out.sql, Dialect::Generic).expect("parse fixed sql");
    assert_eq!(
        parsed.len(),
        2,
        "auto-fix should preserve two independent statements"
    );
}

#[test]
fn subquery_to_cte_text_fix_applies() {
    let fixed = fix_subquery_to_cte("SELECT * FROM (SELECT 1) sub");
    assert_eq!(fixed, "WITH sub AS (SELECT 1) SELECT * FROM sub");
}

#[test]
fn st005_core_autofix_applies_in_unsafe_mode_with_from_config() {
    let sql = "SELECT * FROM (SELECT 1) sub";
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: vec![],
        rule_configs: std::collections::BTreeMap::from([(
            "structure.subquery".to_string(),
            serde_json::json!({"forbid_subquery_in": "from"}),
        )]),
    };

    let fixed = apply_lint_fixes_with_options(
        sql,
        Dialect::Generic,
        &lint_config,
        FixOptions {
            include_unsafe_fixes: true,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix result")
    .sql;
    assert!(
        fixed.to_ascii_uppercase().contains("WITH SUB AS"),
        "expected unsafe core ST005 autofix to rewrite to CTE, got: {fixed}"
    );
}

#[test]
fn subquery_to_cte_text_fix_handles_nested_parentheses() {
    let fixed = fix_subquery_to_cte("SELECT * FROM (SELECT COUNT(*) FROM t) sub");
    assert_eq!(
        fixed,
        "WITH sub AS (SELECT COUNT(*) FROM t) SELECT * FROM sub"
    );
    parse_sql_with_dialect(&fixed, Dialect::Generic).expect("fixed SQL should parse");
}

#[test]
fn st005_ast_fix_rewrites_simple_join_derived_subquery_to_cte() {
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: vec![issue_codes::LINT_AM_005.to_string()],
        rule_configs: std::collections::BTreeMap::new(),
    };
    let sql = "SELECT t.id FROM t JOIN (SELECT id FROM u) sub ON t.id = sub.id";
    assert_rule_case_with_config(sql, issue_codes::LINT_ST_005, 1, 0, 1, &lint_config);

    let out = apply_fix_with_config(sql, &lint_config);
    assert!(
        out.sql.to_ascii_uppercase().contains("WITH SUB AS"),
        "expected AST ST_005 rewrite to emit CTE: {}",
        out.sql
    );
}

#[test]
fn st005_ast_fix_rewrites_simple_from_derived_subquery_with_config() {
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: vec![],
        rule_configs: std::collections::BTreeMap::from([(
            "structure.subquery".to_string(),
            serde_json::json!({"forbid_subquery_in": "from"}),
        )]),
    };
    let sql = "SELECT sub.id FROM (SELECT id FROM u) sub";
    assert_rule_case_with_config(sql, issue_codes::LINT_ST_005, 1, 0, 1, &lint_config);

    let out = apply_fix_with_config(sql, &lint_config);
    assert!(
        out.sql.to_ascii_uppercase().contains("WITH SUB AS"),
        "expected FROM-derived ST_005 rewrite to emit CTE: {}",
        out.sql
    );
}

#[test]
fn consecutive_semicolon_fix_ignores_string_literal_content() {
    let sql = "SELECT 'a;;b' AS txt;;";
    let out = apply_lint_fixes_with_options(
        sql,
        Dialect::Generic,
        &default_lint_config(),
        FixOptions {
            include_unsafe_fixes: true,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix result");
    assert!(
        out.sql.contains("'a;;b'"),
        "string literal content must be preserved: {}",
        out.sql
    );
    assert!(
        out.sql.trim_end().ends_with(';') && !out.sql.trim_end().ends_with(";;"),
        "trailing semicolon run should be collapsed to one terminator: {}",
        out.sql
    );
}

#[test]
fn consecutive_semicolon_fix_collapses_whitespace_separated_runs() {
    let out = apply_lint_fixes_with_options(
        "SELECT 1;\n \t ;",
        Dialect::Generic,
        &default_lint_config(),
        FixOptions {
            include_unsafe_fixes: true,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix result");
    assert_eq!(out.sql.matches(';').count(), 1);
}

#[test]
fn lint_fix_subquery_with_function_call_is_parseable() {
    let sql = "SELECT * FROM (SELECT COUNT(*) FROM t) sub";
    let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
    assert!(
        !out.skipped_due_to_regression,
        "function-call subquery rewrite should not be treated as regression: {}",
        out.sql
    );
    parse_sql_with_dialect(&out.sql, Dialect::Generic).expect("fixed SQL should parse");
}

#[test]
fn distinct_parentheses_fix_preserves_valid_sql() {
    let sql = "SELECT DISTINCT(a) FROM t";
    let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
    assert!(
        !out.sql.contains("a)"),
        "unexpected dangling parenthesis after fix: {}",
        out.sql
    );
    parse_sql_with_dialect(&out.sql, Dialect::Generic).expect("fixed SQL should parse");
}

#[test]
fn not_equal_fix_does_not_rewrite_string_literals() {
    let sql = "SELECT '<>' AS x, a<>b, c!=d FROM t";
    let out = apply_lint_fixes_with_options(
        sql,
        Dialect::Generic,
        &default_lint_config(),
        FixOptions {
            include_unsafe_fixes: false,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix result");
    assert!(
        out.sql.contains("'<>'"),
        "string literal should remain unchanged: {}",
        out.sql
    );
    let compact: String = out.sql.chars().filter(|ch| !ch.is_whitespace()).collect();
    let has_c_style = compact.contains("a!=b") && compact.contains("c!=d");
    let has_ansi_style = compact.contains("a<>b") && compact.contains("c<>d");
    assert!(
        has_c_style || has_ansi_style || compact.contains("a<>b") && compact.contains("c!=d"),
        "operator usage outside string literals should remain intact: {}",
        out.sql
    );
}

#[test]
fn spacing_fixes_do_not_rewrite_single_quoted_literals() {
    let operator_fixed = apply_lint_fixes_with_options(
        "SELECT payload->>'id', 'x=y' FROM t",
        Dialect::Generic,
        &default_lint_config(),
        FixOptions {
            include_unsafe_fixes: false,
            include_rewrite_candidates: false,
        },
    )
    .expect("operator spacing fix result")
    .sql;
    assert!(
        operator_fixed.contains("'x=y'"),
        "operator spacing must not mutate literals: {operator_fixed}"
    );
    assert!(
        operator_fixed.contains("payload ->>"),
        "operator spacing should still apply: {operator_fixed}"
    );

    let comma_fixed = apply_lint_fixes_with_options(
        "SELECT a,b, 'x,y' FROM t",
        Dialect::Generic,
        &default_lint_config(),
        FixOptions {
            include_unsafe_fixes: false,
            include_rewrite_candidates: false,
        },
    )
    .expect("comma spacing fix result")
    .sql;
    assert!(
        comma_fixed.contains("'x,y'"),
        "comma spacing must not mutate literals: {comma_fixed}"
    );
    assert!(
        !comma_fixed.contains("a,b"),
        "comma spacing should still apply: {comma_fixed}"
    );
}

#[test]
fn keyword_newline_fix_does_not_rewrite_literals_or_quoted_identifiers() {
    let sql = "SELECT COUNT(1), 'hello FROM world', \"x WHERE y\" FROM t WHERE a = 1";
    let fixed = apply_lint_fixes(sql, Dialect::Generic, &[])
        .expect("fix result")
        .sql;
    assert!(
        fixed.contains("'hello FROM world'"),
        "single-quoted literal should remain unchanged: {fixed}"
    );
    assert!(
        fixed.contains("\"x WHERE y\""),
        "double-quoted identifier should remain unchanged: {fixed}"
    );
    assert!(
        !fixed.contains("hello\nFROM world"),
        "keyword newline fix must not inject newlines into literals: {fixed}"
    );
}

#[test]
fn cp04_fix_reduces_literal_capitalisation_violations() {
    // Per-identifier: true and False both violate upper → 2 violations, 2 fixes.
    assert_rule_case(
        "SELECT NULL, true, False FROM t",
        issue_codes::LINT_CP_004,
        2,
        0,
        2,
    );
}

#[test]
fn cp05_fix_reduces_type_capitalisation_violations() {
    // Per-identifier: VarChar violates upper (INT is already correct) → 1 violation.
    assert_rule_case(
        "CREATE TABLE t (a INT, b VarChar(10));",
        issue_codes::LINT_CP_005,
        1,
        0,
        1,
    );
}

#[test]
fn cv06_fix_adds_missing_final_terminator() {
    assert_rule_case("SELECT 1 ;", issue_codes::LINT_CV_006, 1, 0, 1);
}

#[test]
fn lt03_fix_moves_trailing_operator_to_leading_position() {
    assert_rule_case("SELECT a +\n b FROM t", issue_codes::LINT_LT_003, 1, 0, 1);
}

#[test]
fn lt04_fix_moves_comma_around_templated_columns_in_ansi() {
    let leading_sql = "SELECT\n    c1,\n    {{ \"c2\" }} AS days_since\nFROM logs";
    let leading_config = lint_config_keep_only_rule(
        issue_codes::LINT_LT_004,
        LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.commas".to_string(),
                serde_json::json!({"line_position": "leading"}),
            )]),
        },
    );
    let leading_issues = lint_issues(leading_sql, Dialect::Ansi, &leading_config);
    let leading_lt04 = leading_issues
        .iter()
        .find(|issue| issue.code == issue_codes::LINT_LT_004)
        .expect("expected LT04 issue before fix");
    assert!(
        leading_lt04.autofix.is_some(),
        "expected LT04 issue to carry autofix metadata in fix pipeline"
    );
    let leading_out = apply_lint_fixes_with_options(
        leading_sql,
        Dialect::Ansi,
        &leading_config,
        FixOptions {
            include_unsafe_fixes: true,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix result");
    assert!(
        !leading_out.skipped_due_to_regression,
        "LT04 leading templated fix should not be treated as regression"
    );
    assert_eq!(
        leading_out.sql,
        "SELECT\n    c1\n    , {{ \"c2\" }} AS days_since\nFROM logs"
    );

    let trailing_sql = "SELECT\n    {{ \"c1\" }}\n    , c2 AS days_since\nFROM logs";
    let trailing_config =
        lint_config_keep_only_rule(issue_codes::LINT_LT_004, default_lint_config());
    let trailing_out = apply_lint_fixes_with_options(
        trailing_sql,
        Dialect::Ansi,
        &trailing_config,
        FixOptions {
            include_unsafe_fixes: true,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix result");
    assert!(
        !trailing_out.skipped_due_to_regression,
        "LT04 trailing templated fix should not be treated as regression"
    );
    assert_eq!(
        trailing_out.sql,
        "SELECT\n    {{ \"c1\" }},\n    c2 AS days_since\nFROM logs"
    );
}
#[test]
fn rf004_core_autofix_respects_rule_filter() {
    let sql = "select a from users as select\n";

    let out_rf_disabled = apply_lint_fixes(
        sql,
        Dialect::Generic,
        &[issue_codes::LINT_RF_004.to_string()],
    )
    .expect("fix result");
    assert_eq!(
        out_rf_disabled.sql, sql,
        "excluding RF_004 should block alias-keyword core autofix"
    );

    let out_al_disabled = apply_lint_fixes(
        sql,
        Dialect::Generic,
        &[issue_codes::LINT_AL_005.to_string()],
    )
    .expect("fix result");
    assert!(
        out_al_disabled.sql.contains("alias_select"),
        "excluding AL_005 must not block RF_004 core autofix: {}",
        out_al_disabled.sql
    );
}

#[test]
fn rf003_core_autofix_respects_rule_filter() {
    let sql = "select a.id, id2 from a\n";

    let rf_disabled_config = LintConfig {
        enabled: true,
        disabled_rules: vec![issue_codes::LINT_RF_003.to_string()],
        rule_configs: std::collections::BTreeMap::new(),
    };
    let out_rf_disabled = apply_lint_fixes_with_options(
        sql,
        Dialect::Generic,
        &rf_disabled_config,
        FixOptions {
            include_unsafe_fixes: true,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix result");
    assert!(
        !out_rf_disabled.sql.contains("a.id2"),
        "excluding RF_003 should block reference qualification core autofix: {}",
        out_rf_disabled.sql
    );

    let al_disabled_config = LintConfig {
        enabled: true,
        disabled_rules: vec![issue_codes::LINT_AL_005.to_string()],
        rule_configs: std::collections::BTreeMap::new(),
    };
    let out_al_disabled = apply_lint_fixes_with_options(
        sql,
        Dialect::Generic,
        &al_disabled_config,
        FixOptions {
            include_unsafe_fixes: true,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix result");
    assert!(
        out_al_disabled.sql.contains("a.id2"),
        "excluding AL_005 must not block RF_003 core autofix: {}",
        out_al_disabled.sql
    );
}

#[test]
fn al001_fix_still_improves_with_fix_mode() {
    let sql = "SELECT * FROM a x JOIN b y ON x.id = y.id";
    assert_rule_case(sql, issue_codes::LINT_AL_001, 2, 0, 2);

    let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
    let upper = out.sql.to_ascii_uppercase();
    assert!(
        upper.contains("FROM A AS X"),
        "expected explicit alias in fixed SQL, got: {}",
        out.sql
    );
    assert!(
        upper.contains("JOIN B AS Y"),
        "expected explicit alias in fixed SQL, got: {}",
        out.sql
    );
}

#[test]
fn al001_fix_does_not_synthesize_missing_aliases() {
    let sql = "SELECT COUNT(1) FROM users";
    let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");

    assert!(
        out.sql.to_ascii_uppercase().contains("COUNT(*)"),
        "expected non-AL001 fix to apply: {}",
        out.sql
    );
    assert!(
        !out.sql.to_ascii_uppercase().contains(" AS T"),
        "AL001 fixer must not generate synthetic aliases: {}",
        out.sql
    );
}

#[test]
fn al001_disabled_preserves_implicit_aliases_when_other_rules_fix() {
    let sql = "select count(1) from a x join b y on x.id = y.id";
    let out = apply_lint_fixes(
        sql,
        Dialect::Generic,
        &[issue_codes::LINT_AL_001.to_string()],
    )
    .expect("fix result");

    assert!(
        out.sql.to_ascii_uppercase().contains("COUNT(*)"),
        "expected non-AL001 fix to apply: {}",
        out.sql
    );
    assert!(
        out.sql.to_ascii_uppercase().contains("FROM A X"),
        "implicit alias should be preserved when AL001 is disabled: {}",
        out.sql
    );
    assert!(
        out.sql.to_ascii_uppercase().contains("JOIN B Y"),
        "implicit alias should be preserved when AL001 is disabled: {}",
        out.sql
    );
    assert!(
        lint_rule_count(&out.sql, issue_codes::LINT_AL_001) > 0,
        "AL001 violations should remain when the rule is disabled: {}",
        out.sql
    );
}

#[test]
fn al001_implicit_config_rewrites_explicit_aliases() {
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: vec![],
        rule_configs: std::collections::BTreeMap::from([(
            issue_codes::LINT_AL_001.to_string(),
            serde_json::json!({"aliasing": "implicit"}),
        )]),
    };

    let sql = "SELECT COUNT(1) FROM a AS x JOIN b AS y ON x.id = y.id";
    assert_eq!(
        lint_rule_count_with_config(sql, issue_codes::LINT_AL_001, &lint_config),
        2,
        "explicit aliases should violate AL001 under implicit mode"
    );

    let out = apply_fix_with_config(sql, &lint_config);
    assert!(
        out.sql.to_ascii_uppercase().contains("COUNT(*)"),
        "expected non-AL001 fix to apply: {}",
        out.sql
    );
    assert!(
        !out.sql.to_ascii_uppercase().contains(" AS X"),
        "implicit-mode AL001 should remove explicit aliases: {}",
        out.sql
    );
    assert!(
        !out.sql.to_ascii_uppercase().contains(" AS Y"),
        "implicit-mode AL001 should remove explicit aliases: {}",
        out.sql
    );
    assert_eq!(
        lint_rule_count_with_config(&out.sql, issue_codes::LINT_AL_001, &lint_config),
        0,
        "AL001 should be resolved under implicit mode: {}",
        out.sql
    );
}

#[test]
fn table_alias_occurrences_handles_with_insert_select_aliases() {
    let sql = r#"
WITH params AS (
    SELECT now() - interval '1 day' AS period_start, now() AS period_end
),
overall AS (
    SELECT route, nav_type, mark FROM metrics.page_performance
),
device_breakdown AS (
    SELECT route, nav_type, mark FROM (
        SELECT route, nav_type, mark FROM metrics.page_performance
    ) sub
),
network_breakdown AS (
    SELECT route, nav_type, mark FROM (
        SELECT route, nav_type, mark FROM metrics.page_performance
    ) sub
),
version_breakdown AS (
    SELECT route, nav_type, mark FROM (
        SELECT route, nav_type, mark FROM metrics.page_performance
    ) sub
)
INSERT INTO metrics.page_performance_summary (route, period_start, period_end, nav_type, mark)
SELECT o.route, p.period_start, p.period_end, o.nav_type, o.mark
FROM overall o
CROSS JOIN params p
LEFT JOIN device_breakdown d ON d.route = o.route
LEFT JOIN network_breakdown n ON n.route = o.route
LEFT JOIN version_breakdown v ON v.route = o.route
ON CONFLICT (route, period_start, nav_type, mark) DO UPDATE SET
    period_end = EXCLUDED.period_end;
"#;

    let occurrences =
        table_alias_occurrences(sql, Dialect::Postgres).expect("alias occurrences should parse");
    let implicit_count = occurrences
        .iter()
        .filter(|alias| !alias.explicit_as)
        .count();
    assert!(
        implicit_count >= 8,
        "expected implicit aliases in CTE+INSERT query, got {}: {:?}",
        implicit_count,
        occurrences
            .iter()
            .map(|alias| (&alias.alias_key, alias.explicit_as))
            .collect::<Vec<_>>()
    );
}

#[test]
fn excluded_rule_is_not_rewritten_when_other_rules_are_fixed() {
    let sql = "SELECT COUNT(1) FROM t WHERE a<>b";
    let disabled = vec![issue_codes::LINT_CV_001.to_string()];
    let out = apply_lint_fixes(sql, Dialect::Generic, &disabled).expect("fix result");
    assert!(
        out.sql.to_ascii_uppercase().contains("COUNT(*)"),
        "expected COUNT style fix: {}",
        out.sql
    );
    assert!(
        out.sql.contains("<>"),
        "excluded CV_005 should remain '<>' (not '!='): {}",
        out.sql
    );
    assert!(
        !out.sql.contains("!="),
        "excluded CV_005 should not be rewritten to '!=': {}",
        out.sql
    );
}

#[test]
fn references_quoting_fix_keeps_reserved_identifier_quotes() {
    let sql = "SELECT \"FROM\" FROM t UNION SELECT 2";
    let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
    assert!(
        out.sql.contains("\"FROM\""),
        "reserved identifier must remain quoted: {}",
        out.sql
    );
}

#[test]
fn references_quoting_fix_unquotes_case_insensitive_dialect() {
    // In a case-insensitive dialect (Generic), mixed-case quoted identifiers
    // are unnecessarily quoted because case doesn't matter.
    let sql = "SELECT \"CamelCase\" FROM t UNION SELECT 2";
    let out = apply_lint_fixes(
        sql,
        Dialect::Generic,
        &[issue_codes::LINT_LT_011.to_string()],
    )
    .expect("fix result");
    assert!(
        out.sql.contains("CamelCase") && !out.sql.contains("\"CamelCase\""),
        "case-insensitive dialect should unquote: {}",
        out.sql
    );
    assert!(
        out.sql.to_ascii_uppercase().contains("DISTINCT SELECT"),
        "expected another fix to persist output: {}",
        out.sql
    );
}

#[test]
fn references_quoting_fix_keeps_case_sensitive_identifier_quotes() {
    // In Postgres (lowercase casefold), mixed-case identifiers must stay
    // quoted because unquoting would fold to lowercase.
    let sql = "SELECT \"CamelCase\" FROM t UNION SELECT 2";
    let out = apply_lint_fixes(
        sql,
        Dialect::Postgres,
        &[issue_codes::LINT_LT_011.to_string()],
    )
    .expect("fix result");
    assert!(
        out.sql.contains("\"CamelCase\""),
        "case-sensitive identifier must remain quoted: {}",
        out.sql
    );
}

#[test]
fn sqlfluff_fix_rule_smoke_cases_reduce_target_violations() {
    let cases = vec![
            (
                issue_codes::LINT_AL_001,
                "SELECT * FROM a x JOIN b y ON x.id = y.id",
            ),
            (
                issue_codes::LINT_AL_005,
                "SELECT u.name FROM users u JOIN orders o ON users.id = orders.user_id",
            ),
            (issue_codes::LINT_AL_009, "SELECT a AS a FROM t"),
            (issue_codes::LINT_AM_002, "SELECT 1 UNION SELECT 2"),
            (
                issue_codes::LINT_AM_003,
                "SELECT * FROM t ORDER BY a, b DESC",
            ),
            (
                issue_codes::LINT_AM_005,
                "SELECT * FROM a JOIN b ON a.id = b.id",
            ),
            (
                issue_codes::LINT_AM_008,
                "SELECT foo.a, bar.b FROM foo INNER JOIN bar",
            ),
            (issue_codes::LINT_CP_001, "SELECT a from t"),
            (issue_codes::LINT_CP_002, "SELECT Col, col FROM t"),
            (issue_codes::LINT_CP_003, "SELECT COUNT(*), count(name) FROM t"),
            (issue_codes::LINT_CP_004, "SELECT NULL, true FROM t"),
            (
                issue_codes::LINT_CP_005,
                "CREATE TABLE t (a INT, b varchar(10))",
            ),
            (
                issue_codes::LINT_CV_001,
                "SELECT * FROM t WHERE a <> b AND c != d",
            ),
            (
                issue_codes::LINT_CV_002,
                "SELECT IFNULL(x, 'default') FROM t",
            ),
            (issue_codes::LINT_CV_003, "SELECT a, FROM t"),
            (issue_codes::LINT_CV_004, "SELECT COUNT(1) FROM t"),
            (issue_codes::LINT_CV_005, "SELECT * FROM t WHERE a = NULL"),
            (issue_codes::LINT_CV_006, "SELECT 1 ;"),
            (issue_codes::LINT_CV_007, "(SELECT 1)"),
            (
                issue_codes::LINT_CV_012,
                "SELECT a.x, b.y FROM a JOIN b WHERE a.id = b.id",
            ),
            (issue_codes::LINT_JJ_001, "SELECT '{{foo}}' AS templated"),
            (issue_codes::LINT_LT_001, "SELECT payload->>'id' FROM t"),
            (issue_codes::LINT_LT_002, "SELECT a\n   , b\nFROM t"),
            (issue_codes::LINT_LT_003, "SELECT a +\n b FROM t"),
            (issue_codes::LINT_LT_004, "SELECT a,b FROM t"),
            (issue_codes::LINT_LT_006, "SELECT COUNT (1) FROM t"),
            (
                issue_codes::LINT_LT_007,
                "WITH cte AS (\n  SELECT 1) SELECT * FROM cte",
            ),
            (issue_codes::LINT_LT_009, "SELECT a,b,c,d,e FROM t"),
            (issue_codes::LINT_LT_010, "SELECT\nDISTINCT a\nFROM t"),
            (
                issue_codes::LINT_LT_011,
                "SELECT 1 UNION SELECT 2\nUNION SELECT 3",
            ),
            (issue_codes::LINT_LT_012, "SELECT 1\nFROM t"),
            (issue_codes::LINT_LT_013, "\n\nSELECT 1"),
            (issue_codes::LINT_LT_014, "SELECT a FROM t\nWHERE a=1"),
            (issue_codes::LINT_LT_015, "SELECT 1\n\n\nFROM t"),
            (issue_codes::LINT_RF_003, "SELECT a.id, id2 FROM a"),
            (issue_codes::LINT_RF_006, "SELECT \"good_name\" FROM t"),
            (
                issue_codes::LINT_ST_001,
                "SELECT CASE WHEN x > 1 THEN 'a' ELSE NULL END FROM t",
            ),
            (
                issue_codes::LINT_ST_004,
                "SELECT CASE WHEN species = 'Rat' THEN 'Squeak' ELSE CASE WHEN species = 'Dog' THEN 'Woof' END END FROM mytable",
            ),
            (
                issue_codes::LINT_ST_002,
                "SELECT CASE WHEN x > 0 THEN true ELSE false END FROM t",
            ),
            (
                issue_codes::LINT_ST_005,
                "SELECT * FROM t JOIN (SELECT * FROM u) sub ON t.id = sub.id",
            ),
            (issue_codes::LINT_ST_006, "SELECT a + 1, a FROM t"),
            (
                issue_codes::LINT_ST_007,
                "SELECT * FROM a JOIN b USING (id)",
            ),
            (issue_codes::LINT_ST_008, "SELECT DISTINCT(a) FROM t"),
            (
                issue_codes::LINT_ST_009,
                "SELECT * FROM a x JOIN b y ON y.id = x.id",
            ),
            (issue_codes::LINT_ST_012, "SELECT 1;;"),
        ];

    for (code, sql) in cases {
        let before = lint_rule_count(sql, code);
        assert!(before > 0, "expected {code} to trigger before fix: {sql}");
        let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix result");
        assert!(
            !out.skipped_due_to_comments,
            "test SQL should not be skipped: {sql}"
        );
        let after = lint_rule_count(&out.sql, code);
        assert!(
                after < before || out.sql != sql,
                "expected {code} count to decrease or SQL to be rewritten. before={before} after={after}\ninput={sql}\noutput={}",
                out.sql
            );
    }
}

// --- CV_012: implicit WHERE join → explicit ON ---

#[test]
fn cv012_simple_where_join_to_on() {
    let sql = "SELECT a.x, b.y FROM a JOIN b WHERE a.id = b.id";
    let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix");
    let lower = out.sql.to_ascii_lowercase();
    assert!(
        lower.contains(" on ") && !lower.contains("where"),
        "expected JOIN ON without WHERE: {}",
        out.sql
    );
}

#[test]
fn cv012_mixed_where_keeps_non_join_predicates() {
    let sql = "SELECT a.x FROM a JOIN b WHERE a.id = b.id AND a.val > 10";
    let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix");
    let lower = out.sql.to_ascii_lowercase();
    assert!(lower.contains(" on "), "expected JOIN ON: {}", out.sql);
    assert!(
        lower.contains("where"),
        "expected remaining WHERE: {}",
        out.sql
    );
}

#[test]
fn cv012_multi_join_chain() {
    let sql = "SELECT * FROM a JOIN b JOIN c WHERE a.id = b.id AND b.id = c.id";
    let out = apply_lint_fixes(
        sql,
        Dialect::Generic,
        &[issue_codes::LINT_AM_005.to_string()],
    )
    .expect("fix");
    let lower = out.sql.to_ascii_lowercase();
    // Both joins should get ON clauses.
    let on_count = lower.matches(" on ").count();
    assert!(on_count >= 2, "expected at least 2 ON clauses: {}", out.sql);
    assert!(
        !lower.contains("where"),
        "all predicates should be extracted: {}",
        out.sql
    );
}

#[test]
fn cv012_preserves_explicit_on() {
    let sql = "SELECT * FROM a JOIN b ON a.id = b.id";
    let out = apply_lint_fixes(sql, Dialect::Generic, &[]).expect("fix");
    assert_eq!(
        lint_rule_count(sql, issue_codes::LINT_CV_012),
        0,
        "explicit ON should not trigger CV_012"
    );
    let lower = out.sql.to_ascii_lowercase();
    assert!(
        lower.contains("on a.id = b.id"),
        "ON clause should be preserved: {}",
        out.sql
    );
}

#[test]
fn cv012_idempotent() {
    let sql = "SELECT a.x, b.y FROM a JOIN b WHERE a.id = b.id";
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: vec![issue_codes::LINT_LT_014.to_string()],
        rule_configs: std::collections::BTreeMap::new(),
    };
    let out1 = apply_lint_fixes_with_options(
        sql,
        Dialect::Generic,
        &lint_config,
        FixOptions {
            include_unsafe_fixes: true,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix");
    let out2 = apply_lint_fixes_with_options(
        &out1.sql,
        Dialect::Generic,
        &lint_config,
        FixOptions {
            include_unsafe_fixes: true,
            include_rewrite_candidates: false,
        },
    )
    .expect("fix2");
    assert_eq!(
        out1.sql.trim_end(),
        out2.sql.trim_end(),
        "second pass should be idempotent aside from trailing-whitespace normalization"
    );
}
