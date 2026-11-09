use super::*;

impl EngineApplication {
    pub(crate) fn detach_vehicle_part(
        &mut self,
        entity: u64,
        part_index: u32,
    ) -> Result<(), String> {
        let Some(binding) = self.vehicle_presentations.get(&entity).cloned() else {
            return Ok(());
        };
        let Some(part) = binding.parts.iter().find(|part| part.index == part_index) else {
            return Ok(());
        };
        if part.detached_entity.is_some() {
            return Ok(());
        }
        let Some(body) = self
            .physics
            .as_ref()
            .and_then(|physics| physics.vehicle_body_state(entity))
        else {
            return Ok(());
        };
        let Some((position, rotation, scale)) = self.scene.entity_transform_values(entity) else {
            return Ok(());
        };
        let indices = fragment_body::assembly_indices(&binding.parts, part_index);
        let mut names: Vec<String> = binding
            .parts
            .iter()
            .filter(|child| {
                indices.contains(&child.index)
                    && child.visible
                    && child.detached_entity.is_none()
                    && !(child.role == ModelFragmentPartRole::Wheel
                        && child.wheel_release_condition == Some(TireCondition::Rim))
                    && !(matches!(
                        child.role,
                        ModelFragmentPartRole::Glass | ModelFragmentPartRole::Light
                    ) && child.damage >= 1.0)
                    && !(child.role == ModelFragmentPartRole::Breakable
                        && child.index != part_index
                        && child.damage >= 1.0)
            })
            .flat_map(|child| child.mesh_names.iter().cloned())
            .collect();
        names.sort();
        names.dedup();
        // A semantic marker with no mesh binding cannot produce valid debris.
        // Keep it on the vehicle until the importer supplies its real geometry.
        if names.is_empty() {
            if let Some(binding) = self.vehicle_presentations.get_mut(&entity) {
                if let Some(original) = binding
                    .parts
                    .iter_mut()
                    .find(|value| value.index == part_index)
                {
                    original.damage = original.damage.min(0.94);
                    original.presentation_override = true;
                }
            }
            return Ok(());
        }
        let key = self.next_vehicle_debris_key()?;
        let detached = self.scene.upsert_runtime_dynamic_entity(
            &key,
            newviso_scene::SceneRuntimeEntityDesc {
                position,
                rotation_degrees: rotation,
                scale,
                ..Default::default()
            },
        )?;
        let (center, extent, hull) = match self
            .scene
            .install_detached_fragment(entity, detached, &names)
        {
            Ok(value) => value,
            Err(error) => {
                self.scene.remove_runtime_entity(&key)?;
                return Err(error);
            }
        };
        let world_center = vehicle_local_point(position, rotation, scale, center);
        self.scene
            .set_runtime_entity_transform(&key, Some(world_center), None, None)?;
        let offset: [f32; 3] = std::array::from_fn(|axis| world_center[axis] - body.position[axis]);
        let inherited_velocity =
            detached_fragment_velocity(body.linear_velocity, body.angular_velocity, offset);
        let boost_world = rotate_euler_xyz(part.detach_velocity_boost, rotation);
        let velocity: [f32; 3] =
            std::array::from_fn(|axis| inherited_velocity[axis] + boost_world[axis]);
        let half: [f32; 3] = std::array::from_fn(|axis| extent[axis] * scale[axis].abs());
        let members = binding
            .parts
            .iter()
            .filter(|p| {
                indices.contains(&p.index)
                    && p.visible
                    && p.detached_entity.is_none()
                    && !(p.role == ModelFragmentPartRole::Wheel
                        && p.wheel_release_condition == Some(TireCondition::Rim))
                    && !(matches!(
                        p.role,
                        ModelFragmentPartRole::Glass
                            | ModelFragmentPartRole::Light
                            | ModelFragmentPartRole::Breakable
                    ) && p.damage >= 1.0
                        && p.index != part_index)
            })
            .collect::<Vec<_>>();
        let fragment = self.vehicle_fragment_body(entity, &members, center, half, scale, hull);
        let mass = fragment.mass;
        let (relative_velocity, relative_spin) =
            self.vehicle_fragment_relative_motion(entity, part, center, rotation, scale);
        let velocity = std::array::from_fn::<_, 3, _>(|a| velocity[a] + relative_velocity[a]);
        let angular_velocity =
            std::array::from_fn::<_, 3, _>(|a| body.angular_velocity[a] + relative_spin[a]);
        let physics = self.physics.as_mut().expect("physics checked above");
        if let Err(error)=physics.upsert_body_from_script(&json!({
            "entity":detached,"body_kind":"dynamic","shape":{"kind":"box","half_extents":half},
            "position":world_center,"rotation":body.rotation,"linear_velocity":velocity,"angular_velocity":angular_velocity,
            "density":mass/(8.0*half[0]*half[1]*half[2]).max(0.001),
            "mass_properties":{"mass":mass,"center_of_mass":fragment.center_of_mass,
                "inertia_diagonal":fragment.inertia},
            "convex_hulls":fragment.hulls,
            "friction":0.65,"restitution":0.08,
            "linear_damping":if part.role==ModelFragmentPartRole::Wheel {0.11} else {0.035},
            "angular_damping":if part.role==ModelFragmentPartRole::Wheel {0.19} else {0.075},
            "participates_in_queries":true,"casts_contacts":true,"continuous_collision":true
        }),0) {self.scene.remove_runtime_entity(&key)?;return Err(error)}
        if !physics.remove_body_collision_components(entity, &fragment.components) {
            physics.remove_fragment_collision_region(entity, center, extent);
        }
        physics.ignore_collision_until_separated(entity, detached);
        self.register_vehicle_debris(entity, detached, part, key.clone(), half);
        if let Some(debris) = self.vehicle_debris.get_mut(&detached) {
            debris.assembly_parts = members.iter().map(|p| p.name.clone()).collect();
            debris.source_components = fragment.components.clone();
            debris.authored_physics = fragment.authored;
            debris.mass = mass;
            debris.center_of_mass = fragment.center_of_mass;
        }
        if let Some(binding) = self.vehicle_presentations.get_mut(&entity) {
            for child in &mut binding.parts {
                if indices.contains(&child.index) {
                    child.detached_entity = Some(detached);
                    child.loose = false;
                    child.detach_velocity_boost = [0.0; 3];
                    child.presentation_override = true;
                }
            }
        }
        if part.role == ModelFragmentPartRole::Wheel {
            if let Some(definition) = self.vehicles.definition(entity) {
                if let Some(index) = resolve_wheel_index(definition, part.wheel_slot) {
                    self.vehicles
                        .set_tire_condition(entity, index, TireCondition::Missing)?;
                }
            }
        }
        self.scene.set_physics_process_active(detached, true)?;
        let kind = match part.role {
            ModelFragmentPartRole::Door => VehicleEventKind::DoorBrokenOff,
            ModelFragmentPartRole::Bonnet => VehicleEventKind::BonnetBrokenOff,
            ModelFragmentPartRole::Boot => VehicleEventKind::BootBrokenOff,
            ModelFragmentPartRole::Wheel => VehicleEventKind::WheelDetached,
            _ => VehicleEventKind::PartBrokenOff,
        };
        if part.role != ModelFragmentPartRole::Wheel {
            let mut event = VehicleEvent::new(0, entity, kind);
            event.part = Some(part.name.clone());
            event.part_index = Some(part.index);
            event.position = Some(world_center);
            event.magnitude = part.damage.max(1.0);
            self.vehicles.emit_event(event);
        }
        Ok(())
    }
}
