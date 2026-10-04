use super::*;

mod agents;
mod animation;
mod atmosphere;
mod audio;
mod camera;
mod effects;
mod entities;
mod environment;
mod items;
mod physics;
mod services;
mod vehicles;
mod world_events;
mod world_navigation;
mod world_population;
mod world_simulation;

impl EngineApplication {
    pub(super) fn resolve_script_scene_entity(
        &self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<u64, String> {
        if let Some(entity) = command.get("entity").and_then(Value::as_u64) {
            return Ok(entity);
        }
        let id = command
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                format!(
                    "script command[{index}] {op} requires unsigned integer 'entity' or string 'id'"
                )
            })?;
        self.scene
            .runtime_entity_stable_id(id)
            .ok_or_else(|| format!("script command[{index}] {op} target '{id}' does not exist"))
    }

    pub(super) fn resolve_script_scene_entity_command<'a>(
        &self,
        command: &'a Value,
        index: usize,
        op: &str,
    ) -> Result<(std::borrow::Cow<'a, Value>, u64), String> {
        let entity = self.resolve_script_scene_entity(command, index, op)?;
        Ok((command_with_entity(command, entity), entity))
    }

    pub(super) fn apply_script_commands(&mut self, commands: &[Value]) -> Result<(), String> {
        for (index, command) in commands.iter().enumerate() {
            let op = command
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("script command[{index}] has no string 'op'"))?;
            match op {
                "scene.render.configure"
                | "scene.entity.process_claim.set"
                | "scene.entity.process_rate.set"
                | "scene.entity.visibility.set"
                | "scene.mass_instance.debug.populate_grid"
                | "scene.mass_instance.upsert"
                | "scene.mass_instance.visible.set"
                | "scene.mass_instance.clear"
                | "scene.dynamic_entity.upsert"
                | "scene.entity.damage"
                | "scene.entity.transform.set"
                | "scene.entity.remove" => self.apply_entities_command(command, index, op)?,
                "runtime.configure" | "console.write" | "events.emit" | "platform.cursor.set" => {
                    self.apply_services_command(command, index, op)?
                }
                "audio.route.gain" | "audio.cue.preload" | "audio.cue.play"
                | "audio.listener.set" => self.apply_audio_command(command, index, op)?,
                "physics.world.configure"
                | "physics.body.upsert"
                | "physics.body.destroy"
                | "physics.body.velocity.set"
                | "physics.body.pose.set"
                | "physics.body.impulse"
                | "physics.ballistic.fire" => self.apply_physics_command(command, index, op)?,
                "vehicle.surface_policy.set"
                | "vehicle.upsert"
                | "vehicle.remove"
                | "vehicle.input.set"
                | "vehicle.enabled.set"
                | "vehicle.health.set"
                | "vehicle.specification.set"
                | "vehicle.damage_policy.set"
                | "vehicle.damage.apply"
                | "vehicle.alarm.set"
                | "vehicle.thermal.set"
                | "vehicle.explode"
                | "vehicle.environment.set"
                | "vehicle.fuel_policy.set"
                | "vehicle.fuel.set"
                | "vehicle.tire.set"
                | "vehicle.part.set"
                | "vehicle.tracks.clear"
                | "vehicle.cabin.set"
                | "vehicle.lights.set"
                | "vehicle.audio_fx.configure"
                | "vehicle.occupant.set"
                | "vehicle.access.layout.set"
                | "vehicle.access.reserve"
                | "vehicle.access.release" => self.apply_vehicles_command(command, index, op)?,
                "scene.clear_color.set"
                | "scene.environment.set"
                | "scene.orbit.configure"
                | "scene.sky.dome.set"
                | "scene.sky.time.set"
                | "scene.sky.time_scale.set"
                | "scene.timecycle.state.set"
                | "scene.weather.state.set"
                | "scene.sky_visual.upsert"
                | "scene.sky_visual.remove"
                | "scene.sky.atmosphere.set" => {
                    self.apply_environment_command(command, index, op)?
                }
                "ped.upsert" | "ped.remove" | "ped.damage" | "ped.relationship.set" => {
                    self.apply_ped_command(command, index, op)?;
                }
                "character.world_actor.bind"
                | "character.world_actor.jump"
                | "character.world_actor.unbind"
                | "navigation.configure"
                | "navigation.obstacle.upsert"
                | "navigation.obstacle.remove"
                | "navigation.off_mesh_link.upsert"
                | "navigation.off_mesh_link.remove"
                | "agent.upsert"
                | "agent.remove"
                | "agent.task.set"
                | "agent.task.clear"
                | "agent.tasks.clear"
                | "agent.blackboard.set"
                | "agent.blackboard.remove" => self.apply_agents_command(command, index, op)?,
                "items.definition.upsert"
                | "items.inventory.ensure"
                | "items.inventory.configure"
                | "items.inventory.equip"
                | "items.inventory.unequip"
                | "items.inventory.add"
                | "items.inventory.remove"
                | "items.pickup.upsert"
                | "items.pickup.bind_body"
                | "items.inventory.drop"
                | "items.pickup.remove"
                | "items.pickup.collect" => self.apply_items_command(command, index, op)?,
                "world.clock.configure"
                | "world.clock.set"
                | "world.simulation.configure"
                | "world.observer.upsert"
                | "world.observer.remove"
                | "world.actor.upsert"
                | "world.actor.remove"
                | "world.process.upsert"
                | "world.process.remove"
                | "world.actor.presentation.bind"
                | "world.actor.presentation.unbind" => {
                    self.apply_world_simulation_command(command, index, op)?
                }
                "world.fact.set"
                | "world.fact.remove"
                | "world.event.schedule"
                | "world.event.cancel"
                | "world.reality.record"
                | "world.scenario.reserve"
                | "world.scenario.release" => {
                    self.apply_world_events_command(command, index, op)?
                }
                "world.navigation.node.upsert"
                | "world.navigation.node.remove"
                | "world.navigation.edge.upsert"
                | "world.navigation.edge.remove"
                | "world.travel.start"
                | "world.travel.cancel" => {
                    self.apply_world_navigation_command(command, index, op)?
                }
                "world.population.channel.upsert"
                | "world.population.channel.remove"
                | "world.population.streaming.configure"
                | "world.zone.upsert"
                | "world.zone.remove"
                | "world.model_set.upsert"
                | "world.model_set.remove"
                | "world.scenario_point.upsert"
                | "world.scenario_point.remove"
                | "world.relationship.set"
                | "world.relationship.remove"
                | "world.stimulus.emit"
                | "world.stimulus.clear" => {
                    self.apply_world_population_command(command, index, op)?
                }
                "scene.light.upsert"
                | "scene.cloudhat.keyframe.set"
                | "scene.sky_clouds.set"
                | "scene.volumetric_clouds.set"
                | "scene.atmospheric_cloud_layer.target.set"
                | "scene.atmospheric_cloud_layer.target.clear"
                | "scene.lens_flare.upsert"
                | "scene.lens_flare.remove" => self.apply_atmosphere_command(command, index, op)?,
                "scene.entity.main_view_meshes.set"
                | "scene.entity.joint_mesh.copy"
                | "scene.entity.arm_ik.set"
                | "scene.entity.animation.play"
                | "scene.entity.attach_to_joint"
                | "scene.entity.attach_joint_to_joint"
                | "scene.entity.animation.stop" => {
                    self.apply_animation_command(command, index, op)?
                }
                "scene.camera.set_relative_to_entity" | "scene.camera.set" => {
                    self.apply_camera_command(command, index, op)?
                }
                "scene.particle_effect.spawn_at_joint"
                | "scene.particle_effect.spawn"
                | "scene.particles.spawn"
                | "scene.particles.clear"
                | "scene.surface_mark.add"
                | "scene.surface_marks.clear"
                | "scene.transient_spheres.set"
                | "scene.overlay_quads.set" => self.apply_effects_command(command, index, op)?,
                _ => return Err(unsupported_command(op, index)),
            }
        }
        Ok(())
    }
}

fn rotate_local_vector_degrees(point: [f32; 3], rotation_degrees: [f32; 3]) -> [f32; 3] {
    let mut p = point;
    let rx = rotation_degrees[0].to_radians();
    let ry = rotation_degrees[1].to_radians();
    let rz = rotation_degrees[2].to_radians();

    p = [
        p[0],
        p[1] * rx.cos() - p[2] * rx.sin(),
        p[1] * rx.sin() + p[2] * rx.cos(),
    ];
    p = [
        p[0] * ry.cos() + p[2] * ry.sin(),
        p[1],
        -p[0] * ry.sin() + p[2] * ry.cos(),
    ];
    [
        p[0] * rz.cos() - p[1] * rz.sin(),
        p[0] * rz.sin() + p[1] * rz.cos(),
        p[2],
    ]
}

fn unsupported_command(op: &str, index: usize) -> String {
    format!("script command[{index}] uses unsupported engine command '{op}'")
}

fn command_with_entity(command: &Value, entity: u64) -> std::borrow::Cow<'_, Value> {
    if command.get("entity").and_then(Value::as_u64) == Some(entity) {
        std::borrow::Cow::Borrowed(command)
    } else {
        let mut resolved = command.clone();
        resolved["entity"] = json!(entity);
        std::borrow::Cow::Owned(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_entity_packet_is_borrowed_without_copying_payload() {
        let command = json!({"op":"vehicle.input.set", "entity":42, "input":{"steering":0.25}});
        let resolved = command_with_entity(&command, 42);
        assert!(std::ptr::eq(resolved.as_ref(), &command));
        assert_eq!(resolved["input"], command["input"]);
    }

    #[test]
    fn symbolic_entity_resolution_preserves_the_original_packet() {
        let command =
            json!({"op":"physics.body.upsert", "id":"vehicle.test", "shape":{"kind":"box"}});
        let resolved = command_with_entity(&command, 42);
        assert_eq!(resolved["entity"], 42);
        assert_eq!(resolved["shape"], command["shape"]);
        assert!(command.get("entity").is_none());
    }
}
