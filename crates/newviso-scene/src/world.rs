use crate::math::Vec3;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const SPATIAL_CELL_SIZE: f32 = 32.0;
const MAX_ENTITY_SPATIAL_CELLS: usize = 128;
const PROCESS_TARGET_FRAME_SECONDS: f32 = 1.0 / 60.0;
const PROCESS_BASE_UPDATES_PER_FRAME: usize = 256;
const PROCESS_MIN_UPDATES_PER_FRAME: usize = 32;
const PROCESS_MAX_UPDATES_PER_FRAME: usize = 512;

type SpatialCell = (i32, i32);

mod lifecycle;
mod mutation;
mod visibility;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SceneEntityId(pub(crate) u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneEntityKind {
    Camera,
    StaticMesh,
    DynamicMesh,
    Light,
    SkyVisual,
    Trigger,
    Portal,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneMobility {
    Static,
    Dynamic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LightType {
    Directional,
    Point,
    Spot,
    Area,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LightComponent {
    pub(crate) light_type: LightType,
    pub(crate) color: [f32; 3],
    pub(crate) intensity: f32,
    pub(crate) range: f32,
    pub(crate) cone_inner_degrees: f32,
    pub(crate) cone_outer_degrees: f32,
    pub(crate) casts_shadows: bool,
    pub(crate) shadow_bias: f32,
    pub(crate) shadow_normal_bias: f32,
    pub(crate) shadow_resolution: u32,
    pub(crate) shadow_distance: f32,
}

impl LightComponent {
    pub(crate) fn validate(self) -> Result<Self, String> {
        if self
            .color
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
            || !self.intensity.is_finite()
            || self.intensity < 0.0
            || !self.range.is_finite()
            || self.range < 0.0
            || !self.cone_inner_degrees.is_finite()
            || !self.cone_outer_degrees.is_finite()
            || self.cone_inner_degrees < 0.0
            || self.cone_outer_degrees < self.cone_inner_degrees
            || self.cone_outer_degrees > 179.0
            || !self.shadow_bias.is_finite()
            || self.shadow_bias < 0.0
            || !self.shadow_normal_bias.is_finite()
            || self.shadow_normal_bias < 0.0
            || self.shadow_resolution == 0
            || !self.shadow_distance.is_finite()
            || self.shadow_distance <= 0.0
        {
            return Err("invalid generic LightComponent parameters".to_owned());
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneLifecycle {
    Constructed,
    Added,
    Active,
    Dormant,
    PendingRemove,
    Removed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneResidency {
    Unloaded,
    Requested,
    Resident,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VisibilityMask {
    channels: BTreeMap<String, bool>,
}

impl VisibilityMask {
    pub(crate) fn set(&mut self, channel: &str, visible: bool) {
        let channel = channel.trim().to_ascii_lowercase();
        if channel.is_empty() {
            return;
        }
        if visible {
            self.channels.remove(&channel);
        } else {
            self.channels.insert(channel, false);
        }
    }

    #[cfg(test)]
    pub(crate) fn visible_for(&self, channel: &str) -> bool {
        self.channels
            .get(&channel.trim().to_ascii_lowercase())
            .copied()
            .unwrap_or(true)
    }

    pub(crate) fn all_visible(&self) -> bool {
        self.channels.values().all(|visible| *visible)
    }

    pub(crate) fn raw(&self) -> &BTreeMap<String, bool> {
        &self.channels
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SceneBounds {
    pub(crate) min: Vec3,
    pub(crate) max: Vec3,
}

impl SceneBounds {
    pub(crate) fn from_center_half_extent(center: Vec3, half: Vec3) -> Self {
        Self {
            min: Vec3::new(center.x - half.x, center.y - half.y, center.z - half.z),
            max: Vec3::new(center.x + half.x, center.y + half.y, center.z + half.z),
        }
    }

    pub(crate) fn center(self) -> Vec3 {
        Vec3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }

    pub(crate) fn radius(self) -> f32 {
        self.max.sub(self.center()).length()
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneLodPolicy {
    pub(crate) visible_distance: f32,
    pub(crate) stream_distance: f32,
    pub(crate) fade_range: f32,
}

impl Default for SceneLodPolicy {
    fn default() -> Self {
        Self {
            visible_distance: f32::INFINITY,
            stream_distance: f32::INFINITY,
            fade_range: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SceneTransform {
    pub(crate) position: Vec3,
    pub(crate) rotation_degrees: Vec3,
    pub(crate) scale: Vec3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneMutationSource {
    Engine,
    Script,
    Physics,
    Animation,
    Parent,
    Streaming,
}

impl SceneMutationSource {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::Script => "script",
            Self::Physics => "physics",
            Self::Animation => "animation",
            Self::Parent => "parent",
            Self::Streaming => "streaming",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SceneDirtyFlags(u16);

impl SceneDirtyFlags {
    pub(crate) const EMPTY: Self = Self(0);
    pub(crate) const TRANSFORM: Self = Self(1 << 0);
    pub(crate) const BOUNDS: Self = Self(1 << 1);
    pub(crate) const VISIBILITY: Self = Self(1 << 2);
    pub(crate) const HIERARCHY: Self = Self(1 << 3);
    pub(crate) const LIFECYCLE: Self = Self(1 << 4);
    pub(crate) const RESIDENCY: Self = Self(1 << 5);
    pub(crate) const LIGHT: Self = Self(1 << 6);
    pub(crate) const PROCESS_CONTROL: Self = Self(1 << 7);
    pub(crate) const MOBILITY: Self = Self(1 << 8);
    pub(crate) const DAMAGE: Self = Self(1 << 9);

    pub(crate) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub(crate) const fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub(crate) fn labels(self) -> Vec<&'static str> {
        [
            (Self::TRANSFORM, "transform"),
            (Self::BOUNDS, "bounds"),
            (Self::VISIBILITY, "visibility"),
            (Self::HIERARCHY, "hierarchy"),
            (Self::LIFECYCLE, "lifecycle"),
            (Self::RESIDENCY, "residency"),
            (Self::LIGHT, "light"),
            (Self::PROCESS_CONTROL, "process_control"),
            (Self::MOBILITY, "mobility"),
            (Self::DAMAGE, "damage"),
        ]
        .into_iter()
        .filter_map(|(flag, label)| self.contains(flag).then_some(label))
        .collect()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SceneProcessReasons(u32);

impl SceneProcessReasons {
    pub(crate) const EMPTY: Self = Self(0);
    pub(crate) const PHYSICS: Self = Self(1 << 0);
    pub(crate) const INTELLIGENCE: Self = Self(1 << 1);
    pub(crate) const ANIMATION: Self = Self(1 << 2);
    pub(crate) const SCRIPT: Self = Self(1 << 3);
    pub(crate) const NETWORK: Self = Self(1 << 4);
    pub(crate) const STREAMING: Self = Self(1 << 5);
    pub(crate) const DESTRUCTION: Self = Self(1 << 6);
    pub(crate) const MOVER: Self = Self(1 << 7);
    pub(crate) const EXPLICIT: Self = Self(1 << 8);
    pub(crate) const CUSTOM: Self = Self(1 << 9);

    pub(crate) const ALL: [Self; 10] = [
        Self::PHYSICS,
        Self::INTELLIGENCE,
        Self::ANIMATION,
        Self::SCRIPT,
        Self::NETWORK,
        Self::STREAMING,
        Self::DESTRUCTION,
        Self::MOVER,
        Self::EXPLICIT,
        Self::CUSTOM,
    ];

    pub(crate) const fn bits(self) -> u32 {
        self.0
    }

    pub(crate) const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub(crate) const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub(crate) const fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub(crate) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub(crate) fn labels(self) -> Vec<&'static str> {
        [
            (Self::PHYSICS, "physics"),
            (Self::INTELLIGENCE, "intelligence"),
            (Self::ANIMATION, "animation"),
            (Self::SCRIPT, "script"),
            (Self::NETWORK, "network"),
            (Self::STREAMING, "streaming"),
            (Self::DESTRUCTION, "destruction"),
            (Self::MOVER, "mover"),
            (Self::EXPLICIT, "explicit"),
            (Self::CUSTOM, "custom"),
        ]
        .into_iter()
        .filter_map(|(flag, label)| self.contains(flag).then_some(label))
        .collect()
    }

    pub(crate) fn from_reason_label(reason: &str) -> Self {
        let reason = reason.trim().to_ascii_lowercase();
        match reason.as_str() {
            "physics" | "physics_awake" | "collision" => Self::PHYSICS,
            "ai" | "intelligence" | "perception" => Self::INTELLIGENCE,
            "animation" | "anim" => Self::ANIMATION,
            "script" | "scripting" | "gameplay_script" => Self::SCRIPT,
            "network" | "networking" | "replication" => Self::NETWORK,
            "streaming" | "residency" | "collision_streaming" => Self::STREAMING,
            "destruction" | "damage" | "breakable" => Self::DESTRUCTION,
            "mover" | "movement" | "moving_platform" => Self::MOVER,
            "explicit" | "force" | "force_update" => Self::EXPLICIT,
            _ => Self::CUSTOM,
        }
    }

    fn from_claim(owner: &str, reason: &str) -> Self {
        let classified = Self::from_reason_label(reason);
        if classified != Self::CUSTOM {
            return classified;
        }

        let owner = owner.trim().to_ascii_lowercase();
        if owner.starts_with("engine.physics") {
            Self::PHYSICS
        } else if owner.starts_with("engine.animation") {
            Self::ANIMATION
        } else if owner.starts_with("engine.streaming") {
            Self::STREAMING
        } else if owner.starts_with("engine.network") {
            Self::NETWORK
        } else if owner.starts_with("project.script") || owner.starts_with("engine.scripting") {
            Self::SCRIPT
        } else {
            Self::CUSTOM
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SceneProcessClaims {
    claims: BTreeMap<String, BTreeSet<String>>,
}

impl SceneProcessClaims {
    pub(crate) fn set(&mut self, owner: String, reason: String, active: bool) -> bool {
        if active {
            self.claims.entry(reason).or_default().insert(owner)
        } else {
            let Some(owners) = self.claims.get_mut(&reason) else {
                return false;
            };
            let changed = owners.remove(&owner);
            if owners.is_empty() {
                self.claims.remove(&reason);
            }
            changed
        }
    }

    pub(crate) fn active(&self) -> bool {
        !self.claims.is_empty()
    }

    pub(crate) fn clear(&mut self) -> bool {
        if self.claims.is_empty() {
            false
        } else {
            self.claims.clear();
            true
        }
    }

    pub(crate) fn reasons(&self) -> Vec<String> {
        self.claims.keys().cloned().collect()
    }

    pub(crate) fn mask(&self) -> SceneProcessReasons {
        let mut mask = SceneProcessReasons::EMPTY;
        for (reason, owners) in &self.claims {
            for owner in owners {
                mask = mask.union(SceneProcessReasons::from_claim(owner, reason));
            }
        }
        mask
    }

    pub(crate) fn snapshot(&self) -> BTreeMap<String, Vec<String>> {
        self.claims
            .iter()
            .map(|(reason, owners)| (reason.clone(), owners.iter().cloned().collect()))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SceneProcessTicket {
    pub(crate) entity: SceneEntityId,
    pub(crate) frame: u64,
    pub(crate) reasons: SceneProcessReasons,
    pub(crate) elapsed_seconds: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneMutation {
    pub(crate) entity: SceneEntityId,
    pub(crate) revision: u64,
    pub(crate) frame: u64,
    pub(crate) source: SceneMutationSource,
    pub(crate) dirty: SceneDirtyFlags,
    pub(crate) transform: SceneTransform,
    pub(crate) bounds: SceneBounds,
    pub(crate) process_active: bool,
    pub(crate) process_reason_mask: u32,
    pub(crate) process_claims: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SceneDestructible {
    pub(crate) max_health: f32,
    pub(crate) health: f32,
    /// Impacts below this threshold do not consume durability.
    pub(crate) impact_damage_threshold: f32,
    /// Damage added for every impulse unit above impact_damage_threshold.
    pub(crate) impact_damage_scale: f32,
    /// A single impact at or above this value breaks the object immediately.
    pub(crate) break_impulse: f32,
    /// Fraction of contact impulse transferred to the newly dynamic body.
    pub(crate) impulse_transfer: f32,
    pub(crate) density: f32,
    pub(crate) friction: f32,
    pub(crate) restitution: f32,
    pub(crate) linear_damping: f32,
    pub(crate) angular_damping: f32,
    pub(crate) broken: bool,
}

impl SceneDestructible {
    pub(crate) fn validate(self) -> Result<Self, String> {
        let values = [
            self.max_health,
            self.health,
            self.impact_damage_threshold,
            self.impact_damage_scale,
            self.break_impulse,
            self.impulse_transfer,
            self.density,
            self.friction,
            self.restitution,
            self.linear_damping,
            self.angular_damping,
        ];
        if values.iter().any(|value| !value.is_finite())
            || self.max_health <= 0.0
            || self.health < 0.0
            || self.health > self.max_health
            || self.impact_damage_threshold < 0.0
            || self.impact_damage_scale < 0.0
            || self.break_impulse <= 0.0
            || self.impulse_transfer < 0.0
            || self.density <= 0.0
            || self.friction < 0.0
            || self.restitution < 0.0
            || self.linear_damping < 0.0
            || self.angular_damping < 0.0
        {
            return Err("invalid SceneDestructible parameters".to_owned());
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SceneDamageOutcome {
    pub(crate) applied_damage: f32,
    pub(crate) remaining_health: f32,
    pub(crate) broke_now: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneEntity {
    pub(crate) id: SceneEntityId,
    pub(crate) name: String,
    pub(crate) kind: SceneEntityKind,
    pub(crate) mobility: SceneMobility,
    pub(crate) lifecycle: SceneLifecycle,
    pub(crate) transform: SceneTransform,
    pub(crate) light: Option<LightComponent>,
    pub(crate) bounds: SceneBounds,
    pub(crate) parent: Option<SceneEntityId>,
    pub(crate) children: Vec<SceneEntityId>,
    pub(crate) visibility: VisibilityMask,
    pub(crate) lod: SceneLodPolicy,
    pub(crate) solid: bool,
    /// Optional model-local collision box. Render/spatial bounds remain
    /// independent so foliage and long-arm street fixtures do not collide as
    /// their full visual AABB.
    pub(crate) collision_local_bounds: Option<SceneBounds>,
    pub(crate) destructible: Option<SceneDestructible>,
    pub(crate) asset_ref: Option<String>,
    /// Geometry generated at runtime can render without a streamable asset path.
    pub(crate) resident_geometry: bool,
    pub(crate) render_slot: Option<usize>,
    pub(crate) residency: SceneResidency,
    pub(crate) priority_score: f32,
    pub(crate) lod_alpha: f32,
    pub(crate) last_visible_frame: Option<u64>,
    pub(crate) revision: u64,
    pub(crate) last_mutation_frame: u64,
    pub(crate) process_claims: SceneProcessClaims,
    pub(crate) last_process_frame: Option<u64>,
}

impl SceneEntity {
    pub(crate) fn is_renderable(&self) -> bool {
        let has_visual = self.resident_geometry
            || self.render_slot.is_some()
            || (self.asset_ref.is_some()
                && matches!(
                    self.kind,
                    SceneEntityKind::StaticMesh | SceneEntityKind::DynamicMesh
                ));
        has_visual
            && self.lifecycle == SceneLifecycle::Active
            && self.residency == SceneResidency::Resident
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneFocusSource {
    Camera,
    Entity(SceneEntityId),
    Override,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneFocus {
    pub(crate) position: Vec3,
    pub(crate) velocity: Vec3,
    pub(crate) source: SceneFocusSource,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneView {
    pub(crate) position: Vec3,
    pub(crate) forward: Vec3,
    pub(crate) up: Vec3,
    pub(crate) near: f32,
    pub(crate) far: f32,
    pub(crate) fov_y_radians: f32,
    pub(crate) aspect: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SceneFramePlan {
    pub(crate) frame: u64,
    pub(crate) visible_render_slots: Vec<usize>,
    pub(crate) visible_entities: Vec<SceneEntityId>,
    pub(crate) requested_entities: Vec<SceneEntityId>,
    pub(crate) streaming_entities: Vec<SceneEntityId>,
    pub(crate) visible_count: usize,
    pub(crate) culled_count: usize,
    pub(crate) resident_count: usize,
    pub(crate) dynamic_count: usize,
    pub(crate) spatial_candidate_count: usize,
}

#[derive(Debug)]
pub(crate) struct SceneWorld {
    entities: Vec<SceneEntity>,
    by_id: HashMap<SceneEntityId, usize>,
    frame: u64,
    focus: SceneFocus,
    last_focus_position: Vec3,
    last_dt: f32,
    mutations: Vec<SceneMutation>,
    static_render_epoch: u64,
    process_active: BTreeSet<SceneEntityId>,
    process_elapsed_seconds: f64,
    process_rates_hz: BTreeMap<(SceneEntityId, u32), f32>,
    process_last_due_seconds: BTreeMap<(SceneEntityId, u32), f64>,
    process_tickets: Vec<SceneProcessTicket>,
    process_scan_cursor: usize,
    process_effective_budget: usize,
    process_scanned_count: usize,
    spatial_cells: HashMap<SpatialCell, BTreeSet<SceneEntityId>>,
    spatial_entity_cells: HashMap<SceneEntityId, Vec<SpatialCell>>,
    spatial_oversized: BTreeSet<SceneEntityId>,
    spatial_always_stream: BTreeSet<SceneEntityId>,
    spatial_max_stream_distance: f32,
}

impl SceneWorld {
    pub(crate) fn process_elapsed_seconds(&self) -> f64 {
        self.process_elapsed_seconds
    }

    pub(crate) fn new(initial_focus: Vec3) -> Self {
        Self {
            entities: Vec::new(),
            by_id: HashMap::new(),
            frame: 0,
            focus: SceneFocus {
                position: initial_focus,
                velocity: Vec3::ZERO,
                source: SceneFocusSource::Camera,
            },
            last_focus_position: initial_focus,
            last_dt: 0.0,
            mutations: Vec::new(),
            static_render_epoch: 0,
            process_active: BTreeSet::new(),
            process_elapsed_seconds: 0.0,
            process_rates_hz: BTreeMap::new(),
            process_last_due_seconds: BTreeMap::new(),
            process_tickets: Vec::new(),
            process_scan_cursor: 0,
            process_effective_budget: PROCESS_BASE_UPDATES_PER_FRAME,
            process_scanned_count: 0,
            spatial_cells: HashMap::new(),
            spatial_entity_cells: HashMap::new(),
            spatial_oversized: BTreeSet::new(),
            spatial_always_stream: BTreeSet::new(),
            spatial_max_stream_distance: 0.0,
        }
    }
}

fn spatial_cell_for_point(point: Vec3) -> SpatialCell {
    (
        (point.x / SPATIAL_CELL_SIZE).floor() as i32,
        (point.z / SPATIAL_CELL_SIZE).floor() as i32,
    )
}

fn spatial_cells_for_bounds(bounds: SceneBounds) -> Option<Vec<SpatialCell>> {
    let min = spatial_cell_for_point(bounds.min);
    let max = spatial_cell_for_point(bounds.max);
    let x_count = i64::from(max.0) - i64::from(min.0) + 1;
    let z_count = i64::from(max.1) - i64::from(min.1) + 1;
    let total = x_count.saturating_mul(z_count);
    if total <= 0 || total as usize > MAX_ENTITY_SPATIAL_CELLS {
        return None;
    }

    let mut cells = Vec::with_capacity(total as usize);
    for x in min.0..=max.0 {
        for z in min.1..=max.1 {
            cells.push((x, z));
        }
    }
    Some(cells)
}

fn stream_priority(radius: f32, distance: f32, mobility: SceneMobility) -> f32 {
    let size_score = radius.max(0.05) / distance.max(0.5);
    let mobility_bias = match mobility {
        SceneMobility::Static => 1.0,
        SceneMobility::Dynamic => 1.35,
    };
    size_score * mobility_bias
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(id: u64, z: f32, render_slot: usize) -> SceneEntity {
        SceneEntity {
            id: SceneEntityId(id),
            name: format!("entity-{id}"),
            kind: SceneEntityKind::StaticMesh,
            mobility: SceneMobility::Static,
            lifecycle: SceneLifecycle::Constructed,
            transform: SceneTransform {
                position: Vec3::new(0.0, 0.0, z),
                rotation_degrees: Vec3::ZERO,
                scale: Vec3::ONE,
            },
            light: None,
            bounds: SceneBounds::from_center_half_extent(
                Vec3::new(0.0, 0.0, z),
                Vec3::new(1.0, 1.0, 1.0),
            ),
            parent: None,
            children: Vec::new(),
            visibility: VisibilityMask::default(),
            lod: SceneLodPolicy::default(),
            solid: false,
            collision_local_bounds: None,
            destructible: None,
            asset_ref: None,
            resident_geometry: false,
            render_slot: Some(render_slot),
            residency: SceneResidency::Resident,
            priority_score: 0.0,
            lod_alpha: 1.0,
            last_visible_frame: None,
            revision: 0,
            last_mutation_frame: 0,
            process_claims: SceneProcessClaims::default(),
            last_process_frame: None,
        }
    }

    fn view() -> SceneView {
        SceneView {
            position: Vec3::ZERO,
            forward: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::Y,
            near: 0.1,
            far: 100.0,
            fov_y_radians: 70.0_f32.to_radians(),
            aspect: 16.0 / 9.0,
        }
    }

    #[test]
    fn lifecycle_and_visibility_scan_match_scene_phases() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(1, -8.0, 0)).unwrap();
        world.add_entity(entity(2, 8.0, 1)).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        world.update();

        let plan = world.scan_visibility(view());
        assert_eq!(plan.visible_render_slots, vec![0]);
        assert_eq!(plan.visible_count, 1);
        assert_eq!(plan.culled_count, 1);
    }

    #[test]
    fn visibility_modules_can_hide_entity_without_removing_it() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(1, -8.0, 0)).unwrap();
        world.activate_all();
        world
            .set_visibility(SceneEntityId(1), "script", false)
            .unwrap();
        assert!(!world
            .entity(SceneEntityId(1))
            .unwrap()
            .visibility
            .visible_for("script"));
        world
            .set_visibility(SceneEntityId(1), "camera", false)
            .unwrap();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        let plan = world.scan_visibility(view());
        assert!(plan.visible_render_slots.is_empty());
    }

    #[test]
    fn generated_mesh_is_visible_without_asset_or_primitive_slot() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut generated = entity(1, -3.0, 0);
        generated.render_slot = None;
        generated.resident_geometry = true;
        generated.kind = SceneEntityKind::DynamicMesh;
        generated.mobility = SceneMobility::Dynamic;
        world.add_entity(generated).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        let plan = world.scan_visibility(view());
        assert_eq!(plan.visible_entities, vec![SceneEntityId(1)]);
        assert!(plan.requested_entities.is_empty());
        assert!(plan.visible_render_slots.is_empty());
    }

    #[test]
    fn generic_light_component_is_stored_without_game_semantics() {
        let mut e = entity(9, -4.0, 0);
        e.kind = SceneEntityKind::Light;
        e.render_slot = None;
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(e).unwrap();
        world.activate_all();

        world
            .set_light(
                SceneEntityId(9),
                Some(LightComponent {
                    light_type: LightType::Directional,
                    color: [1.0, 0.9, 0.8],
                    intensity: 1.5,
                    range: 0.0,
                    cone_inner_degrees: 0.0,
                    cone_outer_degrees: 0.0,
                    casts_shadows: true,
                    shadow_bias: 0.0015,
                    shadow_normal_bias: 0.02,
                    shadow_resolution: 2048,
                    shadow_distance: 96.0,
                }),
            )
            .unwrap();

        let lights = world.active_lights();
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].2.light_type, LightType::Directional);
        assert!(lights[0].2.casts_shadows);
    }

    #[test]
    fn dynamic_entities_receive_streaming_priority_bias() {
        let static_score = stream_priority(1.0, 10.0, SceneMobility::Static);
        let dynamic_score = stream_priority(1.0, 10.0, SceneMobility::Dynamic);
        assert!(dynamic_score > static_score);
    }

    #[test]
    fn static_render_epoch_ignores_dynamic_and_process_only_mutations() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let static_id = SceneEntityId(80);
        let dynamic_id = SceneEntityId(81);
        world.add_entity(entity(static_id.0, -8.0, 0)).unwrap();

        let mut dynamic = entity(dynamic_id.0, -8.0, 1);
        dynamic.mobility = SceneMobility::Dynamic;
        dynamic.kind = SceneEntityKind::DynamicMesh;
        world.add_entity(dynamic).unwrap();
        world.activate_all();
        world.seal_initial_state();
        assert_eq!(world.static_render_epoch(), 0);

        let dynamic_transform = SceneTransform {
            position: Vec3::new(2.0, 0.0, -8.0),
            rotation_degrees: Vec3::ZERO,
            scale: Vec3::ONE,
        };
        let dynamic_bounds =
            SceneBounds::from_center_half_extent(dynamic_transform.position, Vec3::ONE);
        world
            .update_spatial_from(
                dynamic_id,
                dynamic_transform,
                dynamic_bounds,
                SceneMutationSource::Animation,
            )
            .unwrap();
        assert_eq!(world.static_render_epoch(), 0);

        world
            .set_process_claim(
                static_id,
                "project.script",
                "script",
                true,
                SceneMutationSource::Script,
            )
            .unwrap();
        assert_eq!(world.static_render_epoch(), 0);

        let static_transform = SceneTransform {
            position: Vec3::new(3.0, 0.0, -8.0),
            rotation_degrees: Vec3::ZERO,
            scale: Vec3::ONE,
        };
        let static_bounds =
            SceneBounds::from_center_half_extent(static_transform.position, Vec3::ONE);
        world
            .update_spatial_from(
                static_id,
                static_transform,
                static_bounds,
                SceneMutationSource::Engine,
            )
            .unwrap();
        assert_eq!(world.static_render_epoch(), 1);
    }

    #[test]
    fn spatial_visibility_ignores_far_static_entities_without_global_scan() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(1, -8.0, 0)).unwrap();
        for i in 0..1000u64 {
            let mut far = entity(10_000 + i, -8.0, (i + 1) as usize);
            far.transform.position = Vec3::new(5_000.0 + i as f32 * 4.0, 0.0, -8.0);
            far.bounds = SceneBounds::from_center_half_extent(
                far.transform.position,
                Vec3::new(1.0, 1.0, 1.0),
            );
            world.add_entity(far).unwrap();
        }
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert_eq!(plan.visible_entities, vec![SceneEntityId(1)]);
        assert_eq!(plan.spatial_candidate_count, 1);
        assert!(world.spatial_cell_count() > 100);
    }

    #[test]
    fn spatial_static_solid_query_returns_only_intersecting_colliders() {
        let mut world = SceneWorld::new(Vec3::ZERO);

        let mut near = entity(70, -8.0, 0);
        near.solid = true;
        near.bounds = SceneBounds::from_center_half_extent(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 2.0, 2.0),
        );
        world.add_entity(near).unwrap();

        let mut far = entity(71, -8.0, 1);
        far.solid = true;
        far.bounds = SceneBounds::from_center_half_extent(
            Vec3::new(1_000.0, 0.0, 0.0),
            Vec3::new(2.0, 2.0, 2.0),
        );
        world.add_entity(far).unwrap();

        world.activate_all();
        let interests = [SceneBounds {
            min: Vec3::new(-16.0, -16.0, -16.0),
            max: Vec3::new(16.0, 16.0, 16.0),
        }];
        let solids = world.static_solid_bounds_near(&interests);

        assert_eq!(solids.len(), 1);
        assert_eq!(solids[0], world.entity(SceneEntityId(70)).unwrap().bounds);
    }

    #[test]
    fn oversized_spatial_entity_fails_open() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut huge = entity(2, -50.0, 0);
        huge.bounds = SceneBounds {
            min: Vec3::new(-2_000.0, -10.0, -2_000.0),
            max: Vec3::new(2_000.0, 10.0, 2_000.0),
        };
        world.add_entity(huge).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert_eq!(world.spatial_oversized_count(), 1);
        assert!(plan.spatial_candidate_count >= 1);
        assert!(plan.visible_entities.contains(&SceneEntityId(2)));
    }

    #[test]
    fn conservative_guard_band_keeps_edge_visible_entity() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut e = entity(1, -10.0, 0);
        e.transform.position = Vec3::new(14.5, 0.0, -10.0);
        e.bounds =
            SceneBounds::from_center_half_extent(e.transform.position, Vec3::new(1.0, 1.0, 1.0));
        world.add_entity(e).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert_eq!(plan.visible_render_slots, vec![0]);
    }

    #[test]
    fn hierarchy_propagates_visibility_and_rejects_cycles() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(1, -8.0, 0)).unwrap();
        world.add_entity(entity(2, -8.0, 1)).unwrap();
        world.activate_all();
        world
            .set_parent(SceneEntityId(2), Some(SceneEntityId(1)))
            .unwrap();
        assert!(world
            .set_parent(SceneEntityId(1), Some(SceneEntityId(2)))
            .is_err());

        world
            .set_visibility(SceneEntityId(1), "world", false)
            .unwrap();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        let plan = world.scan_visibility(view());
        assert!(!plan.visible_entities.contains(&SceneEntityId(2)));
    }

    #[test]
    fn focus_tracks_entity_and_supports_explicit_override() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(7, -12.0, 0)).unwrap();
        world.activate_all();

        world.set_focus_entity(SceneEntityId(7)).unwrap();
        world.pre_update(Vec3::new(99.0, 0.0, 0.0), 1.0 / 60.0);
        assert!((world.focus().position.z + 12.0).abs() < 0.001);

        world.set_focus_override(Vec3::new(3.0, 4.0, 5.0), Vec3::new(1.0, 0.0, 0.0));
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        assert_eq!(world.focus().source, SceneFocusSource::Override);
        assert!((world.focus().position.x - 3.0).abs() < 0.001);

        world.set_focus_camera();
        world.pre_update(Vec3::new(2.0, 0.0, 0.0), 1.0 / 60.0);
        assert_eq!(world.focus().source, SceneFocusSource::Camera);
        assert!((world.focus().position.x - 2.0).abs() < 0.001);
    }

    #[test]
    fn spatial_mutation_tracks_revision_source_and_dirty_state() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut moving = entity(41, -8.0, 0);
        moving.mobility = SceneMobility::Dynamic;
        world.add_entity(moving).unwrap();
        world.activate_all();

        let next_transform = SceneTransform {
            position: Vec3::new(3.0, 2.0, -9.0),
            rotation_degrees: Vec3::new(0.0, 45.0, 0.0),
            scale: Vec3::ONE,
        };
        let next_bounds =
            SceneBounds::from_center_half_extent(next_transform.position, Vec3::new(1.0, 2.0, 1.0));
        world
            .update_spatial_from(
                SceneEntityId(41),
                next_transform,
                next_bounds,
                SceneMutationSource::Physics,
            )
            .unwrap();

        let mutations = world.drain_mutations();
        assert_eq!(mutations.len(), 1);
        assert_eq!(mutations[0].entity, SceneEntityId(41));
        assert_eq!(mutations[0].revision, 1);
        assert_eq!(mutations[0].source, SceneMutationSource::Physics);
        assert!(mutations[0].dirty.contains(SceneDirtyFlags::TRANSFORM));
        assert!(mutations[0].dirty.contains(SceneDirtyFlags::BOUNDS));

        world
            .update_spatial_from(
                SceneEntityId(41),
                next_transform,
                next_bounds,
                SceneMutationSource::Physics,
            )
            .unwrap();
        assert!(world.drain_mutations().is_empty());
        assert_eq!(world.entity(SceneEntityId(41)).unwrap().revision, 1);
    }

    #[test]
    fn process_claims_are_multi_owner_and_do_not_affect_render_lifecycle() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(50, -8.0, 0)).unwrap();
        world.activate_all();

        assert_eq!(world.process_active_count(), 0);
        assert_eq!(
            world.entity(SceneEntityId(50)).unwrap().lifecycle,
            SceneLifecycle::Active
        );

        world
            .set_process_claim(
                SceneEntityId(50),
                "engine.physics",
                "physics",
                true,
                SceneMutationSource::Physics,
            )
            .unwrap();
        world
            .set_process_claim(
                SceneEntityId(50),
                "project.script",
                "gameplay",
                true,
                SceneMutationSource::Script,
            )
            .unwrap();

        assert_eq!(world.process_active_count(), 1);
        assert!(world
            .entity(SceneEntityId(50))
            .unwrap()
            .process_claims
            .active());

        world
            .set_process_claim(
                SceneEntityId(50),
                "engine.physics",
                "physics",
                false,
                SceneMutationSource::Physics,
            )
            .unwrap();
        assert_eq!(world.process_active_count(), 1);
        assert!(world
            .entity(SceneEntityId(50))
            .unwrap()
            .process_claims
            .active());

        world
            .set_process_claim(
                SceneEntityId(50),
                "project.script",
                "gameplay",
                false,
                SceneMutationSource::Script,
            )
            .unwrap();
        assert_eq!(world.process_active_count(), 0);
        assert!(!world
            .entity(SceneEntityId(50))
            .unwrap()
            .process_claims
            .active());
        assert_eq!(
            world.entity(SceneEntityId(50)).unwrap().lifecycle,
            SceneLifecycle::Active
        );
    }

    #[test]
    fn dormant_entity_suspends_process_membership_but_preserves_claims() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(55, -8.0, 0)).unwrap();
        world.activate_all();
        world
            .set_process_claim(
                SceneEntityId(55),
                "project.script",
                "gameplay",
                true,
                SceneMutationSource::Script,
            )
            .unwrap();
        assert!(world.process_is_active(SceneEntityId(55)));

        world.set_dormant(SceneEntityId(55), true).unwrap();
        assert!(!world.process_is_active(SceneEntityId(55)));
        assert!(world
            .entity(SceneEntityId(55))
            .unwrap()
            .process_claims
            .active());

        world.activate_entity(SceneEntityId(55)).unwrap();
        assert!(world.process_is_active(SceneEntityId(55)));
    }

    #[test]
    fn removal_clears_process_claims_atomically() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(56, -8.0, 0)).unwrap();
        world.activate_all();
        world
            .set_process_claim(
                SceneEntityId(56),
                "engine.physics",
                "physics",
                true,
                SceneMutationSource::Physics,
            )
            .unwrap();
        world.drain_mutations();

        world.request_remove(SceneEntityId(56)).unwrap();
        assert!(!world.process_is_active(SceneEntityId(56)));
        assert!(!world
            .entity(SceneEntityId(56))
            .unwrap()
            .process_claims
            .active());
        let mutation = world.drain_mutations().pop().expect("removal mutation");
        assert!(mutation.dirty.contains(SceneDirtyFlags::LIFECYCLE));
        assert!(mutation.dirty.contains(SceneDirtyFlags::PROCESS_CONTROL));
        assert!(!mutation.process_active);
        assert!(mutation.process_claims.is_empty());
    }

    #[test]
    fn process_claims_compile_to_typed_reason_mask() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(58, -8.0, 0)).unwrap();
        world.activate_all();

        world
            .set_process_claim(
                SceneEntityId(58),
                "engine.physics",
                "physics",
                true,
                SceneMutationSource::Physics,
            )
            .unwrap();
        world
            .set_process_claim(
                SceneEntityId(58),
                "project.script",
                "gameplay",
                true,
                SceneMutationSource::Script,
            )
            .unwrap();

        let mask = world
            .entity(SceneEntityId(58))
            .unwrap()
            .process_claims
            .mask();
        assert!(mask.contains(SceneProcessReasons::PHYSICS));
        assert!(mask.contains(SceneProcessReasons::SCRIPT));
        assert!(!mask.contains(SceneProcessReasons::ANIMATION));
        assert_eq!(mask.labels(), vec!["physics", "script"]);
    }

    #[test]
    fn process_scheduler_applies_independent_reason_cadence() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(59, -8.0, 0)).unwrap();
        world.activate_all();
        world
            .set_process_claim(
                SceneEntityId(59),
                "engine.physics",
                "physics",
                true,
                SceneMutationSource::Physics,
            )
            .unwrap();
        world
            .set_process_claim(
                SceneEntityId(59),
                "engine.animation",
                "animation",
                true,
                SceneMutationSource::Animation,
            )
            .unwrap();
        world
            .set_process_rate_hz(SceneEntityId(59), "animation", 4.0)
            .unwrap();
        assert!(world
            .set_process_rate_hz(SceneEntityId(59), "physics", 30.0)
            .is_err());

        world.pre_update(Vec3::ZERO, 0.05);
        world.update();
        let first = world.process_tickets();
        assert_eq!(first.len(), 1);
        assert!(first[0].reasons.contains(SceneProcessReasons::PHYSICS));
        assert!(first[0].reasons.contains(SceneProcessReasons::ANIMATION));

        world.pre_update(Vec3::ZERO, 0.05);
        world.update();
        let second = world.process_tickets();
        assert_eq!(second.len(), 1);
        assert!(second[0].reasons.contains(SceneProcessReasons::PHYSICS));
        assert!(!second[0].reasons.contains(SceneProcessReasons::ANIMATION));

        for _ in 0..4 {
            world.pre_update(Vec3::ZERO, 0.05);
            world.update();
        }
        let due = world.process_tickets();
        assert_eq!(due.len(), 1);
        assert!(due[0].reasons.contains(SceneProcessReasons::PHYSICS));
        assert!(due[0].reasons.contains(SceneProcessReasons::ANIMATION));
    }

    #[test]
    fn streaming_residency_suspends_and_restores_process_membership() {
        let mut e = entity(62, -8.0, 0);
        e.asset_ref = Some("models/process_test.ydd".to_owned());
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(e).unwrap();
        world.activate_all();
        world
            .set_process_claim(
                SceneEntityId(62),
                "project.script",
                "script",
                true,
                SceneMutationSource::Script,
            )
            .unwrap();
        assert!(world.process_is_active(SceneEntityId(62)));

        world
            .set_residency(SceneEntityId(62), SceneResidency::Requested)
            .unwrap();
        assert!(!world.process_is_active(SceneEntityId(62)));
        assert!(world
            .entity(SceneEntityId(62))
            .unwrap()
            .process_claims
            .active());

        world
            .set_residency(SceneEntityId(62), SceneResidency::Resident)
            .unwrap();
        assert!(world.process_is_active(SceneEntityId(62)));

        world.pre_update(Vec3::ZERO, 0.016);
        world.update();
        assert_eq!(world.process_tickets().len(), 1);
        assert!(world.process_tickets()[0]
            .reasons
            .contains(SceneProcessReasons::SCRIPT));
    }

    #[test]
    fn process_timeslice_budget_rotates_without_starvation() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        for i in 0..600u64 {
            world
                .add_entity(entity(20_000 + i, -8.0, i as usize))
                .unwrap();
        }
        world.activate_all();
        for i in 0..600u64 {
            world
                .set_process_claim(
                    SceneEntityId(20_000 + i),
                    "project.script",
                    "script",
                    true,
                    SceneMutationSource::Script,
                )
                .unwrap();
        }

        let mut seen = BTreeSet::new();
        for frame in 0..3 {
            world.pre_update(Vec3::ZERO, 1.0 / 60.0);
            world.update();
            assert_eq!(world.process_effective_budget(), 256);
            assert_eq!(world.process_due_count(), 256);
            assert_eq!(world.process_scanned_count(), 256);
            let frame_ids = world
                .process_tickets()
                .iter()
                .map(|ticket| ticket.entity)
                .collect::<BTreeSet<_>>();
            if frame < 2 {
                assert_eq!(frame_ids.len(), 256);
            }
            seen.extend(frame_ids);
        }
        assert_eq!(seen.len(), 600);

        world.pre_update(Vec3::ZERO, 1.0 / 30.0);
        world.update();
        assert_eq!(world.process_effective_budget(), 128);
        assert_eq!(world.process_due_count(), 128);
    }

    #[test]
    fn process_update_marks_only_claimed_entities() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(60, -8.0, 0)).unwrap();
        world.add_entity(entity(61, -8.0, 1)).unwrap();
        world.activate_all();
        world
            .set_process_claim(
                SceneEntityId(61),
                "project.script",
                "animation",
                true,
                SceneMutationSource::Script,
            )
            .unwrap();

        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        world.update();

        assert_eq!(
            world.entity(SceneEntityId(60)).unwrap().last_process_frame,
            None
        );
        assert_eq!(
            world.entity(SceneEntityId(61)).unwrap().last_process_frame,
            Some(1)
        );
    }

    #[test]
    fn transform_only_mutation_does_not_fake_bounds_change() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(42, -8.0, 0)).unwrap();
        world.activate_all();

        let transform = SceneTransform {
            position: Vec3::new(0.0, 0.0, -7.0),
            rotation_degrees: Vec3::ZERO,
            scale: Vec3::ONE,
        };
        world
            .update_transform_from(SceneEntityId(42), transform, SceneMutationSource::Animation)
            .unwrap();

        let mutations = world.drain_mutations();
        assert_eq!(mutations.len(), 1);
        assert_eq!(mutations[0].source, SceneMutationSource::Animation);
        assert!(mutations[0].dirty.contains(SceneDirtyFlags::TRANSFORM));
        assert!(!mutations[0].dirty.contains(SceneDirtyFlags::BOUNDS));
    }

    #[test]
    fn infinite_stream_distance_keeps_collision_asset_in_streaming_interest() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut collision = entity(776, -8.0, 0);
        collision.transform.position = Vec3::new(20_000.0, 0.0, 20_000.0);
        collision.bounds = SceneBounds::from_center_half_extent(
            collision.transform.position,
            Vec3::new(25.0, 5.0, 25.0),
        );
        collision.lod = SceneLodPolicy {
            visible_distance: 100.0,
            stream_distance: f32::INFINITY,
            fade_range: 0.0,
        };
        collision.residency = SceneResidency::Unloaded;
        collision.asset_ref = Some("collisions/always-resident.ybn".to_owned());
        world.add_entity(collision).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert!(plan.requested_entities.contains(&SceneEntityId(776)));
        assert!(plan.streaming_entities.contains(&SceneEntityId(776)));
    }

    #[test]
    fn large_collision_bounds_stream_by_nearest_surface_not_origin() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut sector = entity(777, -8.0, 0);
        sector.transform.position = Vec3::new(50.0, 0.0, 0.0);
        sector.bounds = SceneBounds {
            min: Vec3::new(5.0, -2.0, -2.0),
            max: Vec3::new(95.0, 2.0, 2.0),
        };
        sector.lod = SceneLodPolicy {
            visible_distance: 100.0,
            stream_distance: 10.0,
            fade_range: 0.0,
        };
        sector.residency = SceneResidency::Unloaded;
        sector.asset_ref = Some("collisions/large-sector.ybn".to_owned());
        world.add_entity(sector).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert!(
            plan.requested_entities.contains(&SceneEntityId(777)),
            "a sector whose authored bounds reach within stream distance must load even when its origin is far away"
        );
    }

    #[test]
    fn unloaded_assets_are_requested_in_priority_order() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut far = entity(1, -30.0, 0);
        far.residency = SceneResidency::Unloaded;
        far.asset_ref = Some("assets/far_model@main".to_owned());
        let mut near = entity(2, -5.0, 1);
        near.residency = SceneResidency::Unloaded;
        near.asset_ref = Some("assets/near_model@main".to_owned());
        world.add_entity(far).unwrap();
        world.add_entity(near).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert_eq!(
            plan.requested_entities,
            vec![SceneEntityId(2), SceneEntityId(1)]
        );
        assert_eq!(
            plan.streaming_entities,
            vec![SceneEntityId(2), SceneEntityId(1)]
        );

        world
            .set_residency(SceneEntityId(2), SceneResidency::Resident)
            .unwrap();
        let next = world.scan_visibility(view());
        assert_eq!(
            next.streaming_entities,
            vec![SceneEntityId(2), SceneEntityId(1)]
        );
        assert!(!next.requested_entities.contains(&SceneEntityId(2)));
    }

    #[test]
    fn destructible_scene_entity_accumulates_damage_and_promotes_to_dynamic() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut prop = entity(90, -8.0, 0);
        prop.solid = true;
        prop.destructible = Some(
            SceneDestructible {
                max_health: 50.0,
                health: 50.0,
                impact_damage_threshold: 10.0,
                impact_damage_scale: 1.0,
                break_impulse: 40.0,
                impulse_transfer: 0.9,
                density: 30.0,
                friction: 0.6,
                restitution: 0.05,
                linear_damping: 0.1,
                angular_damping: 0.2,
                broken: false,
            }
            .validate()
            .unwrap(),
        );
        world.add_entity(prop).unwrap();
        world.activate_all();
        world.seal_initial_state();

        let first = world
            .apply_damage(SceneEntityId(90), 12.0, 5.0)
            .unwrap()
            .expect("destructible outcome");
        assert!(!first.broke_now);
        assert!((first.remaining_health - 38.0).abs() < 1.0e-6);
        assert_eq!(
            world.entity(SceneEntityId(90)).unwrap().mobility,
            SceneMobility::Static
        );

        let broken = world
            .apply_damage(SceneEntityId(90), 0.0, 45.0)
            .unwrap()
            .expect("break outcome");
        assert!(broken.broke_now);
        let prop = world.entity(SceneEntityId(90)).unwrap();
        assert_eq!(prop.mobility, SceneMobility::Dynamic);
        assert_eq!(prop.kind, SceneEntityKind::DynamicMesh);
        assert!(prop.destructible.unwrap().broken);
        assert!(world.static_render_epoch() > 0);
    }

    #[test]
    fn damage_to_unknown_or_non_destructible_entity_is_ignored() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(91, -8.0, 0)).unwrap();
        world.activate_all();

        assert!(world
            .apply_damage(SceneEntityId(999_999), 25.0, 100.0)
            .unwrap()
            .is_none());
        assert!(world
            .apply_damage(SceneEntityId(91), 25.0, 100.0)
            .unwrap()
            .is_none());
    }

    #[test]
    fn lod_fade_is_computed_before_visibility_gather() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut e = entity(1, -8.0, 0);
        e.lod = SceneLodPolicy {
            visible_distance: 10.0,
            stream_distance: 12.0,
            fade_range: 4.0,
        };
        world.add_entity(e).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert_eq!(plan.visible_count, 1);
        let alpha = world.entity(SceneEntityId(1)).unwrap().lod_alpha;
        assert!(alpha > 0.0 && alpha < 1.0);
    }
}
