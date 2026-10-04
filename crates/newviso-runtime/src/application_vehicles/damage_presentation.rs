use super::*;

impl EngineApplication {
    pub(crate) fn apply_vehicle_contact_damage(
        &mut self,
        contact: application_physics::PhysicsDamageContact,
    ) -> Result<bool, String> {
        self.apply_vehicle_damage_transaction(contact, None)
    }

    pub(crate) fn apply_vehicle_damage_transaction(
        &mut self,
        contact: application_physics::PhysicsDamageContact,
        component_override: Option<VehicleDamageComponent>,
    ) -> Result<bool, String> {
        self.apply_vehicle_damage_transaction_to_part(contact, component_override, None)
    }

    pub(crate) fn apply_vehicle_damage_transaction_to_part(
        &mut self,
        contact: application_physics::PhysicsDamageContact,
        component_override: Option<VehicleDamageComponent>,
        part_override: Option<u32>,
    ) -> Result<bool, String> {
        if !self.vehicles.contains(contact.target) {
            return Ok(false);
        }
        let Some(definition) = self.vehicles.definition(contact.target).cloned() else {
            return Ok(false);
        };
        let Some((position, rotation_degrees, scale)) =
            self.scene.entity_transform_values(contact.target)
        else {
            return Ok(false);
        };

        let mass = definition.handling.mass.max(1.0);
        // GTA filters ordinary road/support contacts before CVehicleDamage.
        // Preserve that boundary here: only a real impact (or explicit weapon
        // damage) reaches the reference damage transaction.
        let impact_excess = (contact.contact_impulse - mass * 1.5).max(0.0);
        if impact_excess <= 0.0 && contact.direct_damage <= 0.0 {
            return Ok(false);
        }

        let damage_type = match contact.damage_kind {
            application_physics::PhysicsDamageKind::Collision => VehicleDamageType::Collision,
            application_physics::PhysicsDamageKind::Bullet => VehicleDamageType::Bullet,
            application_physics::PhysicsDamageKind::Explosive => VehicleDamageType::Explosive,
            application_physics::PhysicsDamageKind::Fire => VehicleDamageType::Fire,
            application_physics::PhysicsDamageKind::Melee => VehicleDamageType::Melee,
            application_physics::PhysicsDamageKind::Water => VehicleDamageType::Water,
            application_physics::PhysicsDamageKind::Script => VehicleDamageType::Script,
        };
        let raw_damage = if damage_type == VehicleDamageType::Collision {
            impact_excess.max(contact.direct_damage)
        } else {
            contact.direct_damage
        };

        let relative = [
            contact.point[0] - position[0],
            contact.point[1] - position[1],
            contact.point[2] - position[2],
        ];
        let unrotated = inverse_rotate_euler_xyz(relative, rotation_degrees);
        let local_point: [f32; 3] = std::array::from_fn(|axis| {
            let denominator = scale[axis].abs().max(1.0e-4);
            unrotated[axis] / denominator
        });
        let local_direction = inverse_rotate_euler_xyz(contact.impulse_direction, rotation_degrees);

        // Resolve the actual imported fragment child first. Damage semantics,
        // presentation, audio and break-off therefore all refer to the same part.
        let nearest = self
            .vehicle_presentations
            .get(&contact.target)
            .and_then(|binding| {
                let mut nearest = None::<(usize, f32)>;
                for (part_index, part) in binding.parts.iter().enumerate() {
                    if part_override.is_some_and(|index| part.index != index) {
                        continue;
                    }
                    if !part.visible
                        || part.detached_entity.is_some()
                        || matches!(
                            part.role,
                            ModelFragmentPartRole::Glass | ModelFragmentPartRole::Light
                        ) && part.damage >= 1.0
                    {
                        continue;
                    }
                    if component_override.is_some_and(|component| {
                        !vehicle_role_matches_damage_component(part.role, component)
                    }) {
                        continue;
                    }
                    if !matches!(
                        part.role,
                        ModelFragmentPartRole::Body
                            | ModelFragmentPartRole::Wheel
                            | ModelFragmentPartRole::Door
                            | ModelFragmentPartRole::Bonnet
                            | ModelFragmentPartRole::Boot
                            | ModelFragmentPartRole::Glass
                            | ModelFragmentPartRole::BodyPanel
                            | ModelFragmentPartRole::Breakable
                            | ModelFragmentPartRole::Light
                            | ModelFragmentPartRole::Extra
                            | ModelFragmentPartRole::Spoiler
                            | ModelFragmentPartRole::Roof
                    ) {
                        continue;
                    }
                    let dx = local_point[0] - part.pivot[0];
                    let dy = local_point[1] - part.pivot[1];
                    let dz = local_point[2] - part.pivot[2];
                    let distance_sq = self
                        .scene
                        .entity_fragment_surface_hit(contact.target, &part.mesh_names, local_point)
                        .map(|(distance, _, _)| distance)
                        .unwrap_or(dx * dx + dy * dy + dz * dz);
                    let priority = vehicle_damage_surface_priority(part.role);
                    if nearest.is_none_or(|(index, best)| {
                        distance_sq < best - 1.0e-5
                            || (distance_sq - best).abs() <= 1.0e-5
                                && priority
                                    > vehicle_damage_surface_priority(binding.parts[index].role)
                    }) {
                        nearest = Some((part_index, distance_sq));
                    }
                }
                nearest.map(|(index, distance)| {
                    let part = &binding.parts[index];
                    (
                        index,
                        distance,
                        part.role,
                        part.wheel_slot,
                        part.name.clone(),
                    )
                })
            });

        let Some((part_index, distance_sq, part_role, wheel_slot, part_name)) = nearest else {
            if part_override.is_some() {
                return Ok(false);
            }
            let outcome = self.vehicles.apply_damage(
                contact.target,
                VehicleDamageRequest {
                    source_entity: (contact.source != 0).then_some(contact.source),
                    damage_type,
                    component: component_override.unwrap_or(VehicleDamageComponent::Body),
                    raw_damage,
                    local_position: local_point,
                    local_direction,
                    local_normal: local_direction.map(|v| -v),
                    contact_impulse: contact.contact_impulse,
                    upside_down: rotate_euler_xyz([0.0, 1.0, 0.0], rotation_degrees)[1] < 0.0,
                    ..VehicleDamageRequest::default()
                },
            )?;
            return Ok(outcome.effective_damage > 0.0);
        };
        if component_override.is_none()
            && distance_sq
                > if contact.direct_damage > 0.0 {
                    0.36
                } else {
                    12.25
                }
        {
            return Ok(false);
        }

        let raw_damage =
            if damage_type == VehicleDamageType::Collision && contact.direct_damage <= 0.0 {
                let binding = self.vehicle_presentations.get_mut(&contact.target).unwrap();
                binding
                    .impact_history
                    .retain(|_, (time, _)| self.elapsed_seconds - *time < 1.0);
                let key = (contact.source, binding.parts[part_index].index);
                let effective = collision_episode_damage(
                    binding.impact_history.get(&key).copied(),
                    self.elapsed_seconds,
                    impact_excess,
                );
                let peak = binding
                    .impact_history
                    .get(&key)
                    .filter(|(time, _)| self.elapsed_seconds - *time <= 0.18)
                    .map_or(impact_excess, |(_, peak)| peak.max(impact_excess));
                binding
                    .impact_history
                    .insert(key, (self.elapsed_seconds, peak));
                if effective <= 0.0 {
                    return Ok(false);
                }
                effective
            } else {
                raw_damage
            };

        let part_name_lower = part_name.to_ascii_lowercase();
        let component =
            if part_name_lower.contains("engine") || part_name_lower.contains("overheat") {
                VehicleDamageComponent::Engine
            } else if part_name_lower.contains("petrol")
                || part_name_lower.contains("fuel")
                || part_name_lower.contains("tank")
            {
                VehicleDamageComponent::PetrolTank
            } else {
                match part_role {
                    ModelFragmentPartRole::Wheel => resolve_wheel_index(&definition, wheel_slot)
                        .map(VehicleDamageComponent::Wheel)
                        .unwrap_or(VehicleDamageComponent::Body),
                    ModelFragmentPartRole::Glass => VehicleDamageComponent::Glass,
                    ModelFragmentPartRole::Light => VehicleDamageComponent::Light,
                    ModelFragmentPartRole::Door => VehicleDamageComponent::Door,
                    ModelFragmentPartRole::Bonnet => VehicleDamageComponent::Bonnet,
                    ModelFragmentPartRole::Boot => VehicleDamageComponent::Boot,
                    ModelFragmentPartRole::Breakable => VehicleDamageComponent::Breakable,
                    _ => VehicleDamageComponent::Body,
                }
            };
        let component = component_override.unwrap_or(component);
        let world_up = rotate_euler_xyz([0.0, 1.0, 0.0], rotation_degrees);
        let upside_down = world_up[1] < 0.0;
        let body_state = self
            .physics
            .as_ref()
            .and_then(|physics| physics.vehicle_body_state(contact.target));
        let speed_mps = body_state
            .map(|body| {
                let velocity = body.linear_velocity;
                (velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2])
                    .sqrt()
            })
            .unwrap_or_default();
        let local_angular_velocity = body_state
            .map(|body| inverse_rotate_euler_xyz(body.angular_velocity, rotation_degrees))
            .unwrap_or([0.0; 3]);
        let mut angular_damage_multiplier = (local_angular_velocity[0].abs() / 2.0).clamp(0.0, 1.0);
        if upside_down {
            angular_damage_multiplier = (angular_damage_multiplier * 10.0).clamp(0.0, 1.0);
        }

        let outcome = self.vehicles.apply_damage(
            contact.target,
            VehicleDamageRequest {
                source_entity: (contact.source != 0).then_some(contact.source),
                part_index: self
                    .vehicle_presentations
                    .get(&contact.target)
                    .and_then(|b| b.parts.get(part_index))
                    .map(|p| p.index),
                glass_laminated: part_role == ModelFragmentPartRole::Glass
                    && (part_name_lower.contains("windscreen")
                        || part_name_lower.contains("windshield")),
                damage_type,
                component,
                raw_damage,
                local_position: local_point,
                local_normal: local_direction.map(|value| -value),
                local_direction,
                contact_impulse: contact.contact_impulse,
                speed_mps,
                upside_down,
            },
        )?;
        if outcome.effective_damage <= 0.0 {
            return Ok(false);
        }

        if definition.class != VehicleClass::Bike
            && !matches!(
                damage_type,
                VehicleDamageType::Fire | VehicleDamageType::Water
            )
        {
            let direct_wheel = matches!(component, VehicleDamageComponent::Wheel(_));
            if !direct_wheel {
                for (index, wheel) in definition.wheels.iter().enumerate() {
                    // Reference GetSuspensionPos uses roughly 1.5 * wheel
                    // radius as the damage sphere around the suspension line.
                    let radius = wheel.radius * 1.5;
                    let distance_sq = (0..3)
                        .map(|axis| (local_point[axis] - wheel.mount_local[axis]).powi(2))
                        .sum::<f32>();
                    if distance_sq < radius * radius {
                        let _ = self.vehicles.apply_suspension_damage(
                            contact.target,
                            index,
                            outcome.effective_damage,
                        )?;
                    }
                }
            }
        }

        let impact = (outcome.effective_damage / 100.0).clamp(0.0, 1.5);
        let strength = (impact * definition.handling.deformation_damage_multiplier).clamp(0.0, 1.5);
        // CVehicleDamage::ApplyDamageToWindows visits every window intersecting
        // the deformation sphere, independently of the primary damaged body part.
        let glass_hits = if matches!(
            damage_type,
            VehicleDamageType::Collision | VehicleDamageType::Explosive
        ) {
            self.vehicle_presentations
                .get(&contact.target)
                .map(|binding| {
                    binding
                        .parts
                        .iter()
                        .filter(|part| {
                            part.role == ModelFragmentPartRole::Glass
                                && part.damage < 1.0
                                && part.visible
                                && part.detached_entity.is_none()
                                && !(component == VehicleDamageComponent::Glass
                                    && part.index == binding.parts[part_index].index)
                        })
                        .filter_map(|part| {
                            let (min, max) = self
                                .scene
                                .entity_fragment_mesh_bounds(contact.target, &part.mesh_names)?;
                            let nearest: [f32; 3] =
                                std::array::from_fn(|i| local_point[i].clamp(min[i], max[i]));
                            let distance = (0..3)
                                .map(|i| (nearest[i] - local_point[i]).powi(2))
                                .sum::<f32>()
                                .sqrt();
                            let radius = 0.15 + strength.min(1.0) * 0.9;
                            if distance >= radius {
                                return None;
                            }
                            let (_, hit, uv) = self.scene.entity_fragment_surface_hit(
                                contact.target,
                                &part.mesh_names,
                                nearest,
                            )?;
                            Some((
                                part.index,
                                part.name_lower.contains("windscreen")
                                    || part.name_lower.contains("windshield"),
                                outcome.effective_damage * (1.0 - distance / radius) * 0.75,
                                hit,
                                uv,
                            ))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for (index, laminated, damage, point, _) in &glass_hits {
            self.vehicles.damage_glass(
                contact.target,
                *index,
                *damage,
                damage_type,
                *laminated,
                *point,
                local_direction.map(|v| -v),
            )?;
        }
        let glass_uv = self
            .scene
            .entity_fragment_surface_hit(
                contact.target,
                &self
                    .vehicle_presentations
                    .get(&contact.target)
                    .unwrap()
                    .parts[part_index]
                    .mesh_names,
                local_point,
            )
            .map(|(_, _, uv)| uv)
            .unwrap_or([0.5; 2]);
        let damage_state = self
            .vehicles
            .damage_state(contact.target)
            .cloned()
            .ok_or_else(|| format!("vehicle {} lost damage state", contact.target))?;
        let Some(binding) = self.vehicle_presentations.get_mut(&contact.target) else {
            return Ok(false);
        };
        binding.body_health = damage_state.body_health;
        binding.engine_health = damage_state.engine_health;
        for (index, _, _, _, uv) in &glass_hits {
            if let Some(part) = binding.parts.iter_mut().find(|p| p.index == *index) {
                if let Some(glass) = damage_state.glass.get(index) {
                    part.damage = glass.damage;
                    part.glass_hit_uv = *uv;
                    part.presentation_override = true;
                }
            }
        }

        if strength > 0.005 && damage_type == VehicleDamageType::Collision {
            let displacement: [f32; 3] =
                std::array::from_fn(|axis| local_direction[axis] * (strength * 0.45).min(0.4));
            let radius = 0.45 + strength.min(1.0) * 0.8;
            if let Some(physics) = self.physics.as_mut() {
                physics.deform_body_hulls(contact.target, local_point, displacement, radius);
            }
            if binding.dents.len() < 32 {
                binding.dents.push(SceneModelDent {
                    point: local_point,
                    displacement,
                    radius,
                });
            } else if let Some(dent) = binding.dents.iter_mut().min_by(|a, b| {
                let distance = |dent: &SceneModelDent| {
                    (0..3)
                        .map(|axis| (dent.point[axis] - local_point[axis]).powi(2))
                        .sum::<f32>()
                };
                distance(a).total_cmp(&distance(b))
            }) {
                dent.displacement = std::array::from_fn(|axis| {
                    (dent.displacement[axis] + displacement[axis]).clamp(-0.55, 0.55)
                });
            }
        }

        let cooldown_ready = self.elapsed_seconds >= binding.next_part_break_seconds || upside_down;
        let body_health = binding.body_health;
        let part = &mut binding.parts[part_index];
        let before = part.damage;
        let fragility = match part.role {
            ModelFragmentPartRole::Wheel => 2.5,
            ModelFragmentPartRole::Glass => 2.6,
            ModelFragmentPartRole::Breakable | ModelFragmentPartRole::Light => 2.0,
            ModelFragmentPartRole::Door
            | ModelFragmentPartRole::Bonnet
            | ModelFragmentPartRole::Boot => 1.25,
            _ => 1.0,
        };
        let accumulated = if part.role == ModelFragmentPartRole::Glass {
            part.glass_hit_uv = glass_uv;
            damage_state
                .glass
                .get(&part.index)
                .map_or(part.damage, |g| g.damage)
        } else {
            part.damage + impact.min(1.0) * fragility
        };
        // Detachable fragment children only reach 1.0 through an actual break
        // transition. Generic health accumulation can bend/loosen them but no
        // longer silently converts ordinary damage into detached debris.
        part.damage =
            if vehicle_part_detachable(part.role) && part.role != ModelFragmentPartRole::Wheel {
                accumulated.clamp(0.0, 0.94)
            } else {
                accumulated.clamp(0.0, 1.0)
            };

        let sample = vehicle_damage_sample(contact.target, part.index, local_point, 0x41);
        let secondary_sample = vehicle_damage_sample(contact.target, part.index, local_point, 0xA7);
        let has_latch_signal = outcome
            .signals
            .iter()
            .any(|signal| signal.kind == VehicleEventKind::DoorLatchLoosened);
        let mut reset_break_cooldown = false;

        if matches!(
            part.role,
            ModelFragmentPartRole::Door
                | ModelFragmentPartRole::Bonnet
                | ModelFragmentPartRole::Boot
        ) && cooldown_ready
            && !part.locked
        {
            let side = if part.pivot[0] < 0.0 { -1.0 } else { 1.0 };
            let local_normal = local_direction.map(|value| -value);
            let side_push = side * local_direction[0] * outcome.effective_damage;
            let loosen_side_hit = side_push > damage_threshold(15.0, angular_damage_multiplier);
            let pop_side_hit = side_push > damage_threshold(50.0, angular_damage_multiplier);
            let loosen_threshold = damage_threshold(10.0, angular_damage_multiplier);
            let pop_threshold = damage_threshold(40.0, angular_damage_multiplier);
            let loosen_chance =
                damage_probability(0.8, outcome.effective_damage, angular_damage_multiplier);
            let pop_chance = damage_probability(
                if part.role == ModelFragmentPartRole::Bonnet {
                    0.5
                } else {
                    0.5
                },
                outcome.effective_damage,
                angular_damage_multiplier,
            );
            let damage_can_move_door = matches!(
                damage_type,
                VehicleDamageType::Collision
                    | VehicleDamageType::Explosive
                    | VehicleDamageType::Melee
                    | VehicleDamageType::Bullet
            );
            let loosen = damage_can_move_door
                && (has_latch_signal
                    || outcome.effective_damage > loosen_threshold
                    || loosen_side_hit)
                && sample < loosen_chance;
            if loosen && !part.loose {
                // SetLooseLatch: the door remains retained, but its closed pose
                // opens by the authored loose-latch angle.
                part.loose = true;
                part.open = part.open.max(loose_latched_ratio(part.role));
                if let Some(motion) = part.door_motion.as_mut() {
                    motion.latched = true;
                    motion.driven = false;
                    motion.swinging = false;
                    motion.current_speed = 0.0;
                    motion.target_ratio = part.open;
                    motion.break_stress = (motion.break_stress + impact * 0.18).clamp(0.0, 2.0);
                }
                reset_break_cooldown = true;
            } else if part.loose {
                let bullet_only_loosen = damage_type == VehicleDamageType::Bullet
                    && part.role == ModelFragmentPartRole::Door;
                let melee_bonnet_block = damage_type == VehicleDamageType::Melee
                    && part.role == ModelFragmentPartRole::Bonnet;
                let bonnet_side_block =
                    part.role == ModelFragmentPartRole::Bonnet && local_normal[0].abs() > 0.6;
                let can_pop = damage_can_move_door
                    && !bullet_only_loosen
                    && !melee_bonnet_block
                    && !bonnet_side_block
                    && (outcome.effective_damage > pop_threshold || pop_side_hit)
                    && secondary_sample < pop_chance;
                if can_pop {
                    // Second transaction: release the loose latch and let the
                    // articulated hinge swing. Break-off is handled later by
                    // hinge over-limit/deformation, as in CCarDoor physics.
                    if let Some(motion) = part.door_motion.as_mut() {
                        motion.latched = false;
                        motion.driven = false;
                        motion.swinging = true;
                        let impulse_sign = (local_direction[0] * side + local_direction[2] * 0.35)
                            .clamp(-1.0, 1.0);
                        motion.current_speed = (motion.current_speed
                            + impulse_sign * (0.45 + impact * 1.7))
                            .clamp(-3.2, 3.2);
                        motion.target_ratio = part.open;
                        motion.break_stress = (motion.break_stress + impact * 0.35).clamp(0.0, 2.0);
                    }
                    part.open = part.open.max(0.08);
                    reset_break_cooldown = true;
                }
            }
        }

        if matches!(
            part.role,
            ModelFragmentPartRole::BodyPanel
                | ModelFragmentPartRole::Breakable
                | ModelFragmentPartRole::Extra
                | ModelFragmentPartRole::Spoiler
                | ModelFragmentPartRole::Roof
        ) && matches!(
            damage_type,
            VehicleDamageType::Collision | VehicleDamageType::Explosive
        ) && cooldown_ready
        {
            let lower_name = &part.name_lower;
            let bumper = lower_name.contains("bumper") || lower_name.contains("bump");
            if bumper
                && !part.loose
                && loose_panel_damage_gate(body_health, outcome.effective_damage)
                && sample < 0.5
            {
                // Reference bouncing-panel state: the bumper remains attached
                // to the vehicle but visibly hangs from its mount.
                part.loose = true;
                part.damage = part.damage.max(0.45);
                let mut event = VehicleEvent::new(0, contact.target, VehicleEventKind::PartLoose);
                event.part = Some(part.name.clone());
                event.position = Some(contact.point);
                event.magnitude = outcome.effective_damage;
                self.vehicles.emit_event(event);
                reset_break_cooldown = true;
            }

            let adjusted_break_chance = (0.9f32).max(angular_damage_multiplier).clamp(0.0, 1.0);
            if vehicle_part_damage_detachable(part.role, &part.name_lower)
                && break_panel_damage_gate(body_health, outcome.effective_damage, upside_down)
                && secondary_sample < adjusted_break_chance
            {
                part.damage = 1.0;
                part.loose = false;
                let boost_scale = (outcome.effective_damage / 120.0).clamp(0.25, 1.8);
                part.detach_velocity_boost = [
                    local_direction[0] * boost_scale * 2.4,
                    0.7 + boost_scale * 1.2,
                    local_direction[2] * boost_scale * 2.4,
                ];
                reset_break_cooldown = true;
            }
        }

        if matches!(
            damage_type,
            VehicleDamageType::Fire | VehicleDamageType::Explosive
        ) && !matches!(
            part.role,
            ModelFragmentPartRole::Glass | ModelFragmentPartRole::Light
        ) {
            let ignition = if damage_type == VehicleDamageType::Explosive {
                (0.72 + impact * 0.22).clamp(0.72, 1.0)
            } else {
                (0.42 + impact * 0.35).clamp(0.42, 0.92)
            };
            part.fire_intensity = part.fire_intensity.max(ignition);
            part.fire_remaining_seconds = part.fire_remaining_seconds.max(
                if damage_type == VehicleDamageType::Explosive {
                    8.0
                } else {
                    5.0
                } + impact * 5.0,
            );
        }
        part.presentation_override = true;

        // Semantic state transitions are authoritative over generic presentation
        // accumulation. They match the transaction that produced them.
        for signal in &outcome.signals {
            match signal.kind {
                VehicleEventKind::TyrePunctured => {
                    if signal.wheel_index == resolve_wheel_index(&definition, part.wheel_slot) {
                        part.damage = part.damage.max(0.3);
                    }
                }
                VehicleEventKind::TyreBurst => {
                    if signal.wheel_index == resolve_wheel_index(&definition, part.wheel_slot) {
                        part.damage = part.damage.max(0.7);
                    }
                }
                VehicleEventKind::WheelDetached if part.role == ModelFragmentPartRole::Wheel => {
                    part.damage = 1.0;
                }
                VehicleEventKind::GlassBroken if part.role == ModelFragmentPartRole::Glass => {
                    part.damage = 1.0;
                }
                VehicleEventKind::LightSmashed if part.role == ModelFragmentPartRole::Light => {
                    part.damage = 1.0;
                }
                VehicleEventKind::PartBrokenOff if vehicle_part_detachable(part.role) => {
                    part.damage = 1.0;
                    part.loose = false;
                    reset_break_cooldown = true;
                }
                VehicleEventKind::DoorBrokenOff if part.role == ModelFragmentPartRole::Door => {
                    part.damage = 1.0;
                }
                VehicleEventKind::BonnetBrokenOff if part.role == ModelFragmentPartRole::Bonnet => {
                    part.damage = 1.0;
                }
                VehicleEventKind::BootBrokenOff if part.role == ModelFragmentPartRole::Boot => {
                    part.damage = 1.0;
                }
                _ => {}
            }
        }
        if reset_break_cooldown && !upside_down {
            binding.next_part_break_seconds = self.elapsed_seconds + 1.0;
        }

        let glass_broken = matches!(
            part.role,
            ModelFragmentPartRole::Glass | ModelFragmentPartRole::Light
        ) && part.damage >= 1.0;

        if self.elapsed_seconds >= binding.audio_fx.next_impact_seconds {
            let effect = if glass_broken {
                // GlassBroken/LightSmashed emit the authored burst once on transition.
                None
            } else {
                if part.role == ModelFragmentPartRole::Glass {
                    binding.audio_fx.glass_effect.as_deref()
                } else if damage_type == VehicleDamageType::Bullet {
                    binding
                        .audio_fx
                        .damage_effects
                        .get("bullet_impact_effect")
                        .map(String::as_str)
                        .or(binding.audio_fx.impact_effect.as_deref())
                } else {
                    binding.audio_fx.impact_effect.as_deref()
                }
            };
            if let Some(effect) = effect {
                let request = json!({
                    "asset_ref": effect,
                    "position": contact.point,
                    "direction": contact.impulse_direction.map(|value| -value),
                    "scale": 1.0,
                    "count_scale": if part.role == ModelFragmentPartRole::Glass {0.25}
                        else {(0.35 + impact.min(1.0) * 0.65).clamp(0.35,1.0)},
                    "inherited_velocity": self.physics
                        .as_ref()
                        .map(|physics| physics.body_linear_velocity(contact.target))
                        .unwrap_or([0.0; 3]),
                    "seed": (contact.target as u32)
                        .wrapping_add((self.elapsed_seconds * 1000.0) as u32)
                });
                match application_particle_effects::spawn_particle_effect(
                    &mut self.scene,
                    &request,
                    0,
                ) {
                    Ok(report) => binding.audio_fx.impact_emitted += report.emitted as u64,
                    Err(error) => host::warn(
                        "newviso.vehicle.fx",
                        format!("impact effect skipped: {error}"),
                    ),
                }
            }
            binding.audio_fx.next_impact_seconds = self.elapsed_seconds + 0.18;
        }

        if self.elapsed_seconds >= binding.audio_fx.next_impact_audio_seconds {
            let clip = if glass_broken {
                binding.audio_fx.glass_break_clip.clone()
            } else if outcome.effective_damage >= 45.0 {
                binding.audio_fx.impact_heavy_clip.clone()
            } else {
                binding.audio_fx.impact_clip.clone()
            };
            if let Some(clip) = clip {
                let normalized = (outcome.effective_damage / 100.0).clamp(0.0, 1.0);
                let gain = (0.22 + normalized * 1.15).clamp(0.0, 1.35);
                let pitch = (1.03 - normalized * 0.09).clamp(0.90, 1.04);
                match play_vehicle_one_shot(
                    &AudioClient::new(),
                    &clip,
                    gain,
                    pitch,
                    contact.target,
                    contact.point,
                ) {
                    Ok(()) => binding.audio_fx.audio_events_emitted += 1,
                    Err(error) => host::warn(
                        "newviso.vehicle.audio",
                        format!(
                            "vehicle entity={} collision audio skipped: {error}",
                            contact.target
                        ),
                    ),
                }
            }
            binding.audio_fx.next_impact_audio_seconds = self.elapsed_seconds + 0.07;
        }

        host::debug(
            "newviso.vehicle.damage",
            format!(
                "vehicle={} part='{}' role={:?} raw={:.2} effective={:.2} body={:.1} engine={:.1} damage={:.3}->{:.3}",
                contact.target,
                part_name,
                part.role,
                raw_damage,
                outcome.effective_damage,
                binding.body_health,
                binding.engine_health,
                before,
                part.damage
            ),
        );

        Ok(part.damage > before + 1.0e-4 || !outcome.signals.is_empty())
    }

    pub(super) fn detach_vehicle_part(
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
        let mut indices = vec![part_index];
        loop {
            let mut changed = false;
            for child in &binding.parts {
                if child
                    .parent_part_index
                    .is_some_and(|parent| indices.contains(&parent))
                    && !indices.contains(&child.index)
                {
                    indices.push(child.index);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut names: Vec<String> = binding
            .parts
            .iter()
            .filter(|child| {
                indices.contains(&child.index)
                    && child.visible
                    && child.detached_entity.is_none()
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
        let mass = match part.role {
            ModelFragmentPartRole::Wheel => 22.0,
            ModelFragmentPartRole::Door => 28.0,
            ModelFragmentPartRole::Bonnet => 18.0,
            ModelFragmentPartRole::Boot => 16.0,
            ModelFragmentPartRole::Roof => 24.0,
            ModelFragmentPartRole::BodyPanel => 12.0,
            ModelFragmentPartRole::Spoiler => 7.0,
            ModelFragmentPartRole::Breakable | ModelFragmentPartRole::Extra => 6.0,
            _ => 8.0,
        };
        let hull = super::collision_hull::fragment_collision_hull(
            hull.into_iter()
                .map(|point| std::array::from_fn(|axis| point[axis] * scale[axis]))
                .collect(),
        );
        let physics = self.physics.as_mut().expect("physics checked above");
        if let Err(error)=physics.upsert_body_from_script(&json!({
            "entity":detached,"body_kind":"dynamic","shape":{"kind":"box","half_extents":half},
            "position":world_center,"rotation":body.rotation,"linear_velocity":velocity,"angular_velocity":body.angular_velocity,
            "density":mass/(8.0*half[0]*half[1]*half[2]).max(0.001),
            "mass_properties":{"mass":mass,"center_of_mass":[0.0,0.0,0.0],
                "inertia_diagonal":std::array::from_fn::<_,3,_>(|axis|mass/3.0*(half[(axis+1)%3].powi(2)+half[(axis+2)%3].powi(2)).max(0.001))},
            "convex_hulls":if hull.len()>=4 {vec![hull]} else {Vec::new()},
            "friction":0.65,"restitution":0.08,
            "linear_damping":if part.role==ModelFragmentPartRole::Wheel {0.11} else {0.035},
            "angular_damping":if part.role==ModelFragmentPartRole::Wheel {0.19} else {0.075},
            "participates_in_queries":true,"casts_contacts":true,"continuous_collision":true
        }),0) {self.scene.remove_runtime_entity(&key)?;return Err(error)}
        physics.remove_fragment_collision_region(entity, center, extent);
        self.register_vehicle_debris(entity, detached, part, key.clone(), half);
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
            event.position = Some(world_center);
            event.magnitude = part.damage.max(1.0);
            self.vehicles.emit_event(event);
        }
        Ok(())
    }

    pub(super) fn advance_vehicle_part_fires(&mut self, entity: u64) {
        let elapsed = self.elapsed_seconds;
        let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
            return;
        };
        let dt = (elapsed - binding.last_damage_update_seconds).clamp(0.0, 0.1) as f32;
        binding.last_damage_update_seconds = elapsed;
        if dt <= 0.0 {
            return;
        }

        for part in &mut binding.parts {
            if part.fire_intensity <= 0.0 || part.detached_entity.is_some() || !part.visible {
                continue;
            }
            let intensity = part.fire_intensity.clamp(0.0, 1.0);
            part.damage = (part.damage + dt * (0.018 + intensity * 0.055)).clamp(0.0, 1.0);
            part.fire_remaining_seconds = (part.fire_remaining_seconds - dt).max(0.0);
            if part.fire_remaining_seconds <= 0.0 {
                part.fire_intensity = (part.fire_intensity - dt * 0.28).max(0.0);
            }
            part.presentation_override = true;
        }

        // Fire spreads only through fragment hierarchy adjacency. This keeps a
        // burning door local instead of turning the whole vehicle into one health blob.
        let sources = binding
            .parts
            .iter()
            .filter(|part| {
                part.fire_intensity >= 0.68
                    && part.damage >= 0.32
                    && part.visible
                    && part.detached_entity.is_none()
            })
            .map(|part| (part.index, part.parent_part_index, part.fire_intensity))
            .collect::<Vec<_>>();
        let mut ignite = Vec::<(u32, f32)>::new();
        for (source_index, source_parent, intensity) in sources {
            for target in &binding.parts {
                if target.fire_intensity > 0.25
                    || !target.visible
                    || target.detached_entity.is_some()
                    || matches!(
                        target.role,
                        ModelFragmentPartRole::Glass | ModelFragmentPartRole::Light
                    )
                {
                    continue;
                }
                let adjacent = target.parent_part_index == Some(source_index)
                    || source_parent == Some(target.index);
                if adjacent {
                    ignite.push((target.index, intensity * 0.52));
                }
            }
        }
        for (index, intensity) in ignite {
            if let Some(part) = binding.parts.iter_mut().find(|part| part.index == index) {
                part.fire_intensity = part.fire_intensity.max(intensity.clamp(0.0, 0.72));
                part.fire_remaining_seconds = part.fire_remaining_seconds.max(4.5);
                part.presentation_override = true;
            }
        }
    }

    pub(super) fn apply_vehicle_fire_event_presentation(
        &mut self,
        entity: u64,
        kind: VehicleEventKind,
    ) {
        let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
            return;
        };
        match kind {
            VehicleEventKind::EngineFireStarted => {
                let preferred = binding
                    .parts
                    .iter()
                    .position(|part| {
                        part.visible
                            && part.detached_entity.is_none()
                            && (part.name_lower.contains("engine")
                                || part.name_lower.contains("overheat"))
                    })
                    .or_else(|| {
                        binding.parts.iter().position(|part| {
                            part.role == ModelFragmentPartRole::Bonnet
                                && part.visible
                                && part.detached_entity.is_none()
                        })
                    })
                    .or_else(|| {
                        binding
                            .parts
                            .iter()
                            .enumerate()
                            .filter(|(_, part)| {
                                part.visible
                                    && part.detached_entity.is_none()
                                    && matches!(
                                        part.role,
                                        ModelFragmentPartRole::Body
                                            | ModelFragmentPartRole::BodyPanel
                                            | ModelFragmentPartRole::Extra
                                    )
                            })
                            .min_by(|(_, a), (_, b)| a.pivot[2].total_cmp(&b.pivot[2]))
                            .map(|(index, _)| index)
                    });
                if let Some(index) = preferred {
                    let part = &mut binding.parts[index];
                    part.fire_intensity = part.fire_intensity.max(0.78);
                    part.fire_remaining_seconds = part.fire_remaining_seconds.max(12.0);
                    part.presentation_override = true;
                }
            }
            VehicleEventKind::PetrolFireStarted => {
                let mut candidates = binding
                    .parts
                    .iter()
                    .enumerate()
                    .filter(|(_, part)| {
                        part.visible
                            && part.detached_entity.is_none()
                            && !matches!(
                                part.role,
                                ModelFragmentPartRole::Glass | ModelFragmentPartRole::Light
                            )
                    })
                    .map(|(index, part)| (index, part.pivot[2]))
                    .collect::<Vec<_>>();
                candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
                for (index, _) in candidates.into_iter().take(2) {
                    let part = &mut binding.parts[index];
                    part.fire_intensity = part.fire_intensity.max(0.84);
                    part.fire_remaining_seconds = part.fire_remaining_seconds.max(12.0);
                    part.presentation_override = true;
                }
            }
            _ => {}
        }
    }

    pub(super) fn apply_vehicle_explosion_presentation(&mut self, entity: u64) {
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

fn vehicle_role_matches_damage_component(
    role: ModelFragmentPartRole,
    component: VehicleDamageComponent,
) -> bool {
    match component {
        VehicleDamageComponent::Glass => role == ModelFragmentPartRole::Glass,
        VehicleDamageComponent::Light => role == ModelFragmentPartRole::Light,
        VehicleDamageComponent::Door => role == ModelFragmentPartRole::Door,
        VehicleDamageComponent::Bonnet => role == ModelFragmentPartRole::Bonnet,
        VehicleDamageComponent::Boot => role == ModelFragmentPartRole::Boot,
        VehicleDamageComponent::Wheel(_) => role == ModelFragmentPartRole::Wheel,
        VehicleDamageComponent::Body => role == ModelFragmentPartRole::Body,
        _ => true,
    }
}

fn vehicle_damage_surface_priority(role: ModelFragmentPartRole) -> u8 {
    match role {
        ModelFragmentPartRole::Glass => 4,
        ModelFragmentPartRole::Light => 3,
        ModelFragmentPartRole::Body => 0,
        _ => 1,
    }
}

/// Persisting manifolds report support impulses each frame. Only a rising impact
/// within a contact episode adds damage; separation opens a new episode.
fn collision_episode_damage(previous: Option<(f64, f32)>, seconds: f64, impulse: f32) -> f32 {
    previous
        .filter(|(time, _)| seconds - *time <= 0.18)
        .map_or(impulse, |(_, peak)| (impulse - peak).max(0.0))
}

#[cfg(test)]
mod collision_episode_tests {
    use super::*;
    #[test]
    fn sustained_contact_is_not_repeated_impact_damage() {
        assert_eq!(
            collision_episode_damage(Some((1.0, 1000.0)), 1.01, 1000.0),
            0.0
        );
        assert_eq!(
            collision_episode_damage(Some((1.0, 1000.0)), 1.01, 1200.0),
            200.0
        );
        assert_eq!(
            collision_episode_damage(Some((1.0, 1000.0)), 1.3, 1200.0),
            1200.0
        );
    }
    #[test]
    fn glass_wins_only_a_surface_tie_and_explicit_body_damage_stays_on_body() {
        assert!(
            vehicle_damage_surface_priority(ModelFragmentPartRole::Glass)
                > vehicle_damage_surface_priority(ModelFragmentPartRole::Door)
        );
        assert!(!vehicle_role_matches_damage_component(
            ModelFragmentPartRole::Door,
            VehicleDamageComponent::Glass
        ));
        assert!(vehicle_role_matches_damage_component(
            ModelFragmentPartRole::Body,
            VehicleDamageComponent::Body
        ));
    }
}
