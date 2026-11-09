//! A latch/bolt reacts at the strike plate, not as a frozen leaf transform.
use super::*;
impl PhysicsRuntime {
    pub(super) fn update_door_retention(
        &mut self,
        b: &DoorBinding,
        torque: f32,
    ) -> Result<(), String> {
        if torque <= 0.0 {
            self.joints.remove(&b.retention_joint);
            return Ok(());
        }
        let g = b
            .geometry
            .as_ref()
            .ok_or("hardware has no hinge geometry")?;
        let base = b.baseline.ok_or("hardware has no hinge assembly")?[0];
        let parent = self
            .bodies
            .get(&base.parent)
            .ok_or("hardware frame is missing")?;
        let child_anchor = g.latch_point;
        let strike = dm::add(g.center, dm::rotate(g.rotation, child_anchor));
        let inverse = [
            -parent.rotation[0],
            -parent.rotation[1],
            -parent.rotation[2],
            parent.rotation[3],
        ];
        let lever = (child_anchor[0] - g.pivot[0])
            .hypot(child_anchor[2] - g.pivot[2])
            .max(f32::EPSILON);
        let mut joint = base;
        joint.id = b.retention_joint;
        joint.parent_anchor = dm::rotate(inverse, dm::sub(strike, parent.position));
        joint.child_anchor = child_anchor;
        joint.max_force = torque / lever;
        joint.angular_limits = None;
        joint.motor = None;
        joint.cone_limit = std::f32::consts::PI;
        joint.angular_stiffness = 0.0;
        joint.angular_damping = 0.0;
        self.joints.insert(joint.id, joint);
        Ok(())
    }
}
