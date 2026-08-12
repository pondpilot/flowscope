//! LINT_CP_004: Literal capitalisation.
//!
//! SQLFluff CP04 parity (current scope): detect mixed-case usage for
//! NULL/TRUE/FALSE literal keywords.

use std::collections::HashSet;

use crate::linter::config::LintConfig;
use crate::linter::rule::{BuiltinLintRule, RuleContext};
use crate::types::{issue_codes, Dialect, Issue, IssueAutofixApplicability, IssuePatchEdit, Span};
use regex::Regex;
use sqlparser::ast::Statement;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

use super::capitalisation_policy_helpers::{
    ignored_words_from_config, ignored_words_regex_from_config, token_is_ignored,
    tokens_violate_policy, CapitalisationPolicy,
};

pub struct CapitalisationLiterals {
    policy: CapitalisationPolicy,
    ignore_words: HashSet<String>,
    ignore_words_regex: Option<Regex>,
}

impl CapitalisationLiterals {
    pub fn from_config(config: &LintConfig) -> Self {
        // SQLFluff accepts both `extended_capitalisation_policy` and the
        // shorter `capitalisation_policy` for CP04.
        let policy = config
            .rule_option_str(issue_codes::LINT_CP_004, "extended_capitalisation_policy")
            .or_else(|| config.rule_option_str(issue_codes::LINT_CP_004, "capitalisation_policy"))
            .map(CapitalisationPolicy::from_raw_value)
            .unwrap_or(CapitalisationPolicy::Consistent);

        Self {
            policy,
            ignore_words: ignored_words_from_config(config, issue_codes::LINT_CP_004),
            ignore_words_regex: ignored_words_regex_from_config(config, issue_codes::LINT_CP_004),
        }
    }
}

impl Default for CapitalisationLiterals {
    fn default() -> Self {
        Self {
            policy: CapitalisationPolicy::Consistent,
            ignore_words: HashSet::new(),
            ignore_words_regex: None,
        }
    }
}

impl BuiltinLintRule for CapitalisationLiterals {
    fn code(&self) -> &'static str {
        issue_codes::LINT_CP_004
    }

    fn name(&self) -> &'static str {
        "Literal capitalisation"
    }

    fn description(&self) -> &'static str {
        "Inconsistent capitalisation of boolean/null literal."
    }

    fn check_with_context(&self, _statement: &Statement, ctx: &RuleContext) -> Vec<Issue> {
        let literals =
            literal_tokens_for_context(ctx, &self.ignore_words, self.ignore_words_regex.as_ref());
        let literal_values = literals
            .iter()
            .map(|candidate| candidate.value.clone())
            .collect::<Vec<_>>();
        if !tokens_violate_policy(&literal_values, self.policy) {
            return Vec::new();
        }

        let autofix_edits = literal_autofix_edits(ctx, &literals, self.policy);

        // Emit one issue per violating literal at its specific position.
        if autofix_edits.is_empty() {
            return vec![Issue::info(
                issue_codes::LINT_CP_004,
                "Literal keywords (NULL/TRUE/FALSE) use inconsistent capitalisation.",
            )
            .with_statement(ctx.statement_index)];
        }

        autofix_edits
            .into_iter()
            .map(|edit| {
                let span = Span::new(edit.span.start, edit.span.end);
                Issue::info(
                    issue_codes::LINT_CP_004,
                    "Literal keywords (NULL/TRUE/FALSE) use inconsistent capitalisation.",
                )
                .with_statement(ctx.statement_index)
                .with_span(span)
                .with_autofix_edits(IssueAutofixApplicability::Safe, vec![edit])
            })
            .collect()
    }
}

#[derive(Clone)]
struct LiteralCandidate {
    value: String,
    start: usize,
    end: usize,
}

fn literal_tokens_for_context(
    ctx: &RuleContext,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
) -> Vec<LiteralCandidate> {
    let from_document_tokens = ctx.with_document_tokens(|tokens| {
        if tokens.is_empty() {
            return None;
        }

        let mut out = Vec::new();
        for token in tokens {
            let Some((start, end)) = token_with_span_offsets(ctx.sql, token) else {
                continue;
            };
            if start < ctx.statement_range.start || end > ctx.statement_range.end {
                continue;
            }

            if let Token::Word(word) = &token.token {
                // Document token spans are tied to rendered SQL. If the source
                // slice does not match the token text, fall back to
                // statement-local tokenization.
                if !source_word_matches(ctx.sql, start, end, word.value.as_str()) {
                    return None;
                }
                if matches!(
                    word.value.to_ascii_uppercase().as_str(),
                    "NULL" | "TRUE" | "FALSE"
                ) && !token_is_ignored(word.value.as_str(), ignore_words, ignore_words_regex)
                {
                    let Some(local_start) = start.checked_sub(ctx.statement_range.start) else {
                        continue;
                    };
                    let Some(local_end) = end.checked_sub(ctx.statement_range.start) else {
                        continue;
                    };
                    out.push(LiteralCandidate {
                        value: word.value.clone(),
                        start: local_start,
                        end: local_end,
                    });
                }
            }
        }
        Some(out)
    });

    if let Some(tokens) = from_document_tokens {
        return tokens;
    }

    literal_tokens(
        ctx.statement_sql(),
        ignore_words,
        ignore_words_regex,
        ctx.dialect(),
    )
}

fn literal_tokens(
    sql: &str,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
    dialect: Dialect,
) -> Vec<LiteralCandidate> {
    let dialect = dialect.to_sqlparser_dialect();
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let Ok(tokens) = tokenizer.tokenize_with_location() else {
        return Vec::new();
    };

    tokens
        .into_iter()
        .filter_map(|token| {
            if let Token::Word(word) = &token.token {
                if matches!(
                    word.value.to_ascii_uppercase().as_str(),
                    "NULL" | "TRUE" | "FALSE"
                ) && !token_is_ignored(word.value.as_str(), ignore_words, ignore_words_regex)
                {
                    let (start, end) = token_with_span_offsets(sql, &token)?;
                    return Some(LiteralCandidate {
                        value: word.value.clone(),
                        start,
                        end,
                    });
                }
            }
            None
        })
        .collect()
}

fn literal_autofix_edits(
    ctx: &RuleContext,
    literals: &[LiteralCandidate],
    policy: CapitalisationPolicy,
) -> Vec<IssuePatchEdit> {
    // For `Consistent` mode, resolve to the majority case among the
    // literal tokens so that the fewest edits are generated.
    let resolved = if policy == CapitalisationPolicy::Consistent {
        resolve_consistent_policy(literals)
    } else {
        policy
    };

    let mut edits = Vec::new();

    for candidate in literals {
        let Some(replacement) = literal_case_replacement(candidate.value.as_str(), resolved) else {
            continue;
        };
        if replacement == candidate.value {
            continue;
        }

        edits.push(IssuePatchEdit::new(
            ctx.span_from_statement_offset(candidate.start, candidate.end),
            replacement,
        ));
    }

    edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
    edits.dedup_by(|left, right| {
        left.span.start == right.span.start
            && left.span.end == right.span.end
            && left.replacement == right.replacement
    });
    edits
}

fn literal_case_replacement(value: &str, policy: CapitalisationPolicy) -> Option<String> {
    match policy {
        CapitalisationPolicy::Lower => Some(value.to_ascii_lowercase()),
        CapitalisationPolicy::Upper => Some(value.to_ascii_uppercase()),
        CapitalisationPolicy::Capitalise => Some(capitalise_ascii_token(value)),
        // `Consistent` should be resolved to Upper/Lower before calling
        // this function.  Fall back to lowercase if somehow unresolved.
        CapitalisationPolicy::Consistent => Some(value.to_ascii_lowercase()),
        // These policies are currently report-only in CP04 autofix scope.
        CapitalisationPolicy::Pascal
        | CapitalisationPolicy::Camel
        | CapitalisationPolicy::Snake => None,
    }
}

/// Resolve `Consistent` mode by adopting the style of the first literal token.
fn resolve_consistent_policy(literals: &[LiteralCandidate]) -> CapitalisationPolicy {
    for lit in literals {
        if lit.value == lit.value.to_ascii_uppercase() {
            return CapitalisationPolicy::Upper;
        }
        if lit.value == lit.value.to_ascii_lowercase() {
            return CapitalisationPolicy::Lower;
        }
    }
    CapitalisationPolicy::Lower
}

fn capitalise_ascii_token(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut seen_alpha = false;

    for ch in value.chars() {
        if !ch.is_ascii_alphabetic() {
            out.push(ch);
            continue;
        }

        if !seen_alpha {
            out.push(ch.to_ascii_uppercase());
            seen_alpha = true;
        } else {
            out.push(ch.to_ascii_lowercase());
        }
    }

    out
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

fn source_word_matches(sql: &str, start: usize, end: usize, value: &str) -> bool {
    let Some(raw) = sql.get(start..end) else {
        return false;
    };
    let normalized = raw.trim_matches(|ch| matches!(ch, '"' | '`' | '[' | ']'));
    normalized.eq_ignore_ascii_case(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::config::LintConfig;
    use crate::parser::parse_sql;
    use crate::types::IssueAutofixApplicability;

    fn run(sql: &str) -> Vec<Issue> {
        let statements = parse_sql(sql).expect("parse");
        let rule = CapitalisationLiterals::default();
        statements
            .iter()
            .enumerate()
            .flat_map(|(index, statement)| {
                rule.check_with_context(statement, &RuleContext::new(sql, 0..sql.len(), index))
            })
            .collect()
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

    #[test]
    fn flags_mixed_literal_case() {
        let issues = run("SELECT NULL, true FROM t");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, issue_codes::LINT_CP_004);
    }

    #[test]
    fn emits_safe_autofix_for_mixed_literal_case() {
        // Consistent mode detects the majority case among literal tokens.
        // NULL (upper) vs true (lower) — tie goes to upper.
        let sql = "SELECT NULL, true FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let autofix = issues[0].autofix.as_ref().expect("autofix metadata");
        assert_eq!(autofix.applicability, IssueAutofixApplicability::Safe);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT NULL, TRUE FROM t");
    }

    #[test]
    fn does_not_flag_consistent_literal_case() {
        assert!(run("SELECT NULL, TRUE FROM t").is_empty());
    }

    #[test]
    fn does_not_flag_literal_words_in_strings_or_comments() {
        let sql = "SELECT 'null true false' AS txt -- NULL true\nFROM t";
        assert!(run(sql).is_empty());
    }

    #[test]
    fn upper_policy_flags_lowercase_literal() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.literals".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "upper"}),
            )]),
        };
        let rule = CapitalisationLiterals::from_config(&config);
        let sql = "SELECT true FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn upper_policy_emits_uppercase_autofix() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.literals".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "upper"}),
            )]),
        };
        let rule = CapitalisationLiterals::from_config(&config);
        let sql = "SELECT null, true FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        // Both null and true violate upper → 2 violations.
        assert_eq!(issues.len(), 2);
        let fixed = {
            let mut edits: Vec<_> = issues
                .iter()
                .filter_map(|i| i.autofix.as_ref())
                .flat_map(|a| a.edits.clone())
                .collect();
            edits.sort_by_key(|e| (e.span.start, e.span.end));
            let mut out = sql.to_string();
            for edit in edits.into_iter().rev() {
                out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
            }
            out
        };
        assert_eq!(fixed, "SELECT NULL, TRUE FROM t");
    }

    #[test]
    fn camel_policy_violation_remains_report_only() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.literals".to_string(),
                serde_json::json!({"extended_capitalisation_policy": "camel"}),
            )]),
        };
        let rule = CapitalisationLiterals::from_config(&config);
        let sql = "SELECT NULL, TRUE FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0].autofix.is_none(),
            "camel/pascal/snake are report-only in current CP004 autofix scope"
        );
    }

    #[test]
    fn consistent_majority_lowercase_emits_lowercase_autofix() {
        // true (lower), false (lower) vs NULL (upper) — majority lowercase.
        let sql = "SELECT true, false, NULL FROM t";
        let issues = run(sql);
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT true, false, null FROM t");
    }

    #[test]
    fn capitalisation_policy_config_key_fallback() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.literals".to_string(),
                serde_json::json!({"capitalisation_policy": "upper"}),
            )]),
        };
        let rule = CapitalisationLiterals::from_config(&config);
        let sql = "SELECT true FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert_eq!(issues.len(), 1);
        let fixed = apply_issue_autofix(sql, &issues[0]).expect("apply autofix");
        assert_eq!(fixed, "SELECT TRUE FROM t");
    }

    #[test]
    fn ignore_words_regex_excludes_literals_from_check() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::from([(
                "capitalisation.literals".to_string(),
                serde_json::json!({"ignore_words_regex": "^true$"}),
            )]),
        };
        let rule = CapitalisationLiterals::from_config(&config);
        let sql = "SELECT NULL, true FROM t";
        let statements = parse_sql(sql).expect("parse");
        let issues =
            rule.check_with_context(&statements[0], &RuleContext::new(sql, 0..sql.len(), 0));
        assert!(issues.is_empty());
    }
}
