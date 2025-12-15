//! Schema registry for managing table schema information.
//!
//! This module provides [`SchemaRegistry`], which centralizes all schema-related state
//! including imported (user-provided) and implied (DDL-inferred) table schemas.
//! It handles schema resolution, conflict detection, and case sensitivity normalization.
//!
//! # Architecture
//!
//! The `SchemaRegistry` is the single source of truth for schema metadata during analysis.
//! It manages two types of schema information:
//!
//! - **Imported schema**: User-provided schema metadata (e.g., from a database catalog).
//!   This is considered authoritative and cannot be overwritten by DDL inference.
//!
//! - **Implied schema**: Schema inferred from DDL statements (CREATE TABLE, etc.).
//!   This is captured opportunistically to provide better analysis even without
//!   explicit schema metadata.
//!
//! # Table Resolution
//!
//! When resolving table references, the registry uses a multi-step process:
//!
//! 1. Check if the reference is already fully qualified (e.g., `catalog.schema.table`)
//! 2. Search through the search path for unqualified names
//! 3. Apply default schema/catalog if configured
//! 4. Normalize identifiers according to dialect case sensitivity rules
//!
//! # Thread Safety
//!
//! `SchemaRegistry` is designed for single-threaded use within an analysis pass.
//! If concurrent access is needed, external synchronization must be provided.

use crate::types::{
    issue_codes, CaseSensitivity, ColumnSchema, Dialect, Issue, SchemaMetadata, SchemaOrigin,
    SchemaTable, TableConstraintInfo,
};
use chrono::{DateTime, Utc};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use super::helpers::{
    extract_simple_name, is_quoted_identifier, split_qualified_identifiers, unquote_identifier,
};

/// Parameters for registering implied schema.
///
/// Groups the arguments for `register_implied_internal` to improve readability.
pub(crate) struct RegisterImpliedParams<'a> {
    /// The canonical table name.
    pub(crate) canonical: &'a str,
    /// Column definitions for the table.
    pub(crate) columns: Vec<ColumnSchema>,
    /// Table-level constraints (composite PKs, FKs, etc.).
    pub(crate) constraints: Vec<TableConstraintInfo>,
    /// Whether this is a temporary/session-scoped table.
    pub(crate) is_temporary: bool,
    /// The type of statement that created this schema (e.g., "CREATE TABLE").
    pub(crate) statement_type: &'a str,
    /// The index of the statement that created this schema.
    pub(crate) statement_index: usize,
    /// Whether to emit warnings for schema conflicts.
    pub(crate) emit_warnings: bool,
    /// Whether this is a seed operation (forward declaration).
    pub(crate) is_seed: bool,
}

/// Schema table entry with origin metadata for tracking imported vs implied schema.
///
/// Each entry tracks not only the table schema itself, but also metadata about
/// where the schema information came from and when it was last updated.
#[derive(Debug, Clone)]
pub(crate) struct SchemaTableEntry {
    /// The table schema (name, columns, etc.).
    pub(crate) table: SchemaTable,
    /// Whether this schema was imported or inferred from DDL.
    pub(crate) origin: SchemaOrigin,
    /// The statement index that created this schema (for implied schemas).
    pub(crate) source_statement_idx: Option<usize>,
    /// When this entry was last updated.
    pub(crate) updated_at: DateTime<Utc>,
    /// Whether this is a temporary table (session-scoped).
    pub(crate) temporary: bool,
    /// Table-level constraints (composite PKs, FKs, etc.)
    pub(crate) constraints: Vec<TableConstraintInfo>,
}

/// Search path entry for resolving unqualified table names.
///
/// Similar to PostgreSQL's `search_path`, this allows unqualified table names
/// to be resolved by searching through a list of schemas in order.
#[derive(Debug, Clone)]
pub(crate) struct SearchPathEntry {
    /// Optional catalog qualifier (e.g., database name in some systems).
    pub(crate) catalog: Option<String>,
    /// Schema name to search within.
    pub(crate) schema: String,
}

/// Result of resolving a table reference.
///
/// Contains both the canonical name and metadata about how the resolution occurred.
#[derive(Debug, Clone)]
pub(crate) struct TableResolution {
    /// The canonical (normalized, fully-qualified) table name.
    ///
    /// This is the normalized form used for lookups and comparison.
    /// Format depends on qualification level: `table`, `schema.table`, or `catalog.schema.table`.
    pub(crate) canonical: String,
    /// Whether the table was found in known schema.
    ///
    /// When `true`, the table exists in either imported or implied schema.
    /// When `false`, the table reference could not be matched to any known table.
    pub(crate) matched_schema: bool,
}

/// Centralizes all schema-related state and operations.
///
/// `SchemaRegistry` is the core component for schema management during SQL analysis.
/// It provides a unified interface for:
///
/// - **Table tracking**: Maintains sets of known tables from imported schema and DDL inference
/// - **Schema lookup**: Maps canonical table names to full schema entries with column info
/// - **Name resolution**: Resolves table references using search paths and defaults
/// - **Case normalization**: Handles identifier case sensitivity per SQL dialect
///
/// # Invariants
///
/// - `known_tables` is a superset of all tables in `schema_tables` and `imported_tables`
/// - Tables in `imported_tables` are never overwritten by implied schema
/// - All canonical names are normalized according to the current case sensitivity setting
///
/// # Example
///
/// ```ignore
/// let (registry, issues) = SchemaRegistry::new(Some(&schema_metadata), Dialect::Postgres);
///
/// // Resolve an unqualified table name
/// let resolution = registry.canonicalize_table_reference("users");
/// if resolution.matched_schema {
///     println!("Found table: {}", resolution.canonical);
/// }
/// ```
pub(crate) struct SchemaRegistry {
    /// All known table canonical names (for quick existence checks).
    ///
    /// This is a superset containing all tables from any source:
    /// - Tables from `imported_tables` (user-provided schema)
    /// - Tables from `forward_declared_tables` (discovered during DDL pre-pass)
    /// - Tables discovered during analysis (e.g., from DDL statements)
    ///
    /// Used for O(1) existence checks when resolving table references.
    pub(crate) known_tables: HashSet<String>,
    /// Tables discovered during the DDL pre-collection pass (forward declarations).
    ///
    /// These are CREATE TABLE/VIEW targets found before the main analysis begins.
    /// They allow earlier statements to reference tables defined later in the script.
    ///
    /// This set is used to distinguish between:
    /// - Tables the user explicitly provided schema for (`imported_tables`)
    /// - Tables only known because they appear in DDL within the script
    ///
    /// When all known tables are forward-declared (i.e., no imported schema),
    /// we suppress `UNRESOLVED_REFERENCE` warnings for external tables since
    /// the user hasn't provided authoritative schema metadata.
    forward_declared_tables: HashSet<String>,
    /// Tables from imported (user-provided) schema that should not be overwritten.
    ///
    /// These represent authoritative schema from an external source (e.g., database catalog).
    /// Tables in this set:
    /// - Have priority over DDL-inferred schema (conflicts emit warnings)
    /// - Cannot be removed by DROP statements
    /// - Trigger `UNRESOLVED_REFERENCE` warnings when other tables are referenced
    pub(crate) imported_tables: HashSet<String>,
    /// Schema lookup: table canonical name -> table schema entry with metadata.
    ///
    /// Contains full schema information including column definitions.
    pub(crate) schema_tables: HashMap<String, SchemaTableEntry>,
    /// Default catalog for unqualified identifiers (e.g., database name).
    pub(crate) default_catalog: Option<String>,
    /// Default schema for unqualified identifiers (e.g., "public" in PostgreSQL).
    pub(crate) default_schema: Option<String>,
    /// Ordered search path entries for resolution.
    ///
    /// When resolving unqualified names, schemas are searched in order.
    pub(crate) search_path: Vec<SearchPathEntry>,
    /// Case sensitivity setting for identifier normalization.
    case_sensitivity: CaseSensitivity,
    /// SQL dialect for default case sensitivity behavior.
    dialect: Dialect,
    /// Whether to capture schema from DDL statements.
    allow_implied: bool,
    /// Cache for identifier normalization to avoid repeated string operations.
    identifier_cache: RefCell<HashMap<String, String>>,
}

impl SchemaRegistry {
    /// Creates a new schema registry from request metadata.
    pub(crate) fn new(schema: Option<&SchemaMetadata>, dialect: Dialect) -> (Self, Vec<Issue>) {
        let mut registry = Self {
            known_tables: HashSet::new(),
            forward_declared_tables: HashSet::new(),
            imported_tables: HashSet::new(),
            schema_tables: HashMap::new(),
            default_catalog: None,
            default_schema: None,
            search_path: Vec::new(),
            case_sensitivity: CaseSensitivity::Dialect,
            dialect,
            allow_implied: true,
            identifier_cache: RefCell::new(HashMap::new()),
        };

        let issues = registry.initialize_from_metadata(schema);
        (registry, issues)
    }

    /// Initializes the registry from schema metadata.
    fn initialize_from_metadata(&mut self, schema: Option<&SchemaMetadata>) -> Vec<Issue> {
        let issues = Vec::new();

        if let Some(schema) = schema {
            self.case_sensitivity = schema.case_sensitivity.unwrap_or(CaseSensitivity::Dialect);
            self.allow_implied = schema.allow_implied;

            self.default_catalog = schema
                .default_catalog
                .as_ref()
                .map(|c| self.normalize_identifier(c));
            self.default_schema = schema
                .default_schema
                .as_ref()
                .map(|s| self.normalize_identifier(s));

            if let Some(search_path) = schema.search_path.as_ref() {
                self.search_path = search_path
                    .iter()
                    .map(|hint| SearchPathEntry {
                        catalog: hint.catalog.as_ref().map(|c| self.normalize_identifier(c)),
                        schema: self.normalize_identifier(&hint.schema),
                    })
                    .collect();
            } else if let Some(default_schema) = &self.default_schema {
                self.search_path = vec![SearchPathEntry {
                    catalog: self.default_catalog.clone(),
                    schema: default_schema.clone(),
                }];
            }

            for table in &schema.tables {
                let canonical = self.schema_table_key(table);
                self.known_tables.insert(canonical.clone());
                self.imported_tables.insert(canonical.clone());
                self.schema_tables.insert(
                    canonical,
                    SchemaTableEntry {
                        table: table.clone(),
                        origin: SchemaOrigin::Imported,
                        source_statement_idx: None,
                        updated_at: Utc::now(),
                        temporary: false,
                        constraints: Vec::new(),
                    },
                );
            }
        }

        issues
    }

    /// Checks if implied schema capture is allowed.
    pub(crate) fn allow_implied(&self) -> bool {
        self.allow_implied
    }

    /// Gets a schema table entry by canonical name.
    pub(crate) fn get(&self, canonical: &str) -> Option<&SchemaTableEntry> {
        self.schema_tables.get(canonical)
    }

    /// Checks if a table is known (in schema or discovered).
    pub(crate) fn is_known(&self, canonical: &str) -> bool {
        self.known_tables.contains(canonical)
    }

    /// Checks if a table was imported (user-provided).
    #[cfg(test)]
    pub(crate) fn is_imported(&self, canonical: &str) -> bool {
        self.imported_tables.contains(canonical)
    }

    /// Removes an implied schema entry (for DROP statements).
    pub(crate) fn remove_implied(&mut self, canonical: &str) {
        if !self.imported_tables.contains(canonical) {
            self.schema_tables.remove(canonical);
            self.known_tables.remove(canonical);
            self.forward_declared_tables.remove(canonical);
        }
    }

    /// Internal helper for registering implied schema.
    ///
    /// Consolidates the logic for both `register_implied*` and `seed_implied_schema*` methods.
    fn register_implied_internal(&mut self, params: RegisterImpliedParams<'_>) -> Option<Issue> {
        let RegisterImpliedParams {
            canonical,
            columns,
            constraints,
            is_temporary,
            statement_type,
            statement_index,
            emit_warnings,
            is_seed,
        } = params;

        self.known_tables.insert(canonical.to_string());

        if is_seed {
            self.forward_declared_tables.insert(canonical.to_string());
        } else {
            self.forward_declared_tables.remove(canonical);
        }

        // Check for conflict with imported schema (only emit warning if requested).
        if self.imported_tables.contains(canonical) {
            if emit_warnings {
                if let Some(imported_entry) = self.schema_tables.get(canonical) {
                    let imported_cols: HashSet<_> = imported_entry
                        .table
                        .columns
                        .iter()
                        .map(|c| &c.name)
                        .collect();
                    let ddl_cols: HashSet<_> = columns.iter().map(|c| &c.name).collect();

                    if imported_cols != ddl_cols {
                        return Some(
                            Issue::warning(
                                issue_codes::SCHEMA_CONFLICT,
                                format!(
                                    "{} for '{}' conflicts with imported schema. Using imported schema (imported has {} columns, {} has {} columns)",
                                    statement_type,
                                    canonical,
                                    imported_cols.len(),
                                    statement_type,
                                    ddl_cols.len()
                                ),
                            )
                            .with_statement(statement_index),
                        );
                    }
                }
            }
            // Don't overwrite imported schema.
            return None;
        }

        // If implied capture is disabled or there are no columns, avoid persisting schema.
        if !self.allow_implied || columns.is_empty() {
            return None;
        }

        // Parse canonical name into parts.
        let parts = split_qualified_identifiers(canonical);
        let (catalog, schema, table_name) = match parts.as_slice() {
            [catalog_part, schema_part, table] => (
                Some(catalog_part.clone()),
                Some(schema_part.clone()),
                table.clone(),
            ),
            [schema_part, table] => (None, Some(schema_part.clone()), table.clone()),
            [table] => (None, None, table.clone()),
            _ => (None, None, extract_simple_name(canonical)),
        };

        self.schema_tables.insert(
            canonical.to_string(),
            SchemaTableEntry {
                table: SchemaTable {
                    catalog,
                    schema,
                    name: table_name,
                    columns,
                },
                origin: SchemaOrigin::Implied,
                source_statement_idx: Some(statement_index),
                updated_at: Utc::now(),
                temporary: is_temporary,
                constraints,
            },
        );

        None
    }

    /// Registers implied schema from DDL statements.
    ///
    /// Returns an optional warning issue if there's a conflict with imported schema.
    pub(crate) fn register_implied(
        &mut self,
        canonical: &str,
        columns: Vec<ColumnSchema>,
        is_temporary: bool,
        statement_type: &str,
        statement_index: usize,
    ) -> Option<Issue> {
        self.register_implied_internal(RegisterImpliedParams {
            canonical,
            columns,
            constraints: Vec::new(),
            is_temporary,
            statement_type,
            statement_index,
            emit_warnings: true,
            is_seed: false,
        })
    }

    /// Registers implied schema from DDL statements with constraint information.
    ///
    /// Returns an optional warning issue if there's a conflict with imported schema.
    pub(crate) fn register_implied_with_constraints(
        &mut self,
        canonical: &str,
        columns: Vec<ColumnSchema>,
        constraints: Vec<TableConstraintInfo>,
        is_temporary: bool,
        statement_type: &str,
        statement_index: usize,
    ) -> Option<Issue> {
        self.register_implied_internal(RegisterImpliedParams {
            canonical,
            columns,
            constraints,
            is_temporary,
            statement_type,
            statement_index,
            emit_warnings: true,
            is_seed: false,
        })
    }

    /// Marks a table as known without persisting schema information.
    ///
    /// Used during pre-analysis passes to avoid `UNRESOLVED_REFERENCE`
    /// warnings for forward-declared tables/views.
    pub(crate) fn mark_table_known(&mut self, canonical: &str) {
        self.known_tables.insert(canonical.to_string());
        self.forward_declared_tables.insert(canonical.to_string());
    }

    /// Seeds implied schema metadata with constraints, without emitting conflict warnings.
    ///
    /// This is used for forward declarations so earlier statements can see
    /// column layouts from later DDL statements, including constraint information.
    pub(crate) fn seed_implied_schema_with_constraints(
        &mut self,
        canonical: &str,
        columns: Vec<ColumnSchema>,
        constraints: Vec<TableConstraintInfo>,
        is_temporary: bool,
        statement_index: usize,
    ) {
        // Ignore the return value since we don't emit warnings during seeding.
        let _ = self.register_implied_internal(RegisterImpliedParams {
            canonical,
            columns,
            constraints,
            is_temporary,
            statement_type: "seed",
            statement_index,
            emit_warnings: false,
            is_seed: true,
        });
    }

    /// Generates a canonical key for a schema table.
    pub(crate) fn schema_table_key(&self, table: &SchemaTable) -> String {
        let mut parts = Vec::new();
        if let Some(catalog) = &table.catalog {
            parts.push(catalog.clone());
        }
        if let Some(schema) = &table.schema {
            parts.push(schema.clone());
        }
        parts.push(table.name.clone());
        self.normalize_table_name(&parts.join("."))
    }

    /// Canonicalizes a table reference using search path and defaults.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self), fields(input = name)))]
    pub(crate) fn canonicalize_table_reference(&self, name: &str) -> TableResolution {
        let parts = split_qualified_identifiers(name);
        if parts.is_empty() {
            return TableResolution {
                canonical: String::new(),
                matched_schema: false,
            };
        }

        let normalized: Vec<String> = parts
            .into_iter()
            .map(|part| self.normalize_identifier(&part))
            .collect();

        match normalized.len() {
            len if len >= 3 => {
                let canonical = normalized.join(".");
                let matched = self.known_tables.contains(&canonical);
                TableResolution {
                    canonical,
                    matched_schema: matched,
                }
            }
            2 => {
                let canonical = normalized.join(".");
                if self.known_tables.contains(&canonical) {
                    return TableResolution {
                        canonical,
                        matched_schema: true,
                    };
                }
                if let Some(default_catalog) = &self.default_catalog {
                    let with_catalog = format!("{default_catalog}.{canonical}");
                    if self.known_tables.contains(&with_catalog) {
                        return TableResolution {
                            canonical: with_catalog,
                            matched_schema: true,
                        };
                    }
                }
                TableResolution {
                    canonical,
                    matched_schema: false,
                }
            }
            _ => {
                let table_only = normalized[0].clone();

                if self.known_tables.contains(&table_only) {
                    return TableResolution {
                        canonical: table_only,
                        matched_schema: true,
                    };
                }

                if let Some(candidate) = self.resolve_via_search_path(&table_only) {
                    return TableResolution {
                        canonical: candidate,
                        matched_schema: true,
                    };
                }

                if let Some(schema) = &self.default_schema {
                    let canonical = if let Some(catalog) = &self.default_catalog {
                        format!("{catalog}.{schema}.{table_only}")
                    } else {
                        format!("{schema}.{table_only}")
                    };
                    let matched = self.known_tables.contains(&canonical);
                    return TableResolution {
                        canonical,
                        matched_schema: matched,
                    };
                }

                TableResolution {
                    canonical: table_only.clone(),
                    matched_schema: self.known_tables.contains(&table_only),
                }
            }
        }
    }

    /// Resolves a table name via the search path.
    pub(crate) fn resolve_via_search_path(&self, table: &str) -> Option<String> {
        for entry in &self.search_path {
            let canonical = match (&entry.catalog, &entry.schema) {
                (Some(catalog), schema) => format!("{catalog}.{schema}.{table}"),
                (None, schema) => format!("{schema}.{table}"),
            };

            if self.known_tables.contains(&canonical) {
                return Some(canonical);
            }
        }
        None
    }

    /// Normalizes an identifier according to dialect case sensitivity rules.
    ///
    /// Results are cached to avoid repeated string operations for the same identifiers.
    pub(crate) fn normalize_identifier(&self, name: &str) -> String {
        // Check cache first
        {
            let cache = self.identifier_cache.borrow();
            if let Some(cached) = cache.get(name) {
                return cached.clone();
            }
        }

        let strategy = self.case_sensitivity.resolve(self.dialect);

        let normalized = if is_quoted_identifier(name) {
            unquote_identifier(name)
        } else {
            strategy.apply(name).into_owned()
        };

        // Store in cache
        self.identifier_cache
            .borrow_mut()
            .insert(name.to_string(), normalized.clone());

        normalized
    }

    /// Normalizes a qualified table name according to dialect case sensitivity rules.
    pub(crate) fn normalize_table_name(&self, name: &str) -> String {
        let strategy = self.case_sensitivity.resolve(self.dialect);

        let parts = split_qualified_identifiers(name);
        if parts.is_empty() {
            return String::new();
        }

        let normalized: Vec<String> = parts
            .into_iter()
            .map(|part| {
                if is_quoted_identifier(&part) {
                    unquote_identifier(&part)
                } else {
                    strategy.apply(&part).into_owned()
                }
            })
            .collect();

        normalized.join(".")
    }

    /// Validates that a column exists in the schema for a given table.
    ///
    /// Returns an optional warning issue if the column is not found.
    ///
    /// # Parameters
    ///
    /// - `canonical`: The canonical table name to validate against
    /// - `column`: The column name to check
    /// - `statement_index`: The statement index for issue reporting
    pub(crate) fn validate_column(
        &self,
        canonical: &str,
        column: &str,
        statement_index: usize,
    ) -> Option<Issue> {
        if let Some(entry) = self.schema_tables.get(canonical) {
            let normalized_col = self.normalize_identifier(column);
            let column_exists = entry
                .table
                .columns
                .iter()
                .any(|c| self.normalize_identifier(&c.name) == normalized_col);

            if !column_exists {
                return Some(
                    Issue::warning(
                        issue_codes::UNKNOWN_COLUMN,
                        format!(
                            "Column '{}' not found in table '{}'. Available columns: {}",
                            column,
                            canonical,
                            entry
                                .table
                                .columns
                                .iter()
                                .map(|c| c.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    )
                    .with_statement(statement_index),
                );
            }
        }
        None
    }

    /// Gets all schema table entries for building resolved schema output.
    pub(crate) fn all_entries(&self) -> impl Iterator<Item = &SchemaTableEntry> {
        self.schema_tables.values()
    }

    /// Checks if the registry has no column metadata.
    pub(crate) fn is_empty(&self) -> bool {
        self.schema_tables.is_empty()
    }

    /// Checks if no tables are known at all (neither imported nor discovered).
    ///
    /// This is used to determine whether to suppress unresolved reference warnings.
    /// When no tables are known, we assume the caller didn't provide schema metadata
    /// and we should be permissive. Once any table is known, we warn about unknowns.
    ///
    /// # Forward-declared tables
    ///
    /// Tables discovered during the DDL pre-pass (forward declarations) are treated
    /// specially: if the *only* known tables are forward-declared ones, we still
    /// consider the registry as having "no known tables" for warning purposes.
    /// This prevents false `UNRESOLVED_REFERENCE` warnings for tables that exist
    /// in the database but aren't created within the script itself.
    ///
    /// For example, given a script like:
    /// ```sql
    /// SELECT * FROM external_table;  -- exists in DB, not in script
    /// CREATE TABLE my_table AS SELECT 1;
    /// ```
    /// We want to suppress warnings about `external_table` because the user
    /// hasn't provided any external schema metadata - `my_table` only appears
    /// as a forward declaration from the DDL pre-pass.
    pub(crate) fn has_no_known_tables(&self) -> bool {
        if !self.imported_tables.is_empty() {
            return false;
        }
        // If all known tables are forward-declared (discovered during DDL pre-pass),
        // treat it as if no external schema was provided.
        self.known_tables
            .iter()
            .all(|name| self.forward_declared_tables.contains(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SchemaNamespaceHint;

    #[test]
    fn test_normalize_identifier_lowercase() {
        let (registry, _) = SchemaRegistry::new(None, Dialect::Postgres);
        assert_eq!(registry.normalize_identifier("MyTable"), "mytable");
    }

    #[test]
    fn test_normalize_identifier_quoted() {
        let (registry, _) = SchemaRegistry::new(None, Dialect::Postgres);
        assert_eq!(registry.normalize_identifier("\"MyTable\""), "MyTable");
    }

    #[test]
    fn test_normalize_identifier_uppercase_dialect() {
        let schema = SchemaMetadata {
            tables: vec![],
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: Some(CaseSensitivity::Upper),
            allow_implied: true,
        };
        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Snowflake);
        assert_eq!(registry.normalize_identifier("MyTable"), "MYTABLE");
    }

    #[test]
    fn test_normalize_identifier_exact() {
        let schema = SchemaMetadata {
            tables: vec![],
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: Some(CaseSensitivity::Exact),
            allow_implied: true,
        };
        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        assert_eq!(registry.normalize_identifier("MyTable"), "MyTable");
    }

    #[test]
    fn test_canonicalize_simple_name() {
        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![],
            }],
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        let resolution = registry.canonicalize_table_reference("users");
        assert_eq!(resolution.canonical, "public.users");
        assert!(resolution.matched_schema);
    }

    #[test]
    fn test_canonicalize_qualified_name() {
        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("analytics".to_string()),
                name: "events".to_string(),
                columns: vec![],
            }],
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        let resolution = registry.canonicalize_table_reference("analytics.events");
        assert_eq!(resolution.canonical, "analytics.events");
        assert!(resolution.matched_schema);
    }

    #[test]
    fn test_canonicalize_with_search_path() {
        let schema = SchemaMetadata {
            tables: vec![
                SchemaTable {
                    catalog: None,
                    schema: Some("staging".to_string()),
                    name: "users".to_string(),
                    columns: vec![],
                },
                SchemaTable {
                    catalog: None,
                    schema: Some("public".to_string()),
                    name: "orders".to_string(),
                    columns: vec![],
                },
            ],
            default_catalog: None,
            default_schema: None,
            search_path: Some(vec![
                SchemaNamespaceHint {
                    catalog: None,
                    schema: "staging".to_string(),
                },
                SchemaNamespaceHint {
                    catalog: None,
                    schema: "public".to_string(),
                },
            ]),
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);

        // "users" should resolve to staging.users (first in search path)
        let resolution = registry.canonicalize_table_reference("users");
        assert_eq!(resolution.canonical, "staging.users");
        assert!(resolution.matched_schema);

        // "orders" should resolve to public.orders
        let resolution = registry.canonicalize_table_reference("orders");
        assert_eq!(resolution.canonical, "public.orders");
        assert!(resolution.matched_schema);
    }

    #[test]
    fn test_canonicalize_unknown_table() {
        let (registry, _) = SchemaRegistry::new(None, Dialect::Postgres);
        let resolution = registry.canonicalize_table_reference("unknown_table");
        assert_eq!(resolution.canonical, "unknown_table");
        assert!(!resolution.matched_schema);
    }

    #[test]
    fn test_register_implied_schema() {
        let (mut registry, _) = SchemaRegistry::new(None, Dialect::Postgres);

        let columns = vec![
            ColumnSchema {
                name: "id".to_string(),
                data_type: Some("integer".to_string()),
                is_primary_key: None,
                foreign_key: None,
                classifications: None,
            },
            ColumnSchema {
                name: "name".to_string(),
                data_type: Some("text".to_string()),
                is_primary_key: None,
                foreign_key: None,
                classifications: None,
            },
        ];

        let issue = registry.register_implied("public.users", columns, false, "CREATE TABLE", 0);
        assert!(issue.is_none());
        assert!(registry.is_known("public.users"));
        assert!(!registry.is_imported("public.users"));

        let entry = registry.get("public.users").unwrap();
        assert_eq!(entry.table.columns.len(), 2);
        assert_eq!(entry.origin, SchemaOrigin::Implied);
    }

    #[test]
    fn test_register_implied_conflict_with_imported() {
        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![ColumnSchema {
                    name: "id".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                    classifications: None,
                }],
            }],
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (mut registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);

        // Try to register implied schema with different columns
        let columns = vec![
            ColumnSchema {
                name: "id".to_string(),
                data_type: Some("integer".to_string()),
                is_primary_key: None,
                foreign_key: None,
                classifications: None,
            },
            ColumnSchema {
                name: "email".to_string(),
                data_type: Some("text".to_string()),
                is_primary_key: None,
                foreign_key: None,
                classifications: None,
            },
        ];

        let issue = registry.register_implied("public.users", columns, false, "CREATE TABLE", 0);
        assert!(issue.is_some());
        let issue = issue.unwrap();
        assert!(issue.message.contains("conflicts with imported schema"));
    }

    #[test]
    fn test_remove_implied_schema() {
        let (mut registry, _) = SchemaRegistry::new(None, Dialect::Postgres);

        let columns = vec![ColumnSchema {
            name: "id".to_string(),
            data_type: Some("integer".to_string()),
            is_primary_key: None,
            foreign_key: None,
            classifications: None,
        }];

        registry.register_implied("public.temp", columns, false, "CREATE TABLE", 0);
        assert!(registry.is_known("public.temp"));

        registry.remove_implied("public.temp");
        assert!(!registry.is_known("public.temp"));
    }

    #[test]
    fn test_remove_does_not_affect_imported() {
        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![],
            }],
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (mut registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        assert!(registry.is_known("public.users"));

        registry.remove_implied("public.users");
        // Imported tables should not be removed
        assert!(registry.is_known("public.users"));
    }

    #[test]
    fn test_validate_column_exists() {
        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![
                    ColumnSchema {
                        name: "id".to_string(),
                        data_type: Some("integer".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                        classifications: None,
                    },
                    ColumnSchema {
                        name: "email".to_string(),
                        data_type: Some("text".to_string()),
                        is_primary_key: None,
                        foreign_key: None,
                        classifications: None,
                    },
                ],
            }],
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);

        // Valid column should return None (no issue)
        let issue = registry.validate_column("public.users", "id", 0);
        assert!(issue.is_none());

        // Invalid column should return a warning
        let issue = registry.validate_column("public.users", "nonexistent", 0);
        assert!(issue.is_some());
        let issue = issue.unwrap();
        assert!(issue.message.contains("not found in table"));
    }

    #[test]
    fn test_validate_column_case_insensitive() {
        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![ColumnSchema {
                    name: "UserName".to_string(),
                    data_type: Some("text".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                    classifications: None,
                }],
            }],
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);

        // Should match case-insensitively
        let issue = registry.validate_column("public.users", "username", 0);
        assert!(issue.is_none());
    }

    #[test]
    fn test_normalize_table_name_qualified() {
        let (registry, _) = SchemaRegistry::new(None, Dialect::Postgres);
        assert_eq!(
            registry.normalize_table_name("Schema.TableName"),
            "schema.tablename"
        );
        assert_eq!(
            registry.normalize_table_name("Catalog.Schema.Table"),
            "catalog.schema.table"
        );
    }

    #[test]
    fn test_allow_implied_disabled() {
        let schema = SchemaMetadata {
            tables: vec![],
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: false,
        };

        let (mut registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        assert!(!registry.allow_implied());

        let columns = vec![ColumnSchema {
            name: "id".to_string(),
            data_type: Some("integer".to_string()),
            is_primary_key: None,
            foreign_key: None,
            classifications: None,
        }];

        // Should still mark as known but not store schema details
        registry.register_implied("public.users", columns, false, "CREATE TABLE", 0);
        assert!(registry.is_known("public.users"));
        assert!(registry.get("public.users").is_none());
    }

    #[test]
    fn test_empty_table_reference() {
        let (registry, _) = SchemaRegistry::new(None, Dialect::Postgres);
        let resolution = registry.canonicalize_table_reference("");
        assert_eq!(resolution.canonical, "");
        assert!(!resolution.matched_schema);
    }

    #[test]
    fn test_canonicalize_three_part_name() {
        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: Some("mydb".to_string()),
                schema: Some("myschema".to_string()),
                name: "mytable".to_string(),
                columns: vec![],
            }],
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        let resolution = registry.canonicalize_table_reference("mydb.myschema.mytable");
        assert_eq!(resolution.canonical, "mydb.myschema.mytable");
        assert!(resolution.matched_schema);
    }

    #[test]
    fn test_canonicalize_two_part_with_default_catalog() {
        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: Some("defaultdb".to_string()),
                schema: Some("myschema".to_string()),
                name: "mytable".to_string(),
                columns: vec![],
            }],
            default_catalog: Some("defaultdb".to_string()),
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        // Two-part name should try with default catalog
        let resolution = registry.canonicalize_table_reference("myschema.mytable");
        assert_eq!(resolution.canonical, "defaultdb.myschema.mytable");
        assert!(resolution.matched_schema);
    }

    #[test]
    fn test_search_path_priority() {
        // Same table name exists in multiple schemas
        let schema = SchemaMetadata {
            tables: vec![
                SchemaTable {
                    catalog: None,
                    schema: Some("schema_a".to_string()),
                    name: "users".to_string(),
                    columns: vec![ColumnSchema {
                        name: "a_col".to_string(),
                        data_type: None,
                        is_primary_key: None,
                        foreign_key: None,
                        classifications: None,
                    }],
                },
                SchemaTable {
                    catalog: None,
                    schema: Some("schema_b".to_string()),
                    name: "users".to_string(),
                    columns: vec![ColumnSchema {
                        name: "b_col".to_string(),
                        data_type: None,
                        is_primary_key: None,
                        foreign_key: None,
                        classifications: None,
                    }],
                },
            ],
            default_catalog: None,
            default_schema: None,
            search_path: Some(vec![
                SchemaNamespaceHint {
                    catalog: None,
                    schema: "schema_b".to_string(),
                },
                SchemaNamespaceHint {
                    catalog: None,
                    schema: "schema_a".to_string(),
                },
            ]),
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        // Should resolve to schema_b.users (first in search path)
        let resolution = registry.canonicalize_table_reference("users");
        assert_eq!(resolution.canonical, "schema_b.users");
        assert!(resolution.matched_schema);
    }

    #[test]
    fn test_register_implied_with_empty_columns() {
        let (mut registry, _) = SchemaRegistry::new(None, Dialect::Postgres);

        // Empty columns should mark as known but not store schema
        let issue =
            registry.register_implied("public.empty_table", vec![], false, "CREATE TABLE", 0);
        assert!(issue.is_none());
        assert!(registry.is_known("public.empty_table"));
        assert!(registry.get("public.empty_table").is_none());
    }

    #[test]
    fn test_validate_column_unknown_table() {
        let (registry, _) = SchemaRegistry::new(None, Dialect::Postgres);

        // Validating column on unknown table should return None (no issue)
        // because we don't have schema to validate against
        let issue = registry.validate_column("unknown.table", "any_column", 0);
        assert!(issue.is_none());
    }

    #[test]
    fn test_has_no_known_tables_initially() {
        let (registry, _) = SchemaRegistry::new(None, Dialect::Postgres);
        assert!(registry.has_no_known_tables());
    }

    #[test]
    fn test_has_known_tables_after_import() {
        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![],
            }],
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        assert!(!registry.has_no_known_tables());
    }

    #[test]
    fn test_all_entries_iteration() {
        let schema = SchemaMetadata {
            tables: vec![
                SchemaTable {
                    catalog: None,
                    schema: Some("public".to_string()),
                    name: "users".to_string(),
                    columns: vec![],
                },
                SchemaTable {
                    catalog: None,
                    schema: Some("public".to_string()),
                    name: "orders".to_string(),
                    columns: vec![],
                },
            ],
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        let entries: Vec<_> = registry.all_entries().collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_is_empty() {
        let (registry, _) = SchemaRegistry::new(None, Dialect::Postgres);
        assert!(registry.is_empty());

        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: None,
                name: "test".to_string(),
                columns: vec![],
            }],
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry_with_tables, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        assert!(!registry_with_tables.is_empty());
    }

    #[test]
    fn test_register_implied_identical_to_imported() {
        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![ColumnSchema {
                    name: "id".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: None,
                    classifications: None,
                }],
            }],
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (mut registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);

        // Registering implied with same columns should NOT produce a warning
        let columns = vec![ColumnSchema {
            name: "id".to_string(),
            data_type: Some("integer".to_string()),
            is_primary_key: None,
            foreign_key: None,
            classifications: None,
        }];

        let issue = registry.register_implied("public.users", columns, false, "CREATE TABLE", 0);
        assert!(issue.is_none());
    }

    #[test]
    fn test_snowflake_uppercase_normalization() {
        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("PUBLIC".to_string()),
                name: "USERS".to_string(),
                columns: vec![],
            }],
            default_catalog: None,
            default_schema: None,
            search_path: None,
            case_sensitivity: Some(CaseSensitivity::Upper),
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Snowflake);

        // Lowercase input should be normalized to uppercase
        let resolution = registry.canonicalize_table_reference("public.users");
        assert_eq!(resolution.canonical, "PUBLIC.USERS");
        assert!(resolution.matched_schema);
    }

    #[test]
    fn test_column_with_primary_key() {
        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![ColumnSchema {
                    name: "id".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: Some(true),
                    foreign_key: None,
                    classifications: None,
                }],
            }],
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        let entry = registry.get("public.users").unwrap();
        assert_eq!(entry.table.columns[0].is_primary_key, Some(true));
    }

    #[test]
    fn test_column_with_foreign_key() {
        use crate::types::ForeignKeyRef;

        let schema = SchemaMetadata {
            tables: vec![SchemaTable {
                catalog: None,
                schema: Some("public".to_string()),
                name: "orders".to_string(),
                columns: vec![ColumnSchema {
                    name: "user_id".to_string(),
                    data_type: Some("integer".to_string()),
                    is_primary_key: None,
                    foreign_key: Some(ForeignKeyRef {
                        table: "public.users".to_string(),
                        column: "id".to_string(),
                    }),
                    classifications: None,
                }],
            }],
            default_catalog: None,
            default_schema: Some("public".to_string()),
            search_path: None,
            case_sensitivity: None,
            allow_implied: true,
        };

        let (registry, _) = SchemaRegistry::new(Some(&schema), Dialect::Postgres);
        let entry = registry.get("public.orders").unwrap();
        let fk = entry.table.columns[0].foreign_key.as_ref().unwrap();
        assert_eq!(fk.table, "public.users");
        assert_eq!(fk.column, "id");
    }

    #[test]
    fn test_implied_schema_with_constraints() {
        use crate::types::{ConstraintType, ForeignKeyRef};

        let (mut registry, _) = SchemaRegistry::new(None, Dialect::Postgres);

        let columns = vec![
            ColumnSchema {
                name: "id".to_string(),
                data_type: Some("integer".to_string()),
                is_primary_key: Some(true),
                foreign_key: None,
                classifications: None,
            },
            ColumnSchema {
                name: "order_id".to_string(),
                data_type: Some("integer".to_string()),
                is_primary_key: None,
                foreign_key: Some(ForeignKeyRef {
                    table: "orders".to_string(),
                    column: "id".to_string(),
                }),
                classifications: None,
            },
        ];

        let constraints = vec![TableConstraintInfo {
            constraint_type: ConstraintType::ForeignKey,
            columns: vec!["order_id".to_string()],
            referenced_table: Some("orders".to_string()),
            referenced_columns: Some(vec!["id".to_string()]),
        }];

        registry.register_implied_with_constraints(
            "public.order_items",
            columns,
            constraints,
            false,
            "CREATE TABLE",
            0,
        );

        let entry = registry.get("public.order_items").unwrap();
        assert_eq!(entry.table.columns.len(), 2);
        assert_eq!(entry.table.columns[0].is_primary_key, Some(true));
        assert!(entry.table.columns[1].foreign_key.is_some());
        assert_eq!(entry.constraints.len(), 1);
        assert_eq!(
            entry.constraints[0].constraint_type,
            ConstraintType::ForeignKey
        );
    }
}
