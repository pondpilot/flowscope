//! Constraint extraction utilities for DDL statements.
//!
//! This module provides shared logic for extracting primary key, foreign key,
//! and other constraints from SQL DDL statements.

use crate::types::{ColumnSchema, ConstraintType, ForeignKeyRef, TableConstraintInfo};
use sqlparser::ast::{ColumnDef, ColumnOption, ColumnOptionDef, TableConstraint};
use std::collections::HashSet;

/// Extract constraint info from column options (inline PRIMARY KEY, FOREIGN KEY).
///
/// Returns a tuple of (is_primary_key, foreign_key_ref).
/// The foreign_key_ref is only created if the referenced column can be determined.
pub fn extract_column_constraints(
    options: &[ColumnOptionDef],
) -> (Option<bool>, Option<ForeignKeyRef>) {
    let mut is_pk = None;
    let mut fk_ref = None;

    for opt in options {
        match &opt.option {
            ColumnOption::Unique { is_primary, .. } if *is_primary => {
                is_pk = Some(true);
            }
            ColumnOption::ForeignKey {
                foreign_table,
                referred_columns,
                ..
            } => {
                // Only create FK ref if we have a referenced column.
                // If referred_columns is empty (e.g., `REFERENCES orders`), we skip
                // creating the FK reference since we can't determine the target column.
                if let Some(col) = referred_columns.first() {
                    fk_ref = Some(ForeignKeyRef {
                        table: foreign_table.to_string(),
                        column: col.value.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    (is_pk, fk_ref)
}

/// Extract table-level constraints (composite PRIMARY KEY, FOREIGN KEY, UNIQUE).
///
/// Returns a tuple of (pk_column_names, constraint_infos).
/// The pk_column_names is useful for marking individual columns as part of a composite PK.
pub fn extract_table_constraints(
    constraints: &[TableConstraint],
) -> (Vec<String>, Vec<TableConstraintInfo>) {
    let mut pk_columns = Vec::new();
    let mut constraint_infos = Vec::new();

    for constraint in constraints {
        match constraint {
            TableConstraint::PrimaryKey { columns, .. } => {
                // IndexColumn has column: OrderByExpr, we extract the column name from expr
                let col_names: Vec<String> =
                    columns.iter().map(|c| c.column.expr.to_string()).collect();
                pk_columns.extend(col_names.clone());
                constraint_infos.push(TableConstraintInfo {
                    constraint_type: ConstraintType::PrimaryKey,
                    columns: col_names,
                    referenced_table: None,
                    referenced_columns: None,
                });
            }
            TableConstraint::ForeignKey {
                columns,
                foreign_table,
                referred_columns,
                ..
            } => {
                // FK columns are Vec<Ident>
                let col_names: Vec<String> = columns.iter().map(|c| c.value.clone()).collect();
                let ref_col_names: Vec<String> =
                    referred_columns.iter().map(|c| c.value.clone()).collect();
                constraint_infos.push(TableConstraintInfo {
                    constraint_type: ConstraintType::ForeignKey,
                    columns: col_names,
                    referenced_table: Some(foreign_table.to_string()),
                    referenced_columns: Some(ref_col_names),
                });
            }
            TableConstraint::Unique { columns, .. } => {
                // IndexColumn has column: OrderByExpr
                let col_names: Vec<String> =
                    columns.iter().map(|c| c.column.expr.to_string()).collect();
                constraint_infos.push(TableConstraintInfo {
                    constraint_type: ConstraintType::Unique,
                    columns: col_names,
                    referenced_table: None,
                    referenced_columns: None,
                });
            }
            _ => {}
        }
    }

    (pk_columns, constraint_infos)
}

/// Build column schemas with constraint information from DDL column definitions.
///
/// This function consolidates the logic for extracting column schema and constraint
/// information from CREATE TABLE statements, used in both DDL pre-collection and
/// main analysis passes.
///
/// # Arguments
/// * `columns` - Column definitions from the CREATE TABLE statement
/// * `table_constraints` - Table-level constraints (composite PKs, FKs, etc.)
///
/// # Returns
/// A tuple of (column_schemas, table_constraint_infos) ready for schema registration.
pub fn build_column_schemas_with_constraints(
    columns: &[ColumnDef],
    table_constraints: &[TableConstraint],
) -> (Vec<ColumnSchema>, Vec<TableConstraintInfo>) {
    // Extract table-level constraints and build a set of PK column names for O(1) lookup.
    let (pk_column_names, table_constraint_infos) = extract_table_constraints(table_constraints);
    let pk_columns_set: HashSet<&str> = pk_column_names.iter().map(|s| s.as_str()).collect();

    let column_schemas = columns
        .iter()
        .map(|c| {
            let (is_pk, fk_ref) = extract_column_constraints(&c.options);
            // Column is PK if either inline constraint or in table-level PK
            let is_primary_key =
                if is_pk.unwrap_or(false) || pk_columns_set.contains(c.name.value.as_str()) {
                    Some(true)
                } else {
                    None
                };
            ColumnSchema {
                name: c.name.value.clone(),
                data_type: Some(c.data_type.to_string()),
                is_primary_key,
                foreign_key: fk_ref,
            }
        })
        .collect();

    (column_schemas, table_constraint_infos)
}
