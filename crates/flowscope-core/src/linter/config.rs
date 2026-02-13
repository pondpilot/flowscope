//! Configuration for the SQL linter.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// Configuration for the SQL linter.
///
/// Controls which lint rules are enabled/disabled. By default, all rules are enabled.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LintConfig {
    /// Master toggle for linting (default: true).
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// List of rule codes to disable (e.g., ["LINT_AM_008"]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_rules: Vec<String>,

    /// Per-rule option objects keyed by rule reference (`LINT_*`, `AL01`,
    /// `aliasing.table`, etc).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rule_configs: BTreeMap<String, serde_json::Value>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            disabled_rules: Vec::new(),
            rule_configs: BTreeMap::new(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

impl LintConfig {
    /// Returns true if a specific rule is enabled.
    pub fn is_rule_enabled(&self, code: &str) -> bool {
        let requested =
            canonicalize_rule_code(code).unwrap_or_else(|| code.trim().to_ascii_uppercase());
        let disabled: HashSet<String> = self
            .disabled_rules
            .iter()
            .filter_map(|rule| canonicalize_rule_code(rule))
            .collect();

        self.enabled && !disabled.contains(&requested)
    }

    /// Returns a rule-level config object, if present.
    pub fn rule_config_object(
        &self,
        code: &str,
    ) -> Option<&serde_json::Map<String, serde_json::Value>> {
        self.matching_rule_config_value(code)?.as_object()
    }

    /// Returns a string option for a rule config.
    pub fn rule_option_str(&self, code: &str, key: &str) -> Option<&str> {
        self.rule_config_object(code)?.get(key)?.as_str()
    }

    /// Returns a boolean option for a rule config.
    pub fn rule_option_bool(&self, code: &str, key: &str) -> Option<bool> {
        self.rule_config_object(code)?.get(key)?.as_bool()
    }

    /// Returns an unsigned integer option for a rule config.
    pub fn rule_option_usize(&self, code: &str, key: &str) -> Option<usize> {
        let value = self.rule_config_object(code)?.get(key)?.as_u64()?;
        usize::try_from(value).ok()
    }

    /// Returns a list-of-string option for a rule config.
    pub fn rule_option_string_list(&self, code: &str, key: &str) -> Option<Vec<String>> {
        let values = self.rule_config_object(code)?.get(key)?.as_array()?;
        values
            .iter()
            .map(|value| value.as_str().map(str::to_string))
            .collect()
    }

    fn matching_rule_config_value(&self, code: &str) -> Option<&serde_json::Value> {
        let canonical = canonicalize_rule_code(code)?;
        self.rule_configs.iter().find_map(|(rule_ref, value)| {
            (canonicalize_rule_code(rule_ref).as_deref() == Some(canonical.as_str()))
                .then_some(value)
        })
    }
}

/// Canonicalizes a user-facing rule spec to a canonical `LINT_*` code.
pub fn canonicalize_rule_code(rule: &str) -> Option<String> {
    let raw = rule.trim();
    if raw.is_empty() {
        return None;
    }

    let normalized = raw.to_ascii_uppercase();

    if let Some(code) = canonical_lint_code(&normalized) {
        return Some(code.to_string());
    }

    if let Some(code) = underscore_or_short_code_to_lint(&normalized) {
        return Some(code);
    }

    if let Some(code) = dotted_name_to_code(&normalized) {
        return Some(code.to_string());
    }

    None
}

/// SQLFluff dotted rule name for a canonical rule code.
pub fn sqlfluff_name_for_code(code: &str) -> Option<&'static str> {
    let canonical = canonicalize_rule_code(code)?;
    match canonical.as_str() {
        "LINT_AL_001" => Some("aliasing.table"),
        "LINT_AL_002" => Some("aliasing.column"),
        "LINT_AL_003" => Some("aliasing.expression"),
        "LINT_AL_004" => Some("aliasing.unique.table"),
        "LINT_AL_005" => Some("aliasing.unused"),
        "LINT_AL_006" => Some("aliasing.length"),
        "LINT_AL_007" => Some("aliasing.forbid"),
        "LINT_AL_008" => Some("aliasing.unique.column"),
        "LINT_AL_009" => Some("aliasing.self_alias.column"),
        "LINT_AM_001" => Some("ambiguous.distinct"),
        "LINT_AM_002" => Some("ambiguous.union"),
        "LINT_AM_003" => Some("ambiguous.order_by"),
        "LINT_AM_004" => Some("ambiguous.column_count"),
        "LINT_AM_005" => Some("ambiguous.join"),
        "LINT_AM_006" => Some("ambiguous.column_references"),
        "LINT_AM_007" => Some("ambiguous.set_columns"),
        "LINT_AM_008" => Some("ambiguous.join_condition"),
        "LINT_AM_009" => Some("ambiguous.order_by_limit"),
        "LINT_CP_001" => Some("capitalisation.keywords"),
        "LINT_CP_002" => Some("capitalisation.identifiers"),
        "LINT_CP_003" => Some("capitalisation.functions"),
        "LINT_CP_004" => Some("capitalisation.literals"),
        "LINT_CP_005" => Some("capitalisation.types"),
        "LINT_CV_001" => Some("convention.not_equal"),
        "LINT_CV_002" => Some("convention.coalesce"),
        "LINT_CV_003" => Some("convention.select_trailing_comma"),
        "LINT_CV_004" => Some("convention.count_rows"),
        "LINT_CV_005" => Some("convention.is_null"),
        "LINT_CV_006" => Some("convention.terminator"),
        "LINT_CV_007" => Some("convention.statement_brackets"),
        "LINT_CV_008" => Some("convention.left_join"),
        "LINT_CV_009" => Some("convention.blocked_words"),
        "LINT_CV_010" => Some("convention.quoted_literals"),
        "LINT_CV_011" => Some("convention.casting_style"),
        "LINT_CV_012" => Some("convention.join_condition"),
        "LINT_JJ_001" => Some("jinja.padding"),
        "LINT_LT_001" => Some("layout.spacing"),
        "LINT_LT_002" => Some("layout.indent"),
        "LINT_LT_003" => Some("layout.operators"),
        "LINT_LT_004" => Some("layout.commas"),
        "LINT_LT_005" => Some("layout.long_lines"),
        "LINT_LT_006" => Some("layout.functions"),
        "LINT_LT_007" => Some("layout.cte_bracket"),
        "LINT_LT_008" => Some("layout.cte_newline"),
        "LINT_LT_009" => Some("layout.select_targets"),
        "LINT_LT_010" => Some("layout.select_modifiers"),
        "LINT_LT_011" => Some("layout.set_operators"),
        "LINT_LT_012" => Some("layout.end_of_file"),
        "LINT_LT_013" => Some("layout.start_of_file"),
        "LINT_LT_014" => Some("layout.keyword_newline"),
        "LINT_LT_015" => Some("layout.newlines"),
        "LINT_RF_001" => Some("references.from"),
        "LINT_RF_002" => Some("references.qualification"),
        "LINT_RF_003" => Some("references.consistent"),
        "LINT_RF_004" => Some("references.keywords"),
        "LINT_RF_005" => Some("references.special_chars"),
        "LINT_RF_006" => Some("references.quoting"),
        "LINT_ST_001" => Some("structure.else_null"),
        "LINT_ST_002" => Some("structure.simple_case"),
        "LINT_ST_003" => Some("structure.unused_cte"),
        "LINT_ST_004" => Some("structure.nested_case"),
        "LINT_ST_005" => Some("structure.subquery"),
        "LINT_ST_006" => Some("structure.column_order"),
        "LINT_ST_007" => Some("structure.using"),
        "LINT_ST_008" => Some("structure.distinct"),
        "LINT_ST_009" => Some("structure.join_condition_order"),
        "LINT_ST_010" => Some("structure.constant_expression"),
        "LINT_ST_011" => Some("structure.unused_join"),
        "LINT_ST_012" => Some("structure.consecutive_semicolons"),
        "LINT_TQ_001" => Some("tsql.sp_prefix"),
        "LINT_TQ_002" => Some("tsql.procedure_begin_end"),
        "LINT_TQ_003" => Some("tsql.empty_batch"),
        _ => None,
    }
}

fn canonical_lint_code(normalized: &str) -> Option<&'static str> {
    if !normalized.starts_with("LINT_") {
        return None;
    }

    if sqlfluff_name_for_canonical_code(normalized).is_some() {
        Some(match normalized {
            "LINT_AL_001" => "LINT_AL_001",
            "LINT_AL_002" => "LINT_AL_002",
            "LINT_AL_003" => "LINT_AL_003",
            "LINT_AL_004" => "LINT_AL_004",
            "LINT_AL_005" => "LINT_AL_005",
            "LINT_AL_006" => "LINT_AL_006",
            "LINT_AL_007" => "LINT_AL_007",
            "LINT_AL_008" => "LINT_AL_008",
            "LINT_AL_009" => "LINT_AL_009",
            "LINT_AM_001" => "LINT_AM_001",
            "LINT_AM_002" => "LINT_AM_002",
            "LINT_AM_003" => "LINT_AM_003",
            "LINT_AM_004" => "LINT_AM_004",
            "LINT_AM_005" => "LINT_AM_005",
            "LINT_AM_006" => "LINT_AM_006",
            "LINT_AM_007" => "LINT_AM_007",
            "LINT_AM_008" => "LINT_AM_008",
            "LINT_AM_009" => "LINT_AM_009",
            "LINT_CP_001" => "LINT_CP_001",
            "LINT_CP_002" => "LINT_CP_002",
            "LINT_CP_003" => "LINT_CP_003",
            "LINT_CP_004" => "LINT_CP_004",
            "LINT_CP_005" => "LINT_CP_005",
            "LINT_CV_001" => "LINT_CV_001",
            "LINT_CV_002" => "LINT_CV_002",
            "LINT_CV_003" => "LINT_CV_003",
            "LINT_CV_004" => "LINT_CV_004",
            "LINT_CV_005" => "LINT_CV_005",
            "LINT_CV_006" => "LINT_CV_006",
            "LINT_CV_007" => "LINT_CV_007",
            "LINT_CV_008" => "LINT_CV_008",
            "LINT_CV_009" => "LINT_CV_009",
            "LINT_CV_010" => "LINT_CV_010",
            "LINT_CV_011" => "LINT_CV_011",
            "LINT_CV_012" => "LINT_CV_012",
            "LINT_JJ_001" => "LINT_JJ_001",
            "LINT_LT_001" => "LINT_LT_001",
            "LINT_LT_002" => "LINT_LT_002",
            "LINT_LT_003" => "LINT_LT_003",
            "LINT_LT_004" => "LINT_LT_004",
            "LINT_LT_005" => "LINT_LT_005",
            "LINT_LT_006" => "LINT_LT_006",
            "LINT_LT_007" => "LINT_LT_007",
            "LINT_LT_008" => "LINT_LT_008",
            "LINT_LT_009" => "LINT_LT_009",
            "LINT_LT_010" => "LINT_LT_010",
            "LINT_LT_011" => "LINT_LT_011",
            "LINT_LT_012" => "LINT_LT_012",
            "LINT_LT_013" => "LINT_LT_013",
            "LINT_LT_014" => "LINT_LT_014",
            "LINT_LT_015" => "LINT_LT_015",
            "LINT_RF_001" => "LINT_RF_001",
            "LINT_RF_002" => "LINT_RF_002",
            "LINT_RF_003" => "LINT_RF_003",
            "LINT_RF_004" => "LINT_RF_004",
            "LINT_RF_005" => "LINT_RF_005",
            "LINT_RF_006" => "LINT_RF_006",
            "LINT_ST_001" => "LINT_ST_001",
            "LINT_ST_002" => "LINT_ST_002",
            "LINT_ST_003" => "LINT_ST_003",
            "LINT_ST_004" => "LINT_ST_004",
            "LINT_ST_005" => "LINT_ST_005",
            "LINT_ST_006" => "LINT_ST_006",
            "LINT_ST_007" => "LINT_ST_007",
            "LINT_ST_008" => "LINT_ST_008",
            "LINT_ST_009" => "LINT_ST_009",
            "LINT_ST_010" => "LINT_ST_010",
            "LINT_ST_011" => "LINT_ST_011",
            "LINT_ST_012" => "LINT_ST_012",
            "LINT_TQ_001" => "LINT_TQ_001",
            "LINT_TQ_002" => "LINT_TQ_002",
            "LINT_TQ_003" => "LINT_TQ_003",
            _ => return None,
        })
    } else {
        None
    }
}

fn sqlfluff_name_for_canonical_code(code: &str) -> Option<&'static str> {
    match code {
        "LINT_AL_001" => Some("aliasing.table"),
        "LINT_AL_002" => Some("aliasing.column"),
        "LINT_AL_003" => Some("aliasing.expression"),
        "LINT_AL_004" => Some("aliasing.unique.table"),
        "LINT_AL_005" => Some("aliasing.unused"),
        "LINT_AL_006" => Some("aliasing.length"),
        "LINT_AL_007" => Some("aliasing.forbid"),
        "LINT_AL_008" => Some("aliasing.unique.column"),
        "LINT_AL_009" => Some("aliasing.self_alias.column"),
        "LINT_AM_001" => Some("ambiguous.distinct"),
        "LINT_AM_002" => Some("ambiguous.union"),
        "LINT_AM_003" => Some("ambiguous.order_by"),
        "LINT_AM_004" => Some("ambiguous.column_count"),
        "LINT_AM_005" => Some("ambiguous.join"),
        "LINT_AM_006" => Some("ambiguous.column_references"),
        "LINT_AM_007" => Some("ambiguous.set_columns"),
        "LINT_AM_008" => Some("ambiguous.join_condition"),
        "LINT_AM_009" => Some("ambiguous.order_by_limit"),
        "LINT_CP_001" => Some("capitalisation.keywords"),
        "LINT_CP_002" => Some("capitalisation.identifiers"),
        "LINT_CP_003" => Some("capitalisation.functions"),
        "LINT_CP_004" => Some("capitalisation.literals"),
        "LINT_CP_005" => Some("capitalisation.types"),
        "LINT_CV_001" => Some("convention.not_equal"),
        "LINT_CV_002" => Some("convention.coalesce"),
        "LINT_CV_003" => Some("convention.select_trailing_comma"),
        "LINT_CV_004" => Some("convention.count_rows"),
        "LINT_CV_005" => Some("convention.is_null"),
        "LINT_CV_006" => Some("convention.terminator"),
        "LINT_CV_007" => Some("convention.statement_brackets"),
        "LINT_CV_008" => Some("convention.left_join"),
        "LINT_CV_009" => Some("convention.blocked_words"),
        "LINT_CV_010" => Some("convention.quoted_literals"),
        "LINT_CV_011" => Some("convention.casting_style"),
        "LINT_CV_012" => Some("convention.join_condition"),
        "LINT_JJ_001" => Some("jinja.padding"),
        "LINT_LT_001" => Some("layout.spacing"),
        "LINT_LT_002" => Some("layout.indent"),
        "LINT_LT_003" => Some("layout.operators"),
        "LINT_LT_004" => Some("layout.commas"),
        "LINT_LT_005" => Some("layout.long_lines"),
        "LINT_LT_006" => Some("layout.functions"),
        "LINT_LT_007" => Some("layout.cte_bracket"),
        "LINT_LT_008" => Some("layout.cte_newline"),
        "LINT_LT_009" => Some("layout.select_targets"),
        "LINT_LT_010" => Some("layout.select_modifiers"),
        "LINT_LT_011" => Some("layout.set_operators"),
        "LINT_LT_012" => Some("layout.end_of_file"),
        "LINT_LT_013" => Some("layout.start_of_file"),
        "LINT_LT_014" => Some("layout.keyword_newline"),
        "LINT_LT_015" => Some("layout.newlines"),
        "LINT_RF_001" => Some("references.from"),
        "LINT_RF_002" => Some("references.qualification"),
        "LINT_RF_003" => Some("references.consistent"),
        "LINT_RF_004" => Some("references.keywords"),
        "LINT_RF_005" => Some("references.special_chars"),
        "LINT_RF_006" => Some("references.quoting"),
        "LINT_ST_001" => Some("structure.else_null"),
        "LINT_ST_002" => Some("structure.simple_case"),
        "LINT_ST_003" => Some("structure.unused_cte"),
        "LINT_ST_004" => Some("structure.nested_case"),
        "LINT_ST_005" => Some("structure.subquery"),
        "LINT_ST_006" => Some("structure.column_order"),
        "LINT_ST_007" => Some("structure.using"),
        "LINT_ST_008" => Some("structure.distinct"),
        "LINT_ST_009" => Some("structure.join_condition_order"),
        "LINT_ST_010" => Some("structure.constant_expression"),
        "LINT_ST_011" => Some("structure.unused_join"),
        "LINT_ST_012" => Some("structure.consecutive_semicolons"),
        "LINT_TQ_001" => Some("tsql.sp_prefix"),
        "LINT_TQ_002" => Some("tsql.procedure_begin_end"),
        "LINT_TQ_003" => Some("tsql.empty_batch"),
        _ => None,
    }
}

fn underscore_or_short_code_to_lint(normalized: &str) -> Option<String> {
    if normalized.len() == 6 {
        let mut chars = normalized.chars();
        let p0 = chars.next()?;
        let p1 = chars.next()?;
        let underscore = chars.next()?;
        let d0 = chars.next()?;
        let d1 = chars.next()?;
        let d2 = chars.next()?;

        if p0.is_ascii_alphabetic()
            && p1.is_ascii_alphabetic()
            && underscore == '_'
            && d0.is_ascii_digit()
            && d1.is_ascii_digit()
            && d2.is_ascii_digit()
        {
            let lint = format!("LINT_{}", normalized);
            return canonical_lint_code(&lint).map(str::to_string);
        }
    }

    if normalized.len() >= 3 {
        let prefix = &normalized[..2];
        let digits = &normalized[2..];

        if prefix.chars().all(|c| c.is_ascii_alphabetic())
            && digits.chars().all(|c| c.is_ascii_digit())
        {
            let number: usize = digits.parse().ok()?;
            if number == 0 || number > 999 {
                return None;
            }
            let lint = format!("LINT_{}_{number:03}", prefix);
            return canonical_lint_code(&lint).map(str::to_string);
        }
    }

    None
}

fn dotted_name_to_code(alias: &str) -> Option<&'static str> {
    match alias {
        "ALIASING.TABLE" => Some("LINT_AL_001"),
        "ALIASING.COLUMN" => Some("LINT_AL_002"),
        "ALIASING.EXPRESSION" => Some("LINT_AL_003"),
        "ALIASING.UNIQUE.TABLE" => Some("LINT_AL_004"),
        "ALIASING.UNUSED" => Some("LINT_AL_005"),
        "ALIASING.LENGTH" => Some("LINT_AL_006"),
        "ALIASING.FORBID" => Some("LINT_AL_007"),
        "ALIASING.UNIQUE.COLUMN" => Some("LINT_AL_008"),
        "ALIASING.SELF_ALIAS.COLUMN" => Some("LINT_AL_009"),
        "AMBIGUOUS.DISTINCT" => Some("LINT_AM_001"),
        "AMBIGUOUS.UNION" => Some("LINT_AM_002"),
        "AMBIGUOUS.ORDER_BY" => Some("LINT_AM_003"),
        "AMBIGUOUS.COLUMN_COUNT" => Some("LINT_AM_004"),
        "AMBIGUOUS.JOIN" => Some("LINT_AM_005"),
        "AMBIGUOUS.COLUMN_REFERENCES" => Some("LINT_AM_006"),
        "AMBIGUOUS.SET_COLUMNS" => Some("LINT_AM_007"),
        "AMBIGUOUS.JOIN_CONDITION" | "AMBIGUOUS.JOIN.CONDITION" => Some("LINT_AM_008"),
        "AMBIGUOUS.ORDER_BY_LIMIT" => Some("LINT_AM_009"),
        "CAPITALISATION.KEYWORDS" => Some("LINT_CP_001"),
        "CAPITALISATION.IDENTIFIERS" => Some("LINT_CP_002"),
        "CAPITALISATION.FUNCTIONS" => Some("LINT_CP_003"),
        "CAPITALISATION.LITERALS" => Some("LINT_CP_004"),
        "CAPITALISATION.TYPES" => Some("LINT_CP_005"),
        "CONVENTION.NOT_EQUAL" => Some("LINT_CV_001"),
        "CONVENTION.COALESCE" => Some("LINT_CV_002"),
        "CONVENTION.SELECT_TRAILING_COMMA" => Some("LINT_CV_003"),
        "CONVENTION.COUNT_ROWS" | "CONVENTION.STAR_COUNT" => Some("LINT_CV_004"),
        "CONVENTION.IS_NULL" => Some("LINT_CV_005"),
        "CONVENTION.TERMINATOR" => Some("LINT_CV_006"),
        "CONVENTION.STATEMENT_BRACKETS" => Some("LINT_CV_007"),
        "CONVENTION.LEFT_JOIN" => Some("LINT_CV_008"),
        "CONVENTION.BLOCKED_WORDS" => Some("LINT_CV_009"),
        "CONVENTION.QUOTED_LITERALS" => Some("LINT_CV_010"),
        "CONVENTION.CASTING_STYLE" => Some("LINT_CV_011"),
        "CONVENTION.JOIN_CONDITION" | "CONVENTION.JOIN" => Some("LINT_CV_012"),
        "JJ.PADDING" | "JINJA.PADDING" | "JJ.JJ01" => Some("LINT_JJ_001"),
        "LAYOUT.SPACING" => Some("LINT_LT_001"),
        "LAYOUT.INDENT" => Some("LINT_LT_002"),
        "LAYOUT.OPERATORS" => Some("LINT_LT_003"),
        "LAYOUT.COMMAS" => Some("LINT_LT_004"),
        "LAYOUT.LONG_LINES" => Some("LINT_LT_005"),
        "LAYOUT.FUNCTIONS" => Some("LINT_LT_006"),
        "LAYOUT.CTE_BRACKET" => Some("LINT_LT_007"),
        "LAYOUT.CTE_NEWLINE" => Some("LINT_LT_008"),
        "LAYOUT.SELECT_TARGETS" => Some("LINT_LT_009"),
        "LAYOUT.SELECT_MODIFIERS" => Some("LINT_LT_010"),
        "LAYOUT.SET_OPERATORS" => Some("LINT_LT_011"),
        "LAYOUT.END_OF_FILE" => Some("LINT_LT_012"),
        "LAYOUT.START_OF_FILE" => Some("LINT_LT_013"),
        "LAYOUT.KEYWORD_NEWLINE" => Some("LINT_LT_014"),
        "LAYOUT.NEWLINES" => Some("LINT_LT_015"),
        "REFERENCES.FROM" => Some("LINT_RF_001"),
        "REFERENCES.QUALIFICATION" => Some("LINT_RF_002"),
        "REFERENCES.CONSISTENT" => Some("LINT_RF_003"),
        "REFERENCES.KEYWORDS" => Some("LINT_RF_004"),
        "REFERENCES.SPECIAL_CHARS" => Some("LINT_RF_005"),
        "REFERENCES.QUOTING" => Some("LINT_RF_006"),
        "STRUCTURE.ELSE_NULL" => Some("LINT_ST_001"),
        "STRUCTURE.SIMPLE_CASE" => Some("LINT_ST_002"),
        "STRUCTURE.UNUSED_CTE" => Some("LINT_ST_003"),
        "STRUCTURE.NESTED_CASE" => Some("LINT_ST_004"),
        "STRUCTURE.SUBQUERY" => Some("LINT_ST_005"),
        "STRUCTURE.COLUMN_ORDER" => Some("LINT_ST_006"),
        "STRUCTURE.USING" => Some("LINT_ST_007"),
        "STRUCTURE.DISTINCT" => Some("LINT_ST_008"),
        "STRUCTURE.JOIN_CONDITION_ORDER" => Some("LINT_ST_009"),
        "STRUCTURE.CONSTANT_EXPRESSION" => Some("LINT_ST_010"),
        "STRUCTURE.UNUSED_JOIN" => Some("LINT_ST_011"),
        "STRUCTURE.CONSECUTIVE_SEMICOLONS" => Some("LINT_ST_012"),
        "TSQL.SP_PREFIX" => Some("LINT_TQ_001"),
        "TSQL.PROCEDURE_BEGIN_END" => Some("LINT_TQ_002"),
        "TSQL.EMPTY_BATCH" => Some("LINT_TQ_003"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_enables_all() {
        let config = LintConfig::default();
        assert!(config.enabled);
        assert!(config.is_rule_enabled("LINT_AM_008"));
    }

    #[test]
    fn disabled_rules_are_case_insensitive_and_trimmed() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![" lint_am_009 ".to_string(), " LINT_ST_006".to_string()],
            rule_configs: BTreeMap::new(),
        };
        assert!(!config.is_rule_enabled("LINT_AM_009"));
        assert!(!config.is_rule_enabled("lint_st_006"));
        assert!(config.is_rule_enabled("LINT_CV_007"));
    }

    #[test]
    fn disabled_rules_support_dotted_names() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![
                " ambiguous.join ".to_string(),
                "AMBIGUOUS.UNION".to_string(),
            ],
            rule_configs: BTreeMap::new(),
        };
        assert!(!config.is_rule_enabled("LINT_AM_005"));
        assert!(!config.is_rule_enabled("LINT_AM_002"));
        assert!(config.is_rule_enabled("LINT_AM_001"));
    }

    #[test]
    fn canonicalize_rule_supports_short_and_underscore_forms() {
        assert_eq!(
            canonicalize_rule_code("al01"),
            Some("LINT_AL_001".to_string())
        );
        assert_eq!(
            canonicalize_rule_code("AL_001"),
            Some("LINT_AL_001".to_string())
        );
        assert_eq!(
            canonicalize_rule_code("ambiguous.order_by"),
            Some("LINT_AM_003".to_string())
        );
        assert_eq!(canonicalize_rule_code("unknown"), None);
    }

    #[test]
    fn sqlfluff_name_lookup_works() {
        assert_eq!(
            sqlfluff_name_for_code("LINT_CV_008"),
            Some("convention.left_join")
        );
        assert_eq!(sqlfluff_name_for_code("cv08"), Some("convention.left_join"));
        assert_eq!(sqlfluff_name_for_code("unknown"), None);
    }

    #[test]
    fn master_toggle_off_disables_everything() {
        let config = LintConfig {
            enabled: false,
            disabled_rules: vec![],
            rule_configs: BTreeMap::new(),
        };
        assert!(!config.is_rule_enabled("LINT_AM_008"));
    }

    #[test]
    fn deserialization_defaults() {
        let json = "{}";
        let config: LintConfig = serde_json::from_str(json).expect("valid lint config json");
        assert!(config.enabled);
        assert!(config.disabled_rules.is_empty());
        assert!(config.rule_configs.is_empty());
    }

    #[test]
    fn rule_config_options_resolve_by_dotted_or_code() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: BTreeMap::from([
                (
                    "aliasing.table".to_string(),
                    serde_json::json!({"aliasing": "implicit"}),
                ),
                (
                    "LINT_AL_002".to_string(),
                    serde_json::json!({"aliasing": "explicit"}),
                ),
            ]),
        };

        assert_eq!(
            config.rule_option_str("LINT_AL_001", "aliasing"),
            Some("implicit")
        );
        assert_eq!(
            config.rule_option_str("aliasing.column", "aliasing"),
            Some("explicit")
        );
    }
}
