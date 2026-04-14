use std::collections::{HashMap, HashSet};

use flowscope_core::{Edge, EdgeType, Node, NodeType};

/// Return the IDs of edges in the flat graph that should be exported as join rows.
///
/// Join metadata may now appear on column-level lineage edges as well as relation-level
/// edges. To keep exported join views stable, collapse those tagged edges down to one
/// representative row per logical relation pair and join metadata tuple.
pub(crate) fn representative_join_edge_ids<'a>(
    nodes: &'a [Node],
    edges: &'a [Edge],
) -> HashSet<&'a str> {
    let node_types: HashMap<&str, NodeType> = nodes
        .iter()
        .map(|node| (node.id.as_ref(), node.node_type))
        .collect();

    let owned_node_to_relation: HashMap<&str, &str> = edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::Ownership)
        .filter_map(|edge| {
            let owner_type = node_types.get(edge.from.as_ref()).copied()?;
            owner_type
                .is_relation()
                .then_some((edge.to.as_ref(), edge.from.as_ref()))
        })
        .collect();

    let mut seen = HashSet::new();
    let mut representatives: HashSet<&str> = HashSet::new();

    for edge in edges {
        let Some(join_type) = edge.join_type else {
            continue;
        };

        let Some(from_relation_id) =
            resolve_relation_node_id(edge.from.as_ref(), &node_types, &owned_node_to_relation)
        else {
            continue;
        };
        let Some(to_relation_id) =
            resolve_relation_node_id(edge.to.as_ref(), &node_types, &owned_node_to_relation)
        else {
            continue;
        };

        if from_relation_id == to_relation_id {
            continue;
        }

        // Include the producing statement(s) in the dedup key so the same
        // logical join appearing in two different statements is exported as
        // two rows (matching what users expect when inspecting per-statement
        // joins).
        let statements_key: Vec<usize> = edge.statement_ids.clone();

        let key = (
            from_relation_id.to_owned(),
            to_relation_id.to_owned(),
            format!("{join_type:?}"),
            edge.join_condition
                .as_deref()
                .unwrap_or_default()
                .to_owned(),
            statements_key,
        );

        if seen.insert(key) {
            representatives.insert(edge.id.as_ref());
        }
    }

    representatives
}

fn resolve_relation_node_id<'a>(
    node_id: &'a str,
    node_types: &HashMap<&'a str, NodeType>,
    owned_node_to_relation: &HashMap<&'a str, &'a str>,
) -> Option<&'a str> {
    match node_types.get(node_id).copied() {
        Some(node_type) if node_type.is_relation() => Some(node_id),
        _ => owned_node_to_relation.get(node_id).copied(),
    }
}
