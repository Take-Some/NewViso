use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

const MAX_SOUND_GRAPH_NODES: usize = 256;
const MAX_SOUND_GRAPH_EDGES_PER_NODE: usize = 64;
const MAX_SOUND_GRAPH_DEPTH: usize = 64;
const MAX_SYMBOL_LEN: usize = 256;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct YsncdSoundGraph {
    pub root: String,
    pub nodes: Vec<YsncdSoundGraphNode>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum YsncdSoundGraphNode {
    Clip {
        id: String,
        clip: String,
        #[serde(default = "one")]
        gain: f32,
        #[serde(default = "one")]
        pitch: f32,
    },
    Random {
        id: String,
        children: Vec<YsncdWeightedNodeRef>,
    },
    Sequence {
        id: String,
        children: Vec<String>,
    },
    Switch {
        id: String,
        switch: String,
        cases: BTreeMap<String, String>,
        #[serde(default)]
        default: Option<String>,
    },
    Blend1d {
        id: String,
        input: String,
        points: Vec<YsncdBlendPoint>,
    },
    Parameter {
        id: String,
        parameter: String,
        #[serde(default)]
        default: f32,
        #[serde(default = "default_parameter_min")]
        min: f32,
        #[serde(default = "default_parameter_max")]
        max: f32,
    },
    Envelope {
        id: String,
        input: String,
        points: Vec<[f32; 2]>,
    },
    Layer {
        id: String,
        children: Vec<YsncdLayerNodeRef>,
    },
}

impl YsncdSoundGraphNode {
    #[inline]
    pub fn id(&self) -> &str {
        match self {
            Self::Clip { id, .. }
            | Self::Random { id, .. }
            | Self::Sequence { id, .. }
            | Self::Switch { id, .. }
            | Self::Blend1d { id, .. }
            | Self::Parameter { id, .. }
            | Self::Envelope { id, .. }
            | Self::Layer { id, .. } => id,
        }
    }

    #[inline]
    pub const fn output_kind(&self) -> YsncdSoundGraphValueKind {
        match self {
            Self::Parameter { .. } | Self::Envelope { .. } => YsncdSoundGraphValueKind::Scalar,
            _ => YsncdSoundGraphValueKind::Voices,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YsncdSoundGraphValueKind {
    Scalar,
    Voices,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct YsncdWeightedNodeRef {
    pub node: String,
    pub weight: f32,
}

impl Default for YsncdWeightedNodeRef {
    fn default() -> Self {
        Self {
            node: String::new(),
            weight: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct YsncdBlendPoint {
    pub value: f32,
    pub node: String,
}

impl Default for YsncdBlendPoint {
    fn default() -> Self {
        Self {
            value: 0.0,
            node: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct YsncdLayerNodeRef {
    pub node: String,
    pub gain: f32,
    pub pitch: f32,
}

impl Default for YsncdLayerNodeRef {
    fn default() -> Self {
        Self {
            node: String::new(),
            gain: 1.0,
            pitch: 1.0,
        }
    }
}

impl YsncdSoundGraph {
    pub fn validate<'a>(
        &'a self,
        clip_names: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), String> {
        if self.nodes.is_empty() {
            return Err("YSNCD SoundGraph requires at least one node".to_owned());
        }
        if self.nodes.len() > MAX_SOUND_GRAPH_NODES {
            return Err(format!(
                "YSNCD SoundGraph node count {} exceeds {}",
                self.nodes.len(),
                MAX_SOUND_GRAPH_NODES
            ));
        }
        validate_symbol("YSNCD SoundGraph root", &self.root)?;

        let clips = clip_names
            .into_iter()
            .map(|name| name.trim().to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let mut nodes = HashMap::<String, &YsncdSoundGraphNode>::with_capacity(self.nodes.len());
        for node in &self.nodes {
            validate_symbol("YSNCD SoundGraph node id", node.id())?;
            let key = node.id().trim().to_ascii_lowercase();
            if nodes.insert(key.clone(), node).is_some() {
                return Err(format!(
                    "duplicate YSNCD SoundGraph node id '{}'",
                    node.id()
                ));
            }
            self.validate_node_payload(node, &clips)?;
        }

        let root_key = self.root.trim().to_ascii_lowercase();
        let root = nodes
            .get(&root_key)
            .copied()
            .ok_or_else(|| format!("YSNCD SoundGraph root '{}' does not resolve", self.root))?;
        if root.output_kind() != YsncdSoundGraphValueKind::Voices {
            return Err(format!(
                "YSNCD SoundGraph root '{}' must produce voices",
                self.root
            ));
        }

        for node in &self.nodes {
            self.validate_references(node, &nodes)?;
        }
        validate_acyclic(&root_key, &nodes)?;
        Ok(())
    }

    fn validate_node_payload(
        &self,
        node: &YsncdSoundGraphNode,
        clips: &BTreeSet<String>,
    ) -> Result<(), String> {
        match node {
            YsncdSoundGraphNode::Clip {
                id,
                clip,
                gain,
                pitch,
            } => {
                validate_symbol("YSNCD SoundGraph clip", clip)?;
                if !clips.contains(&clip.trim().to_ascii_lowercase()) {
                    return Err(format!(
                        "YSNCD SoundGraph Clip node '{id}' references unknown clip '{clip}'"
                    ));
                }
                validate_gain_pitch(id, *gain, *pitch)?;
            }
            YsncdSoundGraphNode::Random { id, children } => {
                validate_non_empty_bounded(id, "Random children", children.len())?;
                for child in children {
                    validate_symbol("YSNCD SoundGraph Random child", &child.node)?;
                    if !child.weight.is_finite() || child.weight <= 0.0 {
                        return Err(format!(
                            "YSNCD SoundGraph Random node '{id}' has non-positive/non-finite weight"
                        ));
                    }
                }
            }
            YsncdSoundGraphNode::Sequence { id, children } => {
                validate_non_empty_bounded(id, "Sequence children", children.len())?;
                for child in children {
                    validate_symbol("YSNCD SoundGraph Sequence child", child)?;
                }
            }
            YsncdSoundGraphNode::Switch {
                id,
                switch,
                cases,
                default,
            } => {
                validate_symbol("YSNCD SoundGraph switch name", switch)?;
                validate_non_empty_bounded(id, "Switch cases", cases.len())?;
                let mut normalized_cases = HashSet::with_capacity(cases.len());
                for (value, target) in cases {
                    validate_value("YSNCD SoundGraph switch value", value)?;
                    validate_symbol("YSNCD SoundGraph Switch target", target)?;
                    if !normalized_cases.insert(value.trim().to_ascii_lowercase()) {
                        return Err(format!(
                            "YSNCD SoundGraph Switch node '{id}' has duplicate case '{value}'"
                        ));
                    }
                }
                if let Some(default) = default {
                    validate_symbol("YSNCD SoundGraph Switch default", default)?;
                }
            }
            YsncdSoundGraphNode::Blend1d { id, input, points } => {
                validate_symbol("YSNCD SoundGraph Blend1D input", input)?;
                if points.is_empty() || points.len() > MAX_SOUND_GRAPH_EDGES_PER_NODE {
                    return Err(format!(
                        "YSNCD SoundGraph Blend1D node '{id}' requires 1..={} points",
                        MAX_SOUND_GRAPH_EDGES_PER_NODE
                    ));
                }
                let mut previous = None::<f32>;
                for point in points {
                    if !point.value.is_finite() {
                        return Err(format!(
                            "YSNCD SoundGraph Blend1D node '{id}' has non-finite point"
                        ));
                    }
                    validate_symbol("YSNCD SoundGraph Blend1D child", &point.node)?;
                    if previous.is_some_and(|value| point.value <= value) {
                        return Err(format!(
                            "YSNCD SoundGraph Blend1D node '{id}' points must be strictly ascending"
                        ));
                    }
                    previous = Some(point.value);
                }
            }
            YsncdSoundGraphNode::Parameter {
                id,
                parameter,
                default,
                min,
                max,
            } => {
                validate_symbol("YSNCD SoundGraph parameter name", parameter)?;
                if !default.is_finite()
                    || !min.is_finite()
                    || !max.is_finite()
                    || min > max
                    || default < min
                    || default > max
                {
                    return Err(format!(
                        "YSNCD SoundGraph Parameter node '{id}' requires finite min <= default <= max"
                    ));
                }
            }
            YsncdSoundGraphNode::Envelope { id, input, points } => {
                validate_symbol("YSNCD SoundGraph Envelope input", input)?;
                if points.is_empty() || points.len() > MAX_SOUND_GRAPH_EDGES_PER_NODE {
                    return Err(format!(
                        "YSNCD SoundGraph Envelope node '{id}' requires 1..={} points",
                        MAX_SOUND_GRAPH_EDGES_PER_NODE
                    ));
                }
                let mut previous = None::<f32>;
                for [x, y] in points {
                    if !x.is_finite() || !y.is_finite() {
                        return Err(format!(
                            "YSNCD SoundGraph Envelope node '{id}' has non-finite point"
                        ));
                    }
                    if previous.is_some_and(|value| *x <= value) {
                        return Err(format!(
                            "YSNCD SoundGraph Envelope node '{id}' x values must be strictly ascending"
                        ));
                    }
                    previous = Some(*x);
                }
            }
            YsncdSoundGraphNode::Layer { id, children } => {
                validate_non_empty_bounded(id, "Layer children", children.len())?;
                for child in children {
                    validate_symbol("YSNCD SoundGraph Layer child", &child.node)?;
                    validate_gain_pitch(id, child.gain, child.pitch)?;
                }
            }
        }
        Ok(())
    }

    fn validate_references(
        &self,
        node: &YsncdSoundGraphNode,
        nodes: &HashMap<String, &YsncdSoundGraphNode>,
    ) -> Result<(), String> {
        let id = node.id();
        match node {
            YsncdSoundGraphNode::Clip { .. } | YsncdSoundGraphNode::Parameter { .. } => Ok(()),
            YsncdSoundGraphNode::Random { children, .. } => {
                for child in children {
                    require_kind(nodes, id, &child.node, YsncdSoundGraphValueKind::Voices)?;
                }
                Ok(())
            }
            YsncdSoundGraphNode::Sequence { children, .. } => {
                for child in children {
                    require_kind(nodes, id, child, YsncdSoundGraphValueKind::Voices)?;
                }
                Ok(())
            }
            YsncdSoundGraphNode::Switch { cases, default, .. } => {
                for target in cases.values() {
                    require_kind(nodes, id, target, YsncdSoundGraphValueKind::Voices)?;
                }
                if let Some(default) = default {
                    require_kind(nodes, id, default, YsncdSoundGraphValueKind::Voices)?;
                }
                Ok(())
            }
            YsncdSoundGraphNode::Blend1d { input, points, .. } => {
                require_kind(nodes, id, input, YsncdSoundGraphValueKind::Scalar)?;
                for point in points {
                    require_kind(nodes, id, &point.node, YsncdSoundGraphValueKind::Voices)?;
                }
                Ok(())
            }
            YsncdSoundGraphNode::Envelope { input, .. } => {
                require_kind(nodes, id, input, YsncdSoundGraphValueKind::Scalar)
            }
            YsncdSoundGraphNode::Layer { children, .. } => {
                for child in children {
                    require_kind(nodes, id, &child.node, YsncdSoundGraphValueKind::Voices)?;
                }
                Ok(())
            }
        }
    }
}

fn require_kind(
    nodes: &HashMap<String, &YsncdSoundGraphNode>,
    owner: &str,
    target: &str,
    kind: YsncdSoundGraphValueKind,
) -> Result<(), String> {
    let target_key = target.trim().to_ascii_lowercase();
    let node = nodes.get(&target_key).copied().ok_or_else(|| {
        format!("YSNCD SoundGraph node '{owner}' references unknown node '{target}'")
    })?;
    if node.output_kind() != kind {
        return Err(format!(
            "YSNCD SoundGraph node '{owner}' references '{target}' as {:?}, but it produces {:?}",
            kind,
            node.output_kind()
        ));
    }
    Ok(())
}

fn validate_acyclic(
    root: &str,
    nodes: &HashMap<String, &YsncdSoundGraphNode>,
) -> Result<(), String> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Visiting,
        Done,
    }
    fn visit(
        key: &str,
        nodes: &HashMap<String, &YsncdSoundGraphNode>,
        marks: &mut HashMap<String, Mark>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > MAX_SOUND_GRAPH_DEPTH {
            return Err(format!(
                "YSNCD SoundGraph traversal exceeds max depth {MAX_SOUND_GRAPH_DEPTH}"
            ));
        }
        match marks.get(key).copied() {
            Some(Mark::Visiting) => {
                return Err(format!("YSNCD SoundGraph dependency cycle reaches '{key}'"));
            }
            Some(Mark::Done) => return Ok(()),
            None => {}
        }
        marks.insert(key.to_owned(), Mark::Visiting);
        let node = nodes.get(key).copied().ok_or_else(|| {
            format!("YSNCD SoundGraph node '{key}' disappeared during validation")
        })?;
        for child in node_edges(node) {
            visit(&child.to_ascii_lowercase(), nodes, marks, depth + 1)?;
        }
        marks.insert(key.to_owned(), Mark::Done);
        Ok(())
    }

    let mut marks = HashMap::<String, Mark>::new();
    visit(root, nodes, &mut marks, 0)?;
    for key in nodes.keys() {
        visit(key, nodes, &mut marks, 0)?;
    }
    Ok(())
}

fn node_edges(node: &YsncdSoundGraphNode) -> Vec<&str> {
    match node {
        YsncdSoundGraphNode::Clip { .. } | YsncdSoundGraphNode::Parameter { .. } => Vec::new(),
        YsncdSoundGraphNode::Random { children, .. } => {
            children.iter().map(|child| child.node.as_str()).collect()
        }
        YsncdSoundGraphNode::Sequence { children, .. } => {
            children.iter().map(String::as_str).collect()
        }
        YsncdSoundGraphNode::Switch { cases, default, .. } => cases
            .values()
            .map(String::as_str)
            .chain(default.iter().map(String::as_str))
            .collect(),
        YsncdSoundGraphNode::Blend1d { input, points, .. } => std::iter::once(input.as_str())
            .chain(points.iter().map(|point| point.node.as_str()))
            .collect(),
        YsncdSoundGraphNode::Envelope { input, .. } => vec![input.as_str()],
        YsncdSoundGraphNode::Layer { children, .. } => {
            children.iter().map(|child| child.node.as_str()).collect()
        }
    }
}

fn validate_non_empty_bounded(id: &str, label: &str, len: usize) -> Result<(), String> {
    if len == 0 || len > MAX_SOUND_GRAPH_EDGES_PER_NODE {
        return Err(format!(
            "YSNCD SoundGraph node '{id}' {label} must contain 1..={} entries",
            MAX_SOUND_GRAPH_EDGES_PER_NODE
        ));
    }
    Ok(())
}

fn validate_gain_pitch(id: &str, gain: f32, pitch: f32) -> Result<(), String> {
    if !gain.is_finite() || !(0.0..=4.0).contains(&gain) {
        return Err(format!(
            "YSNCD SoundGraph node '{id}' gain must be finite in [0, 4]"
        ));
    }
    if !pitch.is_finite() || !(0.05..=4.0).contains(&pitch) {
        return Err(format!(
            "YSNCD SoundGraph node '{id}' pitch must be finite in [0.05, 4]"
        ));
    }
    Ok(())
}

fn validate_symbol(label: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_SYMBOL_LEN || value.chars().any(char::is_control) {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn validate_value(label: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_SYMBOL_LEN || value.chars().any(char::is_control) {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

#[inline]
const fn one() -> f32 {
    1.0
}

#[inline]
const fn default_parameter_min() -> f32 {
    -1_000_000.0
}

#[inline]
const fn default_parameter_max() -> f32 {
    1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_graph() -> YsncdSoundGraph {
        YsncdSoundGraph {
            root: "root".to_owned(),
            nodes: vec![
                YsncdSoundGraphNode::Parameter {
                    id: "speed".to_owned(),
                    parameter: "project.speed".to_owned(),
                    default: 0.0,
                    min: 0.0,
                    max: 1.0,
                },
                YsncdSoundGraphNode::Envelope {
                    id: "speed_curve".to_owned(),
                    input: "speed".to_owned(),
                    points: vec![[0.0, 0.0], [1.0, 1.0]],
                },
                YsncdSoundGraphNode::Clip {
                    id: "a".to_owned(),
                    clip: "a".to_owned(),
                    gain: 1.0,
                    pitch: 1.0,
                },
                YsncdSoundGraphNode::Clip {
                    id: "b".to_owned(),
                    clip: "b".to_owned(),
                    gain: 1.0,
                    pitch: 1.0,
                },
                YsncdSoundGraphNode::Blend1d {
                    id: "root".to_owned(),
                    input: "speed_curve".to_owned(),
                    points: vec![
                        YsncdBlendPoint {
                            value: 0.0,
                            node: "a".to_owned(),
                        },
                        YsncdBlendPoint {
                            value: 1.0,
                            node: "b".to_owned(),
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn validates_typed_dag_and_opaque_parameter_names() {
        valid_graph()
            .validate(["a", "b"])
            .expect("valid typed graph");
    }

    #[test]
    fn rejects_cycle() {
        let graph = YsncdSoundGraph {
            root: "a".to_owned(),
            nodes: vec![
                YsncdSoundGraphNode::Layer {
                    id: "a".to_owned(),
                    children: vec![YsncdLayerNodeRef {
                        node: "b".to_owned(),
                        ..Default::default()
                    }],
                },
                YsncdSoundGraphNode::Layer {
                    id: "b".to_owned(),
                    children: vec![YsncdLayerNodeRef {
                        node: "a".to_owned(),
                        ..Default::default()
                    }],
                },
            ],
        };
        assert!(graph.validate(["clip"]).is_err());
    }

    #[test]
    fn rejects_scalar_root_and_type_mismatch() {
        let scalar_root = YsncdSoundGraph {
            root: "p".to_owned(),
            nodes: vec![YsncdSoundGraphNode::Parameter {
                id: "p".to_owned(),
                parameter: "project.any".to_owned(),
                default: 0.0,
                min: -1.0,
                max: 1.0,
            }],
        };
        assert!(scalar_root.validate(["a"]).is_err());

        let mismatch = YsncdSoundGraph {
            root: "layer".to_owned(),
            nodes: vec![
                YsncdSoundGraphNode::Parameter {
                    id: "p".to_owned(),
                    parameter: "project.any".to_owned(),
                    default: 0.0,
                    min: -1.0,
                    max: 1.0,
                },
                YsncdSoundGraphNode::Layer {
                    id: "layer".to_owned(),
                    children: vec![YsncdLayerNodeRef {
                        node: "p".to_owned(),
                        ..Default::default()
                    }],
                },
            ],
        };
        assert!(mismatch.validate(["a"]).is_err());
    }
}
