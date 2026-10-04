use super::*;

/// Detached geometry belongs to the world, independently of its source car.
/// Particle/fire emission can finish, but the rigid body has no expiry or budget.
#[derive(Clone, Debug)]
pub(crate) struct VehicleDebrisState {
    pub entity: u64,
    pub source_entity: u64,
    pub part: String,
    pub key: String,
    pub created_seconds: f64,
    pub fire_until_seconds: f64,
    pub fire_intensity: f32,
    pub scale: f32,
    pub next_fx_seconds: f64,
    pub emitted: u64,
    effect: Option<String>,
}

impl VehicleDebrisState {
    pub(super) fn runtime_state(&self) -> Value {
        json!({
            "entity": self.entity, "entity_key": self.entity.to_string(),
            "source_entity": self.source_entity, "part": self.part,
            "scene_key": self.key, "created_seconds": self.created_seconds,
            "persistent": true, "expires_seconds": null,
            "fire_until_seconds": self.fire_until_seconds,
            "fire_intensity": self.fire_intensity, "emitted": self.emitted
        })
    }
}

impl EngineApplication {
    pub(super) fn next_vehicle_debris_key(&mut self) -> Result<String, String> {
        loop {
            self.next_vehicle_debris_serial = self
                .next_vehicle_debris_serial
                .checked_add(1)
                .ok_or("vehicle debris identity space exhausted")?;
            let key = format!("world.vehicle_debris.{}", self.next_vehicle_debris_serial);
            if self.scene.runtime_entity_stable_id(&key).is_none() {
                return Ok(key);
            }
        }
    }

    pub(crate) fn vehicle_debris_runtime_state(&self) -> Value {
        Value::Array(
            self.vehicle_debris
                .values()
                .map(|debris| {
                    let mut state = debris.runtime_state();
                    state["rendered"] = json!(self.scene.entity_model_installed(debris.entity));
                    state
                })
                .collect(),
        )
    }

    pub(super) fn advance_vehicle_debris(&mut self) -> Result<(), String> {
        let elapsed = self.elapsed_seconds;
        // An explicit scene removal releases its collider as well. Resting,
        // burning out, repair and source-vehicle removal never enter this path.
        let removed = self
            .vehicle_debris
            .values()
            .filter(|debris| !self.scene.runtime_entity_exists(&debris.key))
            .map(|debris| debris.entity)
            .collect::<Vec<_>>();
        for id in removed {
            if let Some(physics) = self.physics.as_mut() {
                if physics.vehicle_body_state(id).is_some() {
                    physics.destroy_body_from_script(&json!({"entity": id}), 0)?;
                }
            }
            self.vehicle_debris.remove(&id);
        }

        let mut requests = Vec::new();
        for debris in self.vehicle_debris.values_mut() {
            if elapsed >= debris.fire_until_seconds
                || debris.fire_intensity <= 0.01
                || elapsed < debris.next_fx_seconds
            {
                continue;
            }
            let Some(body) = self
                .physics
                .as_ref()
                .and_then(|physics| physics.vehicle_body_state(debris.entity))
            else {
                continue;
            };
            if let Some(effect) = debris.effect.as_deref() {
                requests.push((debris.entity, json!({
                    "asset_ref": effect, "position": body.position, "direction": [0.0, 1.0, 0.0],
                    "inherited_velocity": body.linear_velocity, "scale": debris.scale,
                    "count_scale": debris.fire_intensity, "emission_seconds": 0.08,
                    "seed": (debris.entity as u32).wrapping_add((elapsed * 120.0) as u32)
                })));
            }
            debris.next_fx_seconds = elapsed + 0.08;
        }
        for (id, request) in requests {
            match application_particle_effects::spawn_particle_effect(&mut self.scene, &request, 0)
            {
                Ok(report) => {
                    if let Some(debris) = self.vehicle_debris.get_mut(&id) {
                        debris.emitted += report.emitted as u64;
                        if let Some(binding) =
                            self.vehicle_presentations.get_mut(&debris.source_entity)
                        {
                            binding.audio_fx.damage_emitted += report.emitted as u64;
                        }
                    }
                }
                Err(error) => host::warn(
                    "newviso.vehicle.fx",
                    format!("debris entity={id} effect skipped: {error}"),
                ),
            }
        }
        Ok(())
    }

    pub(super) fn register_vehicle_debris(
        &mut self,
        entity: u64,
        id: u64,
        part: &VehiclePresentationPart,
        key: String,
        extent: [f32; 3],
    ) {
        let elapsed = self.elapsed_seconds;
        let effect = self
            .vehicle_presentations
            .get(&entity)
            .and_then(|binding| binding.audio_fx.damage_effects.get("debris_effect"))
            .cloned();
        let mut size = extent.map(|value| value.abs() * 2.0);
        size.sort_by(f32::total_cmp);
        let scale = ((size[1] + size[2]) * 0.5).clamp(0.15, 2.5);
        self.vehicle_debris.insert(
            id,
            VehicleDebrisState {
                entity: id,
                source_entity: entity,
                part: part.name.clone(),
                key,
                created_seconds: elapsed,
                fire_until_seconds: elapsed
                    + if part.fire_intensity > 0.01 {
                        part.fire_remaining_seconds.max(12.0) as f64
                    } else {
                        0.0
                    },
                fire_intensity: part.fire_intensity,
                scale,
                next_fx_seconds: elapsed,
                emitted: 0,
                effect,
            },
        );
    }
}
