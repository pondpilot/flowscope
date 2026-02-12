//! Additional SQLFluff-parity lint rules.
//!
//! These rules provide broad coverage for SQLFluff rule families that are not
//! deeply modeled in the core AST lints yet. They intentionally use conservative
//! heuristics (regex / token pattern matching on statement SQL) to avoid excessive
//! false positives.
//!
//! ## Differences from SQLFluff
//!
//! Each rule here maps to a SQLFluff rule code but has **narrower scope**:
//!
//! - **No configuration options** — SQLFluff rules often support `allow_*`,
//!   `prefer_*`, and case-style knobs. Parity rules use fixed defaults.
//! - **Regex-based detection** — Unlike SQLFluff's token-level traversal, these
//!   rules match patterns on the raw SQL text. They may miss complex cases and
//!   may produce false positives on SQL embedded inside string literals.
//! - **No auto-fix** — Parity rules are detection-only; `--fix` is not supported.
//!
//! See `docs/sqlfluff-gap-matrix.md` for the full mapping and per-rule notes.

use crate::linter::rule::{LintContext, LintRule};
use crate::types::{issue_codes, Issue};
use regex::Regex;
use sqlparser::ast::Statement;
use std::collections::HashSet;

macro_rules! define_predicate_rule {
    ($name:ident, $code:path, $rule_name:expr, $desc:expr, $severity:ident, $predicate:ident, $message:expr) => {
        pub struct $name;

        impl LintRule for $name {
            fn code(&self) -> &'static str {
                $code
            }

            fn name(&self) -> &'static str {
                $rule_name
            }

            fn description(&self) -> &'static str {
                $desc
            }

            fn check(&self, stmt: &Statement, ctx: &LintContext) -> Vec<Issue> {
                if $predicate(stmt, ctx) {
                    vec![Issue::$severity($code, $message).with_statement(ctx.statement_index)]
                } else {
                    Vec::new()
                }
            }
        }
    };
}

fn stmt_sql<'a>(ctx: &'a LintContext<'a>) -> &'a str {
    ctx.statement_sql()
}

fn is_keyword(token: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "SELECT",
        "FROM",
        "WHERE",
        "JOIN",
        "LEFT",
        "RIGHT",
        "FULL",
        "INNER",
        "OUTER",
        "CROSS",
        "ON",
        "USING",
        "GROUP",
        "ORDER",
        "ASC",
        "DESC",
        "BY",
        "HAVING",
        "LIMIT",
        "OFFSET",
        "WITH",
        "AS",
        "UNION",
        "INTERSECT",
        "EXCEPT",
        "CASE",
        "WHEN",
        "THEN",
        "ELSE",
        "END",
        "INSERT",
        "UPDATE",
        "DELETE",
        "CREATE",
        "VIEW",
        "TABLE",
        "TRUE",
        "FALSE",
        "NULL",
        "GO",
    ];
    KEYWORDS.contains(&token.to_ascii_uppercase().as_str())
}

fn has_re(sql: &str, pattern: &str) -> bool {
    Regex::new(pattern)
        .expect("valid parity regex")
        .is_match(sql)
}

fn capture_group(sql: &str, pattern: &str, group_idx: usize) -> Vec<String> {
    Regex::new(pattern)
        .expect("valid parity regex")
        .captures_iter(sql)
        .filter_map(|caps| caps.get(group_idx))
        .map(|m| m.as_str().to_string())
        .collect()
}

fn duplicate_case_insensitive(values: &[String]) -> bool {
    let mut seen = HashSet::new();
    for value in values {
        let key = value.to_ascii_uppercase();
        if !seen.insert(key) {
            return true;
        }
    }
    false
}

fn table_refs(sql: &str) -> Vec<String> {
    capture_group(sql, r"(?i)\b(?:from|join)\s+([A-Za-z_][A-Za-z0-9_\.]*)", 1)
        .into_iter()
        .map(|name| name.rsplit('.').next().map(str::to_string).unwrap_or(name))
        .collect()
}

fn table_aliases_raw(sql: &str) -> Vec<String> {
    capture_group(
        sql,
        r"(?i)\b(?:from|join)\s+[A-Za-z_][A-Za-z0-9_\.]*\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*)",
        1,
    )
}

fn table_aliases(sql: &str) -> Vec<String> {
    table_aliases_raw(sql)
        .into_iter()
        .filter(|alias| !is_keyword(alias))
        .collect()
}

fn join_aliases(sql: &str) -> Vec<String> {
    capture_group(
        sql,
        r"(?i)\bjoin\s+[A-Za-z_][A-Za-z0-9_\.]*\s+(?:as\s+)?([A-Za-z_][A-Za-z0-9_]*)",
        1,
    )
    .into_iter()
    .filter(|alias| !is_keyword(alias))
    .collect()
}

fn column_aliases(sql: &str) -> Vec<String> {
    capture_group(sql, r"(?i)\bas\s+([A-Za-z_][A-Za-z0-9_]*)", 1)
}

fn select_clause(sql: &str) -> Option<String> {
    Regex::new(r"(?is)\bselect\b(.*?)\bfrom\b")
        .expect("valid parity regex")
        .captures(sql)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

fn split_top_level_commas(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_single = false;
    let mut in_double = false;

    for ch in input.chars() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(ch);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(ch);
            }
            '(' if !in_single && !in_double => {
                depth += 1;
                current.push(ch);
            }
            ')' if !in_single && !in_double && depth > 0 => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !in_single && !in_double && depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    parts
}

fn select_items(sql: &str) -> Vec<String> {
    select_clause(sql)
        .map(|clause| split_top_level_commas(&clause))
        .unwrap_or_default()
}

fn item_has_as_alias(item: &str) -> bool {
    has_re(item, r"(?i)\bas\s+[A-Za-z_][A-Za-z0-9_]*\s*$")
}

fn item_is_simple_identifier(item: &str) -> bool {
    has_re(item, r"(?i)^\s*[A-Za-z_][A-Za-z0-9_\.]*\s*$")
}

fn has_qualified_reference(sql: &str) -> bool {
    has_re(
        sql,
        r"(?i)\b[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*\b",
    )
}

fn has_unqualified_reference_in_select(sql: &str) -> bool {
    select_items(sql).into_iter().any(|item| {
        let raw = item.trim();
        if raw == "*" || raw.ends_with(".*") {
            return false;
        }
        if raw.contains('.') || raw.contains('(') || raw.contains(' ') {
            return false;
        }
        item_is_simple_identifier(raw)
    })
}

fn case_style(token: &str) -> &'static str {
    let alpha: String = token.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    if alpha.is_empty() {
        return "mixed";
    }
    if alpha.chars().all(|c| c.is_ascii_uppercase()) {
        "upper"
    } else if alpha.chars().all(|c| c.is_ascii_lowercase()) {
        "lower"
    } else {
        "mixed"
    }
}

fn mixed_case_for_tokens(tokens: &[String]) -> bool {
    let mut styles = HashSet::new();
    for token in tokens {
        styles.insert(case_style(token));
    }
    styles.len() > 1
}

fn keyword_tokens(sql: &str) -> Vec<String> {
    let re = Regex::new(
        r"(?i)\b(select|from|where|join|left|right|inner|outer|full|cross|group|order|having|with|as|union|intersect|except|insert|update|delete|create|view|table|on|using)\b",
    )
    .expect("valid parity regex");
    re.captures_iter(sql)
        .filter_map(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .collect()
}

fn function_tokens(sql: &str) -> Vec<String> {
    let re = Regex::new(r"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\s*\(").expect("valid parity regex");
    re.captures_iter(sql)
        .filter_map(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .filter(|name| !is_keyword(name))
        .collect()
}

fn literal_tokens(sql: &str) -> Vec<String> {
    capture_group(sql, r"(?i)\b(null|true|false)\b", 1)
}

fn type_tokens(sql: &str) -> Vec<String> {
    capture_group(
        sql,
        r"(?i)\b(int|integer|bigint|smallint|tinyint|varchar|char|text|boolean|bool|date|timestamp|numeric|decimal|float|double)\b",
        1,
    )
}

fn identifier_tokens(sql: &str) -> Vec<String> {
    capture_group(sql, r#"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\b"#, 1)
        .into_iter()
        .filter(|token| !is_keyword(token))
        .collect()
}

fn contains_plain_join(sql: &str) -> bool {
    let re = Regex::new(r"(?i)\bjoin\b").expect("valid parity regex");
    for mat in re.find_iter(sql) {
        let prefix = &sql[..mat.start()];
        let prev_word = prefix
            .split_whitespace()
            .last()
            .unwrap_or("")
            .to_ascii_uppercase();
        let explicit = matches!(
            prev_word.as_str(),
            "LEFT" | "RIGHT" | "INNER" | "FULL" | "CROSS" | "OUTER" | "SEMI" | "ANTI" | "STRAIGHT"
        );
        if !explicit {
            return true;
        }
    }
    false
}

fn issue_if_regex(stmt: &Statement, ctx: &LintContext, pattern: &str) -> bool {
    let _ = stmt;
    has_re(stmt_sql(ctx), pattern)
}

fn rule_al_01(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    if has_re(sql, r"(?i)\bwith\b") {
        return false;
    }
    let refs = table_refs(sql);
    refs.len() > 1 && table_aliases(sql).len() < refs.len()
}

fn rule_al_02(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let items = select_items(stmt_sql(ctx));
    if items.is_empty() {
        return false;
    }
    let mut aliased = false;
    let mut unaliased_expr = false;
    for item in items {
        if item_has_as_alias(&item) {
            aliased = true;
        } else if !item_is_simple_identifier(&item)
            && item.trim() != "*"
            && !item.trim().ends_with(".*")
        {
            unaliased_expr = true;
        }
    }
    aliased && unaliased_expr
}

fn rule_al_04(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    duplicate_case_insensitive(&table_aliases(stmt_sql(ctx)))
}

fn rule_al_06(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    table_aliases(stmt_sql(ctx))
        .iter()
        .any(|alias| alias.len() > 30)
}

fn rule_al_07(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    table_refs(sql).len() == 1 && !table_aliases(sql).is_empty()
}

fn rule_al_08(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    duplicate_case_insensitive(&column_aliases(stmt_sql(ctx)))
}

fn rule_al_09(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let re = Regex::new(r"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\s+as\s+([A-Za-z_][A-Za-z0-9_]*)\b")
        .expect("valid parity regex");
    let has_self_alias = re.captures_iter(stmt_sql(ctx)).any(|caps| {
        let left = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let right = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
        left.eq_ignore_ascii_case(right)
    });
    has_self_alias
}

fn rule_am_03(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?i)\border\s+by\s+\d+\b")
}

fn rule_am_05(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(
        stmt,
        ctx,
        r"(?i)\bfrom\s+[A-Za-z_][A-Za-z0-9_\.]*\s*,\s*[A-Za-z_][A-Za-z0-9_\.]*\b",
    )
}

fn rule_am_06(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    has_qualified_reference(stmt_sql(ctx)) && has_unqualified_reference_in_select(stmt_sql(ctx))
}

fn rule_am_07(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    has_re(sql, r"(?i)\b(union|intersect|except)\b") && has_re(sql, r"(?i)\bselect\s+\*")
}

fn rule_am_08(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?i)\bjoin\b[^;]*\bon\s+(?:true|1\s*=\s*1)\b")
}

fn rule_cp_01(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    mixed_case_for_tokens(&keyword_tokens(stmt_sql(ctx)))
}

fn rule_cp_02(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let function_names: HashSet<String> = function_tokens(stmt_sql(ctx))
        .into_iter()
        .map(|name| name.to_ascii_uppercase())
        .collect();
    let identifiers: Vec<String> = identifier_tokens(stmt_sql(ctx))
        .into_iter()
        .filter(|ident| !function_names.contains(&ident.to_ascii_uppercase()))
        .collect();
    mixed_case_for_tokens(&identifiers)
}

fn rule_cp_03(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    mixed_case_for_tokens(&function_tokens(stmt_sql(ctx)))
}

fn rule_cp_04(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    mixed_case_for_tokens(&literal_tokens(stmt_sql(ctx)))
}

fn rule_cp_05(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    mixed_case_for_tokens(&type_tokens(stmt_sql(ctx)))
}

fn rule_cv_01(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"<>")
}

fn rule_cv_03(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?is)\bselect\b[^;]*,\s*\bfrom\b")
}

fn rule_cv_06(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    // Be conservative: only flag when semicolons exist in the file but this
    // statement SQL snippet itself doesn't end with one.
    ctx.sql.contains(';') && !stmt_sql(ctx).trim_end().ends_with(';')
}

fn rule_cv_07(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx).trim();
    sql.starts_with('(') && sql.ends_with(')')
}

fn rule_cv_09(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?i)\b(todo|fixme|foo|bar)\b")
}

fn rule_cv_10(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r#""[^"]+""#)
}

fn rule_cv_11(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"::")
}

fn rule_cv_12(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    contains_plain_join(sql)
        && !has_re(sql, r"(?i)\bjoin\b[^;]*\bon\b[^;]*=")
        && !has_re(sql, r"(?i)\bjoin\b[^;]*\busing\b")
}

fn rule_jj_01(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    has_re(sql, r"\{\{[^ \n]") || has_re(sql, r"[^ \n]\}\}") || has_re(sql, r"\{%[^ \n]")
}

fn rule_lt_01(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?i)\w(?:=|<>|!=|<|>|<=|>=|\+|-|\*|/)\w")
}

fn rule_lt_02(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    if !sql.contains('\n') {
        return false;
    }
    sql.lines().skip(1).any(|line| {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return false;
        }
        let indent = line.len() - trimmed.len();
        indent % 2 != 0
    })
}

fn rule_lt_03(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?m)(\+|-|\*|/|=|<>|!=|<|>)\s*$")
}

fn rule_lt_04(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    has_re(sql, r"\s+,") || has_re(sql, r",[^\s\n]")
}

fn rule_lt_05(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    stmt_sql(ctx).lines().any(|line| line.len() > 300)
}

fn rule_lt_06(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let re = Regex::new(r"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\s+\(").expect("valid parity regex");
    let has_violation = re.captures_iter(stmt_sql(ctx)).any(|caps| {
        let token = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        !is_keyword(token)
    });
    has_violation
}

fn rule_lt_07(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(
        stmt,
        ctx,
        r"(?is)\bwith\b\s+[A-Za-z_][A-Za-z0-9_]*\s+as\s+select\b",
    )
}

fn rule_lt_08(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    !sql.contains('\n')
        && has_re(sql, r"(?is)\bwith\b")
        && has_re(sql, r"(?i)\)\s+select\s+\*")
        && !has_re(sql, r"\)\s*\n\s*select\b")
}

fn rule_lt_09(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let items = select_items(stmt_sql(ctx));
    items.len() > 4 && !stmt_sql(ctx).contains('\n')
}

fn rule_lt_10(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?is)\bselect\s*\n+\s*(distinct|all)\b")
}

fn rule_lt_11(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    if !has_re(sql, r"(?i)\b(union|intersect|except)\b") || !sql.contains('\n') {
        return false;
    }

    sql.lines().any(|line| {
        let trimmed = line.trim().to_ascii_lowercase();
        match trimmed.as_str() {
            "union" | "union all" | "intersect" | "except" => false,
            _ => has_re(&trimmed, r"\b(union|intersect|except)\b"),
        }
    })
}

fn rule_lt_12(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    ctx.statement_range.end == ctx.sql.len() && ctx.sql.contains('\n') && !ctx.sql.ends_with('\n')
}

fn rule_lt_13(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    ctx.statement_index == 0 && has_re(ctx.sql, r"^\s*\n")
}

fn rule_lt_14(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    sql.contains('\n')
        && has_re(
            sql,
            r"(?im)^\s*select\b[^\n]*\b(from|where|group by|order by)\b",
        )
}

fn rule_lt_15(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"\n\s*\n\s*\n+")
}

fn rule_rf_01(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    let mut known: HashSet<String> = HashSet::new();
    for name in table_refs(sql) {
        known.insert(name.to_ascii_uppercase());
    }
    for alias in table_aliases(sql) {
        known.insert(alias.to_ascii_uppercase());
    }
    capture_group(
        sql,
        r"(?i)\b([A-Za-z_][A-Za-z0-9_]*)\.[A-Za-z_][A-Za-z0-9_]*\b",
        1,
    )
    .into_iter()
    .any(|prefix| !known.contains(&prefix.to_ascii_uppercase()))
}

fn rule_rf_02(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    table_refs(stmt_sql(ctx)).len() > 1 && has_unqualified_reference_in_select(stmt_sql(ctx))
}

fn rule_rf_03(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    has_qualified_reference(stmt_sql(ctx)) && has_unqualified_reference_in_select(stmt_sql(ctx))
}

fn rule_rf_04(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    capture_group(
        stmt_sql(ctx),
        r"(?i)\b(?:from|join)\s+[A-Za-z_][A-Za-z0-9_\.]*\s+as\s+([A-Za-z_][A-Za-z0-9_]*)",
        1,
    )
    .into_iter()
    .any(|alias| is_keyword(&alias))
}

fn rule_rf_05(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    capture_group(stmt_sql(ctx), r#""([^"]+)""#, 1)
        .into_iter()
        .any(|ident| !has_re(&ident, r"^[A-Za-z0-9_]+$"))
}

fn rule_rf_06(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    capture_group(stmt_sql(ctx), r#""([^"]+)""#, 1)
        .into_iter()
        .any(|ident| has_re(&ident, r"^[A-Za-z_][A-Za-z0-9_]*$"))
}

fn rule_st_02(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx).to_ascii_lowercase();
    if let Some(caps) = Regex::new(r"case\s+when\s+([a-z_][a-z0-9_\.]*)\s*=")
        .expect("valid parity regex")
        .captures(&sql)
    {
        if let Some(lhs) = caps.get(1) {
            let pattern = format!(r"when\s+{}\s*=", regex::escape(lhs.as_str()));
            let repeated_when_count = Regex::new(&pattern)
                .expect("valid parity regex")
                .find_iter(&sql)
                .count();
            return repeated_when_count >= 2;
        }
    }
    false
}

fn rule_st_05(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?is)\b(from|where|in)\s*\(\s*select\b")
}

fn rule_st_06(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let items = select_items(stmt_sql(ctx));
    let mut seen_expression = false;
    for item in items {
        if item_is_simple_identifier(&item) {
            if seen_expression {
                return true;
            }
        } else {
            seen_expression = true;
        }
    }
    false
}

fn rule_st_08(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?is)\bselect\s+distinct\s*\(")
}

fn rule_st_09(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    let aliases = table_aliases(sql);
    if aliases.len() < 2 {
        return false;
    }
    let left = aliases[0].to_ascii_lowercase();
    let right = aliases[1].to_ascii_lowercase();
    let rev_pattern = format!(
        r"(?is)\bon\s+{}\.[a-z0-9_]+\s*=\s*{}\.[a-z0-9_]+",
        regex::escape(&right),
        regex::escape(&left)
    );
    has_re(&sql.to_ascii_lowercase(), &rev_pattern)
}

fn rule_st_10(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(
        stmt,
        ctx,
        r"(?i)\b(1\s*=\s*1|1\s*=\s*0|true\s+(and|or)|false\s+(and|or))\b",
    )
}

fn rule_st_11(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    for alias in join_aliases(sql) {
        let pat = format!(r"(?i)\b{}\.", regex::escape(&alias));
        let count = Regex::new(&pat)
            .expect("valid parity regex")
            .find_iter(sql)
            .count();
        if count == 0 {
            return true;
        }
    }
    false
}

fn rule_st_12(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    ctx.statement_index == 0 && has_re(ctx.sql, r";\s*;")
}

fn rule_tq_01(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(
        stmt,
        ctx,
        r"(?i)\bcreate\s+(?:proc|procedure)\s+sp_[A-Za-z0-9_]*",
    )
}

fn rule_tq_02(stmt: &Statement, ctx: &LintContext) -> bool {
    let _ = stmt;
    let sql = stmt_sql(ctx);
    has_re(sql, r"(?i)\bcreate\s+(?:proc|procedure)\b")
        && !(has_re(sql, r"(?i)\bbegin\b") && has_re(sql, r"(?i)\bend\b"))
}

fn rule_tq_03(stmt: &Statement, ctx: &LintContext) -> bool {
    issue_if_regex(stmt, ctx, r"(?im)^\s*GO\s*$\s*(?:\r?\n\s*GO\s*$)+")
}

define_predicate_rule!(
    AliasingTableStyle,
    issue_codes::LINT_AL_003,
    "Table alias style",
    "Prefer explicit table aliases in multi-table queries.",
    warning,
    rule_al_01,
    "Use explicit aliases consistently for tables in multi-table queries."
);
define_predicate_rule!(
    AliasingColumnStyle,
    issue_codes::LINT_AL_004,
    "Column alias style",
    "Avoid mixing explicit and implicit aliasing for expressions.",
    info,
    rule_al_02,
    "Avoid mixing explicit and implicit expression aliases."
);
define_predicate_rule!(
    AliasingUniqueTable,
    issue_codes::LINT_AL_005,
    "Unique table alias",
    "Table aliases should be unique within a statement.",
    warning,
    rule_al_04,
    "Table aliases should be unique."
);
define_predicate_rule!(
    AliasingLength,
    issue_codes::LINT_AL_006,
    "Alias length",
    "Alias names should be readable and not excessively long.",
    info,
    rule_al_06,
    "Alias length should not exceed 30 characters."
);
define_predicate_rule!(
    AliasingForbidSingleTable,
    issue_codes::LINT_AL_007,
    "Forbid unnecessary alias",
    "Single-table queries should avoid unnecessary aliases.",
    info,
    rule_al_07,
    "Avoid unnecessary aliases in single-table queries."
);
define_predicate_rule!(
    AliasingUniqueColumn,
    issue_codes::LINT_AL_008,
    "Unique column alias",
    "Column aliases should be unique in projection lists.",
    warning,
    rule_al_08,
    "Column aliases should be unique within SELECT projection."
);
define_predicate_rule!(
    AliasingSelfAliasColumn,
    issue_codes::LINT_AL_009,
    "Self alias column",
    "Avoid aliasing a column/expression to the same name.",
    info,
    rule_al_09,
    "Avoid self-aliasing columns or expressions."
);

define_predicate_rule!(
    AmbiguousOrderByOrdinal,
    issue_codes::LINT_AM_005,
    "Ambiguous ORDER BY",
    "Avoid positional ORDER BY references.",
    warning,
    rule_am_03,
    "Avoid positional ORDER BY references (e.g., ORDER BY 1)."
);
define_predicate_rule!(
    AmbiguousJoinStyle,
    issue_codes::LINT_AM_006,
    "Ambiguous join style",
    "Avoid implicit/comma joins.",
    warning,
    rule_am_05,
    "Avoid comma joins; use explicit JOIN ... ON syntax."
);
define_predicate_rule!(
    AmbiguousColumnRefs,
    issue_codes::LINT_AM_007,
    "Ambiguous column references",
    "Avoid mixing qualified and unqualified references.",
    info,
    rule_am_06,
    "Avoid mixing qualified and unqualified column references."
);
define_predicate_rule!(
    AmbiguousSetColumns,
    issue_codes::LINT_AM_008,
    "Ambiguous set columns",
    "Avoid wildcard projections in set operations.",
    warning,
    rule_am_07,
    "Avoid wildcard projections in UNION/INTERSECT/EXCEPT branches."
);
define_predicate_rule!(
    AmbiguousJoinCondition,
    issue_codes::LINT_AM_009,
    "Ambiguous join condition",
    "Join conditions should be explicit and meaningful.",
    warning,
    rule_am_08,
    "Join condition appears ambiguous (e.g., ON TRUE / ON 1=1)."
);

define_predicate_rule!(
    CapitalisationKeywords,
    issue_codes::LINT_CP_001,
    "Keyword capitalisation",
    "SQL keywords should use a consistent case style.",
    info,
    rule_cp_01,
    "SQL keywords use inconsistent capitalisation."
);
define_predicate_rule!(
    CapitalisationIdentifiers,
    issue_codes::LINT_CP_002,
    "Identifier capitalisation",
    "Identifiers should use a consistent case style.",
    info,
    rule_cp_02,
    "Identifiers use inconsistent capitalisation."
);
define_predicate_rule!(
    CapitalisationFunctions,
    issue_codes::LINT_CP_003,
    "Function capitalisation",
    "Functions should use a consistent case style.",
    info,
    rule_cp_03,
    "Function names use inconsistent capitalisation."
);
define_predicate_rule!(
    CapitalisationLiterals,
    issue_codes::LINT_CP_004,
    "Literal capitalisation",
    "NULL/TRUE/FALSE should use a consistent case style.",
    info,
    rule_cp_04,
    "Literal keywords (NULL/TRUE/FALSE) use inconsistent capitalisation."
);
define_predicate_rule!(
    CapitalisationTypes,
    issue_codes::LINT_CP_005,
    "Type capitalisation",
    "Type names should use a consistent case style.",
    info,
    rule_cp_05,
    "Type names use inconsistent capitalisation."
);

define_predicate_rule!(
    ConventionNotEqual,
    issue_codes::LINT_CV_005,
    "Not-equal style",
    "Use a consistent not-equal operator style.",
    info,
    rule_cv_01,
    "Use consistent not-equal style (prefer !=)."
);
define_predicate_rule!(
    ConventionSelectTrailingComma,
    issue_codes::LINT_CV_006,
    "Select trailing comma",
    "Avoid trailing comma before FROM.",
    warning,
    rule_cv_03,
    "Avoid trailing comma before FROM in SELECT clause."
);
define_predicate_rule!(
    ConventionTerminator,
    issue_codes::LINT_CV_007,
    "Statement terminator",
    "Statements should use consistent semicolon termination.",
    info,
    rule_cv_06,
    "Statement terminator style is inconsistent."
);
define_predicate_rule!(
    ConventionStatementBrackets,
    issue_codes::LINT_CV_008,
    "Statement brackets",
    "Avoid unnecessary wrapping brackets around full statements.",
    info,
    rule_cv_07,
    "Avoid wrapping the full statement in unnecessary brackets."
);
define_predicate_rule!(
    ConventionBlockedWords,
    issue_codes::LINT_CV_009,
    "Blocked words",
    "Avoid blocked placeholder words.",
    warning,
    rule_cv_09,
    "Blocked placeholder words detected (e.g., TODO/FIXME/foo/bar)."
);
define_predicate_rule!(
    ConventionQuotedLiterals,
    issue_codes::LINT_CV_010,
    "Quoted literals style",
    "Quoted literal style is inconsistent with SQL convention.",
    info,
    rule_cv_10,
    "Quoted literal style appears inconsistent."
);
define_predicate_rule!(
    ConventionCastingStyle,
    issue_codes::LINT_CV_011,
    "Casting style",
    "Use consistent casting style.",
    info,
    rule_cv_11,
    "Use consistent casting style (avoid mixing :: and CAST)."
);
define_predicate_rule!(
    ConventionJoinCondition,
    issue_codes::LINT_CV_012,
    "Join condition convention",
    "JOIN clauses should use explicit, meaningful join predicates.",
    warning,
    rule_cv_12,
    "JOIN clause appears to lack a meaningful join condition."
);

define_predicate_rule!(
    JinjaPadding,
    issue_codes::LINT_JJ_001,
    "Jinja padding",
    "Jinja tags should use consistent padding.",
    info,
    rule_jj_01,
    "Jinja tag spacing appears inconsistent."
);

define_predicate_rule!(
    LayoutSpacing,
    issue_codes::LINT_LT_001,
    "Layout spacing",
    "Operator spacing should be consistent.",
    info,
    rule_lt_01,
    "Operator spacing appears inconsistent."
);
define_predicate_rule!(
    LayoutIndent,
    issue_codes::LINT_LT_002,
    "Layout indent",
    "Indentation should use consistent step sizes.",
    info,
    rule_lt_02,
    "Indentation appears inconsistent."
);
define_predicate_rule!(
    LayoutOperators,
    issue_codes::LINT_LT_003,
    "Layout operators",
    "Operator line placement should be consistent.",
    info,
    rule_lt_03,
    "Operator line placement appears inconsistent."
);
define_predicate_rule!(
    LayoutCommas,
    issue_codes::LINT_LT_004,
    "Layout commas",
    "Comma spacing should be consistent.",
    info,
    rule_lt_04,
    "Comma spacing appears inconsistent."
);
define_predicate_rule!(
    LayoutLongLines,
    issue_codes::LINT_LT_005,
    "Layout long lines",
    "Avoid excessively long SQL lines.",
    info,
    rule_lt_05,
    "SQL contains excessively long lines."
);
define_predicate_rule!(
    LayoutFunctions,
    issue_codes::LINT_LT_006,
    "Layout functions",
    "Function call spacing should be consistent.",
    info,
    rule_lt_06,
    "Function call spacing appears inconsistent."
);
define_predicate_rule!(
    LayoutCteBracket,
    issue_codes::LINT_LT_007,
    "Layout CTE bracket",
    "CTE bodies should be wrapped in brackets.",
    warning,
    rule_lt_07,
    "CTE AS clause appears to be missing surrounding brackets."
);
define_predicate_rule!(
    LayoutCteNewline,
    issue_codes::LINT_LT_008,
    "Layout CTE newline",
    "CTE to main query transition should use newline separation.",
    info,
    rule_lt_08,
    "CTE and main query should be separated by a newline."
);
define_predicate_rule!(
    LayoutSelectTargets,
    issue_codes::LINT_LT_009,
    "Layout select targets",
    "Large SELECT target lists should be line-broken.",
    info,
    rule_lt_09,
    "Large SELECT target list should use line breaks."
);
define_predicate_rule!(
    LayoutSelectModifiers,
    issue_codes::LINT_LT_010,
    "Layout select modifiers",
    "SELECT modifiers should be placed consistently.",
    info,
    rule_lt_10,
    "SELECT modifiers (DISTINCT/ALL) should be consistently formatted."
);
define_predicate_rule!(
    LayoutSetOperators,
    issue_codes::LINT_LT_011,
    "Layout set operators",
    "Set operators should be consistently line-broken.",
    info,
    rule_lt_11,
    "Set operators should be on their own line in multiline queries."
);
define_predicate_rule!(
    LayoutEndOfFile,
    issue_codes::LINT_LT_012,
    "Layout end of file",
    "File should end with newline.",
    info,
    rule_lt_12,
    "SQL document should end with a trailing newline."
);
define_predicate_rule!(
    LayoutStartOfFile,
    issue_codes::LINT_LT_013,
    "Layout start of file",
    "Avoid leading blank lines at file start.",
    info,
    rule_lt_13,
    "Avoid leading blank lines at the start of SQL file."
);
define_predicate_rule!(
    LayoutKeywordNewline,
    issue_codes::LINT_LT_014,
    "Layout keyword newline",
    "Major clauses should be consistently line-broken.",
    info,
    rule_lt_14,
    "Major clauses should be consistently line-broken."
);
define_predicate_rule!(
    LayoutNewlines,
    issue_codes::LINT_LT_015,
    "Layout newlines",
    "Avoid excessive blank lines.",
    info,
    rule_lt_15,
    "SQL contains excessive blank lines."
);

define_predicate_rule!(
    ReferencesFrom,
    issue_codes::LINT_RF_001,
    "References from",
    "Qualified references should resolve to known FROM/JOIN sources.",
    warning,
    rule_rf_01,
    "Reference prefix appears unresolved from FROM/JOIN sources."
);
define_predicate_rule!(
    ReferencesQualification,
    issue_codes::LINT_RF_002,
    "References qualification",
    "Use qualification consistently in multi-table queries.",
    warning,
    rule_rf_02,
    "Use qualified references in multi-table queries."
);
define_predicate_rule!(
    ReferencesConsistent,
    issue_codes::LINT_RF_003,
    "References consistent",
    "Avoid mixing qualified and unqualified references.",
    info,
    rule_rf_03,
    "Avoid mixing qualified and unqualified references."
);
define_predicate_rule!(
    ReferencesKeywords,
    issue_codes::LINT_RF_004,
    "References keywords",
    "Avoid SQL keywords as identifiers.",
    warning,
    rule_rf_04,
    "Avoid SQL keywords as identifiers."
);
define_predicate_rule!(
    ReferencesSpecialChars,
    issue_codes::LINT_RF_005,
    "References special chars",
    "Identifiers should avoid special characters.",
    warning,
    rule_rf_05,
    "Identifier contains special characters."
);
define_predicate_rule!(
    ReferencesQuoting,
    issue_codes::LINT_RF_006,
    "References quoting",
    "Avoid unnecessary identifier quoting.",
    info,
    rule_rf_06,
    "Identifier quoting appears unnecessary."
);

define_predicate_rule!(
    StructureSimpleCase,
    issue_codes::LINT_ST_005,
    "Structure simple case",
    "Prefer simple CASE form where applicable.",
    info,
    rule_st_02,
    "CASE expression may be simplified to simple CASE form."
);
define_predicate_rule!(
    StructureSubquery,
    issue_codes::LINT_ST_006,
    "Structure subquery",
    "Avoid unnecessary nested subqueries.",
    info,
    rule_st_05,
    "Subquery detected; consider refactoring with CTEs."
);
define_predicate_rule!(
    StructureColumnOrder,
    issue_codes::LINT_ST_007,
    "Structure column order",
    "Place simple columns before complex expressions.",
    info,
    rule_st_06,
    "Prefer simple columns before complex expressions in SELECT."
);
define_predicate_rule!(
    StructureDistinct,
    issue_codes::LINT_ST_008,
    "Structure distinct",
    "DISTINCT usage appears structurally suboptimal.",
    info,
    rule_st_08,
    "DISTINCT usage appears structurally suboptimal."
);
define_predicate_rule!(
    StructureJoinConditionOrder,
    issue_codes::LINT_ST_009,
    "Structure join condition order",
    "Join condition ordering appears reversed.",
    info,
    rule_st_09,
    "Join condition ordering appears reversed."
);
define_predicate_rule!(
    StructureConstantExpression,
    issue_codes::LINT_ST_010,
    "Structure constant expression",
    "Avoid constant boolean expressions in predicates.",
    warning,
    rule_st_10,
    "Constant boolean expression detected in predicate."
);
define_predicate_rule!(
    StructureUnusedJoin,
    issue_codes::LINT_ST_011,
    "Structure unused join",
    "Joined sources should be referenced meaningfully.",
    warning,
    rule_st_11,
    "Joined source appears unused."
);
define_predicate_rule!(
    StructureConsecutiveSemicolons,
    issue_codes::LINT_ST_012,
    "Structure consecutive semicolons",
    "Avoid consecutive semicolons.",
    warning,
    rule_st_12,
    "Consecutive semicolons detected."
);

define_predicate_rule!(
    TsqlSpPrefix,
    issue_codes::LINT_TQ_001,
    "TSQL sp_ prefix",
    "Avoid sp_ procedure prefix in TSQL.",
    warning,
    rule_tq_01,
    "Avoid stored procedure names with sp_ prefix."
);
define_predicate_rule!(
    TsqlProcedureBeginEnd,
    issue_codes::LINT_TQ_002,
    "TSQL procedure BEGIN/END",
    "TSQL procedures should include BEGIN/END block.",
    warning,
    rule_tq_02,
    "CREATE PROCEDURE should include BEGIN/END block."
);
define_predicate_rule!(
    TsqlEmptyBatch,
    issue_codes::LINT_TQ_003,
    "TSQL empty batch",
    "Avoid empty TSQL batches between GO separators.",
    warning,
    rule_tq_03,
    "Empty TSQL batch detected between GO separators."
);

/// Returns all parity rule implementations defined in this module.
pub fn parity_rules() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(AliasingTableStyle),
        Box::new(AliasingColumnStyle),
        Box::new(AliasingUniqueTable),
        Box::new(AliasingLength),
        Box::new(AliasingForbidSingleTable),
        Box::new(AliasingUniqueColumn),
        Box::new(AliasingSelfAliasColumn),
        Box::new(AmbiguousOrderByOrdinal),
        Box::new(AmbiguousJoinStyle),
        Box::new(AmbiguousColumnRefs),
        Box::new(AmbiguousSetColumns),
        Box::new(AmbiguousJoinCondition),
        Box::new(CapitalisationKeywords),
        Box::new(CapitalisationIdentifiers),
        Box::new(CapitalisationFunctions),
        Box::new(CapitalisationLiterals),
        Box::new(CapitalisationTypes),
        Box::new(ConventionNotEqual),
        Box::new(ConventionSelectTrailingComma),
        Box::new(ConventionTerminator),
        Box::new(ConventionStatementBrackets),
        Box::new(ConventionBlockedWords),
        Box::new(ConventionQuotedLiterals),
        Box::new(ConventionCastingStyle),
        Box::new(ConventionJoinCondition),
        Box::new(JinjaPadding),
        Box::new(LayoutSpacing),
        Box::new(LayoutIndent),
        Box::new(LayoutOperators),
        Box::new(LayoutCommas),
        Box::new(LayoutLongLines),
        Box::new(LayoutFunctions),
        Box::new(LayoutCteBracket),
        Box::new(LayoutCteNewline),
        Box::new(LayoutSelectTargets),
        Box::new(LayoutSelectModifiers),
        Box::new(LayoutSetOperators),
        Box::new(LayoutEndOfFile),
        Box::new(LayoutStartOfFile),
        Box::new(LayoutKeywordNewline),
        Box::new(LayoutNewlines),
        Box::new(ReferencesFrom),
        Box::new(ReferencesQualification),
        Box::new(ReferencesConsistent),
        Box::new(ReferencesKeywords),
        Box::new(ReferencesSpecialChars),
        Box::new(ReferencesQuoting),
        Box::new(StructureSimpleCase),
        Box::new(StructureSubquery),
        Box::new(StructureColumnOrder),
        Box::new(StructureDistinct),
        Box::new(StructureJoinConditionOrder),
        Box::new(StructureConstantExpression),
        Box::new(StructureUnusedJoin),
        Box::new(StructureConsecutiveSemicolons),
        Box::new(TsqlSpPrefix),
        Box::new(TsqlProcedureBeginEnd),
        Box::new(TsqlEmptyBatch),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;
    use crate::types::issue_codes;

    fn run_rule(rule: &dyn LintRule, sql: &str) -> Vec<Issue> {
        let stmts = parse_sql(sql).expect("test SQL should parse");
        let mut issues = Vec::new();
        for (idx, stmt) in stmts.iter().enumerate() {
            let ctx = LintContext {
                sql,
                statement_range: 0..sql.len(),
                statement_index: idx,
            };
            issues.extend(rule.check(stmt, &ctx));
        }
        issues
    }

    #[test]
    fn aliasing_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(
            &AliasingTableStyle,
            "SELECT * FROM users JOIN orders ON users.id = orders.user_id",
        );
        assert_rule_not_triggers(
            &AliasingTableStyle,
            "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
        );

        assert_rule_triggers(&AliasingColumnStyle, "SELECT a + 1 AS x, b + 2 FROM t");
        assert_rule_not_triggers(&AliasingColumnStyle, "SELECT a + 1 AS x, b + 2 AS y FROM t");

        assert_rule_triggers(
            &AliasingUniqueTable,
            "SELECT * FROM users u JOIN orders u ON u.id = u.user_id",
        );
        assert_rule_not_triggers(
            &AliasingUniqueTable,
            "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
        );

        assert_rule_triggers(
            &AliasingLength,
            "SELECT * FROM users this_alias_name_is_longer_than_thirty",
        );
        assert_rule_not_triggers(&AliasingLength, "SELECT * FROM users u");

        assert_rule_triggers(&AliasingForbidSingleTable, "SELECT * FROM users u");
        assert_rule_not_triggers(
            &AliasingForbidSingleTable,
            "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
        );

        assert_rule_triggers(&AliasingUniqueColumn, "SELECT a AS x, b AS x FROM t");
        assert_rule_not_triggers(&AliasingUniqueColumn, "SELECT a AS x, b AS y FROM t");

        assert_rule_triggers(&AliasingSelfAliasColumn, "SELECT a AS a FROM t");
        assert_rule_not_triggers(&AliasingSelfAliasColumn, "SELECT a AS b FROM t");
    }

    #[test]
    fn ambiguous_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(&AmbiguousOrderByOrdinal, "SELECT name FROM t ORDER BY 1");
        assert_rule_not_triggers(&AmbiguousOrderByOrdinal, "SELECT name FROM t ORDER BY name");

        assert_rule_triggers(&AmbiguousJoinStyle, "SELECT * FROM a, b");
        assert_rule_not_triggers(&AmbiguousJoinStyle, "SELECT * FROM a JOIN b ON a.id = b.id");

        assert_rule_triggers(&AmbiguousColumnRefs, "SELECT a.id, name FROM a");
        assert_rule_not_triggers(&AmbiguousColumnRefs, "SELECT a.id, a.name FROM a");

        assert_rule_triggers(
            &AmbiguousSetColumns,
            "SELECT * FROM a UNION SELECT * FROM b",
        );
        assert_rule_not_triggers(
            &AmbiguousSetColumns,
            "SELECT a FROM a UNION SELECT b FROM b",
        );

        assert_rule_triggers(&AmbiguousJoinCondition, "SELECT * FROM a JOIN b ON TRUE");
        assert_rule_not_triggers(
            &AmbiguousJoinCondition,
            "SELECT * FROM a JOIN b ON a.id = b.id",
        );
    }

    #[test]
    fn capitalisation_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(&CapitalisationKeywords, "SELECT a from t");
        assert_rule_not_triggers(&CapitalisationKeywords, "SELECT a FROM t");

        assert_rule_triggers(&CapitalisationIdentifiers, "SELECT Col, col FROM t");
        assert_rule_not_triggers(&CapitalisationIdentifiers, "SELECT col_one, col_two FROM t");

        assert_rule_triggers(&CapitalisationFunctions, "SELECT COUNT(*), lower(x) FROM t");
        assert_rule_not_triggers(&CapitalisationFunctions, "SELECT lower(x), upper(y) FROM t");

        assert_rule_triggers(&CapitalisationLiterals, "SELECT NULL, true FROM t");
        assert_rule_not_triggers(&CapitalisationLiterals, "SELECT NULL, TRUE FROM t");

        assert_rule_triggers(
            &CapitalisationTypes,
            "CREATE TABLE t (a INT, b varchar(10))",
        );
        assert_rule_not_triggers(
            &CapitalisationTypes,
            "CREATE TABLE t (a int, b varchar(10))",
        );
    }

    #[test]
    fn convention_and_jinja_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(&ConventionNotEqual, "SELECT * FROM t WHERE a <> b");
        assert_rule_not_triggers(&ConventionNotEqual, "SELECT * FROM t WHERE a != b");

        assert_rule_triggers(&ConventionSelectTrailingComma, "SELECT a, FROM t");
        assert_rule_not_triggers(&ConventionSelectTrailingComma, "SELECT a, b FROM t");

        assert_rule_triggers(&ConventionTerminator, "SELECT 1; SELECT 2");
        assert_rule_not_triggers(&ConventionTerminator, "SELECT 1; SELECT 2;");

        assert_rule_triggers(&ConventionStatementBrackets, "(SELECT 1)");
        assert_rule_not_triggers(&ConventionStatementBrackets, "SELECT 1");

        assert_rule_triggers(&ConventionBlockedWords, "SELECT foo FROM t");
        assert_rule_not_triggers(&ConventionBlockedWords, "SELECT customer_id FROM t");

        assert_rule_triggers(&ConventionQuotedLiterals, "SELECT \"abc\" FROM t");
        assert_rule_not_triggers(&ConventionQuotedLiterals, "SELECT 'abc' FROM t");

        assert_rule_triggers(&ConventionCastingStyle, "SELECT amount::INT FROM t");
        assert_rule_not_triggers(&ConventionCastingStyle, "SELECT CAST(amount AS INT) FROM t");

        assert_rule_triggers(
            &ConventionJoinCondition,
            "SELECT * FROM a JOIN b ON b.id > 0",
        );
        assert_rule_not_triggers(
            &ConventionJoinCondition,
            "SELECT * FROM a JOIN b ON a.id = b.id",
        );

        assert_rule_triggers(&JinjaPadding, "SELECT '{{foo}}' AS templated");
        assert_rule_not_triggers(&JinjaPadding, "SELECT '{{ foo }}' AS templated");
    }

    #[test]
    fn layout_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(&LayoutSpacing, "SELECT * FROM t WHERE a=1");
        assert_rule_not_triggers(&LayoutSpacing, "SELECT * FROM t WHERE a = 1");

        assert_rule_triggers(&LayoutIndent, "SELECT a\n   , b\nFROM t");
        assert_rule_not_triggers(&LayoutIndent, "SELECT a\n    , b\nFROM t");

        assert_rule_triggers(&LayoutOperators, "SELECT a +\n b FROM t");
        assert_rule_not_triggers(&LayoutOperators, "SELECT a\n + b FROM t");

        assert_rule_triggers(&LayoutCommas, "SELECT a,b FROM t");
        assert_rule_not_triggers(&LayoutCommas, "SELECT a, b FROM t");

        let long_line = format!("SELECT {} FROM t", "x".repeat(320));
        assert_rule_triggers(&LayoutLongLines, &long_line);
        assert_rule_not_triggers(&LayoutLongLines, "SELECT x FROM t");

        assert_rule_triggers(&LayoutFunctions, "SELECT COUNT (1) FROM t");
        assert_rule_not_triggers(&LayoutFunctions, "SELECT COUNT(1) FROM t");

        let lt07 = run_rule(
            &LayoutCteBracket,
            "SELECT 'WITH cte AS SELECT 1' AS sql_snippet",
        );
        assert!(
            lt07.iter()
                .any(|issue| issue.code == issue_codes::LINT_LT_007),
            "expected {} to trigger; got: {lt07:?}",
            issue_codes::LINT_LT_007,
        );
        assert_rule_not_triggers(
            &LayoutCteBracket,
            "WITH cte AS (SELECT 1) SELECT * FROM cte",
        );

        assert_rule_triggers(
            &LayoutCteNewline,
            "WITH cte AS (SELECT 1) SELECT * FROM cte",
        );
        assert_rule_not_triggers(
            &LayoutCteNewline,
            "WITH cte AS (SELECT 1)\nSELECT * FROM cte",
        );

        assert_rule_triggers(&LayoutSelectTargets, "SELECT a,b,c,d,e FROM t");
        assert_rule_not_triggers(&LayoutSelectTargets, "SELECT a, b, c, d FROM t");

        assert_rule_triggers(&LayoutSelectModifiers, "SELECT\nDISTINCT a\nFROM t");
        assert_rule_not_triggers(&LayoutSelectModifiers, "SELECT DISTINCT a FROM t");

        assert_rule_triggers(
            &LayoutSetOperators,
            "SELECT 1 UNION SELECT 2\nUNION SELECT 3",
        );
        assert_rule_not_triggers(
            &LayoutSetOperators,
            "SELECT 1\nUNION\nSELECT 2\nUNION\nSELECT 3",
        );

        assert_rule_triggers(&LayoutEndOfFile, "SELECT 1\nFROM t");
        assert_rule_not_triggers(&LayoutEndOfFile, "SELECT 1\nFROM t\n");

        assert_rule_triggers(&LayoutStartOfFile, "\n\nSELECT 1");
        assert_rule_not_triggers(&LayoutStartOfFile, "SELECT 1");

        assert_rule_triggers(&LayoutKeywordNewline, "SELECT a FROM t\nWHERE a = 1");
        assert_rule_not_triggers(&LayoutKeywordNewline, "SELECT a\nFROM t\nWHERE a = 1");

        assert_rule_triggers(&LayoutNewlines, "SELECT 1\n\n\nFROM t");
        assert_rule_not_triggers(&LayoutNewlines, "SELECT 1\n\nFROM t");
    }

    #[test]
    fn references_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(&ReferencesFrom, "SELECT x.id FROM users");
        assert_rule_not_triggers(&ReferencesFrom, "SELECT users.id FROM users");

        assert_rule_triggers(
            &ReferencesQualification,
            "SELECT id FROM users u JOIN orders o ON u.id = o.user_id",
        );
        assert_rule_not_triggers(
            &ReferencesQualification,
            "SELECT u.id FROM users u JOIN orders o ON u.id = o.user_id",
        );

        assert_rule_triggers(&ReferencesConsistent, "SELECT u.id, id FROM users u");
        assert_rule_not_triggers(&ReferencesConsistent, "SELECT u.id, u.name FROM users u");

        let rf04 = run_rule(
            &ReferencesKeywords,
            "SELECT 'FROM tbl AS SELECT' AS sql_snippet",
        );
        assert!(
            rf04.iter()
                .any(|issue| issue.code == issue_codes::LINT_RF_004),
            "expected {} to trigger; got: {rf04:?}",
            issue_codes::LINT_RF_004,
        );
        assert_rule_not_triggers(
            &ReferencesKeywords,
            "SELECT 'FROM tbl AS alias_name' AS sql_snippet",
        );

        assert_rule_triggers(&ReferencesSpecialChars, "SELECT \"bad-name\" FROM t");
        assert_rule_not_triggers(&ReferencesSpecialChars, "SELECT \"good_name\" FROM t");

        assert_rule_triggers(&ReferencesQuoting, "SELECT \"good_name\" FROM t");
        assert_rule_not_triggers(&ReferencesQuoting, "SELECT \"bad-name\" FROM t");
    }

    #[test]
    fn structure_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(
            &StructureSimpleCase,
            "SELECT CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' END FROM t",
        );
        assert_rule_not_triggers(
            &StructureSimpleCase,
            "SELECT CASE WHEN x = 1 THEN 'a' WHEN y = 2 THEN 'b' END FROM t",
        );

        assert_rule_triggers(&StructureSubquery, "SELECT * FROM (SELECT 1) sub");
        assert_rule_not_triggers(
            &StructureSubquery,
            "WITH cte AS (SELECT 1) SELECT * FROM cte",
        );

        assert_rule_triggers(&StructureColumnOrder, "SELECT a + 1, a FROM t");
        assert_rule_not_triggers(&StructureColumnOrder, "SELECT a, a + 1 FROM t");

        assert_rule_triggers(&StructureDistinct, "SELECT DISTINCT(a) FROM t");
        assert_rule_not_triggers(&StructureDistinct, "SELECT DISTINCT a FROM t");

        assert_rule_triggers(
            &StructureJoinConditionOrder,
            "SELECT * FROM users u JOIN orders o ON o.user_id = u.id",
        );
        assert_rule_not_triggers(
            &StructureJoinConditionOrder,
            "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
        );

        assert_rule_triggers(&StructureConstantExpression, "SELECT * FROM t WHERE 1 = 1");
        assert_rule_not_triggers(&StructureConstantExpression, "SELECT * FROM t WHERE id = 1");

        assert_rule_triggers(
            &StructureUnusedJoin,
            "SELECT u.id FROM users u JOIN orders o ON u.id = u.id",
        );
        assert_rule_not_triggers(
            &StructureUnusedJoin,
            "SELECT u.id, o.id FROM users u JOIN orders o ON o.user_id = u.id",
        );

        assert_rule_triggers(&StructureConsecutiveSemicolons, "SELECT 1;;");
        assert_rule_not_triggers(&StructureConsecutiveSemicolons, "SELECT 1;");
    }

    #[test]
    fn tsql_rules_cover_fail_and_pass_cases() {
        assert_rule_triggers(
            &TsqlSpPrefix,
            "SELECT 'CREATE PROCEDURE sp_legacy AS SELECT 1' AS sql_snippet",
        );
        assert_rule_not_triggers(
            &TsqlSpPrefix,
            "SELECT 'CREATE PROCEDURE proc_legacy AS SELECT 1' AS sql_snippet",
        );

        assert_rule_triggers(
            &TsqlProcedureBeginEnd,
            "SELECT 'CREATE PROCEDURE p AS SELECT 1' AS sql_snippet",
        );
        assert_rule_not_triggers(
            &TsqlProcedureBeginEnd,
            "SELECT 'CREATE PROCEDURE p AS BEGIN SELECT 1 END' AS sql_snippet",
        );

        assert_rule_triggers(&TsqlEmptyBatch, "SELECT '\nGO\nGO\n' AS sql_snippet");
        assert_rule_not_triggers(&TsqlEmptyBatch, "SELECT '\nGO\n' AS sql_snippet");
    }

    fn assert_rule_triggers(rule: &dyn LintRule, sql: &str) {
        let issues = run_rule(rule, sql);
        assert!(
            issues.iter().any(|issue| issue.code == rule.code()),
            "expected {} to trigger for SQL: {sql}; got: {issues:?}",
            rule.code(),
        );
    }

    fn assert_rule_not_triggers(rule: &dyn LintRule, sql: &str) {
        let issues = run_rule(rule, sql);
        assert!(
            !issues.iter().any(|issue| issue.code == rule.code()),
            "did not expect {} for SQL: {sql}; got: {issues:?}",
            rule.code(),
        );
    }
}
