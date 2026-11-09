//! Script-facing revolute joints, expanded into existing backend-neutral sockets.
use super::*;
impl PhysicsRuntime {
    pub(crate) fn upsert_hinge_from_script(&mut self, value: &Value) -> Result<(), String> {
        if !self.supports_articulated_drives() {
            return Err(
                "physical hinges require JointConstraints, JointMotors and JointAngularLimits"
                    .into(),
            );
        }
        let hinge: newviso_physics_client::PhysicsHinge = serde_json::from_value(value.clone())
            .map_err(|e| format!("invalid physical hinge: {e}"))?;
        let joints = hinge.expand()?;
        if !self.bodies.contains_key(&hinge.parent) || !self.bodies.contains_key(&hinge.child) {
            return Err("physical hinge requires resident parent and child bodies".into());
        }
        for joint in &joints {
            if self
                .joints
                .get(&joint.id)
                .is_some_and(|old| old.parent != joint.parent || old.child != joint.child)
            {
                return Err(format!(
                    "hinge joint ID {} belongs to another body pair",
                    joint.id
                ));
            }
        }
        for joint in joints {
            self.upsert_joint(joint)?;
        }
        Ok(())
    }
    pub(crate) fn remove_hinge_from_script(&mut self, value: &Value) -> Result<(), String> {
        let ids: [u64; 2] = serde_json::from_value(
            value
                .get("joint_ids")
                .cloned()
                .ok_or("hinge.remove requires joint_ids")?,
        )
        .map_err(|e| format!("invalid hinge IDs: {e}"))?;
        if ids[0] == ids[1] {
            return Err("hinge IDs must differ".into());
        }
        for id in ids {
            self.remove_joint(id);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_hinge_updates_are_atomic_and_existing_owners_are_not_overwritten() {
        let mut p = PhysicsRuntime::empty_for_tests();
        p.supports_joints = true;
        p.supports_articulation = true;
        for entity in [1, 2, 3] {
            p.upsert_body_from_script(
                &json!({"entity":entity,"body_kind":"static",
            "shape":{"kind":"box","half_extents":[0.1,0.1,0.1]},"position":[0,0,0]}),
                0,
            )
            .unwrap();
        }
        let mut v = json!({"joint_ids":[11,12],"parent":1,"child":2,"parent_pivot":[0,0,0],"child_pivot":[-0.5,0,0],
            "parent_frame_rotation":[0,0,0,1],"child_frame_rotation":[0,0,0,1],"span":1.5,"max_force":10000,
            "collision_family":1,"angle_limits":[-1.6,1.6],"stop_stiffness":1000,"stop_damping":10,"stop_max_torque":500});
        p.upsert_hinge_from_script(&v).unwrap();
        assert_eq!(p.joints.len(), 2);
        v["child"] = json!(3);
        assert!(p.upsert_hinge_from_script(&v).is_err());
        assert_eq!(p.joints[&11].child, 2);
        v["child"] = json!(2);
        v["span"] = json!(0);
        assert!(p.upsert_hinge_from_script(&v).is_err());
        assert_eq!(p.joints.len(), 2);
        p.remove_hinge_from_script(&json!({"joint_ids":[11,12]}))
            .unwrap();
        assert!(p.joints.is_empty());
    }
}
