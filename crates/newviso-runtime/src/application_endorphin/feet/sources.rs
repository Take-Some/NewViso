//! Anatomy sources are adapters. Placement never tests player/Ped/project identity.
use super::*;
use newviso_scene::{CharacterUpdatePhase, SceneCharacterBinding};
pub(super) type FootSource = (
    u64,
    u64,
    Arc<Vec<LegBinding>>,
    Option<Arc<SceneCharacterBinding>>,
);
impl EngineApplication {
    pub(crate) fn suspend_physical_character_drives(&mut self) {
        let Some(physics) = self.physics.as_mut() else {
            return;
        };
        for (&entity, binding) in self.scene.character_bindings() {
            if !binding.enabled || !self.scene.entity_has_physical_pose(entity) {
                continue;
            }
            let body = binding
                .body_entity
                .or_else(|| {
                    binding
                        .body_id
                        .as_deref()
                        .and_then(|id| self.scene.runtime_entity_stable_id(id))
                })
                .or_else(|| binding.body_id.is_none().then_some(entity));
            if let Some(body) = body {
                physics.suspend_character_drive(body);
            }
        }
    }

    pub(super) fn foot_profile_asset(
        &mut self,
        path: &str,
    ) -> Result<Arc<Vec<LegBinding>>, String> {
        if let Some(profile) = self.endorphin.feet.profiles.get(path) {
            return Ok(profile.clone());
        }
        let legs: Vec<LegBinding> =
            serde_json::from_value(newviso_assets_client::AssetClient::new().json(path)?)
                .map_err(|e| format!("invalid foot profile '{path}': {e}"))?;
        newviso_character::validate_foot_profile(&legs)?;
        let legs = Arc::new(legs);
        self.endorphin
            .feet
            .profiles
            .insert(path.into(), legs.clone());
        Ok(legs)
    }
    pub(super) fn foot_rig_asset(&mut self, path: &str) -> Result<Arc<Vec<LegBinding>>, String> {
        let key = format!("rig:{path}");
        if let Some(profile) = self.endorphin.feet.profiles.get(&key) {
            return Ok(profile.clone());
        }
        if !self.endorphin.models.contains_key(path) {
            let model: EndorphinModel =
                serde_json::from_value(newviso_assets_client::AssetClient::new().json(path)?)
                    .map_err(|e| format!("invalid character anatomy '{path}': {e}"))?;
            self.endorphin
                .joint_maps
                .insert(path.into(), model.joint_map.clone());
            self.endorphin
                .models
                .insert(path.into(), Arc::new(model.bind_joints()?));
        }
        let parts = &self.endorphin.models[path].rig.parts;
        let legs = parts
            .iter()
            .filter_map(|foot| {
                let sole = foot.sole?;
                let knee = parts
                    .iter()
                    .find(|p| Some(&p.name) == foot.parent.as_ref())?;
                let hip = parts
                    .iter()
                    .find(|p| Some(&p.name) == knee.parent.as_ref())?;
                Some(LegBinding {
                    hip: hip.joint.clone(),
                    knee: knee.joint.clone(),
                    foot: foot.joint.clone(),
                    toe: foot.end_joint.clone()?,
                    sole_height: sole.vertical_offset_radius_scale.abs() * foot.radius,
                    sole_half_width: Some(sole.width_radius_scale * foot.radius),
                    heel_length: Some(sole.heel_radius_scale * foot.radius),
                })
            })
            .collect::<Vec<_>>();
        newviso_character::validate_foot_profile(&legs)?;
        let legs = Arc::new(legs);
        self.endorphin.feet.profiles.insert(key, legs.clone());
        Ok(legs)
    }
    pub(super) fn entity_foot_sources(
        &mut self,
        script_phase: bool,
    ) -> Result<Vec<FootSource>, String> {
        let bindings = self
            .scene
            .character_bindings()
            .iter()
            .map(|(&id, b)| (id, b.clone()))
            .collect::<Vec<_>>();
        let mut sources = Vec::new();
        let mut owners = BTreeMap::new();
        for (entity, binding) in bindings {
            if !binding.enabled {
                continue;
            }
            let body = if let Some(body) = binding.body_entity {
                Some(body)
            } else if let Some(id) = &binding.body_id {
                self.scene.runtime_entity_stable_id(id)
            } else {
                Some(entity)
            };
            let Some(body) = body else { continue };
            if let Some(other) = owners.insert(body, entity) {
                if other != entity {
                    return Err(format!(
                        "physics body {body} has more than one character owner ({other}, {entity})"
                    ));
                }
            }
            if script_phase != (binding.phase == CharacterUpdatePhase::Script)
                || !self.scene.character_entity_is_active(entity)
            {
                continue;
            }
            let legs = if let Some(asset) = &binding.foot_placement_profile {
                self.foot_profile_asset(asset)?
            } else {
                self.foot_rig_asset(
                    binding
                        .endorphin_model
                        .as_deref()
                        .ok_or("character anatomy absent")?,
                )?
            };
            sources.push((entity, body, legs, Some(binding)));
        }
        self.endorphin.feet.body_owners = owners;
        Ok(sources)
    }
}
pub(super) fn validate_leg_skeleton(
    skeleton: &newviso_model::ModelSkeleton,
    legs: &[LegBinding],
) -> Result<(), String> {
    newviso_character::validate_foot_profile(legs)?;
    let index = |name: &str| {
        skeleton
            .joints
            .iter()
            .position(|j| j.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                format!(
                    "skeleton '{}' has no supporting joint '{name}'",
                    skeleton.name
                )
            })
    };
    let descendant = |mut child: usize, parent: usize| {
        for _ in 0..skeleton.joints.len() {
            let Some(p) = skeleton.joints.get(child).and_then(|joint| joint.parent) else {
                return false;
            };
            child = p as usize;
            if child == parent {
                return true;
            }
        }
        false
    };
    let mut roots = Vec::new();
    for leg in legs {
        let h = index(&leg.hip)?;
        let k = index(&leg.knee)?;
        let f = index(&leg.foot)?;
        let t = index(&leg.toe)?;
        if !descendant(k, h) || !descendant(f, k) || !descendant(t, f) {
            return Err("support profile requires ordered upper/lower/foot/toe descendants".into());
        }
        roots.push(h);
    }
    for (i, &a) in roots.iter().enumerate() {
        for &b in &roots[..i] {
            if descendant(a, b) || descendant(b, a) {
                return Err("supporting limb roots must be independent branches".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
