//! Complexity score calculation for SQL statements.

use crate::types::{JoinType, Node, NodeType};

/// Weights for complexity calculation.
const TABLE_WEIGHT: usize = 5;
const JOIN_WEIGHT: usize = 10;
const COMPLEX_JOIN_WEIGHT: usize = 15; // CROSS, FULL joins
const CTE_WEIGHT: usize = 8;
const FILTER_WEIGHT: usize = 2;

/// Calculate complexity score for a set of nodes.
///
/// Returns a score from 1-100 based on:
/// - Number of tables (5 points each)
/// - Number of joins (10 points each, 15 for CROSS/FULL)
/// - Number of CTEs (8 points each)
/// - Number of filter predicates (2 points each)
pub fn calculate_complexity(nodes: &[Node]) -> u8 {
    let mut table_count = 0;
    let mut cte_count = 0;
    let mut join_count = 0;
    let mut complex_join_count = 0;
    let mut filter_count = 0;

    for node in nodes {
        match node.node_type {
            NodeType::Table => {
                table_count += 1;
                filter_count += node.filters.len();

                if let Some(join_type) = &node.join_type {
                    if is_complex_join(join_type) {
                        complex_join_count += 1;
                    } else {
                        join_count += 1;
                    }
                }
            }
            NodeType::Cte => {
                cte_count += 1;
                filter_count += node.filters.len();

                if let Some(join_type) = &node.join_type {
                    if is_complex_join(join_type) {
                        complex_join_count += 1;
                    } else {
                        join_count += 1;
                    }
                }
            }
            NodeType::Column => {
                // Columns don't directly contribute to complexity
            }
        }
    }

    let raw_score = table_count * TABLE_WEIGHT
        + join_count * JOIN_WEIGHT
        + complex_join_count * COMPLEX_JOIN_WEIGHT
        + cte_count * CTE_WEIGHT
        + filter_count * FILTER_WEIGHT;

    // Normalize to 1-100 range
    raw_score.clamp(1, 100) as u8
}

/// Count the number of JOIN operations in nodes.
pub fn count_joins(nodes: &[Node]) -> usize {
    nodes.iter().filter(|n| n.join_type.is_some()).count()
}

/// Check if a join type is considered "complex" (higher weight).
fn is_complex_join(join_type: &JoinType) -> bool {
    matches!(join_type, JoinType::Cross | JoinType::Full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_table() {
        let nodes = vec![Node::table("t1".into(), "users".into())];
        assert_eq!(calculate_complexity(&nodes), 5);
        assert_eq!(count_joins(&nodes), 0);
    }

    #[test]
    fn test_table_with_join() {
        let nodes = vec![
            Node::table("t1".into(), "users".into()),
            Node::table("t2".into(), "orders".into()).with_join_type(JoinType::Inner),
        ];
        assert_eq!(count_joins(&nodes), 1);
        // 2 tables (10) + 1 join (10) = 20
        assert_eq!(calculate_complexity(&nodes), 20);
    }

    #[test]
    fn test_complex_query() {
        let nodes = vec![
            Node::table("t1".into(), "users".into()),
            Node::table("t2".into(), "orders".into()).with_join_type(JoinType::Left),
            Node::table("t3".into(), "products".into()).with_join_type(JoinType::Left),
            Node::cte("c1".into(), "active_users".into()),
        ];
        // 3 tables (15) + 1 CTE (8) + 2 joins (20) = 43
        assert_eq!(calculate_complexity(&nodes), 43);
        assert_eq!(count_joins(&nodes), 2);
    }

    #[test]
    fn test_cross_join_higher_weight() {
        let nodes = vec![
            Node::table("t1".into(), "users".into()),
            Node::table("t2".into(), "dates".into()).with_join_type(JoinType::Cross),
        ];
        // 2 tables (10) + 1 cross join (15) = 25
        assert_eq!(calculate_complexity(&nodes), 25);
    }

    #[test]
    fn test_caps_at_100() {
        let mut nodes = Vec::new();
        for i in 0..20 {
            nodes.push(Node::table(format!("t{i}"), format!("table{i}")));
        }
        // 20 tables * 5 = 100, should cap at 100
        assert_eq!(calculate_complexity(&nodes), 100);
    }
}
