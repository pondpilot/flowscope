use crate::types::{Edge, FilterClauseType, FilterPredicate, JoinType, Node};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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
    /// CTE columns: CTE name -> list of output columns
    pub(crate) cte_columns: HashMap<String, Vec<OutputColumn>>,
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
}

/// Represents an output column in the SELECT list
#[derive(Debug, Clone)]
pub(crate) struct OutputColumn {
    /// Alias or derived name for the column
    pub(crate) name: String,
    /// Source columns that contribute to this output
    #[allow(dead_code)]
    pub(crate) sources: Vec<ColumnRef>,
    /// Expression text for computed columns
    #[allow(dead_code)]
    pub(crate) expression: Option<String>,
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
    /// Resolved table canonical name (if known)
    #[allow(dead_code)]
    pub(crate) resolved_table: Option<String>,
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
            table_aliases: HashMap::new(),
            subquery_aliases: HashSet::new(),
            last_operation: None,
            current_join_info: JoinInfo::default(),
            table_node_ids: HashMap::new(),
            output_columns: Vec::new(),
            cte_columns: HashMap::new(),
            scope_stack: Vec::new(),
            pending_filters: HashMap::new(),
            grouping_columns: HashSet::new(),
            has_group_by: false,
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
            // Fallback to global table_node_ids if no scope (shouldn't happen normally)
            self.table_node_ids.keys().cloned().collect()
        }
    }
}
