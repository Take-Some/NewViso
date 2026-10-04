use crate::types::{Vec3, VehicleEntity, VehicleSurfaceClass};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleEventKind {
    VehicleCreated,
    VehicleRemoved,
    OccupantEntered,
    OccupantLeft,
    SeatChanged,
    AlarmStarted,
    AlarmStopped,
    LockChanged,
    LightsChanged,
    CoolingFanStarted,
    CoolingFanStopped,
    EngineStarted,
    EngineStartFailed,
    EngineStopped,
    GearShifted,
    BrakeReleased,
    HandbrakeApplied,
    HandbrakeReleased,
    SkidStarted,
    SkidStopped,
    WheelSpinStarted,
    WheelSpinStopped,
    TyrePunctured,
    TyreBurst,
    WheelDetached,
    SuspensionImpact,
    JumpLanded,
    CollisionImpact,
    CollisionScrape,
    Deformation,
    GlassCracked,
    GlassBroken,
    LightSmashed,
    DoorOpened,
    DoorClosed,
    DoorLockedAttempt,
    DoorLatchLoosened,
    DoorBrokenOff,
    BonnetBrokenOff,
    BootBrokenOff,
    PartLoose,
    PartBrokenOff,
    HornStarted,
    HornStopped,
    SirenStarted,
    SirenStopped,
    EngineDamaged,
    DamageApplied,
    SpecificationChanged,
    DriveabilityChanged,
    VehicleRestored,
    OilLeakStarted,
    OilLeakStopped,
    EngineFireStarted,
    EngineFireStopped,
    EngineMisfireStarted,
    EngineMisfireStopped,
    PetrolLeakStarted,
    PetrolLeakStopped,
    PetrolFireStarted,
    PetrolFireStopped,
    FuelExhausted,
    VehicleExploded,
    VehicleDisabled,
    VehicleRepaired,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VehicleEvent {
    pub sequence: u64,
    pub entity: VehicleEntity,
    pub kind: VehicleEventKind,
    #[serde(default)]
    pub actor_entity: Option<VehicleEntity>,
    #[serde(default)]
    pub seat: Option<String>,
    #[serde(default)]
    pub previous_seat: Option<String>,
    #[serde(default)]
    pub local_space: bool,
    #[serde(default)]
    pub damage_type: Option<crate::damage::VehicleDamageType>,
    #[serde(default)]
    pub wheel_index: Option<usize>,
    #[serde(default)]
    pub part: Option<String>,
    #[serde(default)]
    pub part_index: Option<u32>,
    #[serde(default)]
    pub position: Option<Vec3>,
    #[serde(default)]
    pub normal: Option<Vec3>,
    #[serde(default)]
    pub other_entity: Option<VehicleEntity>,
    #[serde(default)]
    pub surface_id: Option<u32>,
    #[serde(default)]
    pub surface_class: Option<VehicleSurfaceClass>,
    #[serde(default)]
    pub details: serde_json::Value,
    #[serde(default)]
    pub magnitude: f32,
    #[serde(default)]
    pub speed_mps: f32,
    #[serde(default)]
    pub old_gear: Option<i8>,
    #[serde(default)]
    pub new_gear: Option<i8>,
    #[serde(default)]
    pub hold_seconds: f32,
}

impl VehicleEvent {
    pub fn new(sequence: u64, entity: VehicleEntity, kind: VehicleEventKind) -> Self {
        Self {
            sequence,
            entity,
            kind,
            actor_entity: None,
            seat: None,
            previous_seat: None,
            local_space: false,
            damage_type: None,
            wheel_index: None,
            part: None,
            part_index: None,
            position: None,
            normal: None,
            other_entity: None,
            surface_id: None,
            surface_class: None,
            details: serde_json::Value::Null,
            magnitude: 0.0,
            speed_mps: 0.0,
            old_gear: None,
            new_gear: None,
            hold_seconds: 0.0,
        }
    }
}

impl VehicleEventKind {
    pub fn topic(self) -> &'static str {
        match self {
            Self::VehicleCreated => "engine.vehicle.vehicle_created",
            Self::VehicleRemoved => "engine.vehicle.vehicle_removed",
            Self::OccupantEntered => "engine.vehicle.occupant_entered",
            Self::OccupantLeft => "engine.vehicle.occupant_left",
            Self::SeatChanged => "engine.vehicle.seat_changed",
            Self::AlarmStarted => "engine.vehicle.alarm_started",
            Self::AlarmStopped => "engine.vehicle.alarm_stopped",
            Self::LockChanged => "engine.vehicle.lock_changed",
            Self::LightsChanged => "engine.vehicle.lights_changed",
            Self::CoolingFanStarted => "engine.vehicle.cooling_fan_started",
            Self::CoolingFanStopped => "engine.vehicle.cooling_fan_stopped",
            Self::EngineStarted => "engine.vehicle.engine_started",
            Self::EngineStartFailed => "engine.vehicle.engine_start_failed",
            Self::EngineStopped => "engine.vehicle.engine_stopped",
            Self::GearShifted => "engine.vehicle.gear_shifted",
            Self::BrakeReleased => "engine.vehicle.brake_released",
            Self::HandbrakeApplied => "engine.vehicle.handbrake_applied",
            Self::HandbrakeReleased => "engine.vehicle.handbrake_released",
            Self::SkidStarted => "engine.vehicle.skid_started",
            Self::SkidStopped => "engine.vehicle.skid_stopped",
            Self::WheelSpinStarted => "engine.vehicle.wheel_spin_started",
            Self::WheelSpinStopped => "engine.vehicle.wheel_spin_stopped",
            Self::TyrePunctured => "engine.vehicle.tyre_punctured",
            Self::TyreBurst => "engine.vehicle.tyre_burst",
            Self::WheelDetached => "engine.vehicle.wheel_detached",
            Self::SuspensionImpact => "engine.vehicle.suspension_impact",
            Self::JumpLanded => "engine.vehicle.jump_landed",
            Self::CollisionImpact => "engine.vehicle.collision_impact",
            Self::CollisionScrape => "engine.vehicle.collision_scrape",
            Self::Deformation => "engine.vehicle.deformation",
            Self::GlassCracked => "engine.vehicle.glass_cracked",
            Self::GlassBroken => "engine.vehicle.glass_broken",
            Self::LightSmashed => "engine.vehicle.light_smashed",
            Self::DoorOpened => "engine.vehicle.door_opened",
            Self::DoorClosed => "engine.vehicle.door_closed",
            Self::DoorLockedAttempt => "engine.vehicle.door_locked_attempt",
            Self::DoorLatchLoosened => "engine.vehicle.door_latch_loosened",
            Self::DoorBrokenOff => "engine.vehicle.door_broken_off",
            Self::BonnetBrokenOff => "engine.vehicle.bonnet_broken_off",
            Self::BootBrokenOff => "engine.vehicle.boot_broken_off",
            Self::PartLoose => "engine.vehicle.part_loose",
            Self::PartBrokenOff => "engine.vehicle.part_broken_off",
            Self::HornStarted => "engine.vehicle.horn_started",
            Self::HornStopped => "engine.vehicle.horn_stopped",
            Self::SirenStarted => "engine.vehicle.siren_started",
            Self::SirenStopped => "engine.vehicle.siren_stopped",
            Self::EngineDamaged => "engine.vehicle.engine_damaged",
            Self::DamageApplied => "engine.vehicle.damage_applied",
            Self::SpecificationChanged => "engine.vehicle.specification_changed",
            Self::DriveabilityChanged => "engine.vehicle.driveability_changed",
            Self::VehicleRestored => "engine.vehicle.vehicle_restored",
            Self::OilLeakStarted => "engine.vehicle.oil_leak_started",
            Self::OilLeakStopped => "engine.vehicle.oil_leak_stopped",
            Self::EngineFireStarted => "engine.vehicle.engine_fire_started",
            Self::EngineFireStopped => "engine.vehicle.engine_fire_stopped",
            Self::EngineMisfireStarted => "engine.vehicle.engine_misfire_started",
            Self::EngineMisfireStopped => "engine.vehicle.engine_misfire_stopped",
            Self::PetrolLeakStarted => "engine.vehicle.petrol_leak_started",
            Self::PetrolLeakStopped => "engine.vehicle.petrol_leak_stopped",
            Self::PetrolFireStarted => "engine.vehicle.petrol_fire_started",
            Self::PetrolFireStopped => "engine.vehicle.petrol_fire_stopped",
            Self::FuelExhausted => "engine.vehicle.fuel_exhausted",
            Self::VehicleExploded => "engine.vehicle.vehicle_exploded",
            Self::VehicleDisabled => "engine.vehicle.vehicle_disabled",
            Self::VehicleRepaired => "engine.vehicle.vehicle_repaired",
        }
    }
}
