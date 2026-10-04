use crate::events::VehicleEventKind;
use crate::types::{HandlingData, TireCondition, Vec3, VehicleClass};
use crate::VehicleSpecification;
use serde::{Deserialize, Serialize};

pub const BODY_HEALTH_MAX: f32 = 1000.0;
pub const ENGINE_HEALTH_MAX: f32 = 1000.0;
pub const ENGINE_DAMAGE_RADBURST: f32 = 400.0;
pub const ENGINE_DAMAGE_OIL_LEAKING: f32 = 200.0;
pub const ENGINE_DAMAGE_ON_FIRE: f32 = 0.0;
pub const ENGINE_DAMAGE_FIRE_FULL: f32 = -700.0;
pub const ENGINE_DAMAGE_FX_FADE: f32 = -3200.0;
pub const ENGINE_DAMAGE_FIRE_FINISH: f32 = -3600.0;
pub const ENGINE_DAMAGE_FINISHED: f32 = -4000.0;
pub const PETROL_TANK_HEALTH_MAX: f32 = 1000.0;
pub const PETROL_TANK_LEAKING: f32 = 700.0;
pub const PETROL_TANK_ON_FIRE: f32 = 0.0;
pub const PETROL_TANK_FINISHED: f32 = -1000.0;
pub const TYRE_HEALTH_MAX: f32 = 1000.0;
pub const TYRE_HEALTH_FLAT: f32 = 350.0;
pub const TYRE_HEALTH_FLAT_ADD: f32 = 0.0001;
pub const SUSPENSION_HEALTH_MAX: f32 = 1000.0;
pub const SUSPENSION_HEALTH_LIMIT_1: f32 = 500.0;
pub const SUSPENSION_HEALTH_SPRING_MULT_1: f32 = 0.7;
pub const SUSPENSION_HEALTH_DAMP_MULT_1: f32 = 0.8;
pub const SUSPENSION_HEALTH_LIMIT_2: f32 = 100.0;
pub const SUSPENSION_HEALTH_SPRING_MULT_2: f32 = 0.4;
pub const SUSPENSION_HEALTH_DAMP_MULT_2: f32 = 0.6;
pub const ENGINE_DAMAGE_OIL_LEAK_RATE: f32 = 0.025;
pub const ENGINE_DAMAGE_OIL_FRACTION_BEFORE_DAMAGE: f32 = 0.25;
pub const ENGINE_DAMAGE_OIL_LOW: f32 = 2.0;
pub const ENGINE_FIRE_HEALTH_DROP_MIN: f32 = 60.0;
pub const ENGINE_FIRE_HEALTH_DROP_MAX: f32 = 100.0;
pub const ENGINE_FIRE_SPREAD_TIMES: [f32; 8] = [
    -250.0, -500.0, -750.0, -1000.0, -1250.0, -1500.0, -2000.0, -3000.0,
];
pub const ENGINE_FIRE_SPREAD_THRESHOLD: f32 = 95.0;
pub const ENGINE_FIRE_SPREAD_MIN_FORWARD_SPEED: f32 = 10.0;
pub const ENGINE_FIRE_SPREAD_THRESHOLD_MOVING_SUBTRACT: f32 = 3.0;
pub const PETROL_TANK_FIRE_BURN_RATE_MIN: f32 = 60.0;
pub const PETROL_TANK_FIRE_BURN_RATE_MAX: f32 = 150.0;
pub const PETROL_TANK_LEAK_RATE: f32 = 2.5;
pub const PETROL_TANK_LEVEL_BEFORE_MISFIRE: f32 = 0.1;

pub const COLLISION_DAMAGE_IMPACT_MULTIPLIER: f32 = 10.0;
pub const GLASS_DAMAGE_THRESHOLD: f32 = 0.5;
pub const TYRE_DEFLATE_PERIOD: f32 = 0.2;
pub const TYRE_DISINTEGRATE_VELOCITY_MPS: f32 = 10.0;
pub const TYRE_BURST_CHECK_INTERVAL: f32 = 1.0 / 30.0;
pub const WHEEL_FRICTION_DAMAGE_BURST_THRESHOLD: f32 = 0.22;
pub const WHEEL_FRICTION_DAMAGE_BREAK_THRESHOLD: f32 = 0.36;
pub const WHEEL_BURST_DAMAGE_CHANCE: f32 = 0.82;
/// Distance budget for a freshly-flat tyre at low/medium speed. Speed and slip
/// accelerate wear; standing still never consumes the rubber.
pub const TYRE_FLAT_RUBBER_WEAR_DISTANCE_M: f32 = 320.0;
pub const TYRE_FLAT_RUBBER_WEAR_MIN_SPEED_MPS: f32 = 0.75;

pub const POP_DOOR_MIN_DAMAGE: f32 = 40.0;
pub const POP_DOOR_MAX_DAMAGE: f32 = 100.0;
pub const POP_DOOR_SIDE_DAMAGE: f32 = 50.0;
pub const POP_DOOR_CHANCE: f32 = 0.5;
pub const LOOSEN_LATCH_DAMAGE: f32 = 10.0;
pub const LOOSEN_LATCH_SIDE_DAMAGE: f32 = 15.0;
pub const LOOSEN_LATCH_CHANCE: f32 = 0.8;
pub const BREAK_DOOR_CHANCE: f32 = 0.2;
pub const BREAK_MISC_CHANCE: f32 = 0.9;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleDamageType {
    Collision,
    Bullet,
    Explosive,
    Fire,
    Melee,
    Water,
    #[default]
    Script,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleDamageComponent {
    Body,
    Engine,
    PetrolTank,
    Glass,
    Light,
    Wheel(usize),
    Door,
    Bonnet,
    Boot,
    Breakable,
    Unknown,
}

impl Default for VehicleDamageComponent {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VehicleDamageRequest {
    pub source_entity: Option<crate::VehicleEntity>,
    pub part_index: Option<u32>,
    /// Laminated windscreens retain cracks longer than tempered side panes.
    pub glass_laminated: bool,
    pub damage_type: VehicleDamageType,
    pub component: VehicleDamageComponent,
    pub raw_damage: f32,
    pub local_position: Vec3,
    pub local_normal: Vec3,
    pub local_direction: Vec3,
    pub contact_impulse: f32,
    pub speed_mps: f32,
    pub upside_down: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct WheelDamageState {
    pub tyre_health: f32,
    pub suspension_health: f32,
    pub friction_damage: f32,
    pub burst_check_timer: f32,
    /// 1.0 = full tyre carcass around the rim; 0.0 = bare rim. Kept separate
    /// from pressure/health so a punctured tyre can remain visibly present.
    #[serde(default = "default_tyre_rubber_remaining")]
    pub tyre_rubber_remaining: f32,
    pub tyre_condition: TireCondition,
}

fn default_tyre_rubber_remaining() -> f32 {
    1.0
}

impl Default for WheelDamageState {
    fn default() -> Self {
        Self {
            tyre_health: TYRE_HEALTH_MAX,
            suspension_health: SUSPENSION_HEALTH_MAX,
            friction_damage: 0.0,
            burst_check_timer: 0.0,
            tyre_rubber_remaining: 1.0,
            tyre_condition: TireCondition::Intact,
        }
    }
}

impl WheelDamageState {
    pub fn suspension_spring_multiplier(&self) -> f32 {
        if self.suspension_health < SUSPENSION_HEALTH_LIMIT_2 {
            SUSPENSION_HEALTH_SPRING_MULT_2
        } else if self.suspension_health < SUSPENSION_HEALTH_LIMIT_1 {
            SUSPENSION_HEALTH_SPRING_MULT_1
        } else {
            1.0
        }
    }

    pub fn suspension_damping_multiplier(&self) -> f32 {
        if self.suspension_health < SUSPENSION_HEALTH_LIMIT_2 {
            SUSPENSION_HEALTH_DAMP_MULT_2
        } else if self.suspension_health < SUSPENSION_HEALTH_LIMIT_1 {
            SUSPENSION_HEALTH_DAMP_MULT_1
        } else {
            1.0
        }
    }
}

/// Persistent, independently repairable damage for an imported glass component.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VehicleGlassDamageState {
    pub damage: f32,
    pub broken: bool,
    pub laminated: bool,
    pub crack_points: Vec<Vec3>,
}

impl VehicleGlassDamageState {
    pub fn apply_hit(
        &mut self,
        damage: f32,
        kind: VehicleDamageType,
        laminated: bool,
        point: Vec3,
    ) -> Option<VehicleEventKind> {
        if self.broken
            || !damage.is_finite()
            || damage <= 0.0
            || matches!(kind, VehicleDamageType::Fire | VehicleDamageType::Water)
        {
            return None;
        }
        self.laminated |= laminated;
        let threshold = if self.laminated { 55.0 } else { 25.0 };
        let strength = match kind {
            VehicleDamageType::Explosive => damage / 12.0,
            VehicleDamageType::Bullet => damage / if self.laminated { 45.0 } else { 8.0 },
            _ => damage / threshold,
        };
        self.damage = (self.damage + strength).clamp(0.0, 1.0);
        if self.crack_points.len() < 8
            && self
                .crack_points
                .iter()
                .all(|old| (0..3).map(|i| (old[i] - point[i]).powi(2)).sum::<f32>() > 0.0025)
        {
            self.crack_points.push(point);
        }
        self.broken = self.damage >= 1.0;
        Some(if self.broken {
            VehicleEventKind::GlassBroken
        } else {
            VehicleEventKind::GlassCracked
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VehicleDamageState {
    pub overall_health: f32,
    pub body_health: f32,
    pub engine_health: f32,
    pub petrol_tank_health: f32,
    pub petrol_tank_level: f32,
    pub petrol_tank_capacity: f32,
    pub oil_level: f32,
    pub oil_capacity: f32,
    pub engine_on_fire: bool,
    pub petrol_tank_on_fire: bool,
    pub oil_leaking: bool,
    pub petrol_leaking: bool,
    pub engine_fire_spread_mask: u8,
    pub engine_misfire_remaining: f32,
    pub engine_misfire_recovery_remaining: f32,
    pub exploded: bool,
    /// Collision may kill the engine at exactly the on-fire boundary without
    /// entering the burning path. This is distinct from negative burning health.
    pub engine_dead: bool,
    /// Player GetIsDriveable(true, ...) semantics.
    pub driveable_player: bool,
    /// AI GetIsDriveable(false, ...) semantics.
    pub driveable_ai: bool,
    /// Backward-compatible operational flag: player-driveable state.
    pub driveable: bool,
    pub wheels: Vec<WheelDamageState>,
    #[serde(default)]
    pub glass: std::collections::BTreeMap<u32, VehicleGlassDamageState>,
}

impl VehicleDamageState {
    pub fn status(&self) -> VehicleDamageStatus {
        VehicleDamageStatus {
            overall_health: self.overall_health,
            body_health: self.body_health,
            engine_health: self.engine_health,
            petrol_tank_health: self.petrol_tank_health,
            petrol_tank_level: self.petrol_tank_level,
            oil_level: self.oil_level,
            oil_leaking: self.oil_leaking,
            petrol_leaking: self.petrol_leaking,
            engine_on_fire: self.engine_on_fire,
            petrol_tank_on_fire: self.petrol_tank_on_fire,
            engine_misfiring: self.engine_misfiring(),
            driveable_player: self.driveable_player,
            driveable_ai: self.driveable_ai,
            exploded: self.exploded,
        }
    }

    pub fn new(wheel_count: usize) -> Self {
        Self::new_with_volumes(wheel_count, 30.0, 5.0)
    }

    pub fn new_with_volumes(
        wheel_count: usize,
        petrol_tank_capacity: f32,
        oil_capacity: f32,
    ) -> Self {
        let petrol_tank_capacity = petrol_tank_capacity.max(0.0);
        let oil_capacity = oil_capacity.max(0.0);
        Self {
            overall_health: BODY_HEALTH_MAX,
            body_health: BODY_HEALTH_MAX,
            engine_health: ENGINE_HEALTH_MAX,
            petrol_tank_health: PETROL_TANK_HEALTH_MAX,
            petrol_tank_level: petrol_tank_capacity,
            petrol_tank_capacity,
            oil_level: oil_capacity,
            oil_capacity,
            engine_on_fire: false,
            petrol_tank_on_fire: false,
            oil_leaking: false,
            petrol_leaking: false,
            engine_fire_spread_mask: 0,
            engine_misfire_remaining: 0.0,
            engine_misfire_recovery_remaining: 0.0,
            exploded: false,
            engine_dead: false,
            driveable_player: true,
            driveable_ai: true,
            driveable: true,
            wheels: vec![WheelDamageState::default(); wheel_count],
            glass: std::collections::BTreeMap::new(),
        }
    }

    /// A zero-volume tank is the reference infinite-fuel mode.
    pub fn has_fuel(&self) -> bool {
        self.petrol_tank_capacity <= 0.0 || self.petrol_tank_level > 0.0
    }

    pub fn repair(&mut self) {
        let count = self.wheels.len();
        *self = Self::new_with_volumes(count, self.petrol_tank_capacity, self.oil_capacity);
    }

    pub fn engine_condition(&self) -> f32 {
        (self.engine_health / ENGINE_HEALTH_MAX).clamp(0.0, 1.0)
    }

    /// CVfxVehicle::CalcDamageAndFireEvos damageEvo for ordinary road vehicles.
    pub fn engine_damage_evolution(&self) -> f32 {
        if self.engine_health >= ENGINE_DAMAGE_RADBURST {
            0.0
        } else if self.engine_health > ENGINE_DAMAGE_ON_FIRE {
            1.0 - (self.engine_health / ENGINE_DAMAGE_RADBURST)
        } else {
            1.0
        }
        .clamp(0.0, 1.0)
    }

    /// CVfxVehicle::CalcDamageAndFireEvos fireEvo, preserving the reference
    /// fade-in/full/fade-out health regions.
    pub fn engine_fire_evolution(&self) -> f32 {
        let health = self.engine_health;
        if health >= ENGINE_DAMAGE_ON_FIRE || self.engine_dead && !self.engine_on_fire {
            return 0.0;
        }
        if health > ENGINE_DAMAGE_FIRE_FULL {
            return (health / ENGINE_DAMAGE_FIRE_FULL).clamp(0.0, 1.0);
        }
        if health >= ENGINE_DAMAGE_FX_FADE {
            return 1.0;
        }
        if health > ENGINE_DAMAGE_FIRE_FINISH {
            return ((health - ENGINE_DAMAGE_FIRE_FINISH)
                / (ENGINE_DAMAGE_FX_FADE - ENGINE_DAMAGE_FIRE_FINISH))
                .clamp(0.0, 1.0);
        }
        0.0
    }

    pub fn petrol_leak_evolution(&self) -> f32 {
        if !self.petrol_leaking {
            return 0.0;
        }
        (((PETROL_TANK_LEAKING - self.petrol_tank_health)
            / (PETROL_TANK_LEAKING - PETROL_TANK_ON_FIRE))
            .clamp(0.0, 1.0)
            + 0.1)
            .clamp(0.0, 1.0)
    }

    pub fn petrol_fire_evolution(&self) -> f32 {
        if !self.petrol_tank_on_fire || self.petrol_tank_level <= 0.0 {
            return 0.0;
        }
        ((PETROL_TANK_ON_FIRE - self.petrol_tank_health)
            / (PETROL_TANK_ON_FIRE - PETROL_TANK_FINISHED))
            .clamp(0.0, 1.0)
    }

    pub fn engine_misfiring(&self) -> bool {
        self.engine_misfire_remaining > 0.0
    }

    /// GTA applies a random negative resultant-force scale while transmission
    /// misfire is active. Use a deterministic pulse driven by the same countdown
    /// so headless/replay execution remains stable while producing the same
    /// stuttering loss of engine output.
    pub fn engine_output_multiplier(&self) -> f32 {
        if !self.engine_misfiring() {
            return 1.0;
        }
        let pulse = (self.engine_misfire_remaining * 41.0).sin() * 0.5 + 0.5;
        (0.18 + pulse * 0.67).clamp(0.18, 0.85)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct VehicleDamageStatus {
    pub overall_health: f32,
    pub body_health: f32,
    pub engine_health: f32,
    pub petrol_tank_health: f32,
    pub petrol_tank_level: f32,
    pub oil_level: f32,
    pub oil_leaking: bool,
    pub petrol_leaking: bool,
    pub engine_on_fire: bool,
    pub petrol_tank_on_fire: bool,
    pub engine_misfiring: bool,
    pub driveable_player: bool,
    pub driveable_ai: bool,
    pub exploded: bool,
}

pub fn damage_transition_signals(
    before: VehicleDamageStatus,
    after: VehicleDamageStatus,
) -> Vec<VehicleDamageSignal> {
    let mut signals = Vec::new();
    for (old, new, start, stop) in [
        (
            before.oil_leaking,
            after.oil_leaking,
            VehicleEventKind::OilLeakStarted,
            VehicleEventKind::OilLeakStopped,
        ),
        (
            before.petrol_leaking,
            after.petrol_leaking,
            VehicleEventKind::PetrolLeakStarted,
            VehicleEventKind::PetrolLeakStopped,
        ),
        (
            before.engine_on_fire,
            after.engine_on_fire,
            VehicleEventKind::EngineFireStarted,
            VehicleEventKind::EngineFireStopped,
        ),
        (
            before.petrol_tank_on_fire,
            after.petrol_tank_on_fire,
            VehicleEventKind::PetrolFireStarted,
            VehicleEventKind::PetrolFireStopped,
        ),
        (
            before.engine_misfiring,
            after.engine_misfiring,
            VehicleEventKind::EngineMisfireStarted,
            VehicleEventKind::EngineMisfireStopped,
        ),
    ] {
        if old != new {
            signals.push(VehicleDamageSignal::new(
                if new { start } else { stop },
                1.0,
            ));
        }
    }
    if before.driveable_player != after.driveable_player
        || before.driveable_ai != after.driveable_ai
    {
        signals.push(VehicleDamageSignal::new(
            VehicleEventKind::DriveabilityChanged,
            1.0,
        ));
        if before.driveable_player && !after.driveable_player
            || before.driveable_ai && !after.driveable_ai
        {
            signals.push(VehicleDamageSignal::new(
                VehicleEventKind::VehicleDisabled,
                1.0,
            ));
        }
        if !before.driveable_player && after.driveable_player
            || !before.driveable_ai && after.driveable_ai
        {
            signals.push(VehicleDamageSignal::new(
                VehicleEventKind::VehicleRestored,
                1.0,
            ));
        }
    }
    signals
}

pub fn reconcile_damage_features(
    state: &mut VehicleDamageState,
    class: VehicleClass,
    specification: &VehicleSpecification,
    player_driver: bool,
) {
    let features = &specification.damage;
    state.oil_leaking = specification.uses_combustion(class)
        && features.oil_leaks.permits(player_driver)
        && state.oil_level > 0.0
        && state.engine_health < ENGINE_DAMAGE_OIL_LEAKING
        && state.engine_health > ENGINE_DAMAGE_ON_FIRE
        && !state.exploded;
    state.petrol_leaking = specification.has_petrol_tank(class)
        && features.petrol_leaks.permits(player_driver)
        && state.petrol_tank_level > 0.0
        && state.petrol_tank_health < PETROL_TANK_LEAKING
        && state.petrol_tank_health > PETROL_TANK_FINISHED
        && !state.exploded;
    state.engine_on_fire = features.engine_fires
        && specification.uses_combustion(class)
        && state.engine_health < ENGINE_DAMAGE_ON_FIRE
        && state.engine_health > ENGINE_DAMAGE_FINISHED
        && !state.exploded
        && !(state.engine_dead && state.engine_health == ENGINE_DAMAGE_ON_FIRE);
    state.petrol_tank_on_fire = features.petrol_tank_damage
        && specification.has_petrol_tank(class)
        && state.petrol_tank_health < PETROL_TANK_ON_FIRE
        && state.petrol_tank_health > PETROL_TANK_FINISHED
        && !state.exploded;
    if !features.engine_fires && state.engine_health < ENGINE_DAMAGE_ON_FIRE {
        state.engine_dead = true;
    }
    if !features.engine_misfires || !specification.uses_combustion(class) || state.exploded {
        state.engine_misfire_remaining = 0.0;
        state.engine_misfire_recovery_remaining = 0.0;
    }
    state.driveable_player = is_driveable(state, class, true);
    state.driveable_ai = is_driveable(state, class, false);
    state.driveable = state.driveable_player;
}

#[derive(Clone, Debug, Default)]
pub struct VehicleDamageOutcome {
    pub effective_damage: f32,
    pub signals: Vec<VehicleDamageSignal>,
}

#[derive(Clone, Debug)]
pub struct VehicleDamageSignal {
    pub kind: VehicleEventKind,
    pub wheel_index: Option<usize>,
    pub magnitude: f32,
}

impl VehicleDamageSignal {
    pub(crate) fn new(kind: VehicleEventKind, magnitude: f32) -> Self {
        Self {
            kind,
            wheel_index: None,
            magnitude,
        }
    }
    fn wheel(kind: VehicleEventKind, wheel_index: usize, magnitude: f32) -> Self {
        Self {
            kind,
            wheel_index: Some(wheel_index),
            magnitude,
        }
    }
}

pub fn apply_damage(
    state: &mut VehicleDamageState,
    handling: &HandlingData,
    class: VehicleClass,
    request: VehicleDamageRequest,
    deterministic_sample: f32,
) -> VehicleDamageOutcome {
    apply_damage_with_specification(
        state,
        handling,
        class,
        &VehicleSpecification::default(),
        false,
        request,
        deterministic_sample,
    )
}

pub fn apply_damage_with_specification(
    state: &mut VehicleDamageState,
    handling: &HandlingData,
    class: VehicleClass,
    specification: &VehicleSpecification,
    player_driver: bool,
    request: VehicleDamageRequest,
    deterministic_sample: f32,
) -> VehicleDamageOutcome {
    let mut result = VehicleDamageOutcome::default();
    let features = &specification.damage;
    if !request.raw_damage.is_finite()
        || request.raw_damage <= 0.0
        || features.indestructible
        || features.bullet_proof && request.damage_type == VehicleDamageType::Bullet
    {
        return result;
    }
    let before = state.status();

    let mut damage = request.raw_damage;
    if request.damage_type == VehicleDamageType::Collision {
        damage *= COLLISION_DAMAGE_IMPACT_MULTIPLIER * handling.collision_damage_multiplier
            / handling.mass.max(1.0);
    } else if matches!(
        request.damage_type,
        VehicleDamageType::Bullet
            | VehicleDamageType::Explosive
            | VehicleDamageType::Fire
            | VehicleDamageType::Melee
    ) {
        damage *= handling.weapon_damage_multiplier;
        if request.damage_type == VehicleDamageType::Fire
            && matches!(class, VehicleClass::Plane | VehicleClass::Helicopter)
        {
            damage *= 0.05;
        }
    }
    result.effective_damage = damage.max(0.0);
    if result.effective_damage <= 0.0 {
        return result;
    }
    if request.damage_type == VehicleDamageType::Collision {
        result.signals.push(VehicleDamageSignal::new(
            VehicleEventKind::CollisionImpact,
            result.effective_damage,
        ));
    }

    let old_body = state.body_health;
    let old_engine = state.engine_health;

    let wheel_hit = match request.component {
        VehicleDamageComponent::Wheel(index) => Some(index),
        _ => None,
    };

    if wheel_hit.is_none() || request.damage_type == VehicleDamageType::Explosive {
        let body_damage =
            damage * handling.deformation_damage_multiplier * features.body_damage_scale;
        state.body_health = (state.body_health - body_damage).max(0.0);
        if state.body_health < old_body {
            result.signals.push(VehicleDamageSignal::new(
                VehicleEventKind::Deformation,
                old_body - state.body_health,
            ));
        }
    }
    // CVehicleDamage::ApplyDamageToOverallHealth runs after wheel processing for
    // every non-bicycle vehicle, including wheel-only hits.
    if class != VehicleClass::Bike {
        state.overall_health =
            (state.overall_health - damage * features.body_damage_scale).max(0.0);
    }

    let engine_region = (wheel_hit.is_none()
        || request.damage_type == VehicleDamageType::Explosive)
        && (request.component == VehicleDamageComponent::Engine
            || matches!(
                request.damage_type,
                VehicleDamageType::Explosive | VehicleDamageType::Fire
            )
            || features
                .engine_region
                .map_or(request.local_position[2] < -0.55, |region| {
                    region.contains(request.local_position)
                })
            || (request.upside_down && damage > 5.0));

    if engine_region && specification.has_engine(class) && features.engine_damage {
        let mut engine_damage = damage * features.engine_damage_scale;
        if old_engine < ENGINE_DAMAGE_ON_FIRE
            && !matches!(
                request.damage_type,
                VehicleDamageType::Bullet | VehicleDamageType::Explosive | VehicleDamageType::Fire
            )
        {
            engine_damage = 0.0;
        }
        if request.damage_type == VehicleDamageType::Collision {
            engine_damage *= handling.engine_damage_multiplier;
        }
        if request.upside_down {
            engine_damage *= 2.0;
        }
        if request.damage_type == VehicleDamageType::Melee {
            if state.engine_health <= 100.0 {
                engine_damage = 0.0;
            } else {
                engine_damage *= 0.3;
                engine_damage = engine_damage.min(state.engine_health - 100.0);
            }
        }
        state.engine_health = (state.engine_health - engine_damage).max(ENGINE_DAMAGE_FINISHED);
        if state.engine_health < old_engine {
            result.signals.push(VehicleDamageSignal::new(
                VehicleEventKind::EngineDamaged,
                old_engine - state.engine_health,
            ));
        }

        if old_engine >= ENGINE_DAMAGE_ON_FIRE && state.engine_health <= ENGINE_DAMAGE_ON_FIRE {
            let threshold = if matches!(
                request.damage_type,
                VehicleDamageType::Bullet | VehicleDamageType::Explosive | VehicleDamageType::Fire
            ) {
                0.5
            } else {
                0.7
            };
            state.engine_health = ENGINE_DAMAGE_ON_FIRE + 0.01;
            if deterministic_sample > threshold
                && features.engine_fires
                && specification.uses_combustion(class)
            {
                let collision_dies = request.damage_type == VehicleDamageType::Collision
                    && (deterministic_sample - threshold) / (1.0 - threshold) < 0.5;
                state.engine_health = if collision_dies {
                    ENGINE_DAMAGE_ON_FIRE
                } else {
                    ENGINE_DAMAGE_ON_FIRE - 1.0
                };
                state.engine_dead = collision_dies;
                state.engine_on_fire = !collision_dies;
            } else if !features.engine_fires || !specification.uses_combustion(class) {
                state.engine_health = ENGINE_DAMAGE_ON_FIRE;
                state.engine_dead = true;
            }
        } else if state.engine_health < ENGINE_DAMAGE_ON_FIRE {
            state.engine_on_fire = true;
        }
        if state.engine_health <= ENGINE_DAMAGE_FIRE_FINISH {
            state.engine_dead = true;
        }
    }

    // Petrol tank damage is spatial in the reference implementation. Our
    // normalized vehicle space treats positive local Z as rearward.
    let petrol_region = request.component == VehicleDamageComponent::PetrolTank
        || matches!(request.damage_type, VehicleDamageType::Explosive)
        || (features
            .petrol_tank_region
            .map_or(request.local_position[2] > 0.45, |region| {
                region.contains(request.local_position)
            })
            && matches!(
                request.damage_type,
                VehicleDamageType::Bullet | VehicleDamageType::Collision | VehicleDamageType::Fire
            ));
    if petrol_region && specification.has_petrol_tank(class) && features.petrol_tank_damage {
        let scaled_tank_damage = damage * features.petrol_tank_damage_scale;
        let tank_damage = if request.damage_type == VehicleDamageType::Collision {
            // Collision damage is capped at the leaking boundary until another
            // damage source pushes the tank into the fire state.
            scaled_tank_damage.min((state.petrol_tank_health - PETROL_TANK_LEAKING - 0.1).max(0.0))
        } else {
            scaled_tank_damage
        };
        state.petrol_tank_health =
            (state.petrol_tank_health - tank_damage).max(PETROL_TANK_FINISHED + 0.1);
        if state.petrol_tank_health < PETROL_TANK_ON_FIRE
            && matches!(
                request.damage_type,
                VehicleDamageType::Bullet | VehicleDamageType::Explosive
            )
        {
            state.petrol_tank_health = PETROL_TANK_FINISHED + 0.1;
        }
        if state.petrol_tank_health < PETROL_TANK_LEAKING {
            state.petrol_leaking = true;
        }
        if state.petrol_tank_health < PETROL_TANK_ON_FIRE
            && matches!(
                request.damage_type,
                VehicleDamageType::Bullet | VehicleDamageType::Explosive | VehicleDamageType::Fire
            )
        {
            state.petrol_tank_on_fire = true;
        }
    }

    if let Some(index) = wheel_hit.filter(|index| *index < state.wheels.len()) {
        let wheel = &mut state.wheels[index];
        let old_condition = wheel.tyre_condition;
        let tyre_damage = damage.max(0.0);

        // The reference applies suspension damage to nearby non-bike wheel
        // suspension independently from tyre rubber damage. A direct wheel
        // component hit is our authoritative "inside suspension sphere" result.
        if class != VehicleClass::Bike
            && !matches!(
                request.damage_type,
                VehicleDamageType::Fire | VehicleDamageType::Water
            )
            && wheel.tyre_condition != TireCondition::Missing
        {
            wheel.suspension_health = (wheel.suspension_health - tyre_damage).max(0.0);
            if wheel.suspension_health <= 0.0 && features.wheels_can_break {
                wheel.tyre_condition = TireCondition::Missing;
                wheel.tyre_health = 0.0;
                wheel.tyre_rubber_remaining = 0.0;
                result.signals.push(VehicleDamageSignal::wheel(
                    VehicleEventKind::WheelDetached,
                    index,
                    tyre_damage,
                ));
            }
        }

        if wheel.tyre_condition != TireCondition::Missing && features.tyres_can_burst {
            if wheel.tyre_health == TYRE_HEALTH_MAX && tyre_damage > 0.0 {
                result.signals.push(VehicleDamageSignal::wheel(
                    VehicleEventKind::TyrePunctured,
                    index,
                    tyre_damage,
                ));
            }
            if wheel.tyre_health > tyre_damage + TYRE_HEALTH_FLAT {
                wheel.tyre_health -= tyre_damage;
            } else if wheel.tyre_health > 0.0 {
                wheel.tyre_health = TYRE_HEALTH_FLAT + TYRE_HEALTH_FLAT_ADD;
            }
            wheel.friction_damage = (wheel.friction_damage + tyre_damage / 1000.0).clamp(0.0, 2.0);
            wheel.tyre_condition = condition_from_health(wheel.tyre_health, old_condition);
        }
    }

    if request.component == VehicleDamageComponent::Glass {
        let glass = state
            .glass
            .entry(request.part_index.unwrap_or(u32::MAX))
            .or_default();
        if let Some(kind) = glass.apply_hit(
            damage,
            request.damage_type,
            request.glass_laminated,
            request.local_position,
        ) {
            result.signals.push(VehicleDamageSignal::new(kind, damage));
        }
    } else if request.component == VehicleDamageComponent::Light && damage >= GLASS_DAMAGE_THRESHOLD
    {
        result.signals.push(VehicleDamageSignal::new(
            VehicleEventKind::LightSmashed,
            damage,
        ));
    }

    if matches!(
        request.component,
        VehicleDamageComponent::Door
            | VehicleDamageComponent::Bonnet
            | VehicleDamageComponent::Boot
    ) && matches!(
        request.damage_type,
        VehicleDamageType::Collision
            | VehicleDamageType::Explosive
            | VehicleDamageType::Melee
            | VehicleDamageType::Bullet
    ) {
        // Body damage does not normally tear a latched door straight off.
        // The first transition in the reference is a loose latch; the fragment
        // presentation owns the later loose -> swinging -> broken progression.
        let pop_rate = ((damage - POP_DOOR_MIN_DAMAGE)
            / (POP_DOOR_MAX_DAMAGE - POP_DOOR_MIN_DAMAGE))
            .clamp(0.0, 1.0);
        let loosen_chance = LOOSEN_LATCH_CHANCE + (1.0 - LOOSEN_LATCH_CHANCE) * pop_rate;
        if damage > LOOSEN_LATCH_DAMAGE && deterministic_sample < loosen_chance {
            result.signals.push(VehicleDamageSignal::new(
                VehicleEventKind::DoorLatchLoosened,
                damage,
            ));
        }
    }

    if request.component == VehicleDamageComponent::Breakable
        && matches!(
            request.damage_type,
            VehicleDamageType::Collision | VehicleDamageType::Explosive
        )
    {
        let break_gate = damage > 120.0
            || (state.body_health < 700.0 && damage > 30.0)
            || (state.body_health < 500.0 && damage > 10.0)
            || (request.upside_down && damage > 5.0);
        if break_gate && deterministic_sample < BREAK_MISC_CHANCE {
            result.signals.push(VehicleDamageSignal::new(
                VehicleEventKind::PartBrokenOff,
                damage,
            ));
        }
    }

    reconcile_damage_features(state, class, specification, player_driver);
    result
        .signals
        .extend(damage_transition_signals(before, state.status()));
    result
}

pub fn process_damage_frame(
    state: &mut VehicleDamageState,
    class: VehicleClass,
    wheel_speeds_mps: &[f32],
    wheel_contacts: &[bool],
    dt: f32,
    deterministic_sample: impl FnMut(usize) -> f32,
) -> Vec<VehicleDamageSignal> {
    process_damage_frame_sampled(
        state,
        class,
        dt,
        |index| wheel_speeds_mps.get(index).copied().unwrap_or_default(),
        |index| wheel_contacts.get(index).copied().unwrap_or(false),
        deterministic_sample,
    )
}

/// Allocation-free frame path used by VehicleRuntime. Callers can expose wheel
/// telemetry directly instead of materializing parallel speed/contact vectors.
pub fn process_damage_frame_sampled(
    state: &mut VehicleDamageState,
    class: VehicleClass,
    dt: f32,
    wheel_speed_mps: impl FnMut(usize) -> f32,
    wheel_contact: impl FnMut(usize) -> bool,
    deterministic_sample: impl FnMut(usize) -> f32,
) -> Vec<VehicleDamageSignal> {
    process_damage_frame_with_specification(
        state,
        class,
        &VehicleSpecification::default(),
        dt,
        wheel_speed_mps,
        wheel_contact,
        deterministic_sample,
    )
}

pub fn process_damage_frame_with_specification(
    state: &mut VehicleDamageState,
    class: VehicleClass,
    specification: &VehicleSpecification,
    dt: f32,
    mut wheel_speed_mps: impl FnMut(usize) -> f32,
    mut wheel_contact: impl FnMut(usize) -> bool,
    _deterministic_sample: impl FnMut(usize) -> f32,
) -> Vec<VehicleDamageSignal> {
    let mut signals = Vec::new();
    let dt = dt.clamp(0.0, 0.1);
    let features = &specification.damage;

    for (index, wheel) in state.wheels.iter_mut().enumerate() {
        if features.wheels_can_break
            && class != VehicleClass::Bike
            && wheel.suspension_health <= 0.0
            && wheel.tyre_condition != TireCondition::Missing
        {
            wheel.tyre_condition = TireCondition::Missing;
            wheel.tyre_health = 0.0;
            wheel.tyre_rubber_remaining = 0.0;
            signals.push(VehicleDamageSignal::wheel(
                VehicleEventKind::WheelDetached,
                index,
                SUSPENSION_HEALTH_MAX,
            ));
            continue;
        }

        if features.tyres_can_burst
            && wheel.tyre_health > 0.0
            && wheel.tyre_health < TYRE_HEALTH_MAX
            && wheel.tyre_health > TYRE_HEALTH_FLAT
        {
            wheel.tyre_health -= dt * (TYRE_HEALTH_MAX / TYRE_DEFLATE_PERIOD);
            if wheel.tyre_health < TYRE_HEALTH_FLAT {
                wheel.tyre_health = TYRE_HEALTH_FLAT + TYRE_HEALTH_FLAT_ADD;
            }
        }

        let speed = wheel_speed_mps(index).abs();
        let touching = wheel_contact(index);
        if features.tyres_can_burst
            && wheel.tyre_condition == TireCondition::Punctured
            && wheel.tyre_health <= TYRE_HEALTH_FLAT + TYRE_HEALTH_FLAT_ADD
            && wheel.tyre_health > 0.0
            && speed > TYRE_FLAT_RUBBER_WEAR_MIN_SPEED_MPS
            && touching
        {
            // Once pressure is gone, consume the tyre carcass by travelled
            // distance rather than by a random per-frame burst. High speed and
            // accumulated sliding friction scrub the rubber off faster.
            let distance = speed * dt;
            let speed_factor = (0.8 + speed / 40.0).clamp(0.8, 2.25);
            let friction_factor = (1.0 + wheel.friction_damage * 1.5).clamp(1.0, 3.5);
            let wear = distance / TYRE_FLAT_RUBBER_WEAR_DISTANCE_M
                * speed_factor
                * friction_factor;
            wheel.tyre_rubber_remaining = (wheel.tyre_rubber_remaining - wear).max(0.0);
            wheel.burst_check_timer = 0.0;

            if wheel.tyre_rubber_remaining <= f32::EPSILON {
                wheel.tyre_rubber_remaining = 0.0;
                wheel.tyre_health = 0.0;
                wheel.tyre_condition = TireCondition::Rim;
                signals.push(VehicleDamageSignal::wheel(
                    VehicleEventKind::TyreBurst,
                    index,
                    speed,
                ));
            }
        } else {
            wheel.burst_check_timer = 0.0;
        }

        // Tyre wear alone must never delete the wheel. Reaching Rim means the
        // vehicle continues on a bare metal wheel; detachment is reserved for
        // explicit suspension/hub structural failure above.
    }

    state.driveable_player = is_driveable(state, class, true);
    state.driveable_ai = is_driveable(state, class, false);
    state.driveable = state.driveable_player;
    signals
}

#[derive(Clone, Copy, Debug)]
pub struct VehicleDamageFrameContext {
    pub engine_running: bool,
    pub rev_ratio: f32,
    pub forward_speed_mps: f32,
    pub player_driver: bool,
    /// CVehicle::GetFuelConsumptionRate. Zero preserves GTA's default
    /// infinite-runtime behavior unless scripts/handling opt into consumption.
    pub fuel_consumption_rate: f32,
    pub upside_down: bool,
    pub allow_fire_damage: bool,
    pub allow_explosion: bool,
    pub allow_oil_damage: bool,
}

impl Default for VehicleDamageFrameContext {
    fn default() -> Self {
        Self {
            engine_running: false,
            rev_ratio: 0.0,
            forward_speed_mps: 0.0,
            player_driver: false,
            fuel_consumption_rate: 0.0,
            upside_down: false,
            allow_fire_damage: true,
            allow_explosion: true,
            allow_oil_damage: true,
        }
    }
}

pub fn process_powertrain_damage_frame(
    state: &mut VehicleDamageState,
    class: VehicleClass,
    context: VehicleDamageFrameContext,
    dt: f32,
    deterministic_sample: impl FnMut(usize) -> f32,
) -> Vec<VehicleDamageSignal> {
    process_powertrain_with_specification(
        state,
        class,
        &VehicleSpecification::default(),
        context,
        dt,
        deterministic_sample,
    )
}

pub fn process_powertrain_with_specification(
    state: &mut VehicleDamageState,
    class: VehicleClass,
    specification: &VehicleSpecification,
    context: VehicleDamageFrameContext,
    dt: f32,
    mut deterministic_sample: impl FnMut(usize) -> f32,
) -> Vec<VehicleDamageSignal> {
    let mut signals = Vec::new();
    let dt = dt.clamp(0.0, 0.1);
    if dt <= 0.0 || state.exploded {
        return signals;
    }
    let before = state.status();
    let features = &specification.damage;
    let combustion = specification.uses_combustion(class);
    let has_tank = specification.has_petrol_tank(class);
    reconcile_damage_features(state, class, specification, context.player_driver);

    // CVehicleDamage::ProcessOilLeak.
    if combustion
        && features.oil_leaks.permits(context.player_driver)
        && state.engine_health < ENGINE_DAMAGE_OIL_LEAKING
        && state.oil_capacity > 0.0
    {
        let ratio = if state.engine_health > ENGINE_DAMAGE_ON_FIRE {
            ((ENGINE_DAMAGE_OIL_LEAKING - state.engine_health)
                / (ENGINE_DAMAGE_OIL_LEAKING - ENGINE_DAMAGE_ON_FIRE))
                .clamp(0.0, 1.0)
        } else {
            1.0
        };
        let oil_leak_rate = (ratio + 0.1) * ENGINE_DAMAGE_OIL_LEAK_RATE;
        state.oil_level = (state.oil_level - oil_leak_rate * dt).max(0.0);

        let oil_fraction = state.oil_level / state.oil_capacity.max(f32::EPSILON);
        if context.engine_running
            && context.allow_oil_damage
            && features.engine_damage
            && oil_fraction < ENGINE_DAMAGE_OIL_FRACTION_BEFORE_DAMAGE
            && state.engine_health > ENGINE_DAMAGE_ON_FIRE
        {
            let low_oil = ((ENGINE_DAMAGE_OIL_FRACTION_BEFORE_DAMAGE - oil_fraction)
                / ENGINE_DAMAGE_OIL_FRACTION_BEFORE_DAMAGE)
                .clamp(0.0, 1.0);
            let damage = ENGINE_DAMAGE_OIL_LOW
                * low_oil
                * context.rev_ratio.abs().clamp(0.0, 1.5)
                * if context.upside_down { 2.0 } else { 1.0 }
                * features.engine_damage_scale
                * dt;
            if damage > 0.0 {
                let old = state.engine_health;
                state.engine_health = (state.engine_health - damage).max(ENGINE_DAMAGE_FINISHED);
                if state.engine_health < old {
                    signals.push(VehicleDamageSignal::new(
                        VehicleEventKind::EngineDamaged,
                        old - state.engine_health,
                    ));
                }
            }
        }
    }

    // CTransmission::ProcessEngineFire.
    if combustion
        && features.engine_fires
        && context.allow_fire_damage
        && state.engine_health < ENGINE_DAMAGE_ON_FIRE
        && state.engine_health > ENGINE_DAMAGE_FINISHED
    {
        state.engine_on_fire = true;
        let sample = deterministic_sample(0x1000).clamp(0.0, 1.0);
        let health_drop = ENGINE_FIRE_HEALTH_DROP_MIN
            + (ENGINE_FIRE_HEALTH_DROP_MAX - ENGINE_FIRE_HEALTH_DROP_MIN) * sample;
        let old_health = state.engine_health;
        state.engine_health = (state.engine_health
            - health_drop * features.engine_damage_scale * dt)
            .max(ENGINE_DAMAGE_FINISHED);

        for (index, threshold) in ENGINE_FIRE_SPREAD_TIMES.iter().enumerate() {
            let bit = 1u8 << index;
            if state.engine_fire_spread_mask & bit != 0 {
                continue;
            }
            if state.engine_health < *threshold && old_health >= *threshold {
                state.engine_fire_spread_mask |= bit;
                if context.forward_speed_mps > ENGINE_FIRE_SPREAD_MIN_FORWARD_SPEED {
                    let spread_threshold = ENGINE_FIRE_SPREAD_THRESHOLD
                        - context.forward_speed_mps * ENGINE_FIRE_SPREAD_THRESHOLD_MOVING_SUBTRACT;
                    if health_drop > spread_threshold || index == ENGINE_FIRE_SPREAD_TIMES.len() - 1
                    {
                        if has_tank
                            && features.petrol_tank_damage
                            && !state.petrol_tank_on_fire
                            && state.petrol_tank_health > PETROL_TANK_FINISHED
                        {
                            state.petrol_tank_health = PETROL_TANK_ON_FIRE - 1.0;
                            state.petrol_tank_on_fire = true;
                        }
                    }
                }
            }
        }
        if state.engine_health <= ENGINE_DAMAGE_FIRE_FINISH {
            state.engine_dead = true;
        }
    }

    // CVehicleDamage::ProcessFuelConsumption. A zero tank volume means
    // HasInfiniteFuel() in the reference and therefore never consumes.
    if has_tank
        && context.engine_running
        && state.petrol_tank_capacity > 0.0
        && context.fuel_consumption_rate > 0.0
        && state.petrol_tank_level > 0.0
    {
        let old_level = state.petrol_tank_level;
        state.petrol_tank_level =
            (state.petrol_tank_level - context.fuel_consumption_rate * dt).max(0.0);
        if old_level > 0.0 && state.petrol_tank_level <= 0.0 {
            signals.push(VehicleDamageSignal::new(
                VehicleEventKind::FuelExhausted,
                1.0,
            ));
        }
    }

    // CVehicleDamage::ProcessPetrolTankDamage.
    if has_tank && state.petrol_leaking && state.petrol_tank_capacity > 0.0 {
        let leak_interp = ((PETROL_TANK_LEAKING - state.petrol_tank_health)
            / (PETROL_TANK_LEAKING - PETROL_TANK_ON_FIRE))
            .clamp(0.0, 1.0);
        let leak_rate = (leak_interp + 0.1) * PETROL_TANK_LEAK_RATE;
        state.petrol_tank_level = (state.petrol_tank_level - leak_rate * dt).max(0.0);
    }

    // Advance an existing stutter independently from its trigger. Refuelling,
    // leaving the aircraft damage band, stopping the motor or disabling the
    // capability must all produce the corresponding end transition.
    let can_misfire = combustion
        && features.engine_misfires
        && context.engine_running
        && (state.petrol_tank_capacity <= 0.0 || state.petrol_tank_level > 0.0);
    let fuel_fraction = if has_tank && state.petrol_tank_capacity > 0.0 {
        (state.petrol_tank_level / state.petrol_tank_capacity).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let fuel_severity = ((PETROL_TANK_LEVEL_BEFORE_MISFIRE - fuel_fraction)
        / PETROL_TANK_LEVEL_BEFORE_MISFIRE)
        .clamp(0.0, 1.0);
    let aircraft_severity = if matches!(class, VehicleClass::Plane | VehicleClass::Helicopter) {
        ((600.0 - state.engine_health) / (600.0 - 200.0)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if !can_misfire {
        state.engine_misfire_remaining = 0.0;
        state.engine_misfire_recovery_remaining = 0.0;
    } else if state.engine_misfire_remaining > 0.0 {
        state.engine_misfire_remaining = (state.engine_misfire_remaining - dt).max(0.0);
    } else if state.engine_misfire_recovery_remaining > 0.0 {
        state.engine_misfire_recovery_remaining =
            (state.engine_misfire_recovery_remaining - dt).max(0.0);
    } else if fuel_severity > 0.0 || aircraft_severity > 0.0 {
        let sample = deterministic_sample(0x2000).clamp(0.0, 1.0);
        let recovery_sample = deterministic_sample(0x2001).clamp(0.0, 1.0);
        if fuel_severity > 0.0 {
            state.engine_misfire_remaining = 0.5 + (1.5 * fuel_severity - 0.5) * sample;
            let recovery = (1.0 - fuel_severity).clamp(0.0, 1.0);
            state.engine_misfire_recovery_remaining = (0.5 + 3.5 * recovery_sample) * recovery;
        } else {
            state.engine_misfire_remaining = 0.5 + 0.5 * aircraft_severity * sample;
            // Preserve the authored reference interpolation, including its
            // reversed recovery endpoints at low aircraft health.
            let recovery_start = 20.0 + 10.0 * aircraft_severity;
            state.engine_misfire_recovery_remaining =
                recovery_start + (20.0 - recovery_start) * recovery_sample;
        }
    }
    if has_tank
        && state.petrol_tank_capacity > 0.0
        && state.petrol_tank_level <= 0.0
        && context.engine_running
        && !signals
            .iter()
            .any(|signal| signal.kind == VehicleEventKind::FuelExhausted)
    {
        signals.push(VehicleDamageSignal::new(
            VehicleEventKind::FuelExhausted,
            1.0,
        ));
    }

    if has_tank
        && state.petrol_tank_on_fire
        && context.allow_fire_damage
        && state.petrol_tank_health > PETROL_TANK_FINISHED
    {
        let burn_rate = if context.player_driver {
            PETROL_TANK_FIRE_BURN_RATE_MIN
        } else {
            let sample = deterministic_sample(0x3000).clamp(0.0, 1.0);
            PETROL_TANK_FIRE_BURN_RATE_MIN
                + (PETROL_TANK_FIRE_BURN_RATE_MAX - PETROL_TANK_FIRE_BURN_RATE_MIN) * sample
        };
        state.petrol_tank_health -= burn_rate * features.petrol_tank_damage_scale * dt;
    }

    state.petrol_tank_health = state.petrol_tank_health.max(PETROL_TANK_FINISHED);
    // Explosion proof can defer the terminal tank transition. Removing the
    // proof must still resolve it after the fire timer has reached its limit.
    if has_tank
        && features.petrol_tank_damage
        && state.petrol_tank_health <= PETROL_TANK_FINISHED
        && context.allow_explosion
    {
        state.exploded = true;
        state.overall_health = 0.0;
        state.body_health = 0.0;
        state.engine_health = ENGINE_DAMAGE_FINISHED;
        state.engine_dead = true;
        state.engine_on_fire = false;
        signals.push(VehicleDamageSignal::new(
            VehicleEventKind::VehicleExploded,
            1.0,
        ));
    }
    reconcile_damage_features(state, class, specification, context.player_driver);
    for edge in damage_transition_signals(before, state.status()) {
        if !signals.iter().any(|s| s.kind == edge.kind) {
            signals.push(edge);
        }
    }
    signals
}

pub fn condition_from_health(health: f32, previous: TireCondition) -> TireCondition {
    if previous == TireCondition::Missing {
        TireCondition::Missing
    } else if health <= 0.0 {
        TireCondition::Rim
    } else if health < TYRE_HEALTH_MAX {
        TireCondition::Punctured
    } else {
        TireCondition::Intact
    }
}

pub fn is_driveable(
    state: &VehicleDamageState,
    class: VehicleClass,
    check_for_player: bool,
) -> bool {
    if state.exploded || state.overall_health <= 0.0 {
        return false;
    }

    if class == VehicleClass::Submarine {
        return state.engine_health >= 0.0;
    }

    let petrol_limit = if check_for_player {
        PETROL_TANK_FINISHED
    } else {
        PETROL_TANK_ON_FIRE
    };
    let engine_limit = if check_for_player {
        ENGINE_DAMAGE_FINISHED
    } else {
        ENGINE_DAMAGE_ON_FIRE
    };
    if state.petrol_tank_health < petrol_limit || state.engine_health <= engine_limit {
        return false;
    }

    // CTransmission may explicitly kill an engine at the zero-health collision
    // boundary. A burning engine below zero is a different state and may remain
    // player-driveable until ENGINE_DAMAGE_FINISHED.
    if state.engine_dead {
        return false;
    }

    if !check_for_player {
        if state
            .wheels
            .iter()
            .any(|wheel| wheel.tyre_condition == TireCondition::Missing)
        {
            return false;
        }
    }
    if state.wheels.iter().any(|wheel| wheel.friction_damage > 1.0) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_health_thresholds_are_preserved() {
        assert_eq!(BODY_HEALTH_MAX, 1000.0);
        assert_eq!(ENGINE_DAMAGE_RADBURST, 400.0);
        assert_eq!(ENGINE_DAMAGE_OIL_LEAKING, 200.0);
        assert_eq!(ENGINE_DAMAGE_ON_FIRE, 0.0);
        assert_eq!(ENGINE_DAMAGE_FINISHED, -4000.0);
        assert_eq!(PETROL_TANK_LEAKING, 700.0);
        assert_eq!(TYRE_HEALTH_FLAT, 350.0);
        assert_eq!(SUSPENSION_HEALTH_MAX, 1000.0);
        assert_eq!(SUSPENSION_HEALTH_LIMIT_1, 500.0);
        assert_eq!(SUSPENSION_HEALTH_LIMIT_2, 100.0);
    }

    #[test]
    fn collision_damage_uses_reference_impact_mass_scaling() {
        let mut state = VehicleDamageState::new(4);
        let handling = HandlingData {
            mass: 1500.0,
            collision_damage_multiplier: 1.0,
            ..HandlingData::default()
        };
        let out = apply_damage(
            &mut state,
            &handling,
            VehicleClass::Automobile,
            VehicleDamageRequest {
                damage_type: VehicleDamageType::Collision,
                raw_damage: 15_000.0,
                local_position: [0.0, 0.0, 0.0],
                ..VehicleDamageRequest::default()
            },
            0.9,
        );
        assert!((out.effective_damage - 100.0).abs() < 1.0e-4);
        assert!(state.body_health < BODY_HEALTH_MAX);
    }

    #[test]
    fn tyre_puncture_deflates_to_reference_flat_health() {
        let mut state = VehicleDamageState::new(1);
        let handling = HandlingData::default();
        let out = apply_damage(
            &mut state,
            &handling,
            VehicleClass::Automobile,
            VehicleDamageRequest {
                damage_type: VehicleDamageType::Bullet,
                component: VehicleDamageComponent::Wheel(0),
                raw_damage: 100.0,
                ..VehicleDamageRequest::default()
            },
            0.5,
        );
        assert!(out
            .signals
            .iter()
            .any(|s| s.kind == VehicleEventKind::TyrePunctured));
        let _ = process_damage_frame(
            &mut state,
            VehicleClass::Automobile,
            &[0.0],
            &[true],
            0.1,
            |_| 1.0,
        );
        let _ = process_damage_frame(
            &mut state,
            VehicleClass::Automobile,
            &[0.0],
            &[true],
            0.1,
            |_| 1.0,
        );
        assert!(
            (state.wheels[0].tyre_health - (TYRE_HEALTH_FLAT + TYRE_HEALTH_FLAT_ADD)).abs() < 0.01
        );
        assert_eq!(state.wheels[0].tyre_condition, TireCondition::Punctured);
    }

    #[test]
    fn flat_tyre_wears_by_distance_then_keeps_the_bare_rim() {
        let mut state = VehicleDamageState::new(1);
        state.wheels[0].tyre_health = TYRE_HEALTH_FLAT + TYRE_HEALTH_FLAT_ADD;
        state.wheels[0].tyre_condition = TireCondition::Punctured;
        state.wheels[0].tyre_rubber_remaining = 1.0;

        for _ in 0..20 {
            let _ = process_damage_frame(
                &mut state,
                VehicleClass::Automobile,
                &[0.0],
                &[true],
                0.1,
                |_| 0.0,
            );
        }
        assert_eq!(state.wheels[0].tyre_rubber_remaining, 1.0);

        let mut burst_seen = false;
        for _ in 0..400 {
            let events = process_damage_frame(
                &mut state,
                VehicleClass::Automobile,
                &[15.0],
                &[true],
                0.1,
                |_| 0.0,
            );
            burst_seen |= events
                .iter()
                .any(|event| event.kind == VehicleEventKind::TyreBurst);
            if state.wheels[0].tyre_condition == TireCondition::Rim {
                break;
            }
        }
        assert!(burst_seen);
        assert_eq!(state.wheels[0].tyre_condition, TireCondition::Rim);
        assert_eq!(state.wheels[0].tyre_rubber_remaining, 0.0);

        for _ in 0..120 {
            let _ = process_damage_frame(
                &mut state,
                VehicleClass::Automobile,
                &[25.0],
                &[true],
                0.1,
                |_| 0.0,
            );
        }
        assert_eq!(state.wheels[0].tyre_condition, TireCondition::Rim);
    }

    #[test]
    fn suspension_health_uses_reference_spring_and_damping_steps() {
        let mut wheel = WheelDamageState::default();
        assert_eq!(wheel.suspension_spring_multiplier(), 1.0);
        assert_eq!(wheel.suspension_damping_multiplier(), 1.0);
        wheel.suspension_health = 499.0;
        assert_eq!(wheel.suspension_spring_multiplier(), 0.7);
        assert_eq!(wheel.suspension_damping_multiplier(), 0.8);
        wheel.suspension_health = 99.0;
        assert_eq!(wheel.suspension_spring_multiplier(), 0.4);
        assert_eq!(wheel.suspension_damping_multiplier(), 0.6);
    }

    #[test]
    fn zero_suspension_health_detaches_wheel() {
        let mut state = VehicleDamageState::new(1);
        state.wheels[0].suspension_health = 20.0;
        let out = apply_damage(
            &mut state,
            &HandlingData::default(),
            VehicleClass::Automobile,
            VehicleDamageRequest {
                damage_type: VehicleDamageType::Collision,
                component: VehicleDamageComponent::Wheel(0),
                raw_damage: 4_000.0,
                ..VehicleDamageRequest::default()
            },
            0.99,
        );
        assert_eq!(state.wheels[0].tyre_condition, TireCondition::Missing);
        assert!(out.signals.iter().any(|signal| {
            signal.kind == VehicleEventKind::WheelDetached && signal.wheel_index == Some(0)
        }));
    }

    #[test]
    fn ordinary_door_damage_loosens_latch_before_breaking_off() {
        let mut state = VehicleDamageState::new(4);
        let out = apply_damage(
            &mut state,
            &HandlingData::default(),
            VehicleClass::Automobile,
            VehicleDamageRequest {
                damage_type: VehicleDamageType::Collision,
                component: VehicleDamageComponent::Door,
                raw_damage: 12_000.0,
                ..VehicleDamageRequest::default()
            },
            0.1,
        );
        assert!(out
            .signals
            .iter()
            .any(|signal| { signal.kind == VehicleEventKind::DoorLatchLoosened }));
        assert!(!out.signals.iter().any(|signal| matches!(
            signal.kind,
            VehicleEventKind::DoorBrokenOff
                | VehicleEventKind::BonnetBrokenOff
                | VehicleEventKind::BootBrokenOff
        )));
    }

    #[test]
    fn burning_engine_preserves_player_driveability_but_not_ai_driveability() {
        let mut state = VehicleDamageState::new(4);
        state.engine_health = -1.0;
        state.engine_on_fire = true;
        state.engine_dead = false;
        assert!(is_driveable(&state, VehicleClass::Automobile, true));
        assert!(!is_driveable(&state, VehicleClass::Automobile, false));
        state.engine_health = ENGINE_DAMAGE_FINISHED;
        assert!(!is_driveable(&state, VehicleClass::Automobile, true));
    }

    #[test]
    fn engine_damage_and_fire_evolutions_follow_reference_health_ranges() {
        let mut state = VehicleDamageState::new(4);
        assert_eq!(state.engine_damage_evolution(), 0.0);
        state.engine_health = 300.0;
        assert!((state.engine_damage_evolution() - 0.25).abs() < 1.0e-5);
        state.engine_health = 100.0;
        assert!((state.engine_damage_evolution() - 0.75).abs() < 1.0e-5);

        state.engine_health = -350.0;
        state.engine_on_fire = true;
        assert!((state.engine_fire_evolution() - 0.5).abs() < 1.0e-5);
        state.engine_health = ENGINE_DAMAGE_FIRE_FULL;
        assert!((state.engine_fire_evolution() - 1.0).abs() < 1.0e-5);
        state.engine_health = -3400.0;
        assert!((state.engine_fire_evolution() - 0.5).abs() < 1.0e-5);
        state.engine_health = ENGINE_DAMAGE_FIRE_FINISH;
        assert_eq!(state.engine_fire_evolution(), 0.0);
    }

    #[test]
    fn active_misfire_reduces_engine_output_with_deterministic_stutter() {
        let mut state = VehicleDamageState::new(4);
        assert_eq!(state.engine_output_multiplier(), 1.0);
        state.engine_misfire_remaining = 0.75;
        let first = state.engine_output_multiplier();
        state.engine_misfire_remaining = 0.70;
        let second = state.engine_output_multiplier();
        assert!((0.18..=0.85).contains(&first));
        assert!((0.18..=0.85).contains(&second));
        assert!((first - second).abs() > 1.0e-4);
    }

    #[test]
    fn source_oil_leak_and_engine_fire_progress_over_time() {
        let mut state = VehicleDamageState::new_with_volumes(4, 65.0, 5.0);
        state.engine_health = 100.0;
        let before_oil = state.oil_level;
        let _ = process_powertrain_damage_frame(
            &mut state,
            VehicleClass::Automobile,
            VehicleDamageFrameContext {
                engine_running: true,
                rev_ratio: 1.0,
                forward_speed_mps: 0.0,
                player_driver: true,
                fuel_consumption_rate: 0.0,
                ..Default::default()
            },
            0.1,
            |_| 0.0,
        );
        assert!(state.oil_level < before_oil);

        state.engine_health = -1.0;
        state.engine_dead = false;
        let old = state.engine_health;
        let _ = process_powertrain_damage_frame(
            &mut state,
            VehicleClass::Automobile,
            VehicleDamageFrameContext {
                engine_running: true,
                rev_ratio: 1.0,
                forward_speed_mps: 0.0,
                player_driver: true,
                fuel_consumption_rate: 0.0,
                ..Default::default()
            },
            0.1,
            |_| 0.0,
        );
        assert!(state.engine_health < old);
    }

    #[test]
    fn source_fuel_consumption_exhausts_tank_and_emits_once() {
        let mut state = VehicleDamageState::new_with_volumes(4, 0.05, 5.0);
        let events = process_powertrain_damage_frame(
            &mut state,
            VehicleClass::Automobile,
            VehicleDamageFrameContext {
                engine_running: true,
                fuel_consumption_rate: 1.0,
                ..VehicleDamageFrameContext::default()
            },
            0.1,
            |_| 0.0,
        );
        assert_eq!(state.petrol_tank_level, 0.0);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == VehicleEventKind::FuelExhausted)
                .count(),
            1
        );

        let next = process_powertrain_damage_frame(
            &mut state,
            VehicleClass::Automobile,
            VehicleDamageFrameContext {
                engine_running: false,
                fuel_consumption_rate: 1.0,
                ..VehicleDamageFrameContext::default()
            },
            0.1,
            |_| 0.0,
        );
        assert!(!next
            .iter()
            .any(|event| event.kind == VehicleEventKind::FuelExhausted));

        let mut infinite = VehicleDamageState::new_with_volumes(4, 0.0, 5.0);
        let _ = process_powertrain_damage_frame(
            &mut infinite,
            VehicleClass::Automobile,
            VehicleDamageFrameContext {
                engine_running: true,
                fuel_consumption_rate: 5.0,
                ..VehicleDamageFrameContext::default()
            },
            1.0,
            |_| 0.0,
        );
        assert_eq!(infinite.petrol_tank_capacity, 0.0);
        assert_eq!(infinite.petrol_tank_level, 0.0);
    }

    #[test]
    fn burning_petrol_tank_reaches_explosion_threshold() {
        let mut state = VehicleDamageState::new_with_volumes(4, 65.0, 5.0);
        state.petrol_tank_health = PETROL_TANK_FINISHED + 1.0;
        state.petrol_tank_on_fire = true;
        let events = process_powertrain_damage_frame(
            &mut state,
            VehicleClass::Automobile,
            VehicleDamageFrameContext {
                player_driver: true,
                ..VehicleDamageFrameContext::default()
            },
            0.1,
            |_| 0.0,
        );
        assert!(state.exploded);
        assert!(events
            .iter()
            .any(|event| event.kind == VehicleEventKind::VehicleExploded));
    }
}

#[cfg(test)]
mod glass_damage_tests {
    use super::*;
    #[test]
    fn repeated_low_energy_hits_crack_then_break_once() {
        let mut glass = VehicleGlassDamageState::default();
        assert_eq!(
            glass.apply_hit(5.0, VehicleDamageType::Collision, false, [0.0; 3]),
            Some(VehicleEventKind::GlassCracked)
        );
        assert!(!glass.broken);
        assert_eq!(
            glass.apply_hit(20.0, VehicleDamageType::Collision, false, [0.1, 0.0, 0.0]),
            Some(VehicleEventKind::GlassBroken)
        );
        assert_eq!(
            glass.apply_hit(100.0, VehicleDamageType::Bullet, false, [0.0; 3]),
            None
        );
        assert_eq!(glass.crack_points.len(), 2);
    }
    #[test]
    fn laminated_windscreen_retains_a_bullet_crack_while_side_pane_breaks() {
        let mut windscreen = VehicleGlassDamageState::default();
        let mut side = VehicleGlassDamageState::default();
        assert_eq!(
            windscreen.apply_hit(10.0, VehicleDamageType::Bullet, true, [0.0; 3]),
            Some(VehicleEventKind::GlassCracked)
        );
        assert_eq!(
            side.apply_hit(10.0, VehicleDamageType::Bullet, false, [0.0; 3]),
            Some(VehicleEventKind::GlassBroken)
        );
    }
    #[test]
    fn panes_accumulate_independent_damage_and_repair_clears_it() {
        let mut state = VehicleDamageState::new(4);
        state.glass.entry(3).or_default().apply_hit(
            30.0,
            VehicleDamageType::Collision,
            false,
            [0.0; 3],
        );
        state.glass.entry(4).or_default().apply_hit(
            5.0,
            VehicleDamageType::Collision,
            false,
            [0.0; 3],
        );
        assert!(state.glass[&3].broken);
        assert!(!state.glass[&4].broken);
        state.repair();
        assert!(state.glass.is_empty());
    }
    #[test]
    fn fire_and_water_do_not_shatter_glass() {
        let mut glass = VehicleGlassDamageState::default();
        assert_eq!(
            glass.apply_hit(1000.0, VehicleDamageType::Fire, false, [0.0; 3]),
            None
        );
        assert_eq!(glass.damage, 0.0);
    }
}
