use super::*;
use newviso_scene::ScenePhysicsSource;
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Spec {
    pub entity: u64,
    pub anchor_entity: u64,
    pub joint_ids: [u64; 2],
    pub local_pivot: [f32; 3],
    pub frame_rotation: [f32; 4],
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub span_fraction: f32,
    pub max_force: f32,
    pub angle_limits: [f32; 2],
    pub stop_stiffness: f32,
    pub stop_damping: f32,
    pub stop_max_torque: f32,
    pub collision_inset: [f32; 3],
    #[serde(default)]
    pub collision_bounds: Option<([f32; 3], [f32; 3])>,
}
impl Spec {
    pub fn validate(&self) -> Result<(), String> {
        if self.entity == self.anchor_entity
            || self.anchor_entity == 0
            || self.anchor_entity >= STATIC_COLLIDER_ID_BASE
            || self.joint_ids.contains(&0)
            || self.joint_ids[0] == self.joint_ids[1]
            || !self.mass.is_finite()
            || self.mass <= 0.0
            || ![self.friction, self.restitution]
                .iter()
                .all(|x| x.is_finite() && *x >= 0.0)
            || ![self.linear_damping, self.angular_damping]
                .iter()
                .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
            || !self.span_fraction.is_finite()
            || !(0.0..=1.0).contains(&self.span_fraction)
            || self.span_fraction == 0.0
            || !self.local_pivot.iter().all(|x| x.is_finite())
            || !self
                .collision_inset
                .iter()
                .all(|x| x.is_finite() && *x >= 0.0)
        {
            return Err(
                "invalid scene hinge bodies, mass, material, damping, pivot or span".into(),
            );
        }
        self.hinge([0.0; 3], 1.0).expand().map(|_| ())
    }
    pub fn hinge(&self, child_pivot: [f32; 3], span: f32) -> newviso_physics_client::PhysicsHinge {
        newviso_physics_client::PhysicsHinge {
            joint_ids: self.joint_ids,
            parent: self.anchor_entity,
            child: self.entity,
            parent_pivot: [0.0; 3],
            child_pivot,
            parent_frame_rotation: self.frame_rotation,
            child_frame_rotation: self.frame_rotation,
            span,
            max_force: self.max_force,
            collision_family: self.joint_ids[0],
            angle_limits: self.angle_limits,
            stop_stiffness: self.stop_stiffness,
            stop_damping: self.stop_damping,
            stop_max_torque: self.stop_max_torque,
            motor: None,
        }
    }
}
#[derive(Clone, Debug, serde::Serialize)]
pub(super) struct Geometry {
    pub center: [f32; 3],
    pub half_extents: [f32; 3],
    pub rotation: [f32; 4],
    pub hinge_position: [f32; 3],
    pub child_pivot: [f32; 3],
    pub visual_offset: [f32; 3],
    pub span: f32,
}
impl Geometry {
    pub fn from_source(s: &ScenePhysicsSource, spec: &Spec) -> Result<Self, String> {
        let (min, max) = spec
            .collision_bounds
            .or(s.local_bounds)
            .ok_or("scene model geometry is not resident")?;
        if !min
            .iter()
            .chain(&max)
            .chain(&s.position)
            .chain(&s.rotation_degrees)
            .chain(&s.scale)
            .all(|x| x.is_finite())
            || (0..3).any(|i| max[i] <= min[i] || s.scale[i] == 0.0)
        {
            return Err("invalid scene hinge model bounds or transform".into());
        }
        let local_center = std::array::from_fn(|i| (min[i] + max[i]) * 0.5 * s.scale[i]);
        let half_extents = std::array::from_fn(|i| {
            (max[i] - min[i]) * 0.5 * s.scale[i].abs() - spec.collision_inset[i]
        });
        if !half_extents.iter().all(|x| x.is_finite() && *x > 0.0) {
            return Err("scene hinge collision inset removes its body".into());
        }
        let rotation = euler_degrees_to_quaternion(s.rotation_degrees);
        let offset = rotate_camera_vector(local_center, rotation);
        let center = std::array::from_fn(|i| s.position[i] + offset[i]);
        let axis = rotate_camera_vector([0.0, 1.0, 0.0], spec.frame_rotation);
        let mut pivot = std::array::from_fn(|i| spec.local_pivot[i] * s.scale[i]);
        let shift = (0..3)
            .map(|i| (local_center[i] - pivot[i]) * axis[i])
            .sum::<f32>();
        for i in 0..3 {
            pivot[i] += axis[i] * shift;
        }
        let world_pivot = rotate_camera_vector(pivot, rotation);
        let hinge_position = std::array::from_fn(|i| s.position[i] + world_pivot[i]);
        let child_pivot = std::array::from_fn(|i| pivot[i] - local_center[i]);
        let span =
            2.0 * (0..3).map(|i| axis[i].abs() * half_extents[i]).sum::<f32>() * spec.span_fraction;
        let result = Self {
            center,
            half_extents,
            rotation,
            hinge_position,
            child_pivot,
            visual_offset: local_center.map(|x| -x),
            span,
        };
        spec.hinge(child_pivot, span).expand()?;
        Ok(result)
    }
}
#[derive(Debug)]
pub(super) struct Binding {
    pub spec: Spec,
    pub source: Option<ScenePhysicsSource>,
    pub geometry: Option<Geometry>,
    pub error: Option<String>,
}
#[derive(Debug, Default)]
pub(crate) struct SceneHingeBindings {
    pub(super) entries: BTreeMap<u64, Binding>,
}
