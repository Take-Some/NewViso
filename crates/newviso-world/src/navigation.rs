use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};

use crate::data::{invalid_float_map, valid_json_payload, valid_label, validate_id};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct WorldNavNodeDesc {
    pub id: String,
    pub position: [f32; 3],
    pub tags: Vec<String>,
    pub parameters: BTreeMap<String, f32>,
}

impl WorldNavNodeDesc {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_id("world navigation node", &self.id)?;
        if self.position.iter().any(|value| !value.is_finite())
            || self.tags.iter().any(|tag| !valid_label(tag))
            || invalid_float_map(&self.parameters)
        {
            return Err("invalid generic WorldNavNodeDesc parameters".to_owned());
        }
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct WorldNavEdgeDesc {
    pub id: String,
    pub from: String,
    pub to: String,
    pub bidirectional: bool,
    pub distance: Option<f32>,
    pub cost_scale: f32,
    pub enabled: bool,
    pub tags: Vec<String>,
    pub parameters: BTreeMap<String, f32>,
}

impl WorldNavEdgeDesc {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_id("world navigation edge", &self.id)?;
        validate_id("world navigation edge from", &self.from)?;
        validate_id("world navigation edge to", &self.to)?;
        if self.from == self.to
            || self
                .distance
                .is_some_and(|value| !value.is_finite() || value <= 0.0 || value > 1.0e9)
            || !self.cost_scale.is_finite()
            || !(0.0001..=1.0e6).contains(&self.cost_scale)
            || self.tags.iter().any(|tag| !valid_label(tag))
            || invalid_float_map(&self.parameters)
        {
            return Err("invalid generic WorldNavEdgeDesc parameters".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldTravelRequestDesc {
    pub actor_id: String,
    pub start_node: Option<String>,
    pub destination_node: String,
    pub speed: f32,
    pub mode: String,
    pub payload: Value,
}

impl WorldTravelRequestDesc {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_id("world travel actor", &self.actor_id)?;
        if let Some(start) = &self.start_node {
            validate_id("world travel start node", start)?;
        }
        validate_id("world travel destination node", &self.destination_node)?;
        if !self.speed.is_finite()
            || !(0.001..=1.0e7).contains(&self.speed)
            || !valid_label(&self.mode)
            || !valid_json_payload(&self.payload)
        {
            return Err("invalid generic WorldTravelRequestDesc parameters".to_owned());
        }
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct WorldTravelRecord {
    pub(crate) actor_id: String,
    pub(crate) route: Vec<String>,
    pub(crate) next_waypoint_index: usize,
    pub(crate) destination_node: String,
    pub(crate) speed: f32,
    pub(crate) mode: String,
    pub(crate) payload: Value,
    pub(crate) started_world_seconds: f64,
    pub(crate) last_update_world_seconds: f64,
    pub(crate) distance_travelled: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct WorldTravelCompletion {
    pub(crate) actor_id: String,
    pub(crate) destination_node: String,
    pub(crate) mode: String,
    pub(crate) payload: Value,
    pub(crate) world_seconds: f64,
    pub(crate) distance_travelled: f64,
}

#[derive(Clone, Copy, Debug)]
struct QueueState {
    cost: f32,
    node_index: usize,
}

impl Eq for QueueState {}

impl PartialEq for QueueState {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.node_index == other.node_index
    }
}

impl Ord for QueueState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.node_index.cmp(&self.node_index))
    }
}

impl PartialOrd for QueueState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(crate) fn nearest_node(
    nodes: &BTreeMap<String, WorldNavNodeDesc>,
    point: [f32; 3],
) -> Option<String> {
    nodes
        .values()
        .min_by(|a, b| {
            distance_squared(a.position, point)
                .total_cmp(&distance_squared(b.position, point))
                .then_with(|| a.id.cmp(&b.id))
        })
        .map(|node| node.id.clone())
}

pub(crate) fn shortest_route(
    nodes: &BTreeMap<String, WorldNavNodeDesc>,
    edges: &BTreeMap<String, WorldNavEdgeDesc>,
    start: &str,
    destination: &str,
) -> Result<Vec<String>, String> {
    if !nodes.contains_key(start) {
        return Err(format!(
            "world navigation start node '{start}' does not exist"
        ));
    }
    if !nodes.contains_key(destination) {
        return Err(format!(
            "world navigation destination node '{destination}' does not exist"
        ));
    }
    if start == destination {
        return Ok(vec![start.to_owned()]);
    }

    let node_ids = nodes.keys().cloned().collect::<Vec<_>>();
    let index_of = node_ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut dist = vec![f32::INFINITY; node_ids.len()];
    let mut prev = vec![None::<usize>; node_ids.len()];
    let start_index = index_of[start];
    let destination_index = index_of[destination];

    dist[start_index] = 0.0;
    let mut heap = BinaryHeap::new();
    heap.push(QueueState {
        cost: 0.0,
        node_index: start_index,
    });

    while let Some(QueueState { cost, node_index }) = heap.pop() {
        if cost > dist[node_index] {
            continue;
        }
        if node_index == destination_index {
            break;
        }

        let node_id = &node_ids[node_index];
        for edge in edges.values().filter(|edge| edge.enabled) {
            let next_id = if edge.from == *node_id {
                Some(edge.to.as_str())
            } else if edge.bidirectional && edge.to == *node_id {
                Some(edge.from.as_str())
            } else {
                None
            };
            let Some(next_id) = next_id else {
                continue;
            };
            let Some(&next_index) = index_of.get(next_id) else {
                continue;
            };
            let Some(from_node) = nodes.get(node_id) else {
                continue;
            };
            let Some(to_node) = nodes.get(next_id) else {
                continue;
            };
            let distance = edge
                .distance
                .unwrap_or_else(|| distance_between(from_node.position, to_node.position));
            let next_cost = cost + distance.max(0.001) * edge.cost_scale;
            if next_cost < dist[next_index] {
                dist[next_index] = next_cost;
                prev[next_index] = Some(node_index);
                heap.push(QueueState {
                    cost: next_cost,
                    node_index: next_index,
                });
            }
        }
    }

    if !dist[destination_index].is_finite() {
        return Err(format!(
            "world navigation has no enabled route from '{start}' to '{destination}'"
        ));
    }

    let mut path = vec![destination_index];
    let mut cursor = destination_index;
    while cursor != start_index {
        cursor = prev[cursor].ok_or_else(|| {
            format!(
                "world navigation route reconstruction failed from '{start}' to '{destination}'"
            )
        })?;
        path.push(cursor);
    }
    path.reverse();
    Ok(path
        .into_iter()
        .map(|index| node_ids[index].clone())
        .collect())
}

pub(crate) fn advance_toward(
    position: [f32; 3],
    target: [f32; 3],
    max_distance: f32,
) -> ([f32; 3], f32, bool) {
    let dx = target[0] - position[0];
    let dy = target[1] - position[1];
    let dz = target[2] - position[2];
    let distance = (dx * dx + dy * dy + dz * dz).sqrt();
    if distance <= 1.0e-5 {
        return (target, 0.0, true);
    }
    if max_distance >= distance {
        return (target, distance, true);
    }
    let t = (max_distance / distance).clamp(0.0, 1.0);
    (
        [
            position[0] + dx * t,
            position[1] + dy * t,
            position[2] + dz * t,
        ],
        max_distance.max(0.0),
        false,
    )
}

pub(crate) fn distance_between(a: [f32; 3], b: [f32; 3]) -> f32 {
    distance_squared(a, b).sqrt()
}

fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, x: f32) -> WorldNavNodeDesc {
        WorldNavNodeDesc {
            id: id.to_owned(),
            position: [x, 0.0, 0.0],
            tags: Vec::new(),
            parameters: BTreeMap::new(),
        }
    }

    fn edge(id: &str, from: &str, to: &str, cost_scale: f32) -> WorldNavEdgeDesc {
        WorldNavEdgeDesc {
            id: id.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            bidirectional: true,
            distance: None,
            cost_scale,
            enabled: true,
            tags: Vec::new(),
            parameters: BTreeMap::new(),
        }
    }

    #[test]
    fn shortest_route_uses_edge_cost() {
        let nodes = BTreeMap::from([
            ("a".to_owned(), node("a", 0.0)),
            ("b".to_owned(), node("b", 10.0)),
            ("c".to_owned(), node("c", 20.0)),
        ]);
        let edges = BTreeMap::from([
            ("ab".to_owned(), edge("ab", "a", "b", 1.0)),
            ("bc".to_owned(), edge("bc", "b", "c", 1.0)),
            ("ac".to_owned(), edge("ac", "a", "c", 10.0)),
        ]);

        assert_eq!(
            shortest_route(&nodes, &edges, "a", "c").unwrap(),
            vec!["a", "b", "c"]
        );
    }
}
