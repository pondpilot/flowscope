use crate::error::ParseError;
use crate::types::Dialect;
use sqlparser::ast::Statement;
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

/// Parse SQL using the specified dialect
pub fn parse_sql_with_dialect(sql: &str, dialect: Dialect) -> Result<Vec<Statement>, ParseError> {
    let sqlparser_dialect = dialect.to_sqlparser_dialect();
    match Parser::parse_sql(sqlparser_dialect.as_ref(), sql) {
        Ok(statements) => Ok(statements),
        Err(primary_err) => {
            // Parity fallback: Generic dialect frequently fails on Postgres-specific
            // operators (`?`, `->>`, `::`) commonly used in warehouse SQL.
            if matches!(dialect, Dialect::Generic) && looks_like_postgres_syntax(sql) {
                let postgres = PostgreSqlDialect {};
                if let Ok(statements) = Parser::parse_sql(&postgres, sql) {
                    return Ok(statements);
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
        || sql.contains("?	")
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
}
