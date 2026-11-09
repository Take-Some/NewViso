use super::*;
use newviso_agent::peds::melee::{ActiveMeleeStrike, MeleeEquipment, MeleeModel};
use newviso_collision::{sweep_strike_capsules, StrikeCapsule, StrikeContact};
mod contacts;
mod sampling;
#[derive(Clone, Debug)]
struct MeleeSample {
    sequence: u64,
    time: f64,
    capsules: Vec<StrikeCapsule>,
}
#[derive(Clone, Debug)]
struct MeleeReceiver {
    actor: String,
    part: Option<String>,
    region: Option<String>,
    entity: u64,
    capsule: StrikeCapsule,
}
impl MeleeReceiver {
    fn key(&self) -> (String, Option<String>, u64) {
        (self.actor.clone(), self.part.clone(), self.entity)
    }
}
#[derive(Debug, Default, serde::Serialize)]
pub(super) struct MeleeRuntime {
    #[serde(skip)]
    models: BTreeMap<String, MeleeModel>,
    #[serde(skip)]
    samples: BTreeMap<String, MeleeSample>,
    #[serde(skip)]
    receivers: BTreeMap<(String, Option<String>, u64), StrikeCapsule>,
    sweeps: u64,
    body_contacts: u64,
    world_blocks: u64,
    stale_cancellations: u64,
    pending_poses: u64,
    last_contact: Option<Value>,
}
impl MeleeRuntime {
    pub(super) fn invalidate(&mut self, id: &str) {
        self.samples.remove(id);
        self.receivers.retain(|(actor, _, _), _| actor != id);
    }
}
impl EngineApplication {
    pub(super) fn suspend_unavailable_ped_melee(&mut self, id: &str) {
        if !self.ped_combat_authority(id)
            && self
                .peds
                .suspend_melee(id, self.living_world.world_seconds())
        {
            self.ped_physics.melee.invalidate(id);
        }
    }

    pub(crate) fn cancel_ped_melee(&mut self, id: &str) {
        self.peds
            .cancel_melee(id, self.living_world.world_seconds());
        self.ped_physics.melee.invalidate(id);
    }

    pub(super) fn combat_body_entity(&self, id: &str) -> Option<u64> {
        self.peds
            .state(id)
            .and_then(|p| p.profile.damage_entity)
            .or_else(|| self.ped_physics.bodies.get(id).map(|b| b.entity))
    }
    pub(super) fn clear_combat_line(
        &self,
        from: &str,
        to: &str,
        origin: [f32; 3],
        goal: [f32; 3],
    ) -> bool {
        let Some(physics) = self.physics.as_ref() else {
            return false;
        };
        let Some(source) = self.combat_body_entity(from) else {
            return false;
        };
        let Some(target) = self.combat_body_entity(to) else {
            return false;
        };
        let min = std::array::from_fn(|i| origin[i].min(goal[i]) - 0.02);
        let max = std::array::from_fn(|i| origin[i].max(goal[i]) + 0.02);
        let solids = self.scene.physics_static_solid_aabbs_near(&[(min, max)]);
        let delta = std::array::from_fn(|i| goal[i] - origin[i]);
        physics
            .character_sweep_sphere(origin, delta, 0.015, Some(source), &solids)
            .is_none_or(|hit| hit.entity == Some(target))
    }

    fn load_melee_model(&mut self, path: &str) -> Result<MeleeModel, String> {
        if !self.ped_physics.melee.models.contains_key(path) {
            let value = newviso_assets_client::AssetClient::new().json(path)?;
            let model: MeleeModel = serde_json::from_value(value)
                .map_err(|e| format!("invalid melee model '{path}': {e}"))?;
            model.validate()?;
            self.ped_physics.melee.models.insert(path.into(), model);
        }
        Ok(self.ped_physics.melee.models[path].clone())
    }
    pub(super) fn resolve_ped_melee_model(
        &mut self,
        profile: &mut PedProfile,
    ) -> Result<(), String> {
        let joints = if profile.melee.is_some() && profile.damage_entity.is_none() {
            self.ped_semantic_joints(&profile.actor_id)?
        } else {
            BTreeMap::new()
        };
        if let Some(p) = profile.melee.as_mut() {
            for (semantic, actual) in joints {
                p.joint_map.entry(semantic).or_insert(actual);
            }
            if p.model.is_none() {
                p.model = Some(self.load_melee_model(&p.model_asset)?);
            }
            if let Some(m) = &p.model {
                m.validate()?;
            }
        }
        Ok(())
    }
    pub(super) fn begin_ped_melee(
        &mut self,
        id: &str,
        target: Option<String>,
        direction: [f32; 3],
    ) -> Result<(), String> {
        if !self.ped_combat_authority(id) {
            return Ok(());
        }
        let Some(ped) = self.peds.state(id) else {
            return Ok(());
        };
        let Some(profile) = ped.profile.melee.as_ref() else {
            return Ok(());
        };
        let owner = profile
            .inventory_owner
            .clone()
            .unwrap_or_else(|| id.to_owned());
        let mut model = profile
            .model
            .clone()
            .ok_or("melee model was not resolved")?;
        let mut damage = profile.damage;
        let sequence = ped.melee.as_ref().map_or(0, |s| s.attacks);
        let item = self.items.equipped_item(&owner, "hands").map(str::to_owned);
        let equipment = if let Some(item) = item {
            let definition = self
                .items
                .definition(&item)
                .ok_or("equipped melee definition missing")?;
            let weapon = definition.metadata.get("weapon");
            let asset = weapon
                .and_then(|w| w.get("melee_model"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            if weapon.and_then(|w| w.get("type")).and_then(Value::as_str) == Some("melee")
                && asset.is_none()
            {
                return Err(format!("melee item '{item}' has no authored attack model"));
            }
            damage = weapon
                .and_then(|w| w.get("melee_damage"))
                .and_then(Value::as_f64)
                .map_or(damage, |v| v as f32);
            if let Some(asset) = asset {
                model = self.load_melee_model(&asset)?;
            }
            Some(MeleeEquipment {
                owner_id: owner.clone(),
                generation: self.items.equipped_generation(&owner, "hands"),
                item_id: Some(item),
            })
        } else {
            Some(MeleeEquipment {
                owner_id: owner.clone(),
                item_id: None,
                generation: self.items.equipped_generation(&owner, "hands"),
            })
        };
        let action = model.actions[sequence as usize % model.actions.len()].clone();
        self.peds.begin_melee_action(
            id,
            target,
            direction,
            self.living_world.world_seconds(),
            action,
            damage,
            equipment,
        )?;
        Ok(())
    }
    pub(super) fn apply_melee_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        let id = command
            .get("actor_id")
            .and_then(Value::as_str)
            .ok_or("melee command requires actor_id")?;
        if op == "ped.melee.cancel" {
            self.cancel_ped_melee(id);
            return Ok(());
        }
        let direction = command_vec3(command, "direction", index)?;
        if op == "ped.melee.guard" {
            let enabled = command
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or("melee guard requires enabled")?;
            return self
                .peds
                .melee_guard(id, enabled && self.ped_combat_authority(id), direction);
        }
        if !self
            .living_world
            .actor_runtime_views()
            .iter()
            .any(|a| a.id == id && a.enabled && a.simulation_tier == "full")
        {
            return Ok(());
        }
        let target = command
            .get("target_actor")
            .and_then(Value::as_str)
            .map(str::to_owned);
        self.begin_ped_melee(id, target, direction)
    }
    pub(crate) fn tick_ped_melee(&mut self, dt: f32) -> Result<(), String> {
        // Only phase/stamina update here. Contact requires the final animated pose.
        let now = self.living_world.world_seconds();
        for (id, body) in &self.ped_physics.bodies {
            if !body.authority(self.peds.is_dead(id)).combat && self.peds.suspend_melee(id, now) {
                self.ped_physics.melee.invalidate(id);
            }
        }
        self.peds.advance_melee(now, dt);
        Ok(())
    }
}
