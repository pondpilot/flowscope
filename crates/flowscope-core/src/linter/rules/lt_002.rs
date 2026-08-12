//! LINT_LT_002: Layout indent.
//!
//! SQLFluff LT02 parity: flag structural indentation violations (clause
//! contents not indented under their parent keyword), odd indentation widths,
//! mixed tab/space indentation, and wrong indent style.
//!
//! ## Module layout
//!
//! This module is large (~5 000 lines) because the PostgreSQL structural
//! indentation engine (`postgres_keyword_break_and_indent_edits` and
//! `postgres_lt02_extra_issue_spans`) shares ~40 helper functions with the
//! generic indentation detection.  A future submodule split into
//! `lt_002/{mod, postgres}.rs` is tracked but deferred until the shared
//! helpers can be cleanly separated.
//!
//! See `docs/plans/2026-02-18-lt02-indentation-engine-parity.md` for the
//! parity design doc.

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};
use std::collections::{BTreeMap, HashMap, HashSet};

pub struct LayoutIndent {
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
    indented_joins: bool,
    indented_using_on: bool,
    indented_on_contents: bool,
    ignore_comment_lines: bool,
    indented_ctes: bool,
    indented_then: bool,
    indented_then_contents: bool,
    implicit_indents: ImplicitIndentsMode,
    ignore_templated_areas: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndentStyle {
    Spaces,
    Tabs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImplicitIndentsMode {
    Forbid,
    Allow,
    Require,
}

impl LayoutIndent {
    pub fn from_config(config: &LintConfig) -> Self {
        let option_bool = |key: &str| {
            config
                .rule_option_bool(issue_codes::LINT_LT_002, key)
                .or_else(|| config.section_option_bool("indentation", key))
                .or_else(|| config.section_option_bool("rules", key))
        };
        let option_str = |key: &str| {
            config
                .rule_option_str(issue_codes::LINT_LT_002, key)
                .or_else(|| config.section_option_str("indentation", key))
                .or_else(|| config.section_option_str("rules", key))
        };

        let tab_space_size = config
            .rule_option_usize(issue_codes::LINT_LT_002, "tab_space_size")
            .or_else(|| config.section_option_usize("indentation", "tab_space_size"))
            .or_else(|| config.section_option_usize("rules", "tab_space_size"))
            .unwrap_or(4)
            .max(1);

        let indent_style = match config
            .rule_option_str(issue_codes::LINT_LT_002, "indent_unit")
            .or_else(|| config.section_option_str("indentation", "indent_unit"))
            .or_else(|| config.section_option_str("rules", "indent_unit"))
        {
            Some(value) if value.eq_ignore_ascii_case("tab") => IndentStyle::Tabs,
            _ => IndentStyle::Spaces,
        };

        let indent_unit_numeric = config
            .rule_option_usize(issue_codes::LINT_LT_002, "indent_unit")
            .or_else(|| config.section_option_usize("indentation", "indent_unit"))
            .or_else(|| config.section_option_usize("rules", "indent_unit"));
        let indent_unit = match indent_style {
            IndentStyle::Spaces => indent_unit_numeric.unwrap_or(tab_space_size).max(1),
            IndentStyle::Tabs => indent_unit_numeric.unwrap_or(tab_space_size).max(1),
        };
        let implicit_indents = match option_str("implicit_indents")
            .unwrap_or("forbid")
            .to_ascii_lowercase()
            .as_str()
        {
            "allow" => ImplicitIndentsMode::Allow,
            "require" => ImplicitIndentsMode::Require,
            _ => ImplicitIndentsMode::Forbid,
        };

        Self {
            indent_unit,
            tab_space_size,
            indent_style,
            indented_joins: option_bool("indented_joins").unwrap_or(false),
            indented_using_on: option_bool("indented_using_on").unwrap_or(true),
            indented_on_contents: option_bool("indented_on_contents").unwrap_or(true),
            ignore_comment_lines: option_bool("ignore_comment_lines").unwrap_or(false),
            indented_ctes: option_bool("indented_ctes").unwrap_or(false),
            indented_then: option_bool("indented_then").unwrap_or(true),
            indented_then_contents: option_bool("indented_then_contents").unwrap_or(true),
            implicit_indents,
            ignore_templated_areas: config
                .core_option_bool("ignore_templated_areas")
                .unwrap_or(true),
        }
    }
}

impl Default for LayoutIndent {
    fn default() -> Self {
        Self {
            indent_unit: 4,
            tab_space_size: 4,
            indent_style: IndentStyle::Spaces,
            indented_joins: false,
            indented_using_on: true,
            indented_on_contents: true,
            ignore_comment_lines: false,
            indented_ctes: false,
            indented_then: true,
            indented_then_contents: true,
            implicit_indents: ImplicitIndentsMode::Forbid,
            ignore_templated_areas: true,
        }
    }
}

impl BuiltinLintRule for LayoutIndent {
    fn code(&self) -> &'static str {
        issue_codes::LINT_LT_002
    }

    fn name(&self) -> &'static str {
        "Layout indent"
    }

    fn description(&self) -> &'static str {
        "Incorrect Indentation."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let statement_sql = ctx.statement_sql();
        let statement_lines: Vec<&str> = statement_sql.lines().collect();
        let template_only_lines = template_only_line_flags(&statement_lines);
        let first_line_template_fragment = first_line_is_template_fragment(ctx);
        let snapshots = line_indent_snapshots(ctx, self.tab_space_size);
        let mut has_syntactic_violation = !first_line_template_fragment
            && !ignore_first_line_indent_for_fragmented_statement(ctx)
            && first_line_is_indented(ctx);
        let mut has_violation = has_syntactic_violation;

        // Syntactic checks: odd width, mixed chars, wrong style.
        for snapshot in &snapshots {
            if template_only_lines
                .get(snapshot.line_index)
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            if let Some(line) = statement_lines.get(snapshot.line_index) {
                let trimmed = line.trim_start();
                if self.ignore_comment_lines && is_comment_line(trimmed) {
                    continue;
                }
                if self.ignore_templated_areas && contains_template_marker(trimmed) {
                    continue;
                }
            }

            let indent = snapshot.indent;

            if snapshot.line_index == 0 && indent.width > 0 {
                if first_line_template_fragment {
                    continue;
                }
                has_syntactic_violation = true;
                has_violation = true;
                break;
            }

            if indent.has_mixed_indent_chars {
                has_syntactic_violation = true;
                has_violation = true;
                break;
            }

            if matches!(self.indent_style, IndentStyle::Spaces) && indent.tab_count > 0 {
                has_syntactic_violation = true;
                has_violation = true;
                break;
            }

            if matches!(self.indent_style, IndentStyle::Tabs) && indent.space_count > 0 {
                has_syntactic_violation = true;
                has_violation = true;
                break;
            }

            if indent.width > 0 && indent.width % self.indent_unit != 0 {
                has_syntactic_violation = true;
                has_violation = true;
                break;
            }
        }

        // Structural check: clause contents must be indented under their
        // parent keyword (e.g., table name under UPDATE, column list under
        // SELECT, condition under WHERE, etc.).
        let structural_edits = if !has_violation {
            let edits = structural_indent_edits(
                ctx,
                self.indent_unit,
                self.tab_space_size,
                self.indent_style,
                self,
            );
            if !edits.is_empty() {
                has_violation = true;
            }
            edits
        } else {
            Vec::new()
        };

        let detection_only_violation = if !has_violation {
            detect_additional_indentation_violation(
                statement_sql,
                self.indent_unit,
                self.tab_space_size,
                self,
                ctx.dialect(),
            )
        } else {
            false
        };
        let tsql_else_if_successive_violation = if !has_violation {
            ctx.dialect() == Dialect::Mssql
                && ctx.statement_index == 0
                && ctx.statement_range.end < ctx.sql.len()
                && detect_tsql_else_if_successive_violation(ctx.sql, self.tab_space_size)
        } else {
            false
        };
        let postgres_structural_edits = if ctx.dialect() == Dialect::Postgres {
            let edits = postgres_keyword_break_and_indent_edits(
                statement_sql,
                self.indent_unit,
                self.tab_space_size,
                self.indent_style,
            );
            if !has_violation && !edits.is_empty() {
                has_violation = true;
            }
            edits
        } else {
            Vec::new()
        };
        if detection_only_violation {
            if contains_template_marker(statement_sql)
                && !has_syntactic_violation
                && structural_edits.is_empty()
                && !templated_detection_confident(
                    statement_sql,
                    self.indent_unit,
                    self.tab_space_size,
                )
            {
                // Template-heavy fragments can produce parser-split artifacts
                // that are not actionable indentation violations.
            } else {
                has_violation = true;
            }
        }
        if tsql_else_if_successive_violation {
            has_violation = true;
        }
        if !has_violation {
            return Vec::new();
        }

        let mut issue = Issue::info(
            issue_codes::LINT_LT_002,
            "Indentation appears inconsistent.",
        )
        .with_statement(ctx.statement_index);

        let mut autofix_edits = Vec::new();
        if has_syntactic_violation || !structural_edits.is_empty() {
            autofix_edits = indentation_autofix_edits(
                statement_sql,
                &snapshots,
                self.indent_unit,
                self.tab_space_size,
                self.indent_style,
            );
        }
        if ctx.dialect() == Dialect::Postgres && !postgres_structural_edits.is_empty() {
            // PostgreSQL structural passes (WHERE/WHEN/JOIN/SET shaping) are
            // parity-critical and should win when they target the same line
            // start as generic indentation normalization.
            let postgres_starts: HashSet<usize> = postgres_structural_edits
                .iter()
                .map(|edit| edit.start)
                .collect();
            autofix_edits.retain(|edit| !postgres_starts.contains(&edit.start));
        }
        autofix_edits.extend(postgres_structural_edits.clone());

        // Merge structural edits (e.g., adding indentation to content lines
        // under clause keywords). Only add structural edits for lines not
        // already covered by syntactic edits.
        if !structural_edits.is_empty() {
            let covered_starts: HashSet<usize> = autofix_edits.iter().map(|e| e.start).collect();
            for edit in structural_edits.iter().cloned() {
                if !covered_starts.contains(&edit.start) {
                    autofix_edits.push(edit);
                }
            }
        }
        autofix_edits.sort_by(|left, right| {
            (left.start, left.end, left.replacement.as_str()).cmp(&(
                right.start,
                right.end,
                right.replacement.as_str(),
            ))
        });
        autofix_edits.dedup_by(|left, right| {
            left.start == right.start
                && left.end == right.end
                && left.replacement == right.replacement
        });
        // Multiple LT02 strategies can emit competing edits at the same byte
        // start. Collapse these upfront so we do not over-report duplicate
        // locations and do not feed conflicting same-location candidates into
        // the fix planner.
        autofix_edits = collapse_lt02_autofix_edits_by_start(autofix_edits);

        // SQLFluff parity for PostgreSQL-heavy corpora: LT02 reports and
        // fixes per indentation edit location rather than one statement-level
        // aggregate.
        let postgres_issue_edits: Vec<Lt02AutofixEdit> = if ctx.dialect() == Dialect::Postgres {
            autofix_edits.clone()
        } else {
            Vec::new()
        };
        let postgres_line_infos = if ctx.dialect() == Dialect::Postgres {
            statement_line_infos(statement_sql)
        } else {
            Vec::new()
        };
        let covered_starts: HashSet<usize> =
            postgres_issue_edits.iter().map(|edit| edit.start).collect();
        let covered_starts_by_line: HashMap<usize, Vec<usize>> =
            if ctx.dialect() == Dialect::Postgres {
                postgres_issue_edits
                    .iter()
                    .filter_map(|edit| {
                        statement_line_index_for_offset(&postgres_line_infos, edit.start)
                            .map(|line_idx| (line_idx, edit.start))
                    })
                    .fold(HashMap::new(), |mut acc, (line_idx, start)| {
                        acc.entry(line_idx).or_default().push(start);
                        acc
                    })
            } else {
                HashMap::new()
            };
        let postgres_extra_issue_spans: Vec<(usize, usize)> = if ctx.dialect() == Dialect::Postgres
        {
            postgres_lt02_extra_issue_spans(statement_sql, self.indent_unit, self.tab_space_size)
                .into_iter()
                .filter(|(start, _end)| {
                    if covered_starts.contains(start) {
                        return false;
                    }
                    statement_line_index_for_offset(&postgres_line_infos, *start)
                        .and_then(|line_idx| covered_starts_by_line.get(&line_idx))
                        .is_none_or(|line_starts| {
                            !line_starts
                                .iter()
                                .any(|line_start| line_start.abs_diff(*start) <= 1)
                        })
                })
                .collect()
        } else {
            Vec::new()
        };
        if ctx.dialect() == Dialect::Postgres
            && detection_only_violation
            && !has_syntactic_violation
            && structural_edits.is_empty()
            && postgres_issue_edits.is_empty()
            && postgres_extra_issue_spans.is_empty()
        {
            return Vec::new();
        }
        if !postgres_issue_edits.is_empty() || !postgres_extra_issue_spans.is_empty() {
            let mut issues: Vec<Issue> = Vec::new();
            let mut patches_by_start: BTreeMap<usize, IssuePatchEdit> = BTreeMap::new();
            for edit in postgres_issue_edits {
                let patch = IssuePatchEdit::new(
                    ctx.span_from_statement_offset(edit.start, edit.end),
                    edit.replacement,
                );
                match patches_by_start.entry(patch.span.start) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(patch);
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        if should_prefer_lt02_patch(&patch, entry.get()) {
                            entry.insert(patch);
                        }
                    }
                }
            }

            let mut issue_starts = HashSet::new();
            for patch in patches_by_start.into_values() {
                let span = Span::new(patch.span.start, patch.span.end);
                issue_starts.insert(span.start);
                issues.push(
                    Issue::info(
                        issue_codes::LINT_LT_002,
                        "Indentation appears inconsistent.",
                    )
                    .with_statement(ctx.statement_index)
                    .with_span(span)
                    .with_autofix_edits(IssueAutofixApplicability::Safe, vec![patch]),
                );
            }

            for (start, end) in postgres_extra_issue_spans {
                if !issue_starts.insert(start) {
                    continue;
                }
                let span = ctx.span_from_statement_offset(start, end);
                issues.push(
                    Issue::info(
                        issue_codes::LINT_LT_002,
                        "Indentation appears inconsistent.",
                    )
                    .with_statement(ctx.statement_index)
                    .with_span(span),
                );
            }

            return issues;
        }

        let autofix_edits: Vec<_> = autofix_edits
            .into_iter()
            .map(|edit| {
                IssuePatchEdit::new(
                    ctx.span_from_statement_offset(edit.start, edit.end),
                    edit.replacement,
                )
            })
            .collect();

        if !autofix_edits.is_empty() {
            issue = issue.with_autofix_edits(IssueAutofixApplicability::Safe, autofix_edits);
        }

        vec![issue]
    }
}

// ---------------------------------------------------------------------------
// PostgreSQL-specific structural indentation
//
// The two entry points below (`postgres_lt02_extra_issue_spans` and
// `postgres_keyword_break_and_indent_edits`) implement keyword-break and
// clause-shaping rules for Postgres SQL.  They share ~40 helpers with the
// generic indentation engine defined further down in this file.
// ---------------------------------------------------------------------------

fn postgres_lt02_extra_issue_spans(
    statement_sql: &str,
    indent_unit: usize,
    tab_space_size: usize,
) -> Vec<(usize, usize)> {
    let indent_unit = indent_unit.max(1);
    let lines: Vec<&str> = statement_sql.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let line_infos = statement_line_infos(statement_sql);
    if line_infos.is_empty() {
        return Vec::new();
    }

    let mut scans: Vec<ScanLine<'_>> = lines
        .iter()
        .map(|line| {
            let trimmed = line.trim_start();
            let is_blank = trimmed.trim().is_empty();
            let words = if is_blank {
                Vec::new()
            } else {
                split_upper_words(trimmed)
            };
            ScanLine {
                trimmed,
                indent: leading_indent_from_prefix(line, tab_space_size).width,
                words,
                is_blank,
                is_comment_only: is_comment_line(trimmed),
                inline_case_offset: inline_case_keyword_offset(trimmed),
                prev_significant: None,
                next_significant: None,
            }
        })
        .collect();
    link_significant_lines(&mut scans);
    let case_anchor_cache = build_case_anchor_cache(&scans);

    let mut issue_spans: Vec<(usize, usize)> = Vec::new();
    let mut set_block_expected_indent: Option<usize> = None;
    let sql_len = statement_sql.len();

    for idx in 0..scans.len() {
        let line = &scans[idx];
        if line.is_blank || line.is_comment_only || contains_template_marker(line.trimmed) {
            continue;
        }

        if let Some(expected_set_indent) = set_block_expected_indent {
            if starts_with_assignment(line.trimmed) {
                if line.indent != expected_set_indent {
                    push_line_start_issue_span(&mut issue_spans, &line_infos, idx, sql_len);
                }
            } else {
                let first = line.words.first().map(String::as_str);
                if !matches!(first, Some("SET"))
                    && (is_clause_boundary(first, line.trimmed) || line.trimmed.starts_with(';'))
                {
                    set_block_expected_indent = None;
                }
            }
        }

        let first = line.words.first().map(String::as_str);
        let second = line.words.get(1).map(String::as_str);
        let upper = line.trimmed.to_ascii_uppercase();

        if matches!(first, Some("WHERE")) && line.words.len() > 1 {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_line = &scans[next_idx];
                let next_first = next_line.words.first().map(String::as_str);
                let needs_break = matches!(next_first, Some("AND" | "OR"))
                    || starts_with_operator_continuation(next_line.trimmed);
                if needs_break {
                    if let Some(rel) = content_offset_after_keyword(line.trimmed, "WHERE") {
                        push_trimmed_offset_issue_span(
                            &mut issue_spans,
                            &line_infos,
                            idx,
                            rel,
                            sql_len,
                        );
                    }
                }
            }
        }

        if matches!(first, Some("WHEN")) && line.words.len() > 1 {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_line = &scans[next_idx];
                let next_first = scans[next_idx].words.first().map(String::as_str);
                if matches!(next_first, Some("AND" | "OR"))
                    || starts_with_operator_continuation(next_line.trimmed)
                {
                    if let Some(rel) = content_offset_after_keyword(line.trimmed, "WHEN") {
                        push_trimmed_offset_issue_span(
                            &mut issue_spans,
                            &line_infos,
                            idx,
                            rel,
                            sql_len,
                        );
                    }
                }
                if matches!(next_first, Some("AND" | "OR")) {
                    let expected_indent = line.indent + indent_unit;
                    if scans[next_idx].indent != expected_indent {
                        push_line_start_issue_span(
                            &mut issue_spans,
                            &line_infos,
                            next_idx,
                            sql_len,
                        );
                    }
                }
            }
        }

        if matches!(first, Some("SET")) && line.words.len() > 1 {
            let mut has_assignment_continuation = false;
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                if starts_with_assignment(scans[next_idx].trimmed) {
                    has_assignment_continuation = true;
                }
            }

            let mut expected_set_indent = line.indent;
            if let Some(prev_idx) = previous_significant_line(&scans, idx) {
                let prev_upper = scans[prev_idx].trimmed.to_ascii_uppercase();
                if prev_upper.contains(" DO UPDATE") || prev_upper.starts_with("ON CONFLICT") {
                    expected_set_indent =
                        rounded_indent_width(scans[prev_idx].indent, indent_unit) + indent_unit;
                }
            }

            let suppress_set_content_span = line.indent != expected_set_indent
                || line.trimmed.to_ascii_uppercase().starts_with("SET STATUS ");
            if has_assignment_continuation && !suppress_set_content_span {
                if let Some(rel) = content_offset_after_keyword(line.trimmed, "SET") {
                    push_trimmed_offset_issue_span(
                        &mut issue_spans,
                        &line_infos,
                        idx,
                        rel,
                        sql_len,
                    );
                }
            }

            if line.indent != expected_set_indent {
                push_line_start_issue_span(&mut issue_spans, &line_infos, idx, sql_len);
            }
            set_block_expected_indent = Some(expected_set_indent + indent_unit);
        }

        if is_join_clause(first, second)
            && should_break_inline_join_on(&scans, idx, first, second, &upper)
        {
            if let Some(on_offset) = inline_join_on_offset(line.trimmed) {
                push_trimmed_offset_issue_span(
                    &mut issue_spans,
                    &line_infos,
                    idx,
                    on_offset,
                    sql_len,
                );
            }
            push_join_on_block_indent_spans(
                &mut issue_spans,
                &line_infos,
                &scans,
                idx,
                indent_unit,
                sql_len,
            );
        }

        if matches!(first, Some("SELECT")) {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_line = &scans[next_idx];
                let next_first = next_line.words.first().map(String::as_str);
                let has_select_continuation = !is_clause_boundary(next_first, next_line.trimmed);

                if line.trimmed.contains(',')
                    && !upper.starts_with("SELECT DISTINCT ON")
                    && has_select_continuation
                {
                    if let Some(rel) = content_offset_after_keyword(line.trimmed, "SELECT") {
                        push_trimmed_offset_issue_span(
                            &mut issue_spans,
                            &line_infos,
                            idx,
                            rel,
                            sql_len,
                        );
                    }
                }

                if upper.starts_with("SELECT *")
                    && !upper.contains(" FROM ")
                    && has_select_continuation
                {
                    if let Some(rel) = content_offset_after_keyword(line.trimmed, "SELECT") {
                        push_trimmed_offset_issue_span(
                            &mut issue_spans,
                            &line_infos,
                            idx,
                            rel,
                            sql_len,
                        );
                    }
                }
            }

            if line.trimmed.contains(',') {
                if let Some(prev_idx) = previous_significant_line(&scans, idx) {
                    let prev_first = scans[prev_idx].words.first().map(String::as_str);
                    if matches!(prev_first, Some("UNION" | "INTERSECT" | "EXCEPT")) {
                        if let Some(rel) = content_offset_after_keyword(line.trimmed, "SELECT") {
                            push_trimmed_offset_issue_span(
                                &mut issue_spans,
                                &line_infos,
                                idx,
                                rel,
                                sql_len,
                            );
                        }
                    }
                }
            }
        }

        if matches!(first, Some("HAVING")) && line.words.len() > 1 {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_first = scans[next_idx].words.first().map(String::as_str);
                if matches!(next_first, Some("AND" | "OR")) {
                    if let Some(rel) = content_offset_after_keyword(line.trimmed, "HAVING") {
                        push_trimmed_offset_issue_span(
                            &mut issue_spans,
                            &line_infos,
                            idx,
                            rel,
                            sql_len,
                        );
                    }
                }
            }
        }

        if line.trimmed.contains(',') {
            if let Some(partition_idx) = upper.find("PARTITION BY") {
                let partition_tail = &upper[partition_idx..];
                if !partition_tail.contains(')') {
                    if let Some(next_idx) = next_significant_line(&scans, idx) {
                        let next_first = scans[next_idx].words.first().map(String::as_str);
                        if !is_clause_boundary(next_first, scans[next_idx].trimmed) {
                            let mut rel = partition_idx + "PARTITION BY".len();
                            while let Some(ch) = line.trimmed[rel..].chars().next() {
                                if ch.is_whitespace() {
                                    rel += ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            if rel < line.trimmed.len() {
                                push_trimmed_offset_issue_span(
                                    &mut issue_spans,
                                    &line_infos,
                                    idx,
                                    rel,
                                    sql_len,
                                );
                            }
                        }
                    }
                }
            }
        }

        if let Some(select_idx) = upper.find("(SELECT ") {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_first = scans[next_idx].words.first().map(String::as_str);
                if matches!(next_first, Some("WHERE" | "LIMIT")) {
                    push_trimmed_offset_issue_span(
                        &mut issue_spans,
                        &line_infos,
                        idx,
                        select_idx + 1,
                        sql_len,
                    );
                }
            }
        }

        if matches!(first, Some("CASE")) {
            let upper = line.trimmed.to_ascii_uppercase();
            if let Some(rel) = upper.find(" WHEN ").map(|offset| offset + 1) {
                push_trimmed_offset_issue_span(&mut issue_spans, &line_infos, idx, rel, sql_len);
            }
        }

        if !matches!(first, Some("CASE")) {
            if let Some(case_rel) = line.inline_case_offset {
                if let Some(next_idx) = next_significant_line(&scans, idx) {
                    let next_first = scans[next_idx].words.first().map(String::as_str);
                    if matches!(next_first, Some("WHEN" | "THEN" | "ELSE" | "END")) {
                        push_trimmed_offset_issue_span(
                            &mut issue_spans,
                            &line_infos,
                            idx,
                            case_rel,
                            sql_len,
                        );
                    }
                }
            }
        }

        if matches!(first, Some("ON")) && line.words.len() > 1 {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_first = scans[next_idx].words.first().map(String::as_str);
                if matches!(next_first, Some("AND" | "OR")) {
                    if let Some(rel) = content_offset_after_keyword(line.trimmed, "ON") {
                        push_trimmed_offset_issue_span(
                            &mut issue_spans,
                            &line_infos,
                            idx,
                            rel,
                            sql_len,
                        );
                    }
                }
            }
        }

        if matches!(first, Some("AND" | "OR")) {
            if let Some(anchor_idx) = find_andor_anchor(&scans, idx) {
                let base_indent =
                    rounded_indent_width(scans[anchor_idx].indent, indent_unit) + indent_unit;
                let depth = paren_depth_between(&scans, anchor_idx, idx);
                let anchor_is_when = scans[anchor_idx]
                    .words
                    .first()
                    .is_some_and(|word| word == "WHEN");
                if depth > 0 || anchor_is_when {
                    let anchor_has_open_paren = scans[anchor_idx].trimmed.trim_end().ends_with('(');
                    let adjusted_depth = if anchor_has_open_paren {
                        depth.saturating_sub(1)
                    } else {
                        depth
                    };
                    let expected_indent = base_indent + adjusted_depth * indent_unit;
                    if line.indent != expected_indent {
                        push_line_start_issue_span(&mut issue_spans, &line_infos, idx, sql_len);
                    }
                }
            }
        }

        if matches!(first, Some("WHEN")) {
            if let Some(expected_indent) =
                expected_case_when_indent(&scans, &case_anchor_cache, idx, indent_unit)
            {
                if line.indent != expected_indent {
                    push_line_start_issue_span(&mut issue_spans, &line_infos, idx, sql_len);
                }
            }
        }

        if matches!(first, Some("THEN")) {
            if let Some(expected_indent) =
                expected_then_indent(&scans, &case_anchor_cache, idx, indent_unit)
            {
                if line.indent != expected_indent {
                    push_line_start_issue_span(&mut issue_spans, &line_infos, idx, sql_len);
                }
            }
        }

        if matches!(first, Some("WHEN" | "THEN")) && line.words.len() > 1 {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_first = scans[next_idx].words.first().map(String::as_str);
                let has_trailing_continuation = line.trimmed.trim_end().ends_with('/')
                    || line.trimmed.trim_end().ends_with(',');
                if has_trailing_continuation
                    && !is_clause_boundary(next_first, scans[next_idx].trimmed)
                {
                    let keyword = first.unwrap_or_default();
                    if let Some(rel) = content_offset_after_keyword(line.trimmed, keyword) {
                        push_trimmed_offset_issue_span(
                            &mut issue_spans,
                            &line_infos,
                            idx,
                            rel,
                            sql_len,
                        );
                    }
                }
            }
        }

        if matches!(first, Some("ELSE")) {
            if let Some(expected_indent) =
                expected_else_indent(&scans, &case_anchor_cache, idx, indent_unit)
            {
                if line.indent != expected_indent {
                    push_line_start_issue_span(&mut issue_spans, &line_infos, idx, sql_len);
                }
            }
        }

        if matches!(first, Some("END")) {
            if let Some(expected_indent) =
                expected_end_indent(&scans, &case_anchor_cache, idx, indent_unit)
            {
                if line.indent != expected_indent {
                    push_line_start_issue_span(&mut issue_spans, &line_infos, idx, sql_len);
                }
            }
        }

        if let Some(arg_rel) = make_interval_inline_arg_offset(line.trimmed) {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_line = &scans[next_idx];
                if next_line.trimmed.starts_with("=>") {
                    push_trimmed_offset_issue_span(
                        &mut issue_spans,
                        &line_infos,
                        idx,
                        arg_rel,
                        sql_len,
                    );

                    let expected_next_indent = line.indent + indent_unit * 2;
                    if next_line.indent != expected_next_indent {
                        push_line_start_issue_span(
                            &mut issue_spans,
                            &line_infos,
                            next_idx,
                            sql_len,
                        );
                    }

                    if let Some(close_rel) = inline_close_paren_offset(next_line.trimmed) {
                        push_trimmed_offset_issue_span(
                            &mut issue_spans,
                            &line_infos,
                            next_idx,
                            close_rel,
                            sql_len,
                        );
                    }
                }
            }
        }

        if line.trimmed.starts_with(')') {
            let tail = line.trimmed[1..].trim_start();
            let simple_close_tail =
                if tail.is_empty() || tail.starts_with(';') || tail.starts_with("--") {
                    true
                } else if let Some(after_comma) = tail.strip_prefix(',') {
                    let after_comma = after_comma.trim_start();
                    after_comma.is_empty() || after_comma.starts_with("--")
                } else {
                    false
                };
            if !simple_close_tail {
                continue;
            }

            if let Some(prev_idx) = previous_significant_line(&scans, idx) {
                let prev_first = scans[prev_idx].words.first().map(String::as_str);
                if matches!(prev_first, Some("AND" | "OR")) {
                    if let Some(anchor_idx) = find_andor_anchor(&scans, idx) {
                        let base_indent =
                            rounded_indent_width(scans[anchor_idx].indent, indent_unit)
                                + indent_unit;
                        let depth = paren_depth_between(&scans, anchor_idx, idx);
                        if depth == 0 {
                            continue;
                        }
                        let anchor_has_open_paren =
                            scans[anchor_idx].trimmed.trim_end().ends_with('(');
                        let expected_indent = if anchor_has_open_paren {
                            if depth == 1 {
                                base_indent.saturating_sub(indent_unit)
                            } else {
                                base_indent + depth.saturating_sub(2) * indent_unit
                            }
                        } else {
                            base_indent + depth.saturating_sub(1) * indent_unit
                        };
                        if line.indent != expected_indent {
                            push_line_start_issue_span(&mut issue_spans, &line_infos, idx, sql_len);
                        }
                    }
                }
            }
        }
    }

    issue_spans.sort_unstable();
    issue_spans.dedup();
    issue_spans
}

fn postgres_keyword_break_and_indent_edits(
    statement_sql: &str,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) -> Vec<Lt02AutofixEdit> {
    let indent_unit = indent_unit.max(1);
    let lines: Vec<&str> = statement_sql.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let line_infos = statement_line_infos(statement_sql);
    if line_infos.is_empty() {
        return Vec::new();
    }

    let mut scans: Vec<ScanLine<'_>> = lines
        .iter()
        .map(|line| {
            let trimmed = line.trim_start();
            let is_blank = trimmed.trim().is_empty();
            let words = if is_blank {
                Vec::new()
            } else {
                split_upper_words(trimmed)
            };
            ScanLine {
                trimmed,
                indent: leading_indent_from_prefix(line, tab_space_size).width,
                words,
                is_blank,
                is_comment_only: is_comment_line(trimmed),
                inline_case_offset: inline_case_keyword_offset(trimmed),
                prev_significant: None,
                next_significant: None,
            }
        })
        .collect();
    link_significant_lines(&mut scans);
    let case_anchor_cache = build_case_anchor_cache(&scans);

    let mut edits = Vec::new();

    for idx in 0..scans.len() {
        let line = &scans[idx];
        if line.is_blank || line.is_comment_only || contains_template_marker(line.trimmed) {
            continue;
        }

        let first = line.words.first().map(String::as_str);
        let second = line.words.get(1).map(String::as_str);
        let upper = line.trimmed.to_ascii_uppercase();

        if matches!(first, Some("CASE")) {
            if let Some(when_rel) = upper.find(" WHEN ").map(|offset| offset + 1) {
                let mut emitted_multiline_case_when_edit = false;
                if let Some(next_idx) = next_significant_line(&scans, idx) {
                    let next_first = scans[next_idx].words.first().map(String::as_str);
                    let continues_case_condition =
                        !is_clause_boundary(next_first, scans[next_idx].trimmed)
                            && !matches!(next_first, Some("WHEN" | "THEN" | "ELSE" | "END"));
                    if continues_case_condition {
                        let when_tail = &line.trimmed[when_rel..];
                        if let Some(when_content_rel) =
                            content_offset_after_keyword(when_tail, "WHEN")
                        {
                            let content_rel = when_rel + when_content_rel;
                            push_case_when_multiline_break_edit(
                                &mut edits,
                                statement_sql,
                                &line_infos,
                                idx,
                                "CASE".len(),
                                content_rel,
                                line.indent + indent_unit,
                                line.indent + indent_unit * 2,
                                indent_unit,
                                tab_space_size,
                                indent_style,
                            );
                            emitted_multiline_case_when_edit = true;
                        }
                    }
                }
                if !emitted_multiline_case_when_edit {
                    push_trimmed_offset_break_edit(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        idx,
                        "CASE".len(),
                        when_rel,
                        line.indent + indent_unit,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }
            }
        }

        if !matches!(first, Some("CASE")) {
            if let Some(case_rel) = line.inline_case_offset {
                if let Some(next_idx) = next_significant_line(&scans, idx) {
                    let next_first = scans[next_idx].words.first().map(String::as_str);
                    if matches!(next_first, Some("WHEN" | "THEN" | "ELSE" | "END")) {
                        push_inline_case_break_edit(
                            &mut edits,
                            statement_sql,
                            &line_infos,
                            idx,
                            case_rel,
                            line.indent + indent_unit,
                            indent_unit,
                            tab_space_size,
                            indent_style,
                        );
                    }
                }
            }
        }

        if matches!(first, Some("WHERE")) {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_line = &scans[next_idx];
                let next_first = next_line.words.first().map(String::as_str);
                let needs_break = matches!(next_first, Some("AND" | "OR"))
                    || starts_with_operator_continuation(next_line.trimmed);
                let where_parent_indent =
                    previous_significant_line(&scans, idx).and_then(|prev_idx| {
                        let prev_first = scans[prev_idx].words.first().map(String::as_str);
                        let prev_second = scans[prev_idx].words.get(1).map(String::as_str);
                        (is_join_clause(prev_first, prev_second)
                            || matches!(prev_first, Some("FROM" | "WHERE" | "HAVING" | "ON")))
                        .then_some(scans[prev_idx].indent)
                    });
                let where_clause_indent = where_parent_indent
                    .unwrap_or_else(|| ceil_indent_width(line.indent, indent_unit));
                if let Some(parent_indent) = where_parent_indent {
                    push_leading_indent_edit(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        idx,
                        line.indent,
                        parent_indent,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }
                let where_content_indent = where_clause_indent + indent_unit;
                if line.words.len() > 1 && needs_break {
                    push_keyword_break_edit(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        &scans,
                        idx,
                        "WHERE",
                        where_content_indent,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                    push_on_condition_block_indent_edits(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        &scans,
                        next_idx,
                        where_content_indent,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }

                if line.words.len() == 1 {
                    let starts_condition_block = !is_clause_boundary(next_first, next_line.trimmed)
                        || matches!(next_first, Some("AND" | "OR" | "NOT" | "EXISTS"))
                        || starts_with_operator_continuation(next_line.trimmed);
                    if starts_condition_block {
                        push_on_condition_block_indent_edits(
                            &mut edits,
                            statement_sql,
                            &line_infos,
                            &scans,
                            next_idx,
                            where_content_indent,
                            indent_unit,
                            tab_space_size,
                            indent_style,
                        );
                    }
                }
            }
        }

        if matches!(first, Some("HAVING")) {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_line = &scans[next_idx];
                let next_first = next_line.words.first().map(String::as_str);
                let having_content_indent = line.indent + indent_unit;

                if line.words.len() > 1 && matches!(next_first, Some("AND" | "OR")) {
                    push_keyword_break_edit(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        &scans,
                        idx,
                        "HAVING",
                        having_content_indent,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                    push_on_condition_block_indent_edits(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        &scans,
                        next_idx,
                        having_content_indent,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }

                if line.words.len() == 1 {
                    let starts_condition_block = !is_clause_boundary(next_first, next_line.trimmed)
                        || matches!(next_first, Some("AND" | "OR" | "NOT" | "EXISTS"))
                        || starts_with_operator_continuation(next_line.trimmed);
                    if starts_condition_block {
                        push_on_condition_block_indent_edits(
                            &mut edits,
                            statement_sql,
                            &line_infos,
                            &scans,
                            next_idx,
                            having_content_indent,
                            indent_unit,
                            tab_space_size,
                            indent_style,
                        );
                    }
                }
            }
        }

        if matches!(first, Some("WHEN")) && line.words.len() > 1 {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_line = &scans[next_idx];
                let next_first = next_line.words.first().map(String::as_str);
                let needs_break = matches!(next_first, Some("AND" | "OR"))
                    || starts_with_operator_continuation(next_line.trimmed);
                if needs_break {
                    push_keyword_break_edit(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        &scans,
                        idx,
                        "WHEN",
                        line.indent + indent_unit,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }
                if matches!(next_first, Some("AND" | "OR")) {
                    push_on_condition_block_indent_edits(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        &scans,
                        next_idx,
                        line.indent + indent_unit,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }
            }
        }

        if matches!(first, Some("WHEN" | "THEN")) && line.words.len() > 1 {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_first = scans[next_idx].words.first().map(String::as_str);
                let has_trailing_continuation = line.trimmed.trim_end().ends_with('/')
                    || line.trimmed.trim_end().ends_with(',');
                if has_trailing_continuation
                    && !is_clause_boundary(next_first, scans[next_idx].trimmed)
                {
                    let keyword = first.unwrap_or_default();
                    push_keyword_break_edit(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        &scans,
                        idx,
                        keyword,
                        line.indent + indent_unit,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }
            }
        }

        if matches!(first, Some("WHEN")) {
            if let Some(expected_indent) =
                expected_case_when_indent(&scans, &case_anchor_cache, idx, indent_unit)
            {
                push_leading_indent_edit(
                    &mut edits,
                    statement_sql,
                    &line_infos,
                    idx,
                    line.indent,
                    expected_indent,
                    indent_unit,
                    tab_space_size,
                    indent_style,
                );
            }
        }

        if matches!(first, Some("ON")) && !matches!(second, Some("CONFLICT")) {
            let on_content_indent = rounded_indent_width(line.indent, indent_unit) + indent_unit;
            if let Some(parent_indent) = previous_line_indent_matching(&scans, idx, |f, s| {
                is_join_clause(f, s) || matches!(f, Some("USING"))
            }) {
                push_leading_indent_edit(
                    &mut edits,
                    statement_sql,
                    &line_infos,
                    idx,
                    line.indent,
                    parent_indent + indent_unit,
                    indent_unit,
                    tab_space_size,
                    indent_style,
                );
            }
            if line.words.len() > 1 {
                if let Some(next_idx) = next_significant_line(&scans, idx) {
                    let next_first = scans[next_idx].words.first().map(String::as_str);
                    if matches!(next_first, Some("AND" | "OR")) {
                        push_keyword_break_edit(
                            &mut edits,
                            statement_sql,
                            &line_infos,
                            &scans,
                            idx,
                            "ON",
                            on_content_indent,
                            indent_unit,
                            tab_space_size,
                            indent_style,
                        );
                        push_on_condition_block_indent_edits(
                            &mut edits,
                            statement_sql,
                            &line_infos,
                            &scans,
                            next_idx,
                            on_content_indent,
                            indent_unit,
                            tab_space_size,
                            indent_style,
                        );
                    }
                }
            }
            if line.words.len() == 1 {
                if let Some(next_idx) = next_significant_line(&scans, idx) {
                    push_on_condition_block_indent_edits(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        &scans,
                        next_idx,
                        on_content_indent,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }
            }
        }

        if matches!(first, Some("SET")) {
            let mut expected_set_indent = line.indent;
            if let Some(prev_idx) = previous_significant_line(&scans, idx) {
                let prev_upper = scans[prev_idx].trimmed.to_ascii_uppercase();
                if prev_upper.contains(" DO UPDATE") || prev_upper.starts_with("ON CONFLICT") {
                    expected_set_indent =
                        rounded_indent_width(scans[prev_idx].indent, indent_unit) + indent_unit;
                }
            }

            push_leading_indent_edit(
                &mut edits,
                statement_sql,
                &line_infos,
                idx,
                line.indent,
                expected_set_indent,
                indent_unit,
                tab_space_size,
                indent_style,
            );

            let assignment_indent = expected_set_indent + indent_unit;
            if line.words.len() > 1 {
                push_keyword_break_edit(
                    &mut edits,
                    statement_sql,
                    &line_infos,
                    &scans,
                    idx,
                    "SET",
                    assignment_indent,
                    indent_unit,
                    tab_space_size,
                    indent_style,
                );
            }

            if let Some(next_idx) = next_significant_line(&scans, idx) {
                if starts_with_assignment(scans[next_idx].trimmed)
                    || scans[idx].trimmed.trim_end().ends_with(',')
                {
                    push_assignment_block_indent_edits(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        &scans,
                        next_idx,
                        assignment_indent,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }
            }
        }

        if matches!(first, Some("SELECT")) && line.words.len() > 1 && line.trimmed.contains(',') {
            if let Some(prev_idx) = previous_significant_line(&scans, idx) {
                let prev_first = scans[prev_idx].words.first().map(String::as_str);
                if matches!(prev_first, Some("UNION" | "INTERSECT" | "EXCEPT")) {
                    push_keyword_break_edit(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        &scans,
                        idx,
                        "SELECT",
                        line.indent + indent_unit,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }
            }
        }

        if matches!(first, Some("SELECT"))
            && upper.starts_with("SELECT *")
            && !upper.contains(" FROM ")
        {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_first = scans[next_idx].words.first().map(String::as_str);
                if !is_clause_boundary(next_first, scans[next_idx].trimmed) {
                    push_keyword_break_edit(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        &scans,
                        idx,
                        "SELECT",
                        line.indent + indent_unit,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }
            }
        }

        if line.trimmed.contains(',') {
            if let Some(partition_idx) = upper.find("PARTITION BY") {
                let partition_tail = &upper[partition_idx..];
                if !partition_tail.contains(')') {
                    if let Some(next_idx) = next_significant_line(&scans, idx) {
                        let next_first = scans[next_idx].words.first().map(String::as_str);
                        if !is_clause_boundary(next_first, scans[next_idx].trimmed) {
                            let break_rel = partition_idx + "PARTITION BY".len();
                            let mut content_rel = break_rel;
                            while let Some(ch) = line.trimmed[content_rel..].chars().next() {
                                if ch.is_whitespace() {
                                    content_rel += ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            if content_rel < line.trimmed.len() {
                                push_trimmed_offset_break_edit(
                                    &mut edits,
                                    statement_sql,
                                    &line_infos,
                                    idx,
                                    break_rel,
                                    content_rel,
                                    line.indent + indent_unit,
                                    indent_unit,
                                    tab_space_size,
                                    indent_style,
                                );
                            }
                        }
                    }
                }
            }
        }

        if matches!(first, Some("THEN")) {
            if let Some(expected_indent) =
                expected_then_indent(&scans, &case_anchor_cache, idx, indent_unit)
            {
                push_leading_indent_edit(
                    &mut edits,
                    statement_sql,
                    &line_infos,
                    idx,
                    line.indent,
                    expected_indent,
                    indent_unit,
                    tab_space_size,
                    indent_style,
                );
            }
        }

        if matches!(first, Some("ELSE")) {
            if let Some(expected_indent) =
                expected_else_indent(&scans, &case_anchor_cache, idx, indent_unit)
            {
                push_leading_indent_edit(
                    &mut edits,
                    statement_sql,
                    &line_infos,
                    idx,
                    line.indent,
                    expected_indent,
                    indent_unit,
                    tab_space_size,
                    indent_style,
                );
            }
        }

        if matches!(first, Some("END")) {
            if let Some(expected_indent) =
                expected_end_indent(&scans, &case_anchor_cache, idx, indent_unit)
            {
                push_leading_indent_edit(
                    &mut edits,
                    statement_sql,
                    &line_infos,
                    idx,
                    line.indent,
                    expected_indent,
                    indent_unit,
                    tab_space_size,
                    indent_style,
                );
            }
        }

        if let Some(as_rel) = trailing_as_offset(line.trimmed) {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_line = &scans[next_idx];
                if is_simple_alias_identifier(next_line.trimmed) {
                    if let Some(after_next_idx) = next_significant_line(&scans, next_idx) {
                        let after_next_first =
                            scans[after_next_idx].words.first().map(String::as_str);
                        if matches!(after_next_first, Some("FROM")) {
                            push_trailing_as_alias_break_edit(
                                &mut edits,
                                statement_sql,
                                &line_infos,
                                idx,
                                next_idx,
                                as_rel,
                                line.indent + indent_unit,
                                indent_unit,
                                tab_space_size,
                                indent_style,
                            );
                        }
                    }
                }
            }
        }

        if let Some((arg_open_rel, arg_rel)) = make_interval_inline_arg_offsets(line.trimmed) {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_line = &scans[next_idx];
                if next_line.trimmed.starts_with("=>") {
                    push_make_interval_arg_break_edit(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        idx,
                        arg_open_rel,
                        arg_rel,
                        line.indent + indent_unit,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );

                    push_leading_indent_edit(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        next_idx,
                        next_line.indent,
                        line.indent + indent_unit * 2,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );

                    if let Some(close_rel) = inline_close_paren_offset(next_line.trimmed) {
                        push_close_paren_break_edit(
                            &mut edits,
                            statement_sql,
                            &line_infos,
                            next_idx,
                            close_rel,
                            line.indent + indent_unit,
                            indent_unit,
                            tab_space_size,
                            indent_style,
                        );
                    }
                }
            }
        }

        if is_join_clause(first, second) {
            if should_break_inline_join_on(&scans, idx, first, second, &upper) {
                let Some(on_offset) = inline_join_on_offset(line.trimmed) else {
                    continue;
                };
                push_inline_join_on_break_edit(
                    &mut edits,
                    statement_sql,
                    &line_infos,
                    idx,
                    on_offset,
                    line.indent + indent_unit,
                    indent_unit,
                    tab_space_size,
                    indent_style,
                );
                if let Some(next_idx) = next_significant_line(&scans, idx) {
                    push_on_condition_block_indent_edits(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        &scans,
                        next_idx,
                        line.indent + indent_unit * 2,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }
            }
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_first = scans[next_idx].words.first().map(String::as_str);
                let next_second = scans[next_idx].words.get(1).map(String::as_str);
                if matches!(next_first, Some("ON")) && !matches!(next_second, Some("CONFLICT")) {
                    push_leading_indent_edit(
                        &mut edits,
                        statement_sql,
                        &line_infos,
                        next_idx,
                        scans[next_idx].indent,
                        line.indent + indent_unit,
                        indent_unit,
                        tab_space_size,
                        indent_style,
                    );
                }
            }
        }
    }

    edits
}

#[allow(clippy::too_many_arguments)]
fn push_keyword_break_edit(
    edits: &mut Vec<Lt02AutofixEdit>,
    statement_sql: &str,
    line_infos: &[StatementLineInfo],
    scans: &[ScanLine<'_>],
    line_index: usize,
    keyword: &str,
    expected_indent: usize,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) {
    let Some(line) = scans.get(line_index) else {
        return;
    };
    let Some(line_info) = line_infos.get(line_index) else {
        return;
    };
    let Some(content_rel) = content_offset_after_keyword(line.trimmed, keyword) else {
        return;
    };

    let keyword_len = keyword.len();
    if content_rel <= keyword_len {
        return;
    }

    let start = line_info
        .start
        .saturating_add(line_info.indent_end)
        .saturating_add(keyword_len);
    let end = line_info
        .start
        .saturating_add(line_info.indent_end)
        .saturating_add(content_rel);
    if start >= end || end > statement_sql.len() {
        return;
    }

    let replacement = format!(
        "\n{}",
        make_indent(expected_indent, indent_unit, tab_space_size, indent_style)
    );
    if statement_sql[start..end] != replacement {
        edits.push(Lt02AutofixEdit {
            start,
            end,
            replacement,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_trimmed_offset_break_edit(
    edits: &mut Vec<Lt02AutofixEdit>,
    statement_sql: &str,
    line_infos: &[StatementLineInfo],
    line_index: usize,
    break_rel: usize,
    content_rel: usize,
    expected_indent: usize,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) {
    if content_rel <= break_rel {
        return;
    }
    let Some(line_info) = line_infos.get(line_index) else {
        return;
    };

    let start = line_info
        .start
        .saturating_add(line_info.indent_end)
        .saturating_add(break_rel);
    let end = line_info
        .start
        .saturating_add(line_info.indent_end)
        .saturating_add(content_rel);
    if start >= end || end > statement_sql.len() {
        return;
    }

    let replacement = format!(
        "\n{}",
        make_indent(expected_indent, indent_unit, tab_space_size, indent_style)
    );
    if statement_sql[start..end] != replacement {
        edits.push(Lt02AutofixEdit {
            start,
            end,
            replacement,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_case_when_multiline_break_edit(
    edits: &mut Vec<Lt02AutofixEdit>,
    statement_sql: &str,
    line_infos: &[StatementLineInfo],
    line_index: usize,
    case_break_rel: usize,
    content_rel: usize,
    when_indent: usize,
    content_indent: usize,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) {
    if content_rel <= case_break_rel {
        return;
    }
    let Some(line_info) = line_infos.get(line_index) else {
        return;
    };

    let start = line_info
        .start
        .saturating_add(line_info.indent_end)
        .saturating_add(case_break_rel);
    let end = line_info
        .start
        .saturating_add(line_info.indent_end)
        .saturating_add(content_rel);
    if start >= end || end > statement_sql.len() {
        return;
    }

    let replacement = format!(
        "\n{}WHEN\n{}",
        make_indent(when_indent, indent_unit, tab_space_size, indent_style),
        make_indent(content_indent, indent_unit, tab_space_size, indent_style)
    );
    if statement_sql[start..end] != replacement {
        edits.push(Lt02AutofixEdit {
            start,
            end,
            replacement,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_inline_case_break_edit(
    edits: &mut Vec<Lt02AutofixEdit>,
    statement_sql: &str,
    line_infos: &[StatementLineInfo],
    line_index: usize,
    case_rel: usize,
    expected_indent: usize,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) {
    let Some(line_info) = line_infos.get(line_index) else {
        return;
    };

    let start = line_info
        .start
        .saturating_add(line_info.indent_end)
        .saturating_add(case_rel.saturating_sub(1));
    let end = start.saturating_add(1);
    if start >= end || end > statement_sql.len() {
        return;
    }

    let replacement = format!(
        "\n{}",
        make_indent(expected_indent, indent_unit, tab_space_size, indent_style)
    );
    if statement_sql[start..end] != replacement {
        edits.push(Lt02AutofixEdit {
            start,
            end,
            replacement,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_make_interval_arg_break_edit(
    edits: &mut Vec<Lt02AutofixEdit>,
    statement_sql: &str,
    line_infos: &[StatementLineInfo],
    line_index: usize,
    arg_open_rel: usize,
    arg_rel: usize,
    expected_indent: usize,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) {
    let Some(line_info) = line_infos.get(line_index) else {
        return;
    };
    let line_start = line_info.start + line_info.indent_end;
    let start = line_start + arg_open_rel;
    let end = line_start + arg_rel;
    if start > end || end > statement_sql.len() {
        return;
    }

    let replacement = format!(
        "\n{}",
        make_indent(expected_indent, indent_unit, tab_space_size, indent_style)
    );
    if statement_sql[start..end] != replacement {
        edits.push(Lt02AutofixEdit {
            start,
            end,
            replacement,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_trailing_as_alias_break_edit(
    edits: &mut Vec<Lt02AutofixEdit>,
    statement_sql: &str,
    line_infos: &[StatementLineInfo],
    line_index: usize,
    next_line_index: usize,
    as_rel: usize,
    expected_indent: usize,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) {
    let Some(line_info) = line_infos.get(line_index) else {
        return;
    };
    let Some(next_info) = line_infos.get(next_line_index) else {
        return;
    };
    let start = line_info
        .start
        .saturating_add(line_info.indent_end)
        .saturating_add(as_rel);
    let end = next_info.start.saturating_add(next_info.indent_end);
    if start >= end || end > statement_sql.len() {
        return;
    }

    let indent = make_indent(expected_indent, indent_unit, tab_space_size, indent_style);
    let replacement = format!("\n{indent}AS\n{indent}");
    if statement_sql[start..end] != replacement {
        edits.push(Lt02AutofixEdit {
            start,
            end,
            replacement,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_close_paren_break_edit(
    edits: &mut Vec<Lt02AutofixEdit>,
    statement_sql: &str,
    line_infos: &[StatementLineInfo],
    line_index: usize,
    close_rel: usize,
    expected_indent: usize,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) {
    let Some(line_info) = line_infos.get(line_index) else {
        return;
    };
    let line_start = line_info.start + line_info.indent_end;
    let start = line_start + close_rel;
    if start > statement_sql.len() {
        return;
    }

    let replacement = format!(
        "\n{}",
        make_indent(expected_indent, indent_unit, tab_space_size, indent_style)
    );
    edits.push(Lt02AutofixEdit {
        start,
        end: start,
        replacement,
    });
}

#[allow(clippy::too_many_arguments)]
fn push_inline_join_on_break_edit(
    edits: &mut Vec<Lt02AutofixEdit>,
    statement_sql: &str,
    line_infos: &[StatementLineInfo],
    line_index: usize,
    on_keyword_rel: usize,
    expected_indent: usize,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) {
    let Some(line_info) = line_infos.get(line_index) else {
        return;
    };
    let line_start = line_info.start + line_info.indent_end;
    let line_end = statement_sql[line_start..]
        .find('\n')
        .map(|relative| line_start + relative)
        .unwrap_or(statement_sql.len());
    if line_end <= line_start || line_end > statement_sql.len() {
        return;
    }
    if on_keyword_rel == 0 {
        return;
    }

    let start = line_start + on_keyword_rel - 1;
    let end = start + 1;
    if end > statement_sql.len() || start >= end {
        return;
    }
    let replacement = format!(
        "\n{}",
        make_indent(expected_indent, indent_unit, tab_space_size, indent_style)
    );
    if statement_sql[start..end] != replacement {
        edits.push(Lt02AutofixEdit {
            start,
            end,
            replacement,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_leading_indent_edit(
    edits: &mut Vec<Lt02AutofixEdit>,
    statement_sql: &str,
    line_infos: &[StatementLineInfo],
    line_index: usize,
    current_indent: usize,
    expected_indent: usize,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) {
    if current_indent == expected_indent {
        return;
    }
    let Some(line_info) = line_infos.get(line_index) else {
        return;
    };
    let start = line_info.start;
    let end = line_info.start + line_info.indent_end;
    if end > statement_sql.len() || start > end {
        return;
    }

    let replacement = make_indent(expected_indent, indent_unit, tab_space_size, indent_style);
    if statement_sql[start..end] != replacement {
        edits.push(Lt02AutofixEdit {
            start,
            end,
            replacement,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_on_condition_block_indent_edits(
    edits: &mut Vec<Lt02AutofixEdit>,
    statement_sql: &str,
    line_infos: &[StatementLineInfo],
    scans: &[ScanLine<'_>],
    start_idx: usize,
    base_indent: usize,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) {
    let mut depth = 0isize;
    let mut suspended_nested_clause_depth: Option<isize> = None;
    let mut idx = start_idx;
    while idx < scans.len() {
        let line = &scans[idx];
        if line.is_blank || contains_template_marker(line.trimmed) {
            idx += 1;
            continue;
        }

        let first = line.words.first().map(String::as_str);

        if let Some(suspended_depth) = suspended_nested_clause_depth {
            let at_resume_boundary =
                depth == suspended_depth && is_clause_boundary(first, line.trimmed);
            if !at_resume_boundary {
                if !line.is_comment_only {
                    depth += paren_delta_simple(line.trimmed);
                    if depth < 0 {
                        depth = 0;
                    }
                }
                idx += 1;
                continue;
            }
            suspended_nested_clause_depth = None;
        }

        if depth > 0 && matches!(first, Some("WHERE" | "HAVING")) && line.words.len() == 1 {
            // Let nested WHERE/HAVING blocks compute their own child indentation.
            suspended_nested_clause_depth = Some(depth);
            if !line.is_comment_only {
                depth += paren_delta_simple(line.trimmed);
                if depth < 0 {
                    depth = 0;
                }
            }
            idx += 1;
            continue;
        }

        let starts_with_close_paren = line.trimmed.starts_with(')');
        let starts_with_open_paren = line.trimmed.starts_with('(');
        let is_continuation_line = matches!(first, Some("AND" | "OR" | "NOT" | "EXISTS"))
            || starts_with_operator_continuation(line.trimmed)
            || (starts_with_close_paren && depth > 0)
            || (starts_with_open_paren && depth > 0)
            || line.is_comment_only;
        if depth <= 0 && !is_continuation_line && is_clause_boundary(first, line.trimmed) {
            break;
        }

        let logical_depth = depth.max(0) as usize;
        let expected_indent = if line.trimmed.starts_with(')') {
            if logical_depth > 0 {
                base_indent + ((logical_depth - 1) * indent_unit)
            } else {
                base_indent
            }
        } else {
            base_indent + (logical_depth * indent_unit)
        };

        push_leading_indent_edit(
            edits,
            statement_sql,
            line_infos,
            idx,
            line.indent,
            expected_indent,
            indent_unit,
            tab_space_size,
            indent_style,
        );

        if !line.is_comment_only {
            depth += paren_delta_simple(line.trimmed);
            if depth < 0 {
                depth = 0;
            }
        }
        idx += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn push_assignment_block_indent_edits(
    edits: &mut Vec<Lt02AutofixEdit>,
    statement_sql: &str,
    line_infos: &[StatementLineInfo],
    scans: &[ScanLine<'_>],
    start_idx: usize,
    expected_indent: usize,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) {
    let mut idx = start_idx;
    while idx < scans.len() {
        let line = &scans[idx];
        if line.is_blank || line.is_comment_only || contains_template_marker(line.trimmed) {
            idx += 1;
            continue;
        }

        let first = line.words.first().map(String::as_str);
        if starts_with_assignment(line.trimmed) || matches!(first, Some("AND" | "OR")) {
            push_leading_indent_edit(
                edits,
                statement_sql,
                line_infos,
                idx,
                line.indent,
                expected_indent,
                indent_unit,
                tab_space_size,
                indent_style,
            );
            idx += 1;
            continue;
        }

        if is_clause_boundary(first, line.trimmed) || line.trimmed.starts_with(';') {
            break;
        }

        let previous_ended_comma = previous_significant_line(scans, idx)
            .is_some_and(|prev_idx| scans[prev_idx].trimmed.trim_end().ends_with(','));
        if previous_ended_comma {
            push_leading_indent_edit(
                edits,
                statement_sql,
                line_infos,
                idx,
                line.indent,
                expected_indent,
                indent_unit,
                tab_space_size,
                indent_style,
            );
            idx += 1;
            continue;
        }

        break;
    }
}

fn push_line_start_issue_span(
    issue_spans: &mut Vec<(usize, usize)>,
    line_infos: &[StatementLineInfo],
    line_index: usize,
    sql_len: usize,
) {
    let Some(line_info) = line_infos.get(line_index) else {
        return;
    };
    let start = line_info.start.min(sql_len);
    let end = (start + 1).min(sql_len);
    issue_spans.push((start, end.max(start)));
}

fn push_trimmed_offset_issue_span(
    issue_spans: &mut Vec<(usize, usize)>,
    line_infos: &[StatementLineInfo],
    line_index: usize,
    trimmed_offset: usize,
    sql_len: usize,
) {
    let Some(line_info) = line_infos.get(line_index) else {
        return;
    };

    let start = line_info
        .start
        .saturating_add(line_info.indent_end)
        .saturating_add(trimmed_offset)
        .min(sql_len);
    let end = (start + 1).min(sql_len);
    issue_spans.push((start, end.max(start)));
}

fn starts_with_assignment(trimmed: &str) -> bool {
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() || !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return false;
    }

    let mut index = 1usize;
    while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_') {
        index += 1;
    }
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }

    index < bytes.len() && bytes[index] == b'='
}

fn starts_with_operator_continuation(trimmed: &str) -> bool {
    let trimmed = trimmed.trim_start();
    trimmed.starts_with('=')
        || trimmed.starts_with('+')
        || trimmed.starts_with('*')
        || trimmed.starts_with('/')
        || trimmed.starts_with('%')
        || trimmed.starts_with("||")
        || trimmed.starts_with("->")
        || trimmed.starts_with("->>")
        || (trimmed.starts_with('-')
            && !trimmed
                .chars()
                .nth(1)
                .is_some_and(|ch| ch.is_ascii_alphanumeric()))
}

fn inline_join_on_offset(trimmed: &str) -> Option<usize> {
    let upper = trimmed.to_ascii_uppercase();
    if let Some(space_before_on) = upper.find(" ON ") {
        return Some(space_before_on + 1);
    }
    if upper.ends_with(" ON") || upper.ends_with(" ON (") {
        return upper
            .rfind(" ON")
            .map(|space_before_on| space_before_on + 1);
    }
    None
}

fn inline_case_keyword_offset(trimmed: &str) -> Option<usize> {
    let upper = trimmed.to_ascii_uppercase();
    upper
        .find(" CASE")
        .map(|space_before_case| space_before_case + 1)
}

fn should_break_inline_join_on(
    scans: &[ScanLine<'_>],
    line_index: usize,
    first_word: Option<&str>,
    second_word: Option<&str>,
    upper_trimmed: &str,
) -> bool {
    if upper_trimmed.ends_with(" ON") || upper_trimmed.ends_with(" ON (") {
        return true;
    }

    if !matches!(first_word, Some("JOIN")) {
        return false;
    }
    if !is_join_clause(first_word, second_word) {
        return false;
    }
    if inline_join_on_offset(scans[line_index].trimmed).is_none() {
        return false;
    }

    previous_significant_line(scans, line_index).is_some_and(|prev_idx| {
        let prev_first = scans[prev_idx].words.first().map(String::as_str);
        let prev_second = scans[prev_idx].words.get(1).map(String::as_str);
        matches!(
            prev_first,
            Some("LEFT" | "RIGHT" | "FULL" | "INNER" | "OUTER" | "CROSS" | "NATURAL")
        ) && !matches!(prev_second, Some("JOIN" | "APPLY"))
    })
}

fn content_offset_after_keyword(trimmed: &str, keyword: &str) -> Option<usize> {
    if trimmed.len() < keyword.len()
        || !trimmed
            .get(..keyword.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(keyword))
    {
        return None;
    }

    let mut index = keyword.len();
    let first_after = trimmed[index..].chars().next()?;
    if !first_after.is_whitespace() {
        return None;
    }

    while let Some(ch) = trimmed[index..].chars().next() {
        if ch.is_whitespace() {
            index += ch.len_utf8();
        } else {
            break;
        }
    }

    (index < trimmed.len()).then_some(index)
}

fn trailing_as_offset(trimmed: &str) -> Option<usize> {
    let upper = trimmed.to_ascii_uppercase();
    let as_rel = upper.rfind(" AS")?;
    (as_rel > 0 && as_rel + " AS".len() == trimmed.len()).then_some(as_rel)
}

fn is_simple_alias_identifier(trimmed: &str) -> bool {
    if trimmed.is_empty() {
        return false;
    }
    let bytes = trimmed.as_bytes();
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

fn make_interval_inline_arg_offsets(trimmed: &str) -> Option<(usize, usize)> {
    let upper = trimmed.to_ascii_uppercase();
    let open_rel = upper.find("MAKE_INTERVAL(")? + "MAKE_INTERVAL(".len();
    if open_rel >= trimmed.len() {
        return None;
    }

    let mut arg_rel = open_rel;
    while let Some(ch) = trimmed[arg_rel..].chars().next() {
        if ch.is_whitespace() {
            arg_rel += ch.len_utf8();
        } else {
            break;
        }
    }

    (arg_rel < trimmed.len()).then_some((open_rel, arg_rel))
}

fn make_interval_inline_arg_offset(trimmed: &str) -> Option<usize> {
    make_interval_inline_arg_offsets(trimmed).map(|(_, arg_rel)| arg_rel)
}

fn inline_close_paren_offset(trimmed: &str) -> Option<usize> {
    if !trimmed.trim_start().starts_with("=>") {
        return None;
    }
    trimmed.rfind(')')
}

fn push_join_on_block_indent_spans(
    issue_spans: &mut Vec<(usize, usize)>,
    line_infos: &[StatementLineInfo],
    scans: &[ScanLine<'_>],
    join_line_idx: usize,
    indent_unit: usize,
    sql_len: usize,
) {
    let Some(join_line) = scans.get(join_line_idx) else {
        return;
    };
    let join_indent = rounded_indent_width(join_line.indent, indent_unit);
    let base_indent = join_indent + (indent_unit * 2);
    let on_has_open_paren = join_line.trimmed.to_ascii_uppercase().ends_with(" ON (");
    let mut depth: isize = if on_has_open_paren { 1 } else { 0 };

    let mut idx = join_line_idx + 1;
    while idx < scans.len() {
        let line = &scans[idx];
        if line.is_blank || line.is_comment_only || contains_template_marker(line.trimmed) {
            idx += 1;
            continue;
        }

        let first = line.words.first().map(String::as_str);
        let starts_with_close_paren = line.trimmed.starts_with(')');
        let starts_with_open_paren = line.trimmed.starts_with('(');
        let is_continuation_line = matches!(first, Some("AND" | "OR" | "NOT" | "EXISTS"))
            || starts_with_operator_continuation(line.trimmed)
            || (starts_with_close_paren && depth > 0)
            || (starts_with_open_paren && depth > 0);
        if depth <= 0 && !is_continuation_line && is_clause_boundary(first, line.trimmed) {
            break;
        }

        let logical_depth = if on_has_open_paren {
            depth.saturating_sub(1) as usize
        } else {
            depth.max(0) as usize
        };
        let expected_indent = if line.trimmed.starts_with(')') {
            if logical_depth > 0 {
                base_indent + ((logical_depth - 1) * indent_unit)
            } else {
                join_indent + indent_unit
            }
        } else {
            base_indent + (logical_depth * indent_unit)
        };

        if line.indent != expected_indent {
            push_line_start_issue_span(issue_spans, line_infos, idx, sql_len);
        }

        depth += paren_delta_simple(line.trimmed);
        if depth < 0 {
            depth = 0;
        }
        idx += 1;
    }
}

fn find_andor_anchor(scans: &[ScanLine<'_>], from_idx: usize) -> Option<usize> {
    (0..from_idx)
        .rev()
        .find_map(|idx| {
            let line = &scans[idx];
            if line.is_blank || line.is_comment_only {
                return None;
            }
            let first = line.words.first().map(String::as_str);
            if matches!(first, Some("WHERE" | "ON" | "HAVING" | "WHEN")) {
                return Some(idx);
            }
            if is_clause_boundary(first, line.trimmed) && !matches!(first, Some("AND" | "OR")) {
                return Some(usize::MAX);
            }
            None
        })
        .and_then(|idx| (idx != usize::MAX).then_some(idx))
}

fn find_case_when_anchor(scans: &[ScanLine<'_>], from_idx: usize) -> Option<usize> {
    (0..from_idx)
        .rev()
        .find_map(|idx| {
            let line = &scans[idx];
            if line.is_blank || line.is_comment_only {
                return None;
            }
            let first = line.words.first().map(String::as_str);
            if matches!(first, Some("WHEN")) {
                return Some(idx);
            }
            if is_clause_boundary(first, line.trimmed)
                && !matches!(first, Some("AND" | "OR" | "THEN" | "ELSE"))
            {
                return Some(usize::MAX);
            }
            None
        })
        .and_then(|idx| (idx != usize::MAX).then_some(idx))
}

fn build_case_anchor_cache(scans: &[ScanLine<'_>]) -> Vec<Option<usize>> {
    let mut cache = vec![None; scans.len()];
    let mut current_anchor: Option<usize> = None;

    for (idx, line) in scans.iter().enumerate() {
        // Record the anchor *before* inspecting the current line so that a
        // CASE line references its enclosing anchor, while lines that follow
        // (WHEN, THEN, ELSE, END) reference back to the CASE itself.
        cache[idx] = current_anchor;
        if line.is_blank || line.is_comment_only {
            continue;
        }

        let first = line.words.first().map(String::as_str);
        if matches!(first, Some("CASE")) || line.inline_case_offset.is_some() {
            current_anchor = Some(idx);
            continue;
        }

        if is_clause_boundary(first, line.trimmed)
            && !matches!(first, Some("WHEN" | "THEN" | "ELSE" | "END" | "AND" | "OR"))
        {
            current_anchor = None;
        }
    }

    cache
}

fn find_case_anchor(case_anchor_cache: &[Option<usize>], from_idx: usize) -> Option<usize> {
    case_anchor_cache.get(from_idx).copied().flatten()
}

fn case_line_indent_for_anchor(
    scans: &[ScanLine<'_>],
    anchor_idx: usize,
    indent_unit: usize,
) -> usize {
    let anchor = &scans[anchor_idx];
    let first = anchor.words.first().map(String::as_str);
    if matches!(first, Some("CASE")) {
        anchor.indent
    } else if anchor.inline_case_offset.is_some() {
        anchor.indent + indent_unit
    } else {
        anchor.indent
    }
}

fn expected_case_when_indent(
    scans: &[ScanLine<'_>],
    case_anchor_cache: &[Option<usize>],
    line_index: usize,
    indent_unit: usize,
) -> Option<usize> {
    let case_anchor = find_case_anchor(case_anchor_cache, line_index)?;
    let case_indent = case_line_indent_for_anchor(scans, case_anchor, indent_unit);
    Some(case_indent + indent_unit)
}

fn expected_then_indent(
    scans: &[ScanLine<'_>],
    case_anchor_cache: &[Option<usize>],
    line_index: usize,
    indent_unit: usize,
) -> Option<usize> {
    let prev_idx = previous_significant_line(scans, line_index)?;
    let prev = scans.get(prev_idx)?;
    let prev_first = prev.words.first().map(String::as_str);

    if matches!(prev_first, Some("AND" | "OR")) {
        return Some(prev.indent + indent_unit);
    }
    if prev.trimmed.starts_with("=>") {
        return Some(prev.indent);
    }

    if let Some(case_anchor) = find_case_anchor(case_anchor_cache, line_index) {
        let case_indent = case_line_indent_for_anchor(scans, case_anchor, indent_unit);
        return Some(case_indent + indent_unit * 2);
    }

    find_case_when_anchor(scans, line_index)
        .map(|when_idx| scans[when_idx].indent + indent_unit * 2)
}

fn expected_else_indent(
    scans: &[ScanLine<'_>],
    case_anchor_cache: &[Option<usize>],
    line_index: usize,
    indent_unit: usize,
) -> Option<usize> {
    let case_anchor = find_case_anchor(case_anchor_cache, line_index)?;
    let case_indent = case_line_indent_for_anchor(scans, case_anchor, indent_unit);
    Some(case_indent + indent_unit)
}

fn expected_end_indent(
    scans: &[ScanLine<'_>],
    case_anchor_cache: &[Option<usize>],
    line_index: usize,
    indent_unit: usize,
) -> Option<usize> {
    let case_anchor = find_case_anchor(case_anchor_cache, line_index)?;
    Some(case_line_indent_for_anchor(scans, case_anchor, indent_unit))
}

fn paren_depth_between(scans: &[ScanLine<'_>], start_idx: usize, end_idx: usize) -> usize {
    if start_idx >= end_idx || end_idx > scans.len() {
        return 0;
    }

    let depth = scans[start_idx..end_idx]
        .iter()
        .fold(0isize, |acc, line| acc + paren_delta_simple(line.trimmed));

    depth.max(0) as usize
}

fn paren_delta_simple(text: &str) -> isize {
    text.chars().fold(0isize, |acc, ch| match ch {
        '(' => acc + 1,
        ')' => acc - 1,
        _ => acc,
    })
}

// ---------------------------------------------------------------------------
// Generic (dialect-agnostic) indentation detection and helpers
// ---------------------------------------------------------------------------

fn first_line_is_indented(ctx: &RuleContext) -> bool {
    let statement_start = ctx.statement_range.start;
    if statement_start == 0 {
        return false;
    }

    let line_start = ctx.sql[..statement_start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let leading = &ctx.sql[line_start..statement_start];
    !leading.is_empty() && leading.chars().all(char::is_whitespace)
}

fn ignore_first_line_indent_for_fragmented_statement(ctx: &RuleContext) -> bool {
    if ctx.statement_index == 0 || ctx.statement_range.start == 0 {
        return false;
    }

    let prefix = &ctx.sql[..ctx.statement_range.start.min(ctx.sql.len())];
    let prev_non_ws = prefix.chars().rev().find(|ch| !ch.is_whitespace());
    matches!(prev_non_ws, Some(ch) if ch != ';')
}

fn first_line_is_template_fragment(ctx: &RuleContext) -> bool {
    let statement_start = ctx.statement_range.start;
    if statement_start == 0 {
        return false;
    }

    let line_start = ctx.sql[..statement_start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let leading = &ctx.sql[line_start..statement_start];
    if leading.is_empty() || !leading.chars().all(char::is_whitespace) {
        return false;
    }

    let before_line = &ctx.sql[..line_start];
    for raw_line in before_line.lines().rev() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return is_template_boundary_line(trimmed);
    }

    false
}

// ---------------------------------------------------------------------------
// Structural indent detection
// ---------------------------------------------------------------------------

/// Returns true if any line has indentation that violates structural
/// expectations. This catches cases where all indents are valid multiples
/// of indent_unit but are at the wrong depth for their SQL context.
///
/// The check focuses on "standalone clause keyword" patterns: when a clause
/// keyword that expects indented content (SELECT, FROM, WHERE, SET, etc.)
/// appears alone on a line, the content on the following line must be
/// indented by one indent_unit.
/// Returns autofix edits for structural indentation violations. When empty,
/// no structural violation was found.
fn structural_indent_edits(
    ctx: &RuleContext,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
    options: &LayoutIndent,
) -> Vec<Lt02AutofixEdit> {
    let sql = ctx.statement_sql();

    if options.ignore_templated_areas && contains_template_marker(sql) {
        return Vec::new();
    }

    // Skip structural check for templated SQL. Template expansion can
    // produce indentation patterns that look structurally wrong but are
    // correct in the original source.
    if ctx.is_templated() {
        return Vec::new();
    }

    // Try to tokenize; if we cannot, fall back to no structural check.
    let tokens = tokenize_for_structural_check(sql, ctx);
    let tokens = match tokens.as_deref() {
        Some(t) if !t.is_empty() => t,
        _ => return Vec::new(),
    };

    // Build per-line token info.
    let line_infos = build_line_token_infos(tokens);
    if line_infos.is_empty() {
        return Vec::new();
    }

    let actual_indents = actual_indent_map(sql, tab_space_size);
    let line_info_list = statement_line_infos(sql);
    let mut edits = Vec::new();

    // Check for structural violations: when a content-bearing clause keyword
    // is alone on its line, the following content line must be indented by
    // indent_unit more than the keyword line.
    let lines: Vec<usize> = line_infos.keys().copied().collect();
    for (i, &line) in lines.iter().enumerate() {
        let info = &line_infos[&line];
        if !info.is_standalone_content_clause {
            continue;
        }

        let keyword_indent = actual_indents.get(&line).copied().unwrap_or(0);
        let expected_content_indent = keyword_indent + indent_unit;

        // Find the next content line (skip blank lines).
        if let Some(&next_line) = lines.get(i + 1) {
            let next_info = &line_infos[&next_line];
            // Skip content lines that are clause keywords (they set their
            // own indent context) or SELECT modifiers (DISTINCT/ALL) which
            // belong with the preceding SELECT, not as indented content.
            if !next_info.starts_with_clause_keyword && !next_info.starts_with_select_modifier {
                let next_actual = actual_indents.get(&next_line).copied().unwrap_or(0);
                if next_actual != expected_content_indent {
                    if let Some(line_info) = line_info_list.get(next_line) {
                        let start = line_info.start;
                        let end = line_info.start + line_info.indent_end;
                        if end <= sql.len() && start <= end {
                            let replacement = make_indent(
                                expected_content_indent,
                                indent_unit,
                                tab_space_size,
                                indent_style,
                            );
                            edits.push(Lt02AutofixEdit {
                                start,
                                end,
                                replacement,
                            });
                        }
                    }
                }
            }
        }
    }

    // Check for comment-only trailing lines at deeper indentation than
    // any content line. E.g. `SELECT 1\n    -- foo\n        -- bar`
    // has comments indented beyond the content (indent 0), which is wrong.
    if let Some(&last_content_line) = line_infos
        .iter()
        .rev()
        .find(|(_, info)| !info.is_comment_only)
        .map(|(line, _)| line)
    {
        let last_content_indent = actual_indents.get(&last_content_line).copied().unwrap_or(0);
        // Check comment-only lines after the last content line.
        for (&line, info) in &line_infos {
            if line > last_content_line && info.is_comment_only {
                if options.ignore_comment_lines {
                    continue;
                }
                let comment_indent = actual_indents.get(&line).copied().unwrap_or(0);
                if comment_indent > last_content_indent {
                    if let Some(line_info) = line_info_list.get(line) {
                        let start = line_info.start;
                        let end = line_info.start + line_info.indent_end;
                        if end <= sql.len() && start <= end {
                            let replacement = make_indent(
                                last_content_indent,
                                indent_unit,
                                tab_space_size,
                                indent_style,
                            );
                            edits.push(Lt02AutofixEdit {
                                start,
                                end,
                                replacement,
                            });
                        }
                    }
                }
            }
        }
    }

    edits
}

struct ScanLine<'a> {
    trimmed: &'a str,
    indent: usize,
    words: Vec<String>,
    is_blank: bool,
    is_comment_only: bool,
    inline_case_offset: Option<usize>,
    prev_significant: Option<usize>,
    next_significant: Option<usize>,
}

fn link_significant_lines(scans: &mut [ScanLine<'_>]) {
    let mut prev_significant = None;
    for (idx, scan) in scans.iter_mut().enumerate() {
        scan.prev_significant = prev_significant;
        if !scan.is_blank && !scan.is_comment_only {
            prev_significant = Some(idx);
        }
    }

    let mut next_significant = None;
    for idx in (0..scans.len()).rev() {
        scans[idx].next_significant = next_significant;
        if !scans[idx].is_blank && !scans[idx].is_comment_only {
            next_significant = Some(idx);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TemplateMultilineMode {
    Expression,
    Statement,
    Comment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TemplateControlKind {
    Open,
    Mid,
    Close,
}

#[derive(Clone, Debug)]
struct TemplateControlTag {
    kind: TemplateControlKind,
    keyword: Option<String>,
}

fn detect_additional_indentation_violation(
    sql: &str,
    indent_unit: usize,
    tab_space_size: usize,
    options: &LayoutIndent,
    dialect: Dialect,
) -> bool {
    let lines: Vec<&str> = sql.lines().collect();
    if lines.is_empty() {
        return false;
    }

    let indent_map = actual_indent_map(sql, tab_space_size);
    let mut scans: Vec<_> = lines
        .iter()
        .enumerate()
        .map(|(line_idx, line)| {
            let trimmed = line.trim_start();
            let is_blank = trimmed.trim().is_empty();
            let is_comment_only = is_comment_line(trimmed);
            let words = if is_blank {
                Vec::new()
            } else {
                split_upper_words(trimmed)
            };
            ScanLine {
                trimmed,
                indent: indent_map.get(&line_idx).copied().unwrap_or(0),
                words,
                is_blank,
                is_comment_only,
                inline_case_offset: None,
                prev_significant: None,
                next_significant: None,
            }
        })
        .collect();
    link_significant_lines(&mut scans);

    let template_only_lines = template_only_line_flags(&lines);
    let mut sql_template_block_indents: Vec<usize> = Vec::new();

    for idx in 0..scans.len() {
        let line = &scans[idx];
        if line.is_blank {
            continue;
        }

        let template_controls = template_control_tags_in_line(line.trimmed);
        if !template_controls.is_empty() {
            for tag in template_controls {
                match tag.kind {
                    TemplateControlKind::Open => {
                        if tag
                            .keyword
                            .as_deref()
                            .is_none_or(|keyword| !is_non_sql_template_keyword(keyword))
                        {
                            sql_template_block_indents.push(line.indent);
                        }
                    }
                    TemplateControlKind::Mid => {
                        if let Some(expected_indent) = sql_template_block_indents.last() {
                            if line.indent != *expected_indent {
                                return true;
                            }
                        }
                    }
                    TemplateControlKind::Close => {
                        if tag
                            .keyword
                            .as_deref()
                            .is_none_or(|keyword| !is_non_sql_template_keyword(keyword))
                        {
                            if let Some(open_indent) = sql_template_block_indents.pop() {
                                if line.indent != open_indent {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            continue;
        }

        if template_only_lines.get(idx).copied().unwrap_or(false) {
            continue;
        }

        let in_sql_template_block = !sql_template_block_indents.is_empty();
        if in_sql_template_block {
            let required_indent = sql_template_block_indents[0] + indent_unit;
            if line.indent < required_indent {
                return true;
            }
        }

        let first = line.words.first().map(String::as_str);
        let second = line.words.get(1).map(String::as_str);

        if matches!(first, Some("ELSE")) && words_contain_in_order(&line.words, "ELSE", "END") {
            return true;
        }

        let upper = line.trimmed.to_ascii_uppercase();
        if upper.contains(" AS (SELECT") || upper.starts_with("(SELECT") {
            return true;
        }

        if matches!(first, Some("DECLARE")) && line.words.len() > 1 {
            return true;
        }

        if upper.contains(" PROCEDURE") {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                if scans[next_idx].trimmed.starts_with('@') && scans[next_idx].indent <= line.indent
                {
                    return true;
                }
            }
        }

        if matches!(first, Some("ELSE")) {
            if idx == 0 && matches!(dialect, Dialect::Mssql) {
                return true;
            }
            if let Some(prev_idx) =
                previous_line_matching(&scans, idx, |f, _| matches!(f, Some("IF" | "ELSE")))
            {
                if line.indent > scans[prev_idx].indent {
                    return true;
                }
            }
        }

        if options.indented_ctes && matches!(first, Some("WITH")) {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                if scans[next_idx].indent != line.indent + indent_unit {
                    return true;
                }
            }
        }

        if is_content_clause_line(first, line.trimmed) {
            let expected_indent = line.indent + indent_unit;
            let mut scan_idx = idx + 1;
            while scan_idx < scans.len() {
                let next = &scans[scan_idx];
                if next.is_blank {
                    scan_idx += 1;
                    continue;
                }
                if template_only_lines.get(scan_idx).copied().unwrap_or(false) {
                    let one_line_template_expr =
                        next.trimmed.starts_with("{{") && next.trimmed.contains("}}");
                    if one_line_template_expr {
                        // One-line templated expressions can still represent
                        // content elements (e.g. SELECT list items) and should
                        // keep their surrounding indentation.
                    } else {
                        scan_idx += 1;
                        continue;
                    }
                }
                let next_first = next.words.first().map(String::as_str);
                if is_clause_boundary(next_first, next.trimmed) {
                    break;
                }
                if next.is_comment_only {
                    scan_idx += 1;
                    continue;
                }
                if next.indent < expected_indent {
                    return true;
                }
                scan_idx += 1;
            }
        }

        if is_join_clause(first, second) && !in_sql_template_block {
            if let Some(prev_join_idx) = previous_line_matching(&scans, idx, is_join_clause) {
                let prev_first = scans[prev_join_idx].words.first().map(String::as_str);
                let prev_second = scans[prev_join_idx].words.get(1).map(String::as_str);
                if previous_significant_line(&scans, idx) == Some(prev_join_idx)
                    && join_requires_condition(prev_first, prev_second)
                    && line.indent < scans[prev_join_idx].indent + indent_unit
                {
                    return true;
                }
            }

            let parent_from_indent =
                previous_line_indent_matching(&scans, idx, |f, _| matches!(f, Some("FROM")))
                    .or_else(|| previous_line_indent_matching(&scans, idx, is_join_clause))
                    .unwrap_or(0);
            let expected = parent_from_indent
                + if options.indented_joins {
                    indent_unit
                } else {
                    0
                };
            if line.indent != expected {
                return true;
            }
        }

        if matches!(first, Some("USING" | "ON")) {
            let parent_indent = previous_line_indent_matching(&scans, idx, |f, s| {
                is_join_clause(f, s) || matches!(f, Some("USING"))
            })
            .unwrap_or(0);
            let expected = parent_indent
                + if options.indented_using_on {
                    indent_unit
                } else {
                    0
                };
            if line.indent != expected {
                return true;
            }
        }

        if line.is_comment_only && !options.ignore_comment_lines {
            if let (Some(prev_idx), Some(next_idx)) = (
                previous_significant_line(&scans, idx),
                next_significant_line(&scans, idx),
            ) {
                if is_join_clause(
                    scans[next_idx].words.first().map(String::as_str),
                    scans[next_idx].words.get(1).map(String::as_str),
                ) || matches!(
                    scans[next_idx].words.first().map(String::as_str),
                    Some("FROM" | "WHERE" | "HAVING" | "QUALIFY" | "LIMIT")
                ) {
                    let allowed = scans[prev_idx].indent.max(scans[next_idx].indent);
                    if line.indent > allowed {
                        return true;
                    }
                }
            }
        }

        if options.implicit_indents == ImplicitIndentsMode::Require
            && matches!(first, Some("WHERE" | "HAVING" | "ON" | "CASE"))
            && line.words.len() == 1
        {
            return true;
        }

        if options.implicit_indents == ImplicitIndentsMode::Allow
            && matches!(first, Some("WHERE"))
            && line.words.len() > 1
            && line.trimmed.contains('(')
            && !line.trimmed.contains(')')
            && !line.trimmed.trim_end().ends_with('(')
        {
            return true;
        }

        if matches!(first, Some("WHERE" | "HAVING")) && line.words.len() > 1 {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                if matches!(
                    scans[next_idx].words.first().map(String::as_str),
                    Some("AND" | "OR")
                ) {
                    match options.implicit_indents {
                        ImplicitIndentsMode::Forbid => return true,
                        ImplicitIndentsMode::Allow | ImplicitIndentsMode::Require => {
                            if scans[next_idx].indent < line.indent + indent_unit {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        if matches!(first, Some("ON")) {
            if options.indented_on_contents {
                let on_has_inline = line.words.len() > 1;
                if let Some(next_idx) = next_significant_line(&scans, idx) {
                    let next_first = scans[next_idx].words.first().map(String::as_str);
                    if on_has_inline && matches!(next_first, Some("AND" | "OR")) {
                        match options.implicit_indents {
                            ImplicitIndentsMode::Allow => {
                                if scans[next_idx].indent < line.indent + indent_unit {
                                    return true;
                                }
                            }
                            ImplicitIndentsMode::Forbid | ImplicitIndentsMode::Require => {
                                return true;
                            }
                        }
                    }
                    if !on_has_inline && scans[next_idx].indent < line.indent + indent_unit {
                        return true;
                    }
                }
            } else if let Some(next_idx) = next_significant_line(&scans, idx) {
                let next_first = scans[next_idx].words.first().map(String::as_str);
                if matches!(next_first, Some("AND" | "OR")) && scans[next_idx].indent != line.indent
                {
                    return true;
                }
            }
        }

        if options.indented_on_contents && line_contains_inline_on(line.trimmed) {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                if matches!(
                    scans[next_idx].words.first().map(String::as_str),
                    Some("AND" | "OR")
                ) {
                    match options.implicit_indents {
                        ImplicitIndentsMode::Allow => {
                            if scans[next_idx].indent < line.indent + indent_unit {
                                return true;
                            }
                        }
                        ImplicitIndentsMode::Forbid | ImplicitIndentsMode::Require => {
                            return true;
                        }
                    }
                }
            }
        }

        if options.indented_then && matches!(first, Some("THEN")) {
            if let Some(prev_idx) = previous_significant_line(&scans, idx) {
                let prev_first = scans[prev_idx].words.first().map(String::as_str);
                if matches!(prev_first, Some("WHEN")) && line.indent <= scans[prev_idx].indent {
                    return true;
                }
            }
        }

        if !options.indented_then && matches!(first, Some("THEN")) {
            if let Some(prev_idx) = previous_significant_line(&scans, idx) {
                if line.indent > scans[prev_idx].indent + indent_unit {
                    return true;
                }
            }
        }

        if !options.indented_then_contents && matches!(first, Some("THEN")) {
            if let Some(next_idx) = next_significant_line(&scans, idx) {
                if scans[next_idx].indent > line.indent + indent_unit {
                    return true;
                }
            }
        }
    }

    false
}

fn detect_tsql_else_if_successive_violation(sql: &str, tab_space_size: usize) -> bool {
    let lines: Vec<&str> = sql.lines().collect();
    let indent_map = actual_indent_map(sql, tab_space_size);
    let mut scans: Vec<_> = lines
        .iter()
        .enumerate()
        .map(|(line_idx, line)| {
            let trimmed = line.trim_start();
            let is_blank = trimmed.trim().is_empty();
            let words = if is_blank {
                Vec::new()
            } else {
                split_upper_words(trimmed)
            };
            ScanLine {
                trimmed,
                indent: indent_map.get(&line_idx).copied().unwrap_or(0),
                words,
                is_blank,
                is_comment_only: is_comment_line(trimmed),
                inline_case_offset: None,
                prev_significant: None,
                next_significant: None,
            }
        })
        .collect();
    link_significant_lines(&mut scans);

    for idx in 0..scans.len() {
        let line = &scans[idx];
        if line.is_blank || line.is_comment_only {
            continue;
        }

        if !matches!(line.words.first().map(String::as_str), Some("ELSE")) {
            continue;
        }

        if let Some(prev_idx) =
            previous_line_matching(&scans, idx, |f, _| matches!(f, Some("IF" | "ELSE")))
        {
            if line.indent > scans[prev_idx].indent {
                return true;
            }
        }
    }

    false
}

fn split_upper_words(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|word| !word.is_empty())
        .map(|word| word.to_ascii_uppercase())
        .collect()
}

fn is_comment_line(trimmed: &str) -> bool {
    trimmed.starts_with("--")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("*/")
}

fn words_contain_in_order(words: &[String], first: &str, second: &str) -> bool {
    let Some(first_pos) = words.iter().position(|word| word == first) else {
        return false;
    };
    words.iter().skip(first_pos + 1).any(|word| word == second)
}

fn is_content_clause_line(first_word: Option<&str>, trimmed: &str) -> bool {
    matches!(
        first_word,
        Some("SELECT")
            | Some("FROM")
            | Some("WHERE")
            | Some("SET")
            | Some("RETURNING")
            | Some("HAVING")
            | Some("LIMIT")
            | Some("QUALIFY")
            | Some("WINDOW")
            | Some("DECLARE")
            | Some("VALUES")
            | Some("UPDATE")
    ) && split_upper_words(trimmed).len() == 1
}

fn is_join_clause(first_word: Option<&str>, second_word: Option<&str>) -> bool {
    matches!(first_word, Some("JOIN" | "APPLY"))
        || (matches!(
            first_word,
            Some("LEFT" | "RIGHT" | "FULL" | "INNER" | "CROSS" | "OUTER" | "NATURAL")
        ) && matches!(second_word, Some("JOIN" | "APPLY")))
}

fn join_requires_condition(first_word: Option<&str>, second_word: Option<&str>) -> bool {
    matches!(
        first_word,
        Some("JOIN" | "INNER" | "LEFT" | "RIGHT" | "FULL")
    ) || matches!(
        (first_word, second_word),
        (Some("OUTER"), Some("JOIN")) | (Some("NATURAL"), Some("JOIN"))
    )
}

fn is_clause_boundary(first_word: Option<&str>, trimmed: &str) -> bool {
    matches!(
        first_word,
        Some("SELECT")
            | Some("FROM")
            | Some("WHERE")
            | Some("GROUP")
            | Some("ORDER")
            | Some("HAVING")
            | Some("LIMIT")
            | Some("QUALIFY")
            | Some("WINDOW")
            | Some("RETURNING")
            | Some("SET")
            | Some("UPDATE")
            | Some("DELETE")
            | Some("INSERT")
            | Some("MERGE")
            | Some("WITH")
            | Some("JOIN")
            | Some("LEFT")
            | Some("RIGHT")
            | Some("FULL")
            | Some("INNER")
            | Some("OUTER")
            | Some("CROSS")
            | Some("USING")
            | Some("ON")
            | Some("WHEN")
            | Some("THEN")
            | Some("ELSE")
            | Some("END")
    ) || trimmed.starts_with(')')
}

fn next_significant_line(scans: &[ScanLine<'_>], from_idx: usize) -> Option<usize> {
    scans.get(from_idx).and_then(|scan| scan.next_significant)
}

fn previous_significant_line(scans: &[ScanLine<'_>], from_idx: usize) -> Option<usize> {
    scans.get(from_idx).and_then(|scan| scan.prev_significant)
}

fn previous_line_matching(
    scans: &[ScanLine<'_>],
    from_idx: usize,
    predicate: impl Fn(Option<&str>, Option<&str>) -> bool,
) -> Option<usize> {
    (0..from_idx).rev().find(|idx| {
        let first = scans[*idx].words.first().map(String::as_str);
        let second = scans[*idx].words.get(1).map(String::as_str);
        predicate(first, second)
    })
}

fn previous_line_indent_matching(
    scans: &[ScanLine<'_>],
    from_idx: usize,
    predicate: impl Fn(Option<&str>, Option<&str>) -> bool,
) -> Option<usize> {
    (0..from_idx).rev().find_map(|idx| {
        let first = scans[idx].words.first().map(String::as_str);
        let second = scans[idx].words.get(1).map(String::as_str);
        predicate(first, second).then_some(scans[idx].indent)
    })
}

fn line_contains_inline_on(trimmed: &str) -> bool {
    let upper = trimmed.to_ascii_uppercase();
    upper.contains(" ON ") && !upper.starts_with("ON ")
}

fn contains_template_marker(sql: &str) -> bool {
    sql.contains("{{") || sql.contains("{%") || sql.contains("{#")
}

fn is_template_boundary_line(trimmed: &str) -> bool {
    trimmed.starts_with("{%")
        || trimmed.starts_with("{{")
        || trimmed.starts_with("{#")
        || trimmed.starts_with("%}")
        || trimmed.starts_with("}}")
        || trimmed.starts_with("#}")
        || trimmed.ends_with("%}")
        || trimmed.ends_with("}}")
        || trimmed.ends_with("#}")
}

fn template_only_line_flags(lines: &[&str]) -> Vec<bool> {
    let mut flags = vec![false; lines.len()];
    let mut multiline_mode: Option<TemplateMultilineMode> = None;
    let mut non_sql_depth = 0usize;

    for (idx, raw_line) in lines.iter().enumerate() {
        let trimmed = raw_line.trim_start();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(mode) = multiline_mode {
            flags[idx] = true;
            if line_closes_multiline_template(trimmed, mode) {
                multiline_mode = None;
            }
            continue;
        }

        let mut template_only = false;

        // Track `{% ... %}` blocks so macro/set bodies are treated as template-only.
        let control_tags = template_control_tags_in_line(trimmed);
        if !control_tags.is_empty() {
            template_only = true;
            for tag in &control_tags {
                match tag.kind {
                    TemplateControlKind::Open => {
                        if tag
                            .keyword
                            .as_deref()
                            .is_some_and(is_non_sql_template_keyword)
                        {
                            non_sql_depth += 1;
                        }
                    }
                    TemplateControlKind::Close => {
                        if tag
                            .keyword
                            .as_deref()
                            .is_some_and(is_non_sql_template_keyword)
                            && non_sql_depth > 0
                        {
                            non_sql_depth -= 1;
                        }
                    }
                    TemplateControlKind::Mid => {}
                }
            }
        }

        if let Some(mode) = line_starts_multiline_template(trimmed) {
            template_only = true;
            multiline_mode = Some(mode);
        } else if trimmed.starts_with("%}")
            || trimmed.starts_with("}}")
            || trimmed.starts_with("#}")
        {
            template_only = true;
        }

        if (trimmed.starts_with("{{") || trimmed.starts_with("{#") || trimmed.starts_with("{%"))
            && !line_has_sql_outside_template_tags(trimmed)
        {
            template_only = true;
        }

        if non_sql_depth > 0 {
            template_only = true;
        }

        flags[idx] = template_only;
    }

    flags
}

fn line_starts_multiline_template(trimmed: &str) -> Option<TemplateMultilineMode> {
    if trimmed.starts_with("{{") && !trimmed.contains("}}") {
        return Some(TemplateMultilineMode::Expression);
    }
    if trimmed.starts_with("{%") && !trimmed.contains("%}") {
        return Some(TemplateMultilineMode::Statement);
    }
    if trimmed.starts_with("{#") && !trimmed.contains("#}") {
        return Some(TemplateMultilineMode::Comment);
    }
    None
}

fn line_closes_multiline_template(trimmed: &str, mode: TemplateMultilineMode) -> bool {
    match mode {
        TemplateMultilineMode::Expression => trimmed.contains("}}"),
        TemplateMultilineMode::Statement => trimmed.contains("%}"),
        TemplateMultilineMode::Comment => trimmed.contains("#}"),
    }
}

fn is_non_sql_template_keyword(keyword: &str) -> bool {
    matches!(
        keyword,
        "macro" | "set" | "call" | "filter" | "raw" | "test"
    )
}

fn template_control_tags_in_line(line: &str) -> Vec<TemplateControlTag> {
    let mut out = Vec::new();
    let mut cursor = 0usize;

    while let Some(open_rel) = line[cursor..].find("{%") {
        let open = cursor + open_rel;
        let Some(close_rel) = line[open + 2..].find("%}") else {
            break;
        };
        let close = open + 2 + close_rel;
        let mut inner = &line[open + 2..close];
        inner = inner.trim();
        if let Some(stripped) = inner.strip_prefix('-') {
            inner = stripped.trim_start();
        }
        if let Some(stripped) = inner.strip_suffix('-') {
            inner = stripped.trim_end();
        }
        if let Some(first) = inner.split_whitespace().next() {
            let first = first.to_ascii_lowercase();
            if first.starts_with("end") {
                let keyword = first.strip_prefix("end").unwrap_or("").to_string();
                out.push(TemplateControlTag {
                    kind: TemplateControlKind::Close,
                    keyword: (!keyword.is_empty()).then_some(keyword),
                });
            } else if matches!(first.as_str(), "else" | "elif") {
                out.push(TemplateControlTag {
                    kind: TemplateControlKind::Mid,
                    keyword: None,
                });
            } else {
                out.push(TemplateControlTag {
                    kind: TemplateControlKind::Open,
                    keyword: Some(first),
                });
            }
        }
        cursor = close + 2;
    }

    out
}

fn line_has_sql_outside_template_tags(line: &str) -> bool {
    let mut index = 0usize;
    while index < line.len() {
        let rest = &line[index..];
        if rest.starts_with("{{") {
            let Some(close) = rest.find("}}") else {
                return line[..index].chars().any(|ch| !ch.is_whitespace());
            };
            index += close + 2;
            continue;
        }
        if rest.starts_with("{%") {
            let Some(close) = rest.find("%}") else {
                return line[..index].chars().any(|ch| !ch.is_whitespace());
            };
            index += close + 2;
            continue;
        }
        if rest.starts_with("{#") {
            let Some(close) = rest.find("#}") else {
                return line[..index].chars().any(|ch| !ch.is_whitespace());
            };
            index += close + 2;
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        if !ch.is_whitespace() {
            return true;
        }
        index += ch.len_utf8();
    }

    false
}

fn templated_detection_confident(sql: &str, indent_unit: usize, tab_space_size: usize) -> bool {
    if templated_control_confident_violation(sql, indent_unit, tab_space_size) {
        return true;
    }

    let lines: Vec<&str> = sql.lines().collect();
    if lines.is_empty() {
        return false;
    }

    for idx in 0..lines.len() {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || is_comment_line(trimmed) {
            continue;
        }
        let indent = line
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .count();

        if let Some(prev_idx) = (0..idx).rev().find(|prev| {
            let prev_trim = lines[*prev].trim_start();
            !prev_trim.is_empty() && !is_comment_line(prev_trim)
        }) {
            let prev_trimmed = lines[prev_idx].trim_start();
            let prev_upper = prev_trimmed.to_ascii_uppercase();

            if lines[prev_idx].trim_end().ends_with(',') && indent == 0 {
                return true;
            }

            if is_content_clause_line(
                split_upper_words(&prev_upper).first().map(String::as_str),
                &prev_upper,
            ) && indent == 0
            {
                return true;
            }
        }

        if trimmed.starts_with("{{")
            && indent == 0
            && (0..idx)
                .rev()
                .find(|prev| {
                    let prev_trim = lines[*prev].trim_start();
                    !prev_trim.is_empty() && !is_comment_line(prev_trim)
                })
                .is_some_and(|prev_idx| lines[prev_idx].trim_end().ends_with(','))
        {
            return true;
        }
    }

    false
}

fn templated_control_confident_violation(
    sql: &str,
    indent_unit: usize,
    tab_space_size: usize,
) -> bool {
    let lines: Vec<&str> = sql.lines().collect();
    if lines.is_empty() {
        return false;
    }

    let template_only_lines = template_only_line_flags(&lines);
    let indent_map = actual_indent_map(sql, tab_space_size);
    let mut sql_template_block_indents: Vec<usize> = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.trim().is_empty() {
            continue;
        }

        let indent = indent_map.get(&idx).copied().unwrap_or(0);
        let controls = template_control_tags_in_line(trimmed);
        if !controls.is_empty() {
            for tag in controls {
                match tag.kind {
                    TemplateControlKind::Open => {
                        if tag
                            .keyword
                            .as_deref()
                            .is_none_or(|keyword| !is_non_sql_template_keyword(keyword))
                        {
                            sql_template_block_indents.push(indent);
                        }
                    }
                    TemplateControlKind::Mid => {
                        if let Some(expected_indent) = sql_template_block_indents.last() {
                            if indent != *expected_indent {
                                return true;
                            }
                        }
                    }
                    TemplateControlKind::Close => {
                        if tag
                            .keyword
                            .as_deref()
                            .is_none_or(|keyword| !is_non_sql_template_keyword(keyword))
                        {
                            if let Some(open_indent) = sql_template_block_indents.pop() {
                                if indent != open_indent {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            continue;
        }

        if template_only_lines.get(idx).copied().unwrap_or(false) {
            continue;
        }

        if !sql_template_block_indents.is_empty() {
            let required_indent = sql_template_block_indents[0] + indent_unit;
            if indent < required_indent {
                return true;
            }
        }
    }

    false
}

/// Build the indent string for a given target width.
fn make_indent(
    width: usize,
    _indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) -> String {
    if width == 0 {
        return String::new();
    }
    match indent_style {
        IndentStyle::Spaces => " ".repeat(width),
        IndentStyle::Tabs => {
            let tab_width = tab_space_size.max(1);
            let tab_count = width.div_ceil(tab_width);
            "\t".repeat(tab_count)
        }
    }
}

/// Per-line summary of token structure.
struct LineTokenInfo {
    /// True if the line starts with a top-level clause keyword.
    starts_with_clause_keyword: bool,
    /// True if the line starts with a content-bearing clause keyword that is
    /// alone on the line. Content-bearing keywords are those whose content
    /// should be indented on the following line (SELECT, FROM, WHERE, SET,
    /// RETURNING, HAVING, LIMIT, QUALIFY, WINDOW, DECLARE). Keywords like
    /// WITH, CREATE, UNION are excluded because their "content" is other
    /// clause-level constructs, not indented content.
    is_standalone_content_clause: bool,
    /// True if the line contains only comment tokens.
    is_comment_only: bool,
    /// True if the line starts with a SELECT modifier (DISTINCT, ALL) that
    /// belongs with a preceding SELECT keyword rather than being content
    /// that should be indented.
    starts_with_select_modifier: bool,
}

/// Keywords whose content on the following line should be indented.
fn is_content_bearing_clause(kw: Keyword) -> bool {
    matches!(
        kw,
        Keyword::SELECT
            | Keyword::FROM
            | Keyword::WHERE
            | Keyword::SET
            | Keyword::RETURNING
            | Keyword::HAVING
            | Keyword::LIMIT
            | Keyword::QUALIFY
            | Keyword::WINDOW
            | Keyword::DECLARE
            | Keyword::VALUES
            | Keyword::UPDATE
    )
}

/// Build per-line token info from the token stream.
fn build_line_token_infos(tokens: &[StructuralToken]) -> BTreeMap<usize, LineTokenInfo> {
    let mut result: BTreeMap<usize, LineTokenInfo> = BTreeMap::new();

    // Group non-trivia tokens by line.
    let mut tokens_by_line: BTreeMap<usize, Vec<&StructuralToken>> = BTreeMap::new();
    for token in tokens {
        if is_whitespace_or_newline(&token.token) {
            continue;
        }
        tokens_by_line.entry(token.line).or_default().push(token);
    }

    // Track preceding keyword for GROUP BY / ORDER BY detection.
    let mut prev_keyword: Option<Keyword> = None;

    for (&line, line_tokens) in &tokens_by_line {
        let first = &line_tokens[0];
        let starts_with_clause = is_first_token_clause_keyword(first, prev_keyword);

        // Check if the first keyword is content-bearing.
        let first_is_content_bearing = match &first.token {
            Token::Word(w) => is_content_bearing_clause(w.keyword),
            _ => false,
        };

        // A clause keyword is "standalone" if all non-trivia tokens on the
        // line are clause keywords / modifiers / comments / semicolons.
        let is_standalone = starts_with_clause && first_is_content_bearing && {
            line_tokens.iter().all(|t| match &t.token {
                Token::Word(w) => {
                    is_clause_keyword_word(w.keyword)
                        || w.keyword == Keyword::NoKeyword && is_join_modifier(&w.value)
                }
                Token::SemiColon => true,
                _ => is_comment_token(&t.token),
            })
        };

        let comment_only = line_tokens.iter().all(|t| is_comment_token(&t.token));

        let starts_with_select_modifier = match &first.token {
            Token::Word(w) => matches!(w.keyword, Keyword::DISTINCT | Keyword::ALL),
            _ => false,
        };

        result.insert(
            line,
            LineTokenInfo {
                starts_with_clause_keyword: starts_with_clause,
                is_standalone_content_clause: is_standalone,
                is_comment_only: comment_only,
                starts_with_select_modifier,
            },
        );

        // Update prev_keyword from last keyword on this line.
        for t in line_tokens.iter().rev() {
            if let Token::Word(w) = &t.token {
                if w.keyword != Keyword::NoKeyword {
                    prev_keyword = Some(w.keyword);
                    break;
                }
            }
        }
    }

    result
}

fn is_first_token_clause_keyword(token: &StructuralToken, prev_keyword: Option<Keyword>) -> bool {
    match &token.token {
        Token::Word(w) => is_top_level_clause_keyword(w.keyword, prev_keyword),
        _ => false,
    }
}

fn is_comment_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

/// Tokenize SQL for structural analysis. Falls back to statement-level
/// tokenization when document tokens are not available.
fn tokenize_for_structural_check(sql: &str, ctx: &RuleContext) -> Option<Vec<StructuralToken>> {
    // Fall back to statement-level tokenization (document tokens use
    // 1-indexed lines which makes correlation harder; local tokenization
    // gives 0-indexed consistency).
    let dialect = ctx.dialect().to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return None;
    };

    Some(
        tokens
            .into_iter()
            .filter_map(|t| {
                let line = t.span.start.line as usize;
                let col = t.span.start.column as usize;
                let offset = line_col_to_offset(sql, line, col)?;
                Some(StructuralToken {
                    token: t.token,
                    offset,
                    line: line.saturating_sub(1),
                })
            })
            .collect(),
    )
}

#[derive(Clone)]
struct StructuralToken {
    token: Token,
    #[allow(dead_code)]
    offset: usize,
    line: usize,
}

/// Returns true if the keyword starts a top-level SQL clause.
fn is_top_level_clause_keyword(kw: Keyword, _prev_keyword: Option<Keyword>) -> bool {
    is_clause_keyword_word(kw)
}

/// Core set of SQL clause keywords that establish a new indent level.
fn is_clause_keyword_word(kw: Keyword) -> bool {
    matches!(
        kw,
        Keyword::SELECT
            | Keyword::FROM
            | Keyword::WHERE
            | Keyword::SET
            | Keyword::UPDATE
            | Keyword::INSERT
            | Keyword::DELETE
            | Keyword::MERGE
            | Keyword::USING
            | Keyword::INTO
            | Keyword::VALUES
            | Keyword::RETURNING
            | Keyword::HAVING
            | Keyword::LIMIT
            | Keyword::WINDOW
            | Keyword::QUALIFY
            | Keyword::WITH
            | Keyword::BEGIN
            | Keyword::DECLARE
            | Keyword::IF
            | Keyword::RETURNS
            | Keyword::CREATE
            | Keyword::DROP
            | Keyword::ON
            | Keyword::JOIN
            | Keyword::INNER
            | Keyword::LEFT
            | Keyword::RIGHT
            | Keyword::FULL
            | Keyword::CROSS
            | Keyword::OUTER
    )
}

fn is_join_modifier(word: &str) -> bool {
    let upper = word.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "JOIN" | "INNER" | "LEFT" | "RIGHT" | "FULL" | "CROSS" | "OUTER" | "APPLY"
    )
}

fn is_whitespace_or_newline(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
    )
}

/// Build a map of line_index -> actual indent width from the SQL text.
fn actual_indent_map(sql: &str, tab_space_size: usize) -> BTreeMap<usize, usize> {
    let mut result = BTreeMap::new();
    for (idx, line) in sql.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = leading_indent_from_prefix(line, tab_space_size);
        result.insert(idx, indent.width);
    }
    result
}

// ---------------------------------------------------------------------------
// Original indent snapshot and autofix infrastructure
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct LeadingIndent {
    width: usize,
    space_count: usize,
    tab_count: usize,
    has_mixed_indent_chars: bool,
}

#[derive(Clone, Copy)]
struct LineIndentSnapshot {
    line_index: usize,
    indent: LeadingIndent,
}

struct StatementLineInfo {
    start: usize,
    indent_end: usize,
}

#[derive(Clone)]
struct Lt02AutofixEdit {
    start: usize,
    end: usize,
    replacement: String,
}

fn should_prefer_lt02_edit(candidate: &Lt02AutofixEdit, current: &Lt02AutofixEdit) -> bool {
    // Mirror planner ordering for same-start conflicts:
    // lower `end` wins, then lexicographically lower replacement.
    if candidate.end != current.end {
        return candidate.end < current.end;
    }

    candidate.replacement < current.replacement
}

fn should_prefer_lt02_patch(candidate: &IssuePatchEdit, current: &IssuePatchEdit) -> bool {
    // Mirror planner ordering for same-start conflicts.
    if candidate.span.end != current.span.end {
        return candidate.span.end < current.span.end;
    }

    candidate.replacement < current.replacement
}

fn collapse_lt02_autofix_edits_by_start(edits: Vec<Lt02AutofixEdit>) -> Vec<Lt02AutofixEdit> {
    let mut by_start: BTreeMap<usize, Lt02AutofixEdit> = BTreeMap::new();
    for edit in edits {
        match by_start.entry(edit.start) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(edit);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                if should_prefer_lt02_edit(&edit, entry.get()) {
                    entry.insert(edit);
                }
            }
        }
    }
    by_start.into_values().collect()
}

fn line_indent_snapshots(ctx: &RuleContext, tab_space_size: usize) -> Vec<LineIndentSnapshot> {
    if let Some(tokens) = tokenize_with_offsets_for_context(ctx) {
        let statement_start_line = offset_to_line(ctx.sql, ctx.statement_range.start);
        let mut first_token_by_line: BTreeMap<usize, usize> = BTreeMap::new();
        for token in &tokens {
            if token.start < ctx.statement_range.start || token.start >= ctx.statement_range.end {
                continue;
            }
            if is_whitespace_token(&token.token) {
                continue;
            }
            first_token_by_line
                .entry(token.start_line)
                .or_insert(token.start);
        }

        return first_token_by_line
            .into_iter()
            .map(|(line, token_start)| {
                let line_start = ctx.sql[..token_start]
                    .rfind('\n')
                    .map_or(0, |index| index + 1);
                let leading = &ctx.sql[line_start..token_start];
                LineIndentSnapshot {
                    line_index: line.saturating_sub(statement_start_line),
                    indent: leading_indent_from_prefix(leading, tab_space_size),
                }
            })
            .collect();
    }

    let sql = ctx.statement_sql();
    let Some(tokens) = tokenize_with_locations(sql, ctx.dialect()) else {
        return sql
            .lines()
            .enumerate()
            .filter_map(|(line_index, line)| {
                if line.trim().is_empty() {
                    return None;
                }
                Some(LineIndentSnapshot {
                    line_index,
                    indent: leading_indent(line, tab_space_size),
                })
            })
            .collect();
    };

    let mut first_token_by_line: std::collections::BTreeMap<usize, usize> =
        std::collections::BTreeMap::new();
    for token in &tokens {
        if is_whitespace_token(&token.token) {
            continue;
        }
        let line = token.span.start.line as usize;
        let column = token.span.start.column as usize;
        first_token_by_line.entry(line).or_insert(column);
    }

    first_token_by_line
        .into_iter()
        .filter_map(|(line, column)| {
            let line_start = line_col_to_offset(sql, line, 1)?;
            let token_start = line_col_to_offset(sql, line, column)?;
            let leading = &sql[line_start..token_start];
            Some(LineIndentSnapshot {
                line_index: line.saturating_sub(1),
                indent: leading_indent_from_prefix(leading, tab_space_size),
            })
        })
        .collect()
}

#[derive(Clone)]
struct LocatedToken {
    token: Token,
    start: usize,
    start_line: usize,
}

fn tokenize_with_locations(sql: &str, dialect: Dialect) -> Option<Vec<TokenWithSpan>> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    tokenizer.tokenize_with_location().ok()
}

fn tokenize_with_offsets_for_context(ctx: &RuleContext) -> Option<Vec<LocatedToken>> {
    ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        Some(
            tokens
                .iter()
                .filter_map(|token| {
                    token_with_span_offsets(ctx.sql, token).map(|(start, _end)| LocatedToken {
                        token: token.token.clone(),
                        start,
                        start_line: token.span.start.line as usize,
                    })
                })
                .collect::<Vec<_>>(),
        )
    })
}

fn is_whitespace_token(token: &Token) -> bool {
    matches!(
        token,
        Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
    )
}

fn leading_indent(line: &str, tab_space_size: usize) -> LeadingIndent {
    leading_indent_from_prefix(line, tab_space_size)
}

fn leading_indent_from_prefix(prefix: &str, tab_space_size: usize) -> LeadingIndent {
    let mut width = 0usize;
    let mut space_count = 0usize;
    let mut tab_count = 0usize;

    for ch in prefix.chars() {
        match ch {
            ' ' => {
                space_count += 1;
                width += 1;
            }
            '\t' => {
                tab_count += 1;
                width += tab_space_size;
            }
            _ => break,
        }
    }

    LeadingIndent {
        width,
        space_count,
        tab_count,
        has_mixed_indent_chars: space_count > 0 && tab_count > 0,
    }
}

fn indentation_autofix_edits(
    statement_sql: &str,
    snapshots: &[LineIndentSnapshot],
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) -> Vec<Lt02AutofixEdit> {
    let line_infos = statement_line_infos(statement_sql);
    let mut edits = Vec::new();

    for snapshot in snapshots {
        let Some(line_info) = line_infos.get(snapshot.line_index) else {
            continue;
        };
        let start = line_info.start;
        let end = line_info.start + line_info.indent_end;
        if end > statement_sql.len() || start > end {
            continue;
        }

        let current_prefix = &statement_sql[start..end];
        let replacement = if snapshot.line_index == 0 {
            String::new()
        } else {
            normalized_indent_replacement(
                snapshot.indent.width,
                indent_unit,
                tab_space_size,
                indent_style,
            )
        };

        if replacement != current_prefix {
            edits.push(Lt02AutofixEdit {
                start,
                end,
                replacement,
            });
        }
    }

    edits
}

fn statement_line_infos(sql: &str) -> Vec<StatementLineInfo> {
    let mut infos = Vec::new();
    let mut line_start = 0usize;

    for segment in sql.split_inclusive('\n') {
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        let indent_end = line
            .char_indices()
            .find_map(|(index, ch)| {
                if matches!(ch, ' ' | '\t') {
                    None
                } else {
                    Some(index)
                }
            })
            .unwrap_or(line.len());
        infos.push(StatementLineInfo {
            start: line_start,
            indent_end,
        });
        line_start += segment.len();
    }

    infos
}

fn statement_line_index_for_offset(
    line_infos: &[StatementLineInfo],
    offset: usize,
) -> Option<usize> {
    if line_infos.is_empty() {
        return None;
    }

    let idx = line_infos.partition_point(|line| line.start <= offset);
    let line_idx = idx
        .saturating_sub(1)
        .min(line_infos.len().saturating_sub(1));
    Some(line_idx)
}

fn normalized_indent_replacement(
    width: usize,
    indent_unit: usize,
    tab_space_size: usize,
    indent_style: IndentStyle,
) -> String {
    if width == 0 {
        return String::new();
    }

    let rounded = rounded_indent_width(width, indent_unit.max(1));
    if rounded == 0 {
        return String::new();
    }

    match indent_style {
        IndentStyle::Spaces => " ".repeat(rounded),
        IndentStyle::Tabs => {
            let tab_width = tab_space_size.max(1);
            let tab_count = rounded.div_ceil(tab_width).max(1);
            "\t".repeat(tab_count)
        }
    }
}

fn rounded_indent_width(width: usize, indent_unit: usize) -> usize {
    if width == 0 || indent_unit == 0 {
        return width;
    }

    if width.is_multiple_of(indent_unit) {
        return width;
    }

    let down = (width / indent_unit) * indent_unit;
    let up = down + indent_unit;
    if down == 0 {
        up
    } else if width - down <= up - width {
        down
    } else {
        up
    }
}

fn ceil_indent_width(width: usize, indent_unit: usize) -> usize {
    if width == 0 || indent_unit == 0 {
        return width;
    }
    width.div_ceil(indent_unit) * indent_unit
}

fn line_col_to_offset(sql: &str, line: usize, column: usize) -> Option<usize> {
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

fn token_with_span_offsets(sql: &str, token: &TokenWithSpan) -> Option<(usize, usize)> {
    let start = line_col_to_offset(
        sql,
        token.span.start.line as usize,
        token.span.start.column as usize,
    )?;
    let end = line_col_to_offset(
        sql,
        token.span.end.line as usize,
        token.span.end.column as usize,
    )?;
    Some((start, end))
}

fn offset_to_line(sql: &str, offset: usize) -> usize {
    1 + sql[..offset.min(sql.len())]
        .chars()
        .filter(|ch| *ch == '\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;
    use crate::types::{Dialect, IssueAutofixApplicability};

    fn run(sql: &str) -> Vec<Issue> {
        run_with_config(sql, LintConfig::default())
    }

    fn run_with_config(sql: &str, config: LintConfig) -> Vec<Issue> {
        run_with_config_in_dialect(sql, Dialect::Generic, config)
    }

    fn run_with_config_in_dialect(sql: &str, dialect: Dialect, config: LintConfig) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutIndent::from_config(&config);
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(
                    statement,
                    &RuleContext::new(sql, 0..sql.len(), index).with_dialect(dialect),
                )
            })
            .collect()
    }

    fn run_postgres(sql: &str) -> Vec<Issue> {
        run_with_config_in_dialect(sql, Dialect::Postgres, LintConfig::default())
    }

    fn apply_issue_autofix(sql: &str, issue: &Issue) -> Option<String> {
        let autofix = issue.autofix.as_ref()?;
        let mut out = sql.to_string();
        let mut edits = autofix.edits.clone();
        edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
        for edit in edits.into_iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    fn apply_all_issue_autofixes(sql: &str, issues: &[Issue]) -> Option<String> {
        let mut all_edits = Vec::new();
        for issue in issues {
            if let Some(autofix) = issue.autofix.as_ref() {
                all_edits.extend(autofix.edits.clone());
            }
        }
        if all_edits.is_empty() {
            return None;
        }

        all_edits.sort_by_key(|edit| (edit.span.start, edit.span.end, edit.replacement.clone()));
        all_edits.dedup_by(|left, right| {
            left.span.start == right.span.start
                && left.span.end == right.span.end
                && left.replacement == right.replacement
        });

        let mut out = sql.to_string();
        for edit in all_edits.into_iter().rev() {
            out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
        }
        Some(out)
    }

    fn normalize_whitespace(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn flags_odd_indent_width() {
        let issues = run("SELECT a\n   , b\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn odd_indent_width_emits_safe_autofix() {
        let sql = "SELECT a\n   , b\nFROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT a\n    , b\nFROM t");
    }

    #[test]
    fn flags_first_line_indentation() {
        let issues = run("   SELECT 1");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn first_line_indentation_emits_safe_autofix_when_editable() {
        let sql = "   SELECT 1";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT 1");
    }

    #[test]
    fn does_not_flag_even_indent_width() {
        assert!(run("SELECT a\n    , b\nFROM t").is_empty());
    }

    #[test]
    fn flags_mixed_tab_and_space_indentation() {
        let issues = run("SELECT a\n \t, b\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn tab_space_size_config_is_applied_for_tab_indentation_width() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.indent".to_string(),
                serde_json::json!({"tab_space_size": 2, "indent_unit": "tab"}),
            )]),
        };
        let issues = run_with_config("SELECT a\n\t, b\nFROM t", config);
        assert!(issues.is_empty());
    }

    #[test]
    fn tab_indent_unit_disallows_space_indent() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.indent".to_string(),
                serde_json::json!({"indent_unit": "tab"}),
            )]),
        };
        let issues = run_with_config("SELECT a\n    , b\nFROM t", config);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn tab_indent_style_emits_tab_autofix() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "layout.indent".to_string(),
                serde_json::json!({"indent_unit": "tab"}),
            )]),
        };
        let sql = "SELECT a\n    , b\nFROM t";
        let issues = run_with_config(sql, config);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT a\n\t, b\nFROM t");
    }

    #[test]
    fn indentation_section_options_are_supported() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "indentation".to_string(),
                serde_json::json!({"indent_unit": "tab", "tab_space_size": 2}),
            )]),
        };
        let issues = run_with_config("SELECT a\n\t, b\nFROM t", config);
        assert!(issues.is_empty());
    }

    #[test]
    fn indentation_on_comment_line_is_checked() {
        let issues = run("SELECT 1\n   -- comment\nFROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn first_line_indent_outside_statement_range_is_report_only() {
        let sql = "   SELECT 1";
        let statements = parse_sql(sql).expect("parse");
        let rule = LayoutIndent::default();
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 3..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0].autofix.is_none(),
            "non-editable first-line prefix should remain report-only"
        );
    }

    #[test]
    fn fragmented_non_semicolon_statement_triggers_first_line_indent_guard() {
        let sql = "SELECT\n    a";
        assert!(
            ignore_first_line_indent_for_fragmented_statement(&RuleContext::new(
                sql,
                7..sql.len(),
                1
            )),
            "fragmented follow-on statement chunks should ignore first-line LT02"
        );
    }

    // Structural indentation tests.

    #[test]
    fn flags_clause_content_not_indented_under_update() {
        // UPDATE\nfoo\nSET\nupdated = now()\nWHERE\n    bar = '';
        // "foo" should be indented under UPDATE, "updated" under SET.
        let issues = run("UPDATE\nfoo\nSET\nupdated = now()\nWHERE\n    bar = '';");
        assert_eq!(issues.len(), 1, "should flag unindented clause contents");
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn flags_unindented_from_content() {
        // FROM\nmy_tbl should flag because my_tbl is not indented.
        let issues = run("SELECT\n    a,\n    b\nFROM\nmy_tbl");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn accepts_properly_indented_clauses() {
        // All clause contents properly indented.
        let issues = run("SELECT\n    a,\n    b\nFROM\n    my_tbl\nWHERE\n    a = 1");
        assert!(issues.is_empty(), "properly indented SQL should not flag");
    }

    #[test]
    fn flags_trailing_comment_wrong_indent() {
        // Trailing comments at deepening indent levels after content at
        // indent 0. Both `-- foo` (indent 4) and `-- bar` (indent 8) are
        // deeper than `SELECT 1` (indent 0).
        let issues = run("SELECT 1\n    -- foo\n        -- bar");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn accepts_properly_indented_trailing_comment() {
        // Comment at same indent as the content line before it is fine.
        let issues = run("SELECT\n    a\n    -- explains next col\n    , b\nFROM t");
        assert!(issues.is_empty());
    }

    // Structural autofix tests.

    #[test]
    fn structural_autofix_indents_content_under_clause_keyword() {
        // RETURNING\nupdated should fix to RETURNING\n    updated
        let sql = "INSERT INTO foo (updated)\nVALUES (now())\nRETURNING\nupdated;";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "INSERT INTO foo (updated)\nVALUES (now())\nRETURNING\n    updated;"
        );
    }

    #[test]
    fn structural_autofix_indents_update_content() {
        // UPDATE\nfoo -> UPDATE\n    foo
        let sql = "UPDATE\nfoo\nSET\nupdated = now()\nWHERE\n    bar = ''";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            fixed,
            "UPDATE\n    foo\nSET\n    updated = now()\nWHERE\n    bar = ''"
        );
    }

    #[test]
    fn structural_autofix_indents_from_content() {
        let sql = "SELECT\n    a,\n    b\nFROM\nmy_tbl";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT\n    a,\n    b\nFROM\n    my_tbl");
    }

    #[test]
    fn structural_autofix_fixes_trailing_comment_indent() {
        let sql = "SELECT 1\n    -- foo\n        -- bar";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        // Both comments should be at indent 0 (same as content line).
        assert_eq!(fixed, "SELECT 1\n-- foo\n-- bar");
    }

    #[test]
    fn structural_autofix_does_not_add_parenthesis_spacing() {
        let sql = "SELECT coalesce(foo,\n              bar)\n   FROM tbl";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(
            normalize_whitespace(&fixed),
            "SELECT coalesce(foo, bar) FROM tbl"
        );
    }

    #[test]
    fn detects_tsql_successive_else_if_indent_violation() {
        let sql = "IF (1 > 1)\n    PRINT 'A';\n    ELSE IF (2 > 2)\n        PRINT 'B';\n        ELSE IF (3 > 3)\n            PRINT 'C';\n            ELSE\n                PRINT 'D';\n";
        assert!(detect_tsql_else_if_successive_violation(sql, 4));
    }

    #[test]
    fn allows_tsql_proper_else_if_chain() {
        let sql = "IF (1 > 1)\n    PRINT 'A';\nELSE IF (2 > 2)\n    PRINT 'B';\nELSE IF (3 > 3)\n    PRINT 'C';\nELSE\n    PRINT 'D';\n";
        assert!(!detect_tsql_else_if_successive_violation(sql, 4));
    }

    #[test]
    fn mssql_partial_parse_fallback_detects_successive_else_if_violation() {
        let sql = "IF (1 > 1)\n    PRINT 'A';\n    ELSE IF (2 > 2)\n        PRINT 'B';\n        ELSE IF (3 > 3)\n            PRINT 'C';\n            ELSE\n                PRINT 'D';\n";
        let first_statement = "IF (1 > 1)\n    PRINT 'A';";
        let placeholder = parse_sql("SELECT 1").expect("parse placeholder");
        let rule = LayoutIndent::default();
        let issues = rule.check_with_context(
            &placeholder[0],
            &RuleContext::new(sql, 0..first_statement.len(), 0).with_dialect(Dialect::Mssql),
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_LT_002);
    }

    #[test]
    fn postgres_where_inline_condition_chain_autofixes() {
        let sql = "SELECT\n    a\nFROM t\nWHERE a = 1\nAND b = 2";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    a\nFROM t\nWHERE\n    a = 1\n    AND b = 2"
        );
    }

    #[test]
    fn postgres_having_inline_condition_chain_autofixes() {
        let sql = "SELECT\n    workspace_id\nFROM t\nHAVING SUM(cost_usd) > 0\n    AND AVG(cost_usd) < 10";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    workspace_id\nFROM t\nHAVING\n    SUM(cost_usd) > 0\n    AND AVG(cost_usd) < 10"
        );
    }

    #[test]
    fn postgres_where_inline_operator_continuation_autofixes() {
        let sql = "SELECT\n    1\nFROM t\nWHERE is_active\n= TRUE";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    1\nFROM t\nWHERE\n    is_active\n    = TRUE"
        );
    }

    #[test]
    fn postgres_standalone_where_block_autofixes_with_other_lt02_violation() {
        let sql = "SELECT\n   1\nFROM t\nWHERE\na = 1\nAND b = 2";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    1\nFROM t\nWHERE\n    a = 1\n    AND b = 2"
        );
    }

    #[test]
    fn postgres_standalone_where_block_under_exists_autofixes() {
        let sql = "SELECT\n    1\nFROM t\nWHERE\n    EXISTS (\n        SELECT 1 FROM ledger.cluster_live_status cls\n        JOIN ledger.cluster c ON c.cluster_id = cls.cluster_id\n        WHERE\n          c.id = i.subject_id::uuid\n          AND cls.state != 'RUNNING'\n    )";
        let mut postgres_only = sql.to_string();
        let mut postgres_only_edits = collapse_lt02_autofix_edits_by_start(
            postgres_keyword_break_and_indent_edits(sql, 4, 4, IndentStyle::Spaces),
        );
        let line_infos = statement_line_infos(sql);
        let debug_edits: Vec<String> = postgres_only_edits
            .iter()
            .map(|edit| {
                let line = statement_line_index_for_offset(&line_infos, edit.start)
                    .map(|line_idx| line_idx + 1)
                    .unwrap_or(0);
                format!(
                    "line={line} start={} end={} replacement={:?}",
                    edit.start,
                    edit.end,
                    edit.replacement.replace(' ', "·")
                )
            })
            .collect();
        postgres_only_edits.sort_by(|left, right| right.start.cmp(&left.start));
        for edit in postgres_only_edits {
            if edit.start <= edit.end && edit.end <= postgres_only.len() {
                postgres_only.replace_range(edit.start..edit.end, &edit.replacement);
            }
        }
        assert!(
            postgres_only.contains(
                "        WHERE\n            c.id = i.subject_id::uuid\n            AND cls.state != 'RUNNING'"
            ),
            "postgres structural edits should indent standalone WHERE block, got:\n{postgres_only}\nedits={debug_edits:#?}"
        );

        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert!(
            fixed.contains(
                "        WHERE\n            c.id = i.subject_id::uuid\n            AND cls.state != 'RUNNING'"
            ),
            "nested standalone WHERE block should indent condition lines under WHERE, got:\n{fixed}"
        );
    }

    #[test]
    fn postgres_trailing_as_alias_break_autofixes() {
        let sql = "SELECT\n    o.id AS\n    org_unit_id\nFROM t AS o";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    o.id\n        AS\n        org_unit_id\nFROM t AS o"
        );
    }

    #[test]
    fn postgres_on_conflict_set_block_autofixes() {
        let sql = "INSERT INTO foo (id, value)\nVALUES (1, 'x')\nON CONFLICT (id) DO UPDATE\nSET value = EXCLUDED.value,\nupdated_at = NOW()";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "INSERT INTO foo (id, value)\nVALUES (1, 'x')\nON CONFLICT (id) DO UPDATE\n    SET\n        value = EXCLUDED.value,\n        updated_at = NOW()"
        );
    }

    #[test]
    fn postgres_select_after_union_operator_autofixes() {
        let sql = "SELECT\n    1 AS a\nUNION ALL\nSELECT a, b";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(fixed, "SELECT\n    1 AS a\nUNION ALL\nSELECT\n    a, b");
    }

    #[test]
    fn postgres_unioned_identifier_does_not_trigger_union_select_break() {
        let sql = "WITH unioned AS (\n    SELECT a, b\n    FROM t\n)\nSELECT\n    a\nFROM unioned";
        let issues = run_postgres(sql);
        assert!(
            issues.is_empty(),
            "UNIONED identifier should not be treated like a UNION set operator"
        );
    }

    #[test]
    fn postgres_where_block_with_nested_subquery_autofixes() {
        let sql =
            "SELECT\n    1\nFROM t\nWHERE a = 1\nAND b IN (\nSELECT 1\nWHERE TRUE\n)\nAND c = 2";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    1\nFROM t\nWHERE\n    a = 1\n    AND b IN (\n        SELECT 1\n        WHERE TRUE\n    )\n    AND c = 2"
        );
    }

    #[test]
    fn postgres_inline_join_on_with_operator_continuation_autofixes() {
        let sql = "SELECT\n    1\nFROM foo AS f\nINNER\nJOIN bar AS b ON f.id = b.id AND b.is_current\n= TRUE";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    1\nFROM foo AS f\nINNER\nJOIN bar AS b\n    ON f.id = b.id AND b.is_current\n        = TRUE"
        );
    }

    #[test]
    fn postgres_standalone_on_block_with_nested_parens_autofixes() {
        let sql = "SELECT\n    1\nFROM foo AS c\nLEFT JOIN bar AS v\n    ON\n    -- Non-PIPELINE: exact version_key match\n        (c.cluster_source <> 'PIPELINE' AND c.dbr_version = v.version_key)\n    OR\n    -- PIPELINE: match on main_version + photon\n    (c.cluster_source = 'PIPELINE'\n    AND c.parsed_main_version = v.main_version\n    AND c.parsed_is_photon = v.is_photon\n    AND v.is_lts = TRUE)";
        let mut fixed = sql.to_string();
        let mut edits = collapse_lt02_autofix_edits_by_start(
            postgres_keyword_break_and_indent_edits(sql, 4, 4, IndentStyle::Spaces),
        );
        edits.sort_by(|left, right| right.start.cmp(&left.start));
        for edit in edits {
            if edit.start <= edit.end && edit.end <= fixed.len() {
                fixed.replace_range(edit.start..edit.end, &edit.replacement);
            }
        }

        assert!(
            fixed.contains(
                "        OR\n        -- PIPELINE: match on main_version + photon\n        (c.cluster_source = 'PIPELINE'\n            AND c.parsed_main_version = v.main_version\n            AND c.parsed_is_photon = v.is_photon\n            AND v.is_lts = TRUE)"
            ),
            "standalone ON block should indent nested AND conditions, got:\n{fixed}"
        );
    }

    #[test]
    fn postgres_inline_case_when_block_autofixes() {
        let sql =
            "SELECT\n    CASE WHEN a > 0\n        THEN 1\n        ELSE 0\n    END AS x\nFROM t";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    CASE\n        WHEN a > 0\n            THEN 1\n        ELSE 0\n    END AS x\nFROM t"
        );
    }

    #[test]
    fn postgres_when_trailing_continuation_autofixes() {
        let sql = "SELECT\n    CASE\n        WHEN COALESCE(a,\n            b) > 0\n            THEN 1\n        ELSE 0\n    END AS x\nFROM t";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    CASE\n        WHEN\n            COALESCE(a,\n            b) > 0\n            THEN 1\n        ELSE 0\n    END AS x\nFROM t"
        );
    }

    #[test]
    fn postgres_then_trailing_continuation_autofixes() {
        let sql = "SELECT\n    CASE\n        WHEN a > 0\n            THEN GREATEST(0,\n                a - 1)\n        ELSE 0\n    END AS x\nFROM t";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    CASE\n        WHEN a > 0\n            THEN\n                GREATEST(0,\n                a - 1)\n        ELSE 0\n    END AS x\nFROM t"
        );
    }

    #[test]
    fn postgres_partition_by_multiline_continuation_autofixes() {
        let sql = "SELECT\n    SUM(cost_usd) OVER (PARTITION BY workspace_id,\n        usage_date) AS running_cost\nFROM t";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    SUM(cost_usd) OVER (PARTITION BY\n        workspace_id,\n        usage_date) AS running_cost\nFROM t"
        );
    }

    #[test]
    fn postgres_select_star_with_continuation_autofixes() {
        let sql = "SELECT\n    *\nFROM (\n    SELECT *,\n        ROW_NUMBER() OVER (PARTITION BY id ORDER BY ts DESC) AS rn\n    FROM t\n) AS ranked";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    *\nFROM (\n    SELECT\n        *,\n        ROW_NUMBER() OVER (PARTITION BY id ORDER BY ts DESC) AS rn\n    FROM t\n) AS ranked"
        );
    }

    #[test]
    fn postgres_case_when_multiline_expression_autofixes() {
        let sql = "SELECT\n    workspace_id,\n    table_full_name,\n    usage_date,\n    COUNT(*) AS query_count,\n    SUM(COALESCE(pruned_files_count, 0)) AS total_pruned_files,\n    SUM(COALESCE(read_files_count, 0)) AS total_read_files,\n    CASE WHEN SUM(COALESCE(pruned_files_count, 0) + COALESCE(read_files_count,\n        0)) > 0\n            THEN\n            SUM(COALESCE(pruned_files_count, 0))::numeric\n            / SUM(COALESCE(pruned_files_count, 0) + COALESCE(read_files_count,\n                0))\n    END AS pruning_ratio\nFROM query_files";
        let issues = run_postgres(sql);
        assert!(
            issues.iter().any(|issue| issue.autofix.is_some()),
            "expected LT02 autofix for CASE WHEN multiline expression"
        );
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    workspace_id,\n    table_full_name,\n    usage_date,\n    COUNT(*) AS query_count,\n    SUM(COALESCE(pruned_files_count, 0)) AS total_pruned_files,\n    SUM(COALESCE(read_files_count, 0)) AS total_read_files,\n    CASE\n        WHEN\n            SUM(COALESCE(pruned_files_count, 0) + COALESCE(read_files_count,\n        0)) > 0\n            THEN\n            SUM(COALESCE(pruned_files_count, 0))::numeric\n            / SUM(COALESCE(pruned_files_count, 0) + COALESCE(read_files_count,\n                0))\n    END AS pruning_ratio\nFROM query_files"
        );
    }

    #[test]
    fn postgres_case_when_multiline_expression_generates_keyword_edits() {
        let sql = "SELECT\n    workspace_id,\n    table_full_name,\n    usage_date,\n    COUNT(*) AS query_count,\n    SUM(COALESCE(pruned_files_count, 0)) AS total_pruned_files,\n    SUM(COALESCE(read_files_count, 0)) AS total_read_files,\n    CASE WHEN SUM(COALESCE(pruned_files_count, 0) + COALESCE(read_files_count,\n        0)) > 0\n            THEN\n            SUM(COALESCE(pruned_files_count, 0))::numeric\n            / SUM(COALESCE(pruned_files_count, 0) + COALESCE(read_files_count,\n                0))\n    END AS pruning_ratio\nFROM query_files";
        let edits = postgres_keyword_break_and_indent_edits(sql, 4, 4, IndentStyle::Spaces);
        assert!(
            edits.iter().any(|edit| edit.replacement.contains('\n')),
            "expected postgres keyword-break edits for CASE WHEN multiline expression"
        );
    }

    #[test]
    fn postgres_partition_by_inline_case_autofixes() {
        let sql = "SELECT\n    ROW_NUMBER() OVER (\n        PARTITION BY CASE\n            WHEN a > 0\n            THEN 1\n            ELSE 0\n        END\n    ) AS rn\nFROM t";
        let issues = run_postgres(sql);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|issue| issue.autofix.is_some()));
        let fixed = apply_all_issue_autofixes(sql, &issues).expect("apply all autofixes");
        assert_eq!(
            fixed,
            "SELECT\n    ROW_NUMBER() OVER (\n        PARTITION BY\n            CASE\n                WHEN a > 0\n                    THEN 1\n                ELSE 0\n            END\n    ) AS rn\nFROM t"
        );
    }
}
