use crate::error::ParseError;
use crate::types::Dialect;
use sqlparser::ast::Statement;
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

/// Result of parsing SQL with fallback metadata.
pub struct ParseSqlOutput {
    pub statements: Vec<Statement>,
    pub parser_fallback_used: bool,
}

/// Parse SQL using the specified dialect
pub fn parse_sql_with_dialect(sql: &str, dialect: Dialect) -> Result<Vec<Statement>, ParseError> {
    parse_sql_with_dialect_output(sql, dialect).map(|output| output.statements)
}

/// Parse SQL using the specified dialect and report whether parser fallback was used.
pub fn parse_sql_with_dialect_output(
    sql: &str,
    dialect: Dialect,
) -> Result<ParseSqlOutput, ParseError> {
    let sqlparser_dialect = dialect.to_sqlparser_dialect();
    match Parser::parse_sql(sqlparser_dialect.as_ref(), sql) {
        Ok(statements) => Ok(ParseSqlOutput {
            statements,
            parser_fallback_used: false,
        }),
        Err(primary_err) => {
            if let Some(sanitized_sql) = sanitize_escaped_identifiers_for_dialect(sql, dialect) {
                if let Ok(statements) =
                    Parser::parse_sql(sqlparser_dialect.as_ref(), &sanitized_sql)
                {
                    return Ok(ParseSqlOutput {
                        statements,
                        parser_fallback_used: true,
                    });
                }
            }

            if let Some(sanitized_sql) = sanitize_trailing_comma_before_from(sql) {
                if let Ok(statements) =
                    Parser::parse_sql(sqlparser_dialect.as_ref(), &sanitized_sql)
                {
                    return Ok(ParseSqlOutput {
                        statements,
                        parser_fallback_used: true,
                    });
                }
            }

            // Parity fallback: Generic dialect frequently fails on Postgres-specific
            // operators (`?`, `->>`, `::`) commonly used in warehouse SQL.
            if matches!(dialect, Dialect::Generic) && looks_like_postgres_syntax(sql) {
                let postgres = PostgreSqlDialect {};
                if let Ok(statements) = Parser::parse_sql(&postgres, sql) {
                    return Ok(ParseSqlOutput {
                        statements,
                        parser_fallback_used: true,
                    });
                }
            }
            Err(primary_err.into())
        }
    }
}

fn looks_like_postgres_syntax(sql: &str) -> bool {
    sql.contains("::")
        || sql.contains("->")
        || sql.contains("?|")
        || sql.contains("?&")
        || sql.contains(" ? ")
        || sql.contains(" ?\n")
        || sql.contains("? '")
        || sql.contains("?\t")
}

fn sanitize_escaped_identifiers_for_dialect(sql: &str, dialect: Dialect) -> Option<String> {
    let delimiters: &[u8] = match dialect {
        Dialect::Bigquery => &[b'`'],
        Dialect::Clickhouse => &[b'`', b'"'],
        _ => return None,
    };

    if !sql.as_bytes().contains(&b'\\') {
        return None;
    }

    let mut rewritten = rewrite_escaped_quoted_identifiers(sql, delimiters);

    if matches!(dialect, Dialect::Clickhouse) {
        rewritten = remove_trailing_comma_before_from(&rewritten);
    }

    (rewritten != sql).then_some(rewritten)
}

fn sanitize_trailing_comma_before_from(sql: &str) -> Option<String> {
    let rewritten = remove_trailing_comma_before_from(sql);
    (rewritten != sql).then_some(rewritten)
}

fn rewrite_escaped_quoted_identifiers(sql: &str, delimiters: &[u8]) -> String {
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0usize;
    let len = bytes.len();

    while i < len {
        if bytes[i] == b'\'' {
            let start = i;
            i += 1;
            while i < len {
                if bytes[i] == b'\'' {
                    if i + 1 < len && bytes[i + 1] == b'\'' {
                        i += 2;
                    } else {
                        i += 1;
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            out.push_str(&sql[start..i]);
            continue;
        }

        if bytes[i] == b'-' && i + 1 < len && bytes[i + 1] == b'-' {
            let start = i;
            i += 2;
            while i < len && bytes[i] != b'\n' && bytes[i] != b'\r' {
                i += 1;
            }
            out.push_str(&sql[start..i]);
            continue;
        }

        if bytes[i] == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
            let start = i;
            i += 2;
            while i + 1 < len {
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            out.push_str(&sql[start..i.min(len)]);
            continue;
        }

        if delimiters.contains(&bytes[i]) {
            let delimiter = bytes[i];
            let start = i;
            i += 1;
            let mut content = String::new();
            let mut had_escape = false;
            let mut closed = false;

            while i < len {
                if bytes[i] == b'\\' && i + 1 < len && bytes[i + 1] == delimiter {
                    had_escape = true;
                    content.push('_');
                    i += 2;
                    continue;
                }

                if bytes[i] == delimiter {
                    if i + 1 < len && bytes[i + 1] == delimiter {
                        had_escape = true;
                        content.push('_');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    closed = true;
                    break;
                }

                content.push(bytes[i] as char);
                i += 1;
            }

            if !closed {
                out.push_str(&sql[start..len]);
                break;
            }

            if had_escape {
                let normalized = normalize_identifier_content(&content);
                out.push(delimiter as char);
                out.push_str(&normalized);
                out.push(delimiter as char);
            } else {
                out.push_str(&sql[start..i]);
            }
            continue;
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    out
}

fn normalize_identifier_content(content: &str) -> String {
    let mut normalized = String::with_capacity(content.len());
    for ch in content.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            normalized.push(ch.to_ascii_lowercase());
        } else {
            normalized.push('_');
        }
    }

    if normalized.is_empty() || normalized.chars().all(|ch| ch == '_') {
        "escaped_identifier".to_string()
    } else {
        normalized
    }
}

fn remove_trailing_comma_before_from(sql: &str) -> String {
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0usize;
    let len = bytes.len();

    while i < len {
        if bytes[i] == b',' {
            let mut j = i + 1;
            while j < len && matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r') {
                j += 1;
            }

            if j + 4 <= len
                && bytes[j..j + 4].eq_ignore_ascii_case(b"FROM")
                && (j + 4 == len || !bytes[j + 4].is_ascii_alphanumeric())
            {
                i += 1;
                continue;
            }
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    out
}

/// Parse SQL using the generic dialect (legacy compatibility)
pub fn parse_sql(sql: &str) -> Result<Vec<Statement>, ParseError> {
    parse_sql_with_dialect(sql, Dialect::Generic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_select() {
        let sql = "SELECT * FROM users";
        let result = parse_sql(sql);
        assert!(result.is_ok());
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    }

    #[test]
    fn test_parse_invalid_sql() {
        let sql = "SELECT * FROM";
        let result = parse_sql(sql);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_multiple_statements() {
        let sql = "SELECT * FROM users; SELECT * FROM orders;";
        let result = parse_sql(sql);
        assert!(result.is_ok());
        let statements = result.unwrap();
        assert_eq!(statements.len(), 2);
    }

    #[test]
    fn test_parse_with_postgres_dialect() {
        let sql = "SELECT * FROM users WHERE name ILIKE '%test%'";
        let result = parse_sql_with_dialect(sql, Dialect::Postgres);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_with_snowflake_dialect() {
        let sql = "SELECT * FROM db.schema.table";
        let result = parse_sql_with_dialect(sql, Dialect::Snowflake);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_with_bigquery_dialect() {
        let sql = "SELECT * FROM `project.dataset.table`";
        let result = parse_sql_with_dialect(sql, Dialect::Bigquery);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_cte() {
        let sql = r#"
            WITH active_users AS (
                SELECT * FROM users WHERE active = true
            )
            SELECT * FROM active_users
        "#;
        let result = parse_sql(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_insert_select() {
        let sql = "INSERT INTO archive SELECT * FROM users WHERE deleted = true";
        let result = parse_sql(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_create_table_as() {
        let sql = "CREATE TABLE users_backup AS SELECT * FROM users";
        let result = parse_sql(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_union() {
        let sql = "SELECT id FROM users UNION ALL SELECT id FROM admins";
        let result = parse_sql(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_generic_falls_back_for_postgres_json_operator() {
        let sql = "SELECT usage_metadata ? 'pipeline_id' FROM ledger.usage_line_item";
        let result = parse_sql(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_generic_falls_back_for_postgres_cast_operator() {
        let sql = "SELECT workspace_id::text FROM ledger.usage_line_item";
        let result = parse_sql(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_output_marks_parser_fallback_usage() {
        let generic = sqlparser::dialect::GenericDialect {};
        let sql = [
            "SELECT usage_metadata ? 'pipeline_id' FROM ledger.usage_line_item",
            "SELECT workspace_id::text FROM ledger.usage_line_item",
            "SELECT payload->>'id' FROM ledger.usage_line_item",
        ]
        .into_iter()
        .find(|candidate| Parser::parse_sql(&generic, candidate).is_err())
        .expect("expected at least one postgres-only candidate to fail in generic parser");

        let output = parse_sql_with_dialect_output(sql, Dialect::Generic).expect("parse");
        assert!(output.parser_fallback_used);
        assert_eq!(output.statements.len(), 1);
    }

    #[test]
    fn test_parse_output_bigquery_escaped_identifier_fallback_usage() {
        let sql = "SELECT `\\`a`.col1 FROM tab1 as `\\`A`";
        let output = parse_sql_with_dialect_output(sql, Dialect::Bigquery).expect("parse");
        assert!(output.parser_fallback_used);
        assert_eq!(output.statements.len(), 1);
    }

    #[test]
    fn test_parse_output_clickhouse_escaped_identifier_fallback_usage() {
        let sql = "SELECT \"\\\"`a`\"\"\".col1,\nFROM tab1 as `\"\\`a``\"`";
        let output = parse_sql_with_dialect_output(sql, Dialect::Clickhouse).expect("parse");
        assert!(output.parser_fallback_used);
        assert_eq!(output.statements.len(), 1);
    }

    #[test]
    fn test_parse_output_trailing_comma_before_from_fallback_usage() {
        let sql = "SELECT widget.id,\nwidget.name,\nFROM widget";
        let output = parse_sql_with_dialect_output(sql, Dialect::Ansi).expect("parse");
        assert!(output.parser_fallback_used);
        assert_eq!(output.statements.len(), 1);
    }

    #[test]
    fn test_parse_output_without_fallback() {
        let sql = "SELECT 1";
        let output = parse_sql_with_dialect_output(sql, Dialect::Generic).expect("parse");
        assert!(!output.parser_fallback_used);
        assert_eq!(output.statements.len(), 1);
    }
}
