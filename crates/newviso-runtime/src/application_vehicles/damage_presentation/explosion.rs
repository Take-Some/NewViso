use super::*;

impl EngineApplication {
    pub(crate) fn apply_vehicle_explosion_presentation(&mut self, entity: u64) {
        let definition = self.vehicles.definition(entity).cloned();
        let transform = self.scene.entity_transform_values(entity);
        let mut burst_wheels = Vec::<usize>::new();
        {
            let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
                return;
            };
            for part in &mut binding.parts {
                if !part.visible || part.detached_entity.is_some() {
                    continue;
                }
                let sample = vehicle_damage_sample(entity, part.index, part.pivot, 0xE1);
                let break_probability = match part.role {
                    ModelFragmentPartRole::Door
                    | ModelFragmentPartRole::Bonnet
                    | ModelFragmentPartRole::Boot => 0.50,
                    ModelFragmentPartRole::Wheel => 0.20,
                    role if vehicle_part_damage_detachable(role, &part.name_lower) => 0.50,
                    _ => 0.0,
                };
                match part.role {
                    ModelFragmentPartRole::Glass | ModelFragmentPartRole::Light => {
                        part.damage = 1.0;
                    }
                    ModelFragmentPartRole::Wheel => {
                        if sample < break_probability {
                            part.damage = 1.0;
                        } else {
                            part.damage = part.damage.max(0.72);
                            if let Some(definition) = definition.as_ref() {
                                if let Some(index) =
                                    resolve_wheel_index(definition, part.wheel_slot)
                                {
                                    burst_wheels.push(index);
                                }
                            }
                        }
                    }
                    role if vehicle_part_damage_detachable(role, &part.name_lower) => {
                        if sample < break_probability {
                            part.damage = 1.0;
                            part.loose = false;
                        } else {
                            part.damage = part.damage.max(0.88);
                            if matches!(
                                role,
                                ModelFragmentPartRole::BodyPanel
                                    | ModelFragmentPartRole::Breakable
                                    | ModelFragmentPartRole::Extra
                                    | ModelFragmentPartRole::Spoiler
                                    | ModelFragmentPartRole::Roof
                            ) {
                                part.loose = true;
                            }
                        }
                    }
                    _ => {}
                }

                if part.damage >= 1.0 && vehicle_part_detachable(part.role) {
                    let radial = [
                        part.pivot[0],
                        0.35 + part.pivot[1].abs() * 0.15,
                        part.pivot[2],
                    ];
                    let length =
                        (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2])
                            .sqrt()
                            .max(0.15);
                    let impulse = 4.2 + sample * 3.4;
                    part.detach_velocity_boost =
                        radial.map(|component| component / length * impulse);
                }

                if matches!(
                    part.role,
                    ModelFragmentPartRole::Body
                        | ModelFragmentPartRole::BodyPanel
                        | ModelFragmentPartRole::Bonnet
                        | ModelFragmentPartRole::Boot
                        | ModelFragmentPartRole::Door
                        | ModelFragmentPartRole::Extra
                        | ModelFragmentPartRole::Breakable
                        | ModelFragmentPartRole::Spoiler
                        | ModelFragmentPartRole::Roof
                ) {
                    part.fire_intensity = part.fire_intensity.max(0.75);
                    part.fire_remaining_seconds = part.fire_remaining_seconds.max(8.0);
                }
                part.presentation_override = true;
            }
            binding.wreck_fire_until_seconds = self.elapsed_seconds + 35.0;
            binding.body_health = 0.0;
            binding.engine_health = newviso_vehicle::ENGINE_DAMAGE_FINISHED;
            binding.next_part_break_seconds = 0.0;
        }

        burst_wheels.sort_unstable();
        burst_wheels.dedup();
        for index in burst_wheels {
            if let Err(error) = self
                .vehicles
                .set_tire_condition(entity, index, TireCondition::Rim)
            {
                host::warn(
                    "newviso.vehicle.damage",
                    format!("vehicle entity={entity} explosion tyre burst skipped: {error}"),
                );
            }
        }

        let mass = definition
            .as_ref()
            .map(|definition| definition.handling.mass)
            .unwrap_or(1200.0)
            .max(100.0);
        let body = self
            .physics
            .as_ref()
            .and_then(|physics| physics.vehicle_body_state(entity));
        if let (Some(physics), Some(body)) = (self.physics.as_mut(), body) {
            let side = if entity & 1 == 0 { 1.0 } else { -1.0 };
            let point = transform
                .map(|(position, _, _)| [position[0] + side * 0.55, position[1], position[2]])
                .unwrap_or([
                    body.position[0] + side * 0.55,
                    body.position[1],
                    body.position[2],
                ]);
            let command = json!({
                "entity": entity,
                "impulse": [side * mass * 0.32, mass * 2.25, mass * 0.18],
                "point": point
            });
            if let Err(error) = physics.apply_impulse_from_script(&command, 0) {
                host::warn(
                    "newviso.vehicle.damage",
                    format!("vehicle entity={entity} explosion impulse skipped: {error}"),
                );
            }
        }
    }
}
