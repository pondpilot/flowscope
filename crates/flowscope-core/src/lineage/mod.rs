use sqlparser::ast::{Statement, TableFactor};

pub fn extract_tables(statements: &[Statement]) -> Vec<String> {
    let mut tables = Vec::new();

    for statement in statements {
        match statement {
            Statement::Query(query) => {
                extract_tables_from_query_body(&query.body, &mut tables);
            }
            _ => {}
        }
    }

    tables
}

fn extract_tables_from_query_body(body: &sqlparser::ast::SetExpr, tables: &mut Vec<String>) {
    use sqlparser::ast::SetExpr;

    match body {
        SetExpr::Select(select) => {
            for table_with_joins in &select.from {
                extract_tables_from_table_factor(&table_with_joins.relation, tables);

                for join in &table_with_joins.joins {
                    extract_tables_from_table_factor(&join.relation, tables);
                }
            }
        }
        SetExpr::Query(query) => {
            extract_tables_from_query_body(&query.body, tables);
        }
        SetExpr::SetOperation { left, right, .. } => {
            extract_tables_from_query_body(left, tables);
            extract_tables_from_query_body(right, tables);
        }
        SetExpr::Values(_) => {}
        SetExpr::Insert(_) => {}
        SetExpr::Update(_) => {}
        SetExpr::Table(_) => {}
    }
}

fn extract_tables_from_table_factor(table_factor: &TableFactor, tables: &mut Vec<String>) {
    match table_factor {
        TableFactor::Table { name, .. } => {
            tables.push(name.to_string());
        }
        TableFactor::Derived { subquery, .. } => {
            extract_tables_from_query_body(&subquery.body, tables);
        }
        TableFactor::TableFunction { .. } => {}
        TableFactor::Function { .. } => {}
        TableFactor::UNNEST { .. } => {}
        TableFactor::NestedJoin { table_with_joins, .. } => {
            extract_tables_from_table_factor(&table_with_joins.relation, tables);
            for join in &table_with_joins.joins {
                extract_tables_from_table_factor(&join.relation, tables);
            }
        }
        TableFactor::Pivot { .. } => {}
        TableFactor::Unpivot { .. } => {}
        TableFactor::MatchRecognize { .. } => {}
        TableFactor::JsonTable { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_sql;

    #[test]
    fn test_extract_single_table() {
        let sql = "SELECT * FROM users";
        let statements = parse_sql(sql).unwrap();
        let tables = extract_tables(&statements);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0], "users");
    }

    #[test]
    fn test_extract_multiple_tables_join() {
        let sql = "SELECT * FROM users JOIN orders ON users.id = orders.user_id";
        let statements = parse_sql(sql).unwrap();
        let tables = extract_tables(&statements);
        assert_eq!(tables.len(), 2);
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"orders".to_string()));
    }

    #[test]
    fn test_extract_with_schema() {
        let sql = "SELECT * FROM public.users";
        let statements = parse_sql(sql).unwrap();
        let tables = extract_tables(&statements);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0], "public.users");
    }
}
