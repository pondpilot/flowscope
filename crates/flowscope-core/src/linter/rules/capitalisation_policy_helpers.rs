use std::collections::HashSet;

use crate::linter::config::LintConfig;
use regex::{Regex, RegexBuilder};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapitalisationPolicy {
    Consistent,
    Upper,
    Lower,
    Capitalise,
    Pascal,
    Camel,
    Snake,
}

impl CapitalisationPolicy {
    pub fn from_rule_config(config: &LintConfig, code: &str, key: &str) -> Self {
        config
            .rule_option_str(code, key)
            .map(Self::from_raw_value)
            .unwrap_or(Self::Consistent)
    }

    pub fn from_raw_value(raw: &str) -> Self {
        match raw.to_ascii_lowercase().as_str() {
            "upper" | "uppercase" => Self::Upper,
            "lower" | "lowercase" => Self::Lower,
            "capitalise" | "capitalize" => Self::Capitalise,
            "pascal" | "pascalcase" => Self::Pascal,
            "camel" | "camelcase" => Self::Camel,
            "snake" | "snake_case" => Self::Snake,
            _ => Self::Consistent,
        }
    }
}

pub fn tokens_violate_policy(tokens: &[String], policy: CapitalisationPolicy) -> bool {
    match policy {
        CapitalisationPolicy::Consistent => mixed_case_for_tokens(tokens),
        _ => tokens
            .iter()
            .any(|token| !token_matches_policy(token, policy)),
    }
}

pub fn ignored_words_from_config(config: &LintConfig, code: &str) -> HashSet<String> {
    if let Some(words) = config.rule_option_string_list(code, "ignore_words") {
        return words
            .into_iter()
            .map(|word| word.trim().to_ascii_uppercase())
            .filter(|word| !word.is_empty())
            .collect();
    }

    config
        .rule_option_str(code, "ignore_words")
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|word| !word.is_empty())
                .map(str::to_ascii_uppercase)
                .collect()
        })
        .unwrap_or_default()
}

pub fn ignored_words_regex_from_config(config: &LintConfig, code: &str) -> Option<Regex> {
    let raw = config.rule_option_str(code, "ignore_words_regex")?;
    let pattern = raw.trim();
    if pattern.is_empty() {
        return None;
    }

    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .ok()
}

pub fn token_is_ignored(
    token: &str,
    ignore_words: &HashSet<String>,
    ignore_words_regex: Option<&Regex>,
) -> bool {
    if ignore_words.contains(&token.to_ascii_uppercase()) {
        return true;
    }

    ignore_words_regex
        .map(|regex| regex.is_match(token))
        .unwrap_or(false)
}

fn token_matches_policy(token: &str, policy: CapitalisationPolicy) -> bool {
    match policy {
        CapitalisationPolicy::Consistent => true,
        CapitalisationPolicy::Upper => token == token.to_ascii_uppercase(),
        CapitalisationPolicy::Lower => token == token.to_ascii_lowercase(),
        CapitalisationPolicy::Capitalise => {
            let mut seen_alpha = false;
            for ch in token.chars() {
                if !ch.is_ascii_alphabetic() {
                    continue;
                }
                if !seen_alpha {
                    if !ch.is_ascii_uppercase() {
                        return false;
                    }
                    seen_alpha = true;
                } else if !ch.is_ascii_lowercase() {
                    return false;
                }
            }
            seen_alpha
        }
        CapitalisationPolicy::Pascal => {
            if token.contains('_') {
                return false;
            }
            let mut chars = token.chars();
            let Some(first) = chars.next() else {
                return false;
            };
            first.is_ascii_uppercase() && token.chars().any(|ch| ch.is_ascii_lowercase())
        }
        CapitalisationPolicy::Camel => {
            if token.contains('_') {
                return false;
            }
            let mut chars = token.chars();
            let Some(first) = chars.next() else {
                return false;
            };
            first.is_ascii_lowercase() && token.chars().any(|ch| ch.is_ascii_uppercase())
        }
        CapitalisationPolicy::Snake => token
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'),
    }
}

fn mixed_case_for_tokens(tokens: &[String]) -> bool {
    if tokens.len() < 2 {
        return false;
    }

    let mut saw_upper = false;
    let mut saw_lower = false;
    let mut saw_mixed = false;

    for token in tokens {
        let upper = token.to_ascii_uppercase();
        let lower = token.to_ascii_lowercase();
        if token == &upper {
            saw_upper = true;
        } else if token == &lower {
            saw_lower = true;
        } else {
            saw_mixed = true;
        }
    }

    saw_mixed || (saw_upper && saw_lower)
}
