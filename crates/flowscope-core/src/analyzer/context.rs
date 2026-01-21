use super::helpers::generate_output_node_id;
use crate::types::{Edge, FilterClauseType, FilterPredicate, JoinType, Node, NodeType};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Tracks a SELECT * that couldn't be expanded due to missing schema.
///
/// When a wildcard is encountered without schema metadata to resolve it,
/// we record the source and target so that downstream column references
/// can be used to infer what columns must have flowed through.
#[derive(Debug, Clone)]
pub(crate) struct PendingWildcard {
    /// The source canonical name (table or CTE) with unknown columns
    pub(crate) source_canonical: String,
    /// The target entity (CTE name or derived table alias) receiving the wildcard
    pub(crate) target_name: String,
    /// Node ID of the source
    pub(crate) source_node_id: Arc<str>,
}

/// Represents a single scope level for column resolution.
/// Each SELECT/subquery/CTE body gets its own scope.
#[derive(Debug, Clone, Default)]
pub(crate) struct Scope {
    /// Tables directly referenced in this scope's FROM/JOIN clauses
    /// Maps canonical table name -> node ID
    pub(crate) tables: HashMap<String, Arc<str>>,
    /// Aliases defined in this scope (alias -> canonical name)
    pub(crate) aliases: HashMap<String, String>,
    /// Subquery aliases in this scope
    pub(crate) subquery_aliases: HashSet<String>,
}

impl Scope {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

/// Information about the current JOIN being processed.
#[derive(Debug, Clone, Default)]
pub(crate) struct JoinInfo {
    /// The type of join (INNER, LEFT, etc.)
    pub(crate) join_type: Option<JoinType>,
    /// The join condition expression (ON clause text)
    pub(crate) join_condition: Option<String>,
}

/// Context for analyzing a single statement
pub(crate) struct StatementContext {
    pub(crate) statement_index: usize,
    pub(crate) nodes: Vec<Node>,
    pub(crate) edges: Vec<Edge>,
    pub(crate) node_ids: HashSet<Arc<str>>,
    pub(crate) edge_ids: HashSet<Arc<str>>,
    /// CTE name -> node ID
    pub(crate) cte_definitions: HashMap<String, Arc<str>>,
    /// Node ID -> CTE/derived alias name for reverse lookups
    pub(crate) cte_node_to_name: HashMap<Arc<str>, String>,
    /// Cursor for sequential span searching across identifier definitions.
    ///
    /// Used to locate spans for CTEs and derived table aliases by tracking the
    /// current search position in the SQL text. Updated after each successful
    /// span match to ensure subsequent searches find distinct occurrences.
    ///
    /// # Invariants
    /// - Reset to 0 when entering a new statement context
    /// - Assumes AST traversal is roughly left-to-right in lexical order
    pub(crate) span_search_cursor: usize,
    /// Alias -> canonical table name (global, for backwards compatibility)
    pub(crate) table_aliases: HashMap<String, String>,
    /// Subquery aliases (for reference tracking)
    pub(crate) subquery_aliases: HashSet<String>,
    /// Last join/operation type for edge labeling
    pub(crate) last_operation: Option<String>,
    /// Current join information (type + condition) for edge labeling
    pub(crate) current_join_info: JoinInfo,
    /// Table canonical name -> node ID (for column ownership) - global registry
    pub(crate) table_node_ids: HashMap<String, Arc<str>>,
    /// Output columns for this statement (for column lineage)
    pub(crate) output_columns: Vec<OutputColumn>,
    /// Output node ID for SELECT statements
    pub(crate) output_node_id: Option<Arc<str>>,
    /// Output columns for aliased subqueries (CTEs and derived tables).
    /// Maps the alias name to its output columns for schema resolution during wildcard
    /// expansion and column reference lookups.
    pub(crate) aliased_subquery_columns: HashMap<String, Vec<OutputColumn>>,
    /// Stack of scopes for proper column resolution
    /// The top of the stack (last element) is the current scope
    pub(crate) scope_stack: Vec<Scope>,
    /// Pending filter predicates to attach to table nodes
    /// Maps table canonical name -> list of filter predicates
    pub(crate) pending_filters: HashMap<String, Vec<FilterPredicate>>,
    /// Grouping columns for the current SELECT (normalized expression strings)
    /// Used to detect aggregation vs grouping key columns
    pub(crate) grouping_columns: HashSet<String>,
    /// True if the current SELECT has a GROUP BY clause
    pub(crate) has_group_by: bool,
    /// Columns referenced per source table (canonical_name → column_name → data_type).
    /// Used to build implied schema for source tables in SELECT queries.
    pub(crate) source_table_columns: HashMap<String, HashMap<String, Option<String>>>,
    /// Implied foreign key relationships from JOIN conditions.
    /// Key: (from_table, from_column), Value: (to_table, to_column)
    pub(crate) implied_foreign_keys: HashMap<(String, String), (String, String)>,
    /// Pending wildcards that couldn't be expanded due to missing schema.
    /// Used for backward column inference from downstream references.
    pub(crate) pending_wildcards: Vec<PendingWildcard>,
}

/// Represents an output column in the SELECT list
#[derive(Debug, Clone)]
pub(crate) struct OutputColumn {
    /// Alias or derived name for the column
    pub(crate) name: String,
    /// Inferred data type of the column
    pub(crate) data_type: Option<String>,
    /// Node ID for this column
    pub(crate) node_id: Arc<str>,
}

/// A reference to a source column
#[derive(Debug, Clone)]
pub(crate) struct ColumnRef {
    /// Table name or alias
    pub(crate) table: Option<String>,
    /// Column name
    pub(crate) column: String,
}

impl StatementContext {
    pub(crate) fn new(statement_index: usize) -> Self {
        Self {
            statement_index,
            nodes: Vec::new(),
            edges: Vec::new(),
            node_ids: HashSet::new(),
            edge_ids: HashSet::new(),
            cte_definitions: HashMap::new(),
            cte_node_to_name: HashMap::new(),
            span_search_cursor: 0,
            table_aliases: HashMap::new(),
            subquery_aliases: HashSet::new(),
            last_operation: None,
            current_join_info: JoinInfo::default(),
            table_node_ids: HashMap::new(),
            output_columns: Vec::new(),
            output_node_id: None,
            aliased_subquery_columns: HashMap::new(),
            scope_stack: Vec::new(),
            pending_filters: HashMap::new(),
            grouping_columns: HashSet::new(),
            has_group_by: false,
            source_table_columns: HashMap::new(),
            implied_foreign_keys: HashMap::new(),
            pending_wildcards: Vec::new(),
        }
    }

    /// Clear grouping context for a new SELECT
    pub(crate) fn clear_grouping(&mut self) {
        self.grouping_columns.clear();
        self.has_group_by = false;
    }

    /// Add a grouping column expression
    pub(crate) fn add_grouping_column(&mut self, expr: String) {
        self.grouping_columns.insert(expr);
        self.has_group_by = true;
    }

    /// Check if an expression matches a grouping column
    pub(crate) fn is_grouping_column(&self, expr: &str) -> bool {
        self.grouping_columns.contains(expr)
    }

    /// Record a column reference for a source table.
    ///
    /// This is used to build implied schema for source tables. If the column
    /// already exists without a type and a type is provided, the type is updated.
    pub(crate) fn record_source_column(
        &mut self,
        canonical_table: &str,
        column_name: &str,
        data_type: Option<String>,
    ) {
        let columns = self
            .source_table_columns
            .entry(canonical_table.to_string())
            .or_default();

        columns
            .entry(column_name.to_string())
            .and_modify(|existing| {
                // Update type if we have one and don't already have one
                if existing.is_none() && data_type.is_some() {
                    *existing = data_type.clone();
                }
            })
            .or_insert(data_type);
    }

    /// Record an implied foreign key relationship from a JOIN condition.
    ///
    /// When we see `ON t1.a = t2.b`, we record that t1.a references t2.b.
    /// The "from" side is considered the FK column, "to" is the referenced column.
    ///
    /// ## Self-Join Exclusion
    ///
    /// Conditions where `from_table == to_table` are **intentionally excluded**.
    /// While self-referential FKs do exist (e.g., `employees.manager_id → employees.id`
    /// for hierarchical data), detecting them from JOIN conditions alone would produce
    /// too many false positives. For example, `SELECT * FROM t t1 JOIN t t2 ON t1.x = t2.y`
    /// is a common pattern that doesn't imply a self-FK.
    ///
    /// If self-referential FK detection is needed, users should provide explicit schema
    /// via the `schema` field in the request.
    pub(crate) fn record_implied_foreign_key(
        &mut self,
        from_table: &str,
        from_column: &str,
        to_table: &str,
        to_column: &str,
    ) {
        // Skip self-joins (see doc comment for rationale)
        if from_table != to_table {
            self.implied_foreign_keys.insert(
                (from_table.to_string(), from_column.to_string()),
                (to_table.to_string(), to_column.to_string()),
            );
        }
    }

    /// Add a filter predicate for a specific table.
    ///
    /// # Parameters
    ///
    /// - `canonical`: The canonical table name
    /// - `expression`: The filter expression text
    /// - `clause_type`: The type of SQL clause (WHERE, HAVING, etc.)
    pub(crate) fn add_filter_for_table(
        &mut self,
        canonical: &str,
        expression: String,
        clause_type: FilterClauseType,
    ) {
        self.pending_filters
            .entry(canonical.to_string())
            .or_default()
            .push(FilterPredicate {
                expression,
                clause_type,
            });
    }

    pub(crate) fn add_node(&mut self, node: Node) -> Arc<str> {
        let id = node.id.clone();
        if self.node_ids.insert(id.clone()) {
            self.nodes.push(node);
        }
        id
    }

    pub(crate) fn add_edge(&mut self, edge: Edge) {
        let id = edge.id.clone();
        if self.edge_ids.insert(id) {
            self.edges.push(edge);
        }
    }

    pub(crate) fn ensure_output_node(&mut self) -> Arc<str> {
        if let Some(existing) = self.output_node_id.as_ref() {
            return existing.clone();
        }

        let node_id = generate_output_node_id(self.statement_index);
        // Include statement index in label for disambiguation in multi-statement analysis
        let label = if self.statement_index == 0 {
            "Output".to_string()
        } else {
            format!("Output ({})", self.statement_index + 1)
        };
        let output_node = Node {
            id: node_id.clone(),
            node_type: NodeType::Output,
            label: label.into(),
            qualified_name: None,
            expression: None,
            span: None,
            metadata: None,
            resolution_source: None,
            filters: Vec::new(),
            join_type: None,
            join_condition: None,
            aggregation: None,
        };

        self.add_node(output_node);
        self.output_node_id = Some(node_id.clone());
        node_id
    }

    pub(crate) fn output_node_id(&self) -> Option<&Arc<str>> {
        self.output_node_id.as_ref()
    }

    /// Push a new scope onto the stack (entering a SELECT/subquery)
    pub(crate) fn push_scope(&mut self) {
        self.scope_stack.push(Scope::new());
    }

    /// Pop the current scope (leaving a SELECT/subquery)
    pub(crate) fn pop_scope(&mut self) {
        self.scope_stack.pop();
    }

    /// Get the current (topmost) scope, if any
    pub(crate) fn current_scope(&self) -> Option<&Scope> {
        self.scope_stack.last()
    }

    /// Get the current (topmost) scope mutably, if any
    pub(crate) fn current_scope_mut(&mut self) -> Option<&mut Scope> {
        self.scope_stack.last_mut()
    }

    /// Register a table in the current scope
    pub(crate) fn register_table_in_scope(&mut self, canonical: String, node_id: Arc<str>) {
        // Always register in global table_node_ids for node lookups
        self.table_node_ids
            .insert(canonical.clone(), node_id.clone());

        // Also register in current scope for resolution
        if let Some(scope) = self.current_scope_mut() {
            scope.tables.insert(canonical, node_id);
        }
    }

    /// Register an alias in the current scope
    pub(crate) fn register_alias_in_scope(&mut self, alias: String, canonical: String) {
        // Register in global aliases for backwards compatibility
        self.table_aliases.insert(alias.clone(), canonical.clone());

        // Also register in current scope
        if let Some(scope) = self.current_scope_mut() {
            scope.aliases.insert(alias, canonical);
        }
    }

    /// Register a subquery alias in the current scope
    pub(crate) fn register_subquery_alias_in_scope(&mut self, alias: String) {
        // Register globally
        self.subquery_aliases.insert(alias.clone());

        // Also register in current scope
        if let Some(scope) = self.current_scope_mut() {
            scope.subquery_aliases.insert(alias);
        }
    }

    /// Get tables that are in scope for column resolution.
    /// Returns tables from the current scope only.
    pub(crate) fn tables_in_current_scope(&self) -> Vec<String> {
        if let Some(scope) = self.current_scope() {
            scope.tables.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Returns a checkpoint representing the current length of the output column buffer.
    ///
    /// This is part of the **projection checkpoint pattern** used when analyzing nested
    /// queries (CTEs, derived tables). The pattern works as follows:
    ///
    /// 1. Before analyzing a subquery, call `projection_checkpoint()` to record the current
    ///    buffer position
    /// 2. Analyze the subquery, which appends its output columns to the buffer
    /// 3. Call `take_output_columns_since(checkpoint)` to extract only the columns produced
    ///    by that subquery, leaving earlier columns intact
    ///
    /// This ensures that columns from inner queries don't leak into the schema of outer
    /// statements (e.g., a CTE's internal columns shouldn't appear in a CREATE TABLE AS
    /// statement's implied schema).
    pub(crate) fn projection_checkpoint(&self) -> usize {
        self.output_columns.len()
    }

    /// Drains output columns produced since the provided checkpoint.
    ///
    /// See [`projection_checkpoint`](Self::projection_checkpoint) for usage pattern.
    pub(crate) fn take_output_columns_since(&mut self, checkpoint: usize) -> Vec<OutputColumn> {
        if checkpoint > self.output_columns.len() {
            // This indicates a logic error: the checkpoint was taken from a different context
            // or the output_columns were modified unexpectedly.
            debug_assert!(
                false,
                "Invalid projection checkpoint: {} > buffer length {}",
                checkpoint,
                self.output_columns.len()
            );
            return Vec::new();
        }
        if checkpoint == self.output_columns.len() {
            // No new columns were produced - this is valid (e.g., empty subquery)
            return Vec::new();
        }
        self.output_columns.split_off(checkpoint)
    }
}
