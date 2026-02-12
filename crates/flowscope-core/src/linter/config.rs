//! Configuration for the SQL linter.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Configuration for the SQL linter.
///
/// Controls which lint rules are enabled/disabled. By default, all rules are enabled.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LintConfig {
    /// Master toggle for linting (default: true).
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// List of rule codes to disable (e.g., ["LINT_AM_002"]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_rules: Vec<String>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            disabled_rules: Vec::new(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

impl LintConfig {
    /// Returns true if a specific rule is enabled.
    pub fn is_rule_enabled(&self, code: &str) -> bool {
        let requested = code.trim();
        self.enabled
            && !self
                .disabled_rules
                .iter()
                .any(|rule| rule.trim().eq_ignore_ascii_case(requested))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_enables_all() {
        let config = LintConfig::default();
        assert!(config.enabled);
        assert!(config.is_rule_enabled("LINT_AM_001"));
    }

    #[test]
    fn test_disabled_rule() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec!["LINT_AM_001".to_string()],
        };
        assert!(!config.is_rule_enabled("LINT_AM_001"));
        assert!(config.is_rule_enabled("LINT_ST_001"));
    }

    #[test]
    fn test_disabled_rule_matching_is_trimmed_and_case_insensitive() {
        let config = LintConfig {
            enabled: true,
            disabled_rules: vec![" lint_am_001 ".to_string(), " LINT_ST_001".to_string()],
        };
        assert!(!config.is_rule_enabled("LINT_AM_001"));
        assert!(!config.is_rule_enabled("lint_st_001"));
        assert!(config.is_rule_enabled("LINT_CV_001"));
    }

    #[test]
    fn test_master_toggle_off() {
        let config = LintConfig {
            enabled: false,
            disabled_rules: vec![],
        };
        assert!(!config.is_rule_enabled("LINT_AM_001"));
    }

    #[test]
    fn test_deserialization_defaults() {
        let json = "{}";
        let config: LintConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert!(config.disabled_rules.is_empty());
    }
}
