use super::*;
impl PhysicsRuntime {
    pub(crate) fn interact_door(
        &mut self,
        id: u64,
        actor: DoorActor,
        action: &str,
    ) -> Result<dm::DoorResult, String> {
        let mut b = self
            .doors
            .entries
            .remove(&id)
            .ok_or("unknown door mechanism")?;
        let result = (|| {
            if !b.ready {
                return Ok(dm::DoorResult::Busy);
            }
            let g = b.geometry.as_ref().unwrap();
            let body = self
                .bodies
                .get(&actor.body)
                .ok_or("door actor has no live physical body")?;
            let leaf = self.bodies.get(&id).ok_or("door leaf is missing")?;
            let handle = g.handle(leaf.position, leaf.rotation);
            if dm::sub(body.position, handle)
                .iter()
                .map(|v| v * v)
                .sum::<f32>()
                .sqrt()
                > b.mechanism.config.interaction_reach
            {
                return Ok(dm::DoorResult::OutOfReach);
            }
            if self
                .character_sweep_sphere(
                    body.position,
                    dm::sub(handle, body.position),
                    self.settings.contact_skin,
                    Some(actor.body),
                    &[],
                )
                .is_some_and(|hit| hit.entity != Some(id) && hit.fraction < 1.0)
            {
                return Ok(dm::DoorResult::OutOfReach);
            }
            if b.queue.front().is_some_and(|r| r.actor != actor.id) {
                return Ok(dm::DoorResult::Busy);
            }
            let occupied = self
                .bodies
                .values()
                .any(|p| p.entity != id && g.occupied(p, b.mechanism.config.passage_margin));
            let side = g.side(body.position);
            let authorized = b.mechanism.config.access(side, actor.access.contains(&id));
            let target = g.target(side, b.mechanism.config.open_angle_radians);
            let open = || {
                target
                    .map(|target_angle| dm::Operation::Open { target_angle })
                    .ok_or("hinge has no opening range")
            };
            let op = match action {
                "open" => open()?,
                "close" => dm::Operation::Close,
                "lock" => dm::Operation::Lock,
                "unlock" => dm::Operation::Unlock,
                "release" => dm::Operation::Release,
                "repair" => dm::Operation::Repair,
                "toggle" => {
                    if b.angle.abs() * g.width() <= b.mechanism.config.capture_metres {
                        open()?
                    } else {
                        dm::Operation::Close
                    }
                }
                "lock_toggle" => {
                    if b.mechanism.state.locked {
                        dm::Operation::Unlock
                    } else {
                        dm::Operation::Lock
                    }
                }
                _ => return Err("unknown door interaction".into()),
            };
            let now = self.fixed_seconds;
            let o = dm::Observation {
                seconds: now,
                angle: b.angle,
                angular_speed: b.angular_speed,
                edge_radius: g.width(),
                occupied,
                ..Default::default()
            };
            let result = b.mechanism.operate(op, o, authorized);
            if matches!(op, dm::Operation::Release) {
                b.queue.retain(|r| r.actor != actor.id);
            }

            if result == dm::DoorResult::Accepted && matches!(op, dm::Operation::Open { .. }) {
                if !b.queue.iter().any(|r| r.actor == actor.id) {
                    self.doors.next_ticket = self.doors.next_ticket.wrapping_add(1).max(1);
                    b.queue.push_back(PassageRequest::new(
                        &actor,
                        side,
                        self.doors.next_ticket,
                        now,
                        body.position,
                    ));
                }
            }
            Ok(result)
        })();
        if let Ok(result) = result {
            let point = b
                .geometry
                .as_ref()
                .and_then(|g| {
                    self.bodies
                        .get(&id)
                        .map(|leaf| g.handle(leaf.position, leaf.rotation))
                })
                .unwrap_or([0.0; 3]);
            self.doors.action(id, &actor.id, action, result, point);
        }
        self.doors.entries.insert(id, b);
        result
    }
    pub(crate) fn release_door_actor(&mut self, actor: &str) {
        for b in self.doors.entries.values_mut() {
            let owner = b.queue.front().is_some_and(|r| r.actor == actor);
            b.queue.retain(|r| r.actor != actor);
            if owner {
                b.mechanism.state.goal = None;
                b.mechanism.state.handle_until = 0.0;
            }
        }
    }
}
