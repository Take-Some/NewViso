use serde_json::Value;

use crate::data::{valid_json_payload, valid_label, validate_id};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct WorldProcessDesc {
    pub id: String,
    pub interval_seconds: f64,
    pub phase_seconds: f64,
    pub enabled: bool,
    pub priority: i32,
    pub tags: Vec<String>,
    pub payload: Value,
}

impl WorldProcessDesc {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_id("world process", &self.id)?;
        if !self.interval_seconds.is_finite()
            || !(0.001..=31_536_000.0).contains(&self.interval_seconds)
            || !self.phase_seconds.is_finite()
            || self.phase_seconds.abs() > 1.0e12
            || self.tags.iter().any(|tag| !valid_label(tag))
            || !valid_json_payload(&self.payload)
        {
            return Err("invalid generic WorldProcessDesc parameters".to_owned());
        }
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct WorldProcessRecord {
    pub(crate) desc: WorldProcessDesc,
    pub(crate) next_due_seconds: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct WorldProcessActivation {
    pub(crate) id: String,
    pub(crate) priority: i32,
    pub(crate) due_world_seconds: f64,
    pub(crate) delivered_world_seconds: f64,
    pub(crate) occurrences: u64,
    pub(crate) tags: Vec<String>,
    pub(crate) payload: Value,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct WorldScheduledEventDesc {
    pub id: String,
    pub kind: String,
    pub source: String,
    pub cause: Option<String>,
    pub delay_seconds: f64,
    pub ttl_seconds: f64,
    pub priority: i32,
    pub position: Option<[f32; 3]>,
    pub tags: Vec<String>,
    pub payload: Value,
}

impl WorldScheduledEventDesc {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_id("world event", &self.id)?;
        if !valid_label(&self.kind)
            || !valid_label(&self.source)
            || self
                .cause
                .as_deref()
                .is_some_and(|value| !valid_label(value))
            || !self.delay_seconds.is_finite()
            || !(0.0..=31_536_000.0).contains(&self.delay_seconds)
            || !self.ttl_seconds.is_finite()
            || !(0.001..=31_536_000.0).contains(&self.ttl_seconds)
            || self
                .position
                .is_some_and(|position| position.iter().any(|value| !value.is_finite()))
            || self.tags.iter().any(|tag| !valid_label(tag))
            || !valid_json_payload(&self.payload)
        {
            return Err("invalid generic WorldScheduledEventDesc parameters".to_owned());
        }
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct ScheduledWorldEvent {
    pub(crate) desc: WorldScheduledEventDesc,
    pub(crate) due_world_seconds: f64,
    pub(crate) expires_world_seconds: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct WorldEventDelivery {
    pub(crate) reality_event_id: String,
    pub(crate) desc: WorldScheduledEventDesc,
    pub(crate) due_world_seconds: f64,
    pub(crate) delivered_world_seconds: f64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct WorldFactRecord {
    pub(crate) value: Value,
    pub(crate) revision: u64,
    pub(crate) updated_world_seconds: f64,
}

pub(crate) fn first_due_at_or_after(now: f64, interval: f64, phase: f64) -> f64 {
    let phase = phase.rem_euclid(interval);
    if now <= phase {
        return phase;
    }
    phase + ((now - phase) / interval).ceil() * interval
}
