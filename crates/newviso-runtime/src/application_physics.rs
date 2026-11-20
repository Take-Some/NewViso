use super::*;

const STATIC_COLLIDER_ID_BASE: u64 = 1_u64 << 63;
const MIN_PHYSICS_HZ: f32 = 10.0;
const MAX_PHYSICS_HZ: f32 = 1000.0;
const MAX_PHYSICS_STEPS_LIMIT: usize = 64;
const SCENE_COLLIDER_BROADPHASE_MARGIN: f32 = 32.0;
const CONTACT_SURFACE_MAX_DISTANCE: f32 = 0.35;

#[derive(Clone, Copy, Debug)]
struct PhysicsWorldSettings {
    fixed_hz: f32,
    max_steps_per_frame: usize,
    gravity: f32,
    contact_skin: f32,
    scene_colliders_enabled: bool,
    scene_material: PhysicsMaterial,
    scene_participates_in_queries: bool,
    scene_casts_contacts: bool,
}

impl Default for PhysicsWorldSettings {
    fn default() -> Self {
        Self {
            fixed_hz: 60.0,
            max_steps_per_frame: 8,
            gravity: 0.0,
            contact_skin: 0.002,
            scene_colliders_enabled: false,
            scene_material: PhysicsMaterial {
                friction: 0.5,
                restitution: 0.0,
                density: 1.0,
            },
            scene_participates_in_queries: true,
            scene_casts_contacts: true,
        }
    }
}

impl PhysicsWorldSettings {
    fn fixed_dt(self) -> f32 {
        1.0 / self.fixed_hz
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PhysicsDamageKind {
    Collision,
    Bullet,
    Explosive,
    Fire,
    Melee,
    Water,
    Script,
}

impl PhysicsDamageKind {
    pub(super) fn parse(value: Option<&str>) -> Result<Self, String> {
        match value
            .unwrap_or("script")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "collision" | "impact" => Ok(Self::Collision),
            "bullet" | "ballistic" | "weapon" => Ok(Self::Bullet),
            "explosive" | "explosion" => Ok(Self::Explosive),
            "fire" | "burn" => Ok(Self::Fire),
            "melee" => Ok(Self::Melee),
            "water" | "drown" => Ok(Self::Water),
            "script" | "direct" => Ok(Self::Script),
            other => Err(format!("unsupported physics damage kind '{other}'")),
        }
    }

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Collision => "collision",
            Self::Bullet => "bullet",
            Self::Explosive => "explosive",
            Self::Fire => "fire",
            Self::Melee => "melee",
            Self::Water => "water",
            Self::Script => "script",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PhysicsDamageSource {
    damage: f32,
    kind: PhysicsDamageKind,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PhysicsDamageContact {
    pub source: u64,
    pub target: u64,
    pub damage_kind: PhysicsDamageKind,
    pub direct_damage: f32,
    pub contact_impulse: f32,
    pub point: [f32; 3],
    pub impulse_direction: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
struct PendingBallisticDamage {
    source: u64,
    damage: f32,
    damage_kind: PhysicsDamageKind,
    impulse: f32,
    direction: [f32; 3],
    falloff_min: f32,
    falloff_max: f32,
    falloff_modifier: f32,
}

#[derive(Clone, Copy, Debug)]
struct PhysicsBallisticImpact {
    source: u64,
    target: u64,
    point: [f32; 3],
    normal: [f32; 3],
    distance: f32,
    surface_entity: Option<u64>,
    surface_id: Option<u32>,
}

pub(super) struct PreparedStreamedCollision {
    entity: u64,
    collider: MeshCollider,
    sweep_mesh: newviso_collision::SphereSweepMesh,
    surface_ids: Vec<u32>,
    position: [f32; 3],
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
    nav_source: newviso_navigation::NavTileSource,
}

#[derive(Debug)]
pub(super) struct PhysicsRuntime {
    client: PhysicsClient,
    bodies: BTreeMap<u64, PhysicsBodySnapshot>,
    joints: BTreeMap<u64, PhysicsJointSnapshot>,
    supports_joints: bool,
    supports_articulation: bool,
    supports_collision_filters: bool,
    separation_pairs: BTreeSet<[u64; 2]>,
    body_hull_components: BTreeMap<u64, (Vec<u32>, Vec<u32>)>,
    streamed_colliders: BTreeMap<u64, PhysicsFrameColliderSnapshot>,
    camera_collision_meshes: BTreeMap<u64, newviso_collision::SphereSweepMesh>,
    streamed_surface_ids: BTreeMap<u64, Vec<u32>>,
    pending_streamed_colliders: BTreeMap<u64, PhysicsFrameColliderSnapshot>,
    pending_commands: Vec<PhysicsCommand>,
    pending_queries: Vec<PhysicsQuery>,
    pending_ballistic_damage: BTreeMap<u64, PendingBallisticDamage>,
    ballistic_damage_contacts: Vec<PhysicsDamageContact>,
    ballistic_impacts: Vec<PhysicsBallisticImpact>,
    damage_sources: BTreeMap<u64, PhysicsDamageSource>,
    scene_pose_offsets: BTreeMap<u64, [f32; 3]>,
    scene_rotation_offsets: BTreeMap<u64, [f32; 4]>,
    scene_hinges: scene_hinges::SceneHingeBindings,
    doors: doors::PhysicalDoors,
    script_character_drives: BTreeMap<u64, character_drive::ScriptCharacterDrive>,
    character_stairs: BTreeMap<u64, stairs::CharacterStairs>,
    frame_index: u64,
    fixed_tick: u64,
    fixed_seconds: f64,
    next_command_seq: u64,
    accumulator: f32,
    settings: PhysicsWorldSettings,
    water: Option<water::WaterPhysics>,
    water_excluded: BTreeSet<u64>,
    last_output: PhysicsFrameOutput,
    frame_perf: PhysicsFramePerf,
}

mod character_drive;
mod colliders;
mod commands;
mod components;
mod damage;
pub(crate) mod doors;
mod frame_events;
mod hinges;
mod performance;
mod protection;
mod scene_hinges;
pub(crate) use performance::performance_trace_enabled;
pub(crate) use performance::PhysicsFramePerf;
pub(crate) mod math;
mod presentation;
mod queries;
mod stairs;
mod step;
mod water;
use math::*;

impl PhysicsRuntime {
    #[cfg(test)]
    pub(super) fn empty_for_tests() -> Self {
        Self {
            client: PhysicsClient::new(),
            bodies: BTreeMap::new(),
            joints: BTreeMap::new(),
            supports_joints: false,
            supports_articulation: false,
            supports_collision_filters: false,
            separation_pairs: BTreeSet::new(),
            body_hull_components: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            pending_queries: Vec::new(),
            pending_ballistic_damage: BTreeMap::new(),
            ballistic_damage_contacts: Vec::new(),
            ballistic_impacts: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            scene_hinges: Default::default(),
            doors: Default::default(),
            script_character_drives: Default::default(),
            character_stairs: Default::default(),
            frame_index: 0,
            fixed_tick: 0,
            fixed_seconds: 0.0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            water: None,
            water_excluded: BTreeSet::new(),
            last_output: PhysicsFrameOutput::default(),
            frame_perf: PhysicsFramePerf::default(),
        }
    }

    pub(super) fn connect() -> Result<Self, String> {
        let client = PhysicsClient::new();
        let negotiation = client.negotiate(
            vec![
                PhysicsFeature::StaticColliders,
                PhysicsFeature::DynamicBodies,
                PhysicsFeature::MeshColliders,
            ],
            vec![
                PhysicsFeature::Contacts,
                PhysicsFeature::Queries,
                PhysicsFeature::NativeBackend,
            ],
        )?;

        host::info(
            "newviso.physics",
            format!(
                "physics runtime connected version={}.{}.{} features={:?}",
                negotiation.backend_version.major,
                negotiation.backend_version.minor,
                negotiation.backend_version.patch,
                negotiation.enabled_features
            ),
        );

        let supports_joints = client
            .diagnostics()
            .map(|info| info.capabilities.supports(PhysicsFeature::JointConstraints))
            .unwrap_or(false);
        let supports_articulation = client
            .diagnostics()
            .map(|d| {
                d.capabilities.supports(PhysicsFeature::JointMotors)
                    && d.capabilities.supports(PhysicsFeature::JointAngularLimits)
            })
            .unwrap_or(false);
        let supports_collision_filters = client
            .diagnostics()
            .map(|info| {
                info.capabilities
                    .supports(PhysicsFeature::CollisionPairFiltering)
            })
            .unwrap_or(false);
        Ok(Self {
            supports_collision_filters,
            separation_pairs: BTreeSet::new(),
            body_hull_components: BTreeMap::new(),
            client,
            bodies: BTreeMap::new(),
            joints: BTreeMap::new(),
            supports_joints,
            supports_articulation,
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            pending_queries: Vec::new(),
            pending_ballistic_damage: BTreeMap::new(),
            ballistic_damage_contacts: Vec::new(),
            ballistic_impacts: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            scene_hinges: Default::default(),
            doors: Default::default(),
            script_character_drives: Default::default(),
            character_stairs: Default::default(),
            frame_index: 0,
            fixed_tick: 0,
            fixed_seconds: 0.0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            water: None,
            water_excluded: BTreeSet::new(),
            last_output: PhysicsFrameOutput::default(),
            frame_perf: PhysicsFramePerf::default(),
        })
    }
}

fn command_u64(value: &Value, key: &str, index: usize) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("script command[{index}] requires unsigned integer '{key}'"))
}

fn optional_number(value: &Value, key: &str, default: f32) -> Result<f32, String> {
    match value.get(key) {
        Some(value) => value
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("physics field '{key}' must be finite numeric")),
        None => Ok(default),
    }
}

fn optional_nullable_number(value: &Value, key: &str) -> Result<Option<f32>, String> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .map(Some)
            .ok_or_else(|| format!("physics field '{key}' must be finite numeric or null")),
    }
}

fn optional_bool(value: &Value, key: &str, default: bool) -> Result<bool, String> {
    match value.get(key) {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("physics field '{key}' must be boolean")),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod endorphin_tests;
