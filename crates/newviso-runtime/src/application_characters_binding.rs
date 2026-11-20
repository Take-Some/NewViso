//! Validation and position conversion for authored physical character bindings.
use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct PhysicalCharacterBinding {
    pub actor_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub controller: CharacterConfig,
    #[serde(default)]
    pub body_center_offset_y: Option<f32>,
    #[serde(default = "default_arrival_radius")]
    pub arrival_radius: f32,
    #[serde(default = "default_time_horizon")]
    pub steering_time_horizon: f32,
    #[serde(default = "default_separation_weight")]
    pub separation_weight: f32,
    #[serde(default = "default_gravity")]
    pub gravity: f32,
}

fn default_true() -> bool {
    true
}
pub(super) fn default_arrival_radius() -> f32 {
    0.18
}
pub(super) fn default_time_horizon() -> f32 {
    1.5
}
pub(super) fn default_separation_weight() -> f32 {
    2.0
}
pub(super) fn default_gravity() -> f32 {
    9.81
}

impl PhysicalCharacterBinding {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.actor_id.trim().is_empty()
            || self.actor_id.len() > 128
            || !self.arrival_radius.is_finite()
            || !(0.01..=5.0).contains(&self.arrival_radius)
            || !self.steering_time_horizon.is_finite()
            || !(0.05..=30.0).contains(&self.steering_time_horizon)
            || !self.separation_weight.is_finite()
            || !(0.0..=100.0).contains(&self.separation_weight)
            || !self.gravity.is_finite()
            || !(0.0..=1000.0).contains(&self.gravity)
            || self
                .body_center_offset_y
                .is_some_and(|value| !value.is_finite() || !(-10.0..=10.0).contains(&value))
        {
            return Err("invalid physical world-actor character binding".to_owned());
        }
        self.controller.validate()
    }

    pub(crate) fn center_offset_y(&self) -> f32 {
        self.body_center_offset_y
            .unwrap_or(self.controller.radius + self.controller.half_height)
    }

    pub(super) fn actor_to_center(&self, actor_position: [f32; 3]) -> [f32; 3] {
        [
            actor_position[0],
            actor_position[1] + self.center_offset_y(),
            actor_position[2],
        ]
    }

    pub(super) fn center_to_actor(&self, center: [f32; 3]) -> [f32; 3] {
        [center[0], center[1] - self.center_offset_y(), center[2]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_binding_without_changing_defaults() {
        let mut binding = PhysicalCharacterBinding {
            actor_id: "npc".into(),
            enabled: true,
            controller: CharacterConfig::default(),
            body_center_offset_y: None,
            arrival_radius: default_arrival_radius(),
            steering_time_horizon: default_time_horizon(),
            separation_weight: default_separation_weight(),
            gravity: default_gravity(),
        };
        assert!(binding.validate().is_ok());
        binding.gravity = f32::INFINITY;
        assert!(binding.validate().is_err());
    }

    #[test]
    fn actor_center_conversion_is_reversible() {
        let binding = PhysicalCharacterBinding {
            actor_id: "npc".into(),
            enabled: true,
            controller: CharacterConfig::default(),
            body_center_offset_y: Some(1.25),
            arrival_radius: default_arrival_radius(),
            steering_time_horizon: default_time_horizon(),
            separation_weight: default_separation_weight(),
            gravity: default_gravity(),
        };
        let point = [3.0, 5.0, -7.0];
        assert_eq!(
            binding.center_to_actor(binding.actor_to_center(point)),
            point
        );
    }
}
