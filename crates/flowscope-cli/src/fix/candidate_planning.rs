use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SpanEdit {
    start: usize,
    end: usize,
    replacement: String,
}

impl SpanEdit {
    fn replace(start: usize, end: usize, replacement: impl Into<String>) -> Self {
        Self {
            start,
            end,
            replacement: replacement.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
#[allow(dead_code)]
pub(super) enum FixCandidateApplicability {
    Safe,
    Unsafe,
    DisplayOnly,
}

impl FixCandidateApplicability {
    fn sort_key(self) -> u8 {
        match self {
            Self::Safe => 0,
            Self::Unsafe => 1,
            Self::DisplayOnly => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
#[allow(dead_code)]
pub(super) enum FixCandidateSource {
    PrimaryRewrite,
    CoreAutofix,
    UnsafeFallback,
    DisplayHint,
}

pub(super) fn core_autofix_conflict_priority(rule_code: Option<&str>) -> u8 {
    let Some(code) = rule_code else {
        return 2;
    };

    if code.eq_ignore_ascii_case(issue_codes::LINT_AM_001)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CV_001)
        || code.eq_ignore_ascii_case(issue_codes::LINT_AM_002)
        || code.eq_ignore_ascii_case(issue_codes::LINT_AM_003)
        || code.eq_ignore_ascii_case(issue_codes::LINT_AM_005)
        || code.eq_ignore_ascii_case(issue_codes::LINT_AM_008)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CV_002)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CV_003)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CV_004)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CV_005)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CV_006)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CV_007)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CV_010)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CV_012)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CP_001)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CP_002)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CP_003)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CP_004)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CP_005)
        || code.eq_ignore_ascii_case(issue_codes::LINT_AL_001)
        || code.eq_ignore_ascii_case(issue_codes::LINT_AL_002)
        || code.eq_ignore_ascii_case(issue_codes::LINT_AL_005)
        || code.eq_ignore_ascii_case(issue_codes::LINT_AL_007)
        || code.eq_ignore_ascii_case(issue_codes::LINT_AL_009)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_001)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_002)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_003)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_004)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_005)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_006)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_007)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_008)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_009)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_010)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_011)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_012)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_013)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_014)
        || code.eq_ignore_ascii_case(issue_codes::LINT_LT_015)
        || code.eq_ignore_ascii_case(issue_codes::LINT_ST_001)
        || code.eq_ignore_ascii_case(issue_codes::LINT_ST_002)
        || code.eq_ignore_ascii_case(issue_codes::LINT_ST_006)
        || code.eq_ignore_ascii_case(issue_codes::LINT_ST_009)
        || code.eq_ignore_ascii_case(issue_codes::LINT_ST_005)
        || code.eq_ignore_ascii_case(issue_codes::LINT_ST_008)
        || code.eq_ignore_ascii_case(issue_codes::LINT_ST_012)
        || code.eq_ignore_ascii_case(issue_codes::LINT_TQ_002)
        || code.eq_ignore_ascii_case(issue_codes::LINT_TQ_003)
        || code.eq_ignore_ascii_case(issue_codes::LINT_RF_003)
        || code.eq_ignore_ascii_case(issue_codes::LINT_RF_004)
        || code.eq_ignore_ascii_case(issue_codes::LINT_CV_011)
        || code.eq_ignore_ascii_case(issue_codes::LINT_RF_006)
        || code.eq_ignore_ascii_case(issue_codes::LINT_JJ_001)
    {
        0
    } else {
        2
    }
}

#[derive(Debug, Clone)]
pub(super) struct FixCandidate {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) replacement: String,
    pub(super) applicability: FixCandidateApplicability,
    pub(super) source: FixCandidateSource,
    pub(super) rule_code: Option<String>,
}

pub(super) fn fix_candidate_source_priority(candidate: &FixCandidate) -> u8 {
    match candidate.source {
        FixCandidateSource::CoreAutofix => {
            core_autofix_conflict_priority(candidate.rule_code.as_deref())
        }
        FixCandidateSource::PrimaryRewrite => 1,
        FixCandidateSource::UnsafeFallback => 3,
        FixCandidateSource::DisplayHint => 4,
    }
}

#[derive(Debug, Default)]
pub(super) struct PlannedFixes {
    pub(super) edits: Vec<PatchEdit>,
    pub(super) skipped: FixSkippedCounts,
}

pub(super) fn build_fix_candidates_from_rewrite(
    sql: &str,
    rewritten_sql: &str,
    applicability: FixCandidateApplicability,
    source: FixCandidateSource,
) -> Vec<FixCandidate> {
    if sql == rewritten_sql {
        return Vec::new();
    }

    let mut candidates = derive_localized_span_edits(sql, rewritten_sql)
        .into_iter()
        .map(|edit| FixCandidate {
            start: edit.start,
            end: edit.end,
            replacement: edit.replacement,
            applicability,
            source,
            rule_code: None,
        })
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        candidates.push(FixCandidate {
            start: 0,
            end: sql.len(),
            replacement: rewritten_sql.to_string(),
            applicability,
            source,
            rule_code: None,
        });
    }

    candidates
}

pub(super) fn build_fix_candidates_from_issue_autofixes(
    sql: &str,
    issues: &[Issue],
) -> Vec<FixCandidate> {
    let issue_values: Vec<serde_json::Value> = issues
        .iter()
        .filter_map(|issue| serde_json::to_value(issue).ok())
        .collect();
    build_fix_candidates_from_issue_values(sql, &issue_values)
}

pub(super) fn build_fix_candidates_from_issue_values(
    sql: &str,
    issue_values: &[serde_json::Value],
) -> Vec<FixCandidate> {
    let mut candidates = Vec::new();
    let sql_len = sql.len();

    for issue in issue_values {
        let fallback_span = issue.get("span").and_then(json_span_offsets);
        let issue_rule_code = issue
            .get("code")
            .and_then(serde_json::Value::as_str)
            .map(|code| code.to_string());
        if issue_rule_code
            .as_deref()
            .is_some_and(|code| code.eq_ignore_ascii_case(issue_codes::LINT_AL_001))
        {
            // AL01 core-autofix edits can be malformed in complex statement shapes.
            // We generate robust AL01 candidates from spans separately.
            continue;
        }
        let Some(autofix) = issue.get("autofix").or_else(|| issue.get("autoFix")) else {
            continue;
        };
        collect_issue_autofix_candidates(
            autofix,
            fallback_span,
            sql_len,
            None,
            &issue_rule_code,
            &mut candidates,
        );
    }

    candidates
}

pub(super) fn collect_issue_autofix_candidates(
    value: &serde_json::Value,
    fallback_span: Option<(usize, usize)>,
    sql_len: usize,
    inherited_applicability: Option<FixCandidateApplicability>,
    issue_rule_code: &Option<String>,
    out: &mut Vec<FixCandidate>,
) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_issue_autofix_candidates(
                    item,
                    fallback_span,
                    sql_len,
                    inherited_applicability,
                    issue_rule_code,
                    out,
                );
            }
        }
        serde_json::Value::Object(_) => {
            let applicability = parse_issue_autofix_applicability(value)
                .or(inherited_applicability)
                .unwrap_or(FixCandidateApplicability::Safe);

            if let Some(edit) = value.get("edit") {
                collect_issue_autofix_candidates(
                    edit,
                    fallback_span,
                    sql_len,
                    Some(applicability),
                    issue_rule_code,
                    out,
                );
            }
            if let Some(edits) = value
                .get("edits")
                .or_else(|| value.get("fixes"))
                .or_else(|| value.get("changes"))
            {
                collect_issue_autofix_candidates(
                    edits,
                    fallback_span,
                    sql_len,
                    Some(applicability),
                    issue_rule_code,
                    out,
                );
            }

            if let Some((start, end)) = parse_issue_autofix_offsets(value, fallback_span) {
                if start <= end
                    && end <= sql_len
                    && value
                        .get("replacement")
                        .or_else(|| value.get("new_text"))
                        .or_else(|| value.get("newText"))
                        .or_else(|| value.get("text"))
                        .and_then(serde_json::Value::as_str)
                        .is_some()
                {
                    let replacement = value
                        .get("replacement")
                        .or_else(|| value.get("new_text"))
                        .or_else(|| value.get("newText"))
                        .or_else(|| value.get("text"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string();

                    out.push(FixCandidate {
                        start,
                        end,
                        replacement,
                        applicability,
                        source: FixCandidateSource::CoreAutofix,
                        rule_code: issue_rule_code.clone(),
                    });
                }
            }
        }
        _ => {}
    }
}

pub(super) fn parse_issue_autofix_offsets(
    value: &serde_json::Value,
    fallback_span: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    let object = value.as_object()?;

    let mut start = json_usize_field(object, &["start", "start_byte", "startByte"]);
    let mut end = json_usize_field(object, &["end", "end_byte", "endByte"]);

    if let Some((span_start, span_end)) = object.get("span").and_then(json_span_offsets) {
        if start.is_none() {
            start = Some(span_start);
        }
        if end.is_none() {
            end = Some(span_end);
        }
    }

    if let Some((span_start, span_end)) = fallback_span {
        if start.is_none() {
            start = Some(span_start);
        }
        if end.is_none() {
            end = Some(span_end);
        }
    }

    Some((start?, end?))
}

pub(super) fn json_span_offsets(value: &serde_json::Value) -> Option<(usize, usize)> {
    let object = value.as_object()?;
    let start = json_usize_field(object, &["start", "start_byte", "startByte"])?;
    let end = json_usize_field(object, &["end", "end_byte", "endByte"])?;
    Some((start, end))
}

pub(super) fn json_usize_field(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<usize> {
    keys.iter().find_map(|key| {
        object.get(*key).and_then(|value| {
            value
                .as_u64()
                .and_then(|raw| usize::try_from(raw).ok())
                .or_else(|| value.as_str().and_then(|raw| raw.parse::<usize>().ok()))
        })
    })
}

pub(super) fn parse_issue_autofix_applicability(
    value: &serde_json::Value,
) -> Option<FixCandidateApplicability> {
    let object = value.as_object()?;

    if object
        .get("display_only")
        .or_else(|| object.get("displayOnly"))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        return Some(FixCandidateApplicability::DisplayOnly);
    }
    if object.get("unsafe").and_then(serde_json::Value::as_bool) == Some(true) {
        return Some(FixCandidateApplicability::Unsafe);
    }

    let text = object
        .get("applicability")
        .or_else(|| object.get("safety"))
        .or_else(|| object.get("kind"))
        .or_else(|| object.get("mode"))
        .and_then(serde_json::Value::as_str)?;
    parse_issue_autofix_applicability_text(text)
}

pub(super) fn parse_issue_autofix_applicability_text(
    text: &str,
) -> Option<FixCandidateApplicability> {
    match text.trim().to_ascii_lowercase().as_str() {
        "safe" => Some(FixCandidateApplicability::Safe),
        "unsafe" => Some(FixCandidateApplicability::Unsafe),
        "display_only" | "display-only" | "displayonly" | "display" | "hint" | "suggestion" => {
            Some(FixCandidateApplicability::DisplayOnly)
        }
        _ => None,
    }
}

pub(super) fn plan_fix_candidates(
    sql: &str,
    mut candidates: Vec<FixCandidate>,
    protected_ranges: &[PatchProtectedRange],
    allow_unsafe: bool,
) -> PlannedFixes {
    if candidates.is_empty() {
        return PlannedFixes::default();
    }

    candidates.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.end.cmp(&right.end))
            .then_with(|| {
                left.applicability
                    .sort_key()
                    .cmp(&right.applicability.sort_key())
            })
            .then_with(|| {
                fix_candidate_source_priority(left).cmp(&fix_candidate_source_priority(right))
            })
            .then_with(|| left.rule_code.cmp(&right.rule_code))
            .then_with(|| left.replacement.cmp(&right.replacement))
    });
    candidates.dedup_by(|left, right| {
        left.start == right.start
            && left.end == right.end
            && left.replacement == right.replacement
            && left.applicability == right.applicability
            && left.source == right.source
            && left.rule_code == right.rule_code
    });

    let patch_fixes: Vec<PatchFix> = candidates
        .into_iter()
        .enumerate()
        .map(|(idx, candidate)| {
            let rule_code = candidate
                .rule_code
                .clone()
                .unwrap_or_else(|| format!("PATCH_{:?}_{idx}", candidate.source));
            let source_priority = fix_candidate_source_priority(&candidate);
            let mut fix = PatchFix::new(
                rule_code,
                patch_applicability(candidate.applicability),
                vec![PatchEdit::replace(
                    candidate.start,
                    candidate.end,
                    candidate.replacement,
                )],
            );
            fix.priority = source_priority as i32;
            fix
        })
        .collect();

    let mut allowed = vec![PatchApplicability::Safe];
    if allow_unsafe {
        allowed.push(PatchApplicability::Unsafe);
    }

    let plan = plan_fixes(sql, patch_fixes, &allowed, protected_ranges);
    let mut skipped = FixSkippedCounts::default();
    for blocked in &plan.blocked {
        let reasons = &blocked.reasons;
        if reasons.iter().any(|reason| {
            matches!(
                reason,
                BlockedReason::ApplicabilityNotAllowed {
                    applicability: PatchApplicability::Unsafe
                }
            )
        }) {
            skipped.unsafe_skipped += 1;
            continue;
        }
        if reasons.iter().any(|reason| {
            matches!(
                reason,
                BlockedReason::ApplicabilityNotAllowed {
                    applicability: PatchApplicability::DisplayOnly
                }
            )
        }) {
            skipped.display_only += 1;
            continue;
        }
        if reasons
            .iter()
            .any(|reason| matches!(reason, BlockedReason::TouchesProtectedRange { .. }))
        {
            skipped.protected_range_blocked += 1;
            continue;
        }
        skipped.overlap_conflict_blocked += 1;
    }

    PlannedFixes {
        edits: plan.accepted_edits(),
        skipped,
    }
}

pub(super) fn patch_applicability(applicability: FixCandidateApplicability) -> PatchApplicability {
    match applicability {
        FixCandidateApplicability::Safe => PatchApplicability::Safe,
        FixCandidateApplicability::Unsafe => PatchApplicability::Unsafe,
        FixCandidateApplicability::DisplayOnly => PatchApplicability::DisplayOnly,
    }
}

pub(super) fn apply_planned_edits(sql: &str, edits: &[PatchEdit]) -> String {
    apply_patch_edits(sql, edits)
}

pub(super) fn collect_comment_protected_ranges(
    sql: &str,
    dialect: Dialect,
    strict_safety_mode: bool,
) -> Vec<PatchProtectedRange> {
    if !strict_safety_mode {
        return Vec::new();
    }

    derive_protected_ranges(sql, dialect)
        .into_iter()
        .filter(|range| matches!(range.kind, PatchProtectedRangeKind::TemplateTag))
        .collect()
}

pub(super) fn derive_localized_span_edits(original: &str, rewritten: &str) -> Vec<SpanEdit> {
    if original == rewritten {
        return Vec::new();
    }

    let original_chars = original.chars().collect::<Vec<_>>();
    let rewritten_chars = rewritten.chars().collect::<Vec<_>>();

    const MAX_DIFF_MATRIX_CELLS: usize = 2_500_000;
    let matrix_cells = (original_chars.len() + 1).saturating_mul(rewritten_chars.len() + 1);
    if matrix_cells > MAX_DIFF_MATRIX_CELLS {
        return vec![SpanEdit::replace(0, original.len(), rewritten)];
    }

    let diff_steps = diff_steps_via_lcs(&original_chars, &rewritten_chars);
    if diff_steps.is_empty() {
        return Vec::new();
    }

    let original_offsets = char_to_byte_offsets(original);
    let rewritten_offsets = char_to_byte_offsets(rewritten);

    let mut edits = Vec::new();
    let mut original_char_idx = 0usize;
    let mut rewritten_char_idx = 0usize;
    let mut step_idx = 0usize;

    while step_idx < diff_steps.len() {
        if matches!(diff_steps[step_idx], DiffStep::Equal) {
            original_char_idx += 1;
            rewritten_char_idx += 1;
            step_idx += 1;
            continue;
        }

        let edit_original_start = original_char_idx;
        let edit_rewritten_start = rewritten_char_idx;

        while step_idx < diff_steps.len() && !matches!(diff_steps[step_idx], DiffStep::Equal) {
            match diff_steps[step_idx] {
                DiffStep::Delete => original_char_idx += 1,
                DiffStep::Insert => rewritten_char_idx += 1,
                DiffStep::Equal => {}
            }
            step_idx += 1;
        }

        let start = original_offsets[edit_original_start];
        let end = original_offsets[original_char_idx];
        let replacement_start = rewritten_offsets[edit_rewritten_start];
        let replacement_end = rewritten_offsets[rewritten_char_idx];
        edits.push(SpanEdit::replace(
            start,
            end,
            &rewritten[replacement_start..replacement_end],
        ));
    }

    edits
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum DiffStep {
    Equal,
    Delete,
    Insert,
}

pub(super) fn diff_steps_via_lcs(original: &[char], rewritten: &[char]) -> Vec<DiffStep> {
    if original.is_empty() {
        return vec![DiffStep::Insert; rewritten.len()];
    }
    if rewritten.is_empty() {
        return vec![DiffStep::Delete; original.len()];
    }

    let cols = rewritten.len() + 1;
    let mut lcs = vec![0u32; (original.len() + 1) * cols];

    for original_idx in 0..original.len() {
        for rewritten_idx in 0..rewritten.len() {
            let cell = (original_idx + 1) * cols + rewritten_idx + 1;
            lcs[cell] = if original[original_idx] == rewritten[rewritten_idx] {
                lcs[original_idx * cols + rewritten_idx] + 1
            } else {
                lcs[original_idx * cols + rewritten_idx + 1]
                    .max(lcs[(original_idx + 1) * cols + rewritten_idx])
            };
        }
    }

    let mut steps_reversed = Vec::with_capacity(original.len() + rewritten.len());
    let mut original_idx = original.len();
    let mut rewritten_idx = rewritten.len();

    while original_idx > 0 || rewritten_idx > 0 {
        if original_idx > 0
            && rewritten_idx > 0
            && original[original_idx - 1] == rewritten[rewritten_idx - 1]
        {
            steps_reversed.push(DiffStep::Equal);
            original_idx -= 1;
            rewritten_idx -= 1;
            continue;
        }

        let left = if rewritten_idx > 0 {
            lcs[original_idx * cols + rewritten_idx - 1]
        } else {
            0
        };
        let up = if original_idx > 0 {
            lcs[(original_idx - 1) * cols + rewritten_idx]
        } else {
            0
        };

        if rewritten_idx > 0 && (original_idx == 0 || left >= up) {
            steps_reversed.push(DiffStep::Insert);
            rewritten_idx -= 1;
        } else if original_idx > 0 {
            steps_reversed.push(DiffStep::Delete);
            original_idx -= 1;
        }
    }

    steps_reversed.reverse();
    steps_reversed
}

pub(super) fn char_to_byte_offsets(text: &str) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(text.chars().count() + 1);
    offsets.push(0);
    for (idx, ch) in text.char_indices() {
        offsets.push(idx + ch.len_utf8());
    }
    offsets
}
