//! SQL lint auto-fix helpers.
//!
//! Fixing is best-effort and deterministic. We combine:
//! - AST rewrites for structurally safe transforms.
//! - Text rewrites for parity-style formatting/convention rules.
//! - Lint before/after comparison to report per-rule removed violations.

use crate::fix_engine::{
    apply_edits as apply_patch_edits, derive_protected_ranges, plan_fixes, BlockedReason,
    Edit as PatchEdit, Fix as PatchFix, FixApplicability as PatchApplicability,
    ProtectedRange as PatchProtectedRange, ProtectedRangeKind as PatchProtectedRangeKind,
};
use flowscope_core::linter::config::canonicalize_rule_code;
use flowscope_core::{
    analyze, issue_codes, parse_sql_with_dialect, AnalysisOptions, AnalyzeRequest, Dialect, Issue,
    IssueAutofixApplicability, LintConfig, ParseError,
};
#[cfg(feature = "templating")]
use flowscope_core::{TemplateConfig, TemplateMode};
use sqlparser::ast::helpers::attached_token::AttachedToken;
use sqlparser::ast::*;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

mod ast_rewrites;
mod candidate_planning;

use ast_rewrites::*;
use candidate_planning::*;

/// Compute a 64-bit hash of a string for cheap cycle detection.
fn hash_sql(sql: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    sql.hash(&mut h);
    h.finish()
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[must_use]
pub struct FixCounts {
    /// Per-rule fix counts, ordered by rule code for deterministic output.
    by_rule: BTreeMap<String, usize>,
}

impl FixCounts {
    pub fn total(&self) -> usize {
        self.by_rule.values().sum()
    }

    pub fn add(&mut self, code: &str, count: usize) {
        if count == 0 {
            return;
        }
        *self.by_rule.entry(code.to_string()).or_insert(0) += count;
    }

    pub fn get(&self, code: &str) -> usize {
        self.by_rule.get(code).copied().unwrap_or(0)
    }

    pub fn merge(&mut self, other: &Self) {
        for (code, count) in &other.by_rule {
            self.add(code, *count);
        }
    }

    fn from_removed(before: &BTreeMap<String, usize>, after: &BTreeMap<String, usize>) -> Self {
        let mut out = Self::default();
        for (code, before_count) in before {
            let after_count = after.get(code).copied().unwrap_or(0);
            if *before_count > after_count {
                out.add(code, before_count - after_count);
            }
        }
        out
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub struct FixOutcome {
    pub sql: String,
    pub counts: FixCounts,
    pub changed: bool,
    pub skipped_due_to_comments: bool,
    pub skipped_due_to_regression: bool,
    pub skipped_counts: FixSkippedCounts,
}

#[derive(Debug, Clone)]
#[must_use]
pub struct FixLintState {
    issues: Vec<Issue>,
    counts: BTreeMap<String, usize>,
}

impl FixLintState {
    fn from_issues(issues: Vec<Issue>) -> Self {
        let counts = lint_rule_counts_from_issues(&issues);
        Self { issues, counts }
    }

    pub fn counts(&self) -> &BTreeMap<String, usize> {
        &self.counts
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub struct FixPassResult {
    pub outcome: FixOutcome,
    pub post_lint_state: FixLintState,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[must_use]
pub struct FixSkippedCounts {
    pub unsafe_skipped: usize,
    pub protected_range_blocked: usize,
    pub overlap_conflict_blocked: usize,
    pub display_only: usize,
}

#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct FixOptions {
    pub include_unsafe_fixes: bool,
    pub include_rewrite_candidates: bool,
}

impl Default for FixOptions {
    fn default() -> Self {
        Self {
            include_unsafe_fixes: false,
            include_rewrite_candidates: true,
        }
    }
}

/// Max bounded fix passes per file during lint fix execution.
///
/// Three passes capture the vast majority of cascading fixes while avoiding
/// disproportionate long-tail runtime on large statements.
const MAX_LINT_FIX_PASSES: usize = 3;
/// Extra cleanup passes granted at the end of the normal loop budget when
/// progress is still being made.
const MAX_LINT_FIX_BONUS_PASSES: usize = 1;
/// Allow one additional large-SQL cleanup pass when LT02 has been improving.
///
/// This narrowly recovers the last indentation edge cases without reopening
/// the broad long-tail cost of unrestricted bonus passes.
const MAX_LINT_FIX_LARGE_SQL_LT02_EXTRA_PASSES: usize = 1;
/// SQL byte length above which an extra LT02 cleanup pass is considered.
/// 10 KB targets the large statements where LT02 cascading indentation fixes
/// benefit most from one more pass.
const LINT_FIX_LARGE_SQL_LT02_EXTRA_PASS_THRESHOLD: usize = 10_000;
/// Stop extra cleanup passes on large SQL when LT02/LT03 are no longer moving
/// and residual violations are overwhelmingly known mostly-unfixable classes.
const LINT_FIX_MOSTLY_UNFIXABLE_STOP_THRESHOLD: usize = 4_000;
/// If at most this many residual violations belong to fixable rules, the
/// statement is considered "mostly unfixable" and extra passes are skipped.
const MAX_RESIDUAL_POTENTIALLY_FIXABLE_FOR_STOP: usize = 2;
/// Denominator for the mostly-unfixable ratio check: fixable residuals must be
/// at most 1/N of total remaining violations (20% when N=5).
const MOSTLY_UNFIXABLE_RATIO_DENOMINATOR: usize = 5;
/// Hard wall-clock timeout for the entire multi-pass fix loop per file.
/// This is a safety net for pathological inputs; typical files finish in
/// well under a second.
const FIX_LOOP_TIMEOUT: Duration = Duration::from_secs(30);

/// Runtime configuration for the multi-pass lint-fix execution.
///
/// Maps CLI/API flags to the internal fix engine options.
#[derive(Debug, Clone, Copy, Default)]
pub struct LintFixRuntimeOptions {
    pub include_unsafe_fixes: bool,
    pub legacy_ast_fixes: bool,
}

/// Aggregate statistics about fix candidates that were skipped or blocked
/// across all passes of a multi-pass lint-fix execution.
#[derive(Debug, Clone, Copy, Default)]
pub struct FixCandidateStats {
    pub skipped: usize,
    /// Total blocked candidates (sum of all `blocked_*` fields).
    pub blocked: usize,
    pub blocked_unsafe: usize,
    pub blocked_display_only: usize,
    pub blocked_protected_range: usize,
    pub blocked_overlap_conflict: usize,
}

impl FixCandidateStats {
    pub fn total_skipped_or_blocked(self) -> usize {
        self.skipped + self.blocked
    }

    pub fn merge(&mut self, other: Self) {
        self.skipped += other.skipped;
        self.blocked += other.blocked;
        self.blocked_unsafe += other.blocked_unsafe;
        self.blocked_display_only += other.blocked_display_only;
        self.blocked_protected_range += other.blocked_protected_range;
        self.blocked_overlap_conflict += other.blocked_overlap_conflict;
    }
}

/// Result of a multi-pass lint-fix execution.
///
/// Combines the final [`FixOutcome`] with aggregate [`FixCandidateStats`]
/// collected across all passes.
#[derive(Debug, Clone)]
pub struct LintFixExecution {
    pub outcome: FixOutcome,
    pub candidate_stats: FixCandidateStats,
}

#[derive(Debug, Clone, Default)]
struct RuleFilter {
    disabled: HashSet<String>,
    st005_forbid_subquery_in: St005ForbidSubqueryIn,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
enum St005ForbidSubqueryIn {
    Both,
    #[default]
    Join,
    From,
}

impl St005ForbidSubqueryIn {
    fn forbid_from(self) -> bool {
        matches!(self, Self::Both | Self::From)
    }

    fn forbid_join(self) -> bool {
        matches!(self, Self::Both | Self::Join)
    }
}

impl RuleFilter {
    fn from_lint_config(lint_config: &LintConfig) -> Self {
        let disabled: HashSet<String> = lint_config
            .disabled_rules
            .iter()
            .filter_map(|rule| {
                let trimmed = rule.trim();
                if trimmed.is_empty() {
                    return None;
                }
                Some(
                    canonicalize_rule_code(trimmed).unwrap_or_else(|| trimmed.to_ascii_uppercase()),
                )
            })
            .collect();
        let st005_forbid_subquery_in = match lint_config
            .rule_option_str(issue_codes::LINT_ST_005, "forbid_subquery_in")
            .unwrap_or("join")
            .to_ascii_lowercase()
            .as_str()
        {
            "from" => St005ForbidSubqueryIn::From,
            "both" => St005ForbidSubqueryIn::Both,
            _ => St005ForbidSubqueryIn::Join,
        };
        Self {
            disabled,
            st005_forbid_subquery_in,
        }
    }

    fn allows(&self, code: &str) -> bool {
        let canonical =
            canonicalize_rule_code(code).unwrap_or_else(|| code.trim().to_ascii_uppercase());
        !self.disabled.contains(&canonical)
    }

    fn with_rule_disabled(&self, code: &str) -> Self {
        let mut updated = self.clone();
        let canonical =
            canonicalize_rule_code(code).unwrap_or_else(|| code.trim().to_ascii_uppercase());
        updated.disabled.insert(canonical);
        updated
    }
}

struct FixProfileGuard {
    enabled: bool,
    started_at: Instant,
    sql_len: usize,
    include_rewrite_candidates: bool,
    include_unsafe_fixes: bool,
    marks: Vec<(&'static str, Duration)>,
}

impl FixProfileGuard {
    fn new(sql_len: usize, fix_options: FixOptions) -> Self {
        Self {
            enabled: std::env::var_os("FLOWSCOPE_FIX_PROFILE").is_some(),
            started_at: Instant::now(),
            sql_len,
            include_rewrite_candidates: fix_options.include_rewrite_candidates,
            include_unsafe_fixes: fix_options.include_unsafe_fixes,
            marks: Vec::new(),
        }
    }

    fn record(&mut self, label: &'static str, started: Instant) {
        if self.enabled {
            self.marks.push((label, started.elapsed()));
        }
    }
}

impl Drop for FixProfileGuard {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }

        let total = self.started_at.elapsed();
        let mut summary = format!(
            "flowscope: fix-profile sql_len={} rewrite={} unsafe={} total={:.2}ms",
            self.sql_len,
            self.include_rewrite_candidates,
            self.include_unsafe_fixes,
            total.as_secs_f64() * 1000.0
        );
        for (label, duration) in &self.marks {
            summary.push_str(&format!(
                " | {label}={:.2}ms",
                duration.as_secs_f64() * 1000.0
            ));
        }
        eprintln!("{summary}");
    }
}

/// Apply deterministic lint fixes to a SQL document.
///
/// Notes:
/// - Fixes are planned as localized patches and applied only when non-overlapping.
/// - Parse errors are returned so callers can decide whether to continue linting.
pub fn apply_lint_fixes(
    sql: &str,
    dialect: Dialect,
    disabled_rules: &[String],
) -> Result<FixOutcome, ParseError> {
    apply_lint_fixes_with_lint_config(
        sql,
        dialect,
        &LintConfig {
            enabled: true,
            disabled_rules: disabled_rules.to_vec(),
            rule_configs: BTreeMap::new(),
        },
    )
}

pub fn apply_lint_fixes_with_lint_config(
    sql: &str,
    dialect: Dialect,
    lint_config: &LintConfig,
) -> Result<FixOutcome, ParseError> {
    apply_lint_fixes_with_options(
        sql,
        dialect,
        lint_config,
        FixOptions {
            // Preserve existing behavior for direct/internal callers.
            include_unsafe_fixes: true,
            include_rewrite_candidates: true,
        },
    )
}

pub fn apply_lint_fixes_with_options_and_lint_state(
    sql: &str,
    dialect: Dialect,
    lint_config: &LintConfig,
    fix_options: FixOptions,
    before_lint_state: Option<FixLintState>,
) -> Result<FixPassResult, ParseError> {
    apply_lint_fixes_with_options_internal(
        sql,
        dialect,
        lint_config,
        fix_options,
        before_lint_state,
    )
}

pub fn apply_lint_fixes_with_options(
    sql: &str,
    dialect: Dialect,
    lint_config: &LintConfig,
    fix_options: FixOptions,
) -> Result<FixOutcome, ParseError> {
    Ok(
        apply_lint_fixes_with_options_internal(sql, dialect, lint_config, fix_options, None)?
            .outcome,
    )
}

/// Apply lint fixes using a bounded multi-pass loop with cascading fallback.
///
/// Each pass applies safe fixes and re-lints to capture cascading improvements.
/// The loop terminates when no further progress is made, the pass budget is
/// exhausted, or a hard wall-clock timeout is reached.
pub fn apply_lint_fixes_with_runtime_options(
    sql: &str,
    dialect: Dialect,
    lint_config: &LintConfig,
    runtime_options: LintFixRuntimeOptions,
) -> Result<LintFixExecution, ParseError> {
    let fix_options = FixOptions {
        include_unsafe_fixes: runtime_options.include_unsafe_fixes,
        include_rewrite_candidates: runtime_options.legacy_ast_fixes,
    };

    let mut current_sql = sql.to_string();
    let mut merged_counts = FixCounts::default();
    let mut merged_candidate_stats = FixCandidateStats::default();
    let mut any_changed = false;
    let mut lt03_touched = false;
    let mut lt02_touched = false;
    let mut last_outcome = None;
    let mut cached_lint_state: Option<FixLintState> = None;
    let mut seen_sql: HashSet<u64> = HashSet::from([hash_sql(&current_sql)]);
    let mut overlap_retried_sql: HashSet<u64> = HashSet::new();
    let mut pass_limit = MAX_LINT_FIX_PASSES;
    let mut bonus_passes_granted = 0usize;
    let mut large_sql_lt02_extra_passes_granted = 0usize;
    let mut pass_index = 0usize;
    let fix_started_at = Instant::now();

    while pass_index < pass_limit {
        if pass_index > 0 && fix_started_at.elapsed() >= FIX_LOOP_TIMEOUT {
            break;
        }
        let pass_result = apply_lint_fixes_with_options_and_lint_state(
            &current_sql,
            dialect,
            lint_config,
            fix_options,
            cached_lint_state.take(),
        )?;
        let outcome = pass_result.outcome;
        let post_lint_state = pass_result.post_lint_state;

        // Avoid oscillating between previously seen SQL states across passes.
        if outcome.changed && !seen_sql.insert(hash_sql(&outcome.sql)) {
            break;
        }

        merged_counts.merge(&outcome.counts);
        merged_candidate_stats.merge(collect_fix_candidate_stats(&outcome, runtime_options));
        if outcome.counts.get(issue_codes::LINT_LT_003) > 0 {
            lt03_touched = true;
        }
        if outcome.counts.get(issue_codes::LINT_LT_002) > 0 {
            lt02_touched = true;
        }
        let lt_cleanup_progress = outcome.counts.get(issue_codes::LINT_LT_003) > 0
            || outcome.counts.get(issue_codes::LINT_LT_002) > 0;
        let lt02_remaining = post_lint_state
            .counts()
            .get(issue_codes::LINT_LT_002)
            .copied()
            .unwrap_or(0)
            > 0;
        let residual_is_mostly_unfixable = is_mostly_unfixable_residual(post_lint_state.counts());

        if outcome.changed {
            any_changed = true;
            current_sql = outcome.sql.clone();
        }
        cached_lint_state = Some(post_lint_state);

        let mut continue_fixing = outcome.changed
            && !outcome.skipped_due_to_comments
            && !outcome.skipped_due_to_regression;
        if continue_fixing
            && should_stop_large_mostly_unfixable(
                pass_index,
                current_sql.len(),
                lt02_touched,
                lt03_touched,
                lt_cleanup_progress,
                residual_is_mostly_unfixable,
            )
        {
            continue_fixing = false;
        }
        // Only retry overlap conflicts once per unique SQL state: re-running on
        // unchanged SQL would produce the same conflicts and waste the pass budget.
        let overlap_retry = !outcome.changed
            && !outcome.skipped_due_to_comments
            && !outcome.skipped_due_to_regression
            && outcome.skipped_counts.overlap_conflict_blocked > 0
            && overlap_retried_sql.insert(hash_sql(&current_sql));

        // Some files keep improving right at the bounded pass budget. Allow a
        // small number of extra cleanup passes to avoid near-miss leftovers.
        if (continue_fixing || overlap_retry)
            && pass_index + 1 == pass_limit
            && bonus_passes_granted < MAX_LINT_FIX_BONUS_PASSES
            && (overlap_retry || lt03_touched || lt02_touched)
        {
            pass_limit += 1;
            bonus_passes_granted += 1;
        }

        if continue_fixing
            && pass_index + 1 == pass_limit
            && bonus_passes_granted >= MAX_LINT_FIX_BONUS_PASSES
            && large_sql_lt02_extra_passes_granted < MAX_LINT_FIX_LARGE_SQL_LT02_EXTRA_PASSES
            && current_sql.len() >= LINT_FIX_LARGE_SQL_LT02_EXTRA_PASS_THRESHOLD
            && lt02_remaining
        {
            pass_limit += 1;
            large_sql_lt02_extra_passes_granted += 1;
        }

        last_outcome = Some(outcome);

        if !continue_fixing && !overlap_retry {
            break;
        }

        pass_index += 1;
    }

    let mut outcome = last_outcome.expect("at least one fix pass should run");
    if any_changed {
        outcome.sql = current_sql;
        outcome.changed = true;
        outcome.counts = merged_counts;
        // Multi-pass terminated after no further changes or bounded pass limit.
        outcome.skipped_due_to_comments = false;
        outcome.skipped_due_to_regression = false;
    }

    Ok(LintFixExecution {
        outcome,
        candidate_stats: merged_candidate_stats,
    })
}

fn collect_fix_candidate_stats(
    outcome: &FixOutcome,
    runtime_options: LintFixRuntimeOptions,
) -> FixCandidateStats {
    let blocked_unsafe = if runtime_options.include_unsafe_fixes {
        0
    } else {
        outcome.skipped_counts.unsafe_skipped
    };
    let blocked_display_only = outcome.skipped_counts.display_only;
    let blocked_protected_range = outcome.skipped_counts.protected_range_blocked;
    let blocked_overlap_conflict = outcome.skipped_counts.overlap_conflict_blocked;
    let blocked =
        blocked_unsafe + blocked_display_only + blocked_protected_range + blocked_overlap_conflict;

    let stats = FixCandidateStats {
        skipped: 0,
        blocked,
        blocked_unsafe,
        blocked_display_only,
        blocked_protected_range,
        blocked_overlap_conflict,
    };
    debug_assert_eq!(
        stats.blocked,
        stats.blocked_unsafe
            + stats.blocked_display_only
            + stats.blocked_protected_range
            + stats.blocked_overlap_conflict,
        "blocked total must equal sum of blocked_* components"
    );
    stats
}

fn is_mostly_unfixable_rule(code: &str) -> bool {
    matches!(
        code,
        issue_codes::LINT_AL_003
            | issue_codes::LINT_RF_002
            | issue_codes::LINT_RF_004
            | issue_codes::LINT_LT_005
    )
}

fn is_mostly_unfixable_residual(after_counts: &BTreeMap<String, usize>) -> bool {
    let mut residual_total = 0usize;
    let mut potentially_fixable = 0usize;

    for (code, count) in after_counts {
        if *count == 0 || code == issue_codes::PARSE_ERROR {
            continue;
        }
        residual_total += *count;
        if !is_mostly_unfixable_rule(code) {
            potentially_fixable += *count;
            if potentially_fixable > MAX_RESIDUAL_POTENTIALLY_FIXABLE_FOR_STOP {
                return false;
            }
        }
    }

    if residual_total == 0 {
        return false;
    }

    potentially_fixable * MOSTLY_UNFIXABLE_RATIO_DENOMINATOR <= residual_total
}

/// Whether to stop granting extra cleanup passes on large SQL whose residual
/// violations are overwhelmingly in known mostly-unfixable rule classes
/// (AL003, RF002, RF004, LT005) and indentation rules have stopped moving.
fn should_stop_large_mostly_unfixable(
    pass_index: usize,
    sql_len: usize,
    lt02_touched: bool,
    lt03_touched: bool,
    lt_cleanup_progress: bool,
    residual_is_mostly_unfixable: bool,
) -> bool {
    pass_index + 1 >= MAX_LINT_FIX_PASSES
        && sql_len >= LINT_FIX_MOSTLY_UNFIXABLE_STOP_THRESHOLD
        && !lt02_touched
        && !lt03_touched
        && !lt_cleanup_progress
        && residual_is_mostly_unfixable
}

fn apply_lint_fixes_with_options_internal(
    sql: &str,
    dialect: Dialect,
    lint_config: &LintConfig,
    fix_options: FixOptions,
    before_lint_state: Option<FixLintState>,
) -> Result<FixPassResult, ParseError> {
    let mut profile = FixProfileGuard::new(sql.len(), fix_options);
    // Statements above this byte length use tighter iteration budgets to avoid
    // disproportionate runtime on large/complex SQL.  4 KB covers ~95% of
    // real-world statements while keeping the expensive tail bounded.
    const INCREMENTAL_LARGE_SQL_THRESHOLD: usize = 4_000;

    // Parse-error recovery: try a handful of single-rule passes to find a fix
    // set that does not introduce new parse errors.  Large SQL gets a single
    // pass with a single rule evaluation to keep cost O(rules) not O(rules²).
    const INCREMENTAL_MAX_ITERATIONS_PARSE_ERROR: usize = 4;
    const INCREMENTAL_MAX_ITERATIONS_PARSE_ERROR_LARGE_SQL: usize = 1;
    const INCREMENTAL_MAX_RULE_EVALUATIONS_PARSE_ERROR_LARGE_SQL: usize = 1;

    // Default incremental plan: up to 24 iterations lets most multi-rule
    // cascades converge.  Large SQL halves this to keep wall-clock bounded.
    const INCREMENTAL_MAX_ITERATIONS_DEFAULT: usize = 24;
    const INCREMENTAL_MAX_ITERATIONS_DEFAULT_LARGE_SQL: usize = 12;

    // Overlap recovery: retry conflicting edits one rule at a time.  8
    // iterations suffice for typical conflict counts (capped separately by
    // MAX_OVERLAP_CONFLICTS_FOR_INCREMENTAL_RECOVERY).  Large SQL is
    // restricted to a single pass with a single rule to avoid quadratic cost.
    const INCREMENTAL_MAX_ITERATIONS_OVERLAP_RECOVERY: usize = 8;
    const INCREMENTAL_MAX_ITERATIONS_OVERLAP_RECOVERY_LARGE_SQL: usize = 1;
    const INCREMENTAL_MAX_RULE_EVALUATIONS_OVERLAP_RECOVERY_LARGE_SQL: usize = 1;
    let is_large_sql = sql.len() >= INCREMENTAL_LARGE_SQL_THRESHOLD;
    let incremental_parse_error_iterations = if is_large_sql {
        INCREMENTAL_MAX_ITERATIONS_PARSE_ERROR_LARGE_SQL
    } else {
        INCREMENTAL_MAX_ITERATIONS_PARSE_ERROR
    };
    let incremental_default_iterations = if is_large_sql {
        INCREMENTAL_MAX_ITERATIONS_DEFAULT_LARGE_SQL
    } else {
        INCREMENTAL_MAX_ITERATIONS_DEFAULT
    };
    let incremental_parse_error_rule_evaluations = if is_large_sql {
        INCREMENTAL_MAX_RULE_EVALUATIONS_PARSE_ERROR_LARGE_SQL
    } else {
        usize::MAX
    };
    let incremental_overlap_recovery_iterations = if is_large_sql {
        INCREMENTAL_MAX_ITERATIONS_OVERLAP_RECOVERY_LARGE_SQL
    } else {
        INCREMENTAL_MAX_ITERATIONS_OVERLAP_RECOVERY
    };
    let incremental_overlap_recovery_rule_evaluations = if is_large_sql {
        INCREMENTAL_MAX_RULE_EVALUATIONS_OVERLAP_RECOVERY_LARGE_SQL
    } else {
        usize::MAX
    };
    let rule_filter = RuleFilter::from_lint_config(lint_config);

    let (before_issues, before_counts) = if let Some(state) = before_lint_state {
        let stage_started = Instant::now();
        profile.record("lint_state_cached", stage_started);
        (state.issues, state.counts)
    } else {
        let stage_started = Instant::now();
        let before_issues = lint_issues(sql, dialect, lint_config);
        profile.record("lint_issues_before", stage_started);

        let stage_started = Instant::now();
        let before_counts = lint_rule_counts_from_issues(&before_issues);
        profile.record("before_counts", stage_started);
        (before_issues, before_counts)
    };

    let stage_started = Instant::now();
    let mut core_candidates = build_fix_candidates_from_issue_autofixes(sql, &before_issues);
    core_candidates.extend(build_al001_fallback_candidates(
        sql,
        dialect,
        &before_issues,
        lint_config,
    ));
    profile.record("core_candidates", stage_started);

    let stage_started = Instant::now();
    let core_autofix_rules =
        collect_core_autofix_rules(&before_issues, fix_options.include_unsafe_fixes);
    profile.record("core_autofix_rules", stage_started);
    let mut candidates = Vec::new();

    if fix_options.include_rewrite_candidates {
        let rewrite_stage_started = Instant::now();
        let safe_rule_filter = if fix_options.include_unsafe_fixes {
            rule_filter.clone()
        } else {
            // Structural subquery-to-CTE rewrites are useful but higher risk and
            // therefore opt-in under `--unsafe-fixes`.
            rule_filter.with_rule_disabled(issue_codes::LINT_ST_005)
        };

        let mut statements = parse_sql_with_dialect(sql, dialect)?;
        for stmt in &mut statements {
            fix_statement(stmt, &safe_rule_filter);
        }

        let rewritten_sql = render_statements(&statements, sql);
        let rewritten_sql = if safe_rule_filter.allows(issue_codes::LINT_AL_001) {
            apply_configured_table_alias_style(&rewritten_sql, dialect, lint_config)
        } else {
            preserve_original_table_alias_style(sql, &rewritten_sql, dialect)
        };

        let mut rewrite_candidates = build_fix_candidates_from_rewrite(
            sql,
            &rewritten_sql,
            FixCandidateApplicability::Safe,
            FixCandidateSource::PrimaryRewrite,
        );
        if !fix_options.include_unsafe_fixes {
            let mut unsafe_statements = parse_sql_with_dialect(sql, dialect)?;
            for stmt in &mut unsafe_statements {
                fix_statement(stmt, &rule_filter);
            }
            let unsafe_sql = render_statements(&unsafe_statements, sql);
            let unsafe_sql = if rule_filter.allows(issue_codes::LINT_AL_001) {
                apply_configured_table_alias_style(&unsafe_sql, dialect, lint_config)
            } else {
                preserve_original_table_alias_style(sql, &unsafe_sql, dialect)
            };
            if unsafe_sql != rewritten_sql {
                rewrite_candidates.extend(build_fix_candidates_from_rewrite(
                    sql,
                    &unsafe_sql,
                    FixCandidateApplicability::Unsafe,
                    FixCandidateSource::UnsafeFallback,
                ));
            }
        }

        candidates.extend(rewrite_candidates);
        profile.record("rewrite_candidates", rewrite_stage_started);
    }

    candidates.extend(core_candidates.iter().cloned());

    let stage_started = Instant::now();
    let protected_ranges =
        collect_comment_protected_ranges(sql, dialect, !fix_options.include_unsafe_fixes);
    profile.record("protected_ranges", stage_started);

    let stage_started = Instant::now();
    let planned = plan_fix_candidates(
        sql,
        candidates,
        &protected_ranges,
        fix_options.include_unsafe_fixes,
    );
    profile.record("plan_fix_candidates", stage_started);

    let stage_started = Instant::now();
    let mut fixed_sql = apply_planned_edits(sql, &planned.edits);
    profile.record("apply_planned_edits", stage_started);

    let stage_started = Instant::now();
    let mut after_lint_state = if fixed_sql == sql {
        FixLintState {
            issues: before_issues.clone(),
            counts: before_counts.clone(),
        }
    } else {
        lint_state(&fixed_sql, dialect, lint_config)
    };
    let mut after_counts = after_lint_state.counts.clone();
    profile.record("after_counts", stage_started);

    let before_total = regression_guard_total(&before_counts);
    let after_total = regression_guard_total(&after_counts);
    let mut skipped_counts = planned.skipped.clone();

    if parse_errors_increased(&before_counts, &after_counts) {
        if let Some(result) = try_fallback_fix(
            sql,
            dialect,
            lint_config,
            &before_counts,
            &before_issues,
            &core_candidates,
            &protected_ranges,
            fix_options.include_unsafe_fixes,
            incremental_parse_error_iterations,
            incremental_parse_error_rule_evaluations,
            &mut profile,
            "fallback_core_only_parse_errors",
            "fallback_incremental_parse_errors",
        ) {
            return Ok(result);
        }

        return Ok(FixPassResult {
            post_lint_state: FixLintState {
                issues: before_issues.clone(),
                counts: before_counts.clone(),
            },
            outcome: FixOutcome {
                sql: sql.to_string(),
                counts: FixCounts::default(),
                changed: false,
                skipped_due_to_comments: false,
                skipped_due_to_regression: true,
                skipped_counts,
            },
        });
    }

    if fix_options.include_rewrite_candidates
        && core_autofix_rules_not_improved(&before_counts, &after_counts, &core_autofix_rules)
    {
        if let Some(result) = try_fallback_fix(
            sql,
            dialect,
            lint_config,
            &before_counts,
            &before_issues,
            &core_candidates,
            &protected_ranges,
            fix_options.include_unsafe_fixes,
            incremental_default_iterations,
            usize::MAX,
            &mut profile,
            "fallback_core_only_rewrite_guard",
            "fallback_incremental_rewrite_guard",
        ) {
            return Ok(result);
        }
    }

    // Strict regression guard: never apply a fix set that increases total
    // violations, and also retry with core-only planning when net totals are
    // flat but per-rule regressions mask improvements.
    let masked_or_worse = after_total > before_total
        || (after_total == before_total
            && after_counts != before_counts
            && core_autofix_rules_not_improved(&before_counts, &after_counts, &core_autofix_rules));
    if masked_or_worse {
        if let Some(result) = try_fallback_fix(
            sql,
            dialect,
            lint_config,
            &before_counts,
            &before_issues,
            &core_candidates,
            &protected_ranges,
            fix_options.include_unsafe_fixes,
            incremental_default_iterations,
            usize::MAX,
            &mut profile,
            "fallback_core_only_masked_or_worse",
            "fallback_incremental_masked_or_worse",
        ) {
            return Ok(result);
        }

        return Ok(FixPassResult {
            post_lint_state: FixLintState {
                issues: before_issues.clone(),
                counts: before_counts.clone(),
            },
            outcome: FixOutcome {
                sql: sql.to_string(),
                counts: FixCounts::default(),
                changed: false,
                skipped_due_to_comments: false,
                skipped_due_to_regression: true,
                skipped_counts,
            },
        });
    }

    // Incremental overlap recovery can be very expensive on large/highly
    // conflicted statements. Cap this path to low-conflict cases where the
    // extra per-rule search is most likely to pay off.
    const MAX_OVERLAP_CONFLICTS_FOR_INCREMENTAL_RECOVERY: usize = 8;
    const MAX_OVERLAP_CONFLICTS_FOR_INCREMENTAL_RECOVERY_LARGE_SQL: usize = 8;
    let overlap_recovery_conflict_limit = if is_large_sql {
        MAX_OVERLAP_CONFLICTS_FOR_INCREMENTAL_RECOVERY_LARGE_SQL
    } else {
        MAX_OVERLAP_CONFLICTS_FOR_INCREMENTAL_RECOVERY
    };
    if !fix_options.include_rewrite_candidates
        && skipped_counts.overlap_conflict_blocked > 0
        && skipped_counts.overlap_conflict_blocked <= overlap_recovery_conflict_limit
    {
        let stage_started = Instant::now();
        if let Some(incremental) = try_incremental_core_fix_plan(
            &fixed_sql,
            dialect,
            lint_config,
            &after_counts,
            Some(after_lint_state.issues.as_slice()),
            fix_options.include_unsafe_fixes,
            incremental_overlap_recovery_iterations,
            incremental_overlap_recovery_rule_evaluations,
        ) {
            profile.record("incremental_overlap_recovery", stage_started);
            merge_skipped_counts(&mut skipped_counts, &incremental.skipped_counts);
            fixed_sql = incremental.sql;
            let recount_started = Instant::now();
            after_lint_state = lint_state(&fixed_sql, dialect, lint_config);
            after_counts = after_lint_state.counts.clone();
            profile.record("after_counts_overlap_recovery", recount_started);
        } else {
            profile.record("incremental_overlap_recovery", stage_started);
        }
    }

    let stage_started = Instant::now();
    let counts = FixCounts::from_removed(&before_counts, &after_counts);
    profile.record("final_removed_counts", stage_started);

    if counts.total() == 0 {
        return Ok(FixPassResult {
            post_lint_state: FixLintState {
                issues: before_issues.clone(),
                counts: before_counts.clone(),
            },
            outcome: FixOutcome {
                sql: sql.to_string(),
                counts,
                changed: false,
                skipped_due_to_comments: false,
                skipped_due_to_regression: false,
                skipped_counts,
            },
        });
    }
    let changed = fixed_sql != sql;

    Ok(FixPassResult {
        post_lint_state: after_lint_state,
        outcome: FixOutcome {
            sql: fixed_sql,
            counts,
            changed,
            skipped_due_to_comments: false,
            skipped_due_to_regression: false,
            skipped_counts,
        },
    })
}

/// Check whether SQL contains comment markers outside of quoted regions.
#[cfg(test)]
fn contains_comment_markers(sql: &str, dialect: Dialect) -> bool {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum ScanMode {
        Outside,
        SingleQuote,
        DoubleQuote,
        BacktickQuote,
        BracketQuote,
    }

    let bytes = sql.as_bytes();
    let mut mode = ScanMode::Outside;
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        let next = bytes.get(i + 1).copied();

        match mode {
            ScanMode::Outside => {
                if b == b'\'' {
                    mode = ScanMode::SingleQuote;
                    i += 1;
                    continue;
                }
                if b == b'"' {
                    mode = ScanMode::DoubleQuote;
                    i += 1;
                    continue;
                }
                if b == b'`' {
                    mode = ScanMode::BacktickQuote;
                    i += 1;
                    continue;
                }
                if b == b'[' {
                    mode = ScanMode::BracketQuote;
                    i += 1;
                    continue;
                }

                if b == b'-' && next == Some(b'-') {
                    return true;
                }
                if b == b'/' && next == Some(b'*') {
                    return true;
                }
                if matches!(dialect, Dialect::Mysql) && b == b'#' {
                    return true;
                }

                i += 1;
            }
            ScanMode::SingleQuote => {
                if b == b'\'' && next == Some(b'\'') {
                    i += 2;
                } else if b == b'\'' {
                    mode = ScanMode::Outside;
                    i += 1;
                } else {
                    i += 1;
                }
            }
            ScanMode::DoubleQuote => {
                if b == b'"' && next == Some(b'"') {
                    i += 2;
                } else if b == b'"' {
                    mode = ScanMode::Outside;
                    i += 1;
                } else {
                    i += 1;
                }
            }
            ScanMode::BacktickQuote => {
                if b == b'`' && next == Some(b'`') {
                    i += 2;
                } else if b == b'`' {
                    mode = ScanMode::Outside;
                    i += 1;
                } else {
                    i += 1;
                }
            }
            ScanMode::BracketQuote => {
                if b == b']' && next == Some(b']') {
                    i += 2;
                } else if b == b']' {
                    mode = ScanMode::Outside;
                    i += 1;
                } else {
                    i += 1;
                }
            }
        }
    }

    false
}

fn render_statements(statements: &[Statement], original: &str) -> String {
    let mut rendered = statements
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(";\n");

    if statements.len() > 1 || original.trim_end().ends_with(';') {
        rendered.push(';');
    }

    rendered
}

fn lint_rule_counts(
    sql: &str,
    dialect: Dialect,
    lint_config: &LintConfig,
) -> BTreeMap<String, usize> {
    let issues = lint_issues(sql, dialect, lint_config);
    lint_rule_counts_from_issues(&issues)
}

fn lint_state(sql: &str, dialect: Dialect, lint_config: &LintConfig) -> FixLintState {
    let issues = lint_issues(sql, dialect, lint_config);
    FixLintState::from_issues(issues)
}

fn lint_issues(sql: &str, dialect: Dialect, lint_config: &LintConfig) -> Vec<Issue> {
    let mut result = analyze(&AnalyzeRequest {
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
    });

    #[cfg(feature = "templating")]
    {
        if contains_template_markers(sql)
            && issues_have_parse_errors(&result.issues)
            && template_retry_enabled_for_fixes(lint_config)
        {
            let jinja_result = analyze(&AnalyzeRequest {
                sql: sql.to_string(),
                files: None,
                dialect,
                source_name: None,
                options: Some(AnalysisOptions {
                    lint: Some(lint_config.clone()),
                    ..Default::default()
                }),
                schema: None,
                template_config: Some(TemplateConfig {
                    mode: TemplateMode::Jinja,
                    context: HashMap::new(),
                }),
            });

            result = if issues_have_template_errors(&jinja_result.issues) {
                analyze(&AnalyzeRequest {
                    sql: sql.to_string(),
                    files: None,
                    dialect,
                    source_name: None,
                    options: Some(AnalysisOptions {
                        lint: Some(lint_config.clone()),
                        ..Default::default()
                    }),
                    schema: None,
                    template_config: Some(TemplateConfig {
                        mode: TemplateMode::Dbt,
                        context: HashMap::new(),
                    }),
                })
            } else {
                jinja_result
            };
        }
    }

    result
        .issues
        .into_iter()
        .filter(|issue| issue.code.starts_with("LINT_") || issue.code == issue_codes::PARSE_ERROR)
        .collect()
}

#[cfg(feature = "templating")]
fn contains_template_markers(sql: &str) -> bool {
    sql.contains("{{") || sql.contains("{%") || sql.contains("{#")
}

#[cfg(feature = "templating")]
fn template_retry_enabled_for_fixes(lint_config: &LintConfig) -> bool {
    let registry_config = LintConfig {
        enabled: true,
        disabled_rules: vec![],
        rule_configs: BTreeMap::new(),
    };
    let enabled_codes: Vec<String> = flowscope_core::linter::rules::all_rules(&registry_config)
        .into_iter()
        .map(|rule| rule.code().to_string())
        .filter(|code| lint_config.is_rule_enabled(code))
        .collect();

    if enabled_codes.len() != 1 {
        return false;
    }

    let only_code = &enabled_codes[0];
    only_code.eq_ignore_ascii_case(issue_codes::LINT_LT_004)
        || only_code.eq_ignore_ascii_case(issue_codes::LINT_LT_007)
        || only_code.eq_ignore_ascii_case(issue_codes::LINT_CP_003)
}

#[cfg(feature = "templating")]
fn issues_have_parse_errors(issues: &[Issue]) -> bool {
    issues
        .iter()
        .any(|issue| issue.code == issue_codes::PARSE_ERROR)
}

#[cfg(feature = "templating")]
fn issues_have_template_errors(issues: &[Issue]) -> bool {
    issues
        .iter()
        .any(|issue| issue.code == issue_codes::TEMPLATE_ERROR)
}

fn lint_rule_counts_from_issues(issues: &[Issue]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for issue in issues {
        *counts.entry(issue.code.clone()).or_insert(0usize) += 1;
    }
    counts
}

fn collect_core_autofix_rules(issues: &[Issue], allow_unsafe: bool) -> HashSet<String> {
    issues
        .iter()
        .filter_map(|issue| {
            let autofix = issue.autofix.as_ref()?;
            let applicable = match autofix.applicability {
                IssueAutofixApplicability::Safe => true,
                IssueAutofixApplicability::Unsafe => allow_unsafe,
                IssueAutofixApplicability::DisplayOnly => false,
            };
            if applicable && core_autofix_conflict_priority(Some(issue.code.as_str())) == 0 {
                Some(issue.code.clone())
            } else {
                None
            }
        })
        .collect()
}

fn core_autofix_rules_not_improved(
    before_counts: &BTreeMap<String, usize>,
    after_counts: &BTreeMap<String, usize>,
    core_autofix_rules: &HashSet<String>,
) -> bool {
    let lt03_improved = after_counts
        .get(issue_codes::LINT_LT_003)
        .copied()
        .unwrap_or(0)
        < before_counts
            .get(issue_codes::LINT_LT_003)
            .copied()
            .unwrap_or(0);

    core_autofix_rules.iter().any(|code| {
        if lt03_improved && code == issue_codes::LINT_LT_005 {
            // Allow LT03 fixes that trade one LT05 long-line violation for one
            // LT03 operator-layout violation at equal total counts.
            return false;
        }
        let before_count = before_counts.get(code).copied().unwrap_or(0);
        before_count > 0 && after_counts.get(code).copied().unwrap_or(0) >= before_count
    })
}

fn parse_errors_increased(
    before_counts: &BTreeMap<String, usize>,
    after_counts: &BTreeMap<String, usize>,
) -> bool {
    after_counts
        .get(issue_codes::PARSE_ERROR)
        .copied()
        .unwrap_or(0)
        > before_counts
            .get(issue_codes::PARSE_ERROR)
            .copied()
            .unwrap_or(0)
}

fn regression_guard_total(counts: &BTreeMap<String, usize>) -> usize {
    counts.values().copied().sum()
}

/// Try core-only then incremental fallback plans, returning the first
/// successful `FixPassResult`.  Used when the primary fix plan causes a
/// regression (parse errors, rewrite guard, or masked/worse counts).
#[allow(clippy::too_many_arguments)]
fn try_fallback_fix(
    sql: &str,
    dialect: Dialect,
    lint_config: &LintConfig,
    before_counts: &BTreeMap<String, usize>,
    before_issues: &[Issue],
    core_candidates: &[FixCandidate],
    protected_ranges: &[PatchProtectedRange],
    allow_unsafe: bool,
    incremental_iterations: usize,
    incremental_rule_evaluations: usize,
    profile: &mut FixProfileGuard,
    core_label: &'static str,
    incremental_label: &'static str,
) -> Option<FixPassResult> {
    let stage_started = Instant::now();
    if let Some(outcome) = try_core_only_fix_plan(
        sql,
        dialect,
        lint_config,
        before_counts,
        core_candidates,
        protected_ranges,
        allow_unsafe,
    ) {
        profile.record(core_label, stage_started);
        return Some(FixPassResult {
            post_lint_state: lint_state(&outcome.sql, dialect, lint_config),
            outcome,
        });
    }
    profile.record(core_label, stage_started);

    let stage_started = Instant::now();
    if let Some(outcome) = try_incremental_core_fix_plan(
        sql,
        dialect,
        lint_config,
        before_counts,
        Some(before_issues),
        allow_unsafe,
        incremental_iterations,
        incremental_rule_evaluations,
    ) {
        profile.record(incremental_label, stage_started);
        return Some(FixPassResult {
            post_lint_state: lint_state(&outcome.sql, dialect, lint_config),
            outcome,
        });
    }
    profile.record(incremental_label, stage_started);

    None
}

fn try_core_only_fix_plan(
    sql: &str,
    dialect: Dialect,
    lint_config: &LintConfig,
    before_counts: &BTreeMap<String, usize>,
    core_candidates: &[FixCandidate],
    protected_ranges: &[PatchProtectedRange],
    allow_unsafe: bool,
) -> Option<FixOutcome> {
    if core_candidates.is_empty() {
        return None;
    }

    let planned = plan_fix_candidates(
        sql,
        core_candidates.to_vec(),
        protected_ranges,
        allow_unsafe,
    );
    if planned.edits.is_empty() {
        return None;
    }

    let fixed_sql = apply_planned_edits(sql, &planned.edits);
    if fixed_sql == sql {
        return None;
    }

    let after_counts = lint_rule_counts(&fixed_sql, dialect, lint_config);
    if parse_errors_increased(before_counts, &after_counts) {
        return None;
    }

    let counts = FixCounts::from_removed(before_counts, &after_counts);
    let before_total = regression_guard_total(before_counts);
    let after_total = regression_guard_total(&after_counts);
    if counts.total() == 0 || after_total > before_total {
        return None;
    }

    Some(FixOutcome {
        sql: fixed_sql,
        counts,
        changed: true,
        skipped_due_to_comments: false,
        skipped_due_to_regression: false,
        skipped_counts: planned.skipped,
    })
}

fn is_incremental_core_candidate(candidate: &FixCandidate, allow_unsafe: bool) -> bool {
    if candidate.source != FixCandidateSource::CoreAutofix {
        return false;
    }

    if candidate.rule_code.is_none() {
        return false;
    }

    match candidate.applicability {
        FixCandidateApplicability::Safe => true,
        FixCandidateApplicability::Unsafe => allow_unsafe,
        FixCandidateApplicability::DisplayOnly => false,
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Al001AliasingPreference {
    Explicit,
    Implicit,
}

fn al001_aliasing_preference(lint_config: &LintConfig) -> Al001AliasingPreference {
    if lint_config
        .rule_option_str(issue_codes::LINT_AL_001, "aliasing")
        .is_some_and(|value| value.eq_ignore_ascii_case("implicit"))
    {
        Al001AliasingPreference::Implicit
    } else {
        Al001AliasingPreference::Explicit
    }
}

fn build_al001_fallback_candidates(
    sql: &str,
    dialect: Dialect,
    issues: &[Issue],
    lint_config: &LintConfig,
) -> Vec<FixCandidate> {
    let fallback_issues: Vec<&Issue> = issues
        .iter()
        .filter(|issue| {
            issue.code.eq_ignore_ascii_case(issue_codes::LINT_AL_001) && issue.span.is_some()
        })
        .collect();
    if fallback_issues.is_empty() {
        return Vec::new();
    }

    let Some(tokens) = alias_tokenize_with_offsets(sql, dialect) else {
        return Vec::new();
    };

    let preference = al001_aliasing_preference(lint_config);
    let mut candidates = Vec::new();
    for issue in fallback_issues {
        let Some(span) = issue.span else {
            continue;
        };
        let alias_start = span.start.min(sql.len());
        let previous_token = tokens
            .iter()
            .rev()
            .find(|token| token.end <= alias_start && !is_alias_trivia_token(&token.token));

        match preference {
            Al001AliasingPreference::Explicit => {
                if previous_token.is_some_and(|token| is_as_token(&token.token)) {
                    continue;
                }
                let replacement = if has_whitespace_before_offset(sql, alias_start) {
                    "AS "
                } else {
                    " AS "
                };
                candidates.push(FixCandidate {
                    start: alias_start,
                    end: alias_start,
                    replacement: replacement.to_string(),
                    applicability: FixCandidateApplicability::Safe,
                    source: FixCandidateSource::CoreAutofix,
                    rule_code: Some(issue_codes::LINT_AL_001.to_string()),
                });
            }
            Al001AliasingPreference::Implicit => {
                let Some(as_token) = previous_token.filter(|token| is_as_token(&token.token))
                else {
                    continue;
                };
                candidates.push(FixCandidate {
                    start: as_token.start,
                    end: alias_start,
                    replacement: " ".to_string(),
                    applicability: FixCandidateApplicability::Safe,
                    source: FixCandidateSource::CoreAutofix,
                    rule_code: Some(issue_codes::LINT_AL_001.to_string()),
                });
            }
        }
    }

    candidates
}

fn merge_skipped_counts(total: &mut FixSkippedCounts, current: &FixSkippedCounts) {
    total.unsafe_skipped += current.unsafe_skipped;
    total.protected_range_blocked += current.protected_range_blocked;
    total.overlap_conflict_blocked += current.overlap_conflict_blocked;
    total.display_only += current.display_only;
}

#[allow(clippy::too_many_arguments)]
fn try_incremental_core_fix_plan(
    sql: &str,
    dialect: Dialect,
    lint_config: &LintConfig,
    before_counts: &BTreeMap<String, usize>,
    initial_issues: Option<&[Issue]>,
    allow_unsafe: bool,
    max_iterations: usize,
    max_rule_evaluations_per_iteration: usize,
) -> Option<FixOutcome> {
    let mut current_sql = sql.to_string();
    let mut current_counts = before_counts.clone();
    let mut changed = false;
    let mut skipped_counts = FixSkippedCounts::default();
    let mut counts_cache: HashMap<String, BTreeMap<String, usize>> = HashMap::new();
    let mut seen_sql: HashSet<u64> = HashSet::new();
    seen_sql.insert(hash_sql(&current_sql));

    let max_iterations = max_iterations.max(1);
    let max_rule_evaluations_per_iteration = max_rule_evaluations_per_iteration.max(1);
    let mut initial_issues = initial_issues;
    for _ in 0..max_iterations {
        let issues: Cow<'_, [Issue]> = if let Some(issues) = initial_issues.take() {
            Cow::Borrowed(issues)
        } else {
            Cow::Owned(lint_issues(&current_sql, dialect, lint_config))
        };
        let mut all_candidates = build_fix_candidates_from_issue_autofixes(&current_sql, &issues);
        all_candidates.extend(build_al001_fallback_candidates(
            &current_sql,
            dialect,
            &issues,
            lint_config,
        ));
        let candidates = all_candidates
            .into_iter()
            .filter(|candidate| is_incremental_core_candidate(candidate, allow_unsafe))
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            break;
        }

        let mut by_rule: BTreeMap<String, Vec<FixCandidate>> = BTreeMap::new();
        for candidate in candidates {
            if let Some(rule_code) = candidate.rule_code.clone() {
                by_rule.entry(rule_code).or_default().push(candidate);
            }
        }

        if by_rule.is_empty() {
            break;
        }

        let protected_ranges =
            collect_comment_protected_ranges(&current_sql, dialect, !allow_unsafe);
        let current_total = regression_guard_total(&current_counts);
        let mut ordered_rules = by_rule.into_iter().collect::<Vec<_>>();
        if max_rule_evaluations_per_iteration != usize::MAX {
            ordered_rules.sort_by(
                |(left_rule, left_candidates), (right_rule, right_candidates)| {
                    let left_count = current_counts.get(left_rule).copied().unwrap_or(0);
                    let right_count = current_counts.get(right_rule).copied().unwrap_or(0);
                    right_count
                        .cmp(&left_count)
                        .then_with(|| right_candidates.len().cmp(&left_candidates.len()))
                        .then_with(|| left_rule.cmp(right_rule))
                },
            );
        }

        let mut best_rule: Option<String> = None;
        let mut best_sql: Option<String> = None;
        let mut best_counts: Option<BTreeMap<String, usize>> = None;
        let mut best_removed = 0usize;
        let mut best_after_total = usize::MAX;
        let mut evaluated_candidate_sql = HashSet::new();

        let mut rule_evaluations = 0usize;
        for (rule_code, rule_candidates) in ordered_rules {
            if rule_evaluations >= max_rule_evaluations_per_iteration {
                break;
            }
            let planned = plan_fix_candidates(
                &current_sql,
                rule_candidates,
                &protected_ranges,
                allow_unsafe,
            );
            merge_skipped_counts(&mut skipped_counts, &planned.skipped);

            if planned.edits.is_empty() {
                continue;
            }
            rule_evaluations += 1;

            let candidate_sql = apply_planned_edits(&current_sql, &planned.edits);
            if candidate_sql == current_sql {
                continue;
            }
            if !evaluated_candidate_sql.insert(candidate_sql.clone()) {
                continue;
            }

            let candidate_counts = if let Some(cached) = counts_cache.get(&candidate_sql) {
                cached.clone()
            } else {
                let counts = lint_rule_counts(&candidate_sql, dialect, lint_config);
                counts_cache.insert(candidate_sql.clone(), counts.clone());
                counts
            };
            if parse_errors_increased(&current_counts, &candidate_counts) {
                continue;
            }

            let candidate_after_total = regression_guard_total(&candidate_counts);
            if candidate_after_total > current_total {
                continue;
            }

            let candidate_removed =
                FixCounts::from_removed(&current_counts, &candidate_counts).total();
            if candidate_removed == 0 {
                continue;
            }

            let better = candidate_removed > best_removed
                || (candidate_removed == best_removed && candidate_after_total < best_after_total)
                || (candidate_removed == best_removed
                    && candidate_after_total == best_after_total
                    && best_rule
                        .as_ref()
                        .is_none_or(|current_best| rule_code < *current_best));

            if better {
                best_removed = candidate_removed;
                best_after_total = candidate_after_total;
                best_rule = Some(rule_code);
                best_sql = Some(candidate_sql);
                best_counts = Some(candidate_counts);
            }
        }

        let Some(next_sql) = best_sql else {
            break;
        };
        let Some(next_counts) = best_counts else {
            break;
        };
        if !seen_sql.insert(hash_sql(&next_sql)) {
            break;
        }

        current_sql = next_sql;
        current_counts = next_counts;
        changed = true;
    }

    if !changed || current_sql == sql {
        return None;
    }

    let final_counts = FixCounts::from_removed(before_counts, &current_counts);
    if final_counts.total() == 0 {
        return None;
    }

    Some(FixOutcome {
        sql: current_sql,
        counts: final_counts,
        changed: true,
        skipped_due_to_comments: false,
        skipped_due_to_regression: false,
        skipped_counts,
    })
}

#[derive(Debug, Clone)]
struct TableAliasOccurrence {
    alias_key: String,
    alias_start: usize,
    explicit_as: bool,
    as_start: Option<usize>,
}

fn preserve_original_table_alias_style(
    original_sql: &str,
    fixed_sql: &str,
    dialect: Dialect,
) -> String {
    let Some(original_aliases) = table_alias_occurrences(original_sql, dialect) else {
        return fixed_sql.to_string();
    };
    let Some(fixed_aliases) = table_alias_occurrences(fixed_sql, dialect) else {
        return fixed_sql.to_string();
    };

    let mut desired_by_alias: BTreeMap<String, VecDeque<bool>> = BTreeMap::new();
    for alias in original_aliases {
        desired_by_alias
            .entry(alias.alias_key)
            .or_default()
            .push_back(alias.explicit_as);
    }

    let mut removals = Vec::new();
    for alias in fixed_aliases {
        let desired_explicit = desired_by_alias
            .get_mut(&alias.alias_key)
            .and_then(VecDeque::pop_front)
            .unwrap_or(alias.explicit_as);

        if alias.explicit_as && !desired_explicit {
            if let Some(as_start) = alias.as_start {
                removals.push((as_start, alias.alias_start));
            }
        }
    }

    apply_byte_removals(fixed_sql, removals)
}

fn apply_configured_table_alias_style(
    sql: &str,
    dialect: Dialect,
    lint_config: &LintConfig,
) -> String {
    let prefer_implicit = matches!(
        al001_aliasing_preference(lint_config),
        Al001AliasingPreference::Implicit
    );
    enforce_table_alias_style(sql, dialect, prefer_implicit)
}

fn enforce_table_alias_style(sql: &str, dialect: Dialect, prefer_implicit: bool) -> String {
    let Some(aliases) = table_alias_occurrences(sql, dialect) else {
        return sql.to_string();
    };

    if prefer_implicit {
        let removals: Vec<(usize, usize)> = aliases
            .into_iter()
            .filter_map(|alias| {
                if alias.explicit_as {
                    alias.as_start.map(|as_start| (as_start, alias.alias_start))
                } else {
                    None
                }
            })
            .collect();
        return apply_byte_removals(sql, removals);
    }

    let insertions: Vec<(usize, &'static str)> = aliases
        .into_iter()
        .filter(|alias| !alias.explicit_as)
        .map(|alias| {
            let insertion = if has_whitespace_before_offset(sql, alias.alias_start) {
                "AS "
            } else {
                " AS "
            };
            (alias.alias_start, insertion)
        })
        .collect();
    apply_byte_insertions(sql, insertions)
}

fn has_whitespace_before_offset(sql: &str, offset: usize) -> bool {
    sql.get(..offset)
        .and_then(|prefix| prefix.chars().next_back())
        .is_some_and(char::is_whitespace)
}

fn apply_byte_removals(sql: &str, mut removals: Vec<(usize, usize)>) -> String {
    if removals.is_empty() {
        return sql.to_string();
    }

    removals.sort_unstable();
    removals.dedup();

    let mut out = sql.to_string();
    for (start, end) in removals.into_iter().rev() {
        if start < end && end <= out.len() {
            out.replace_range(start..end, "");
        }
    }
    out
}

fn apply_byte_insertions(sql: &str, mut insertions: Vec<(usize, &'static str)>) -> String {
    if insertions.is_empty() {
        return sql.to_string();
    }

    insertions.retain(|(offset, _)| *offset <= sql.len());
    if insertions.is_empty() {
        return sql.to_string();
    }

    insertions
        .sort_unstable_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(right.1)));
    insertions.dedup_by(|left, right| left.0 == right.0);

    let extra_len: usize = insertions
        .iter()
        .map(|(_, insertion)| insertion.len())
        .sum();
    let mut out = String::with_capacity(sql.len() + extra_len);
    let mut cursor = 0usize;
    for (offset, insertion) in insertions {
        if offset < cursor || offset > sql.len() {
            continue;
        }
        out.push_str(&sql[cursor..offset]);
        out.push_str(insertion);
        cursor = offset;
    }
    out.push_str(&sql[cursor..]);
    out
}

fn table_alias_occurrences(sql: &str, dialect: Dialect) -> Option<Vec<TableAliasOccurrence>> {
    let statements = parse_sql_with_dialect(sql, dialect).ok()?;
    let tokens = alias_tokenize_with_offsets(sql, dialect)?;

    let mut aliases = Vec::new();
    for statement in &statements {
        collect_table_alias_idents_in_statement(statement, &mut |ident| {
            aliases.push(ident.clone())
        });
    }

    let mut occurrences = Vec::with_capacity(aliases.len());
    for alias in aliases {
        let Some((alias_start, _alias_end)) = alias_ident_span_offsets(sql, &alias) else {
            continue;
        };
        let previous_token = tokens
            .iter()
            .rev()
            .find(|token| token.end <= alias_start && !is_alias_trivia_token(&token.token));

        let (explicit_as, as_start) = match previous_token {
            Some(token) if is_as_token(&token.token) => (true, Some(token.start)),
            _ => (false, None),
        };

        occurrences.push(TableAliasOccurrence {
            alias_key: alias.value.to_ascii_lowercase(),
            alias_start,
            explicit_as,
            as_start,
        });
    }

    Some(occurrences)
}

fn alias_ident_span_offsets(sql: &str, ident: &Ident) -> Option<(usize, usize)> {
    let start = alias_line_col_to_offset(
        sql,
        ident.span.start.line as usize,
        ident.span.start.column as usize,
    )?;
    let end = alias_line_col_to_offset(
        sql,
        ident.span.end.line as usize,
        ident.span.end.column as usize,
    )?;
    Some((start, end))
}

fn is_as_token(token: &Token) -> bool {
    matches!(token, Token::Word(word) if word.value.eq_ignore_ascii_case("AS"))
}

#[derive(Clone)]
struct AliasLocatedToken {
    token: Token,
    start: usize,
    end: usize,
}

fn alias_tokenize_with_offsets(sql: &str, dialect: Dialect) -> Option<Vec<AliasLocatedToken>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;

    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        let Some((start, end)) = alias_token_with_span_offsets(sql, &token) else {
            continue;
        };
        out.push(AliasLocatedToken {
            token: token.token,
            start,
            end,
        });
    }

    Some(out)
}

fn alias_token_with_span_offsets(sql: &str, token: &TokenWithSpan) -> Option<(usize, usize)> {
    let start = alias_line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )?;
    let end = alias_line_col_to_offset(
        sql,
        token.span.end.line as usize,
        token.span.end.column as usize,
    )?;
    Some((start, end))
}

fn alias_line_col_to_offset(sql: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }

    let mut current_line = 1usize;
    let mut current_col = 1usize;
    for (offset, ch) in sql.char_indices() {
        if current_line == line && current_col == column {
            return Some(offset);
        }
        if ch == '\n' {
            current_line += 1;
            current_col = 1;
        } else {
            current_col += 1;
        }
    }
    if current_line == line && current_col == column {
        return Some(sql.len());
    }
    None
}

fn is_alias_trivia_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(
            Whitespace::Space
                | Whitespace::Newline
                | Whitespace::Tab
                | Whitespace::SingleLineComment { .. }
                | Whitespace::MultiLineComment(_)
        )
    )
}

fn collect_table_alias_idents_in_statement<F: FnMut(&Ident)>(
    statement: &Statement,
    visitor: &mut F,
) {
    match statement {
        Statement::Query(query) => collect_table_alias_idents_in_query(query, visitor),
        Statement::Insert(insert) => {
            if let Some(source) = &insert.source {
                collect_table_alias_idents_in_query(source, visitor);
            }
        }
        Statement::CreateView(CreateView { query, .. }) => {
            collect_table_alias_idents_in_query(query, visitor)
        }
        Statement::CreateTable(create) => {
            if let Some(query) = &create.query {
                collect_table_alias_idents_in_query(query, visitor);
            }
        }
        Statement::Merge(Merge { table, source, .. }) => {
            collect_table_alias_idents_in_table_factor(table, visitor);
            collect_table_alias_idents_in_table_factor(source, visitor);
        }
        _ => {}
    }
}

fn collect_table_alias_idents_in_query<F: FnMut(&Ident)>(query: &Query, visitor: &mut F) {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            collect_table_alias_idents_in_query(&cte.query, visitor);
        }
    }

    collect_table_alias_idents_in_set_expr(&query.body, visitor);
}

fn collect_table_alias_idents_in_set_expr<F: FnMut(&Ident)>(set_expr: &SetExpr, visitor: &mut F) {
    match set_expr {
        SetExpr::Select(select) => {
            for table in &select.from {
                collect_table_alias_idents_in_table_with_joins(table, visitor);
            }
        }
        SetExpr::Query(query) => collect_table_alias_idents_in_query(query, visitor),
        SetExpr::SetOperation { left, right, .. } => {
            collect_table_alias_idents_in_set_expr(left, visitor);
            collect_table_alias_idents_in_set_expr(right, visitor);
        }
        SetExpr::Insert(statement)
        | SetExpr::Update(statement)
        | SetExpr::Delete(statement)
        | SetExpr::Merge(statement) => collect_table_alias_idents_in_statement(statement, visitor),
        _ => {}
    }
}

fn collect_table_alias_idents_in_table_with_joins<F: FnMut(&Ident)>(
    table_with_joins: &TableWithJoins,
    visitor: &mut F,
) {
    collect_table_alias_idents_in_table_factor(&table_with_joins.relation, visitor);
    for join in &table_with_joins.joins {
        collect_table_alias_idents_in_table_factor(&join.relation, visitor);
    }
}

fn collect_table_alias_idents_in_table_factor<F: FnMut(&Ident)>(
    table_factor: &TableFactor,
    visitor: &mut F,
) {
    if let Some(alias) = table_factor_alias_ident(table_factor) {
        visitor(alias);
    }

    match table_factor {
        TableFactor::Derived { subquery, .. } => {
            collect_table_alias_idents_in_query(subquery, visitor)
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => collect_table_alias_idents_in_table_with_joins(table_with_joins, visitor),
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => {
            collect_table_alias_idents_in_table_factor(table, visitor)
        }
        _ => {}
    }
}

#[cfg(test)]
fn is_ascii_whitespace_byte(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t' | 0x0b | 0x0c)
}

#[cfg(test)]
fn is_ascii_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

#[cfg(test)]
fn is_ascii_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
fn skip_ascii_whitespace(bytes: &[u8], mut idx: usize) -> usize {
    while idx < bytes.len() && is_ascii_whitespace_byte(bytes[idx]) {
        idx += 1;
    }
    idx
}

#[cfg(test)]
fn consume_ascii_identifier(bytes: &[u8], start: usize) -> Option<usize> {
    if start >= bytes.len() || !is_ascii_ident_start(bytes[start]) {
        return None;
    }
    let mut idx = start + 1;
    while idx < bytes.len() && is_ascii_ident_continue(bytes[idx]) {
        idx += 1;
    }
    Some(idx)
}

#[cfg(test)]
fn is_word_boundary_for_keyword(bytes: &[u8], idx: usize) -> bool {
    idx == 0 || idx >= bytes.len() || !is_ascii_ident_continue(bytes[idx])
}

#[cfg(test)]
fn match_ascii_keyword_at(bytes: &[u8], start: usize, keyword_upper: &[u8]) -> Option<usize> {
    let end = start.checked_add(keyword_upper.len())?;
    if end > bytes.len() {
        return None;
    }
    if !is_word_boundary_for_keyword(bytes, start.saturating_sub(1))
        || !is_word_boundary_for_keyword(bytes, end)
    {
        return None;
    }
    let matches = bytes[start..end]
        .iter()
        .zip(keyword_upper.iter())
        .all(|(actual, expected)| actual.to_ascii_uppercase() == *expected);
    if matches {
        Some(end)
    } else {
        None
    }
}

#[cfg(test)]
fn parse_subquery_alias_suffix(suffix: &str) -> Option<String> {
    let bytes = suffix.as_bytes();
    let mut i = skip_ascii_whitespace(bytes, 0);
    if let Some(as_end) = match_ascii_keyword_at(bytes, i, b"AS") {
        let after_as = skip_ascii_whitespace(bytes, as_end);
        if after_as == as_end {
            return None;
        }
        i = after_as;
    }

    let alias_start = i;
    let alias_end = consume_ascii_identifier(bytes, alias_start)?;
    i = skip_ascii_whitespace(bytes, alias_end);
    if i < bytes.len() && bytes[i] == b';' {
        i += 1;
        i = skip_ascii_whitespace(bytes, i);
    }
    if i != bytes.len() {
        return None;
    }
    Some(suffix[alias_start..alias_end].to_string())
}

#[cfg(test)]
fn fix_subquery_to_cte(sql: &str) -> String {
    let bytes = sql.as_bytes();
    let mut i = skip_ascii_whitespace(bytes, 0);
    let Some(select_end) = match_ascii_keyword_at(bytes, i, b"SELECT") else {
        return sql.to_string();
    };
    i = skip_ascii_whitespace(bytes, select_end);
    if i == select_end || i >= bytes.len() || bytes[i] != b'*' {
        return sql.to_string();
    }
    i += 1;
    let from_start = skip_ascii_whitespace(bytes, i);
    if from_start == i {
        return sql.to_string();
    }
    let Some(from_end) = match_ascii_keyword_at(bytes, from_start, b"FROM") else {
        return sql.to_string();
    };
    let open_paren_idx = skip_ascii_whitespace(bytes, from_end);
    if open_paren_idx == from_end || open_paren_idx >= bytes.len() || bytes[open_paren_idx] != b'('
    {
        return sql.to_string();
    };

    let Some(close_paren_idx) = find_matching_parenthesis_outside_quotes(sql, open_paren_idx)
    else {
        return sql.to_string();
    };

    let subquery = sql[open_paren_idx + 1..close_paren_idx].trim();
    if !subquery.to_ascii_lowercase().starts_with("select") {
        return sql.to_string();
    }

    let suffix = &sql[close_paren_idx + 1..];
    let Some(alias) = parse_subquery_alias_suffix(suffix) else {
        return sql.to_string();
    };

    let mut rewritten = format!("WITH {alias} AS ({subquery}) SELECT * FROM {alias}");
    if suffix.trim_end().ends_with(';') {
        rewritten.push(';');
    }
    rewritten
}

#[cfg(test)]
fn find_matching_parenthesis_outside_quotes(sql: &str, open_paren_idx: usize) -> Option<usize> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Outside,
        SingleQuote,
        DoubleQuote,
        BacktickQuote,
        BracketQuote,
    }

    let bytes = sql.as_bytes();
    if open_paren_idx >= bytes.len() || bytes[open_paren_idx] != b'(' {
        return None;
    }

    let mut depth = 0usize;
    let mut mode = Mode::Outside;
    let mut i = open_paren_idx;

    while i < bytes.len() {
        let b = bytes[i];
        let next = bytes.get(i + 1).copied();

        match mode {
            Mode::Outside => {
                if b == b'\'' {
                    mode = Mode::SingleQuote;
                    i += 1;
                    continue;
                }
                if b == b'"' {
                    mode = Mode::DoubleQuote;
                    i += 1;
                    continue;
                }
                if b == b'`' {
                    mode = Mode::BacktickQuote;
                    i += 1;
                    continue;
                }
                if b == b'[' {
                    mode = Mode::BracketQuote;
                    i += 1;
                    continue;
                }
                if b == b'(' {
                    depth += 1;
                    i += 1;
                    continue;
                }
                if b == b')' {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                i += 1;
            }
            Mode::SingleQuote => {
                if b == b'\'' {
                    if next == Some(b'\'') {
                        i += 2;
                    } else {
                        mode = Mode::Outside;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            Mode::DoubleQuote => {
                if b == b'"' {
                    if next == Some(b'"') {
                        i += 2;
                    } else {
                        mode = Mode::Outside;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            Mode::BacktickQuote => {
                if b == b'`' {
                    if next == Some(b'`') {
                        i += 2;
                    } else {
                        mode = Mode::Outside;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            Mode::BracketQuote => {
                if b == b']' {
                    if next == Some(b']') {
                        i += 2;
                    } else {
                        mode = Mode::Outside;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests;
