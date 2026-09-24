use serde_json::Value;

use crate::data::{valid_json_payload, valid_label, validate_id};

pub(crate) const MAX_REALITY_HISTORY: usize = 4096;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct WorldRealityEventDesc {
    pub id: String,
    pub kind: String,
    pub source: String,
    pub cause: Option<String>,
    pub participants: Vec<String>,
    pub position: Option<[f32; 3]>,
    pub importance: f32,
    pub tags: Vec<String>,
    pub payload: Value,
}

impl WorldRealityEventDesc {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_id("world reality event", &self.id)?;
        if !valid_label(&self.kind)
            || !valid_label(&self.source)
            || self
                .cause
                .as_deref()
                .is_some_and(|value| !valid_label(value))
            || self.participants.len() > 4096
            || self.participants.iter().any(|value| !valid_label(value))
            || self
                .position
                .is_some_and(|position| position.iter().any(|value| !value.is_finite()))
            || !self.importance.is_finite()
            || !(0.0..=1.0e9).contains(&self.importance)
            || self.tags.iter().any(|tag| !valid_label(tag))
            || !valid_json_payload(&self.payload)
        {
            return Err("invalid generic WorldRealityEventDesc parameters".to_owned());
        }
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct WorldRealityEventRecord {
    pub(crate) sequence: u64,
    pub(crate) occurred_world_seconds: f64,
    pub(crate) desc: WorldRealityEventDesc,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct WorldScenarioReservationDesc {
    pub id: String,
    pub scenario_point_id: String,
    pub actor_id: String,
    pub delay_seconds: f64,
    pub duration_seconds: f64,
    pub priority: i32,
    pub exclusive: bool,
    pub payload: Value,
}

impl WorldScenarioReservationDesc {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_id("world scenario reservation", &self.id)?;
        validate_id("world scenario reservation point", &self.scenario_point_id)?;
        validate_id("world scenario reservation actor", &self.actor_id)?;
        if !self.delay_seconds.is_finite()
            || !(0.0..=31_536_000.0).contains(&self.delay_seconds)
            || !self.duration_seconds.is_finite()
            || !(0.001..=31_536_000.0).contains(&self.duration_seconds)
            || !valid_json_payload(&self.payload)
        {
            return Err("invalid generic WorldScenarioReservationDesc parameters".to_owned());
        }
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct WorldScenarioReservationRecord {
    pub(crate) desc: WorldScenarioReservationDesc,
    pub(crate) starts_world_seconds: f64,
    pub(crate) ends_world_seconds: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct WorldRepresentationChange {
    pub(crate) actor_id: String,
    pub(crate) previous: &'static str,
    pub(crate) current: &'static str,
    pub(crate) world_seconds: f64,
    pub(crate) fixed_tick: u64,
}
