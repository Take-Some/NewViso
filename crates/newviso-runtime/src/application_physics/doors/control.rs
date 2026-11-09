use super::*;
impl PhysicsRuntime {
    pub(crate) fn update_door_controls(&mut self, _dt: f32) -> Result<(), String> {
        let now = self.fixed_seconds;
        let ids = self.doors.entries.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let mut b = self.doors.entries.remove(&id).unwrap();
            if !b.ready {
                let j = b
                    .definition
                    .joint_ids
                    .map(|id| self.joints.get(&id).copied());
                if let [Some(a), Some(c)] = j {
                    let body = self
                        .bodies
                        .get(&id)
                        .ok_or("door leaf disappeared before activation")?;
                    let parent = self
                        .bodies
                        .get(&a.parent)
                        .ok_or("door hinge anchor is missing")?;
                    let ready_position = body.position;
                    b.geometry = Some(DoorGeometry::from_hinge(
                        body,
                        parent,
                        [a, c],
                        &b.definition,
                    )?);
                    b.baseline = Some([a, c]);
                    b.ready = true;
                    self.restore_ready_door(&mut b, now)?;
                    self.doors.event(id, None, "ready", None, ready_position);
                } else {
                    self.doors.entries.insert(id, b);
                    continue;
                }
            }
            if !self.bodies.contains_key(&id)
                || b.definition
                    .joint_ids
                    .iter()
                    .any(|j| !self.joints.contains_key(j))
            {
                self.joints.remove(&b.retention_joint);
                self.doors.event(
                    id,
                    None,
                    "retired",
                    None,
                    b.geometry.as_ref().unwrap().center,
                );
                continue;
            }
            let g = b.geometry.as_ref().unwrap();
            let body = &self.bodies[&id];
            b.angle = dm::twist(g.rotation, body.rotation, g.frame);
            let axis = dm::rotate(g.rotation, dm::rotate(g.frame, [0.0, 1.0, 0.0]));
            b.angular_speed = dm::dot(body.angular_velocity, axis);
            b.occupied = self
                .bodies
                .values()
                .any(|p| p.entity != id && g.occupied(p, b.mechanism.config.passage_margin));
            let completed = b
                .queue
                .iter()
                .filter(|r| {
                    self.bodies.get(&r.body).is_some_and(|body| {
                        dm::dot(dm::sub(body.position, g.center), g.normal) * r.side
                            < -(r.radius + g.half[2] + b.mechanism.config.passage_margin)
                    })
                })
                .map(|r| r.actor.clone())
                .collect::<Vec<_>>();
            for actor in &completed {
                self.doors.event(
                    id,
                    Some(actor),
                    "passed",
                    Some(dm::DoorResult::Accepted),
                    g.center,
                );
            }
            b.queue.retain(|r| !completed.contains(&r.actor));
            b.queue.retain(|r| {
                self.bodies.contains_key(&r.body)
                    && now - r.seen < b.mechanism.config.reservation_seconds as f64
            });
            if let Some(owner) = b.queue.front_mut() {
                if owner.active_since.is_none() {
                    owner.active_since = Some(now);
                    owner.progressed = now;
                }
                let position = self.bodies[&owner.body].position;
                if dm::sub(position, owner.last_position)
                    .iter()
                    .map(|v| v * v)
                    .sum::<f32>()
                    .sqrt()
                    > b.mechanism.config.capture_metres
                {
                    owner.last_position = position;
                    owner.progressed = now;
                }
            }
            if b.queue
                .front()
                .is_some_and(|r| now - r.progressed > b.mechanism.config.reservation_seconds as f64)
            {
                b.queue.pop_front();
            }
            let contact = std::mem::take(&mut b.contact);
            let obs = dm::Observation {
                seconds: now,
                angle: b.angle,
                angular_speed: b.angular_speed,
                edge_radius: g.width(),
                load_nm: contact.load_nm,
                impact_joules: contact.energy,
                occupied: b.occupied,
            };
            let a = b.mechanism.tick(obs);
            self.publish_door_state(id, &mut b, obs);
            self.update_door_retention(&b, a.retention_nm)?;
            self.continue_door_impact(&b, contact, a.released_joules)?;
            let base = b.baseline.unwrap();
            let j = self.joints.get_mut(&b.definition.joint_ids[0]).unwrap();
            *j = base[0];
            if a.retention_nm > 0.0 {
                j.motor = None;
            } else if let Some(target) = a.target {
                j.motor = Some(newviso_physics_client::PhysicsJointMotor {
                    world_drive: None,
                    feed_forward_torque: [0.0; 3],
                    target_rotation: [0.0, (target * 0.5).sin(), 0.0, (target * 0.5).cos()],
                    stiffness: a.motor_stiffness,
                    damping: 0.0,
                    max_torque: a.motor_nm,
                });
            }
            self.doors.entries.insert(id, b);
        }
        Ok(())
    }
}
