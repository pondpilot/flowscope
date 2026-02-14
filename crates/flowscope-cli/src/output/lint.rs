//! Lint output formatting (sqlfluff-style).

use flowscope_core::Severity;
use owo_colors::OwoColorize;
use std::fmt::Write;
use std::time::Duration;

/// Per-file lint result used by the formatter.
pub struct FileLintResult {
    pub name: String,
    pub sql: String,
    pub issues: Vec<LintIssue>,
}

/// A lint issue resolved to line:col.
pub struct LintIssue {
    pub line: usize,
    pub col: usize,
    pub code: String,
    pub message: String,
    pub severity: Severity,
}

/// Convert a byte offset into a 1-based (line, col) pair.
pub fn offset_to_line_col(sql: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(sql.len());
    let mut line = 1usize;
    let mut col = 1usize;

    for (i, ch) in sql.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }

    (line, col)
}

/// Format lint results as human-readable sqlfluff-style text.
pub fn format_lint_results(results: &[FileLintResult], colored: bool, elapsed: Duration) -> String {
    let mut out = String::new();

    let mut total_pass = 0usize;
    let mut total_fail = 0usize;
    let mut total_violations = 0usize;

    for file in results {
        let has_issues = !file.issues.is_empty();

        if has_issues {
            total_fail += 1;
            total_violations += file.issues.len();
        } else {
            total_pass += 1;
        }

        write_file_section(&mut out, file, colored);
    }

    write_summary(
        &mut out,
        total_pass,
        total_fail,
        total_violations,
        colored,
        elapsed,
    );

    out
}

fn write_file_section(out: &mut String, file: &FileLintResult, colored: bool) {
    let status = if file.issues.is_empty() {
        if colored {
            "PASS".green().to_string()
        } else {
            "PASS".to_string()
        }
    } else if colored {
        "FAIL".red().to_string()
    } else {
        "FAIL".to_string()
    };

    writeln!(out, "== [{}] {}", file.name, status).unwrap();

    // Sort issues by line, then column
    let mut sorted: Vec<&LintIssue> = file.issues.iter().collect();
    sorted.sort_by_key(|i| (i.line, i.col));

    for issue in sorted {
        let code_str = if colored {
            match issue.severity {
                Severity::Error => issue.code.red().to_string(),
                Severity::Warning => issue.code.yellow().to_string(),
                Severity::Info => issue.code.blue().to_string(),
            }
        } else {
            issue.code.clone()
        };

        writeln!(
            out,
            "L:{:>4} | P:{:>4} | {} | {}",
            issue.line, issue.col, code_str, issue.message
        )
        .unwrap();
    }
}

fn write_summary(
    out: &mut String,
    pass: usize,
    fail: usize,
    violations: usize,
    colored: bool,
    elapsed: Duration,
) {
    writeln!(out, "All Finished in {}!", format_elapsed(elapsed)).unwrap();

    let summary = format!(
        "  {} passed. {} failed. {} violations found.",
        pass_str(pass, colored),
        fail_str(fail, colored),
        violations
    );
    writeln!(out, "{summary}").unwrap();
}

fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs_f64();
    if secs >= 1.0 {
        format!("{secs:.2}s")
    } else if elapsed.as_millis() >= 1 {
        format!("{}ms", elapsed.as_millis())
    } else {
        format!("{}us", elapsed.as_micros())
    }
}

fn pass_str(count: usize, colored: bool) -> String {
    let s = format!("{count} file{}", if count == 1 { "" } else { "s" });
    if colored && count > 0 {
        s.green().to_string()
    } else {
        s
    }
}

fn fail_str(count: usize, colored: bool) -> String {
    let s = format!("{count} file{}", if count == 1 { "" } else { "s" });
    if colored && count > 0 {
        s.red().to_string()
    } else {
        s
    }
}

/// Format lint results as JSON.
pub fn format_lint_json(results: &[FileLintResult], compact: bool) -> String {
    let json_results: Vec<serde_json::Value> = results
        .iter()
        .map(|file| {
            let violations: Vec<serde_json::Value> = file
                .issues
                .iter()
                .map(|issue| {
                    serde_json::json!({
                        "line": issue.line,
                        "column": issue.col,
                        "code": issue.code,
                        "message": issue.message,
                        "severity": match issue.severity {
                            Severity::Error => "error",
                            Severity::Warning => "warning",
                            Severity::Info => "info",
                        }
                    })
                })
                .collect();

            serde_json::json!({
                "file": file.name,
                "violations": violations
            })
        })
        .collect();

    if compact {
        serde_json::to_string(&json_results).unwrap_or_default()
    } else {
        serde_json::to_string_pretty(&json_results).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_to_line_col_start() {
        assert_eq!(offset_to_line_col("SELECT 1", 0), (1, 1));
    }

    #[test]
    fn test_offset_to_line_col_same_line() {
        assert_eq!(offset_to_line_col("SELECT 1", 7), (1, 8));
    }

    #[test]
    fn test_offset_to_line_col_second_line() {
        let sql = "SELECT 1\nFROM t";
        // offset 9 = 'F' on second line
        assert_eq!(offset_to_line_col(sql, 9), (2, 1));
    }

    #[test]
    fn test_offset_to_line_col_mid_second_line() {
        let sql = "SELECT 1\nFROM t";
        // offset 14 = 't' on second line
        assert_eq!(offset_to_line_col(sql, 14), (2, 6));
    }

    #[test]
    fn test_offset_to_line_col_past_end() {
        let sql = "SELECT 1";
        assert_eq!(offset_to_line_col(sql, 100), (1, 9));
    }

    #[test]
    fn test_offset_to_line_col_utf8_chars() {
        let sql = "SELECT 'é' UNION SELECT 1";
        let union_offset = sql.find("UNION").expect("UNION position");
        assert_eq!(offset_to_line_col(sql, union_offset), (1, 12));
    }

    #[test]
    fn test_format_lint_pass() {
        let results = vec![FileLintResult {
            name: "clean.sql".to_string(),
            sql: String::new(),
            issues: vec![],
        }];

        let output = format_lint_results(&results, false, Duration::from_millis(250));
        assert!(output.contains("PASS"));
        assert!(output.contains("All Finished in 250ms!"));
        assert!(output.contains("clean.sql"));
        assert!(output.contains("1 file passed"));
        assert!(output.contains("0 files failed"));
        assert!(output.contains("0 violations"));
    }

    #[test]
    fn test_format_lint_fail() {
        let results = vec![FileLintResult {
            name: "bad.sql".to_string(),
            sql: String::new(),
            issues: vec![
                LintIssue {
                    line: 3,
                    col: 12,
                    code: "LINT_AM_007".to_string(),
                    message: "Use UNION DISTINCT or UNION ALL instead of bare UNION.".to_string(),
                    severity: Severity::Info,
                },
                LintIssue {
                    line: 7,
                    col: 1,
                    code: "LINT_ST_006".to_string(),
                    message: "CTE 'unused' is defined but never referenced.".to_string(),
                    severity: Severity::Info,
                },
            ],
        }];

        let output = format_lint_results(&results, false, Duration::from_secs_f64(1.5));
        assert!(output.contains("FAIL"));
        assert!(output.contains("All Finished in 1.50s!"));
        assert!(output.contains("bad.sql"));
        assert!(output.contains("LINT_AM_007"));
        assert!(output.contains("LINT_ST_006"));
        assert!(output.contains("L:   3 | P:  12"));
        assert!(output.contains("L:   7 | P:   1"));
        assert!(output.contains("2 violations"));
    }

    #[test]
    fn test_summary_formatting() {
        let results = vec![
            FileLintResult {
                name: "a.sql".to_string(),
                sql: String::new(),
                issues: vec![],
            },
            FileLintResult {
                name: "b.sql".to_string(),
                sql: String::new(),
                issues: vec![LintIssue {
                    line: 1,
                    col: 1,
                    code: "LINT_AM_007".to_string(),
                    message: "test".to_string(),
                    severity: Severity::Info,
                }],
            },
        ];

        let output = format_lint_results(&results, false, Duration::from_micros(700));
        assert!(output.contains("All Finished in 700us!"));
        assert!(output.contains("1 file passed"));
        assert!(output.contains("1 file failed"));
        assert!(output.contains("1 violations"));
    }

    #[test]
    fn test_format_lint_json() {
        let results = vec![FileLintResult {
            name: "test.sql".to_string(),
            sql: String::new(),
            issues: vec![LintIssue {
                line: 1,
                col: 8,
                code: "LINT_AM_007".to_string(),
                message: "Use UNION DISTINCT or UNION ALL.".to_string(),
                severity: Severity::Info,
            }],
        }];

        let json = format_lint_json(&results, false);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["file"], "test.sql");
        assert_eq!(arr[0]["violations"][0]["code"], "LINT_AM_007");
        assert_eq!(arr[0]["violations"][0]["line"], 1);
        assert_eq!(arr[0]["violations"][0]["column"], 8);
    }
}
