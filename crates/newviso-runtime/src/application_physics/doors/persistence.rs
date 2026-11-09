//! Checkpoints keep stable identities, native motion and hardware; actor leases are transient.
use super::*;
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedDoor {
    state: dm::MechanismState,
    position: [f32; 3],
    rotation: [f32; 4],
    linear_velocity: [f32; 3],
    angular_velocity: [f32; 3],
    hinge: [f32; 3],
    half: [f32; 3],
    closed_rotation: [f32; 4],
}
impl SavedDoor {
    fn validate(&self) -> Result<(), String> {
        if self
            .position
            .iter()
            .chain(&self.rotation)
            .chain(&self.linear_velocity)
            .chain(&self.angular_velocity)
            .chain(&self.hinge)
            .chain(&self.half)
            .chain(&self.closed_rotation)
            .any(|v| !v.is_finite())
            || self.half.iter().any(|v| *v <= 0.0)
            || [self.rotation, self.closed_rotation]
                .iter()
                .any(|q| (q.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() > 0.01)
        {
            return Err("invalid saved physical door pose".into());
        }
        Ok(())
    }
    fn capture(binding: &DoorBinding, body: &PhysicsBodySnapshot) -> Option<Self> {
        let g = binding.geometry.as_ref()?;
        Some(Self {
            state: binding.mechanism.state.clone(),
            position: body.position,
            rotation: body.rotation,
            linear_velocity: body.linear_velocity,
            angular_velocity: body.angular_velocity,
            hinge: g.hinge,
            half: g.half,
            closed_rotation: g.rotation,
        })
    }
}
impl PhysicsRuntime {
    pub(crate) fn door_checkpoint(&self) -> Value {
        let mut records = self.doors.saved.clone();
        for (id, b) in &self.doors.entries {
            if let Some(saved) = self
                .bodies
                .get(id)
                .and_then(|body| SavedDoor::capture(b, body))
            {
                records.insert(b.definition.persistent_id.clone(), saved);
            }
        }
        json!({"schema":"newviso.doors.save.v1","records":records})
    }
    pub(crate) fn load_door_checkpoint(&mut self, value: &Value) -> Result<(), String> {
        if value.is_null() {
            return Ok(());
        }
        if value.get("schema").and_then(Value::as_str) != Some("newviso.doors.save.v1") {
            return Err("unsupported door checkpoint".into());
        }
        let records: BTreeMap<String, SavedDoor> =
            serde_json::from_value(value.get("records").cloned().ok_or("missing saved doors")?)
                .map_err(|e| format!("invalid saved doors: {e}"))?;
        for (key, record) in &records {
            if key.is_empty() || key.len() > 256 {
                return Err("invalid saved door identity".into());
            }
            record.validate()?;
        }
        self.doors.saved = records;
        Ok(())
    }
    pub(super) fn stash_door(&mut self, b: &DoorBinding) {
        if let Some(saved) = self
            .bodies
            .get(&b.definition.entity)
            .and_then(|body| SavedDoor::capture(b, body))
        {
            self.doors
                .saved
                .insert(b.definition.persistent_id.clone(), saved);
        }
    }
    pub(super) fn restore_ready_door(
        &mut self,
        b: &mut DoorBinding,
        now: f64,
    ) -> Result<(), String> {
        let Some(saved) = self.doors.saved.remove(&b.definition.persistent_id) else {
            return Ok(());
        };
        let g = b.geometry.as_ref().unwrap();
        let tolerance = b.mechanism.config.capture_metres;
        let valid = dm::sub(saved.hinge, g.hinge)
            .iter()
            .map(|v| v * v)
            .sum::<f32>()
            .sqrt()
            <= tolerance
            && saved
                .half
                .iter()
                .zip(g.half)
                .all(|(a, b)| (a - b).abs() <= tolerance)
            && (dm::quaternion_product(
                [
                    -g.rotation[0],
                    -g.rotation[1],
                    -g.rotation[2],
                    g.rotation[3],
                ],
                saved.closed_rotation,
            )[3]
            .abs()
                - 1.0)
                .abs()
                <= tolerance / g.width()
            && dm::sub(
                dm::add(saved.position, dm::rotate(saved.rotation, g.pivot)),
                g.hinge,
            )
            .iter()
            .map(|v| v * v)
            .sum::<f32>()
            .sqrt()
                <= b.mechanism.config.passage_margin.max(tolerance);
        if !valid {
            self.doors.event(
                b.definition.entity,
                None,
                "restore_rejected",
                None,
                g.center,
            );
            return Ok(());
        }
        let angle = dm::twist(g.rotation, saved.rotation, g.frame);
        b.mechanism.restore(
            saved.state,
            dm::Observation {
                seconds: now,
                angle,
                edge_radius: g.width(),
                ..Default::default()
            },
        )?;
        self.set_body_pose_from_script(&json!({"entity":b.definition.entity,"position":saved.position,"rotation":saved.rotation}),0)?;
        self.set_body_velocity_from_script(
            &json!({"entity":b.definition.entity,"velocity":saved.linear_velocity}),
            0,
        )?;
        if let Some(body) = self.bodies.get_mut(&b.definition.entity) {
            body.angular_velocity = saved.angular_velocity;
        }
        b.angle = angle;
        self.doors
            .event(b.definition.entity, None, "restored", None, g.center);
        Ok(())
    }
}
