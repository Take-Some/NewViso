use super::*;
use crate::application_physics::doors::DoorActor;
impl EngineApplication {
    pub(crate) fn door_actor_access(&self, actor: &str) -> BTreeSet<u64> {
        let player = self.settings.variables.get("engine_player");
        let player_id = player
            .and_then(|p| p.get("scene_id"))
            .and_then(Value::as_str);
        let owner = if player_id == Some(actor) {
            self.settings
                .variables
                .get("engine_items")
                .and_then(|i| i.get("owner_id"))
                .and_then(Value::as_str)
                .unwrap_or("player")
        } else {
            self.peds
                .state(actor)
                .and_then(|p| p.profile.melee.as_ref())
                .and_then(|m| m.inventory_owner.as_deref())
                .unwrap_or(actor)
        };
        self.physics
            .as_ref()
            .map(|p| {
                p.door_key_items()
                    .into_iter()
                    .filter_map(|(id, key)| {
                        key.filter(|key| self.items.inventory_quantity(owner, key) > 0)
                            .map(|_| id)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    fn door_actor_context(&self, actor: &str) -> Option<DoorActor> {
        if self.peds.state(actor).is_some_and(|p| p.dead) || self.ped_has_ragdoll(actor) {
            return None;
        }
        let player = self.settings.variables.get("engine_player");
        let body = self
            .ped_physics
            .bodies
            .get(actor)
            .map(|b| b.entity)
            .or_else(|| self.peds.state(actor).and_then(|p| p.profile.damage_entity))
            .or_else(|| self.physical_characters.native_body_entity(actor))
            .or_else(|| {
                (player
                    .and_then(|p| p.get("scene_id"))
                    .and_then(Value::as_str)
                    == Some(actor))
                .then(|| {
                    player
                        .and_then(|p| p.get("physics_entity"))
                        .and_then(Value::as_u64)
                })
                .flatten()
            })?;
        let physics = self.physics.as_ref()?;
        let (lo, hi) = physics.body_local_bounds(body)?;
        let radius = ((hi[0] - lo[0]).min(hi[2] - lo[2]) * 0.5).max(f32::EPSILON);
        Some(DoorActor {
            id: actor.into(),
            body,
            radius,
            height: hi[1] - lo[1],
            access: self.door_actor_access(actor),
        })
    }
    pub(crate) fn apply_door_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        if op == "door.remove" {
            let entity = self.resolve_script_scene_entity(command, index, op).ok();
            if let Some(id) = entity {
                self.physics
                    .as_mut()
                    .ok_or("doors require physics")?
                    .remove_door(id);
            }
            return Ok(());
        }
        let (value, entity) = self.resolve_script_scene_entity_command(command, index, op)?;
        match op {
            "door.configure" => {
                let mut value = value.into_owned();
                let object = value.as_object_mut().ok_or("invalid door definition")?;
                object.remove("op");
                object.remove("id");
                self.physics
                    .as_mut()
                    .ok_or("doors require physics")?
                    .configure_door(&value)
            }
            "door.remove" => {
                self.physics
                    .as_mut()
                    .ok_or("doors require physics")?
                    .remove_door(entity);
                Ok(())
            }
            "door.state.restore" => {
                let state = serde_json::from_value(
                    command
                        .get("state")
                        .cloned()
                        .ok_or("missing door mechanism state")?,
                )
                .map_err(|e| format!("invalid saved door: {e}"))?;
                self.physics
                    .as_mut()
                    .ok_or("doors require physics")?
                    .door_restore(entity, state)
            }
            "door.interact" => {
                let actor = command
                    .get("actor_id")
                    .and_then(Value::as_str)
                    .ok_or("door interaction requires actor_id")?;
                let action = command
                    .get("action")
                    .and_then(Value::as_str)
                    .ok_or("door interaction requires action")?;
                let context = self.door_actor_context(actor);
                if let Some(context) = context {
                    self.physics
                        .as_mut()
                        .ok_or("doors require physics")?
                        .interact_door(entity, context, action)?;
                } else {
                    self.physics
                        .as_mut()
                        .ok_or("doors require physics")?
                        .deny_door_interaction(entity, actor, action);
                }
                Ok(())
            }
            _ => Err("unknown door command".into()),
        }
    }
}
