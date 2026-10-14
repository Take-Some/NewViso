/// Authored rigid collision/mass data in model-local coordinates. Component
/// identities survive cooking independently of render mesh and skeleton indices.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelFragmentPhysicsChild {
    pub component: u32,
    pub mass: f32,
    pub damaged_mass: f32,
    pub center_of_mass: [f32; 3],
    pub inertia_diagonal: [f32; 3],
    pub convex_hulls: Vec<Vec<[f32; 3]>>,
}

/// One physical attachment group can own several children and render parts
/// (for example the tyre and rim of a single wheel).
#[derive(Clone, Debug, PartialEq)]
pub struct ModelFragmentPartPhysics {
    pub group: u16,
    pub parent_group: Option<u16>,
    /// Negative strength denotes an unbreakable attachment in authored data.
    pub strength: f32,
    pub latch_strength: f32,
    pub transmission_up: f32,
    pub transmission_down: f32,
    pub joint_stiffness: f32,
    pub soft_angle_limits: [f32; 4],
    pub flags: u32,
    pub min_damage_force: f32,
    pub damage_health: f32,
    pub weapon_health: f32,
    pub weapon_scale: f32,
    pub vehicle_scale: f32,
    pub explosion_scale: f32,
    pub object_scale: f32,
    pub children: Vec<ModelFragmentPhysicsChild>,
}
