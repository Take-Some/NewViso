//! Character capability for any scene entity, independent of player/Ped identity.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CharacterUpdatePhase {
    #[default]
    Native,
    Script,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneCharacterBinding {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub body_id: Option<String>,
    #[serde(default)]
    pub body_entity: Option<u64>,
    #[serde(default)]
    pub foot_placement_profile: Option<String>,
    #[serde(default)]
    pub endorphin_model: Option<String>,
    #[serde(default)]
    pub phase: CharacterUpdatePhase,
    #[serde(default = "yes")]
    pub sync_position: bool,
    #[serde(default)]
    pub root_offset_y: Option<f32>,
}
fn asset(path: &str) -> bool {
    !path.trim().is_empty()
        && path.len() <= 2048
        && !path.starts_with('/')
        && !path.contains(['\\', ':'])
        && !path.split('/').any(|p| p == ".." || p.is_empty())
}
impl SceneCharacterBinding {
    pub fn parse(value: &serde_json::Value) -> Result<Option<Self>, String> {
        if value.is_null() {
            return Ok(None);
        }
        let binding: Self = serde_json::from_value(value.clone())
            .map_err(|e| format!("invalid character component: {e}"))?;
        binding.validate()?;
        Ok(Some(binding))
    }
    pub fn validate(&self) -> Result<(), String> {
        if self
            .body_id
            .as_ref()
            .is_some_and(|id| id.trim().is_empty() || id.len() > 2048)
            || self
                .body_entity
                .is_some_and(|id| id == 0 || id >= 1_u64 << 63)
            || (self.body_id.is_some() && self.body_entity.is_some())
            || self.root_offset_y.is_some_and(|v| !v.is_finite())
        {
            return Err("invalid character body reference or root offset".into());
        }
        if self
            .foot_placement_profile
            .as_ref()
            .is_some_and(|p| !asset(p))
            || self.endorphin_model.as_ref().is_some_and(|p| !asset(p))
        {
            return Err("character anatomy must use a logical asset path".into());
        }
        if self.enabled && (self.foot_placement_profile.is_some() == self.endorphin_model.is_some())
        {
            return Err(
                "enabled character needs exactly one foot_placement_profile or endorphin_model"
                    .into(),
            );
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn generic_character_keeps_body_and_anatomy_across_serialization() {
        let v = json!({"body_id":"custom.robot.body","foot_placement_profile":"rigs/custom.feet.json","phase":"script","sync_position":false});
        let a = SceneCharacterBinding::parse(&v).unwrap().unwrap();
        let b = SceneCharacterBinding::parse(&serde_json::to_value(&a).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.phase, CharacterUpdatePhase::Script);
    }
    #[test]
    fn disabled_entities_and_no_component_are_supported() {
        assert!(SceneCharacterBinding::parse(&json!({"enabled":false}))
            .unwrap()
            .is_some());
        assert!(SceneCharacterBinding::parse(&serde_json::Value::Null)
            .unwrap()
            .is_none());
    }
    #[test]
    fn invalid_anatomy_and_conflicting_owners_fail_before_scene_mutation() {
        for v in [
            json!({}),
            json!({"foot_placement_profile":"../outside.json"}),
            json!({"foot_placement_profile":"rigs/a.json","endorphin_model":"rigs/b.json"}),
            json!({"foot_placement_profile":"rigs/a.json","body_id":"a","body_entity":7}),
            json!({"foot_placement_profile":"rigs/a.json","phase":"guess"}),
            json!({"foot_placement_profile":"rigs/a.json","unknown":true}),
        ] {
            assert!(SceneCharacterBinding::parse(&v).is_err(), "{v}");
        }
    }
}
