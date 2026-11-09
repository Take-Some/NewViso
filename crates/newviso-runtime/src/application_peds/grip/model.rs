//! Grip model loading and semantic binding, cached independently of per-frame hand loads.
use super::*;

impl EngineApplication {
    pub(in crate::application_peds) fn resolve_ped_grip_model(
        &mut self,
        profile: &mut PedProfile,
    ) -> Result<(), String> {
        if profile.damage_entity.is_some() {
            return Ok(());
        }
        if profile.weapon_grip.model.is_none() {
            let path = &profile.weapon_grip.model_asset;
            if !self.ped_physics.grip_models.contains_key(path) {
                let value = newviso_assets_client::AssetClient::new().json(path)?;
                let model: WeaponGripModel = serde_json::from_value(value)
                    .map_err(|e| format!("invalid grip model '{path}': {e}"))?;
                model.validate()?;
                self.ped_physics.grip_models.insert(path.clone(), model);
            }
            let mut model = self.ped_physics.grip_models[path].clone();
            let joints = self.ped_semantic_joints(&profile.actor_id)?;
            let resolve = |name: &mut String| {
                if let Some(actual) = joints.get(name) {
                    *name = actual.clone();
                }
            };
            resolve(&mut model.holding_joint);
            for receiver in &mut model.hit_receivers {
                for segment in &mut receiver.segments {
                    for joint in segment {
                        resolve(joint);
                    }
                }
            }
            model.validate()?;
            profile.weapon_grip.model = Some(model);
        }
        Ok(())
    }
}
