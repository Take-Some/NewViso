use serde_json::Value;
use std::collections::BTreeMap;

use crate::data::{invalid_float_map, valid_json_payload, valid_label, validate_id};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct WorldSimulationPolicyDesc {
    pub transient_full_radius: f32,
    pub transient_reduced_radius: f32,
    #[serde(default = "default_tier_hysteresis_radius")]
    pub tier_hysteresis_radius: f32,
    pub full_interval_seconds: f32,
    pub reduced_interval_seconds: f32,
    pub background_interval_seconds: f32,
    pub max_actor_updates_per_step: u32,
}

fn default_tier_hysteresis_radius() -> f32 {
    8.0
}

impl Default for WorldSimulationPolicyDesc {
    fn default() -> Self {
        Self {
            transient_full_radius: 80.0,
            transient_reduced_radius: 260.0,
            tier_hysteresis_radius: default_tier_hysteresis_radius(),
            full_interval_seconds: 0.05,
            reduced_interval_seconds: 0.25,
            background_interval_seconds: 2.0,
            max_actor_updates_per_step: 256,
        }
    }
}

impl WorldSimulationPolicyDesc {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self.transient_full_radius.is_finite()
            || self.transient_full_radius < 0.0
            || !self.transient_reduced_radius.is_finite()
            || self.transient_reduced_radius < self.transient_full_radius
            || !self.tier_hysteresis_radius.is_finite()
            || !(0.0..=10_000.0).contains(&self.tier_hysteresis_radius)
            || !self.full_interval_seconds.is_finite()
            || !(0.001..=86_400.0).contains(&self.full_interval_seconds)
            || !self.reduced_interval_seconds.is_finite()
            || self.reduced_interval_seconds < self.full_interval_seconds
            || !self.background_interval_seconds.is_finite()
            || self.background_interval_seconds < self.reduced_interval_seconds
            || !(1..=1_000_000).contains(&self.max_actor_updates_per_step)
        {
            return Err("invalid generic WorldSimulationPolicyDesc parameters".to_owned());
        }
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct WorldObserverDesc {
    pub id: String,
    pub position: [f32; 3],
    pub full_radius: f32,
    pub reduced_radius: f32,
    pub importance: f32,
    pub tags: Vec<String>,
}

impl WorldObserverDesc {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_id("world observer", &self.id)?;
        if self.position.iter().any(|value| !value.is_finite())
            || !self.full_radius.is_finite()
            || self.full_radius < 0.0
            || !self.reduced_radius.is_finite()
            || self.reduced_radius < self.full_radius
            || !self.importance.is_finite()
            || self.importance < 0.0
            || self.tags.iter().any(|tag| !valid_label(tag))
        {
            return Err("invalid generic WorldObserverDesc parameters".to_owned());
        }
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct WorldActorDesc {
    pub id: String,
    pub kind: String,
    pub position: [f32; 3],
    pub group: Option<String>,
    pub channel: Option<String>,
    pub enabled: bool,
    pub tags: Vec<String>,
    pub parameters: BTreeMap<String, f32>,
    pub state: Value,
}

impl WorldActorDesc {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_id("world actor", &self.id)?;
        if !valid_label(&self.kind)
            || self.position.iter().any(|value| !value.is_finite())
            || self
                .group
                .as_deref()
                .is_some_and(|value| !valid_label(value))
            || self
                .channel
                .as_deref()
                .is_some_and(|value| !valid_label(value))
            || self.tags.iter().any(|tag| !valid_label(tag))
            || invalid_float_map(&self.parameters)
            || !valid_json_payload(&self.state)
        {
            return Err("invalid generic WorldActorDesc parameters".to_owned());
        }
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SimulationTier {
    Full,
    Reduced,
    Background,
}

impl SimulationTier {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Reduced => "reduced",
            Self::Background => "background",
        }
    }

    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Full => 0,
            Self::Reduced => 1,
            Self::Background => 2,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct ActorRecord {
    pub(crate) desc: WorldActorDesc,
    pub(crate) last_update_seconds: f64,
    pub(crate) next_update_seconds: f64,
    pub(crate) last_tier: SimulationTier,
}

#[derive(Clone, Debug)]
pub(crate) struct ActorUpdateTicket {
    pub(crate) actor_id: String,
    pub(crate) tier: SimulationTier,
    pub(crate) delta_seconds: f64,
    pub(crate) world_seconds: f64,
    pub(crate) fixed_tick: u64,
}

pub(crate) fn actor_tier(
    position: [f32; 3],
    observers: &[WorldObserverDesc],
    transient_observers: &[[f32; 3]],
    previous_tier: SimulationTier,
    policy: &WorldSimulationPolicyDesc,
) -> SimulationTier {
    let mut tier = SimulationTier::Background;

    for observer in observers {
        let candidate = classify_distance(
            distance_squared(position, observer.position),
            observer.full_radius,
            observer.reduced_radius,
            previous_tier,
            policy.tier_hysteresis_radius,
        );
        if candidate == SimulationTier::Full {
            return SimulationTier::Full;
        }
        if candidate == SimulationTier::Reduced {
            tier = SimulationTier::Reduced;
        }
    }

    for observer in transient_observers {
        let candidate = classify_distance(
            distance_squared(position, *observer),
            policy.transient_full_radius,
            policy.transient_reduced_radius,
            previous_tier,
            policy.tier_hysteresis_radius,
        );
        if candidate == SimulationTier::Full {
            return SimulationTier::Full;
        }
        if candidate == SimulationTier::Reduced {
            tier = SimulationTier::Reduced;
        }
    }

    tier
}

fn classify_distance(
    distance_sq: f32,
    full_radius: f32,
    reduced_radius: f32,
    previous_tier: SimulationTier,
    hysteresis: f32,
) -> SimulationTier {
    let (full_threshold, reduced_threshold) = match previous_tier {
        SimulationTier::Full => (full_radius + hysteresis, reduced_radius + hysteresis),
        SimulationTier::Reduced => (
            (full_radius - hysteresis).max(0.0),
            reduced_radius + hysteresis,
        ),
        SimulationTier::Background => (
            (full_radius - hysteresis).max(0.0),
            (reduced_radius - hysteresis).max(0.0),
        ),
    };

    if distance_sq <= full_threshold * full_threshold {
        SimulationTier::Full
    } else if distance_sq <= reduced_threshold * reduced_threshold {
        SimulationTier::Reduced
    } else {
        SimulationTier::Background
    }
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

    #[test]
    fn legacy_policy_without_hysteresis_uses_default() {
        let policy: WorldSimulationPolicyDesc = serde_json::from_value(serde_json::json!({
            "transient_full_radius": 12.0,
            "transient_reduced_radius": 24.0,
            "full_interval_seconds": 0.05,
            "reduced_interval_seconds": 0.25,
            "background_interval_seconds": 2.0,
            "max_actor_updates_per_step": 256
        }))
        .unwrap();

        assert_eq!(
            policy.tier_hysteresis_radius,
            default_tier_hysteresis_radius()
        );
    }

    #[test]
    fn hysteresis_keeps_full_tier_stable_at_boundary() {
        let policy = WorldSimulationPolicyDesc {
            transient_full_radius: 80.0,
            transient_reduced_radius: 260.0,
            tier_hysteresis_radius: 10.0,
            ..WorldSimulationPolicyDesc::default()
        };

        assert_eq!(
            actor_tier(
                [85.0, 0.0, 0.0],
                &[],
                &[[0.0, 0.0, 0.0]],
                SimulationTier::Full,
                &policy,
            ),
            SimulationTier::Full
        );
        assert_eq!(
            actor_tier(
                [85.0, 0.0, 0.0],
                &[],
                &[[0.0, 0.0, 0.0]],
                SimulationTier::Background,
                &policy,
            ),
            SimulationTier::Reduced
        );
        assert_eq!(
            actor_tier(
                [91.0, 0.0, 0.0],
                &[],
                &[[0.0, 0.0, 0.0]],
                SimulationTier::Full,
                &policy,
            ),
            SimulationTier::Reduced
        );
    }

    #[test]
    fn hysteresis_delays_background_promotion_and_reduced_demotion() {
        let policy = WorldSimulationPolicyDesc {
            transient_full_radius: 80.0,
            transient_reduced_radius: 260.0,
            tier_hysteresis_radius: 10.0,
            ..WorldSimulationPolicyDesc::default()
        };

        assert_eq!(
            actor_tier(
                [255.0, 0.0, 0.0],
                &[],
                &[[0.0, 0.0, 0.0]],
                SimulationTier::Background,
                &policy,
            ),
            SimulationTier::Background
        );
        assert_eq!(
            actor_tier(
                [269.0, 0.0, 0.0],
                &[],
                &[[0.0, 0.0, 0.0]],
                SimulationTier::Reduced,
                &policy,
            ),
            SimulationTier::Reduced
        );
        assert_eq!(
            actor_tier(
                [271.0, 0.0, 0.0],
                &[],
                &[[0.0, 0.0, 0.0]],
                SimulationTier::Reduced,
                &policy,
            ),
            SimulationTier::Background
        );
    }
}
