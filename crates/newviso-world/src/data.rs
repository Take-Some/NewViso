use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) const MAX_JSON_PAYLOAD_BYTES: usize = 1_048_576;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct PopulationChannelDesc {
    pub id: String,
    pub density: f32,
    pub max_active: u32,
    pub spawn_radius: f32,
    pub despawn_radius: f32,
    pub creation_budget_per_tick: u32,
    pub removal_budget_per_tick: u32,
    pub update_interval_seconds: f32,
    pub model_set: Option<String>,
    pub tags: Vec<String>,
    pub parameters: BTreeMap<String, f32>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct PopulationStreamingPolicyDesc {
    pub max_resident_sets: u32,
    pub request_budget_per_tick: u32,
    pub eviction_budget_per_tick: u32,
    pub fallback_set: Option<String>,
}

impl Default for PopulationStreamingPolicyDesc {
    fn default() -> Self {
        Self {
            max_resident_sets: 32,
            request_budget_per_tick: 4,
            eviction_budget_per_tick: 2,
            fallback_set: None,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct LivingWorldZoneDesc {
    pub id: String,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub priority: i32,
    pub tags: Vec<String>,
    pub parameters: BTreeMap<String, f32>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct AmbientModelSetDesc {
    pub id: String,
    pub category: String,
    pub assets: Vec<String>,
    pub weights: Vec<f32>,
    pub tags: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ScenarioPointDesc {
    pub id: String,
    pub kind: String,
    pub group: Option<String>,
    pub position: [f32; 3],
    pub heading_degrees: f32,
    pub radius: f32,
    pub probability: f32,
    pub model_set: Option<String>,
    pub enabled: bool,
    pub tags: Vec<String>,
    pub parameters: BTreeMap<String, f32>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct RelationshipRuleDesc {
    pub source_group: String,
    pub target_group: String,
    pub relation: String,
    pub weight: f32,
    pub tags: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct WorldStimulusDesc {
    pub id: String,
    pub kind: String,
    pub source: String,
    pub position: [f32; 3],
    pub radius: f32,
    pub intensity: f32,
    pub lifetime_seconds: f32,
    pub tags: Vec<String>,
    pub payload: Value,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct ActiveStimulus {
    pub(crate) desc: WorldStimulusDesc,
    pub(crate) expires_world_seconds: f64,
}

pub(crate) fn valid_label(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= 128
}

pub(crate) fn validate_id(kind: &str, value: &str) -> Result<(), String> {
    if valid_label(value) {
        Ok(())
    } else {
        Err(format!("{kind} id must be non-empty and at most 128 bytes"))
    }
}

pub(crate) fn invalid_float_map(values: &BTreeMap<String, f32>) -> bool {
    values
        .iter()
        .any(|(key, value)| !valid_label(key) || !value.is_finite() || value.abs() > 1.0e9)
}

pub(crate) fn valid_json_payload(value: &Value) -> bool {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() <= MAX_JSON_PAYLOAD_BYTES)
        .unwrap_or(false)
}

pub(crate) fn point_in_aabb(point: [f32; 3], min: [f32; 3], max: [f32; 3]) -> bool {
    point[0] >= min[0]
        && point[0] <= max[0]
        && point[1] >= min[1]
        && point[1] <= max[1]
        && point[2] >= min[2]
        && point[2] <= max[2]
}

pub(crate) fn zone_volume(zone: &LivingWorldZoneDesc) -> f32 {
    (zone.max[0] - zone.min[0]).max(0.0)
        * (zone.max[1] - zone.min[1]).max(0.0)
        * (zone.max[2] - zone.min[2]).max(0.0)
}
