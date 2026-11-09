//! Runtime adapter for native grip, equipped ownership and physical release.
use super::*;
use crate::application_physics::math::{
    euler_degrees_to_quaternion, inverse_rotate_camera_vector, rotate_camera_vector,
};
use newviso_agent::peds::grip::{GripLossReason, WeaponGripLoad, WeaponGripModel};
use newviso_items::{EquipmentDrop, PickupBodyPose, PickupOrigin};
mod model;
mod motion;
mod weapon;
use motion::BodyMotionSample;
use weapon::WeaponPhysics;

#[derive(Clone, Debug)]
pub(super) struct GripMotionSample {
    pub item: String,
    pub generation: u64,
    body_motion: BodyMotionSample,
    pub shot_count: u64,
    pub recoil_load: [f32; 3],
    pub weapon_position: [f32; 3],
    pub weapon_rotation: [f32; 4],
    pub body_position: [f32; 3],
    pub hand_velocity: [f32; 3],
    pub release_angular_velocity: [f32; 3],
    pub pose_source: String,
}
#[derive(Clone, Debug)]
struct HeldPose {
    pose: PickupBodyPose,
    source: String,
    visual: Option<u64>,
    body: u64,
    body_position: [f32; 3],
}
fn norm(v: [f32; 3]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn vec3(value: Option<&Value>) -> Option<[f32; 3]> {
    let v = value?.as_array()?;
    if v.len() != 3 {
        return None;
    }
    let result = [
        v[0].as_f64()? as f32,
        v[1].as_f64()? as f32,
        v[2].as_f64()? as f32,
    ];
    result.iter().all(|v| v.is_finite()).then_some(result)
}
/// Shortest quaternion arc; animation velocity contributes only to release momentum.
fn rotation_velocity(previous: [f32; 4], current: [f32; 4], dt: f32) -> [f32; 3] {
    let [ax, ay, az, aw] = current;
    let [bx, by, bz, bw] = [-previous[0], -previous[1], -previous[2], previous[3]];
    let mut q = [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ];
    if q[3] < 0.0 {
        q = q.map(|v| -v);
    }
    let length = norm([q[0], q[1], q[2]]);
    if length <= f32::EPSILON || dt <= 0.0 {
        return [0.0; 3];
    }
    let scale = 2.0 * length.atan2(q[3].clamp(-1.0, 1.0)) / (length * dt);
    [q[0] * scale, q[1] * scale, q[2] * scale]
}
impl EngineApplication {
    pub(crate) fn invalidate_ped_grip_motion(&mut self, id: &str) {
        self.ped_physics.grip_motion.remove(id);
    }

    /// Rank authored anatomical segments using the current animated joint positions.
    pub(super) fn ped_grip_contact_region(
        &self,
        id: &str,
        point: [f32; 3],
    ) -> Option<(
        super::hit_zones::HitZone,
        newviso_agent::peds::grip::GripImpact,
    )> {
        use newviso_agent::peds::grip::GripImpact;
        let ped = self.peds.state(id)?;
        let model = ped.profile.weapon_grip.model.as_ref()?;
        let parent = self
            .world_actor_presentations
            .get(id)
            .and_then(|p| self.scene.runtime_entity_stable_id(&p.scene_key))?;
        let mut best: Option<(f32, &str)> = None;
        for receiver in &model.hit_receivers {
            for [from, to] in &receiver.segments {
                let (Ok(a), Ok(b)) = (
                    self.scene.entity_joint_world_position(parent, from),
                    self.scene.entity_joint_world_position(parent, to),
                ) else {
                    continue;
                };
                let segment: [f32; 3] = std::array::from_fn(|i| b[i] - a[i]);
                let n2 = segment
                    .iter()
                    .map(|v| v * v)
                    .sum::<f32>()
                    .max(f32::MIN_POSITIVE);
                let t = ((0..3).map(|i| (point[i] - a[i]) * segment[i]).sum::<f32>() / n2)
                    .clamp(0.0, 1.0);
                let d = norm(std::array::from_fn(|i| point[i] - a[i] - t * segment[i]))
                    / (receiver.radius_scale * ped.profile.capsule_radius);
                if best.is_none_or(|(old, _)| d < old) {
                    best = Some((d, &receiver.region));
                }
            }
        }
        let (_, region) = best?;
        let zone = super::hit_zones::HitZone::parse(region)?;
        let impact = match region {
            "left_arm" | "left_hand" => GripImpact::LeftArm,
            "right_arm" | "right_hand" => GripImpact::RightArm,
            "arm" | "hand" => GripImpact::Arm,
            _ => GripImpact::Body,
        };
        Some((zone, impact))
    }

    fn held_weapon_pose(
        &self,
        id: &str,
        definition: &ItemDefinition,
        model: &WeaponGripModel,
    ) -> Option<HeldPose> {
        let body = self.ped_physics.bodies.get(id)?;
        let native = self.physics.as_ref()?.vehicle_body_state(body.entity)?;
        let parent = self
            .world_actor_presentations
            .get(id)
            .and_then(|p| self.scene.runtime_entity_stable_id(&p.scene_key))
            .unwrap_or(body.entity);
        let expected = definition
            .metadata
            .pointer("/presentation/model")
            .and_then(Value::as_str);
        let visual = self
            .scene
            .joint_attachment_children(parent)
            .into_iter()
            .find(|entity| {
                self.scene
                    .entity_state(*entity)
                    .filter(|s| s["residency"] == "resident")
                    .and_then(|s| s.get("asset_ref").cloned())
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .as_deref()
                    == expected
            });
        let (position, rotation, source) = if let Some(transform) = visual
            .and_then(|v| self.scene.entity_state(v))
            .and_then(|s| s.get("transform").cloned())
        {
            (
                vec3(transform.get("position"))?,
                euler_degrees_to_quaternion(vec3(transform.get("rotation_degrees"))?),
                "weapon_attachment",
            )
        } else if let Ok((position, degrees)) = self
            .scene
            .entity_joint_world_pose(parent, &model.holding_joint)
        {
            (
                position,
                euler_degrees_to_quaternion(degrees),
                "hand_socket",
            )
        } else {
            (native.position, native.rotation, "body_pose")
        };
        let offset = std::array::from_fn(|i| position[i] - native.position[i]);
        let rotation_velocity = cross(native.angular_velocity, offset);
        let velocity = std::array::from_fn(|i| native.linear_velocity[i] + rotation_velocity[i]);
        Some(HeldPose {
            pose: PickupBodyPose {
                position,
                rotation,
                linear_velocity: velocity,
                angular_velocity: native.angular_velocity,
            },
            source: source.into(),
            visual,
            body: body.entity,
            body_position: native.position,
        })
    }

    pub(crate) fn tick_ped_weapon_grips(&mut self, dt: f32) -> Result<(), String> {
        if dt <= 0.0 {
            return Ok(());
        }
        let ids = self
            .peds
            .states()
            .filter(|p| p.profile.damage_entity.is_none())
            .map(|p| p.profile.actor_id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            let model = self
                .peds
                .state(&id)
                .and_then(|p| p.profile.weapon_grip.model.as_ref());
            let support = model.map_or([1.0; 2], |m| self.ped_weapon_support(&id, m));
            let equipped = self.items.equipped_item(&id, "hands").map(str::to_owned);
            // Tissue recovery is independent of ownership and streaming; physical load needs a ready body.
            if !self.ped_grip_ready(&id) || equipped.is_none() {
                self.peds.advance_unheld_weapon_grip(&id, support, dt)?;
                self.ped_physics.grip_motion.remove(&id);
                continue;
            }
            let item = equipped.expect("equipped item checked");
            let Some(definition) = self
                .items
                .definition(&item)
                .filter(|d| d.category == "weapon")
                .cloned()
            else {
                continue;
            };
            let Some(ped) = self.peds.state(&id) else {
                continue;
            };
            let Some(model) = ped.profile.weapon_grip.model.clone() else {
                continue;
            };
            let generation = self.items.equipped_generation(&id, "hands");
            let Some(mut held) = self.held_weapon_pose(&id, &definition, &model) else {
                continue;
            };
            if ped.dead {
                self.release_ped_equipment(
                    &id,
                    &item,
                    generation,
                    GripLossReason::Death,
                    &definition,
                    &model,
                    held,
                )?;
                continue;
            }
            let physical = WeaponPhysics::read(&definition)?;
            let Some(native) = self
                .physics
                .as_ref()
                .and_then(|p| p.vehicle_body_state(held.body))
            else {
                continue;
            };
            let shot_count = ped.combat.as_ref().map_or(0, |c| c.shots);
            let same = ped.weapon_grip.held_item.as_deref() == Some(item.as_str())
                && ped.weapon_grip.held_generation == generation;
            if !same {
                if let Some(visual) = held.visual {
                    self.scene
                        .set_entity_visibility(visual, "native.weapon_release", true)?;
                }
                if let Some(ammo) = self
                    .items
                    .item_state(&id, &item)
                    .and_then(|s| s.get("weapon"))
                {
                    let magazine = ammo.get("magazine").and_then(Value::as_u64);
                    let reserve = ammo.get("reserve").and_then(Value::as_u64);
                    if let (Some(magazine), Some(reserve)) = (magazine, reserve) {
                        self.peds.restore_combat_ammo(&id, magazine, reserve)?;
                    }
                }
            }
            let previous = self
                .ped_physics
                .grip_motion
                .get(&id)
                .filter(|p| same && p.item == item && p.generation == generation);
            let weight = 1.0 - (-dt / model.motion_response_seconds).exp();
            let mut sample = GripMotionSample {
                item: item.clone(),
                generation,
                body_motion: {
                    let (tick, fixed_dt) = self
                        .physics
                        .as_ref()
                        .expect("resident physics")
                        .fixed_clock();
                    BodyMotionSample::observe(
                        previous.map(|p| &p.body_motion),
                        tick,
                        fixed_dt,
                        native.linear_velocity,
                        native.angular_velocity,
                        model.motion_response_seconds,
                    )
                },
                shot_count,
                recoil_load: [0.0; 3],
                weapon_position: held.pose.position,
                weapon_rotation: held.pose.rotation,
                body_position: native.position,
                hand_velocity: [0.0; 3],
                release_angular_velocity: native.angular_velocity,
                pose_source: held.source.clone(),
            };
            if let Some(previous) = previous {
                let recoil = definition
                    .metadata
                    .pointer("/weapon/impulse")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as f32;
                let new_shots = shot_count.saturating_sub(previous.shot_count);
                let axis = rotate_camera_vector([0.0, 0.0, -1.0], held.pose.rotation);
                for i in 0..3 {
                    sample.recoil_load[i] = previous.recoil_load[i] * (1.0 - weight)
                        + axis[i] * new_shots as f32 * recoil / model.motion_response_seconds;
                }
            }
            if let Some(previous) = previous.filter(|p| p.pose_source == held.source) {
                let rotation_delta =
                    rotation_velocity(previous.weapon_rotation, held.pose.rotation, dt);
                for i in 0..3 {
                    let relative =
                        (held.pose.position[i] - previous.weapon_position[i] - native.position[i]
                            + previous.body_position[i])
                            / dt;
                    sample.hand_velocity[i] =
                        previous.hand_velocity[i] + weight * (relative - previous.hand_velocity[i]);
                    sample.release_angular_velocity[i] = previous.release_angular_velocity[i]
                        + weight * (rotation_delta[i] - previous.release_angular_velocity[i]);
                    held.pose.linear_velocity[i] =
                        native.linear_velocity[i] + sample.hand_velocity[i];
                }
                held.pose.angular_velocity = sample.release_angular_velocity;
            }
            let presentation = &definition.metadata["presentation"];
            let primary = vec3(presentation.pointer("/aim/grip")).unwrap_or([0.0; 3]);
            let secondary = vec3(presentation.pointer("/aim/support"));
            let com = physical.center();
            let lever_local = std::array::from_fn(|i| com[i] - primary[i]);
            let lever = rotate_camera_vector(lever_local, held.pose.rotation);
            let tangential = cross(sample.body_motion.angular_acceleration, lever);
            let centrifugal = cross(
                native.angular_velocity,
                cross(native.angular_velocity, lever),
            );
            let gravity = self.physics.as_ref().map_or(0.0, |p| p.gravity_magnitude());
            let acceleration: [f32; 3] = std::array::from_fn(|i| {
                sample.body_motion.acceleration[i]
                    + tangential[i]
                    + centrifugal[i]
                    + if i == 1 { gravity } else { 0.0 }
            });
            let force =
                std::array::from_fn(|i| acceleration[i] * physical.mass + sample.recoil_load[i]);
            let torque = cross(lever, force);
            let inertia = physical.inertia();
            let rotational = rotate_camera_vector(
                std::array::from_fn(|i| {
                    inertia[i]
                        * inverse_rotate_camera_vector(
                            sample.body_motion.angular_acceleration,
                            held.pose.rotation,
                        )[i]
                }),
                held.pose.rotation,
            );
            let mut support = self.ped_weapon_support(&id, &model);
            if secondary.is_none() {
                support[1] = 0.0;
            }
            let load = WeaponGripLoad {
                force: norm(force),
                torque: norm(std::array::from_fn(|i| torque[i] + rotational[i])),
                support_span: secondary
                    .map_or(0.0, |s| norm(std::array::from_fn(|i| s[i] - primary[i]))),
                support,
            };
            self.ped_physics.grip_motion.insert(id.clone(), sample);
            self.peds
                .advance_weapon_grip(&id, &item, generation, load, dt)?;
            if self.peds.state(&id).is_some_and(|p| p.weapon_grip.lost) {
                self.release_ped_equipment(
                    &id,
                    &item,
                    generation,
                    GripLossReason::SupportFailure,
                    &definition,
                    &model,
                    held,
                )?;
            }
        }
        Ok(())
    }

    fn release_ped_equipment(
        &mut self,
        id: &str,
        item: &str,
        generation: u64,
        reason: GripLossReason,
        definition: &ItemDefinition,
        model: &WeaponGripModel,
        mut held: HeldPose,
    ) -> Result<bool, String> {
        let physical = WeaponPhysics::read(definition)?;
        // Source-equivalent obstruction probe; use the actual weapon bounds, not a fixed offset.
        let center = physical.center();
        let radius = norm(std::array::from_fn(|i| {
            (physical.bounds_max[i] - center[i])
                .abs()
                .max((physical.bounds_min[i] - center[i]).abs())
        }));
        let delta = std::array::from_fn(|i| held.pose.position[i] - held.body_position[i]);
        let min =
            std::array::from_fn(|i| held.body_position[i].min(held.pose.position[i]) - radius);
        let max =
            std::array::from_fn(|i| held.body_position[i].max(held.pose.position[i]) + radius);
        let solids = self.scene.physics_static_solid_aabbs_near(&[(min, max)]);
        if let Some(physics) = self.physics.as_ref() {
            let own = self
                .ped_physics
                .bodies
                .get(id)
                .and_then(|b| b.endorphin.ragdoll.as_ref())
                .map_or(std::slice::from_ref(&held.body), |rig| rig.entities());
            if let Some(hit) = physics.character_sweep_sphere_ignoring(
                held.body_position,
                delta,
                radius,
                own,
                &solids,
            ) {
                held.pose.position = std::array::from_fn(|i| {
                    held.body_position[i]
                        + delta[i] * hit.fraction
                        + hit.normal[i] * physics.contact_skin()
                });
                held.source.push_str(".projected");
            }
        }
        let ped = self
            .peds
            .state(id)
            .ok_or_else(|| format!("unknown release actor '{id}'"))?;
        let radius = ped.profile.capsule_height * model.collection_reach_fraction;
        let mut state = self.items.item_state(id, item).cloned();
        if let Some(combat) = ped.combat.as_ref().filter(|_| {
            definition
                .metadata
                .pointer("/weapon/type")
                .and_then(Value::as_str)
                == Some("firearm")
        }) {
            let value = state.get_or_insert_with(|| json!({}));
            let object = value
                .as_object_mut()
                .ok_or_else(|| "weapon instance state must be an object".to_owned())?;
            let weapon = object.entry("weapon").or_insert_with(|| json!({}));
            let weapon = weapon
                .as_object_mut()
                .ok_or_else(|| "weapon ammunition state must be an object".to_owned())?;
            weapon.insert("magazine".into(), json!(combat.magazine));
            weapon.insert("reserve".into(), json!(combat.reserve));
        }
        let pickup = format!("ped.drop.{:016x}.{generation}", actor_hash(id));
        let origin = PickupOrigin {
            owner_id: id.into(),
            equipment_generation: generation,
            reason: serde_json::to_value(reason)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default(),
            pose_source: held.source,
            initial_pose: held.pose.clone(),
        };
        if !self.items.drop_equipped(EquipmentDrop {
            owner: id,
            slot: "hands",
            item,
            generation,
            pickup_id: &pickup,
            radius,
            origin,
            state,
        })? {
            return Ok(false);
        }
        self.peds.release_weapon_grip(id, reason);
        if let Some(visual) = held.visual {
            self.scene.clear_entity_joint_attachment(visual);
            self.scene
                .set_entity_visibility(visual, "native.weapon_release", false)?;
        }
        self.ped_physics.grip_motion.remove(id);
        self.save_ped_vitals(id)?;
        host::publish_event_json(
            "ped.weapon.released",
            "newviso.peds",
            json!({"actor_id":id,"item_id":item,"equipment_generation":generation,
                "pickup_id":pickup,"reason":reason,"pose":held.pose}),
        )?;
        host::publish_event_json(
            "gameplay.inventory.equipment_changed",
            "newviso.items",
            json!({"owner_id":id,"slot":"hands","item_id":Value::Null}),
        )?;
        Ok(true)
    }

    pub(super) fn release_weapon_action(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let id = command
            .get("actor_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("script command[{index}] release requires actor_id"))?;
        let item = command
            .get("item_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("script command[{index}] release requires item_id"))?;
        let generation = command
            .get("equipment_generation")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("script command[{index}] release requires equipment_generation")
            })?;
        if self.items.equipped_item(id, "hands") != Some(item)
            || self.items.equipped_generation(id, "hands") != generation
        {
            return Ok(());
        }
        let Some(model) = self
            .peds
            .state(id)
            .and_then(|p| p.profile.weapon_grip.model.clone())
        else {
            return Ok(());
        };
        let Some(definition) = self.items.definition(item).cloned() else {
            return Ok(());
        };
        if let Some(held) = self.held_weapon_pose(id, &definition, &model) {
            self.release_ped_equipment(
                id,
                item,
                generation,
                GripLossReason::Action,
                &definition,
                &model,
                held,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn angular_velocity_uses_the_shortest_arc_and_physical_time() {
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let q = [0.0, h, 0.0, h];
        let velocity = rotation_velocity([0., 0., 0., 1.], q, 0.5);
        assert!((velocity[1] - std::f32::consts::PI).abs() < 1e-5);
        assert_eq!(rotation_velocity(q, q.map(|v| -v), 0.5), [0.; 3]);
    }
    #[test]
    fn physical_item_inertia_is_authored_or_computed_from_actual_size_and_mass() {
        let definition = |mass: f32, inertia: Value| ItemDefinition {
            id: "test".into(),
            display_name: "test".into(),
            category: "weapon".into(),
            max_stack: 1,
            tags: vec![],
            metadata: json!({"physics":{
                "mass":mass,"bounds_min":[-1.,-2.,-3.],"bounds_max":[1.,2.,3.],
                "inertia_diagonal":inertia
            }}),
        };
        let box_shape = WeaponPhysics::read(&definition(3., Value::Null)).unwrap();
        assert_eq!(box_shape.inertia(), [13., 10., 5.]);
        assert_eq!(
            WeaponPhysics::read(&definition(3., json!([1., 2., 3.])))
                .unwrap()
                .inertia(),
            [1., 2., 3.]
        );
        assert!(WeaponPhysics::read(&definition(-3., Value::Null)).is_err());
        assert!(WeaponPhysics::read(&definition(3., json!([-1., 2., 3.]))).is_err());
    }
}
