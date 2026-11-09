use super::*;
#[derive(Clone, Debug, serde::Serialize)]
pub(super) struct DoorGeometry {
    pub center: [f32; 3],
    pub rotation: [f32; 4],
    pub half: [f32; 3],
    pub pivot: [f32; 3],
    pub frame: [f32; 4],
    pub hinge: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 3],
    pub limits: [f32; 2],
    pub handle_point: [f32; 3],
    pub latch_point: [f32; 3],
}
impl DoorGeometry {
    pub fn from_hinge(
        body: &PhysicsBodySnapshot,
        parent: &PhysicsBodySnapshot,
        j: [PhysicsJointSnapshot; 2],
        definition: &DoorDefinition,
    ) -> Result<Self, String> {
        if j[0].parent != j[1].parent || j[0].child != body.entity || j[1].child != body.entity {
            return Err("door joints must belong to the same hinge bodies".into());
        }
        let CollisionShape::Box { half_extents: half } = body.shape else {
            return Err("door mechanisms require a box leaf proxy".into());
        };
        let pivot = std::array::from_fn(|i| (j[0].child_anchor[i] + j[1].child_anchor[i]) * 0.5);
        let parent_pivot =
            std::array::from_fn(|i| (j[0].parent_anchor[i] + j[1].parent_anchor[i]) * 0.5);
        let hinge = dm::add(parent.position, dm::rotate(parent.rotation, parent_pivot));
        let normal = dm::rotate(body.rotation, [0.0, 0.0, 1.0]);
        let tangent = dm::rotate(body.rotation, [1.0, 0.0, 0.0]);
        let limits = j[0]
            .angular_limits
            .ok_or("door hinge requires angular limits")?
            .twist;
        let handle_point = definition.handle_local.unwrap_or([
            -pivot[0].signum() * half[0] * 0.75,
            -half[1] * 0.25,
            0.0,
        ]);
        let latch_point =
            definition
                .latch_local
                .unwrap_or([-pivot[0].signum() * half[0], handle_point[1], 0.0]);
        if (latch_point[0] - pivot[0]).hypot(latch_point[2] - pivot[2]) <= f32::EPSILON {
            return Err("door latch must have a nonzero lever from the hinge".into());
        }
        Ok(Self {
            center: body.position,
            rotation: body.rotation,
            half,
            pivot,
            frame: j[0].child_frame_rotation,
            hinge,
            normal,
            tangent,
            limits,
            handle_point,
            latch_point,
        })
    }
    pub fn width(&self) -> f32 {
        2.0 * self.half[0]
    }
    pub fn handle_local(&self) -> [f32; 3] {
        self.handle_point
    }
    pub fn handle(&self, position: [f32; 3], rotation: [f32; 4]) -> [f32; 3] {
        dm::add(position, dm::rotate(rotation, self.handle_local()))
    }
    pub fn side(&self, position: [f32; 3]) -> f32 {
        dm::dot(dm::sub(position, self.center), self.normal).signum()
    }
    pub fn crossing_point(&self, y: f32) -> [f32; 3] {
        let mut p = dm::add(
            self.center,
            dm::mul(self.tangent, -self.pivot[0].signum() * self.half[0] * 0.4),
        );
        p[1] = y;
        p
    }
    pub fn passage_clear(&self, body: &PhysicsBodySnapshot, radius: f32, margin: f32) -> bool {
        let center = self.crossing_point(self.center[1]);
        let depth = radius + self.half[2] + margin;
        let a = dm::add(center, dm::mul(self.normal, depth));
        let b = dm::add(center, dm::mul(self.normal, -depth));
        let c = dm::add(
            body.position,
            dm::rotate(body.rotation, [-self.half[0], 0.0, 0.0]),
        );
        let d = dm::add(
            body.position,
            dm::rotate(body.rotation, [self.half[0], 0.0, 0.0]),
        );
        dm::segment_distance(a, b, c, d) > radius + self.half[2] + margin
    }
    pub fn target(&self, side: f32, angle: f32) -> Option<f32> {
        let desired = -self.pivot[0].signum() * side;
        let a = if desired > 0.0 {
            self.limits[1].min(angle)
        } else {
            self.limits[0].max(-angle)
        };
        if a.abs() > 0.0 {
            Some(a)
        } else {
            let other = if desired > 0.0 {
                self.limits[0].max(-angle)
            } else {
                self.limits[1].min(angle)
            };
            (other.abs() > 0.0).then_some(other)
        }
    }
    pub fn occupied(&self, body: &PhysicsBodySnapshot, margin: f32) -> bool {
        let CollisionShape::Capsule {
            radius,
            half_height,
        } = body.shape
        else {
            return false;
        };
        let delta = dm::sub(body.position, self.center);
        dm::dot(delta, self.normal).abs() <= radius + self.half[2] + margin
            && dm::dot(delta, self.tangent).abs() <= self.half[0] + radius + margin
            && delta[1].abs() <= self.half[1] + half_height + radius
    }
}
