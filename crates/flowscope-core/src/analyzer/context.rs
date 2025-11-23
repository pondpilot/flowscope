use crate::types::{Edge, Node};
use std::collections::{HashMap, HashSet};

/// Context for analyzing a single statement
pub(crate) struct StatementContext {
    pub(crate) statement_index: usize,
    pub(crate) nodes: Vec<Node>,
    pub(crate) edges: Vec<Edge>,
    pub(crate) node_ids: HashSet<String>,
    pub(crate) edge_ids: HashSet<String>,
    /// CTE name -> node ID
    pub(crate) cte_definitions: HashMap<String, String>,
    /// Alias -> canonical table name
    pub(crate) table_aliases: HashMap<String, String>,
    /// Subquery aliases (for reference tracking)
    pub(crate) subquery_aliases: HashSet<String>,
    /// Last join/operation type for edge labeling
    pub(crate) last_operation: Option<String>,
    /// Table canonical name -> node ID (for column ownership)
    pub(crate) table_node_ids: HashMap<String, String>,
    /// Output columns for this statement (for column lineage)
    pub(crate) output_columns: Vec<OutputColumn>,
    /// CTE columns: CTE name -> list of output columns
    pub(crate) cte_columns: HashMap<String, Vec<OutputColumn>>,
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
    /// Node ID for this column
    pub(crate) node_id: String,
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
            table_node_ids: HashMap::new(),
            output_columns: Vec::new(),
            cte_columns: HashMap::new(),
        }
    }

    pub(crate) fn add_node(&mut self, node: Node) -> String {
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
}
