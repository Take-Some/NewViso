use super::*;

impl EngineApplication {
    pub(crate) fn ped_physics_presentation(
        &self,
        id: &str,
        offset: [f32; 3],
    ) -> Option<([f32; 3], [f32; 3])> {
        let body = self.ped_physics.bodies.get(id)?;
        let pose = self.physics.as_ref()?.vehicle_body_state(body.entity)?;
        if let Some(ragdoll) = &body.ragdoll {
            return Some(ragdoll.presentation(pose.position));
        }
        if body.reaction.is_some() {
            // Original KO/get-up clips already turn and lower the skeleton.
            // Anchor them to the capsule's bottom; applying its pitch again
            // would make a lying character rotate and sink a second time.
            let bottom = pose.position[1]
                - animation::capsule_vertical_extent(pose.rotation, body.radius, body.half_height);
            let floor = self
                .physics
                .as_ref()?
                .character_ground_contact_height(body.entity);
            let mut model_y = bottom.max(floor.unwrap_or(bottom)) + offset[1];
            if let Some(floor) = floor {
                if let Some(rig) = self
                    .world_actor_presentations
                    .get(id)
                    .and_then(|p| p.locomotion.as_ref())
                    .and_then(|l| l.ragdoll.as_ref())
                {
                    let contacts = rig.floor_contacts().collect::<Vec<_>>();
                    if let Ok(Some(lowest)) = self
                        .scene
                        .entity_skeletal_floor_offset(body.entity, &contacts)
                    {
                        // Interrupted blends can lower a hand below the clip's
                        // nominal root floor. Keep authored contact volumes grounded.
                        model_y = model_y.max(floor - lowest + 0.002);
                    }
                }
            }
            return Some((
                [
                    pose.position[0] + offset[0],
                    model_y,
                    pose.position[2] + offset[2],
                ],
                [0.0, body.death_heading, 0.0],
            ));
        }
        if !self.peds.is_dead(id)
            && !body
                .vehicle_response
                .as_ref()
                .is_some_and(|r| r.kind != VehiclePedHit::Bump)
        {
            return None;
        }
        let offset = [offset[0], offset[1] - body.center_offset, offset[2]];
        let [x, y, z, w] = pose.rotation;
        let cross = |a: [f32; 3], b: [f32; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let t = cross([x, y, z], offset).map(|v| v * 2.0);
        let c = cross([x, y, z], t);
        let position = std::array::from_fn(|i| pose.position[i] + offset[i] + w * t[i] + c[i]);
        let (s, h) = (body.death_heading.to_radians() * 0.5).sin_cos();
        let [x, y, z, w] = [x * h - z * s, y * h + w * s, z * h + x * s, w * h - y * s];
        let rotation = [
            (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y)),
            (2.0 * (w * y - z * x)).clamp(-1.0, 1.0).asin(),
            (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z)),
        ]
        .map(f32::to_degrees);
        Some((position, rotation))
    }

    pub(crate) fn ped_runtime_state(&self) -> Value {
        let mut state = self.peds.runtime_state();
        state["physics"] = json!({"bodies": self.ped_physics.bodies.iter().map(|(id, b)| json!({
            "actor_id": id, "entity": b.entity, "entity_key": b.entity.to_string(), "mass_kg": PED_MASS_KG, "center_offset": b.center_offset,
            "recovery_seconds": b.recovery_seconds, "controlled": !self.peds.is_dead(id) && b.recovery_seconds <= 0.0 && b.vehicle_response.is_none() && b.reaction.is_none(),
            "ragdoll": self.ragdoll_snapshot(b),
            "hit_reaction": b.hit_reaction.as_ref().map(|r|json!({"clip_ref":r.clip_ref,"elapsed_seconds":r.elapsed,"duration_seconds":r.duration,"generation":r.generation})),
            "additive_playback": self.scene.entity_additive_animation(b.entity),
            "animation": b.reaction.as_ref().map(|r| json!({"phase": r.phase.as_str(), "cause": r.cause,
                "clip_ref": r.clip.clip_ref, "elapsed_seconds": r.elapsed, "clip_time_seconds": r.clip_time(),
                "duration_seconds": r.clip.duration_seconds, "generation": r.generation})),
            "skeletal_playback": self.scene.entity_animation_playback(b.entity),
            "joint_positions": (["SKEL_Pelvis", "SKEL_Head", "SKEL_L_Hand", "SKEL_R_Hand", "SKEL_L_Foot", "SKEL_R_Foot"]
                .into_iter().filter_map(|joint| self.scene.entity_joint_world_position(b.entity, joint).ok().map(|p| (joint, p)))
                .collect::<BTreeMap<_, _>>()),
            "vehicle_response": b.vehicle_response.as_ref().map(|r| json!({"kind": r.kind.as_str(), "weapon_type": r.kind.weapon_type(),
                "source_entity_key": r.source.to_string(), "elapsed_seconds": r.elapsed, "direction": r.direction, "wheel_load": r.wheel_load}))
        })).collect::<Vec<_>>()});
        state
    }
}
